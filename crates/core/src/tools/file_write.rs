use async_trait::async_trait;
use serde_json::json;
use super::{Tool, ToolError};

pub struct FileWriteTool;

#[async_trait]
impl Tool for FileWriteTool {
    fn name(&self) -> &str { "write_file" }
    fn description(&self) -> &str { "Create or overwrite a file with the given content." }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "The file path to write to" },
                "content": { "type": "string", "description": "The content to write" }
            },
            "required": ["path", "content"]
        })
    }

    fn requires_approval(&self) -> bool { true }

    async fn execute(&self, args: serde_json::Value) -> Result<String, ToolError> {
        let path = args.get("path").and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("Missing 'path'".into()))?;
        let content = args.get("content").and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("Missing 'content'".into()))?;

        if let Some(parent) = std::path::Path::new(path).parent() {
            tokio::fs::create_dir_all(parent).await.map_err(ToolError::Io)?;
        }
        tokio::fs::write(path, content).await.map_err(ToolError::Io)?;
        Ok(format!("Wrote {} lines to {}", content.lines().count(), path))
    }
}
