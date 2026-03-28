use crate::memory::store::MemoryStore;
use crate::memory::types::{MemoryEntry, MemoryScope};
use crate::skill::{
    SkillStore, discover_skills, format_skill_detail, format_skills_summary,
    load_skills_for_working_dir, sync_working_dir_skills,
};

pub const SYSTEM_PROMPT: &str = include_str!("prompts/system.txt");
pub const PLAN_MODE_PROMPT: &str = include_str!("prompts/plan_mode.txt");
pub const SUB_AGENT_TOOL_PROMPT: &str = include_str!("prompts/sub_agent_tool.txt");
pub const SUB_AGENT_MODE_PROMPT: &str = include_str!("prompts/sub_agent_mode.txt");
pub const ENTER_PLAN_MODE_TOOL_PROMPT: &str = include_str!("prompts/enter_plan_mode.txt");

/// Build the full system prompt by combining the built-in prompt with
/// the user's custom prompt (if any) and environment info.
pub async fn build_system_prompt(
    custom_prompt: Option<&str>,
    working_dir: &str,
    model_id: &str,
    plan_mode: bool,
    config: &crate::config::types::AppConfig,
) -> String {
    build_system_prompt_inner(custom_prompt, working_dir, model_id, plan_mode, false, config).await
}

/// Build system prompt for sub-agents (no sub_agent tool, no plan mode).
pub async fn build_sub_agent_system_prompt(
    custom_prompt: Option<&str>,
    working_dir: &str,
    model_id: &str,
    config: &crate::config::types::AppConfig,
) -> String {
    build_system_prompt_inner(custom_prompt, working_dir, model_id, false, true, config).await
}

async fn build_system_prompt_inner(
    custom_prompt: Option<&str>,
    working_dir: &str,
    model_id: &str,
    plan_mode: bool,
    is_sub_agent: bool,
    config: &crate::config::types::AppConfig,
) -> String {
    let mut prompt = SYSTEM_PROMPT.to_string();

    if is_sub_agent {
        prompt.push_str(SUB_AGENT_MODE_PROMPT);
    } else {
        // Only the main agent gets the sub_agent tool description
        prompt.push_str(SUB_AGENT_TOOL_PROMPT);

        if plan_mode {
            prompt.push_str(PLAN_MODE_PROMPT);
        } else {
            prompt.push_str(ENTER_PLAN_MODE_TOOL_PROMPT);
        }
    }

    prompt.push_str(&format!(
        "\n# Environment\n- Working directory: {}\n- Model: {}\n- Platform: {}\n",
        working_dir,
        model_id,
        std::env::consts::OS,
    ));

    if let Some(context_info) = refresh_and_read_context_files(working_dir, config).await {
        prompt.push_str("\n# Project Context\n");
        prompt.push_str(&context_info);
        prompt.push('\n');
    }

    if config.memory.enable_memory {
        if let Some(memory_text) = read_memory_context(working_dir, &config.data_dir, config.memory.max_chars) {
            prompt.push_str("\n# Memory Bank\n");
            prompt.push_str(&memory_text);
            prompt.push('\n');
        }
    }

    if let Some(custom) = custom_prompt {
        if !custom.trim().is_empty() {
            prompt.push_str("\n# Additional Instructions\n");
            prompt.push_str(custom);
            prompt.push('\n');
        }
    }

    prompt
}

/// Read README.md, AGENT files, and skills from the working directory if they exist.
pub async fn refresh_and_read_context_files(
    working_dir: &str,
    config: &crate::config::types::AppConfig,
) -> Option<String> {
    let discovered = discover_skills(working_dir, &config.skills, &config.data_dir)
        .await
        .ok()
        .unwrap_or_default();

    let skills = if let Ok(store) = SkillStore::open(&config.data_dir) {
        if sync_working_dir_skills(&store, working_dir, config.skills.enabled, &discovered).is_ok() {
            discovered
        } else {
            load_skills_for_working_dir(&store, working_dir)
                .ok()
                .unwrap_or_default()
        }
    } else {
        discovered
    };

    build_context_text(working_dir, &skills)
}

fn build_context_text(working_dir: &str, skills: &[crate::skill::SkillInfo]) -> Option<String> {
    let mut context = String::new();
    let mut has_content = false;

    // Try to read README.md
    let readme_path = std::path::Path::new(working_dir).join("README.md");
    if let Ok(readme_content) = std::fs::read_to_string(&readme_path) {
        if !readme_content.trim().is_empty() {
            context.push_str("## README.md\n");
            context.push_str(&readme_content);
            context.push_str("\n\n");
            has_content = true;
        }
    }

    // Try to read AGENT file (no extension)
    let agent_path = std::path::Path::new(working_dir).join("AGENT");
    if let Ok(agent_content) = std::fs::read_to_string(&agent_path) {
        if !agent_content.trim().is_empty() {
            context.push_str("## AGENT\n");
            context.push_str(&agent_content);
            context.push_str("\n\n");
            has_content = true;
        }
    }

    if !skills.is_empty() {
        context.push_str(&format_skills_summary(skills, working_dir));
        context.push_str(&format_skill_detail(skills, working_dir));
        has_content = true;
    }

    if has_content {
        Some(context)
    } else {
        None
    }
}

fn read_memory_context(working_dir: &str, data_dir: &std::path::Path, max_chars: usize) -> Option<String> {
    let store = MemoryStore::open(data_dir).ok()?;
    let project_memories = store.list_project_memories(working_dir).ok().unwrap_or_default();
    let global_memories = store.list_global_memories().ok().unwrap_or_default();

    let mut sections = Vec::new();

    if !project_memories.is_empty() {
        sections.push(format_memory_section("PROJECT", &project_memories));
    }
    if !global_memories.is_empty() {
        sections.push(format_memory_section("GLOBAL", &global_memories));
    }

    if sections.is_empty() {
        return None;
    }

    let joined = sections.join("\n\n");
    Some(truncate_chars(&joined, max_chars.max(1)))
}

fn format_memory_section(label: &str, entries: &[MemoryEntry]) -> String {
    let mut out = format!("## MEMORY BANK: {label}\n");
    for entry in entries {
        let scope_label = match entry.scope {
            MemoryScope::Project => "project",
            MemoryScope::Global => "global",
        };
        out.push_str(&format!(
            "- [{scope_label}] {} (id: {})\n{}\n\n",
            entry.title,
            entry.id,
            entry.content.trim()
        ));
    }
    out.trim_end().to_string()
}

fn truncate_chars(input: &str, max_chars: usize) -> String {
    let mut out = input.chars().take(max_chars).collect::<String>();
    if input.chars().count() > max_chars {
        out.push_str("...");
    }
    out
}
