pub mod tool_name;
pub mod file_read;
pub mod file_write;
pub mod file_edit;
pub mod grep;
pub mod glob;
pub mod shell;
pub mod list_dir;
pub mod list_skills;
pub mod enter_plan_mode;
pub mod add_task;
pub mod edit_task;
pub mod read_task;
pub mod update_task_status;
pub mod read_plan;
pub mod review_plan;
pub mod web_search;
pub mod web_fetch;
pub mod list_memories;
pub mod read_memory;
pub mod write_memory;
pub mod delete_memory;
pub mod diff;

use std::collections::HashMap;
use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};

use crate::agent::events::ToolOutputStream;
use crate::config::types::AppConfig;

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid arguments: {0}")]
    InvalidArgs(String),
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Timeout")]
    Timeout,
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> serde_json::Value;
    fn requires_approval(&self) -> bool;
    async fn execute(&self, args: serde_json::Value) -> Result<String, ToolError>;

    /// Execute with streaming output support. Returns a stream receiver immediately
    /// plus a future-like completion channel for the final result.
    async fn execute_streaming(
        &self,
        args: serde_json::Value,
    ) -> Result<(
        Option<mpsc::UnboundedReceiver<(ToolOutputStream, String)>>,
        oneshot::Receiver<Result<String, ToolError>>,
    ), ToolError> {
        let result = self.execute(args).await?;
        let (result_tx, result_rx) = oneshot::channel();
        let _ = result_tx.send(Ok(result));
        Ok((None, result_rx))
    }
}

pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self { tools: HashMap::new() }
    }

    pub fn register(&mut self, tool: Box<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    pub fn all_tools(&self) -> Vec<&dyn Tool> {
        self.tools.values().map(|t| t.as_ref()).collect()
    }

    pub fn default_registry(config: &AppConfig) -> Self {
        let mut registry = Self::new();
        registry.register(Box::new(file_read::FileReadTool));
        registry.register(Box::new(file_write::FileWriteTool));
        registry.register(Box::new(file_edit::FileEditTool));
        registry.register(Box::new(grep::GrepTool));
        registry.register(Box::new(glob::GlobTool));
        registry.register(Box::new(shell::ShellTool::new(config.shell.clone(), false)));
        registry.register(Box::new(list_dir::ListDirTool));
        registry.register(Box::new(list_skills::ListSkillsTool::new(config.clone())));
        registry.register(Box::new(list_memories::ListMemoriesTool {
            data_dir: config.data_dir.clone(),
            working_dir: std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_else(|_| ".".into()),
        }));
        registry.register(Box::new(read_memory::ReadMemoryTool {
            data_dir: config.data_dir.clone(),
        }));
        registry.register(Box::new(write_memory::WriteMemoryTool {
            data_dir: config.data_dir.clone(),
            working_dir: std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_else(|_| ".".into()),
        }));
        registry.register(Box::new(delete_memory::DeleteMemoryTool {
            data_dir: config.data_dir.clone(),
        }));
        registry.register(Box::new(web_search::WebSearchTool::new()));
        registry.register(Box::new(web_fetch::WebFetchTool::new()));
        registry.register(Box::new(enter_plan_mode::EnterPlanModeTool));
        registry.register(Box::new(read_plan::ReadPlanTool));
        registry.register(Box::new(review_plan::ReviewPlanTool));
        registry.register(Box::new(update_task_status::UpdateTaskStatusTool));
        registry
    }

    /// Registry used in plan mode — allows read-only local tools plus
    /// non-mutating research tools like web access and inspection shell commands.
    pub fn plan_registry(config: &AppConfig) -> Self {
        let mut registry = Self::new();
        registry.register(Box::new(file_read::FileReadTool));
        registry.register(Box::new(grep::GrepTool));
        registry.register(Box::new(glob::GlobTool));
        registry.register(Box::new(list_dir::ListDirTool));
        registry.register(Box::new(list_skills::ListSkillsTool::new(config.clone())));
        registry.register(Box::new(list_memories::ListMemoriesTool {
            data_dir: config.data_dir.clone(),
            working_dir: std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_else(|_| ".".into()),
        }));
        registry.register(Box::new(read_memory::ReadMemoryTool {
            data_dir: config.data_dir.clone(),
        }));
        registry.register(Box::new(add_task::AddTaskTool));
        registry.register(Box::new(edit_task::EditTaskTool));
        registry.register(Box::new(read_task::ReadTaskTool));
        registry.register(Box::new(read_plan::ReadPlanTool));
        registry.register(Box::new(review_plan::ReviewPlanTool));
        registry.register(Box::new(web_search::WebSearchTool::new()));
        registry.register(Box::new(web_fetch::WebFetchTool::new()));
        registry.register(Box::new(shell::ShellTool::new(config.shell.clone(), true)));
        registry
    }
}
