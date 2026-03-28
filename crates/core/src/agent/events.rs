use crate::provider::types::TokenUsage;

pub const ENTER_PLAN_MODE_TOOL_NAME: &str = "enter_plan_mode";
pub const ADD_TASK_TOOL_NAME: &str = "add_task";
pub const EDIT_TASK_TOOL_NAME: &str = "edit_task";
pub const READ_TASK_TOOL_NAME: &str = "read_task";
pub const READ_PLAN_TOOL_NAME: &str = "read_plan";
pub const REVIEW_PLAN_TOOL_NAME: &str = "review_plan";
pub const DELETE_TASK_TOOL_NAME: &str = "delete_task";
pub const UPDATE_TASK_STATUS_TOOL_NAME: &str = "update_task_status";

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolOutputStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    NotStarted,
    InProgress,
    Done,
}

impl Default for TaskStatus {
    fn default() -> Self {
        Self::NotStarted
    }
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotStarted => write!(f, "not_started"),
            Self::InProgress => write!(f, "in_progress"),
            Self::Done => write!(f, "done"),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlanTask {
    pub id: String,
    pub header: String,
    pub description: String,
    pub status: TaskStatus,
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
        tasks: Vec<PlanTask>,
    },
    PlanReadyForReview {
        tasks: Vec<PlanTask>,
    },
    PlanTaskStatusChanged {
        tasks: Vec<PlanTask>,
    },
    Compacting,
    Done,
    Cancelled,
    Error(String),
    QueuedMessageInjected,
}
