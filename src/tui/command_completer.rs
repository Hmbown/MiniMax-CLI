//! Slash command completion widget
//!
//! Provides an interactive fuzzy finder for selecting slash commands when the user
//! types / followed by a command pattern.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem},
};

use crate::commands::{CommandInfo, COMMANDS, commands_matching};
use crate::palette;
use crate::tui::app::AppMode;

/// Maximum number of matches to display
const MAX_VISIBLE_MATCHES: usize = 8;

/// Maximum edit distance for typo suggestions
const MAX_TYP0_DISTANCE: usize = 2;

/// State for the command completion widget
pub struct CommandCompleter {
    /// Current search query (without the leading /)
    query: String,
    /// Current matches
    matches: Vec<CommandMatch>,
    /// Currently selected index
    selected: usize,
    /// Whether the completer is active
    active: bool,
    /// Current app mode for context-aware suggestions
    mode: AppMode,
}

/// A command match with optional suggestion type
#[derive(Debug, Clone, Copy)]
struct CommandMatch {
    cmd: &'static CommandInfo,
    suggestion_type: SuggestionType,
}

/// Type of suggestion
#[derive(Debug, Clone, Copy, PartialEq)]
enum SuggestionType {
    /// Normal match
    Normal,
    /// Fuzzy/typo correction ("Did you mean?")
    TypoCorrection,
}

impl CommandCompleter {
    /// Create a new command completer
    pub fn new() -> Self {
        Self {
            query: String::new(),
            matches: Vec::new(),
            selected: 0,
            active: false,
            mode: AppMode::Normal,
        }
    }

    /// Check if the completer is currently active
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Activate the completer with the current input
    pub fn activate(&mut self, input: &str) {
        self.active = true;
        // Extract query after the leading /
        self.query = if input.len() > 1 {
            input[1..].to_string()
        } else {
            String::new()
        };
        self.update_matches();
    }

    /// Deactivate the completer
    pub fn deactivate(&mut self) {
        self.active = false;
        self.query.clear();
        self.matches.clear();
    }

    /// Get the currently selected command (if any)
    pub fn selected_command(&self) -> Option<&'static CommandInfo> {
        self.matches.get(self.selected).map(|m| m.cmd)
    }

    /// Get the command name for insertion (with leading /)
    pub fn selection_for_insert(&self) -> Option<String> {
        self.selected_command()
            .map(|cmd| format!("/{} {}", cmd.name, cmd.usage.strip_prefix(&format!("/{}", cmd.name)).unwrap_or("").trim()))
    }

    /// Update the query and refresh matches
    pub fn update_query(&mut self, input: &str) {
        if input.starts_with('/') {
            self.query = if input.len() > 1 {
                input[1..].to_string()
            } else {
                String::new()
            };
            self.update_matches();
        } else {
            self.deactivate();
        }
    }

    /// Set the current app mode for context-aware suggestions
    pub fn set_mode(&mut self, mode: AppMode) {
        if self.mode != mode {
            self.mode = mode;
            if self.active {
                self.update_matches();
            }
        }
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

    /// Get the number of matches
    pub fn match_count(&self) -> usize {
        self.matches.len()
    }

    /// Update matches based on current query
    fn update_matches(&mut self) {
        let mut matches = self.get_contextual_matches();
        
        // Apply fuzzy filtering for better matching
        if !self.query.is_empty() {
            let query_lower = self.query.to_lowercase();
            matches = matches
                .into_iter()
                .filter(|m| {
                    fuzzy_matches(m.cmd.name, &query_lower) 
                        || m.cmd.aliases.iter().any(|a| fuzzy_matches(a, &query_lower))
                        || fuzzy_matches(m.cmd.description, &query_lower)
                })
                .collect();
        }
        
        // If no matches found, try typo correction
        if matches.is_empty() && !self.query.is_empty() && self.query.len() >= 2 {
            matches = self.find_typo_corrections();
        }
        
        // Sort by relevance (exact prefix matches first)
        matches.sort_by(|a, b| {
            let a_name_lower = a.cmd.name.to_lowercase();
            let b_name_lower = b.cmd.name.to_lowercase();
            let query_lower = self.query.to_lowercase();
            
            let a_starts = a_name_lower.starts_with(&query_lower);
            let b_starts = b_name_lower.starts_with(&query_lower);
            
            // First sort by suggestion type (normal before typo)
            match (a.suggestion_type, b.suggestion_type) {
                (SuggestionType::Normal, SuggestionType::TypoCorrection) => std::cmp::Ordering::Less,
                (SuggestionType::TypoCorrection, SuggestionType::Normal) => std::cmp::Ordering::Greater,
                _ => match (a_starts, b_starts) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => a.cmd.name.cmp(b.cmd.name),
                }
            }
        });
        
        matches.truncate(MAX_VISIBLE_MATCHES);
        self.matches = matches;
        
        // Reset selection if out of bounds
        if self.selected >= self.matches.len() {
            self.selected = 0;
        }
    }

    /// Get commands relevant to the current mode (context-aware)
    fn get_contextual_matches(&self) -> Vec<CommandMatch> {
        commands_matching(&self.query)
            .into_iter()
            .filter(|cmd| {
                // In RLM mode, prioritize RLM-specific commands
                if self.mode == AppMode::Rlm {
                    return true; // Show all commands in RLM mode
                }
                
                // In other modes, deprioritize RLM-specific commands
                let rlm_specific = ["status", "save-session", "save_session", "repl"];
                if rlm_specific.contains(&cmd.name) {
                    // Still show them but they'll be sorted lower if not explicitly searched
                    return true;
                }
                
                true
            })
            .map(|cmd| CommandMatch { cmd, suggestion_type: SuggestionType::Normal })
            .collect()
    }

    /// Find typo corrections using edit distance
    fn find_typo_corrections(&self) -> Vec<CommandMatch> {
        let query_lower = self.query.to_lowercase();
        
        COMMANDS
            .iter()
            .filter_map(|cmd| {
                let dist = edit_distance(&query_lower, &cmd.name.to_lowercase());
                if dist <= MAX_TYP0_DISTANCE && dist > 0 {
                    Some(CommandMatch {
                        cmd,
                        suggestion_type: SuggestionType::TypoCorrection,
                    })
                } else {
                    // Also check aliases
                    for alias in cmd.aliases {
                        let alias_dist = edit_distance(&query_lower, &alias.to_lowercase());
                        if alias_dist <= MAX_TYP0_DISTANCE && alias_dist > 0 {
                            return Some(CommandMatch {
                                cmd,
                                suggestion_type: SuggestionType::TypoCorrection,
                            });
                        }
                    }
                    None
                }
            })
            .collect()
    }

    /// Check if any matches are typo corrections
    fn has_typo_corrections(&self) -> bool {
        self.matches.iter().any(|m| m.suggestion_type == SuggestionType::TypoCorrection)
    }
}

impl Default for CommandCompleter {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple fuzzy matching - checks if all chars in query appear in order
fn fuzzy_matches(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    
    let hay_lower = haystack.to_lowercase();
    let mut hay_chars = hay_lower.chars();
    
    for needle_char in needle.chars() {
        let mut found = false;
        for hay_char in hay_chars.by_ref() {
            if hay_char == needle_char {
                found = true;
                break;
            }
        }
        if !found {
            return false;
        }
    }
    true
}

/// Calculate Levenshtein edit distance between two strings
fn edit_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let a_len = a_chars.len();
    let b_len = b_chars.len();
    
    if a_len == 0 {
        return b_len;
    }
    if b_len == 0 {
        return a_len;
    }
    
    // Use a simple dynamic programming approach
    let mut prev_row: Vec<usize> = (0..=b_len).collect();
    let mut curr_row = vec![0; b_len + 1];
    
    for i in 1..=a_len {
        curr_row[0] = i;
        for j in 1..=b_len {
            let cost = if a_chars[i - 1] == b_chars[j - 1] { 0 } else { 1 };
            curr_row[j] = (prev_row[j] + 1)
                .min(curr_row[j - 1] + 1)
                .min(prev_row[j - 1] + cost);
        }
        std::mem::swap(&mut prev_row, &mut curr_row);
    }
    
    prev_row[b_len]
}

/// Render the command completer below the input line
pub fn render(f: &mut Frame, completer: &CommandCompleter, area: Rect) {
    if !completer.is_active() || completer.matches.is_empty() {
        return;
    }

    let match_count = completer.matches.len() as u16;
    let popup_height = match_count.min(MAX_VISIBLE_MATCHES as u16) + 2; // +2 for border
    
    // Position below the input line
    let popup_area = Rect {
        x: area.x,
        y: area.y.saturating_sub(popup_height),
        width: area.width.min(70),
        height: popup_height,
    };

    // Clear the background
    f.render_widget(Clear, popup_area);

    // Build title with "Did you mean?" indicator if applicable
    let title = if completer.has_typo_corrections() {
        format!(" Commands ({}/{}) - Did you mean? ", completer.selected + 1, completer.match_count())
    } else {
        format!(" Commands ({}/{}) ", completer.selected + 1, completer.match_count())
    };

    // Draw the border
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(palette::MINIMAX_BLUE));
    f.render_widget(block.clone(), popup_area);

    let inner = block.inner(popup_area);

    // Build list items
    let items: Vec<ListItem> = completer
        .matches
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let is_selected = i == completer.selected;
            let cmd = m.cmd;
            
            // Base style
            let base_style = if is_selected {
                Style::default()
                    .bg(palette::MINIMAX_BLUE)
                    .fg(palette::MINIMAX_BLACK)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(palette::TEXT_PRIMARY)
            };

            let mut spans = Vec::new();

            // Add "Did you mean?" indicator for typo corrections
            if m.suggestion_type == SuggestionType::TypoCorrection && !is_selected {
                spans.push(Span::styled(
                    "💡 ",
                    Style::default().fg(palette::MINIMAX_ORANGE),
                ));
            }

            // Build the line with command name, aliases, and description
            spans.push(Span::styled(
                format!("/{}", cmd.name),
                base_style.add_modifier(Modifier::BOLD),
            ));

            // Add example usage inline
            let usage_example = cmd.usage
                .strip_prefix(&format!("/{}", cmd.name))
                .unwrap_or("")
                .trim();
            if !usage_example.is_empty() && !usage_example.starts_with('<') {
                // Show the example directly after command name
                spans.push(Span::styled(
                    format!(" {}", usage_example.split_whitespace().next().unwrap_or("")),
                    if is_selected {
                        base_style.fg(palette::MINIMAX_SLATE)
                    } else {
                        base_style.fg(palette::MINIMAX_BLUE)
                    },
                ));
            }

            // Add aliases if any (compact)
            if !cmd.aliases.is_empty() && is_selected {
                let alias_text = cmd.aliases.join(", ");
                spans.push(Span::styled(
                    format!(" ({alias_text})"),
                    base_style.fg(palette::MINIMAX_SLATE),
                ));
            }

            // Add description
            spans.push(Span::styled(
                format!(" - {}", cmd.description),
                if is_selected {
                    base_style
                } else {
                    base_style.fg(palette::TEXT_MUTED)
                },
            ));

            ListItem::new(Line::from(spans))
        })
        .collect();

    let matches_list = List::new(items);
    f.render_widget(matches_list, inner);
}

/// Check if the input should trigger the command completer
pub fn should_trigger_completer(input: &str, cursor_pos: usize) -> bool {
    // Must start with /
    if !input.starts_with('/') {
        return false;
    }
    
    // Don't trigger if there's a space (user is typing arguments)
    if input.contains(' ') {
        return false;
    }
    
    // Trigger if we're at the beginning or typing after /
    cursor_pos > 0
}

/// Get a hint for a specific command prefix (e.g., "/model ")
pub fn get_command_hint(input: &str) -> Option<&'static str> {
    let trimmed = input.trim_start();
    
    if trimmed.starts_with("/model ") {
        let after_model = trimmed.strip_prefix("/model ").unwrap_or("").trim();
        if after_model.is_empty() {
            return Some("Available: MiniMax-M2.1, MiniMax-Text-01, gemini-2.5-flash, claude-sonnet-4-20250514");
        }
    }
    
    if trimmed.starts_with("/mode ") {
        let after_mode = trimmed.strip_prefix("/mode ").unwrap_or("").trim();
        if after_mode.is_empty() {
            return Some("Options: normal, agent, yolo, rlm, duo, plan");
        }
    }
    
    if trimmed.starts_with("/set ") {
        let after_set = trimmed.strip_prefix("/set ").unwrap_or("").trim();
        if after_set.is_empty() {
            return Some("Keys: auto_compact, show_thinking, show_tool_details, theme, default_model");
        }
    }
    
    if trimmed.starts_with("/snippet ") {
        let after_snippet = trimmed.strip_prefix("/snippet ").unwrap_or("").trim();
        if after_snippet.is_empty() {
            return Some("Available: review, explain, test, doc, refactor, optimize");
        }
    }
    
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fuzzy_matches_basic() {
        assert!(fuzzy_matches("help", "h"));
        assert!(fuzzy_matches("help", "he"));
        assert!(fuzzy_matches("help", "hel"));
        assert!(fuzzy_matches("help", "help"));
        assert!(fuzzy_matches("help", "hp")); // h...p
        assert!(fuzzy_matches("clear", "cl"));
    }

    #[test]
    fn test_fuzzy_matches_no_match() {
        assert!(!fuzzy_matches("help", "x"));
        assert!(!fuzzy_matches("help", "xyz"));
        assert!(!fuzzy_matches("help", "hlz")); // z not in help
    }

    #[test]
    fn test_fuzzy_matches_empty() {
        assert!(fuzzy_matches("help", ""));
    }

    #[test]
    fn test_should_trigger_completer() {
        assert!(should_trigger_completer("/help", 5));
        assert!(should_trigger_completer("/h", 2));
        assert!(should_trigger_completer("/", 1));
        assert!(!should_trigger_completer("/help arg", 9)); // has space
        assert!(!should_trigger_completer("hello", 5)); // no leading /
    }

    #[test]
    fn test_completer_activation() {
        let mut completer = CommandCompleter::new();
        completer.activate("/he");
        assert!(completer.is_active());
        assert_eq!(completer.query, "he");
    }

    #[test]
    fn test_completer_selection() {
        let mut completer = CommandCompleter::new();
        completer.activate("/he");
        
        // Should have at least 'help' matching
        assert!(!completer.matches.is_empty());
        
        // Test navigation
        let first = completer.selected;
        completer.select_down();
        // May wrap or stay depending on match count
        
        completer.select_up();
        assert_eq!(completer.selected, first);
    }

    #[test]
    fn test_completer_insert_char() {
        let mut completer = CommandCompleter::new();
        completer.activate("/h");
        completer.insert_char('e');
        assert_eq!(completer.query, "he");
    }

    #[test]
    fn test_completer_backspace() {
        let mut completer = CommandCompleter::new();
        completer.activate("/he");
        completer.backspace();
        assert_eq!(completer.query, "h");
    }

    #[test]
    fn test_selection_for_insert() {
        let mut completer = CommandCompleter::new();
        completer.activate("/help");
        
        if let Some(cmd) = completer.selected_command() {
            assert_eq!(cmd.name, "help");
            assert!(completer.selection_for_insert().unwrap().starts_with("/help"));
        }
    }

    #[test]
    fn test_edit_distance() {
        assert_eq!(edit_distance("", ""), 0);
        assert_eq!(edit_distance("a", ""), 1);
        assert_eq!(edit_distance("", "a"), 1);
        assert_eq!(edit_distance("help", "help"), 0);
        assert_eq!(edit_distance("help", "hepl"), 2); // swap
        assert_eq!(edit_distance("help", "hel"), 1); // deletion
        assert_eq!(edit_distance("help", "helpp"), 1); // insertion
        assert_eq!(edit_distance("help", "h3lp"), 1); // substitution
    }

    #[test]
    fn test_typo_correction() {
        let mut completer = CommandCompleter::new();
        completer.activate("/hepl"); // typo for "help"
        
        // Should find "help" as a typo correction
        assert!(completer.matches.iter().any(|m| m.cmd.name == "help"));
    }

    #[test]
    fn test_command_hints() {
        assert!(get_command_hint("/model ").is_some());
        assert!(get_command_hint("/mode ").is_some());
        assert!(get_command_hint("/set ").is_some());
        assert!(get_command_hint("/help ").is_none());
        assert!(get_command_hint("hello").is_none());
    }
}
