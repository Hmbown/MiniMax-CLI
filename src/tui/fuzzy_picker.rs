//! Fuzzy file picker for @-path completion
//!
//! Provides an interactive fuzzy finder for selecting files when the user
//! types @ followed by a path pattern.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

/// Maximum number of matches to display
const MAX_VISIBLE_MATCHES: usize = 10;
/// Minimum characters to trigger fuzzy search
const MIN_QUERY_LEN: usize = 1;

/// A fuzzy match result with score
#[derive(Debug, Clone)]
pub struct FuzzyMatch {
    pub path: PathBuf,
    pub score: i64,
    pub highlight_indices: Vec<usize>,
}

/// State for the fuzzy file picker
pub struct FuzzyPicker {
    /// Current search query
    query: String,
    /// All available paths indexed
    paths: Vec<PathBuf>,
    /// Current matches sorted by score
    matches: Vec<FuzzyMatch>,
    /// Currently selected index
    selected: usize,
    /// Whether the picker is active
    active: bool,
    /// Original input before @ trigger
    input_prefix: String,
    /// Cursor position in original input
    cursor_pos: usize,
}

impl FuzzyPicker {
    /// Create a new fuzzy picker with the given workspace
    pub fn new(workspace: &Path) -> Self {
        let paths = index_paths(workspace);
        Self {
            query: String::new(),
            paths,
            matches: Vec::new(),
            selected: 0,
            active: false,
            input_prefix: String::new(),
            cursor_pos: 0,
        }
    }

    /// Check if the picker is currently active
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Activate the picker at the given cursor position
    pub fn activate(&mut self, input: &str, cursor_pos: usize) {
        self.active = true;
        self.input_prefix = input[..cursor_pos].to_string();
        self.cursor_pos = cursor_pos;
        self.query = String::new();
        self.matches.clear();
        self.selected = 0;
        // Show recent/top files initially
        self.update_matches();
    }

    /// Deactivate the picker
    pub fn deactivate(&mut self) {
        self.active = false;
        self.query.clear();
        self.matches.clear();
    }

    /// Get the currently selected path (if any)
    pub fn selected_path(&self) -> Option<&Path> {
        self.matches.get(self.selected).map(|m| m.path.as_path())
    }

    /// Get the formatted selection for inserting into input
    pub fn selection_for_insert(&self) -> Option<String> {
        self.selected_path().map(|p| {
            let path_str = p.to_string_lossy();
            // Use @path format for consistency
            format!("@{}", path_str)
        })
    }

    /// Handle character input
    pub fn insert_char(&mut self, c: char) {
        self.query.push(c);
        self.update_matches();
    }

    /// Handle backspace
    pub fn backspace(&mut self) {
        self.query.pop();
        self.update_matches();
        // Deactivate if query is empty and user continues backspacing
        if self.query.is_empty() {
            // Keep active but show top files
            self.update_matches();
        }
    }

    /// Move selection up
    pub fn select_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        } else if !self.matches.is_empty() {
            self.selected = self.matches.len() - 1;
        }
    }

    /// Move selection down
    pub fn select_down(&mut self) {
        if !self.matches.is_empty() && self.selected < self.matches.len() - 1 {
            self.selected += 1;
        } else {
            self.selected = 0;
        }
    }

    /// Get the updated input after selection
    pub fn apply_selection(&self, current_input: &str) -> Option<String> {
        self.selection_for_insert().map(|selection| {
            // Find the @ trigger position
            let before_at = &current_input[..self.cursor_pos];
            let after_cursor = &current_input[self.cursor_pos..];

            // Remove any partial path after @ in the prefix
            let at_pos = before_at.rfind('@');
            let base = if let Some(pos) = at_pos {
                &before_at[..=pos]
            } else {
                before_at
            };

            format!(
                "{}{}{}",
                base,
                selection.trim_start_matches('@'),
                after_cursor
            )
        })
    }

    /// Update matches based on current query
    fn update_matches(&mut self) {
        if self.query.len() < MIN_QUERY_LEN {
            // Show most recent/common files when no query
            self.matches = self
                .paths
                .iter()
                .take(MAX_VISIBLE_MATCHES * 2)
                .map(|p| FuzzyMatch {
                    path: p.clone(),
                    score: 0,
                    highlight_indices: Vec::new(),
                })
                .collect();
        } else {
            // Fuzzy search
            let query_lower = self.query.to_lowercase();
            let mut scored: Vec<FuzzyMatch> = self
                .paths
                .iter()
                .filter_map(|p| {
                    let path_str = p.to_string_lossy();
                    let path_lower = path_str.to_lowercase();

                    if let Some((score, indices)) = fuzzy_match(&path_lower, &query_lower) {
                        Some(FuzzyMatch {
                            path: p.clone(),
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
            scored.truncate(MAX_VISIBLE_MATCHES);
            self.matches = scored;
        }

        // Reset selection if out of bounds
        if self.selected >= self.matches.len() {
            self.selected = 0;
        }
    }

    /// Refresh the indexed paths
    #[allow(dead_code)]
    pub fn refresh_paths(&mut self, workspace: &Path) {
        self.paths = index_paths(workspace);
        self.update_matches();
    }
}

/// Render the fuzzy picker
pub fn render(f: &mut Frame, picker: &FuzzyPicker, area: Rect) {
    if !picker.is_active() {
        return;
    }

    // Create a centered popup
    let popup_width = (area.width * 4 / 5).clamp(40, 80);
    let popup_height = (MAX_VISIBLE_MATCHES as u16 + 5).min(area.height - 4);
    let popup_x = (area.width - popup_width) / 2;
    let popup_y = (area.height - popup_height) / 2;
    let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

    // Clear the background
    f.render_widget(Clear, popup_area);

    // Draw the border
    let block = Block::default()
        .title(" File Picker ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(crate::palette::MINIMAX_BLUE));
    f.render_widget(block.clone(), popup_area);

    let inner = block.inner(popup_area);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(inner);

    // Query input
    let query_text = if picker.query.is_empty() {
        Line::from(vec![Span::styled(
            "Type to filter...",
            Style::default().fg(crate::palette::TEXT_DIM),
        )])
    } else {
        Line::from(vec![Span::styled(
            format!("@{}", picker.query),
            Style::default().fg(crate::palette::TEXT_PRIMARY),
        )])
    };
    let query = Paragraph::new(query_text)
        .block(Block::default().borders(Borders::BOTTOM))
        .wrap(Wrap { trim: true });
    f.render_widget(query, chunks[0]);

    // Match list
    let items: Vec<ListItem> = picker
        .matches
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let path_str = m.path.to_string_lossy();
            let style = if i == picker.selected {
                Style::default()
                    .bg(crate::palette::MINIMAX_BLUE)
                    .fg(crate::palette::MINIMAX_BLACK)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            // Build highlighted text
            let mut spans = Vec::new();
            let chars: Vec<char> = path_str.chars().collect();
            let mut last_idx = 0;

            for &idx in &m.highlight_indices {
                if idx > last_idx && last_idx < chars.len() {
                    spans.push(Span::styled(
                        chars[last_idx..idx].iter().collect::<String>(),
                        style,
                    ));
                }
                if idx < chars.len() {
                    spans.push(Span::styled(
                        chars[idx].to_string(),
                        style
                            .add_modifier(Modifier::BOLD)
                            .fg(if i == picker.selected {
                                crate::palette::MINIMAX_SNOW
                            } else {
                                crate::palette::MINIMAX_YELLOW
                            }),
                    ));
                    last_idx = idx + 1;
                }
            }

            if last_idx < chars.len() {
                spans.push(Span::styled(
                    chars[last_idx..].iter().collect::<String>(),
                    style,
                ));
            }

            ListItem::new(Line::from(spans))
        })
        .collect();

    let matches_list = List::new(items);
    f.render_widget(matches_list, chunks[1]);
}

/// Simple fuzzy matching algorithm
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
                if prev_char == '/' || prev_char == '\\' || prev_char == '_' || prev_char == '-' {
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

/// Index all files in the workspace
fn index_paths(workspace: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut queue = VecDeque::new();
    queue.push_back(workspace.to_path_buf());

    // Skip these directories
    let skip_dirs: &[&str] = &[
        ".git",
        "node_modules",
        "target",
        ".codex",
        ".aleph",
        "dist",
        "build",
        ".minimax",
    ];

    while let Some(dir) = queue.pop_front() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

                if path.is_dir() {
                    if !skip_dirs.contains(&file_name) && !file_name.starts_with('.') {
                        queue.push_back(path);
                    }
                } else {
                    // Store relative path from workspace
                    if let Ok(rel_path) = path.strip_prefix(workspace) {
                        paths.push(rel_path.to_path_buf());
                    } else {
                        paths.push(path);
                    }
                }
            }
        }
    }

    // Sort by name for consistent ordering
    paths.sort_by(|a, b| a.to_string_lossy().cmp(&b.to_string_lossy()));
    paths
}

/// Check if the input should trigger the fuzzy picker
pub fn should_trigger_picker(input: &str, cursor_pos: usize) -> bool {
    if cursor_pos == 0 {
        return false;
    }

    // Check if we're right after @ or if @ is in the current "word"
    let before_cursor = &input[..cursor_pos];

    // Find the position of the last @ before cursor
    let at_pos = match before_cursor.rfind('@') {
        Some(pos) => pos,
        None => return false,
    };

    // Check if @ is preceded by whitespace or start of string
    // This prevents triggering for email addresses like "email@example.com"
    if at_pos > 0 {
        let char_before_at = before_cursor.chars().nth(at_pos - 1);
        match char_before_at {
            Some(c) if c.is_whitespace() => {} // OK - @ is after whitespace
            None => {}                         // OK - @ is at start
            _ => return false, // Not OK - @ is after a non-whitespace character (like in email)
        }
    }

    // Direct @ trigger (right after @)
    if before_cursor.ends_with('@') {
        return true;
    }

    // Check if we're typing after @ without spaces
    let after_at = &before_cursor[at_pos + 1..];
    // Trigger if no spaces after @
    !after_at.contains(' ')
}

/// Extract the current query after @ for filtering
#[allow(dead_code)]
pub fn extract_query(input: &str, cursor_pos: usize) -> Option<String> {
    let before_cursor = &input[..cursor_pos];

    if let Some(at_pos) = before_cursor.rfind('@') {
        let query = &before_cursor[at_pos + 1..];
        // Only return if no spaces (part of same "word")
        if !query.contains(' ') {
            return Some(query.to_string());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fuzzy_match_basic() {
        let (score, indices) = fuzzy_match("hello", "hl").unwrap();
        assert!(score > 0);
        assert_eq!(indices, vec![0, 2]);
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
    fn test_should_trigger_picker() {
        assert!(should_trigger_picker("Hello @", 7));
        assert!(should_trigger_picker("Check @file", 10));
        assert!(!should_trigger_picker("Hello world", 11));
        assert!(!should_trigger_picker("email@example.com", 17));
    }

    #[test]
    fn test_extract_query() {
        assert_eq!(extract_query("Hello @", 7), Some("".to_string()));
        assert_eq!(extract_query("Check @file", 10), Some("fil".to_string()));
        assert_eq!(extract_query("Hello world", 11), None);
    }
}
