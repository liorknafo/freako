use async_trait::async_trait;
use serde_json::json;

use super::{Tool, ToolError};
use crate::memory::store::MemoryStore;

pub struct ReadMemoryTool {
    pub data_dir: std::path::PathBuf,
}

#[async_trait]
impl Tool for ReadMemoryTool {
    fn name(&self) -> &str { "read_memory" }

    fn description(&self) -> &str {
        "Read a persisted memory-bank entry by ID."
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

    fn requires_approval(&self) -> bool { false }

    async fn execute(&self, args: serde_json::Value) -> Result<String, ToolError> {
        let id = args.get("id").and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("Missing 'id'".into()))?;
        let store = MemoryStore::open(&self.data_dir)
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        let entry = store
            .load_memory(id)
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?
            .ok_or_else(|| ToolError::ExecutionFailed(format!("Memory not found: {id}")))?;

        Ok(format!(
            "Memory: {}\nID: {}\nScope: {}\nUpdated: {}\n\n{}",
            entry.title,
            entry.id,
            entry.scope.as_str(),
            entry.updated_at.to_rfc3339(),
            entry.content
        ))
    }
}
