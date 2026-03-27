use async_trait::async_trait;
use serde_json::json;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, BufReader};
use super::{Tool, ToolError};

pub struct FileReadTool;

#[async_trait]
impl Tool for FileReadTool {
    fn name(&self) -> &str { "read_file" }

    fn description(&self) -> &str {
        "Read the contents of a file. Optionally specify a line range."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "The file path to read" },
                "start_line": { "type": "integer", "description": "Optional start line (1-indexed)" },
                "end_line": { "type": "integer", "description": "Optional end line (1-indexed, inclusive)" }
            },
            "required": ["path"]
        })
    }

    fn requires_approval(&self) -> bool { false }

    async fn execute(&self, args: serde_json::Value) -> Result<String, ToolError> {
        let path = args.get("path").and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("Missing 'path'".into()))?;

        let start = args.get("start_line").and_then(|v| v.as_u64()).map(|v| v as usize);
        let end = args.get("end_line").and_then(|v| v.as_u64()).map(|v| v as usize);

        if let (Some(start), Some(end)) = (start, end) {
            if start > end {
                return Err(ToolError::InvalidArgs(
                    "'start_line' must be less than or equal to 'end_line'".into(),
                ));
            }
        }

        let file = File::open(path).await.map_err(ToolError::Io)?;
        let mut reader = BufReader::new(file).lines();
        let mut selected = Vec::new();
        let mut total = 0usize;
        let mut selected_bytes = 0usize;
        let max_preview_bytes = 100_000usize;

        while let Some(line) = reader.next_line().await.map_err(ToolError::Io)? {
            total += 1;

            let in_range = total >= start.unwrap_or(1) && end.map(|e| total <= e).unwrap_or(true);
            if in_range {
                selected_bytes += line.len();
                selected.push(format!("{:>4} | {}", total, line));
            }

            if end.map(|e| total >= e).unwrap_or(false) && start.is_some() {
                break;
            }

            if start.is_none() && end.is_none() && selected_bytes > max_preview_bytes {
                return Ok(format!(
                    "File too large (over {} bytes, {} lines read so far). Specify start_line/end_line.",
                    max_preview_bytes,
                    total
                ));
            }
        }

        Ok(format!("File: {} ({} lines total)\n{}", path, total, selected.join("\n")))
    }
}
