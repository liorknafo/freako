use crate::provider::types::TokenUsage;

pub const ENTER_PLAN_MODE_TOOL_NAME: &str = "enter_plan_mode";
pub const EDIT_PLAN_TOOL_NAME: &str = "edit_plan";
pub const READ_PLAN_TOOL_NAME: &str = "read_plan";
pub const REVIEW_PLAN_TOOL_NAME: &str = "review_plan";

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolOutputStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone)]
pub enum AgentEvent {
    StreamDelta(String),
    RetryScheduled {
        error: String,
        attempt: usize,
        max_attempts: usize,
        delay_ms: u64,
    },
    ToolCallRequested {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
    ToolApprovalNeeded {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
    ToolExecutionStarted {
        tool_call_id: String,
        name: String,
    },
    ToolOutputDelta {
        tool_call_id: String,
        stream: ToolOutputStream,
        output: String,
    },
    ToolResult {
        tool_call_id: String,
        name: String,
        content: String,
        is_error: bool,
        arguments: Option<serde_json::Value>,
    },
    ResponseComplete {
        finish_reason: Option<String>,
        usage: TokenUsage,
    },
    Thinking,
    EnteredPlanMode,
    PlanUpdated {
        content: String,
    },
    PlanReadyForReview {
        content: String,
    },
    Done,
    Cancelled,
    Error(String),
    QueuedMessageInjected,
}
