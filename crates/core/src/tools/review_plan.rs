use async_trait::async_trait;
use serde_json::json;

use super::{Tool, ToolError};

pub struct ReviewPlanTool;

#[async_trait]
impl Tool for ReviewPlanTool {
    fn name(&self) -> &str {
        "review_plan"
    }

    fn description(&self) -> &str {
        "Ask the app to present the current stored plan to the user for review without reprinting it in assistant text."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }

    fn requires_approval(&self) -> bool {
        false
    }

    async fn execute(&self, _args: serde_json::Value) -> Result<String, ToolError> {
        Ok("Plan submitted for review".to_string())
    }
}
