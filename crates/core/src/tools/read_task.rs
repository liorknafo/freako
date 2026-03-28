use async_trait::async_trait;
use serde_json::json;

use super::{Tool, ToolError};

pub struct ReadTaskTool;

#[async_trait]
impl Tool for ReadTaskTool {
    fn name(&self) -> &str {
        "read_task"
    }

    fn description(&self) -> &str {
        "Read a specific task by ID. Returns the task's header, description, and status."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "The ID of the task to read"
                }
            },
            "required": ["task_id"],
            "additionalProperties": false
        })
    }

    fn requires_approval(&self) -> bool {
        false
    }

    async fn execute(&self, _args: serde_json::Value) -> Result<String, ToolError> {
        Ok("No plan stored".to_string())
    }
}
