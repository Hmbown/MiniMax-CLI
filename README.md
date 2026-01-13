# MiniMax CLI

![MiniMax CLI](assets/hero.png)

![CI](https://github.com/Hmbown/MiniMax-CLI/actions/workflows/ci.yml/badge.svg)

Terminal client for [MiniMax M2.1](https://platform.minimax.io). Chat, agent mode, and native media generation.

**Note:** Currently tested with the standard MiniMax API only. Coding Plan MCP is supported, but not fully vetted.

## What's Unique

MiniMax M2.1 has built-in media generation APIs. This CLI exposes them directly:

```bash
# Generate images
/minimax image "a cat wearing a space helmet"

# Generate music
/minimax music "lo-fi beats, rainy day, piano"

# Text-to-speech with voice cloning
/minimax tts "Hello world" --voice custom_voice

# Generate video clips
/minimax video "drone shot of mountains at sunset"
```

These aren't wrappers around other services - they're native MiniMax capabilities included with your API key.

## Install

```bash
# npm / bun
npm install -g @hmbown/minimax-cli
bun install -g @hmbown/minimax-cli

# pip / uv
pip install MiniMax-CLI
uv pip install MiniMax-CLI

# cargo (builds from source)
cargo install minimax-cli
```

Or download prebuilt binaries from [Releases](https://github.com/Hmbown/MiniMax-CLI/releases).

## Usage

```bash
minimax              # Interactive TUI
minimax -p "prompt"  # One-shot mode
minimax --yolo       # Agent mode (shell + file tools)
```

## Configuration

Create `~/.minimax/config.toml`:

```toml
api_key = "your-api-key"
```

Or set `MINIMAX_API_KEY` in your environment.

Get an API key at [platform.minimax.io](https://platform.minimax.io).

For Coding Plan MCP (web search + image understanding), see
[`docs/coding-plan-integration.md`](docs/coding-plan-integration.md).

## Features

- **Chat**: Streaming responses, 204K context, interleaved thinking
- **Agent mode**: File operations, shell execution, MCP tool support
- **Web search**: Built-in DuckDuckGo search tool, plus optional Coding Plan MCP
- **Media generation**: Images, audio, video, music - all native to MiniMax
- **Skills**: Reusable workflows for complex multi-step tasks

## Commands

| Command | Description |
|---------|-------------|
| `/help` | Show help |
| `/mode` | Switch modes (normal/edit/agent/plan) |
| `/model` | Switch models (M2.1 / M2.1-lightning) |
| `/yolo` | Enable agent mode |
| `/skills` | List available skills |
| `/minimax` | Media generation commands |
| `/save` `/load` | Session management |
| `/tokens` | Show usage |

## Skills

Skills are multi-step workflows that combine chat + media tools. Examples:

- `video-studio` - Script, narrate, and render short videos
- `voiceover-studio` - Design voices and produce narration
- `audiobook-studio` - Convert text to multi-chapter audiobooks
- `music-video-generator` - Sync generated music with video

Run with `/skills <name>`. Create your own in `~/.minimax/skills/`.

## Documentation

- [ARCHITECTURE.md](docs/ARCHITECTURE.md)
- [coding-plan-integration.md](docs/coding-plan-integration.md)
- [CONTRIBUTING.md](CONTRIBUTING.md)

## License

MIT

---

*Unofficial project. MiniMax is a trademark of MiniMax Inc.*
