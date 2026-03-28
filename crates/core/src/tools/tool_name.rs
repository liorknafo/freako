use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::borrow::Cow;
use std::fmt;

pub trait ToolPresentation {
    fn title(&self) -> Cow<'static, str>;
    fn summary(&self) -> String;
}

/// Typed tool call with parsed arguments. Created from raw (name, JSON) at the boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "tool", content = "args")]
pub enum ToolCall {
    #[serde(rename = "read_file")]
    ReadFile {
        path: String,
        #[serde(default)]
        start_line: Option<u64>,
        #[serde(default)]
        end_line: Option<u64>,
    },
    #[serde(rename = "write_file")]
    WriteFile { path: String, content: String },
    #[serde(rename = "edit_file")]
    EditFile {
        path: String,
        old_string: String,
        new_string: String,
    },
    #[serde(rename = "grep")]
    Grep {
        pattern: String,
        #[serde(default)]
        path: Option<String>,
    },
    #[serde(rename = "glob")]
    Glob { pattern: String },
    #[serde(rename = "shell")]
    Shell { command: String },
    #[serde(rename = "list_dir")]
    ListDir {
        #[serde(default = "default_dot")]
        path: String,
    },
    #[serde(rename = "list_skills")]
    ListSkills,
    #[serde(rename = "enter_plan_mode")]
    EnterPlanMode,
    #[serde(rename = "edit_plan")]
    EditPlan {
        #[serde(default)]
        plan: String,
    },
    #[serde(rename = "read_plan")]
    ReadPlan,
    #[serde(rename = "review_plan")]
    ReviewPlan,
    #[serde(rename = "web_search")]
    WebSearch { query: String },
    #[serde(rename = "web_fetch")]
    WebFetch { url: String },
    #[serde(rename = "list_memories")]
    ListMemories {
        #[serde(default = "default_all")]
        scope: String,
    },
    #[serde(rename = "read_memory")]
    ReadMemory { id: String },
    #[serde(rename = "write_memory")]
    WriteMemory {
        title: String,
        content: String,
        #[serde(default)]
        scope: Option<String>,
        #[serde(default)]
        id: Option<String>,
    },
    #[serde(rename = "delete_memory")]
    DeleteMemory { id: String },
    #[serde(rename = "sub_agent")]
    SubAgent { task: String },
}

fn default_dot() -> String { ".".to_string() }
fn default_all() -> String { "all".to_string() }

fn format_directory_summary(path: &str) -> String {
    if path.ends_with('/') || path.ends_with('\\') {
        path.to_string()
    } else if path == "." {
        "./".to_string()
    } else {
        format!("{}/", path)
    }
}

impl ToolPresentation for ToolCall {
    fn title(&self) -> Cow<'static, str> {
        Cow::Borrowed(self.display_name())
    }

    fn summary(&self) -> String {
        match self {
            Self::ReadFile { path, start_line, end_line } => match (start_line, end_line) {
                (Some(s), Some(e)) => format!("{} (lines {}-{})", path, s, e),
                (Some(s), None) => format!("{} (from line {})", path, s),
                (None, Some(e)) => format!("{} (to line {})", path, e),
                (None, None) => path.clone(),
            },
            Self::WriteFile { path, .. } => path.clone(),
            Self::EditFile { path, .. } => path.clone(),
            Self::Shell { command } => truncate(command, 60),
            Self::Grep { pattern, path } => match path {
                Some(p) => format!("/{}/  in {}", truncate(pattern, 30), truncate(p, 30)),
                None => format!("/{}/", truncate(pattern, 40)),
            },
            Self::Glob { pattern } => pattern.clone(),
            Self::ListDir { path } => format_directory_summary(path),
            Self::WebSearch { query } => truncate(query, 50),
            Self::WebFetch { url } => truncate(url, 60),
            Self::ListMemories { scope } => format!("scope: {}", scope),
            Self::ReadMemory { id } => id.clone(),
            Self::WriteMemory { title, .. } => title.clone(),
            Self::DeleteMemory { id } => id.clone(),
            Self::SubAgent { task } => truncate(task, 60),
            Self::ListSkills => "skills".to_string(),
            Self::EnterPlanMode => "switching to plan mode".to_string(),
            Self::EditPlan { .. } => "updating plan".to_string(),
            Self::ReadPlan => "reading plan".to_string(),
            Self::ReviewPlan => "submitting for review".to_string(),
        }
    }
}

impl ToolCall {
    /// The wire name (used in LLM tool calling protocol).
    pub fn name(&self) -> &'static str {
        match self {
            Self::ReadFile { .. } => "read_file",
            Self::WriteFile { .. } => "write_file",
            Self::EditFile { .. } => "edit_file",
            Self::Grep { .. } => "grep",
            Self::Glob { .. } => "glob",
            Self::Shell { .. } => "shell",
            Self::ListDir { .. } => "list_dir",
            Self::ListSkills => "list_skills",
            Self::EnterPlanMode => "enter_plan_mode",
            Self::EditPlan { .. } => "edit_plan",
            Self::ReadPlan => "read_plan",
            Self::ReviewPlan => "review_plan",
            Self::WebSearch { .. } => "web_search",
            Self::WebFetch { .. } => "web_fetch",
            Self::ListMemories { .. } => "list_memories",
            Self::ReadMemory { .. } => "read_memory",
            Self::WriteMemory { .. } => "write_memory",
            Self::DeleteMemory { .. } => "delete_memory",
            Self::SubAgent { .. } => "sub_agent",
        }
    }

    pub fn display_name(&self) -> &'static str {
        self.name()
    }

    /// Parse from raw name + JSON arguments. Returns None for unknown tool names.
    /// Lenient: if argument parsing fails, falls back to defaults where possible.
    pub fn from_raw(name: &str, arguments: &serde_json::Value) -> Option<Self> {
        let result = match name {
            "read_file" => serde_json::from_value::<ReadFileArgs>(arguments.clone())
                .map(|a| Self::ReadFile { path: a.path, start_line: a.start_line, end_line: a.end_line })
                .ok(),
            "write_file" => serde_json::from_value::<WriteFileArgs>(arguments.clone())
                .map(|a| Self::WriteFile { path: a.path, content: a.content })
                .ok(),
            "edit_file" => serde_json::from_value::<EditFileArgs>(arguments.clone())
                .map(|a| Self::EditFile { path: a.path, old_string: a.old_string, new_string: a.new_string })
                .ok(),
            "grep" => serde_json::from_value::<GrepArgs>(arguments.clone())
                .map(|a| Self::Grep { pattern: a.pattern, path: a.path })
                .ok(),
            "glob" => serde_json::from_value::<GlobArgs>(arguments.clone())
                .map(|a| Self::Glob { pattern: a.pattern })
                .ok(),
            "shell" => serde_json::from_value::<ShellArgs>(arguments.clone())
                .map(|a| Self::Shell { command: a.command })
                .ok(),
            "list_dir" => serde_json::from_value::<ListDirArgs>(arguments.clone())
                .map(|a| Self::ListDir { path: a.path })
                .ok()
                .or(Some(Self::ListDir { path: ".".to_string() })),
            "list_skills" => Some(Self::ListSkills),
            "enter_plan_mode" => Some(Self::EnterPlanMode),
            "edit_plan" => serde_json::from_value::<EditPlanArgs>(arguments.clone())
                .map(|a| Self::EditPlan { plan: a.plan })
                .ok()
                .or(Some(Self::EditPlan { plan: String::new() })),
            "read_plan" => Some(Self::ReadPlan),
            "review_plan" => Some(Self::ReviewPlan),
            "web_search" => serde_json::from_value::<WebSearchArgs>(arguments.clone())
                .map(|a| Self::WebSearch { query: a.query })
                .ok(),
            "web_fetch" => serde_json::from_value::<WebFetchArgs>(arguments.clone())
                .map(|a| Self::WebFetch { url: a.url })
                .ok(),
            "list_memories" => serde_json::from_value::<ListMemoriesArgs>(arguments.clone())
                .map(|a| Self::ListMemories { scope: a.scope })
                .ok()
                .or(Some(Self::ListMemories { scope: "all".to_string() })),
            "read_memory" => serde_json::from_value::<ReadMemoryArgs>(arguments.clone())
                .map(|a| Self::ReadMemory { id: a.id })
                .ok(),
            "write_memory" => serde_json::from_value::<WriteMemoryArgs>(arguments.clone())
                .map(|a| Self::WriteMemory { title: a.title, content: a.content, scope: a.scope, id: a.id })
                .ok(),
            "delete_memory" => serde_json::from_value::<DeleteMemoryArgs>(arguments.clone())
                .map(|a| Self::DeleteMemory { id: a.id })
                .ok(),
            "sub_agent" => serde_json::from_value::<SubAgentArgs>(arguments.clone())
                .map(|a| Self::SubAgent { task: a.task })
                .ok(),
            _ => None,
        };
        result
    }

    /// Whether this tool requires user approval.
    pub fn requires_approval(&self) -> bool {
        matches!(self,
            Self::WriteFile { .. }
            | Self::EditFile { .. }
            | Self::Shell { .. }
            | Self::WebSearch { .. }
            | Self::WebFetch { .. }
            | Self::WriteMemory { .. }
            | Self::DeleteMemory { .. }
        )
    }
}

pub fn format_tool_presentation(name: &str, arguments: &Value) -> Option<(Cow<'static, str>, String)> {
    ToolCall::from_raw(name, arguments).map(|tool_call| {
        let title = ToolPresentation::title(&tool_call);
        let summary = ToolPresentation::summary(&tool_call);
        (title, summary)
    })
}

impl fmt::Display for ToolCall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let keep = max.saturating_sub(3) / 2;
    let start: String = s.chars().take(keep).collect();
    let end: String = s.chars().rev().take(keep).collect::<String>().chars().rev().collect();
    format!("{}...{}", start, end)
}

// Private arg structs for serde deserialization
#[derive(Deserialize)] struct ReadFileArgs { path: String, #[serde(default)] start_line: Option<u64>, #[serde(default)] end_line: Option<u64> }
#[derive(Deserialize)] struct WriteFileArgs { path: String, content: String }
#[derive(Deserialize)] struct EditFileArgs { path: String, old_string: String, new_string: String }
#[derive(Deserialize)] struct GrepArgs { pattern: String, #[serde(default)] path: Option<String> }
#[derive(Deserialize)] struct GlobArgs { pattern: String }
#[derive(Deserialize)] struct ShellArgs { command: String }
#[derive(Deserialize)] struct ListDirArgs { #[serde(default = "default_dot")] path: String }
#[derive(Deserialize)] struct EditPlanArgs { #[serde(default)] plan: String }
#[derive(Deserialize)] struct WebSearchArgs { query: String }
#[derive(Deserialize)] struct WebFetchArgs { url: String }
#[derive(Deserialize)] struct ListMemoriesArgs { #[serde(default = "default_all")] scope: String }
#[derive(Deserialize)] struct ReadMemoryArgs { id: String }
#[derive(Deserialize)] struct WriteMemoryArgs { title: String, content: String, #[serde(default)] scope: Option<String>, #[serde(default)] id: Option<String> }
#[derive(Deserialize)] struct DeleteMemoryArgs { id: String }
#[derive(Deserialize)] struct SubAgentArgs { task: String }
