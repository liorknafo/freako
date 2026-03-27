use async_trait::async_trait;
use serde_json::json;
use super::{Tool, ToolError};

pub struct GlobTool;

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str { "glob" }
    fn description(&self) -> &str { "Find files matching a glob pattern." }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "Glob pattern (e.g. '**/*.rs')" },
                "path": { "type": "string", "description": "Base directory (default: .)" }
            },
            "required": ["pattern"]
        })
    }

    fn requires_approval(&self) -> bool { false }

    async fn execute(&self, args: serde_json::Value) -> Result<String, ToolError> {
        let pattern = args.get("pattern").and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("Missing 'pattern'".into()))?;
        let base = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");

        let full = if pattern.starts_with('/') || pattern.contains(':') {
            pattern.to_string()
        } else {
            format!("{}/{}", base, pattern)
        };

        let pat = full.clone();
        let matches = tokio::task::spawn_blocking(move || {
            glob::glob(&pat)
                .map(|paths| paths.filter_map(|p| p.ok()).map(|p| p.display().to_string()).take(500).collect::<Vec<_>>())
                .unwrap_or_default()
        })
        .await
        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        if matches.is_empty() {
            Ok(format!("No files matching '{}'", full))
        } else {
            Ok(format!("Found {} file(s):\n{}", matches.len(), matches.join("\n")))
        }
    }
}
