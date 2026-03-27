use async_trait::async_trait;
use serde_json::json;

use crate::config::types::AppConfig;
use crate::skill::{
    SkillStore, discover_skills, format_skill_detail, format_skills_summary,
    sync_working_dir_skills,
};

use super::{Tool, ToolError};

pub struct ListSkillsTool {
    config: AppConfig,
}

impl ListSkillsTool {
    pub fn new(config: AppConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl Tool for ListSkillsTool {
    fn name(&self) -> &str { "list_skills" }

    fn description(&self) -> &str {
        "List discovered skills for a working directory and optionally include full content."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "working_dir": { "type": "string", "description": "Working directory to resolve skills for" },
                "include_content": { "type": "boolean", "description": "Include full skill content (default: false)" }
            },
            "required": ["working_dir"]
        })
    }

    fn requires_approval(&self) -> bool { false }

    async fn execute(&self, args: serde_json::Value) -> Result<String, ToolError> {
        let working_dir = args.get("working_dir").and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("Missing 'working_dir'".into()))?;
        let include_content = args.get("include_content").and_then(|v| v.as_bool()).unwrap_or(false);

        let skills = discover_skills(working_dir, &self.config.skills, &self.config.data_dir)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to discover skills: {}", e)))?;

        let store = SkillStore::open(&self.config.data_dir)
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to open skill store: {}", e)))?;
        sync_working_dir_skills(&store, working_dir, self.config.skills.enabled, &skills)
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to sync skills: {}", e)))?;

        if skills.is_empty() {
            return Ok("No skills are currently available.".into());
        }

        let mut out = format_skills_summary(&skills, working_dir);
        if include_content {
            out.push_str(&format_skill_detail(&skills, working_dir));
        }
        Ok(out)
    }
}
