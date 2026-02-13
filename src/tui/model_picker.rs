//! Interactive model picker for switching between MiniMax models.
//!
//! Provides a simple list-based picker for available models with descriptions.

use crate::palette;
use crate::tui::views::{ModalKind, ModalView, ViewAction, ViewEvent};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    prelude::Widget,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

/// Information about a MiniMax model
#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub capabilities: &'static str,
}

/// Available MiniMax models
pub const AVAILABLE_MODELS: &[ModelInfo] = &[
    ModelInfo {
        id: "MiniMax-M2.5",
        name: "MiniMax 2.5",
        description: "Latest generation model with enhanced reasoning, tool calling, and 2M context",
        capabilities: "Text, reasoning, agents, office productivity, code",
    },
    ModelInfo {
        id: "MiniMax-M2.5-lightning",
        name: "MiniMax 2.5 Lightning",
        description: "Fast version of M2.5 for quick responses with same capabilities",
        capabilities: "Fast text, reasoning, agents, code",
    },
    ModelInfo {
        id: "MiniMax-M2.1",
        name: "MiniMax M2.1",
        description: "Polyglot programming mastery with precision code refactoring",
        capabilities: "Text generation, reasoning, analysis, code",
    },
    ModelInfo {
        id: "MiniMax-M2",
        name: "MiniMax M2",
        description: "Efficient agentic model for coding (10B active params, 230B total)",
        capabilities: "Code generation, agents, tool use",
    },
    ModelInfo {
        id: "MiniMax-Text-01",
        name: "MiniMax Text 01",
        description: "Text-optimized model for natural language tasks (256K context)",
        capabilities: "Text generation, summarization, Q&A",
    },
    ModelInfo {
        id: "MiniMax-Coding-01",
        name: "MiniMax Coding 01",
        description: "Code-specialized model for programming tasks (128K context)",
        capabilities: "Code generation, debugging, review",
    },
];

/// Result of a model selection
#[derive(Debug, Clone)]
pub enum ModelPickerResult {
    /// User selected a model
    Selected(String),
    /// User cancelled
    Cancelled,
}

/// Interactive picker for selecting a model
pub struct ModelPicker {
    /// Currently selected index
    selected: usize,
    /// ID of the currently active model (to highlight)
    current_model: String,
}

impl ModelPicker {
    /// Create a new model picker
    pub fn new(current_model: String) -> Self {
        // Find the index of the current model, or default to 0
        let selected = AVAILABLE_MODELS
            .iter()
            .position(|m| m.id == current_model)
            .unwrap_or(0);

        Self {
            selected,
            current_model,
        }
    }

    /// Get the currently selected model ID
    pub fn selected_model_id(&self) -> Option<String> {
        AVAILABLE_MODELS
            .get(self.selected)
            .map(|m| m.id.to_string())
    }

    /// Check if a model is the currently active one
    fn is_current_model(&self, id: &str) -> bool {
        self.current_model == id
    }

    /// Move selection up
    fn select_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        } else {
            self.selected = AVAILABLE_MODELS.len().saturating_sub(1);
        }
    }

    /// Move selection down
    fn select_down(&mut self) {
        if self.selected < AVAILABLE_MODELS.len() - 1 {
            self.selected += 1;
        } else {
            self.selected = 0;
        }
    }

    /// Render a model item
    fn render_model_item(&self, model: &ModelInfo, index: usize) -> ListItem<'_> {
        let is_selected = index == self.selected;
        let is_current = self.is_current_model(model.id);

        // Selection style
        let base_style = if is_selected {
            Style::default()
                .bg(palette::MINIMAX_BLUE)
                .fg(palette::MINIMAX_SNOW)
                .add_modifier(Modifier::BOLD)
        } else if is_current {
            Style::default()
                .fg(palette::MINIMAX_ORANGE)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(palette::TEXT_PRIMARY)
        };

        // Current indicator
        let current_indicator = if is_current { " ● " } else { "   " };

        let mut lines = vec![];

        // Title line with model name and current indicator
        let title_style = if is_selected {
            Style::default()
                .bg(palette::MINIMAX_BLUE)
                .fg(palette::MINIMAX_SNOW)
                .add_modifier(Modifier::BOLD)
        } else if is_current {
            Style::default()
                .fg(palette::MINIMAX_ORANGE)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(palette::TEXT_PRIMARY)
                .add_modifier(Modifier::BOLD)
        };

        let mut title_line = Line::from(vec![
            Span::styled(
                current_indicator,
                if is_current && !is_selected {
                    Style::default()
                        .fg(palette::MINIMAX_ORANGE)
                        .add_modifier(Modifier::BOLD)
                } else {
                    base_style
                },
            ),
            Span::styled(model.name, title_style),
        ]);

        if is_current {
            title_line.push_span(Span::styled(
                " (current)",
                if is_selected {
                    Style::default()
                        .bg(palette::MINIMAX_BLUE)
                        .fg(palette::MINIMAX_ORANGE)
                } else {
                    Style::default().fg(palette::TEXT_DIM)
                },
            ));
        }
        lines.push(title_line);

        // Description line
        let desc_style = if is_selected {
            Style::default()
                .bg(palette::MINIMAX_BLUE)
                .fg(palette::MINIMAX_SILVER)
        } else {
            Style::default().fg(palette::TEXT_DIM)
        };
        lines.push(Line::from(vec![
            Span::styled("     ", base_style),
            Span::styled(model.description, desc_style),
        ]));

        // Capabilities line
        let caps_style = if is_selected {
            Style::default()
                .bg(palette::MINIMAX_BLUE)
                .fg(palette::MINIMAX_SILVER)
        } else {
            Style::default().fg(palette::TEXT_MUTED)
        };
        lines.push(Line::from(vec![
            Span::styled("     ", base_style),
            Span::styled(format!("Capabilities: {}", model.capabilities), caps_style),
        ]));

        // Spacing between items
        lines.push(Line::from(""));

        ListItem::new(lines)
    }
}

impl ModalView for ModelPicker {
    fn kind(&self) -> ModalKind {
        ModalKind::ModelPicker
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn handle_key(&mut self, key: KeyEvent) -> ViewAction {
        match key.code {
            KeyCode::Esc => ViewAction::EmitAndClose(ViewEvent::ModelPickerResult {
                result: ModelPickerResult::Cancelled,
            }),
            KeyCode::Enter => {
                if let Some(id) = self.selected_model_id() {
                    ViewAction::EmitAndClose(ViewEvent::ModelPickerResult {
                        result: ModelPickerResult::Selected(id),
                    })
                } else {
                    ViewAction::Close
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.select_up();
                ViewAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.select_down();
                ViewAction::None
            }
            _ => ViewAction::None,
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        // Create a centered popup
        let popup_width = (area.width * 3 / 5).clamp(50, 70);
        let popup_height = (AVAILABLE_MODELS.len() as u16 * 5 + 6).min(area.height - 4);
        let popup_x = (area.width - popup_width) / 2;
        let popup_y = (area.height - popup_height) / 2;
        let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

        // Clear the background
        Clear.render(popup_area, buf);

        // Draw the border
        let block = Block::default()
            .title(" Model Selection ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(palette::MINIMAX_BLUE));
        let inner = block.inner(popup_area);
        block.render(popup_area, buf);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(inner);

        // Model list
        let items: Vec<ListItem> = AVAILABLE_MODELS
            .iter()
            .enumerate()
            .map(|(i, m)| self.render_model_item(m, i))
            .collect();

        let models_list = List::new(items);
        models_list.render(chunks[0], buf);

        // Help footer
        let help_text = format!(
            "↑/↓ to navigate | Enter to select | Esc to cancel | {} models",
            AVAILABLE_MODELS.len()
        );
        let help = Paragraph::new(Line::from(vec![Span::styled(
            help_text,
            Style::default().fg(palette::TEXT_DIM),
        )]));
        help.render(chunks[1], buf);
    }
}

/// Validate a model name against available models
pub fn validate_model(model_name: &str) -> Option<&'static ModelInfo> {
    let normalized = model_name.trim().to_ascii_lowercase();
    let canonical = match normalized.as_str() {
        "minimax-2.5" | "minimax-m2.5" | "m2.5" => "MiniMax-M2.5",
        "minimax-2.5-lightning" | "minimax-m2.5-lightning" | "m2.5-lightning" | "2.5-lightning" => {
            "MiniMax-M2.5-lightning"
        }
        "minimax-m2" | "m2" => "MiniMax-M2",
        _ => model_name,
    };

    AVAILABLE_MODELS
        .iter()
        .find(|m| m.id.eq_ignore_ascii_case(canonical) || m.name.eq_ignore_ascii_case(canonical))
}

/// Get the canonical model ID for a model name
#[allow(dead_code)]
pub fn resolve_model_id(model_name: &str) -> Option<String> {
    validate_model(model_name).map(|m| m.id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_model_exact_match() {
        let model = validate_model("MiniMax-M2.5");
        assert!(model.is_some());
        assert_eq!(model.unwrap().id, "MiniMax-M2.5");
    }

    #[test]
    fn test_validate_model_case_insensitive() {
        let model = validate_model("minimax-m2.5");
        assert!(model.is_some());
        assert_eq!(model.unwrap().id, "MiniMax-M2.5");
    }

    #[test]
    fn test_validate_model_alias() {
        let model = validate_model("MiniMax-2.5");
        assert!(model.is_some());
        assert_eq!(model.unwrap().id, "MiniMax-M2.5");
    }

    #[test]
    fn test_validate_model_not_found() {
        let model = validate_model("NonExistent-Model");
        assert!(model.is_none());
    }

    #[test]
    fn test_resolve_model_id() {
        assert_eq!(
            resolve_model_id("MiniMax-2.5"),
            Some("MiniMax-M2.5".to_string())
        );
        assert_eq!(
            resolve_model_id("minimax-text-01"),
            Some("MiniMax-Text-01".to_string())
        );
    }

    #[test]
    fn test_model_picker_navigation() {
        let mut picker = ModelPicker::new("MiniMax-M2.5".to_string());
        assert!(!AVAILABLE_MODELS.is_empty());
        let last_index = AVAILABLE_MODELS.len() - 1;
        assert_eq!(picker.selected, 0);

        // Move down through the full list, then wrap to top.
        for expected in 1..=last_index {
            picker.select_down();
            assert_eq!(picker.selected, expected);
        }
        picker.select_down();
        assert_eq!(picker.selected, 0);

        // Move up from top and wrap to last.
        picker.select_up();
        assert_eq!(picker.selected, last_index);
    }
}
