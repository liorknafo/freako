use async_trait::async_trait;
use serde_json::json;
use super::{Tool, ToolError};

pub struct ListDirTool;

#[async_trait]
impl Tool for ListDirTool {
    fn name(&self) -> &str { "list_dir" }
    fn description(&self) -> &str { "List directory contents." }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Directory to list (default: .)" }
            }
        })
    }

    fn requires_approval(&self) -> bool { false }

    async fn execute(&self, args: serde_json::Value) -> Result<String, ToolError> {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let mut entries = tokio::fs::read_dir(path).await.map_err(ToolError::Io)?;

        let mut dirs = Vec::new();
        let mut files = Vec::new();

        while let Some(entry) = entries.next_entry().await.map_err(ToolError::Io)? {
            let name = entry.file_name().to_string_lossy().to_string();
            let ft = entry.file_type().await.map_err(ToolError::Io)?;
            if ft.is_dir() {
                dirs.push(format!("  {}/", name));
            } else {
                let size = entry.metadata().await.ok().map(|m| m.len()).unwrap_or(0);
                files.push(format!("  {} ({})", name, format_size(size)));
            }
        }

        dirs.sort();
        files.sort();

        let mut result = format!("Directory: {}\n", path);
        if !dirs.is_empty() { result.push_str(&format!("\nDirectories ({}):\n{}\n", dirs.len(), dirs.join("\n"))); }
        if !files.is_empty() { result.push_str(&format!("\nFiles ({}):\n{}\n", files.len(), files.join("\n"))); }
        if dirs.is_empty() && files.is_empty() { result.push_str("\n  (empty)\n"); }
        Ok(result)
    }
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    if bytes < KB { format!("{} B", bytes) }
    else if bytes < MB { format!("{:.1} KB", bytes as f64 / KB as f64) }
    else { format!("{:.1} MB", bytes as f64 / MB as f64) }
}
