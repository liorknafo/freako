use async_trait::async_trait;
use serde_json::json;

use super::{Tool, ToolError};
use crate::memory::store::{canonicalize_scope_key, MemoryStore};
use crate::memory::types::MemoryScope;

pub struct WriteMemoryTool {
    pub data_dir: std::path::PathBuf,
    pub working_dir: String,
}

#[async_trait]
impl Tool for WriteMemoryTool {
    fn name(&self) -> &str { "write_memory" }

    fn description(&self) -> &str {
        "Create or update a persisted memory-bank entry."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "description": "Optional memory ID to update" },
                "scope": {
                    "type": "string",
                    "enum": ["project", "global"],
                    "description": "Memory scope"
                },
                "title": { "type": "string", "description": "Memory title" },
                "content": { "type": "string", "description": "Memory content" }
            },
            "required": ["scope", "title", "content"]
        })
    }

    fn requires_approval(&self) -> bool { true }

    async fn execute(&self, args: serde_json::Value) -> Result<String, ToolError> {
        let scope = args.get("scope").and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("Missing 'scope'".into()))?;
        let title = args.get("title").and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("Missing 'title'".into()))?;
        let content = args.get("content").and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("Missing 'content'".into()))?;
        let id = args.get("id").and_then(|v| v.as_str()).map(str::to_string).unwrap_or_else(|| {
            format!("{}:{}", scope, title.to_lowercase().replace(' ', "-"))
        });

        let (scope, scope_key) = match scope {
            "project" => (MemoryScope::Project, canonicalize_scope_key(&self.working_dir)),
            "global" => (MemoryScope::Global, "global".to_string()),
            _ => return Err(ToolError::InvalidArgs("Invalid 'scope'".into())),
        };

        let store = MemoryStore::open(&self.data_dir)
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        store
            .upsert_memory(&id, scope, &scope_key, title, content)
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        Ok(format!("Saved memory '{}' [{}]", title, id))
    }
}
