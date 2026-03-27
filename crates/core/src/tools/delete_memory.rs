use async_trait::async_trait;
use serde_json::json;

use super::{Tool, ToolError};
use crate::memory::store::MemoryStore;

pub struct DeleteMemoryTool {
    pub data_dir: std::path::PathBuf,
}

#[async_trait]
impl Tool for DeleteMemoryTool {
    fn name(&self) -> &str { "delete_memory" }

    fn description(&self) -> &str {
        "Delete a persisted memory-bank entry by ID."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "description": "The memory entry ID" }
            },
            "required": ["id"]
        })
    }

    fn requires_approval(&self) -> bool { true }

    async fn execute(&self, args: serde_json::Value) -> Result<String, ToolError> {
        let id = args.get("id").and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("Missing 'id'".into()))?;
        let store = MemoryStore::open(&self.data_dir)
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        store
            .delete_memory(id)
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        Ok(format!("Deleted memory [{}]", id))
    }
}
