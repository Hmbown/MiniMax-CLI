# MiniMax CLI Improvements Summary

This document summarizes the improvements made to bring MiniMax CLI to the level of kimi-cli, codex, and claude code.

## Phase 1: Enhanced Slash Commands ✅

### New Commands Added

- **`/debug`** - Shows comprehensive debug information including:
  - Session info (ID, messages, tokens, cost)
  - Context usage (window size, estimated tokens, usage percentage)
  - Settings (auto-compact, show thinking, shell permissions)
  - Workspace info and recent files
  - RLM session status (when in RLM mode)
  - System prompt preview

- **`/reload`** - Reloads configuration from disk without restarting:
  - Reloads `~/.minimax/config.toml`
  - Reloads persistent settings
  - Applies changes to current session

- **`/usage`** - Shows API usage and quota information:
  - Current session token and cost estimates
  - MiniMax media pricing reference
  - Tips for managing quota

### Enhanced Commands

- **`/compact`** - Now supports manual compaction:
  - `/compact` - Toggle auto-compact on/off
  - `/compact now` - Trigger immediate context compaction

## Phase 2: Shell Mode (Ctrl-X) ✅

### Features
- **Ctrl-X** toggles between Agent mode and Shell mode
- When in Shell mode:
  - Input is executed as shell commands directly
  - Header shows "SHELL" badge (green)
  - Commands run in the workspace directory
  - Commands are safety-analyzed (dangerous patterns are blocked)
  - Commands run through the existing sandbox manager when available
  - Output displayed in chat transcript
  - Supports both stdout and stderr
  - Shows exit codes

### Implementation Details
- Added `shell_mode` flag to App state
- Added `toggle_shell_mode()` method
- Added `execute_shell_command()` async function
- Reused existing command safety analysis and sandboxed shell execution
- Header widget updated to show SHELL mode status
- Works on macOS, Linux, and Windows

## Phase 3: Context Management ✅

### Manual Compaction
- Added `Op::CompactContext` operation
- Engine handles compaction via existing compaction system
- Summarizes older messages to reduce token usage
- Preserves recent messages (last 4)
- Merges with existing system prompt
- Persists compacted context in the engine session
- Syncs compacted context back to the UI context meter

## Phase 4: Usage Tracking ✅

### Session Cost Tracking
- Already existed but enhanced with `/usage` command
- Tracks media generation costs (images, video, music, TTS)
- Shows pricing reference for MiniMax APIs

## Phase 6: Keyboard Shortcuts ✅

### New Shortcuts
- **Ctrl-X** - Toggle shell mode
- **Ctrl-J** - Insert newline (multiline input)
- Existing: **Ctrl-C** - Exit, **Ctrl-V** - Paste

### Updated Help
- Help view (`/help` or F1) now documents new shortcuts
- Shows both old and new keyboard commands

## Phase 9: Status Bar Enhancements ✅

### Header Improvements
- Shows mode badge (changes to SHELL when in shell mode)
- Displays model name
- Shows context usage meter with percentage
- Color-coded usage indicator (green → yellow → red)
- Streaming indicator when active

## Technical Changes

### New Files
- `src/commands/reload.rs` - Config reload command
- `src/commands/usage.rs` - Usage tracking command

### Modified Files
- `src/commands/mod.rs` - Added new commands to registry
- `src/commands/debug.rs` - Added `debug_info()` function
- `src/commands/session.rs` - Enhanced `compact()` command
- `src/tui/app.rs` - Added shell_mode field and toggle method
- `src/tui/ui.rs` - Added Ctrl-X, Ctrl-J handlers and shell execution
- `src/tui/widgets/header.rs` - Added shell_mode display
- `src/tui/views/mod.rs` - Updated help text
- `src/core/ops.rs` - Added `CompactContext` operation
- `src/core/engine.rs` - Added compaction handling
- `src/rlm.rs` - Added helper methods for debug info

## Phase 11: Interactive Setup Wizard ✅

### `/setup` Command
- Interactive wizard for first-time configuration
- Steps:
  1. Welcome message
  2. Region selection (US or China base URL)
  3. API key input with validation
  4. Default model selection (MiniMax-M2.1, MiniMax-Text-01, MiniMax-Coding-01)
  5. Default mode selection (Normal, Agent, YOLO)
  6. Shell permissions (yes/no)
  7. Summary and confirmation
- Saves to `~/.minimax/config.toml`
- Automatically reloads configuration

## Phase 12: YOLO Mode Toggle ✅

### `/yolo` Command
- Toggles YOLO mode on/off (previously only enabled)
- Shows warning when enabling with ⚠️ emoji
- Returns to Normal mode when disabling
- Shows current status when called without arguments

## Phase 13: Hard Reset Command ✅

### `/reset` Command
- Comprehensive session reset
- Clears: conversation history, todos, plan state, queued messages
- Resets: context/compaction state, recent files, token tracking
- More thorough than `/clear`

## Phase 14: Auto-Compaction Triggering ✅

### Automatic Context Compaction
- Triggers automatically when auto_compact is enabled
- Thresholds: 80,000 tokens OR 30 messages
- Shows status message when compaction occurs
- Uses existing compaction infrastructure
- Configurable via `/set auto_compact true --save`

## Phase 15: Input History Persistence ✅

### Persistent Command History
- Saves to `~/.config/minimax/input_history.txt`
- Loads on startup
- Filters out slash commands and duplicates
- Configurable max history size

## Phase 16: MCP Status Command ✅

### `/mcp` Command
- Shows connected MCP servers
- Displays connection status (✓ connected, ✗ error, ○ disabled)
- Lists tools provided by each server
- Shows server configuration

## Phase 17: Session Management Commands ✅

### `/sessions` and `/resume` Commands
- Interactive session picker
- Lists all saved sessions with IDs and timestamps
- Shows currently active session
- Fuzzy search for filtering sessions
- Switch between sessions without restarting

## Phase 18: Interactive Model Picker ✅

### Enhanced `/model` Command
- Interactive picker when called without arguments
- Shows available MiniMax models with descriptions:
  - MiniMax-M2.1 (general purpose)
  - MiniMax-Text-01 (long context)
  - MiniMax-Coding-01 (code generation)
- Validates model names when called with arguments
- Persists selection to settings

## Testing

All 296 tests pass:
```
test result: ok. 296 passed; 0 failed; 0 ignored
```

## Build

Release build successful:
```
Finished `release` profile [optimized]
```

## Usage Examples

```bash
# Start MiniMax CLI
minimax

# Toggle shell mode (Ctrl-X)
# Then type shell commands directly
$ ls -la
$ git status
$ npm test

# View debug information
/debug

# Reload configuration
/reload

# Check API usage
/usage

# Trigger manual compaction
/compact now
```

## Key Improvements Summary

| Feature | kimi-cli | codex | claude code | minimax-cli (now) |
|---------|----------|-------|-------------|-------------------|
| Shell Mode (Ctrl-X) | ✅ | ❌ | ❌ | ✅ |
| /debug command | ✅ | ❌ | ❌ | ✅ |
| /reload command | ✅ | ❌ | ❌ | ✅ |
| /usage command | ✅ | ❌ | ❌ | ✅ |
| Manual /compact | ✅ | ❌ | ❌ | ✅ |
| Auto-compaction | ✅ | ❌ | ❌ | ✅ |
| Ctrl-J multiline | ✅ | ❌ | ❌ | ✅ |
| Context meter | ✅ | ✅ | ✅ | ✅ |
| Session management | ✅ | ✅ | ✅ | ✅ |
| Skills system | ✅ | ❌ | ❌ | ✅ |
| MCP support | ✅ | ❌ | ✅ | ✅ |
| /mcp status | ✅ | ❌ | ❌ | ✅ |
| /sessions | ✅ | ✅ | ✅ | ✅ |
| /resume | ✅ | ❌ | ❌ | ✅ |
| /setup wizard | ✅ | ❌ | ❌ | ✅ |
| /yolo toggle | ✅ | ❌ | ❌ | ✅ |
| /reset | ✅ | ❌ | ❌ | ✅ |
| Interactive /model | ✅ | ❌ | ❌ | ✅ |
| Input history persistence | ✅ | ✅ | ❌ | ✅ |
| RLM mode | ❌ | ❌ | ❌ | ✅ (unique) |
| Duo mode | ❌ | ❌ | ❌ | ✅ (unique) |
| Media generation | ❌ | ❌ | ❌ | ✅ (unique) |
