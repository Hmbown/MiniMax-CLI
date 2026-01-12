# MiniMax CLI Architecture

This document provides an overview of the MiniMax CLI architecture for developers and contributors.

## High-Level Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                         User Interface                          │
│  ┌─────────────────┐  ┌─────────────────┐  ┌────────────────┐  │
│  │   TUI (ratatui) │  │  One-shot Mode  │  │  Config/CLI    │  │
│  └────────┬────────┘  └────────┬────────┘  └────────┬───────┘  │
└───────────┼─────────────────────┼────────────────────┼──────────┘
            │                     │                    │
            ▼                     ▼                    ▼
┌─────────────────────────────────────────────────────────────────┐
│                        Core Engine                              │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                    Agent Loop (core/engine.rs)           │   │
│  │  ┌─────────┐  ┌─────────────┐  ┌──────────────────────┐ │   │
│  │  │ Session │  │ Turn Mgmt   │  │ Tool Orchestration   │ │   │
│  │  └─────────┘  └─────────────┘  └──────────────────────┘ │   │
│  └─────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
            │                     │                    │
            ▼                     ▼                    ▼
┌─────────────────────────────────────────────────────────────────┐
│                     Tool & Extension Layer                      │
│  ┌──────────┐  ┌──────────┐  ┌─────────┐  ┌────────────────┐   │
│  │  Tools   │  │  Skills  │  │  Hooks  │  │  MCP Servers   │   │
│  │ (shell,  │  │ (plugins)│  │ (pre/   │  │  (external)    │   │
│  │  file)   │  │          │  │  post)  │  │                │   │
│  └──────────┘  └──────────┘  └─────────┘  └────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
            │                     │                    │
            ▼                     ▼                    ▼
┌─────────────────────────────────────────────────────────────────┐
│                        LLM Layer                                │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │              LLM Client Abstraction (llm_client.rs)       │  │
│  │  ┌─────────────────┐  ┌─────────────────────────────┐    │  │
│  │  │  MiniMax Client │  │  Anthropic Client (Claude)  │    │  │
│  │  │   (client.rs)   │  │       (client.rs)           │    │  │
│  │  └─────────────────┘  └─────────────────────────────┘    │  │
│  └──────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

## Module Organization

### Entry Point

- **`main.rs`** - CLI argument parsing (clap), configuration loading, entry point routing

### Core Components

- **`core/`** - Main engine components
  - `engine.rs` - Agent loop, message processing, tool execution orchestration
  - `session.rs` - Session state management
  - `turn.rs` - Turn-based conversation handling
  - `events.rs` - Event system for UI updates
  - `ops.rs` - Core operations

### Configuration

- **`config.rs`** - Configuration loading, profiles, environment variables
- **`settings.rs`** - Runtime settings management

### LLM Integration

- **`client.rs`** - HTTP client for Anthropic-compatible API (MiniMax `/anthropic` and Claude)
- **`llm_client.rs`** - Abstract LLM client trait with retry logic
- **`models.rs`** - Data structures for API requests/responses

#### MiniMax API Endpoints

MiniMax provides two API formats:
- `https://api.minimax.io/anthropic` - Anthropic-compatible (recommended by MiniMax, used by engine)
- `https://api.minimax.io/v1` - OpenAI-compatible (not used by the engine)

The engine uses `handle_anthropic_turn()` for chat models to keep MiniMax and Claude on the
same Anthropic-compatible request/response format.

### Tool System

- **`tools/`** - Built-in tool implementations
  - `mod.rs` - Tool registry and common types
  - `shell.rs` - Shell command execution
  - `file.rs` - File read/write operations
  - `todo.rs` - Todo list management
  - `plan.rs` - Planning tools
  - `subagent.rs` - Sub-agent spawning
  - `spec.rs` - Tool specifications

### Extension Systems

- **`mcp.rs`** - Model Context Protocol client for external tool servers
- **`skills.rs`** - Plugin/skill loading and execution
- **`hooks.rs`** - Pre/post execution hooks with conditions

### User Interface

- **`tui/`** - Terminal UI components (ratatui-based)
  - `app.rs` - Application state and message handling
  - `ui.rs` - Rendering logic
  - `approval.rs` - Tool approval dialog
  - `clipboard.rs` - Clipboard handling
  - `streaming.rs` - Streaming text collector

- **`ui.rs`** - Legacy/simple UI utilities

### Security

- **`sandbox/`** - macOS sandboxing support
  - `mod.rs` - Sandbox type definitions
  - `policy.rs` - Sandbox policy configuration
  - `seatbelt.rs` - macOS Seatbelt profile generation

### Utilities

- **`utils.rs`** - Common utilities
- **`logging.rs`** - Logging infrastructure
- **`compaction.rs`** - Context compaction for long conversations
- **`rlm.rs`** - Reflection/reasoning utilities
- **`pricing.rs`** - Cost estimation
- **`prompts.rs`** - System prompt templates
- **`project_doc.rs`** - Project documentation handling
- **`session.rs`** - Session serialization

## Data Flow

### Interactive Session

1. User input received in TUI
2. Input processed by `core/engine.rs`
3. Message sent to LLM via `llm_client.rs`
4. Response streamed back, parsed in `client.rs`
5. Tool calls extracted and executed via `tools/`
6. Hooks triggered before/after tool execution
7. Results aggregated and sent back to LLM
8. Final response rendered in TUI

### Tool Execution

1. LLM requests tool via `tool_use` content block
2. Tool registry looks up handler
3. Pre-execution hooks run
4. Approval requested if needed (non-yolo mode)
5. Tool executed (possibly sandboxed on macOS)
6. Post-execution hooks run
7. Result returned to agent loop

## Extension Points

### Adding a New Tool

1. Create handler in `tools/`
2. Register in `tools/registry.rs`
3. Add tool specification (name, description, input schema)

### Adding an MCP Server

1. Configure in `~/.minimax/mcp.json`
2. Server auto-discovered at startup
3. Tools exposed to LLM automatically

### Creating a Skill

1. Create skill directory with `SKILL.md`
2. Define skill prompt and optional scripts
3. Place in `~/.minimax/skills/`

### Adding Hooks

Configure in `~/.minimax/config.toml`:

```toml
[[hooks]]
event = "tool_call_before"
command = "echo 'Running tool: $TOOL_NAME'"
```

## Key Design Decisions

1. **Streaming-first**: All LLM responses stream for responsiveness
2. **Tool safety**: Non-yolo mode requires approval for destructive operations
3. **Extensibility**: MCP, skills, and hooks allow customization without code changes
4. **Cross-platform**: Core works on Linux/macOS/Windows, sandboxing macOS-only
5. **Minimal dependencies**: Careful dependency selection for build speed

## Configuration Files

- `~/.minimax/config.toml` - Main configuration
- `~/.minimax/mcp.json` - MCP server configuration
- `~/.minimax/skills/` - User skills directory
- `~/.minimax/sessions/` - Session history
