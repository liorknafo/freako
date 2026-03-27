use async_trait::async_trait;
use serde_json::json;

use super::{Tool, ToolError};
use crate::memory::store::MemoryStore;

pub struct ListMemoriesTool {
    pub data_dir: std::path::PathBuf,
    pub working_dir: String,
}

#[async_trait]
impl Tool for ListMemoriesTool {
    fn name(&self) -> &str { "list_memories" }

    fn description(&self) -> &str {
        "List persisted memory-bank entries for the current project or global scope."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "scope": {
                    "type": "string",
                    "enum": ["project", "global", "all"],
                    "description": "Memory scope to list (default: all)"
                }
            }
        })
    }

    fn requires_approval(&self) -> bool { false }

    async fn execute(&self, args: serde_json::Value) -> Result<String, ToolError> {
        let scope = args.get("scope").and_then(|v| v.as_str()).unwrap_or("all");
        let store = MemoryStore::open(&self.data_dir)
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        let mut lines = Vec::new();
        if matches!(scope, "project" | "all") {
            let entries = store
                .list_project_memories(&self.working_dir)
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
            lines.push(format!("Project memories ({})", entries.len()));
            for entry in entries {
                lines.push(format!("- {} [{}]", entry.title, entry.id));
            }
        }
        if matches!(scope, "global" | "all") {
            let entries = store
                .list_global_memories()
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
            lines.push(format!("Global memories ({})", entries.len()));
            for entry in entries {
                lines.push(format!("- {} [{}]", entry.title, entry.id));
            }
        }

        if lines.is_empty() {
            lines.push("No memories found.".to_string());
        }

        Ok(lines.join("\n"))
    }
}
