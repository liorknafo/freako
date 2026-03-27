# freako

A native desktop AI code assistant built in Rust. Supports **OpenAI-compatible**, **Anthropic**, and **AWS Bedrock** providers with configurable GUI and TUI interfaces.

> Edit tool diff rendering test marker.

## Features

- **Multi-provider** - OpenAI-compatible, Anthropic (direct API), AWS Bedrock (Converse API)
- **Agentic tool calling** - File read/write/edit, grep, glob, shell execution
- **Streaming responses** - Real-time token-by-token output with markdown rendering
- **Tool approval flow** - Approve once, approve-for-session, or deny risky actions
- **Cancellable execution** - Stop agent mid-execution with a button (GUI) or Ctrl+C/`s` key (TUI)
- **Two frontends** - Iced GUI (`freako-gui`) and Ratatui TUI (`freako`)
- **In-app settings** - Configure provider, model, and preferences from the GUI
- **Session persistence** - Conversations saved to local SQLite database
- **Project context awareness** - Automatically reads `README.md` and `AGENT` files from working directory

## Built-in Tools

| Tool | Description | Requires Approval |
|------|-------------|-------------------|
| `read_file` | Read file contents with optional line range | No |
| `write_file` | Create or overwrite files | Yes* |
| `edit_file` | Search-and-replace edits within files | Yes* |
| `grep` | Regex search across files (.gitignore aware) | No |
| `glob` | Find files matching a pattern | No |
| `list_dir` | List directory contents | No |
| `shell` | Execute shell commands | Yes |

**\* Smart Approval:** File operations in your working directory require approval once per file (when you choose "Approve for Session"). Operations outside the working directory and shell commands require approval for each operation.

## Building

### Prerequisites

- Rust nightly toolchain (managed via `rust-toolchain.toml`)
- On Linux: `libxkbcommon-dev`, `libvulkan-dev` (for Iced/wgpu)

### Build & Run

```bash
# GUI (Iced)
cargo run -p freako-gui

# TUI (Ratatui)
cargo run -p freako-cli

# Release build
cargo build --release
```

### CLI Options (TUI)

```
freako [OPTIONS]

Options:
  --working-dir <PATH>   Working directory (default: .)
  --model <MODEL>        Model name override
  --api-key <KEY>        API key override
  --api-base <URL>       API base URL override
  -h, --help             Print help
```

## Usage

### Stopping Agent Execution

You can cancel the agent at any time:

- **GUI**: Click the red "Stop" button (replaces the Send button when agent is working)
- **TUI**: Press `Ctrl+C` or `s` key in normal mode (press `Esc` first if in editing mode)

When stopped, the agent will gracefully terminate and save any partial response to the conversation history.

### Tool Approval System

freako uses a smart approval system to keep you in control while minimizing interruptions:

**No Approval Needed:**
- Reading files (`read_file`)
- Searching files (`grep`, `glob`)
- Listing directories (`list_dir`)

**Smart Approval (File Operations):**
- **Inside working directory**: When you choose "Approve for Session" for `write_file` or `edit_file`, that specific file is approved for the rest of the session. The agent can make multiple edits without asking again.
- **Outside working directory**: Each operation requires individual approval for safety.

**Approval Options:**
- **Approve Once**: Approve this single operation
- **Approve for Session**: Approve all operations on this file (or this specific command) for the current session
- **Always Approve**: Add this tool to your config's auto-approve list - never ask again for any use of this tool

**Shell Commands:**
- Each shell command requires approval by default
- Use "Always Approve" to auto-approve the `shell` tool if you trust the agent completely

**Example:** If the agent edits `src/main.rs` and you approve for session, it can continue editing that file. But if it tries to edit `src/utils.rs`, you'll be asked again (but only once per file).

## Project Context Files

freako automatically reads and includes content from the following files in your working directory to provide better context to the AI:

- **README.md** - Project overview, setup instructions, and documentation
- **AGENT** - Project-specific guidelines and instructions for the AI assistant

These files are read once at the start of each session and included in the system prompt. This helps the AI understand your project structure, conventions, and specific requirements without you having to explain them in every conversation.

Example `AGENT` file:
```
# Agent Instructions

## Project Guidelines
- Use TypeScript strict mode
- Follow the existing code style in src/
- Run tests before committing

## Architecture
- Frontend: React + Vite
- Backend: Express + PostgreSQL
- API routes are in src/api/
```


## Configuration

All configuration is managed through the GUI settings panel or by editing `~/.config/freako/config.toml`:

```toml
[provider]
provider_type = "openai"
api_base = "https://api.openai.com/v1"
api_key = "sk-..."
model = "gpt-4o"
max_tokens = 4096
temperature = 0.7

[shell]
command = "bash"
args = ["-l", "-c"]
timeout_secs = 120

[ui]
theme = "dark"
font_size = 14.0
window_width = 1200
window_height = 800

[context]
enable_compaction = true
compact_after_messages = 40
keep_recent_messages = 12
```

## Architecture

```
freako/
├── crates/
│   ├── core/          # Provider abstraction, agent loop, tools, config, session
│   ├── gui/           # Iced desktop GUI
│   └── cli/           # Ratatui terminal UI
```

## License

MIT
