use async_trait::async_trait;
use ignore::WalkBuilder;
use regex::Regex;
use serde_json::json;
use super::{Tool, ToolError};

pub struct GrepTool;

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str { "grep" }
    fn description(&self) -> &str { "Regex search across files. Respects .gitignore." }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "Regex pattern" },
                "path": { "type": "string", "description": "Directory to search (default: .)" },
                "include": { "type": "string", "description": "Glob filter (e.g. '*.rs')" }
            },
            "required": ["pattern"]
        })
    }

    fn requires_approval(&self) -> bool { false }

    async fn execute(&self, args: serde_json::Value) -> Result<String, ToolError> {
        let pattern_str = args.get("pattern").and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("Missing 'pattern'".into()))?;
        let search_path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let include = args.get("include").and_then(|v| v.as_str());

        let re = Regex::new(pattern_str)
            .map_err(|e| ToolError::InvalidArgs(format!("Invalid regex: {}", e)))?;

        let path = search_path.to_string();
        let include = include.map(|s| s.to_string());

        let results = tokio::task::spawn_blocking(move || {
            let mut results = Vec::new();
            let mut builder = WalkBuilder::new(&path);
            builder.hidden(false).git_ignore(true);

            if let Some(ref glob) = include {
                let mut types = ignore::types::TypesBuilder::new();
                types.add("custom", glob).ok();
                types.select("custom");
                if let Ok(types) = types.build() {
                    builder.types(types);
                }
            }

            for entry in builder.build().flatten() {
                if !entry.file_type().is_some_and(|ft| ft.is_file()) { continue; }
                if let Ok(content) = std::fs::read_to_string(entry.path()) {
                    for (i, line) in content.lines().enumerate() {
                        if re.is_match(line) {
                            results.push(format!("{}:{}: {}", entry.path().display(), i + 1, line.trim()));
                        }
                    }
                }
                if results.len() > 500 {
                    results.push("... (truncated)".into());
                    break;
                }
            }
            results
        })
        .await
        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        if results.is_empty() {
            Ok(format!("No matches for '{}'", pattern_str))
        } else {
            Ok(results.join("\n"))
        }
    }
}
