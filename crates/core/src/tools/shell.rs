use async_trait::async_trait;
use serde_json::json;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;

use crate::agent::events::ToolOutputStream;
use crate::config::types::ShellConfig;

use super::{Tool, ToolError};

const MAX_OUTPUT_LEN: usize = 50_000;

pub struct ShellTool {
    config: ShellConfig,
    plan_mode: bool,
}

impl ShellTool {
    pub fn new(config: ShellConfig, plan_mode: bool) -> Self { Self { config, plan_mode } }

    fn validate_plan_mode_command(&self, command: &str) -> Result<(), ToolError> {
        if !self.plan_mode {
            return Ok(());
        }

        let lower = command.to_ascii_lowercase();
        let blocked_patterns = [
            " rm ", "rm ", " del ", "del ", " remove-item", " set-content", " add-content",
            " new-item", " move-item", " copy-item", " rename-item", " git reset", " git clean",
            " cargo fix", " >", ">>", " out-file", " set-itemproperty", " mkdir ", "md ",
        ];

        if blocked_patterns.iter().any(|pattern| lower.contains(pattern)) {
            return Err(ToolError::ExecutionFailed(
                "Shell command is not allowed in plan mode because it may mutate project or system state".into(),
            ));
        }

        Ok(())
    }

    fn build_command(&self, command: &str, working_dir: Option<&str>) -> tokio::process::Command {
        let mut cmd = tokio::process::Command::new(&self.config.command);
        for arg in &self.config.args {
            cmd.arg(arg);
        }
        cmd.arg(command);
        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd
    }

    fn append_bounded(target: &mut String, text: &str) {
        if target.len() >= MAX_OUTPUT_LEN {
            return;
        }
        let remaining = MAX_OUTPUT_LEN - target.len();
        if text.len() <= remaining {
            target.push_str(text);
        } else {
            let mut cut = remaining;
            while !text.is_char_boundary(cut) {
                cut -= 1;
            }
            target.push_str(&text[..cut]);
        }
    }

    fn finalize_output(mut output: String, status: std::process::ExitStatus, was_truncated: bool) -> String {
        if !status.success() {
            if !output.is_empty() && !output.ends_with('\n') {
                output.push('\n');
            }
            output.push_str(&format!("Exit code: {}", status));
        }
        if was_truncated {
            if !output.is_empty() && !output.ends_with('\n') {
                output.push('\n');
            }
            output.push_str("... (truncated)");
        }
        output
    }
}

#[async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &str { "shell" }
    fn description(&self) -> &str { "Execute a shell command and return stdout + stderr." }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "Shell command to execute" },
                "working_dir": { "type": "string", "description": "Optional working directory" }
            },
            "required": ["command"]
        })
    }

    fn requires_approval(&self) -> bool { true }

    async fn execute(&self, args: serde_json::Value) -> Result<String, ToolError> {
        let command = args.get("command").and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("Missing 'command'".into()))?;
        let working_dir = args.get("working_dir").and_then(|v| v.as_str());

        self.validate_plan_mode_command(command)?;

        let timeout = Duration::from_secs(self.config.timeout_secs);
        let output = tokio::time::timeout(timeout, self.build_command(command, working_dir).output())
            .await
            .map_err(|_| ToolError::Timeout)?
            .map_err(ToolError::Io)?;

        let mut result = String::new();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let mut truncated = false;

        if !stdout.is_empty() {
            Self::append_bounded(&mut result, &stdout);
            truncated |= result.len() >= MAX_OUTPUT_LEN;
        }
        if !stderr.is_empty() {
            Self::append_bounded(&mut result, &stderr);
            truncated |= result.len() >= MAX_OUTPUT_LEN;
        }

        Ok(Self::finalize_output(result, output.status, truncated))
    }

    async fn execute_streaming(
        &self,
        args: serde_json::Value,
    ) -> Result<(
        Option<mpsc::UnboundedReceiver<(ToolOutputStream, String)>>,
        tokio::sync::oneshot::Receiver<Result<String, ToolError>>,
    ), ToolError> {
        let command = args.get("command").and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("Missing 'command'".into()))?;
        let working_dir = args.get("working_dir").and_then(|v| v.as_str());

        self.validate_plan_mode_command(command)?;

        let mut child = self.build_command(command, working_dir).spawn().map_err(ToolError::Io)?;
        let stdout = child.stdout.take().ok_or_else(|| ToolError::ExecutionFailed("Failed to capture stdout".into()))?;
        let stderr = child.stderr.take().ok_or_else(|| ToolError::ExecutionFailed("Failed to capture stderr".into()))?;

        let (tx, rx) = mpsc::unbounded_channel();
        let timeout = Duration::from_secs(self.config.timeout_secs);
        let result_rx = tokio::task::spawn(async move {
            let mut stdout = stdout;
            let mut stderr = stderr;
            let mut stdout_buf = [0u8; 4096];
            let mut stderr_buf = [0u8; 4096];
            let mut stdout_closed = false;
            let mut stderr_closed = false;
            let mut output = String::new();
            let mut truncated = false;
            let timer = tokio::time::sleep(timeout);
            tokio::pin!(timer);

            while !stdout_closed || !stderr_closed {
                tokio::select! {
                    _ = &mut timer => {
                        let _ = child.kill().await;
                        return Err(ToolError::Timeout);
                    }
                    read = stdout.read(&mut stdout_buf), if !stdout_closed => {
                        match read {
                            Ok(0) => stdout_closed = true,
                            Ok(n) => {
                                let chunk = String::from_utf8_lossy(&stdout_buf[..n]).to_string();
                                Self::append_bounded(&mut output, &chunk);
                                truncated |= output.len() >= MAX_OUTPUT_LEN;
                                let _ = tx.send((ToolOutputStream::Stdout, chunk));
                            }
                            Err(err) => return Err(ToolError::Io(err)),
                        }
                    }
                    read = stderr.read(&mut stderr_buf), if !stderr_closed => {
                        match read {
                            Ok(0) => stderr_closed = true,
                            Ok(n) => {
                                let chunk = String::from_utf8_lossy(&stderr_buf[..n]).to_string();
                                Self::append_bounded(&mut output, &chunk);
                                truncated |= output.len() >= MAX_OUTPUT_LEN;
                                let _ = tx.send((ToolOutputStream::Stderr, chunk));
                            }
                            Err(err) => return Err(ToolError::Io(err)),
                        }
                    }
                }
            }

            tokio::select! {
                _ = &mut timer => {
                    let _ = child.kill().await;
                    Err(ToolError::Timeout)
                }
                status = child.wait() => {
                    let status = status.map_err(ToolError::Io)?;
                    Ok(Self::finalize_output(output, status, truncated))
                }
            }
        });

        let (result_tx, forwarded_result_rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let final_result = match result_rx.await {
                Ok(result) => result,
                Err(err) => Err(ToolError::ExecutionFailed(format!("Shell streaming task ended unexpectedly: {}", err))),
            };
            let _ = result_tx.send(final_result);
        });

        Ok((Some(rx), forwarded_result_rx))
    }
}
