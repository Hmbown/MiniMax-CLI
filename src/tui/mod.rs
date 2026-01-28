//! Terminal UI (TUI) module for `MiniMax` CLI.

// === Submodules ===

pub mod app;
pub mod approval;
pub mod clipboard;
pub mod command_completer;
pub mod event_broker;
pub mod fuzzy_picker;
pub mod history;
pub mod history_picker;
pub mod model_picker;
pub mod paste_burst;
pub mod scrolling;
pub mod search_view;
pub mod selection;
pub mod session_picker;
pub mod streaming;
pub mod suggestions;
pub mod syntax;
pub mod transcript;
pub mod tutorial;
pub mod ui;
pub mod views;
pub mod widgets;

// === Re-exports ===

pub use app::TuiOptions;
pub use ui::run_tui;
