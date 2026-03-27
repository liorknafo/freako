use async_trait::async_trait;

use super::{Tool, ToolError};

pub struct EnterPlanModeTool;

#[async_trait]
impl Tool for EnterPlanModeTool {
    fn name(&self) -> &str {
        "enter_plan_mode"
    }

    fn description(&self) -> &str {
        "Switch the app into plan mode. Use this when execution should stop and the rest of the task should continue as non-mutating planning and research."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "reason": {
                    "type": "string",
                    "description": "Short explanation for why plan mode is needed"
                }
            },
            "additionalProperties": false
        })
    }

    fn requires_approval(&self) -> bool {
        false
    }

    async fn execute(&self, _args: serde_json::Value) -> Result<String, ToolError> {
        Ok("Switching to plan mode".to_string())
    }
}
