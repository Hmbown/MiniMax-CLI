# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- MiniMax tool suite expansions (TTS async, file ops, image understanding)
- Example skills for audiobook, video, music, and photo workflows
- Launch assets and checklist documentation

### Changed
- Tool call UI now summarizes output instead of dumping full payloads

### Fixed
- MiniMax tool calling payload now sends reasoning_split at the root

## [0.2.3] - 2026-01-11

### Added
- LICENSE file (MIT)
- CI workflow for linting and testing
- Crates.io publish workflow
- CONTRIBUTING.md with development guidelines
- docs/ARCHITECTURE.md with system overview
- Shell completion support (`minimax completions`)
- `--doctor` command for system diagnostics

### Fixed
- All 35 clippy warnings resolved
- Missing module exports in TUI module
- Unicode width calculation fixed
- Wrap caching implemented
- Bracketed paste enabled
- Approval system integrated

### Changed
- Cleaned up internal prompt files from repository root
- Removed dead code from archive folder

## [0.2.2] - 2026-01-10

### Fixed
- Minor bug fixes and improvements

## [0.2.0] - 2026-01-09

### Changed
- Simplified CLI - just run `minimax` to start
- Improved user experience with auto-configuration

## [0.1.2] - 2026-01-08

### Fixed
- Release workflow: unique artifact names per platform

## [0.1.1] - 2026-01-08

### Fixed
- Release artifact naming

## [0.1.0] - 2026-01-07

### Added
- Initial release
- Interactive TUI chat interface
- MiniMax M2.1 API integration
- Claude API integration (optional)
- Tool execution with shell, file operations
- MCP (Model Context Protocol) support
- Session management with history
- macOS sandboxing support
- Configurable hooks system
- Skills/plugin system
- Cost tracking and estimation
- Multi-profile configuration support

[Unreleased]: https://github.com/Hmbown/MiniMax-CLI/compare/v0.2.3...HEAD
[0.2.3]: https://github.com/Hmbown/MiniMax-CLI/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/Hmbown/MiniMax-CLI/compare/v0.2.0...v0.2.2
[0.2.0]: https://github.com/Hmbown/MiniMax-CLI/compare/v0.1.2...v0.2.0
[0.1.2]: https://github.com/Hmbown/MiniMax-CLI/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/Hmbown/MiniMax-CLI/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/Hmbown/MiniMax-CLI/releases/tag/v0.1.0
