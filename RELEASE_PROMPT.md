# MiniMax CLI - Release Preparation Prompt

## Project Context
You are working on the MiniMax CLI, a professional AI coding assistant built with Rust, featuring:
- **MiniMax API & MiniMax Coding API** dual endpoint support
- **RLM Mode**: Recursive Language Model with context management
- **Duo Mode**: Player-Coach adversarial cooperation for autocoding
- **TUI**: Ratatui-based terminal interface with MiniMax branding

## Current State (BLOCKING ISSUE)

### Critical Build Error
The code has a compilation error in `src/tui/app.rs`:
```
error[E0599]: no method named `todo_summary` found for reference `&App` in the current scope
```

**Root Cause**: Four helper methods were added AFTER the `impl App` block ends (line 830), making them invisible to `ui.rs`:
- `set_process()` - updates current process status
- `add_recent_file()` - tracks recently edited files
- `recent_files_display()` - formats files for display
- `todo_summary()` - summarizes active todos

**The Fix**: These methods MUST be moved INSIDE the `impl App` block, specifically:
- Move them from lines 878-916
- Place them AFTER the `clear_todos` method (around line 827)
- Keep them before the closing `}` of impl App at line 830

### Completed Features (Ready for Integration)
1. **Enhanced CLI branding** in `main.rs`:
   - Modes, Rlm, Duo, Coding subcommands
   - MiniMax M2.5, Coding API, RLM, Duo mode descriptions

2. **Compaction system** in `compaction.rs`:
   - `maybe_compact()` function enabled by default
   - Token estimation with code detection
   - Config: `enabled: true` (changed from false)

3. **Engine caching** in `core/engine.rs`:
   - `cache_system: true` and `cache_tools: true` in EngineConfig
   - Caching helpers for system prompts and tools
   - Compaction check before API calls

4. **Status footer** in `ui.rs`:
   - Dynamic footer height (2 lines when status present)
   - Shows: ⚡ current process, 📋 todos, 📁 recent files
   - Needs properly placed helper methods to work

## Your Tasks (In Priority Order)

### 1. FIX THE BLOCKING COMPILE ERROR ⚠️
```
File: src/tui/app.rs
Action: Move helper methods (set_process, add_recent_file, recent_files_display, todo_summary)
        from AFTER line 830 to INSIDE impl App block (after clear_todos, before line 830)
```

### 2. BUILD & VERIFY
```bash
cargo build 2>&1
```
- Fix any remaining compilation errors
- Ensure all tests pass

### 3. TEST COMPACTION & CACHING
- Verify `cache_control: ephemeral` is being sent for system prompts
- Verify compaction triggers for long conversations (>30k tokens)
- Test that compaction preserves conversation continuity

### 4. TEST STATUS FOOTER
- Run CLI: `cargo run` or `minimax`
- Verify status footer appears when:
  - Process is running (set_process called)
  - Files are edited (add_recent_file called)
  - Todos exist (todo_summary returns content)
- Check visual layout: command area, response area, status footer

### 5. POLISH & RELEASE CHECKLIST
- [ ] All cargo clippy warnings resolved
- [ ] No unwrap() on Result/Option in user-facing paths
- [ ] Error messages are user-friendly
- [ ] Help text is clear and comprehensive
- [ ] MiniMax branding is consistent (colors, icons, terminology)
- [ ] RLM mode loads/saves context correctly
- [ ] Duo mode player-coach interaction works
- [ ] API key configuration works for both endpoints
- [ ] Documentation updated if needed

## Key Files Reference
```
src/
├── main.rs                    # CLI entry point, commands
├── api/
│   ├── mod.rs                # API trait & implementations
│   ├── minimax.rs            # MiniMax API client
│   └── coding.rs             # MiniMax Coding API client
├── core/
│   ├── engine.rs             # Message handling, caching, compaction
│   └── mod.rs
├── compaction.rs             # Auto-compaction logic
├── duo.rs                    # Duo mode implementation
├── rlm/                      # RLM mode module
├── tui/
│   ├── app.rs                # App state, MUST FIX helper methods
│   ├── ui.rs                 # UI rendering, status footer
│   └── mod.rs
└── config.rs                 # Configuration management
```

## Commands to Test
```bash
# Basic usage
cargo run -- "Hello, help me write a Rust function"

# Coding API
cargo run -- coding --help
cargo run -- coding "Create a binary search tree"

# RLM Mode
cargo run -- rlm --help
cargo run -- rlm search "error handling patterns"

# Duo Mode  
cargo run -- duo --help
cargo run -- duo "Write a REST API client"

# Mode info
cargo run -- modes
cargo run -- modes rlm
cargo run -- modes duo

# Check API config
cargo run -- config show
```

## Success Criteria
✅ Code compiles without errors
✅ Status footer displays correctly in TUI
✅ Compaction triggers automatically for long conversations
✅ Caching works (check API payload for cache_control)
✅ All commands are discoverable and documented
✅ MiniMax branding is consistent and professional
✅ No panics or unwrap crashes
✅ Clean exit (Ctrl+C, Ctrl+D, /quit, /exit)

## If You Get Stuck
1. Run `cargo check` to identify specific errors
2. Use `cargo expand --lib minimax` to debug macro expansion
3. Check git status: `git diff src/tui/app.rs` to see recent changes
4. Reference: The helper methods need to be `impl App` methods, not standalone functions

## Important Design Notes
- MiniMax color palette: Use MINIMAX_BLUE_RGB for branding
- Status footer: Show ⚡ for process, 📋 for todos, 📁 for files
- Keep TUI clean: Command area top, response middle, status bottom
- Use MiniMax's iconography: ✨ 🔷 📚 🎯 ⚡ 📋 📁

Good luck! 🚀
