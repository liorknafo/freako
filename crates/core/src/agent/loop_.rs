use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use futures::StreamExt;
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};

use crate::agent::context::{build_request, llm_compact_messages};
use crate::agent::events::{
    AgentEvent, PlanTask, TaskStatus,
    ADD_TASK_TOOL_NAME, EDIT_TASK_TOOL_NAME, ENTER_PLAN_MODE_TOOL_NAME,
    READ_PLAN_TOOL_NAME, READ_TASK_TOOL_NAME, REVIEW_PLAN_TOOL_NAME,
    UPDATE_TASK_STATUS_TOOL_NAME,
};
use crate::agent::prompt::{build_system_prompt, build_sub_agent_system_prompt};
use crate::config::types::AppConfig;
use crate::provider::{self, types::TokenUsage, RetryConfig};
use crate::session::types::{ConversationMessage, MessagePart, Session};
use crate::tools::ToolRegistry;
use crate::tools::sub_agent::{SubAgentTool, SubAgentContext};

#[derive(Debug, Clone)]
pub enum ApprovalResponse {
    Approve,
    ApproveForSession,
    ApproveAlways,
    Deny,
}

/// Run the agent loop. Streams events back via `event_tx`.
/// Receives approval decisions via `approval_rx`.
/// Can be cancelled via `cancel_rx`.
/// Queued user messages can be injected between tool calls via `queued_message_rx`.
/// Sub-agent approval channels are shared so the UI can route approval responses.
pub async fn run_agent_loop(
    mut config: AppConfig,
    session: &mut Session,
    event_tx: mpsc::UnboundedSender<AgentEvent>,
    mut approval_rx: mpsc::UnboundedReceiver<ApprovalResponse>,
    mut cancel_rx: mpsc::UnboundedReceiver<()>,
    mut queued_message_rx: mpsc::UnboundedReceiver<String>,
    sub_agent_approval_senders: Arc<Mutex<HashMap<String, mpsc::UnboundedSender<ApprovalResponse>>>>,
) {
    let provider = match provider::build_provider(&config.provider) {
        Ok(p) => p,
        Err(e) => {
            let _ = event_tx.send(AgentEvent::Error(format!("Provider error: {}", e)));
            return;
        }
    };
    let retry_config = RetryConfig::default();

    // Shared approval state — sub-agents inherit and contribute to this set
    let session_approved: Arc<RwLock<HashSet<String>>> =
        Arc::new(RwLock::new(HashSet::new()));

    // Cancel broadcast — sub-agents subscribe to this for cancellation propagation
    let (cancel_broadcast, _) = broadcast::channel::<()>(16);

    let mut registry = ToolRegistry::default_registry(&config);
    // Register sub_agent tool with runtime context
    registry.register(Box::new(SubAgentTool::new(SubAgentContext {
        config: config.clone(),
        parent_event_tx: event_tx.clone(),
        cancel_broadcast: cancel_broadcast.clone(),
        sub_agent_approval_senders: sub_agent_approval_senders.clone(),
        session_approved: session_approved.clone(),
        working_directory: session.working_directory.clone(),
    })));

    let plan_registry = ToolRegistry::plan_registry(&config);
    let mut active_registry = if config.plan_mode { &plan_registry } else { &registry };
    let mut plan_tasks: Vec<PlanTask> = Vec::new();
    let mut next_task_id: u32 = 1;
    let working_dir_path = std::path::Path::new(&session.working_directory)
        .canonicalize()
        .unwrap_or_else(|_| std::path::PathBuf::from(&session.working_directory));

    let system_prompt = build_system_prompt(
        config.system_prompt.as_deref(),
        &session.working_directory,
        &config.provider.model,
        config.plan_mode,
        &config,
    )
    .await;

    loop {
        // Check for cancellation
        if cancel_rx.try_recv().is_ok() {
            let _ = cancel_broadcast.send(());
            let _ = event_tx.send(AgentEvent::Cancelled);
            break;
        }

        // Send thinking event before making API request
        let _ = event_tx.send(AgentEvent::Thinking);

        let request = build_request(
            &session.messages,
            active_registry,
            &config.provider.model,
            config.provider.max_tokens,
            config.provider.temperature,
            config.provider.thinking_effort.as_deref(),
            Some(&system_prompt),
            &config.context,
            &config.memory,
        );

        let mut attempt = 0usize;
        let stream = 'retry: loop {
            attempt += 1;
            match provider.stream_message(request.clone()).await {
                Ok(s) => break 'retry s,
                Err(e) if e.is_retryable() && attempt < retry_config.max_attempts => {
                    let delay = retry_config
                        .initial_backoff
                        .mul_f32(2f32.powi((attempt - 1) as i32));
                    let _ = event_tx.send(AgentEvent::RetryScheduled {
                        error: e.to_string(),
                        attempt,
                        max_attempts: retry_config.max_attempts,
                        delay_ms: delay.as_millis().min(u128::from(u64::MAX)) as u64,
                    });
                    tokio::select! {
                        _ = cancel_rx.recv() => {
                            let _ = cancel_broadcast.send(());
                            let _ = event_tx.send(AgentEvent::Cancelled);
                            return;
                        }
                        _ = tokio::time::sleep(delay) => {}
                    }
                }
                Err(e) => {
                    let _ = event_tx.send(AgentEvent::Error(format!("Stream error: {}", e)));
                    return;
                }
            }
        };

        let mut text_acc = String::new();
        let mut tool_calls: Vec<PendingToolCall> = Vec::new();
        let mut current_tool: Option<PendingToolCall> = None;
        let mut usage = TokenUsage::default();

        futures::pin_mut!(stream);

        let mut cancelled = false;
        loop {
            tokio::select! {
                _ = cancel_rx.recv() => {
                    cancelled = true;
                    let _ = cancel_broadcast.send(());
                    let _ = event_tx.send(AgentEvent::Cancelled);
                    break;
                }
                event_result = stream.next() => {
                    match event_result {
                        None => break,
                        Some(Err(e)) => {
                            let _ = event_tx.send(AgentEvent::Error(format!("Stream error: {}", e)));
                            break;
                        }
                        Some(Ok(event)) => match event {
                            crate::provider::types::LLMStreamEvent::TextDelta(text) => {
                                text_acc.push_str(&text);
                                let _ = event_tx.send(AgentEvent::StreamDelta(text));
                            }
                            crate::provider::types::LLMStreamEvent::ToolCallStart { id, name } => {
                                if let Some(tc) = current_tool.take() {
                                    tool_calls.push(tc);
                                }
                                current_tool = Some(PendingToolCall {
                                    id,
                                    name,
                                    arguments_json: String::new(),
                                    arguments: serde_json::Value::Object(serde_json::Map::new()),
                                });
                            }
                            crate::provider::types::LLMStreamEvent::ToolCallDelta(chunk) => {
                                if let Some(tc) = &mut current_tool {
                                    tc.arguments_json.push_str(&chunk);
                                }
                            }
                            crate::provider::types::LLMStreamEvent::ToolCallEnd => {
                                if let Some(mut tc) = current_tool.take() {
                                    tc.arguments = serde_json::from_str(&tc.arguments_json)
                                        .unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new()));
                                    tool_calls.push(tc);
                                }
                            }
                            crate::provider::types::LLMStreamEvent::Usage(u) => {
                                usage.input_tokens += u.input_tokens;
                                usage.output_tokens += u.output_tokens;
                            }
                            crate::provider::types::LLMStreamEvent::Done => {
                                if let Some(tc) = current_tool.take() {
                                    tool_calls.push(tc);
                                }
                            }
                        },
                    }
                }
            }
        }

        if cancelled {
            break;
        }

        session.total_input_tokens += usage.input_tokens;
        session.total_output_tokens += usage.output_tokens;

        // Build assistant message
        let mut parts = Vec::new();
        if !text_acc.is_empty() {
            parts.push(MessagePart::Text { text: text_acc });
        }
        for tc in &tool_calls {
            let args: serde_json::Value =
                serde_json::from_str(&tc.arguments_json).unwrap_or(serde_json::Value::Object(Default::default()));
            parts.push(MessagePart::ToolCall {
                id: tc.id.clone(),
                name: tc.name.clone(),
                arguments: args,
            });
        }

        for tc in &tool_calls {
            let args: serde_json::Value =
                serde_json::from_str(&tc.arguments_json).unwrap_or(serde_json::Value::Object(Default::default()));

            let _ = event_tx.send(AgentEvent::ToolCallRequested {
                id: tc.id.clone(),
                name: tc.name.clone(),
                arguments: args,
            });
        }

        let _ = event_tx.send(AgentEvent::ResponseComplete {
            finish_reason: None,
            usage,
        });
        session.messages.push(ConversationMessage::assistant(parts));

        if tool_calls.is_empty() {
            // LLM-based compaction before signaling done
            let interaction_count: usize =
                session.messages.iter().map(|m| m.parts.len().max(1)).sum();
            if config.context.enable_compaction
                && interaction_count > config.context.compact_after_messages
            {
                let _ = event_tx.send(AgentEvent::Compacting);
                match llm_compact_messages(
                    &session.messages,
                    &config.context,
                    provider.as_ref(),
                    &config.provider.model,
                    config.provider.max_tokens,
                )
                .await
                {
                    Ok(compacted) if compacted.len() < session.messages.len() => {
                        session.messages = compacted;
                    }
                    Err(e) => {
                        let _ = event_tx.send(AgentEvent::Error(format!(
                            "Compaction failed (non-fatal): {}",
                            e
                        )));
                    }
                    _ => {}
                }
            }
            let _ = event_tx.send(AgentEvent::Done);
            break;
        }

        // Separate sub_agent calls from regular tool calls for parallel execution
        let mut sub_agent_calls: Vec<&PendingToolCall> = Vec::new();
        let mut regular_calls: Vec<&PendingToolCall> = Vec::new();
        for tc in &tool_calls {
            if tc.name == "sub_agent" {
                sub_agent_calls.push(tc);
            } else {
                regular_calls.push(tc);
            }
        }

        // Execute regular tools sequentially (includes plan-mode tools)
        for tc in &regular_calls {
            if cancel_rx.try_recv().is_ok() {
                let _ = cancel_broadcast.send(());
                let _ = event_tx.send(AgentEvent::Cancelled);
                return;
            }

            let args: serde_json::Value =
                serde_json::from_str(&tc.arguments_json).unwrap_or(serde_json::Value::Object(Default::default()));

            if tc.name == ENTER_PLAN_MODE_TOOL_NAME {
                config.plan_mode = true;
                active_registry = &plan_registry;
                let result = args
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .map(|reason| format!("Switched to plan mode: {}", reason))
                    .unwrap_or_else(|| "Switched to plan mode".to_string());

                let _ = event_tx.send(AgentEvent::ToolResult {
                    tool_call_id: tc.id.clone(),
                    name: tc.name.clone(),
                    content: result.clone(),
                    is_error: false,
                    arguments: Some(args.clone()),
                });
                session.messages.push(ConversationMessage::tool_result(
                    tc.id.clone(),
                    tc.name.clone(),
                    result,
                    false,
                    Some(args.clone()),
                ));
                let _ = event_tx.send(AgentEvent::EnteredPlanMode);
                continue;
            }

            if tc.name == ADD_TASK_TOOL_NAME {
                let header = args.get("header").and_then(|v| v.as_str());
                let description = args.get("description").and_then(|v| v.as_str());
                let after_task_id = args.get("after_task_id").and_then(|v| v.as_str());

                let (result, is_error) = match (header, description) {
                    (Some(header), Some(description)) => {
                        let id = format!("task-{}", next_task_id);
                        next_task_id += 1;
                        let task = PlanTask {
                            id: id.clone(),
                            header: header.to_string(),
                            description: description.to_string(),
                            status: TaskStatus::NotStarted,
                        };
                        if let Some(after_id) = after_task_id {
                            if let Some(pos) = plan_tasks.iter().position(|t| t.id == after_id) {
                                plan_tasks.insert(pos + 1, task);
                            } else {
                                plan_tasks.push(task);
                            }
                        } else {
                            plan_tasks.push(task);
                        }
                        (format!("Task added: {}", id), false)
                    }
                    _ => ("Missing required 'header' or 'description'".to_string(), true),
                };

                if !is_error {
                    let _ = event_tx.send(AgentEvent::PlanUpdated {
                        tasks: plan_tasks.clone(),
                    });
                }
                session.messages.push(ConversationMessage::tool_result(
                    tc.id.clone(), tc.name.clone(), result.clone(), is_error, Some(args.clone()),
                ));
                let _ = event_tx.send(AgentEvent::ToolResult {
                    tool_call_id: tc.id.clone(), name: tc.name.clone(),
                    content: result, is_error, arguments: Some(args.clone()),
                });
                continue;
            }

            if tc.name == EDIT_TASK_TOOL_NAME {
                let task_id = args.get("task_id").and_then(|v| v.as_str());
                let new_header = args.get("header").and_then(|v| v.as_str());
                let new_description = args.get("description").and_then(|v| v.as_str());

                let (result, is_error) = match task_id {
                    Some(task_id) => {
                        if let Some(task) = plan_tasks.iter_mut().find(|t| t.id == task_id) {
                            if let Some(h) = new_header { task.header = h.to_string(); }
                            if let Some(d) = new_description { task.description = d.to_string(); }
                            ("Task edited".to_string(), false)
                        } else {
                            (format!("Task not found: {}", task_id), true)
                        }
                    }
                    None => ("Missing required 'task_id'".to_string(), true),
                };

                if !is_error {
                    let _ = event_tx.send(AgentEvent::PlanUpdated {
                        tasks: plan_tasks.clone(),
                    });
                }
                session.messages.push(ConversationMessage::tool_result(
                    tc.id.clone(), tc.name.clone(), result.clone(), is_error, Some(args.clone()),
                ));
                let _ = event_tx.send(AgentEvent::ToolResult {
                    tool_call_id: tc.id.clone(), name: tc.name.clone(),
                    content: result, is_error, arguments: Some(args.clone()),
                });
                continue;
            }

            if tc.name == READ_TASK_TOOL_NAME {
                let task_id = args.get("task_id").and_then(|v| v.as_str());
                let (result, is_error) = match task_id {
                    Some(task_id) => {
                        if let Some(task) = plan_tasks.iter().find(|t| t.id == task_id) {
                            (serde_json::to_string_pretty(task).unwrap_or_else(|_| "Serialization error".into()), false)
                        } else {
                            (format!("Task not found: {}", task_id), true)
                        }
                    }
                    None => ("Missing required 'task_id'".to_string(), true),
                };
                session.messages.push(ConversationMessage::tool_result(
                    tc.id.clone(), tc.name.clone(), result.clone(), is_error, Some(args.clone()),
                ));
                let _ = event_tx.send(AgentEvent::ToolResult {
                    tool_call_id: tc.id.clone(), name: tc.name.clone(),
                    content: result, is_error, arguments: Some(args.clone()),
                });
                continue;
            }

            if tc.name == READ_PLAN_TOOL_NAME {
                let result = if plan_tasks.is_empty() {
                    "No plan stored".to_string()
                } else {
                    serde_json::to_string_pretty(&plan_tasks).unwrap_or_else(|_| "Serialization error".into())
                };
                session.messages.push(ConversationMessage::tool_result(
                    tc.id.clone(), tc.name.clone(), result.clone(), false, Some(args.clone()),
                ));
                let _ = event_tx.send(AgentEvent::ToolResult {
                    tool_call_id: tc.id.clone(), name: tc.name.clone(),
                    content: result, is_error: false, arguments: Some(args.clone()),
                });
                continue;
            }

            if tc.name == REVIEW_PLAN_TOOL_NAME {
                let is_error = plan_tasks.is_empty();
                let result = if is_error {
                    "No plan stored".to_string()
                } else {
                    "Plan submitted for review".to_string()
                };
                session.messages.push(ConversationMessage::tool_result(
                    tc.id.clone(), tc.name.clone(), result.clone(), is_error, Some(args.clone()),
                ));
                let _ = event_tx.send(AgentEvent::ToolResult {
                    tool_call_id: tc.id.clone(), name: tc.name.clone(),
                    content: result, is_error, arguments: Some(args.clone()),
                });
                if !is_error {
                    let _ = event_tx.send(AgentEvent::PlanReadyForReview {
                        tasks: plan_tasks.clone(),
                    });
                }
                continue;
            }

            if tc.name == UPDATE_TASK_STATUS_TOOL_NAME {
                let task_id = args.get("task_id").and_then(|v| v.as_str());
                let status_str = args.get("status").and_then(|v| v.as_str());

                let (result, is_error) = match (task_id, status_str) {
                    (Some(task_id), Some(status_str)) => {
                        let status = match status_str {
                            "not_started" => Some(TaskStatus::NotStarted),
                            "in_progress" => Some(TaskStatus::InProgress),
                            "done" => Some(TaskStatus::Done),
                            _ => None,
                        };
                        match status {
                            Some(status) => {
                                if let Some(task) = plan_tasks.iter_mut().find(|t| t.id == task_id) {
                                    task.status = status;
                                    ("Status updated".to_string(), false)
                                } else {
                                    (format!("Task not found: {}", task_id), true)
                                }
                            }
                            None => (format!("Invalid status: {}. Use not_started, in_progress, or done", status_str), true),
                        }
                    }
                    _ => ("Missing required 'task_id' or 'status'".to_string(), true),
                };

                if !is_error {
                    let _ = event_tx.send(AgentEvent::PlanTaskStatusChanged {
                        tasks: plan_tasks.clone(),
                    });
                }
                session.messages.push(ConversationMessage::tool_result(
                    tc.id.clone(), tc.name.clone(), result.clone(), is_error, Some(args.clone()),
                ));
                let _ = event_tx.send(AgentEvent::ToolResult {
                    tool_call_id: tc.id.clone(), name: tc.name.clone(),
                    content: result, is_error, arguments: Some(args.clone()),
                });
                continue;
            }

            execute_tool(
                tc, &args, active_registry, &config, &session_approved,
                &working_dir_path, &event_tx, &mut approval_rx, session,
            ).await;

            // After each tool result, check if the user queued a message.
            if let Ok(queued_text) = queued_message_rx.try_recv() {
                session.messages.push(ConversationMessage::user(&queued_text));
                let _ = event_tx.send(AgentEvent::QueuedMessageInjected);
            }
        }

        // Execute sub_agent calls in parallel
        if !sub_agent_calls.is_empty() {
            use tokio::task::JoinSet;

            let mut join_set = JoinSet::new();
            let mut id_order: Vec<String> = Vec::new();

            for tc in &sub_agent_calls {
                let mut args: serde_json::Value =
                    serde_json::from_str(&tc.arguments_json).unwrap_or(serde_json::Value::Object(Default::default()));

                // Inject _tool_call_id so the sub-agent tool knows its parent ID
                if let Some(obj) = args.as_object_mut() {
                    obj.insert("_tool_call_id".to_string(), serde_json::Value::String(tc.id.clone()));
                }

                let tool = active_registry.get("sub_agent");
                let tc_id = tc.id.clone();
                let tc_name = tc.name.clone();
                let tc_arguments = tc.arguments.clone();
                let event_tx_clone = event_tx.clone();
                id_order.push(tc_id.clone());

                if let Some(t) = tool {
                    // Send tool execution started event
                    let _ = event_tx.send(AgentEvent::ToolExecutionStarted {
                        tool_call_id: tc_id.clone(),
                        name: tc_name.clone(),
                    });

                    // We need to get the streaming result and spawn the relay
                    match t.execute_streaming(args.clone()).await {
                        Ok((stream_rx, result_rx)) => {
                            // Spawn a task that drains the stream and waits for result
                            join_set.spawn(async move {
                                if let Some(mut rx) = stream_rx {
                                    while let Some((stream, text)) = rx.recv().await {
                                        let _ = event_tx_clone.send(AgentEvent::ToolOutputDelta {
                                            tool_call_id: tc_id.clone(),
                                            stream,
                                            output: text,
                                        });
                                    }
                                }
                                let (content, is_error) = match result_rx.await {
                                    Ok(Ok(result)) => (result, false),
                                    Ok(Err(e)) => (format!("Tool error: {}", e), true),
                                    Err(_) => ("Tool error: streaming result channel closed unexpectedly".into(), true),
                                };
                                (tc_id, tc_name, content, is_error, tc_arguments)
                            });
                        }
                        Err(e) => {
                            // Immediate failure — record result directly
                            let content = format!("Tool error: {}", e);
                            let msg = ConversationMessage::tool_result(
                                tc_id.clone(), tc_name.clone(), content.clone(), true, Some(tc_arguments.clone()),
                            );
                            session.messages.push(msg);
                            let _ = event_tx.send(AgentEvent::ToolResult {
                                tool_call_id: tc_id,
                                name: tc_name,
                                content,
                                is_error: true,
                                arguments: Some(tc_arguments),
                            });
                        }
                    }
                } else {
                    let content = format!("Unknown tool: {}", tc_name);
                    let msg = ConversationMessage::tool_result(
                        tc_id.clone(), tc_name.clone(), content.clone(), true, Some(tc_arguments.clone()),
                    );
                    session.messages.push(msg);
                    let _ = event_tx.send(AgentEvent::ToolResult {
                        tool_call_id: tc_id,
                        name: tc_name,
                        content,
                        is_error: true,
                        arguments: Some(tc_arguments),
                    });
                }
            }

            // Wait for all sub-agents to complete
            while let Some(result) = join_set.join_next().await {
                if let Ok((tc_id, tc_name, content, is_error, tc_arguments)) = result {
                    let msg = ConversationMessage::tool_result(
                        tc_id.clone(), tc_name.clone(), content.clone(), is_error, Some(tc_arguments.clone()),
                    );
                    session.messages.push(msg);
                    let _ = event_tx.send(AgentEvent::ToolResult {
                        tool_call_id: tc_id,
                        name: tc_name,
                        content,
                        is_error,
                        arguments: Some(tc_arguments),
                    });
                }
            }
        }
    }
}

/// Execute a single tool (non-plan, non-sub-agent). Handles approval and streaming.
async fn execute_tool(
    tc: &PendingToolCall,
    args: &serde_json::Value,
    registry: &ToolRegistry,
    config: &AppConfig,
    session_approved: &Arc<RwLock<HashSet<String>>>,
    working_dir_path: &std::path::Path,
    event_tx: &mpsc::UnboundedSender<AgentEvent>,
    approval_rx: &mut mpsc::UnboundedReceiver<ApprovalResponse>,
    session: &mut Session,
) {
    let tool = registry.get(&tc.name);

    // Determine what needs approval
    let (needs_approval, approval_key) = if tool.is_some_and(|t| t.requires_approval()) {
        if config.auto_approve.contains(&tc.name) {
            (false, String::new())
        } else {
            let approval_key = match tc.name.as_str() {
                "write_file" | "edit_file" => {
                    if let Some(path_str) = args.get("path").and_then(|v| v.as_str()) {
                        let file_path = std::path::Path::new(path_str);
                        let canonical = file_path.canonicalize().unwrap_or_else(|_| file_path.to_path_buf());
                        if canonical.starts_with(working_dir_path) {
                            format!("{}:{}", tc.name, canonical.display())
                        } else {
                            format!("{}:outside:{}", tc.name, canonical.display())
                        }
                    } else {
                        format!("{}:unknown", tc.name)
                    }
                }
                _ => {
                    format!("{}:once", tc.name)
                }
            };
            let already_approved = session_approved.read().await.contains(&approval_key);
            (!already_approved, approval_key)
        }
    } else {
        (false, String::new())
    };

    if needs_approval {
        let _ = event_tx.send(AgentEvent::ToolApprovalNeeded {
            id: tc.id.clone(),
            name: tc.name.clone(),
            arguments: args.clone(),
        });

        match approval_rx.recv().await {
            Some(ApprovalResponse::Approve) => {}
            Some(ApprovalResponse::ApproveForSession) => {
                session_approved.write().await.insert(approval_key);
            }
            Some(ApprovalResponse::ApproveAlways) => {
                session_approved.write().await.insert(approval_key);
            }
            Some(ApprovalResponse::Deny) | None => {
                let msg = ConversationMessage::tool_result(
                    tc.id.clone(), tc.name.clone(), "Denied by user".into(), true, Some(args.clone()),
                );
                session.messages.push(msg);
                let _ = event_tx.send(AgentEvent::ToolResult {
                    tool_call_id: tc.id.clone(),
                    name: tc.name.clone(),
                    content: "Denied by user".into(),
                    is_error: true,
                    arguments: Some(args.clone()),
                });
                return;
            }
        }
    }

    let (content, is_error) = match tool {
        Some(t) => {
            let _ = event_tx.send(AgentEvent::ToolExecutionStarted {
                tool_call_id: tc.id.clone(),
                name: tc.name.clone(),
            });

            match t.execute_streaming(args.clone()).await {
                Ok((stream_rx, result_rx)) => {
                    if let Some(mut rx) = stream_rx {
                        while let Some((stream, text)) = rx.recv().await {
                            let _ = event_tx.send(AgentEvent::ToolOutputDelta {
                                tool_call_id: tc.id.clone(),
                                stream,
                                output: text,
                            });
                        }
                    }

                    match result_rx.await {
                        Ok(Ok(result)) => (result, false),
                        Ok(Err(e)) => (format!("Tool error: {}", e), true),
                        Err(_) => (
                            "Tool error: streaming result channel closed unexpectedly".into(),
                            true,
                        ),
                    }
                }
                Err(e) => (format!("Tool error: {}", e), true),
            }
        }
        None => (format!("Unknown tool: {}", tc.name), true),
    };

    let msg = ConversationMessage::tool_result(
        tc.id.clone(), tc.name.clone(), content.clone(), is_error, Some(tc.arguments.clone()),
    );
    session.messages.push(msg);

    let _ = event_tx.send(AgentEvent::ToolResult {
        tool_call_id: tc.id.clone(),
        name: tc.name.clone(),
        content,
        is_error,
        arguments: Some(tc.arguments.clone()),
    });
}

/// Streamlined agent loop for sub-agents. No plan mode, no sub_agent tool, no queued messages.
/// Shares approval state with parent via `session_approved`.
pub async fn run_sub_agent_loop(
    config: AppConfig,
    session: &mut Session,
    event_tx: mpsc::UnboundedSender<AgentEvent>,
    mut approval_rx: mpsc::UnboundedReceiver<ApprovalResponse>,
    mut cancel_rx: mpsc::UnboundedReceiver<()>,
    session_approved: Arc<RwLock<HashSet<String>>>,
) {
    let provider = match provider::build_provider(&config.provider) {
        Ok(p) => p,
        Err(e) => {
            let _ = event_tx.send(AgentEvent::Error(format!("Provider error: {}", e)));
            return;
        }
    };
    let retry_config = RetryConfig::default();

    let registry = ToolRegistry::sub_agent_registry(&config);
    let working_dir_path = std::path::Path::new(&session.working_directory)
        .canonicalize()
        .unwrap_or_else(|_| std::path::PathBuf::from(&session.working_directory));

    let system_prompt = build_sub_agent_system_prompt(
        config.system_prompt.as_deref(),
        &session.working_directory,
        &config.provider.model,
        &config,
    )
    .await;

    loop {
        if cancel_rx.try_recv().is_ok() {
            let _ = event_tx.send(AgentEvent::Cancelled);
            break;
        }

        let _ = event_tx.send(AgentEvent::Thinking);

        let request = build_request(
            &session.messages,
            &registry,
            &config.provider.model,
            config.provider.max_tokens,
            config.provider.temperature,
            config.provider.thinking_effort.as_deref(),
            Some(&system_prompt),
            &config.context,
            &config.memory,
        );

        let mut attempt = 0usize;
        let stream = 'retry: loop {
            attempt += 1;
            match provider.stream_message(request.clone()).await {
                Ok(s) => break 'retry s,
                Err(e) if e.is_retryable() && attempt < retry_config.max_attempts => {
                    let delay = retry_config
                        .initial_backoff
                        .mul_f32(2f32.powi((attempt - 1) as i32));
                    let _ = event_tx.send(AgentEvent::RetryScheduled {
                        error: e.to_string(),
                        attempt,
                        max_attempts: retry_config.max_attempts,
                        delay_ms: delay.as_millis().min(u128::from(u64::MAX)) as u64,
                    });
                    tokio::select! {
                        _ = cancel_rx.recv() => {
                            let _ = event_tx.send(AgentEvent::Cancelled);
                            return;
                        }
                        _ = tokio::time::sleep(delay) => {}
                    }
                }
                Err(e) => {
                    let _ = event_tx.send(AgentEvent::Error(format!("Stream error: {}", e)));
                    return;
                }
            }
        };

        let mut text_acc = String::new();
        let mut tool_calls: Vec<PendingToolCall> = Vec::new();
        let mut current_tool: Option<PendingToolCall> = None;
        let mut usage = TokenUsage::default();

        futures::pin_mut!(stream);

        let mut cancelled = false;
        loop {
            tokio::select! {
                _ = cancel_rx.recv() => {
                    cancelled = true;
                    let _ = event_tx.send(AgentEvent::Cancelled);
                    break;
                }
                event_result = stream.next() => {
                    match event_result {
                        None => break,
                        Some(Err(e)) => {
                            let _ = event_tx.send(AgentEvent::Error(format!("Stream error: {}", e)));
                            break;
                        }
                        Some(Ok(event)) => match event {
                            crate::provider::types::LLMStreamEvent::TextDelta(text) => {
                                text_acc.push_str(&text);
                                let _ = event_tx.send(AgentEvent::StreamDelta(text));
                            }
                            crate::provider::types::LLMStreamEvent::ToolCallStart { id, name } => {
                                if let Some(tc) = current_tool.take() {
                                    tool_calls.push(tc);
                                }
                                current_tool = Some(PendingToolCall {
                                    id, name,
                                    arguments_json: String::new(),
                                    arguments: serde_json::Value::Object(serde_json::Map::new()),
                                });
                            }
                            crate::provider::types::LLMStreamEvent::ToolCallDelta(chunk) => {
                                if let Some(tc) = &mut current_tool {
                                    tc.arguments_json.push_str(&chunk);
                                }
                            }
                            crate::provider::types::LLMStreamEvent::ToolCallEnd => {
                                if let Some(mut tc) = current_tool.take() {
                                    tc.arguments = serde_json::from_str(&tc.arguments_json)
                                        .unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new()));
                                    tool_calls.push(tc);
                                }
                            }
                            crate::provider::types::LLMStreamEvent::Usage(u) => {
                                usage.input_tokens += u.input_tokens;
                                usage.output_tokens += u.output_tokens;
                            }
                            crate::provider::types::LLMStreamEvent::Done => {
                                if let Some(tc) = current_tool.take() {
                                    tool_calls.push(tc);
                                }
                            }
                        },
                    }
                }
            }
        }

        if cancelled { break; }

        session.total_input_tokens += usage.input_tokens;
        session.total_output_tokens += usage.output_tokens;

        let mut parts = Vec::new();
        if !text_acc.is_empty() {
            parts.push(MessagePart::Text { text: text_acc });
        }
        for tc in &tool_calls {
            let args: serde_json::Value =
                serde_json::from_str(&tc.arguments_json).unwrap_or(serde_json::Value::Object(Default::default()));
            parts.push(MessagePart::ToolCall {
                id: tc.id.clone(),
                name: tc.name.clone(),
                arguments: args,
            });
        }

        for tc in &tool_calls {
            let args: serde_json::Value =
                serde_json::from_str(&tc.arguments_json).unwrap_or(serde_json::Value::Object(Default::default()));
            let _ = event_tx.send(AgentEvent::ToolCallRequested {
                id: tc.id.clone(),
                name: tc.name.clone(),
                arguments: args,
            });
        }

        let _ = event_tx.send(AgentEvent::ResponseComplete {
            finish_reason: None,
            usage,
        });
        session.messages.push(ConversationMessage::assistant(parts));

        if tool_calls.is_empty() {
            let _ = event_tx.send(AgentEvent::Done);
            break;
        }

        // Execute tools sequentially in the sub-agent
        for tc in &tool_calls {
            if cancel_rx.try_recv().is_ok() {
                let _ = event_tx.send(AgentEvent::Cancelled);
                return;
            }

            let args: serde_json::Value =
                serde_json::from_str(&tc.arguments_json).unwrap_or(serde_json::Value::Object(Default::default()));

            execute_tool(
                tc, &args, &registry, &config, &session_approved,
                &working_dir_path, &event_tx, &mut approval_rx, session,
            ).await;
        }
    }
}

struct PendingToolCall {
    id: String,
    name: String,
    arguments_json: String,
    arguments: serde_json::Value,
}
