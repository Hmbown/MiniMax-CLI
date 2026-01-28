//! Search view for transcript search functionality.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};
use regex::Regex;

use crate::palette;
use crate::tui::history::HistoryCell;
use crate::tui::views::{ModalKind, ModalView, ViewAction};

/// A single search result with location info
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// Index into the history cells
    pub cell_index: usize,
    /// The preview text to display
    pub preview: String,
    /// Start byte index of match in preview
    pub match_start: usize,
    /// End byte index of match in preview
    pub match_end: usize,
    /// Source of the message (user/assistant/system)
    pub source: String,
    /// Timestamp or index info
    pub timestamp: String,
}

/// Search view state
pub struct SearchView {
    query: String,
    cursor_position: usize,
    case_sensitive: bool,
    regex_mode: bool,
    selected_idx: usize,
    scroll_offset: usize,
    last_results: Vec<SearchResult>,
}

impl SearchView {
    /// Create a new search view with optional initial query
    pub fn new(initial_query: Option<&str>) -> Self {
        let query = initial_query.unwrap_or("").to_string();
        let cursor_position = query.chars().count();
        Self {
            query,
            cursor_position,
            case_sensitive: false,
            regex_mode: false,
            selected_idx: 0,
            scroll_offset: 0,
            last_results: Vec::new(),
        }
    }

    /// Get current query
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Check if case sensitive
    pub fn case_sensitive(&self) -> bool {
        self.case_sensitive
    }

    /// Check if regex mode
    pub fn regex_mode(&self) -> bool {
        self.regex_mode
    }

    /// Get selected result index
    pub fn selected_idx(&self) -> usize {
        self.selected_idx
    }

    /// Search through history cells and return results
    pub fn search(&mut self, history: &[HistoryCell]) -> Vec<SearchResult> {
        if self.query.is_empty() {
            self.last_results.clear();
            return Vec::new();
        }

        let mut results = Vec::new();

        for (cell_idx, cell) in history.iter().enumerate() {
            let (content, source) = match cell {
                HistoryCell::User { content } => (content.clone(), "You"),
                HistoryCell::Assistant { content, .. } => (content.clone(), "MiniMax"),
                HistoryCell::System { content } => (content.clone(), "System"),
                HistoryCell::ThinkingSummary { summary } => (summary.clone(), "Thinking"),
                _ => continue,
            };

            let timestamp = format!("#{}", cell_idx + 1);

            if self.regex_mode {
                // Regex search
                let pattern = if self.case_sensitive {
                    format!("(?m){}", regex::escape(&self.query))
                } else {
                    format!("(?im){}", regex::escape(&self.query))
                };

                if let Ok(re) = Regex::new(&pattern) {
                    for mat in re.find_iter(&content) {
                        let preview = Self::create_preview(&content, mat.start(), mat.end());
                        let match_start =
                            preview.find(&content[mat.start()..mat.end()]).unwrap_or(0);
                        let match_end = match_start + (mat.end() - mat.start());

                        results.push(SearchResult {
                            cell_index: cell_idx,
                            preview,
                            match_start,
                            match_end,
                            source: source.to_string(),
                            timestamp: timestamp.clone(),
                        });
                    }
                }
            } else {
                // Literal search
                let search_in = if self.case_sensitive {
                    content.clone()
                } else {
                    content.to_lowercase()
                };
                let search_for = if self.case_sensitive {
                    self.query.clone()
                } else {
                    self.query.to_lowercase()
                };

                let mut start = 0;
                while let Some(pos) = search_in[start..].find(&search_for) {
                    let actual_pos = start + pos;
                    let match_end = actual_pos + search_for.len();

                    let preview = Self::create_preview(&content, actual_pos, match_end);

                    // Calculate match position in preview
                    let match_text = if self.case_sensitive {
                        &content[actual_pos..match_end]
                    } else {
                        &search_in[actual_pos..match_end]
                    };
                    let match_start_in_preview = preview.find(match_text).unwrap_or(0);
                    let match_end_in_preview = match_start_in_preview + match_text.len();

                    results.push(SearchResult {
                        cell_index: cell_idx,
                        preview,
                        match_start: match_start_in_preview,
                        match_end: match_end_in_preview,
                        source: source.to_string(),
                        timestamp: timestamp.clone(),
                    });

                    start = actual_pos + 1;
                    if start >= content.len() {
                        break;
                    }
                }
            }
        }

        if results.is_empty() {
            self.selected_idx = 0;
            self.scroll_offset = 0;
        } else if self.selected_idx >= results.len() {
            self.selected_idx = results.len().saturating_sub(1);
            self.adjust_scroll(results.len());
        }

        self.last_results = results.clone();
        results
    }

    /// Create a preview string with context around the match
    fn create_preview(content: &str, match_start: usize, match_end: usize) -> String {
        const PREVIEW_CHARS: usize = 60;

        let content_len = content.len();
        let context_start = match_start.saturating_sub(PREVIEW_CHARS);
        let context_end = (match_end + PREVIEW_CHARS).min(content_len);

        let mut preview = String::new();

        if context_start > 0 {
            preview.push_str("...");
        }

        // Find char boundaries
        let start_idx = content
            .char_indices()
            .find(|(i, _)| *i >= context_start)
            .map(|(i, _)| i)
            .unwrap_or(0);
        let end_idx = content
            .char_indices()
            .find(|(i, _)| *i >= context_end)
            .map(|(i, _)| i)
            .unwrap_or(content_len);

        preview.push_str(&content[start_idx..end_idx]);

        if context_end < content_len {
            preview.push_str("...");
        }

        // Replace newlines with spaces for single-line preview
        preview.replace('\n', " ")
    }

    /// Move to next result
    pub fn next_result(&mut self, total_results: usize) {
        if total_results > 0 {
            self.selected_idx = (self.selected_idx + 1) % total_results;
            self.adjust_scroll(total_results);
        }
    }

    /// Move to previous result
    pub fn prev_result(&mut self, total_results: usize) {
        if total_results > 0 {
            self.selected_idx = self.selected_idx.saturating_sub(1);
            if self.selected_idx >= total_results {
                self.selected_idx = total_results - 1;
            }
            self.adjust_scroll(total_results);
        }
    }

    /// Adjust scroll offset to keep selected item visible
    fn adjust_scroll(&mut self, total_results: usize) {
        const VISIBLE_ITEMS: usize = 8;

        if self.selected_idx < self.scroll_offset {
            self.scroll_offset = self.selected_idx;
        } else if self.selected_idx >= self.scroll_offset + VISIBLE_ITEMS {
            self.scroll_offset = self.selected_idx.saturating_sub(VISIBLE_ITEMS - 1);
        }

        // Ensure scroll doesn't go past end
        if self.scroll_offset + VISIBLE_ITEMS > total_results && total_results > VISIBLE_ITEMS {
            self.scroll_offset = total_results - VISIBLE_ITEMS;
        }
    }
}

impl ModalView for SearchView {
    fn kind(&self) -> ModalKind {
        ModalKind::Search
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn handle_key(&mut self, key: KeyEvent) -> ViewAction {
        let total_results = self.last_results.len();
        match key.code {
            KeyCode::Esc => ViewAction::Close,
            KeyCode::Enter => {
                if let Some(result) = self.last_results.get(self.selected_idx).cloned() {
                    ViewAction::EmitAndClose(crate::tui::views::ViewEvent::SearchResultSelected {
                        result,
                    })
                } else {
                    ViewAction::Close
                }
            }
            KeyCode::Up => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.prev_result(total_results);
                } else {
                    self.prev_result(total_results);
                }
                ViewAction::None
            }
            KeyCode::Down => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.next_result(total_results);
                } else {
                    self.next_result(total_results);
                }
                ViewAction::None
            }
            KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.next_result(total_results);
                ViewAction::None
            }
            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.prev_result(total_results);
                ViewAction::None
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.case_sensitive = !self.case_sensitive;
                self.selected_idx = 0;
                self.scroll_offset = 0;
                self.last_results.clear();
                ViewAction::None
            }
            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.regex_mode = !self.regex_mode;
                self.selected_idx = 0;
                self.scroll_offset = 0;
                self.last_results.clear();
                ViewAction::None
            }
            KeyCode::Char('c') if key.modifiers.is_empty() => {
                self.case_sensitive = !self.case_sensitive;
                self.selected_idx = 0;
                self.scroll_offset = 0;
                self.last_results.clear();
                ViewAction::None
            }
            KeyCode::Char('r') if key.modifiers.is_empty() => {
                self.regex_mode = !self.regex_mode;
                self.selected_idx = 0;
                self.scroll_offset = 0;
                self.last_results.clear();
                ViewAction::None
            }
            KeyCode::Backspace => {
                if self.cursor_position > 0 {
                    let byte_pos = self
                        .query
                        .char_indices()
                        .nth(self.cursor_position - 1)
                        .map(|(i, c)| i + c.len_utf8())
                        .unwrap_or(0);
                    self.query.remove(byte_pos - 1);
                    self.cursor_position -= 1;
                }
                self.selected_idx = 0;
                self.scroll_offset = 0;
                self.last_results.clear();
                ViewAction::None
            }
            KeyCode::Char(c) => {
                let byte_pos = self
                    .query
                    .char_indices()
                    .nth(self.cursor_position)
                    .map(|(i, _)| i)
                    .unwrap_or(self.query.len());
                self.query.insert(byte_pos, c);
                self.cursor_position += 1;
                self.selected_idx = 0;
                self.scroll_offset = 0;
                self.last_results.clear();
                ViewAction::None
            }
            _ => ViewAction::None,
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        // Calculate popup dimensions
        let popup_width = 80.min(area.width.saturating_sub(4));
        let popup_height = 20.min(area.height.saturating_sub(4));

        let popup_area = Rect {
            x: (area.width - popup_width) / 2,
            y: (area.height - popup_height) / 2,
            width: popup_width,
            height: popup_height,
        };

        Clear.render(popup_area, buf);

        // Create layout
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(2), // Query input
                Constraint::Length(1), // Options bar
                Constraint::Min(1),    // Results list
                Constraint::Length(1), // Footer
            ])
            .split(popup_area);

        // Draw border and title
        Block::default()
            .title(" Search Transcript ")
            .title_style(Style::default().fg(palette::MINIMAX_BLUE).bold())
            .borders(Borders::ALL)
            .border_style(Style::default().fg(palette::MINIMAX_ORANGE))
            .render(popup_area, buf);

        // Query input line
        let _query_line = format!("> {}", self.query);
        let query_style = Style::default().fg(palette::TEXT_PRIMARY);
        Paragraph::new(Line::from(vec![
            Span::styled("> ", Style::default().fg(palette::MINIMAX_BLUE).bold()),
            Span::styled(&self.query, query_style),
        ]))
        .render(chunks[0], buf);

        // Options bar
        let case_style = if self.case_sensitive() {
            Style::default()
                .fg(palette::MINIMAX_GREEN)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(palette::TEXT_MUTED)
        };
        let regex_style = if self.regex_mode() {
            Style::default()
                .fg(palette::MINIMAX_GREEN)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(palette::TEXT_MUTED)
        };

        let options_line = Line::from(vec![
            Span::styled("[c]", case_style),
            Span::styled(" Case ", Style::default().fg(palette::TEXT_MUTED)),
            Span::styled("[r]", regex_style),
            Span::styled(" Regex", Style::default().fg(palette::TEXT_MUTED)),
        ]);
        Paragraph::new(options_line).render(chunks[1], buf);

        // Note: We can't actually search here since we don't have access to history
        // The results will be rendered by the caller who has access to App state
        // For now, render a placeholder message
        let placeholder = Line::from(vec![Span::styled(
            "Type to search...",
            Style::default().fg(palette::TEXT_MUTED).italic(),
        )]);
        Paragraph::new(placeholder).render(chunks[2], buf);

        // Footer with key hints
        let footer = Line::from(vec![
            Span::styled("Esc", Style::default().fg(palette::MINIMAX_BLUE)),
            Span::styled(" close ", Style::default().fg(palette::TEXT_MUTED)),
            Span::styled("↑↓", Style::default().fg(palette::MINIMAX_BLUE)),
            Span::styled(" navigate", Style::default().fg(palette::TEXT_MUTED)),
        ]);
        Paragraph::new(footer).render(chunks[3], buf);
    }
}

/// Render search results - called by ui.rs with access to App state
pub fn render_search_results(
    area: Rect,
    buf: &mut Buffer,
    search_view: &SearchView,
    results: &[SearchResult],
    current_idx: Option<usize>,
) {
    let results_area = area.inner(Margin::new(1, 3)); // Account for border, query, options

    if results.is_empty() {
        if search_view.query().is_empty() {
            let msg = Line::from(vec![Span::styled(
                "Type to search...",
                Style::default().fg(palette::TEXT_MUTED).italic(),
            )]);
            Paragraph::new(msg).render(results_area, buf);
        } else {
            let msg = Line::from(vec![Span::styled(
                "No matches found",
                Style::default().fg(palette::TEXT_MUTED).italic(),
            )]);
            Paragraph::new(msg).render(results_area, buf);
        }
        return;
    }

    // Calculate visible range
    let visible_count = results_area.height as usize;
    let scroll = search_view
        .scroll_offset
        .min(results.len().saturating_sub(1));
    let end = (scroll + visible_count).min(results.len());
    let visible_results = &results[scroll..end];

    let mut lines = Vec::new();

    // Counter line
    let counter_text = if let Some(current) = current_idx {
        format!("{} of {} matches", current + 1, results.len())
    } else {
        format!("{} matches", results.len())
    };
    lines.push(Line::from(vec![Span::styled(
        &counter_text,
        Style::default().fg(palette::MINIMAX_YELLOW).bold(),
    )]));
    lines.push(Line::from(""));

    // Result lines
    for (idx, result) in visible_results.iter().enumerate() {
        let absolute_idx = scroll + idx;
        let is_selected = Some(absolute_idx) == current_idx;

        let source_style = match result.source.as_str() {
            "You" => Style::default().fg(palette::MINIMAX_ORANGE).bold(),
            "MiniMax" => Style::default().fg(palette::MINIMAX_BLUE).bold(),
            "System" => Style::default().fg(palette::TEXT_MUTED).italic(),
            _ => Style::default().fg(palette::TEXT_PRIMARY),
        };

        // Source and timestamp
        let mut line_spans = vec![
            Span::styled(
                if is_selected { "> " } else { "  " },
                Style::default().fg(palette::MINIMAX_YELLOW),
            ),
            Span::styled(
                format!("[{}] ", result.timestamp),
                Style::default().fg(palette::TEXT_MUTED),
            ),
            Span::styled(format!("{:<8}", result.source), source_style),
            Span::raw(" "),
        ];

        // Preview with highlighted match
        let preview = &result.preview;
        let match_start = result.match_start.min(preview.len());
        let match_end = result.match_end.min(preview.len());

        if match_start < preview.len() && match_start < match_end {
            // Before match
            if match_start > 0 {
                line_spans.push(Span::styled(
                    &preview[..match_start],
                    Style::default().fg(palette::TEXT_PRIMARY),
                ));
            }
            // Match (highlighted in yellow)
            line_spans.push(Span::styled(
                &preview[match_start..match_end],
                Style::default()
                    .fg(palette::MINIMAX_BLACK)
                    .bg(palette::MINIMAX_YELLOW)
                    .add_modifier(Modifier::BOLD),
            ));
            // After match
            if match_end < preview.len() {
                line_spans.push(Span::styled(
                    &preview[match_end..],
                    Style::default().fg(palette::TEXT_PRIMARY),
                ));
            }
        } else {
            line_spans.push(Span::styled(
                preview,
                Style::default().fg(palette::TEXT_PRIMARY),
            ));
        }

        lines.push(Line::from(line_spans));
    }

    // Scroll indicator
    if results.len() > visible_count.saturating_sub(2) {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            format!("Scroll: {}/{}", scroll + 1, results.len()),
            Style::default().fg(palette::TEXT_MUTED),
        )]));
    }

    Paragraph::new(lines).render(results_area, buf);
}
