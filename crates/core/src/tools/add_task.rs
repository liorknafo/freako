use async_trait::async_trait;
use serde_json::json;

use super::{Tool, ToolError};

pub struct AddTaskTool;

#[async_trait]
impl Tool for AddTaskTool {
    fn name(&self) -> &str {
        "add_task"
    }

    fn description(&self) -> &str {
        "Add a new task to the plan. Provide a short header and a markdown description."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "header": {
                    "type": "string",
                    "description": "Short title for the task"
                },
                "description": {
                    "type": "string",
                    "description": "Markdown description of what the task involves"
                },
                "after_task_id": {
                    "type": "string",
                    "description": "Optional: insert this task after the task with this ID"
                }
            },
            "required": ["header", "description"],
            "additionalProperties": false
        })
    }

    fn requires_approval(&self) -> bool {
        false
    }

    async fn execute(&self, _args: serde_json::Value) -> Result<String, ToolError> {
        Ok("Task added".to_string())
    }
}
