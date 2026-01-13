# MiniMax CLI

![MiniMax CLI hero](assets/hero.png)

**Terminal-native agent for MiniMax M2.1** — tools, skills, media generation, and lightning-fast workflows.

> ⚡ **Powered by MiniMax M2.1** with 204K context, interleaved thinking, and first-class media tools

![CI](https://github.com/Hmbown/MiniMax-CLI/actions/workflows/ci.yml/badge.svg)

---

## ✨ What You Can Do

### 💬 Rich Terminal Conversations
```
> minimax
🤖 MiniMax M2.1 ready
You: Explain how neural networks learn through backpropagation
[MiniMax thinking...]
🤖 Here’s a clear explanation...
```

### 🎨 Generate Media on the Fly
```bash
# Generate images from text
/minimax image "A cyberpunk city at sunset, neon lights"

# Create music from descriptions
/minimax music "Upbeat lo-fi beats for coding"

# Synthesize speech with custom voices
/minimax tts "Hello, this is MiniMax speaking"

# Generate short videos
/minimax video "A drone shot of mountains at golden hour"
```

### 🛠️ Build with Agent Mode
```bash
minimax --yolo  # Enables shell and file tools
You: Create a Python web scraper for product prices
[MiniMax planning, executing, iterating...]
✅ scraper.py created and tested
```

---

## 🚀 Quickstart

### Install with Cargo

```bash
cargo install minimax-cli
```

### Or Build from Source

```bash
git clone https://github.com/Hmbown/MiniMax-CLI
cd MiniMax-CLI
cargo install --path .
```

### Download Binaries

Prebuilt binaries available at:
👉 https://github.com/Hmbown/MiniMax-CLI/releases

### Run It

```bash
# Interactive TUI with full MiniMax experience
minimax

# One-shot prompt (great for quick tasks)
minimax -p "Summarize this article"

# Agent mode — enables tools + shell execution
minimax --yolo
```

---

## 🎯 Why MiniMax CLI?

| Feature | Description |
|---------|-------------|
| **⚡ MiniMax-Native** | Built specifically for M2.1 with full support for interleaved thinking, 204K context windows, and streaming responses |
| **🎨 Media Tools** | First-class image, audio, video, and music generation directly from your terminal |
| **🧠 Agent Mode** | Capable agent with file operations, shell execution, and MCP tool support |
| **🎭 Skills System** | Reusable, shareable workflows for common tasks |
| **🎨 Beautiful TUI** | Expressive terminal interface with clean tool call cards and MiniMax thinking animations |

### TUI Features

- **Esc/Ctrl+C** — Interrupt running requests instantly
- **Token meter** — Real-time usage in footer + `/tokens` command
- **Clean summaries** — Tool calls displayed as elegant cards, not noisy dumps
- **Thinking animation** — Rotating labels show MiniMax is thinking

### Built-in Commands

| Command | Description |
|---------|-------------|
| `/help` | Show help or command details |
| `/mode` | Switch modes (normal/edit/agent/plan) |
| `/model` | Switch between M2.1 and M2.1-lightning |
| `/yolo` | Enable agent mode with shell execution |
| `/minimax` | Show dashboard and docs links |
| `/skills` | List and activate available skills |
| `/save` / `/load` | Save and restore sessions |
| `/tokens` | Show token usage and costs |
| `/context` | Display context window utilization |
| `/retry` | Retry the last request |

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Enter` | Send message |
| `Esc` | Cancel request |
| `Ctrl+C` | Exit application |
| `F1` | Help |
| `↑/↓` | Navigate command history |

---

## 🧠 MiniMax Models

| Model | Context | Best For |
|-------|---------|----------|
| **MiniMax-M2.1** | 204K tokens | Complex reasoning, coding, long documents |
| **MiniMax-M2.1-lightning** | 204K tokens | Faster responses, same capabilities |

Both models support **interleaved thinking** — MiniMax can show its work as it thinks, making reasoning transparent and verifiable.

---

## ⚙️ Configuration

Create your config at `~/.minimax/config.toml`:

```toml
# Required: your MiniMax API key
api_key = "your-api-key"

# Optional: default model (defaults to MiniMax-M2.1)
default_text_model = "MiniMax-M2.1"

# Optional: override base URL
# base_url = "https://api.minimax.io"
```

Or use environment variables:

```bash
export MINIMAX_API_KEY=your-api-key
export MINIMAX_BASE_URL=https://api.minimax.io  # Optional
```

**Get your MiniMax API key:** 👉 https://platform.minimax.io

---

## 🎓 Skills System

Skills are reusable, shareable workflows for common creative tasks. Each skill combines multiple MiniMax tools into a guided production pipeline.

### 🎬 Content Creation Skills

| Skill | Description | Best For |
|-------|-------------|----------|
| `video-studio` | Build custom short videos with script, narration, music, and poster frames | Social media, YouTube intros, product demos |
| `music-video-generator` | Create synchronized music and matching video from a unified prompt | Artist promos, lyric videos, visual albums |
| `social-media-kit` | Generate vertical TikTok/Reels videos with captions, trending music, and hooks | Viral content, brand awareness, influencer marketing |
| `trailer-teaser-generator` | Produce cinematic trailers with voiceover, tension music, and dramatic sequences | Movie/Game/book launches, product reveals |

### 🎵 Audio & Voice Skills

| Skill | Description | Best For |
|-------|-------------|----------|
| `voice-podcast-kit` | Create multi-host podcasts with cloned voices, intro/outro music, and transitions | Interview shows, narrative podcasts, branded audio |
| `voiceover-studio` | Design custom voices from text prompts and produce professional narration | Commercials, documentaries, animations, corporate videos |
| `audiobook-studio` | Produce full multi-chapter audiobooks with consistent narration | Published books, training materials, poetry collections |
| `audiobook-workshop` | Convert long text/files into audiobooks with async TTS and voice selection | Quick audiobook production, content repurposing |
| `short-drama-kit` | Generate mini drama scripts with character voices and optional teaser | Audio dramas, voice acting demos, podcast fiction |

### 📚 Learning & Education Skills

| Skill | Description | Best For |
|-------|-------------|----------|
| `language-learning-kit` | Create vocabulary flashcards with native audio and cultural context images | Language education, vocabulary building, pronunciation practice |
| `educational-course-kit` | Generate complete micro-courses with slides, narration, and preview videos | Online courses, corporate training, tutorial content |
| `storybook-lesson` | Make kid-friendly learning cards with illustrations and narration | Children's education, parenting, classroom activities |
| `photo-learning` | Recognize photos and narrate kid-friendly explanations | Educational apps, parent-child activities, visual learning |

### 🎨 Creative & Wellness Skills

| Skill | Description | Best For |
|-------|-------------|----------|
| `meditation-wellness-kit` | Generate calming audio-visual experiences for meditation and relaxation | Wellness apps, sleep aids, mindfulness practice |
| `game-asset-generator` | Create game assets: characters, items, ambient music, and trailers | Indie game development, prototype assets, pitch materials |
| `interactive-story-kit` | Build branching audio stories with distinct voices and adaptive music | Audio fiction, interactive experiences, storytelling apps |

### 🛒 Marketing & Commerce Skills

| Skill | Description | Best For |
|-------|-------------|----------|
| `music-promo-pack` | Generate music clips with cover art and optional spoken tag lines | Artist promotion, playlist submissions, sound branding |
| `product-photography-kit` | Transform product photos into e-commerce images with lifestyle variations | Amazon listings, Shopify stores, advertising campaigns |
| `ad-commercial-kit` | Create professional commercials with hook, demo, music, and CTA | Digital ads, brand campaigns, product launches |

### Try Them Out

```bash
# Start a skill session
/minimax skill video-studio
/minimax skill voiceover-studio
/minimax skill podcast-kit

# List all available skills
/minimax skill --list
```

### Creating Your Own Skills

Sample skills live in the `skills/` directory. Create your own by adding a folder with a `SKILL.md` file:

```markdown
---
name: my-custom-skill
description: What your skill does
allowed-tools: generate_image, tts, generate_music
---
You are running the My Custom Skill skill.

Goal
- Clear objective for the skill

Ask for
- What information you need from users

Workflow
1) Step one
2) Step two
...
```

See the `skills/` directory for more examples!

---

## 📚 Documentation

- **[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)** — System architecture overview
- **[docs/TUI_UPGRADES.md](docs/TUI_UPGRADES.md)** — UI changes and enhancements
- **[docs/VOICE_AND_TONE.md](docs/VOICE_AND_TONE.md)** — CLI personality guidelines
- **[CODEX_MAIN_UI_UX_PARITY_CHECKLIST.md](CODEX_MAIN_UI_UX_PARITY_CHECKLIST.md)** — UI/UX quality checklist
- **[LAUNCH_CHECKLIST.md](LAUNCH_CHECKLIST.md)** — Release readiness checklist

---

## 🤝 Contributing

We welcome contributions! See [CONTRIBUTING.md](CONTRIBUTING.md) for:

- Development setup
- Code style guidelines
- Testing requirements
- Pull request process

---

## 🙏 Acknowledgments

Inspired by MiniMax's open approach to developer tooling and the broader CLI ecosystem. Special thanks to the MiniMax team for building such a capable model.

---

## 📄 License

MIT License — See [LICENSE](LICENSE) for details.

---

<div align="center">

**MiniMax CLI** — *Your terminal interface to MiniMax M2.1*

Made with ❤️ by the community

</div>

---

*MiniMax and the MiniMax logo are trademarks of MiniMax Inc. This is an unofficial CLI project.*
