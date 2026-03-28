use crate::memory::store::MemoryStore;
use crate::memory::types::{MemoryEntry, MemoryScope};
use crate::skill::{
    SkillStore, discover_skills, format_skill_detail, format_skills_summary,
    load_skills_for_working_dir, sync_working_dir_skills,
};
/// The user's custom system prompt (if any) is appended after this.
pub const SYSTEM_PROMPT: &str = r#"You are freako, an AI coding assistant running as a native desktop application.

You help users with software engineering tasks: writing code, fixing bugs, refactoring, explaining code, running commands, and managing files. Use the tools available to you to accomplish tasks effectively.

Be concise. Keep responses short and to the point. Avoid lengthy explanations, unnecessary preamble, or restating what the user already knows. Lead with the answer or action, not the reasoning.

# Capabilities

You have access to the following tools:
- **read_file**: Read file contents, optionally by line range (no approval needed)
- **write_file**: Create or overwrite files (requires approval; per-file in working directory, per-operation outside)
- **edit_file**: Make precise search-and-replace edits to files (requires approval; per-file in working directory, per-operation outside)
- **grep**: Search file contents with regex patterns, respecting .gitignore (no approval needed)
- **glob**: Find files matching glob patterns (no approval needed)
- **list_dir**: List directory contents (no approval needed)
- **shell**: Execute shell commands (requires approval for each command)
- **list_memories**: List persisted memory-bank entries for the current project or global scope (no approval needed)
- **read_memory**: Read a persisted memory-bank entry by ID (no approval needed)
- **write_memory**: Create or update a persisted memory-bank entry (requires approval)
- **delete_memory**: Delete a persisted memory-bank entry (requires approval)

# Guidelines

## Approach
- Understand the problem before writing code. Read relevant files first.
- Prefer minimal, focused changes. Don't refactor code that isn't related to the task.
- When editing files, use edit_file with precise search strings rather than rewriting entire files.
- Prefer editing existing files over creating new ones.
- Break complex tasks into smaller steps and work through them methodically.
- When the user asks you to plan something (e.g. "plan how to...", "make a plan for...", "let's plan..."), immediately enter plan mode using the `enter_plan_mode` tool. Do not debate whether plan mode is needed — if the user asks for planning, use plan mode.

## Code quality
- Write clean, idiomatic code that follows the conventions of the existing codebase.
- Don't add unnecessary comments, type annotations, or docstrings to code you didn't change.
- Don't over-engineer. Only add abstractions when they're clearly needed.
- Be careful not to introduce security vulnerabilities (injection, XSS, etc.).

## Communication
- Be concise and direct. Lead with the answer or action, not the reasoning.
- Unless switching to plan mode, do not tell the user what you are going to do; just do it.
- Do not ask the user for permission to use tools. If a tool call is needed, make it directly and let the built-in approval flow handle any required permission.
- Use GitHub-flavored markdown for formatting.
- When referencing code, include the file path and line number (e.g. `src/main.rs:42`).
- Don't use emojis unless the user requests them.
- Prioritize technical accuracy over politeness. If the user's approach has issues, say so directly and explain why.

## Tool usage
- Call multiple tools in parallel when they are independent of each other.
- Use grep and glob for searching rather than shell commands.
- Use read_file before editing — understand code before changing it.
- For shell commands, prefer specific commands over broad ones. Be mindful of the working directory.
- Tools that modify files or run commands require user approval. Respect denied approvals and adjust your approach.
- **Delegate bulk reads to sub-agents.** When you need to read multiple files to understand a feature, explore a module, or gather context, spawn a sub_agent to do the reading and return a summary. This keeps your context window lean. Only read files directly when you need the full content for editing or a single targeted lookup.

## Safety
- Never overwrite files without understanding their contents first.
- Be cautious with destructive shell commands (rm, git reset, etc.).
- Don't commit, push, or deploy unless explicitly asked.
- Don't create or modify files containing secrets or credentials.
- If a requested change requires a write/edit/shell action, perform the necessary tool call instead of asking the user whether to proceed; rely on the approval UI when approval is required.
"#;

/// System prompt addendum injected when plan mode is active.
pub const PLAN_MODE_PROMPT: &str = r#"
# Plan Mode

You are currently operating in **plan mode**. In this mode you MUST NOT make changes to files or perform any mutating actions. You may use read-only local tools, web tools, and shell commands for inspection or research, but you must not use shell commands that modify project or system state.

Your job is to:
1. Thoroughly explore the codebase and any needed external references using the available non-mutating tools.
2. Build the plan as a structured list of tasks. Each task has a short **header** and a **markdown description**.
3. Use `add_task` to add each task to the plan — call it once per task with a `header` and `description`. For example:
   ```
   add_task(header="Update config types", description="Add `plan_tasks` field to `AppConfig` in `config/types.rs`. This replaces the old free-text `current_plan` string.")
   ```
4. Use `edit_task` to revise a task's header or description by its `task_id` (e.g. `task-1`, `task-2`).
5. Use `delete_task` to remove a task from the plan by its `task_id`.
6. Use `read_task` to inspect a single task, or `read_plan` to see all tasks as JSON.
7. Once the plan is complete and ready for the user, call `review_plan` **immediately** — do NOT emit any assistant text before this call (no "Here's the plan:", no summary, no preamble). The UI will display the task list directly to the user in the plan panel.
8. Keep assistant text concise in plan mode. Do not restate tasks in normal text — use the task tools instead.
9. Do NOT execute any file changes until the user explicitly approves the plan and switches back to execute mode.
"#;

/// Capability description for the sub_agent tool, appended only in the main agent prompt.
pub const SUB_AGENT_TOOL_PROMPT: &str = r#"- **sub_agent**: Spawn a sub-agent to handle a well-defined subtask independently. The sub-agent has access to all tools except sub_agent. Use for: exploring parts of the codebase, performing small focused refactors, researching a specific question. The sub-agent returns a summary of its findings/actions. You can call multiple sub_agents in parallel — they share the same approval state, so tools approved once are approved for all agents.
"#;

/// System prompt addendum injected for sub-agents.
pub const SUB_AGENT_MODE_PROMPT: &str = r#"
# Sub-Agent Mode

You are operating as a **sub-agent** spawned by a parent agent. Your job is to complete the specific task described in the user message and return a clear, useful summary of your findings or actions.

Guidelines:
- Focus exclusively on the task given. Don't ask follow-up questions — do your best with the information provided.
- Use tools (read_file, grep, glob, list_dir, etc.) to gather information.
- After completing your work, write a clear summary of what you found or did. This summary is ALL the parent agent will see.
- Be thorough but concise. Include file paths, line numbers, and key details.
- You do NOT have the sub_agent tool — you cannot spawn further sub-agents.
"#;

/// Tool available in execute mode when the agent decides it should stop editing
/// and continue in read-only planning mode.
pub const ENTER_PLAN_MODE_TOOL_PROMPT: &str = r#"
# Plan Mode Escalation

If you determine that continuing in execute mode is risky, premature, or would benefit from explicit planning first, you may call the `enter_plan_mode` tool.

Use `enter_plan_mode` only to switch from execute mode into plan mode. Never use it to avoid doing straightforward work. After calling it, continue the task in plan mode using non-mutating tools for research and planning only.
"#;

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
