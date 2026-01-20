# MiniMax Palette

MiniMax CLI uses a shared palette so the TUI and CLI output stay on-brand.
The source of truth is `src/palette.rs`.

## Brand Colors

- MiniMax Blue `#1456F0` (primary accent, headers)
- MiniMax Red `#F23F5D` (errors)
- MiniMax Orange `#FF633A` (warnings, highlights)
- MiniMax Magenta `#E4177F` (plan/duo accents)
- MiniMax Ink `#181E25` (dark surfaces)
- MiniMax Black `#0A0D0D` (deep background)
- MiniMax Slate `#353C43` (muted UI)
- MiniMax Silver `#C9CDD4` (muted text)
- MiniMax Snow `#F7F8FA` (primary text)
- MiniMax Green `#4ADE80` (success)

## Semantic Tokens

- `TEXT_PRIMARY`, `TEXT_MUTED`, `TEXT_DIM`
- `STATUS_SUCCESS`, `STATUS_WARNING`, `STATUS_ERROR`, `STATUS_INFO`
- `SELECTION_BG`, `COMPOSER_BG`

## Usage

- Prefer `crate::palette::*` constants instead of hardcoded colors.
- For CLI output, use the `*_RGB` constants with `colored::Colorize::truecolor`.
