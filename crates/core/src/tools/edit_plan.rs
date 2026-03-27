use async_trait::async_trait;
use serde_json::json;

use super::{Tool, ToolError};

pub struct EditPlanTool;

#[async_trait]
impl Tool for EditPlanTool {
    fn name(&self) -> &str {
        "edit_plan"
    }

    fn description(&self) -> &str {
        "Create or update the stored plan. Supports full replacement, appending, or an exact-string edit of the current plan."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "mode": {
                    "type": "string",
                    "enum": ["replace", "append", "edit"],
                    "description": "How to update the stored plan"
                },
                "content": {
                    "type": "string",
                    "description": "Full replacement content for replace mode, or text to append in append mode"
                },
                "old_text": {
                    "type": "string",
                    "description": "Exact text to replace when mode is edit"
                },
                "new_text": {
                    "type": "string",
                    "description": "Replacement text when mode is edit"
                }
            },
            "required": ["mode"],
            "additionalProperties": false
        })
    }

    fn requires_approval(&self) -> bool {
        false
    }

    async fn execute(&self, _args: serde_json::Value) -> Result<String, ToolError> {
        Ok("Plan updated".to_string())
    }
}
