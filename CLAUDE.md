# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Run Commands

```bash
# Build all crates
cargo build

# Run GUI (Iced desktop app)
cargo run -p freako-gui

# Run TUI (Ratatui terminal app)
cargo run -p freako-cli

# Release build
cargo build --release

# Check all crates without building
cargo check --workspace

# Run tests
cargo test --workspace
```

Requires Rust **nightly** toolchain (set via `rust-toolchain.toml`). Uses Rust edition 2024.

## Architecture

This is a Rust workspace with 4 crates:

### `crates/core` — Shared library
The core engine used by both frontends. Key modules:

- **`provider/`** — `LLMProvider` async trait with implementations for OpenAI-compatible, Anthropic, AWS Bedrock, and Codex (OAuth). All providers stream responses via `LLMStreamEvent` variants (TextDelta, ToolCallStart/Delta/End, Usage, Done). Factory function `build_provider()` creates the right implementation from config.
- **`agent/loop_.rs`** — Main agent loop. Orchestrates: build LLM request → stream response → accumulate text/tool calls → execute tools (with approval gating) → loop until no more tool calls or cancellation. Communicates with UI via `AgentEvent` channel.
- **`agent/context.rs`** — Builds `LLMRequest` from session messages. Handles context compaction (summarizing old messages when conversation grows large).
- **`tools/`** — `Tool` async trait with `ToolRegistry` (HashMap-based). `default_registry()` for normal mode, `plan_registry()` for read-only plan mode. 20+ tool implementations including file ops, shell, grep, glob, web search/fetch, memory, skills, and plan tools.
- **`config/types.rs`** — `AppConfig` root struct. Loaded from `~/.config/freako/config.toml`. Contains provider, shell, UI, context compaction, memory, skills, auto-approve settings.
- **`session/`** — `Session` and `ConversationMessage` types. `SessionStore` persists to SQLite with JSON-serialized messages.
- **`memory/`** — `MemoryStore` (SQLite) with project/global scoped entries.
- **`skill/`** — Skill discovery from filesystem and remote sources, persisted in SQLite.

### `crates/gui` — Iced desktop GUI
Elm architecture (`boot` → `update` → `view` → `subscription`). Main app in `src/app.rs`. UI components in `src/ui/` (chat_view, input_area, settings_panel, sidebar, approval_dialog, status_bar, theme).

### `crates/cli` — Ratatui TUI
Terminal UI in `src/tui/mod.rs`. Uses crossterm for terminal events. Input modes: Normal, Editing, WaitingApproval.

### `crates/iced-selectable-markdown` — Custom widget
Fork/extension of iced markdown with native text selection support.

## Key Patterns

- **Event-driven communication**: Agent loop emits `AgentEvent`s through channels; both GUI and TUI consume these to update their UI.
- **Approval flow**: Tools declare `requires_approval()`. The agent loop sends approval requests through a channel and blocks until the UI responds. Smart per-file session approval for write/edit operations inside the working directory.
- **Cancellation**: Both frontends can cancel the agent mid-execution via a `CancellationToken`.
- **Plan mode**: Switches tool registry to read-only subset; special tools (EnterPlanMode, EditPlan, ReadPlan, ReviewPlan) manage planning workflow.
- **Message format**: `ConversationMessage` uses `MessagePart` enum that mixes Text, ToolCall, ToolResult, and ToolOutput (streaming shell) within a single message.
- **Streaming**: Provider responses stream token-by-token. Shell tool output also streams via channels for real-time display.

## Rules

- Always run `cargo check --workspace` before declaring any task complete.

## Configuration

Config file: `~/.config/freako/config.toml`
Data (SQLite): stored in platform data directory via `dirs` crate.
Project context: Automatically reads `README.md` and `AGENT` files from working directory into the system prompt.
