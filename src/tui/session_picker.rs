//! Interactive session picker for switching between saved sessions.
//!
//! Provides a fuzzy-searchable list of sessions with metadata display.

use crate::palette;
use crate::session_manager::{SessionManager, SessionMetadata};
use crate::tui::views::{ModalKind, ModalView, ViewAction, ViewEvent};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    prelude::Widget,
    style::{Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};

/// Maximum number of sessions to display at once
const MAX_VISIBLE_SESSIONS: usize = 10;

/// Result of a session selection
#[derive(Debug, Clone)]
pub enum SessionPickerResult {
    /// User selected a session
    Selected(String),
    /// User cancelled
    Cancelled,
}

/// A session match result with score for fuzzy filtering
#[derive(Debug, Clone)]
struct SessionMatch {
    metadata: SessionMetadata,
    score: i64,
    highlight_indices: Vec<usize>,
}

/// Interactive picker for selecting a saved session
pub struct SessionPicker {
    /// All available sessions
    sessions: Vec<SessionMetadata>,
    /// Current filtered matches
    matches: Vec<SessionMatch>,
    /// Current search query
    query: String,
    /// Currently selected index
    selected: usize,
    /// ID of the currently active session (to highlight)
    current_session_id: Option<String>,
    /// Whether the picker is actively filtering
    filtering: bool,
}

impl SessionPicker {
    /// Create a new session picker
    pub fn new(current_session_id: Option<String>) -> Self {
        let sessions = load_sessions();
        let matches = sessions
            .iter()
            .map(|s| SessionMatch {
                metadata: s.clone(),
                score: 0,
                highlight_indices: Vec::new(),
            })
            .collect();

        Self {
            sessions,
            matches,
            query: String::new(),
            selected: 0,
            current_session_id,
            filtering: false,
        }
    }

    /// Get the currently selected session ID (if any)
    pub fn selected_session_id(&self) -> Option<String> {
        self.matches.get(self.selected).map(|m| m.metadata.id.clone())
    }

    /// Check if a session is the currently active one
    fn is_current_session(&self, id: &str) -> bool {
        self.current_session_id
            .as_ref()
            .map_or(false, |current| current.starts_with(id) || id.starts_with(current))
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
            // Show all sessions sorted by most recent
            self.matches = self
                .sessions
                .iter()
                .map(|s| SessionMatch {
                    metadata: s.clone(),
                    score: 0,
                    highlight_indices: Vec::new(),
                })
                .collect();
        } else {
            // Fuzzy filter
            let query_lower = self.query.to_lowercase();
            let mut scored: Vec<SessionMatch> = self
                .sessions
                .iter()
                .filter_map(|s| {
                    let title_lower = s.title.to_lowercase();
                    if let Some((score, indices)) = fuzzy_match(&title_lower, &query_lower) {
                        Some(SessionMatch {
                            metadata: s.clone(),
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
    fn format_age(&self, dt: &chrono::DateTime<chrono::Utc>) -> String {
        let now = chrono::Utc::now();
        let duration = now.signed_duration_since(*dt);

        if duration.num_minutes() < 1 {
            "just now".to_string()
        } else if duration.num_hours() < 1 {
            format!("{}m ago", duration.num_minutes())
        } else if duration.num_days() < 1 {
            format!("{}h ago", duration.num_hours())
        } else if duration.num_weeks() < 1 {
            format!("{}d ago", duration.num_days())
        } else {
            format!("{}w ago", duration.num_weeks())
        }
    }

    /// Render a session item
    fn render_session_item(&self, session_match: &SessionMatch, index: usize) -> ListItem {
        let meta = &session_match.metadata;
        let is_selected = index == self.selected;
        let is_current = self.is_current_session(&meta.id);

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

        // Build the title with optional highlighting
        let title_spans = if self.filtering && !session_match.highlight_indices.is_empty() {
            self.build_highlighted_title(&meta.title, session_match, base_style)
        } else {
            vec![Span::styled(meta.title.clone(), base_style)]
        };

        // Current indicator
        let current_indicator = if is_current { " ● " } else { "   " };

        // Format metadata line
        let age = self.format_age(&meta.updated_at);
        let meta_text = format!(
            "{} | {} msgs | {}",
            &meta.id[..8.min(meta.id.len())],
            meta.message_count,
            age
        );
        let meta_style = if is_selected {
            Style::default().fg(palette::MINIMAX_SILVER)
        } else {
            Style::default().fg(palette::TEXT_DIM)
        };

        let line = Line::from(vec![
            Span::styled(
                current_indicator,
                if is_current {
                    Style::default()
                        .fg(palette::MINIMAX_ORANGE)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                },
            ),
            Span::styled(" ", base_style),
        ]);

        let mut lines = vec![line];

        // Title line
        let mut title_line = Line::default();
        title_line.push_span(Span::styled("   ", base_style)); // Indent
        for span in title_spans {
            title_line.push_span(span);
        }
        if is_current {
            title_line.push_span(Span::styled(
                " (current)",
                if is_selected {
                    Style::default().fg(palette::MINIMAX_ORANGE)
                } else {
                    Style::default().fg(palette::TEXT_DIM)
                },
            ));
        }
        lines.push(title_line);

        // Meta line
        lines.push(Line::from(vec![
            Span::styled("   ", base_style),
            Span::styled(meta_text, meta_style),
        ]));

        // Spacing between items
        lines.push(Line::from(""));

        ListItem::new(lines)
    }

    /// Build highlighted title spans
    fn build_highlighted_title(
        &self,
        title: &str,
        session_match: &SessionMatch,
        base_style: Style,
    ) -> Vec<Span> {
        let mut spans = Vec::new();
        let chars: Vec<char> = title.chars().collect();
        let mut last_idx = 0;

        for &idx in &session_match.highlight_indices {
            if idx > last_idx && last_idx < chars.len() {
                spans.push(Span::styled(
                    chars[last_idx..idx].iter().collect::<String>(),
                    base_style,
                ));
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
            spans.push(Span::styled(
                chars[last_idx..].iter().collect::<String>(),
                base_style,
            ));
        }

        spans
    }
}

impl ModalView for SessionPicker {
    fn kind(&self) -> ModalKind {
        ModalKind::SessionPicker
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn handle_key(&mut self, key: KeyEvent) -> ViewAction {
        match key.code {
            KeyCode::Esc => ViewAction::EmitAndClose(ViewEvent::SessionPickerResult {
                result: SessionPickerResult::Cancelled,
            }),
            KeyCode::Enter => {
                if let Some(id) = self.selected_session_id() {
                    ViewAction::EmitAndClose(ViewEvent::SessionPickerResult {
                        result: SessionPickerResult::Selected(id),
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
            KeyCode::Char(c) if !key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
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
        let popup_width = (area.width * 4 / 5).min(80).max(50);
        let popup_height = (MAX_VISIBLE_SESSIONS as u16 * 3 + 8).min(area.height - 4);
        let popup_x = (area.width - popup_width) / 2;
        let popup_y = (area.height - popup_height) / 2;
        let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

        // Clear the background
        Clear.render(popup_area, buf);

        // Draw the border
        let block = Block::default()
            .title(" Session Picker ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(palette::MINIMAX_BLUE));
        let inner = block.inner(popup_area);
        block.render(popup_area, buf);
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1), Constraint::Length(1)])
            .split(inner);

        // Query input
        let query_text = if self.query.is_empty() {
            Line::from(vec![Span::styled(
                "Type to filter sessions...",
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

        // Session list
        let items: Vec<ListItem> = self
            .matches
            .iter()
            .take(MAX_VISIBLE_SESSIONS)
            .enumerate()
            .map(|(i, m)| self.render_session_item(m, i))
            .collect();

        let sessions_list = List::new(items);
        sessions_list.render(chunks[1], buf);

        // Help footer
        let help_text = if self.matches.is_empty() {
            "No sessions found".to_string()
        } else {
            format!(
                "↑/↓ to navigate | Enter to select | Esc to cancel | {} sessions",
                self.matches.len()
            )
        };
        let help = Paragraph::new(Line::from(vec![Span::styled(
            help_text,
            Style::default().fg(palette::TEXT_DIM),
        )]));
        help.render(chunks[2], buf);
    }
}

/// Load all sessions from the default location
fn load_sessions() -> Vec<SessionMetadata> {
    SessionManager::default_location()
        .and_then(|m| m.list_sessions())
        .unwrap_or_default()
}

/// Simple fuzzy matching algorithm (same as fuzzy_picker.rs)
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
            if let Some(prev) = prev_match_idx {
                if hay_idx == prev + 1 {
                    char_score += 5;
                }
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
    fn test_format_age() {
        let picker = SessionPicker::new(None);
        let now = chrono::Utc::now();
        assert_eq!(picker.format_age(&now), "just now");

        let hour_ago = now - chrono::Duration::hours(2);
        assert_eq!(picker.format_age(&hour_ago), "2h ago");

        let day_ago = now - chrono::Duration::days(3);
        assert_eq!(picker.format_age(&day_ago), "3d ago");
    }
}
