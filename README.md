# MiniMax CLI

![CI](https://github.com/Hmbown/MiniMax-CLI/actions/workflows/ci.yml/badge.svg)
![crates.io](https://img.shields.io/crates/v/minimax-cli)
![npm](https://img.shields.io/npm/v/@hmbown/minimax-cli)

Unofficial terminal UI (TUI) + CLI for the [MiniMax platform](https://platform.minimax.io): chat with **MiniMax-M2.1**, run an approval-gated tool-using agent, and generate media (images, video, music, TTS).

Highlights:

- Streaming chat via MiniMax’s Anthropic-compatible API format
- Tool-using agent with approvals + workspace sandbox
- Built-in MiniMax media tools (image/video/music/TTS/voice) that save outputs to your workspace
- Skills (`SKILL.md`) and external tools via MCP
- Project-aware prompts via `AGENTS.md` (and `.claude/instructions.md` / `CLAUDE.md`)

Not affiliated with MiniMax Inc.

## Quickstart

1. Get an API key from https://platform.minimax.io
2. Run `minimax` and paste your key when prompted (saved to `~/.minimax/config.toml`), or set `MINIMAX_API_KEY`
3. Press `F1` or run `/help` for the in-app command list

## Install

### Prebuilt (recommended)

The NPM package is a thin wrapper that downloads the platform-appropriate Rust binary from GitHub Releases.

```bash
# npm / bun (installs `minimax`)
npm install -g @hmbown/minimax-cli
bun install -g @hmbown/minimax-cli

### From source (Rust)

```bash
cargo install minimax-cli
```

### Direct download

Download a prebuilt binary from https://github.com/Hmbown/MiniMax-CLI/releases and put it on your `PATH` as `minimax`.

## Usage

```bash
minimax                     # Interactive TUI
minimax -p "Write a haiku"   # One-shot prompt (prints and exits)

minimax doctor               # Diagnose config + API key
minimax sessions             # List auto-saved sessions (~/.minimax/sessions)
minimax --resume <id>        # Resume by ID/prefix (or "latest")
minimax --continue           # Resume most recent session

minimax --workspace /path/to/project
minimax --yolo               # Start in Agent mode + auto-approve tools (dangerous)

minimax init                 # Generate a starter AGENTS.md for the current directory
```

Shell completions:

```bash
minimax completions zsh > _minimax
minimax completions bash > minimax.bash
minimax completions fish > minimax.fish
```

## Configuration

The TUI can save your API key during onboarding. You can also create `~/.minimax/config.toml` manually.

Minimal config:

```toml
api_key = "YOUR_MINIMAX_API_KEY"
default_text_model = "MiniMax-M2.1" # optional
allow_shell = false                # optional
max_subagents = 3                  # optional (1-5)
```

Useful environment variables:

- `MINIMAX_API_KEY` (overrides config)
- `MINIMAX_BASE_URL` (default: `https://api.minimax.io`; China users may use `https://api.minimaxi.com`)
- `MINIMAX_PROFILE` (selects `[profiles.<name>]` from config; errors if missing)
- `MINIMAX_CONFIG_PATH` (override config file path)
- `MINIMAX_ALLOW_SHELL`, `MINIMAX_SKILLS_DIR`, `MINIMAX_MCP_CONFIG`, `MINIMAX_NOTES_PATH`

See `config.example.toml` for a fuller config reference.

## Project Instructions (AGENTS.md)

If your workspace has an `AGENTS.md`, the TUI loads it into the system prompt automatically (and will also look in parent directories up to the git root). Use it to tell the agent how to work in your repo: commands, conventions, and guardrails.

Create a starter file:

- CLI: `minimax init`
- In-app: `/init`

## What You Can Do In The TUI

### Modes

Switch modes with `Tab` or `/mode`:

- **Normal**: chat
- **Edit**: file-focused assistance
- **Agent**: multi-step tool use (with approvals)
- **Plan**: design-first prompting
- **RLM**: load/search/chunk large files in an in-app sandbox

### Slash Commands (high-signal)

The built-in help (`F1` or `/help`) is always up to date. Common commands:

| Command | What it does |
|---|---|
| `/mode [normal|edit|agent|plan|rlm]` | Switch modes |
| `/model [name]` | View/set model name |
| `/skills` | List skills |
| `/skill <name>` | Activate a skill for your next message |
| `/save [path]` | Save current chat to JSON |
| `/load <path>` | Load chat JSON (or load a file into RLM context in RLM mode) |
| `/export [path]` | Export transcript to Markdown |
| `/yolo` | Switch to Agent mode + enable shell tool (still prompts for approval) |
| `/trust` | Allow file access outside workspace |
| `/tokens` | Token totals + metadata |
| `/context` | Context usage estimate |
| `/cost` | Pricing reference for paid tools |
| `/subagents` | Show sub-agent status |

## Tools, Safety, And The Workspace Boundary

MiniMax CLI exposes a tool set to the model (file read/write, patching, web search, sub-agents, and MiniMax media APIs). By default, the TUI asks before running tools with side effects:

- **File writes**: `write_file`, `edit_file`, `apply_patch`
- **Shell**: `exec_shell` (disabled unless `allow_shell=true` or `/yolo` or `--yolo`)
- **Paid/Media**: `generate_image`, `generate_video`, `generate_music`, `tts`, voice tools, file upload/download

The built-in `web_search` tool is backed by DuckDuckGo HTML results and is auto-approved.

File tools are restricted to the `--workspace` directory unless you enable `/trust`.

## Media Generation (MiniMax APIs)

MiniMax CLI includes first-class tools for MiniMax’s media endpoints. In practice: ask for an image/video/music/TTS and the assistant can generate it and save outputs into your workspace.

Built-in MiniMax tool names (for power users): `generate_image`, `generate_video`, `query_video`, `generate_music`, `tts`, `analyze_image`, `voice_clone`, `voice_list`, `voice_delete`, `voice_design`.

## Skills

Skills are reusable workflows stored as `SKILL.md` files inside a directory.

- If your workspace contains `./skills/`, the TUI uses that.
- Otherwise, it falls back to `~/.minimax/skills/`.

Use `/skills` to list and `/skill <name>` to activate.

This repo includes example skills like `video-studio`, `voiceover-studio`, `music-video-generator`, and `audiobook-studio` under `skills/`.

## MCP (External Tool Servers)

MiniMax CLI can load additional tools via MCP (Model Context Protocol). Configure `~/.minimax/mcp.json` (supports `servers` and `mcpServers` keys), then restart the TUI.

For Coding Plan MCP setup, see `docs/coding-plan-integration.md`.

## Documentation

- `docs/ARCHITECTURE.md`
- `docs/coding-plan-integration.md`
- `CONTRIBUTING.md`

## Development

```bash
cargo build
cargo test
cargo fmt
cargo clippy
```

## License

MIT

---

MiniMax is a trademark of MiniMax Inc. This is an unofficial project.
