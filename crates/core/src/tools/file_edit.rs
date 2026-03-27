use async_trait::async_trait;
use serde_json::json;
use super::{Tool, ToolError};
use crate::tools::diff::render_diff;

pub struct FileEditTool;

#[async_trait]
impl Tool for FileEditTool {
    fn name(&self) -> &str { "edit_file" }
    fn description(&self) -> &str { "Search-and-replace edit. old_string must match exactly once." }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "The file path to edit" },
                "old_string": { "type": "string", "description": "Exact string to find" },
                "new_string": { "type": "string", "description": "Replacement string" }
            },
            "required": ["path", "old_string", "new_string"]
        })
    }

    fn requires_approval(&self) -> bool { true }

    async fn execute(&self, args: serde_json::Value) -> Result<String, ToolError> {
        let path = args.get("path").and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("Missing 'path'".into()))?;
        let old = args.get("old_string").and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("Missing 'old_string'".into()))?;
        let new = args.get("new_string").and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("Missing 'new_string'".into()))?;

        let content = tokio::fs::read_to_string(path).await.map_err(ToolError::Io)?;
        let count = content.matches(old).count();
        if count == 0 {
            return Err(ToolError::InvalidArgs(format!("old_string not found in {}", path)));
        }
        if count > 1 {
            return Err(ToolError::InvalidArgs(format!("old_string found {} times — must be unique", count)));
        }

        let new_content = content.replacen(old, new, 1);
        tokio::fs::write(path, &new_content).await.map_err(ToolError::Io)?;

        Ok(render_diff(path, &content, &new_content))
    }
}
