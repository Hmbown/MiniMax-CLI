# MiniMax CLI TUI upgrades

Goal: exceed codex-style UX with a calmer, more legible, and more helpful TUI.

## Implemented upgrades
- Tool call rendering is compact and readable (typed cells, minimal output, summary lines instead of raw JSON dumps).
- Tool cells now hide args after completion and keep a single-line result by default.
- MiniMax thinking indicator uses a branded squiggle animation in the status line.
- Thinking blocks no longer stream into the transcript; only summary cells are shown.
- Context remaining indicator is shown in the footer.
- Last-token usage is surfaced in the footer when available (prompt/completion).
- Command help popup pulls from the live command registry so it never drifts.
- Local repo `skills/` folder is auto-discovered for `/skills` and `/skill`.
- Image understanding is wired via `analyze_image` (uses `POST /v1/chat/completions`).
- MiniMax demos skills added: audiobook workshop, audiobook studio, video studio, short drama kit, photo learning, storybook lesson, music promo pack.

## Keybindings and commands (working)
- Esc: cancel in-flight request or clear input (also closes popups)
- Ctrl+C: cancel in-flight request or exit
- F1: open help
- Tab: cycle mode (normal -> agent -> plan -> normal)
- PgUp/PgDn/Home/End: scroll transcript
- Alt+Up/Down: small scroll
- /tokens: session token usage details
- /cost: pricing summary
- /context: context window estimate
- /minimax (/dashboard /api): show MiniMax dashboard/docs links
- /skills + /skill <name>: list and run skills

## New experience wins (vs. previous builds)
- Tool calls feel like status cards instead of noisy system logs.
- Multi-step flows can run without flooding the transcript.
- Skills can be bundled alongside the repo for first-run demos.
- Branded status animation gives the UI some personality while working.

## High-impact upgrades to rival codex
- Expand/collapse tool cards (default collapsed, full output on demand).
- Inline diff previews for file edits (mini patch summary + toggle).
- Task timeline lane (queued/running/completed tool calls at a glance).
- Command palette (Ctrl+K) for mode/model/skill switching.
- Session “shots” (save a tool run + outputs as a reusable preset).

## Notes on API examples
- Vision/image understanding uses the official base64-in-prompt pattern.
- Async audiobook flow uses `t2a_async_v2` and file upload/retrieve/download tools.

