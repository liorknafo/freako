use async_trait::async_trait;
use serde_json::json;

use super::{Tool, ToolError};

pub struct ReadPlanTool;

#[async_trait]
impl Tool for ReadPlanTool {
    fn name(&self) -> &str {
        "read_plan"
    }

    fn description(&self) -> &str {
        "Read the latest stored plan draft."
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
        Ok("No plan stored".to_string())
    }
}
