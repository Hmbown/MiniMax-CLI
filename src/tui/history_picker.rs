//! Interactive command history picker for browsing and selecting previous inputs.
//!
//! Provides a fuzzy-searchable list of history entries with preview display.

use crate::palette;
use crate::tui::views::{ModalKind, ModalView, ViewAction, ViewEvent};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    prelude::Widget,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};
use std::time::SystemTime;

/// Maximum number of history entries to display at once
const MAX_VISIBLE_ENTRIES: usize = 10;
/// Maximum number of history entries to keep
const MAX_HISTORY_ENTRIES: usize = 100;
/// Maximum characters to show in preview
const PREVIEW_MAX_CHARS: usize = 80;

/// Result of a history selection
#[derive(Debug, Clone)]
pub enum HistoryPickerResult {
    /// User selected a history entry
    Selected(String),
    /// User cancelled
    Cancelled,
}

/// A history entry with timestamp
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    /// The input text
    pub text: String,
    /// When the entry was created
    pub timestamp: SystemTime,
}

/// A history match result with score for fuzzy filtering
#[derive(Debug, Clone)]
struct HistoryMatch {
    entry: HistoryEntry,
    score: i64,
    highlight_indices: Vec<usize>,
}

/// Interactive picker for selecting a history entry
pub struct HistoryPicker {
    /// All available history entries
    entries: Vec<HistoryEntry>,
    /// Current filtered matches
    matches: Vec<HistoryMatch>,
    /// Current search query
    query: String,
    /// Currently selected index
    selected: usize,
    /// Whether the picker is actively filtering
    filtering: bool,
}

impl HistoryPicker {
    /// Create a new history picker with the given input history
    pub fn new(input_history: &[String]) -> Self {
        // Convert strings to history entries with current timestamp
        // In a real implementation, timestamps would be stored with the history
        let now = SystemTime::now();
        let entries: Vec<HistoryEntry> = input_history
            .iter()
            .rev() // Most recent first
            .take(MAX_HISTORY_ENTRIES)
            .enumerate()
            .map(|(i, text)| HistoryEntry {
                text: text.clone(),
                // Simulate timestamps (more recent = smaller index)
                timestamp: now - std::time::Duration::from_secs(i as u64 * 60),
            })
            .collect();

        let matches = entries
            .iter()
            .map(|e| HistoryMatch {
                entry: e.clone(),
                score: 0,
                highlight_indices: Vec::new(),
            })
            .collect();

        Self {
            entries,
            matches,
            query: String::new(),
            selected: 0,
            filtering: false,
        }
    }

    /// Get the currently selected entry text (if any)
    pub fn selected_entry(&self) -> Option<String> {
        self.matches
            .get(self.selected)
            .map(|m| m.entry.text.clone())
    }

    /// Handle character input for filtering
    fn insert_char(&mut self, c: char) {
        self.filtering = true;
        self.query.push(c);
        self.update_matches();
    }

    /// Handle backspace for filtering
    fn backspace(&mut self) {
        self.query.pop();
        if self.query.is_empty() {
            self.filtering = false;
        }
        self.update_matches();
    }

    /// Move selection up
    fn select_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        } else if !self.matches.is_empty() {
            self.selected = self.matches.len() - 1;
        }
    }

    /// Move selection down
    fn select_down(&mut self) {
        if !self.matches.is_empty() && self.selected < self.matches.len() - 1 {
            self.selected += 1;
        } else {
            self.selected = 0;
        }
    }

    /// Update matches based on current query
    fn update_matches(&mut self) {
        if self.query.is_empty() {
            // Show all entries sorted by most recent
            self.matches = self
                .entries
                .iter()
                .map(|e| HistoryMatch {
                    entry: e.clone(),
                    score: 0,
                    highlight_indices: Vec::new(),
                })
                .collect();
        } else {
            // Fuzzy filter
            let query_lower = self.query.to_lowercase();
            let mut scored: Vec<HistoryMatch> = self
                .entries
                .iter()
                .filter_map(|e| {
                    let text_lower = e.text.to_lowercase();
                    if let Some((score, indices)) = fuzzy_match(&text_lower, &query_lower) {
                        Some(HistoryMatch {
                            entry: e.clone(),
                            score,
                            highlight_indices: indices,
                        })
                    } else {
                        None
                    }
                })
                .collect();

            // Sort by score descending
            scored.sort_by(|a, b| b.score.cmp(&a.score));
            self.matches = scored;
        }

        // Reset selection if out of bounds
        if self.selected >= self.matches.len() {
            self.selected = 0;
        }
    }

    /// Format relative time (e.g., "2h ago")
    fn format_age(&self, timestamp: SystemTime) -> String {
        let now = SystemTime::now();
        let duration = now.duration_since(timestamp).unwrap_or_default();

        let seconds = duration.as_secs();
        if seconds < 60 {
            "just now".to_string()
        } else if seconds < 3600 {
            format!("{}m ago", seconds / 60)
        } else if seconds < 86400 {
            format!("{}h ago", seconds / 3600)
        } else if seconds < 604800 {
            format!("{}d ago", seconds / 86400)
        } else {
            format!("{}w ago", seconds / 604800)
        }
    }

    /// Truncate text for preview
    fn truncate_for_preview(&self, text: &str) -> String {
        if text.len() <= PREVIEW_MAX_CHARS {
            text.to_string()
        } else {
            format!("{}...", &text[..PREVIEW_MAX_CHARS])
        }
    }

    /// Render a history item
    fn render_history_item(&self, history_match: &HistoryMatch, index: usize) -> ListItem<'_> {
        let entry = &history_match.entry;
        let is_selected = index == self.selected;

        // Selection style
        let base_style = if is_selected {
            Style::default()
                .bg(palette::MINIMAX_BLUE)
                .fg(palette::MINIMAX_SNOW)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(palette::TEXT_PRIMARY)
        };

        // Build the text with optional highlighting
        let text_spans = if self.filtering && !history_match.highlight_indices.is_empty() {
            self.build_highlighted_text(&entry.text, history_match, base_style)
        } else {
            let preview = self.truncate_for_preview(&entry.text);
            vec![Span::styled(preview, base_style)]
        };

        // Format metadata line
        let age = self.format_age(entry.timestamp);
        let meta_style = if is_selected {
            Style::default().fg(palette::MINIMAX_SILVER)
        } else {
            Style::default().fg(palette::TEXT_DIM)
        };

        let mut lines = vec![];

        // Text line
        let mut text_line = Line::default();
        for span in text_spans {
            text_line.push_span(span);
        }
        lines.push(text_line);

        // Meta line with timestamp
        lines.push(Line::from(vec![
            Span::styled("  ", base_style),
            Span::styled(age, meta_style),
        ]));

        // Spacing between items
        lines.push(Line::from(""));

        ListItem::new(lines)
    }

    /// Build highlighted text spans
    fn build_highlighted_text(
        &self,
        text: &str,
        history_match: &HistoryMatch,
        base_style: Style,
    ) -> Vec<Span<'_>> {
        let mut spans = Vec::new();
        let chars: Vec<char> = text.chars().collect();
        let mut last_idx = 0;

        for &idx in &history_match.highlight_indices {
            if idx > last_idx && last_idx < chars.len() {
                let chunk: String = chars[last_idx..idx].iter().collect();
                let preview_chunk = self.truncate_for_preview(&chunk);
                spans.push(Span::styled(preview_chunk, base_style));
            }
            if idx < chars.len() {
                spans.push(Span::styled(
                    chars[idx].to_string(),
                    base_style
                        .add_modifier(Modifier::BOLD)
                        .fg(palette::MINIMAX_YELLOW),
                ));
                last_idx = idx + 1;
            }
        }

        if last_idx < chars.len() {
            let chunk: String = chars[last_idx..].iter().collect();
            let preview_chunk = self.truncate_for_preview(&chunk);
            spans.push(Span::styled(preview_chunk, base_style));
        }

        spans
    }

    /// Get the preview text for the currently selected entry
    fn get_preview_text(&self) -> String {
        if let Some(entry) = self.matches.get(self.selected) {
            entry.entry.text.clone()
        } else {
            "No history entries".to_string()
        }
    }
}

impl ModalView for HistoryPicker {
    fn kind(&self) -> ModalKind {
        ModalKind::HistoryPicker
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn handle_key(&mut self, key: KeyEvent) -> ViewAction {
        match key.code {
            KeyCode::Esc => ViewAction::EmitAndClose(ViewEvent::HistoryPickerResult {
                result: HistoryPickerResult::Cancelled,
            }),
            KeyCode::Enter => {
                if let Some(text) = self.selected_entry() {
                    ViewAction::EmitAndClose(ViewEvent::HistoryPickerResult {
                        result: HistoryPickerResult::Selected(text),
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
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                self.insert_char(c);
                ViewAction::None
            }
            KeyCode::Backspace => {
                self.backspace();
                ViewAction::None
            }
            _ => ViewAction::None,
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        // Create a centered popup
        let popup_width = (area.width * 4 / 5).clamp(50, 80);
        let popup_height = (MAX_VISIBLE_ENTRIES as u16 * 3 + 10).min(area.height - 4);
        let popup_x = (area.width - popup_width) / 2;
        let popup_y = (area.height - popup_height) / 2;
        let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

        // Clear the background
        Clear.render(popup_area, buf);

        // Draw the border
        let block = Block::default()
            .title(" Command History ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(palette::MINIMAX_BLUE));
        let inner = block.inner(popup_area);
        block.render(popup_area, buf);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(3),
                Constraint::Length(1),
            ])
            .split(inner);

        // Query input
        let query_text = if self.query.is_empty() {
            Line::from(vec![Span::styled(
                "Type to filter history...",
                Style::default().fg(palette::TEXT_DIM),
            )])
        } else {
            Line::from(vec![
                Span::styled("Filter: ", Style::default().fg(palette::TEXT_MUTED)),
                Span::styled(&self.query, Style::default().fg(palette::TEXT_PRIMARY)),
            ])
        };
        let query = Paragraph::new(query_text)
            .block(Block::default().borders(Borders::BOTTOM))
            .wrap(Wrap { trim: true });
        query.render(chunks[0], buf);

        // History list
        let items: Vec<ListItem> = self
            .matches
            .iter()
            .take(MAX_VISIBLE_ENTRIES)
            .enumerate()
            .map(|(i, m)| self.render_history_item(m, i))
            .collect();

        let history_list = List::new(items);
        history_list.render(chunks[1], buf);

        // Preview of selected entry
        let preview_text = self.get_preview_text();
        let preview_truncated = if preview_text.len() > PREVIEW_MAX_CHARS {
            format!("{}...", &preview_text[..PREVIEW_MAX_CHARS])
        } else {
            preview_text
        };
        let preview = Paragraph::new(vec![
            Line::from(vec![Span::styled(
                "Preview:",
                Style::default()
                    .fg(palette::MINIMAX_ORANGE)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(vec![Span::styled(
                preview_truncated,
                Style::default().fg(palette::TEXT_DIM),
            )]),
        ])
        .block(Block::default().borders(Borders::TOP));
        preview.render(chunks[2], buf);

        // Help footer
        let help_text = if self.matches.is_empty() {
            "No history entries found".to_string()
        } else {
            format!(
                "↑/↓ to navigate | Enter to select | Esc to cancel | {} entries",
                self.matches.len()
            )
        };
        let help = Paragraph::new(Line::from(vec![Span::styled(
            help_text,
            Style::default().fg(palette::TEXT_DIM),
        )]));
        help.render(chunks[3], buf);
    }
}

/// Simple fuzzy matching algorithm (same as fuzzy_picker.rs and session_picker.rs)
fn fuzzy_match(haystack: &str, needle: &str) -> Option<(i64, Vec<usize>)> {
    if needle.is_empty() {
        return Some((0, Vec::new()));
    }

    let hay_chars: Vec<char> = haystack.chars().collect();
    let needle_chars: Vec<char> = needle.chars().collect();
    let mut indices = Vec::new();
    let mut hay_idx = 0;
    let mut needle_idx = 0;
    let mut score: i64 = 0;
    let mut prev_match_idx: Option<usize> = None;

    while hay_idx < hay_chars.len() && needle_idx < needle_chars.len() {
        let hay_char = hay_chars[hay_idx];
        let needle_char = needle_chars[needle_idx];

        if hay_char == needle_char {
            indices.push(hay_idx);

            // Scoring
            let mut char_score: i64 = 1;

            // Bonus for consecutive matches
            if let Some(prev) = prev_match_idx
                && hay_idx == prev + 1
            {
                char_score += 5;
            }

            // Bonus for matching at word boundaries
            if hay_idx == 0 {
                char_score += 3;
            } else if let Some(prev) = hay_idx.checked_sub(1) {
                let prev_char = hay_chars[prev];
                if prev_char == ' ' || prev_char == '_' || prev_char == '-' {
                    char_score += 3;
                }
            }

            // Penalty for skipping characters
            if let Some(prev) = prev_match_idx {
                let gap = hay_idx - prev - 1;
                score -= gap as i64;
            }

            score += char_score;
            prev_match_idx = Some(hay_idx);
            needle_idx += 1;
        }

        hay_idx += 1;
    }

    if needle_idx == needle_chars.len() {
        // All characters matched
        // Bonus for exact matches
        if indices.len() == hay_chars.len() {
            score += 10;
        }
        Some((score, indices))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fuzzy_match_basic() {
        let (score, indices) = fuzzy_match("hello world", "hw").unwrap();
        assert!(score > 0);
        assert_eq!(indices, vec![0, 6]);
    }

    #[test]
    fn test_fuzzy_match_no_match() {
        assert!(fuzzy_match("hello", "xyz").is_none());
    }

    #[test]
    fn test_fuzzy_match_empty() {
        let (score, indices) = fuzzy_match("hello", "").unwrap();
        assert_eq!(score, 0);
        assert!(indices.is_empty());
    }

    #[test]
    fn test_truncate_for_preview() {
        let picker = HistoryPicker::new(&[]);
        assert_eq!(picker.truncate_for_preview("short"), "short");
        let long_text = "a".repeat(PREVIEW_MAX_CHARS + 10);
        let truncated = picker.truncate_for_preview(&long_text);
        assert!(truncated.len() <= PREVIEW_MAX_CHARS + 3);
        assert!(truncated.ends_with("..."));
    }
}
