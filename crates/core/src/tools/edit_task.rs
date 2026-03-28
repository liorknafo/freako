use async_trait::async_trait;
use serde_json::json;

use super::{Tool, ToolError};

pub struct EditTaskTool;

#[async_trait]
impl Tool for EditTaskTool {
    fn name(&self) -> &str {
        "edit_task"
    }

    fn description(&self) -> &str {
        "Edit an existing task's header or description by task ID."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "The ID of the task to edit"
                },
                "header": {
                    "type": "string",
                    "description": "New header for the task (optional)"
                },
                "description": {
                    "type": "string",
                    "description": "New description for the task (optional)"
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
        Ok("Task edited".to_string())
    }
}
