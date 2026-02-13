# Configuration

MiniMax CLI reads configuration from a TOML file plus environment variables.

## Where It Looks

Default config path:

- `~/.minimax/config.toml`

Overrides:

- CLI: `minimax --config /path/to/config.toml`
- Env: `MINIMAX_CONFIG_PATH=/path/to/config.toml`

If both are set, `--config` wins. Environment variable overrides are applied after the file is loaded.

## Profiles

You can define multiple profiles in the same file:

```toml
api_key = "PERSONAL_KEY"
default_text_model = "MiniMax-M2.5"

[profiles.work]
api_key = "WORK_KEY"
base_url = "https://api.minimax.io"
```

Select a profile with:

- CLI: `minimax --profile work`
- Env: `MINIMAX_PROFILE=work`

If a profile is selected but missing, MiniMax CLI exits with an error listing available profiles.

## Environment Variables

These override config values:

- `MINIMAX_API_KEY`
- `MINIMAX_BASE_URL`
- `MINIMAX_OUTPUT_DIR`
- `MINIMAX_SKILLS_DIR`
- `MINIMAX_MCP_CONFIG`
- `MINIMAX_NOTES_PATH`
- `MINIMAX_MEMORY_PATH`
- `MINIMAX_ALLOW_SHELL` (`1`/`true` enables)
- `MINIMAX_MAX_SUBAGENTS` (clamped to `1..=5`)
- `MINIMAX_AUTO_COMPACT` (`1`/`true` enables)
- `MINIMAX_COMPACTION_TOKEN_THRESHOLD` (integer, min `1`)
- `MINIMAX_COMPACTION_MESSAGE_THRESHOLD` (integer, min `1`)
- `MINIMAX_COMPACTION_KEEP_RECENT` (integer, min `1`)
- `MINIMAX_COMPACT_PROMPT` (string)
- `MINIMAX_AUTO_COMPACT_TOKEN_LIMIT` (integer, min `1`)

## Key Reference

### Core keys (used by the TUI/engine)

- `api_key` (string, required): must be non-empty (or set `MINIMAX_API_KEY`).
- `base_url` (string, optional): defaults to `https://api.minimax.io` (the CLI derives the text endpoint as `<base_url>/anthropic`).
- `default_text_model` (string, optional): defaults to `MiniMax-M2.5`.
- `allow_shell` (bool, optional): defaults to `false`.
- `max_subagents` (int, optional): defaults to `5` and is clamped to `1..=5`.
- `skills_dir` (string, optional): defaults to `~/.minimax/skills` (each skill is a directory containing `SKILL.md`).
- `mcp_config_path` (string, optional): defaults to `~/.minimax/mcp.json`.
- `notes_path` (string, optional): defaults to `~/.minimax/notes.txt` and is used by the `note` tool.
- `retry.*` (optional): retry/backoff settings for API requests:
  - `[retry].enabled` (bool, default `true`)
  - `[retry].max_retries` (int, default `3`)
  - `[retry].initial_delay` (float seconds, default `1.0`)
  - `[retry].max_delay` (float seconds, default `60.0`)
  - `[retry].exponential_base` (float, default `2.0`)
- `compaction.*` (optional): automatic/manual context compaction settings:
  - `[compaction].enabled` (bool): override auto-compaction on/off
  - `[compaction].token_threshold` (int): explicit estimated-token threshold
  - `[compaction].model_auto_compact_token_limit` (int): model-aware threshold override
  - `[compaction].message_threshold` (int, default `30`)
  - `[compaction].keep_recent` (int, default `6`)
  - `[compaction].model` (string): optional model override for summarization
  - `[compaction].cache_summary` (bool, default `true`)
  - `[compaction].compact_prompt` (string): custom summarization instruction
- `hooks` (optional): lifecycle hooks configuration (see `config.example.toml`).

### Parsed but currently unused (reserved for future versions)

These keys are accepted by the config loader but not currently used by the interactive TUI or built-in tools:

- `default_image_model`, `default_video_model`, `default_audio_model`, `default_music_model`
- `output_dir`
- `tools_file`
- `memory_path`

## Runtime State Persistence

MiniMax CLI persists background runtime metadata in the workspace so it survives restart/reload:

- Background shell jobs: `<workspace>/.minimax/state/background_jobs.json`
- Sub-agent registry: `<workspace>/.minimax/state/subagents.json`

Restore behavior:

- State is restored on engine startup, session sync (including resume/load/reset flows), and `/reload`.
- Jobs that were previously `running` are restored as `orphaned` with an explicit reason because process handles cannot be reattached after restart.
- Sub-agents that were previously `running` are restored as `failed` with reason `interrupted: previous MiniMax session ended before completion`.
- Existing non-running entries remain until cleaned (`/jobs clean`, `/subagents clean`, or corresponding tools).
- Missing/corrupt state files are handled softly; the UI receives `Runtime state warning: ...` status messages instead of crashing.

## Notes On `minimax doctor`

`minimax doctor` checks default locations under `~/.minimax/` (including `config.toml` and `mcp.json`). If you override paths via `--config` or `MINIMAX_MCP_CONFIG`, the doctor output may not reflect those overrides.
