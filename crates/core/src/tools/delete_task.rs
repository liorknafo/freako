use async_trait::async_trait;
use serde_json::json;

use super::{Tool, ToolError};

pub struct DeleteTaskTool;

#[async_trait]
impl Tool for DeleteTaskTool {
    fn name(&self) -> &str {
        "delete_task"
    }

    fn description(&self) -> &str {
        "Delete a task from the plan by its task_id."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "The ID of the task to delete (e.g. task-1)"
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
        Ok("Task deleted".to_string())
    }
}
