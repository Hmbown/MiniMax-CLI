use crossterm::event::KeyEvent;
use ratatui::{buffer::Buffer, layout::Rect};
use std::fmt;

use crate::palette;
use crate::tui::approval::ReviewDecision;
use crate::tui::history_picker::HistoryPickerResult;
use crate::tui::model_picker::ModelPickerResult;
use crate::tui::session_picker::SessionPickerResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalKind {
    Approval,
    Help,
    SessionPicker,
    ModelPicker,
    HistoryPicker,
    Search,
}

#[derive(Debug, Clone)]
pub enum ViewEvent {
    ApprovalDecision {
        tool_id: String,
        tool_name: String,
        decision: ReviewDecision,
        timed_out: bool,
    },
    SessionPickerResult {
        result: SessionPickerResult,
    },
    ModelPickerResult {
        result: ModelPickerResult,
    },
    HistoryPickerResult {
        result: HistoryPickerResult,
    },
    SearchResultSelected {
        result: SearchResult,
    },
}

#[derive(Debug, Clone)]
pub enum ViewAction {
    None,
    Close,
    EmitAndClose(ViewEvent),
}

pub trait ModalView {
    fn kind(&self) -> ModalKind;
    fn handle_key(&mut self, key: KeyEvent) -> ViewAction;
    fn render(&self, area: Rect, buf: &mut Buffer);
    fn tick(&mut self) -> ViewAction {
        ViewAction::None
    }
    /// Returns true if this view has content that can be expanded/collapsed
    fn has_expandable_content(&self) -> bool {
        false
    }
    /// Returns true if the view's content is currently expanded
    fn is_expanded(&self) -> bool {
        false
    }
    /// Returns self as Any for downcasting
    fn as_any(&self) -> &dyn std::any::Any;
    /// Returns self as Any for mutable downcasting
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

#[derive(Default)]
pub struct ViewStack {
    views: Vec<Box<dyn ModalView>>,
}

impl ViewStack {
    pub fn new() -> Self {
        Self { views: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.views.is_empty()
    }

    pub fn top_kind(&self) -> Option<ModalKind> {
        self.views.last().map(|view| view.kind())
    }

    /// Get reference to top view as Any for downcasting
    pub fn top_as_any(&self) -> Option<&dyn std::any::Any> {
        self.views.last().map(|view| view.as_any())
    }

    /// Get mutable reference to top view as Any for downcasting
    pub fn top_as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        self.views.last_mut().map(|view| view.as_any_mut())
    }

    /// Check if the top view has expandable content that is currently collapsed
    pub fn top_has_collapsed_content(&self) -> bool {
        self.views
            .last()
            .map(|view| view.has_expandable_content() && !view.is_expanded())
            .unwrap_or(false)
    }

    pub fn push<V: ModalView + 'static>(&mut self, view: V) {
        self.views.push(Box::new(view));
    }

    pub fn pop(&mut self) -> Option<Box<dyn ModalView>> {
        self.views.pop()
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        for view in &self.views {
            view.render(area, buf);
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Vec<ViewEvent> {
        let action = self
            .views
            .last_mut()
            .map(|view| view.handle_key(key))
            .unwrap_or(ViewAction::None);
        self.apply_action(action)
    }

    pub fn tick(&mut self) -> Vec<ViewEvent> {
        let action = self
            .views
            .last_mut()
            .map(|view| view.tick())
            .unwrap_or(ViewAction::None);
        self.apply_action(action)
    }

    fn apply_action(&mut self, action: ViewAction) -> Vec<ViewEvent> {
        let mut events = Vec::new();
        match action {
            ViewAction::None => {}
            ViewAction::Close => {
                self.views.pop();
            }
            ViewAction::EmitAndClose(event) => {
                events.push(event);
                self.views.pop();
            }
        }
        events
    }
}

impl fmt::Debug for ViewStack {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ViewStack")
            .field("len", &self.views.len())
            .field("top", &self.top_kind())
            .finish()
    }
}

pub struct HelpView {
    scroll: usize,
}

impl HelpView {
    pub fn new() -> Self {
        Self { scroll: 0 }
    }
}

impl ModalView for HelpView {
    fn kind(&self) -> ModalKind {
        ModalKind::Help
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn handle_key(&mut self, key: KeyEvent) -> ViewAction {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => ViewAction::Close,
            KeyCode::Up | KeyCode::Char('k') => {
                self.scroll = self.scroll.saturating_sub(1);
                ViewAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.scroll = self.scroll.saturating_add(1);
                ViewAction::None
            }
            _ => ViewAction::None,
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        use ratatui::{
            prelude::Stylize,
            style::Style,
            text::{Line, Span},
            widgets::{Block, Borders, Clear, Paragraph, Widget},
        };

        let popup_width = 65.min(area.width.saturating_sub(4));
        let popup_height = 24.min(area.height.saturating_sub(4));

        let popup_area = Rect {
            x: (area.width - popup_width) / 2,
            y: (area.height - popup_height) / 2,
            width: popup_width,
            height: popup_height,
        };

        Clear.render(popup_area, buf);

        let mut help_lines: Vec<Line> = vec![
            Line::from(vec![Span::styled(
                "MiniMax CLI Help",
                Style::default().fg(palette::MINIMAX_BLUE).bold(),
            )]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Modes:",
                Style::default().fg(palette::MINIMAX_ORANGE).bold(),
            )]),
            Line::from("  Tab cycles modes: Normal → Plan → Agent → Yolo → RLM"),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Commands:",
                Style::default().fg(palette::MINIMAX_ORANGE).bold(),
            )]),
        ];

        for cmd in crate::commands::COMMANDS.iter() {
            help_lines.push(Line::from(format!(
                "  /{:<12} - {}",
                cmd.name, cmd.description
            )));
        }

        help_lines.push(Line::from(""));
        help_lines.push(Line::from(vec![Span::styled(
            "Tools:",
            Style::default().fg(palette::MINIMAX_ORANGE).bold(),
        )]));
        help_lines.push(Line::from(
            "  web_search   - Search the web (DuckDuckGo; MCP optional)",
        ));
        help_lines.push(Line::from("  mcp_*        - Tools exposed by MCP servers"));
        help_lines.push(Line::from(""));
        help_lines.push(Line::from(vec![Span::styled(
            "Keys:",
            Style::default().fg(palette::MINIMAX_ORANGE).bold(),
        )]));
        help_lines.push(Line::from(
            "  Enter        - Send message / Execute shell command",
        ));
        help_lines.push(Line::from("  Esc          - Cancel request / Clear input"));
        help_lines.push(Line::from("  Tab          - Cycle modes"));
        help_lines.push(Line::from("  Ctrl+C       - Exit"));
        help_lines.push(Line::from("  Ctrl+X       - Toggle shell mode"));
        help_lines.push(Line::from(
            "  Ctrl+J       - Insert newline (multiline input)",
        ));
        help_lines.push(Line::from("  Alt+Enter    - Insert newline (multiline input)"));
        help_lines.push(Line::from("  Ctrl+D       - Exit when input is empty"));
        help_lines.push(Line::from("  Ctrl+W       - Delete word backward"));
        help_lines.push(Line::from("  Ctrl+K       - Delete to end of line"));
        help_lines.push(Line::from("  Ctrl+V       - Paste from clipboard"));
        help_lines.push(Line::from("  Ctrl+/       - Show this help"));
        help_lines.push(Line::from("  F1           - Show this help"));
        help_lines.push(Line::from("  Up/Down      - Scroll this help"));
        help_lines.push(Line::from(""));

        let total_lines = help_lines.len();
        let visible_lines = (popup_height as usize).saturating_sub(3);
        let max_scroll = total_lines.saturating_sub(visible_lines);
        let scroll = self.scroll.min(max_scroll);

        let scroll_indicator = if total_lines > visible_lines {
            format!(" [{}/{} ↑↓] ", scroll + 1, max_scroll + 1)
        } else {
            String::new()
        };

        let help = Paragraph::new(help_lines)
            .block(
                Block::default()
                    .title(Line::from(vec![Span::styled(
                        " Help ",
                        Style::default().fg(palette::MINIMAX_BLUE).bold(),
                    )]))
                    .title_bottom(Line::from(vec![
                        Span::styled(" Esc to close ", Style::default().fg(palette::TEXT_MUTED)),
                        Span::styled(
                            scroll_indicator,
                            Style::default().fg(palette::MINIMAX_ORANGE),
                        ),
                    ]))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(palette::MINIMAX_ORANGE)),
            )
            .scroll((scroll as u16, 0));

        help.render(popup_area, buf);
    }
}

// Re-export search view types
pub use crate::tui::search_view::{SearchView, SearchResult, render_search_results};
