use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::{broadcast, mpsc, oneshot, RwLock, Mutex};

use crate::agent::events::{AgentEvent, ToolOutputStream};
use crate::agent::loop_::{run_sub_agent_loop, ApprovalResponse};
use crate::config::types::AppConfig;
use crate::session::types::Session;
use super::{Tool, ToolError};

/// A single entry in the sub-agent's activity log, serialized into the tool result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SubAgentLogEntry {
    Text { text: String },
    ToolCall { name: String, summary: String },
    ToolResult { name: String, preview: String, is_error: bool },
}

/// The full result stored as the sub-agent tool's output content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgentResult {
    pub summary: String,
    pub log: Vec<SubAgentLogEntry>,
}

/// Shared context that the SubAgentTool needs from the parent agent loop.
pub struct SubAgentContext {
    pub config: AppConfig,
    pub parent_event_tx: mpsc::UnboundedSender<AgentEvent>,
    pub cancel_broadcast: broadcast::Sender<()>,
    pub sub_agent_approval_senders: Arc<Mutex<HashMap<String, mpsc::UnboundedSender<ApprovalResponse>>>>,
    pub session_approved: Arc<RwLock<HashSet<String>>>,
    pub working_directory: String,
}

pub struct SubAgentTool {
    context: Arc<SubAgentContext>,
}

impl SubAgentTool {
    pub fn new(context: SubAgentContext) -> Self {
        Self { context: Arc::new(context) }
    }
}

#[async_trait]
impl Tool for SubAgentTool {
    fn name(&self) -> &str { "sub_agent" }

    fn description(&self) -> &str {
        "Spawn a sub-agent to handle a well-defined subtask independently. The sub-agent has access to all tools except sub_agent. Use for: exploring parts of the codebase, performing small focused refactors, researching a specific question. Returns a summary of findings/actions. You can call multiple sub_agents in parallel."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "task": {
                    "type": "string",
                    "description": "A clear, self-contained description of the subtask for the sub-agent to perform. Include all necessary context — the sub-agent has no memory of the parent conversation."
                }
            },
            "required": ["task"]
        })
    }

    fn requires_approval(&self) -> bool { false }

    async fn execute(&self, args: serde_json::Value) -> Result<String, ToolError> {
        // Not used — execute_streaming is the real entry point.
        let task = args.get("task").and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("Missing required 'task' argument".into()))?;
        Ok(format!("sub_agent would execute: {}", task))
    }

    async fn execute_streaming(
        &self,
        args: serde_json::Value,
    ) -> Result<(
        Option<mpsc::UnboundedReceiver<(ToolOutputStream, String)>>,
        oneshot::Receiver<Result<String, ToolError>>,
    ), ToolError> {
        let task = args.get("task").and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("Missing required 'task' argument".into()))?
            .to_string();

        // The agent loop injects _tool_call_id so we know our parent ID
        let tool_call_id = args.get("_tool_call_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        // Create child channels
        let (child_event_tx, mut child_event_rx) = mpsc::unbounded_channel::<AgentEvent>();
        let (child_approval_tx, child_approval_rx) = mpsc::unbounded_channel::<ApprovalResponse>();

        // Subscribe to parent's cancel broadcast
        let mut cancel_sub = self.context.cancel_broadcast.subscribe();
        let (child_cancel_tx, child_cancel_rx) = mpsc::unbounded_channel::<()>();
        tokio::spawn(async move {
            let _ = cancel_sub.recv().await;
            let _ = child_cancel_tx.send(());
        });

        // Register the child approval sender so the UI can route responses
        {
            let mut senders = self.context.sub_agent_approval_senders.lock().await;
            senders.insert(tool_call_id.clone(), child_approval_tx);
        }

        // Create temporary child session
        let mut child_session = Session::new(self.context.working_directory.clone());
        child_session.messages.push(
            crate::session::types::ConversationMessage::user(&task),
        );

        // Streaming output + result channels
        let (stream_tx, stream_rx) = mpsc::unbounded_channel::<(ToolOutputStream, String)>();
        let (result_tx, result_rx) = oneshot::channel::<Result<String, ToolError>>();

        let config = self.context.config.clone();
        let session_approved = self.context.session_approved.clone();

        // Spawn the sub-agent loop
        tokio::spawn(async move {
            run_sub_agent_loop(
                config,
                &mut child_session,
                child_event_tx,
                child_approval_rx,
                child_cancel_rx,
                session_approved,
            ).await;
        });

        // Spawn relay task: forward child events to parent, collect summary
        let parent_event_tx = self.context.parent_event_tx.clone();
        let approval_senders = self.context.sub_agent_approval_senders.clone();
        let relay_tool_call_id = tool_call_id.clone();

        tokio::spawn(async move {
            let mut summary = String::new();
            let mut pending_text = String::new();
            let mut log: Vec<SubAgentLogEntry> = Vec::new();

            while let Some(event) = child_event_rx.recv().await {
                let is_done = matches!(event, AgentEvent::Done);
                let is_error = matches!(event, AgentEvent::Error(_));

                match &event {
                    AgentEvent::StreamDelta(text) => {
                        summary.push_str(text);
                        pending_text.push_str(text);
                        let _ = stream_tx.send((ToolOutputStream::Stdout, text.clone()));
                    }
                    AgentEvent::ToolCallRequested { name, arguments, .. } => {
                        // Flush pending text to log
                        let trimmed = pending_text.trim().to_string();
                        if !trimmed.is_empty() {
                            log.push(SubAgentLogEntry::Text { text: trimmed });
                        }
                        pending_text.clear();

                        let tool_summary = crate::tools::tool_name::ToolCall::from_raw(name, arguments)
                            .map(|tc| crate::tools::tool_name::ToolPresentation::summary(&tc))
                            .unwrap_or_default();
                        log.push(SubAgentLogEntry::ToolCall {
                            name: name.clone(),
                            summary: tool_summary,
                        });
                    }
                    AgentEvent::ToolResult { name, content, is_error, .. } => {
                        let preview = content.lines().take(2).collect::<Vec<_>>().join(" | ");
                        let preview = if preview.len() > 100 {
                            format!("{}...", &preview[..97])
                        } else {
                            preview
                        };
                        log.push(SubAgentLogEntry::ToolResult {
                            name: name.clone(),
                            preview,
                            is_error: *is_error,
                        });
                    }
                    _ => {}
                }

                // Relay all events wrapped in SubAgentEvent
                let _ = parent_event_tx.send(AgentEvent::SubAgentEvent {
                    parent_tool_call_id: relay_tool_call_id.clone(),
                    event: Box::new(event.clone()),
                });

                if is_done || is_error {
                    let trimmed = pending_text.trim().to_string();
                    if !trimmed.is_empty() {
                        log.push(SubAgentLogEntry::Text { text: trimmed });
                    }
                    break;
                }
            }

            // Clean up approval sender
            {
                let mut senders = approval_senders.lock().await;
                senders.remove(&relay_tool_call_id);
            }

            // Serialize the full result with log for persistence
            let result = SubAgentResult { summary, log };
            let content = serde_json::to_string(&result).unwrap_or_default();
            let _ = result_tx.send(Ok(content));
        });

        Ok((Some(stream_rx), result_rx))
    }
}
