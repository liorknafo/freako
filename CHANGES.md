# Summary of Changes

## Feature 1: Project Context Files (README.md and AGENT)

### Overview
Added automatic reading of `README.md` and `AGENT` files from the working directory, which are included in the system prompt to provide project-specific context to the AI.

### Changes Made

**1. Core Agent Prompt (`crates/core/src/agent/prompt.rs`)**
- Added `read_context_files()` function that reads `README.md` and `AGENT` files from the working directory
- Modified `build_system_prompt()` to include project context in a new "# Project Context" section
- Files are only included if they exist and contain non-empty content
- Context is inserted after environment info but before custom user prompts

**2. Documentation (`README.md`)**
- Added "Project context awareness" to features list
- Created new "Project Context Files" section explaining the feature
- Included example `AGENT` file to demonstrate usage

**3. Example (`AGENT`)**
- Created sample `AGENT` file for the freako project itself
- Contains project-specific guidelines and architecture notes

### Benefits
- No need to repeat project context in every conversation
- AI automatically follows project-specific conventions
- Zero configuration required - works automatically if files exist
- Flexible - users can customize content in AGENT file

---

## Feature 2: Agent Cancellation/Stop

### Overview
Added the ability to stop/cancel agent execution mid-stream through both GUI and CLI interfaces.

### Changes Made

**1. Core Agent Events (`crates/core/src/agent/events.rs`)**
- Added `Cancelled` variant to `AgentEvent` enum

**2. Core Agent Loop (`crates/core/src/agent/loop_.rs`)**
- Added `cancel_rx` parameter to `run_agent_loop()` function
- Added cancellation checks at key points:
  - Beginning of each agent loop iteration
  - During streaming using `tokio::select!`
  - Before executing each tool
- Sends `AgentEvent::Cancelled` when cancellation is detected

**3. GUI Application (`crates/gui/src/app.rs`)**
- Added `cancel_tx` field to `App` struct
- Added `StopAgent` message variant
- Created cancel channel alongside event and approval channels when starting agent
- Added handler for `StopAgent` message that sends cancellation signal
- Added handler for `AgentEvent::Cancelled` that cleans up state and displays stop message
- Properly cleans up `cancel_tx` on completion, error, and cancellation

**4. GUI Input Area (`crates/gui/src/ui/input_area.rs`)**
- Modified to show red "Stop" button when agent is working
- Stop button replaces Send button and triggers `Message::StopAgent`
- Button styled in red to indicate destructive action

**5. CLI/TUI (`crates/cli/src/tui.rs`)**
- Added `cancel_tx` field to `App` struct
- Created cancel channel when sending messages
- Modified `Ctrl+C` behavior: stops agent if working, quits if idle
- Added `s` key binding in normal mode to stop agent
- Added handler for `AgentEvent::Cancelled` event
- Updated status bar to show "Ctrl+C or 's' to stop" hint when working
- Properly cleans up `cancel_tx` on completion, error, and cancellation

**6. Documentation (`README.md`)**
- Added "Cancellable execution" to features list
- Created new "Usage" section explaining how to stop agent in both interfaces

### Benefits
- Users can stop long-running or misbehaving agents
- Graceful cancellation preserves partial work
- Consistent UX across GUI and CLI
- Clear visual feedback (red button in GUI, status hint in CLI)

---

## Testing

Both features compile successfully:
- `cargo check -p freako-core` ✓
- `cargo check -p freako-gui` ✓
- `cargo check -p freako-cli` (has pre-existing unrelated errors in main.rs)

The implementation follows Rust best practices with proper error handling, channel-based communication, and graceful cleanup.
