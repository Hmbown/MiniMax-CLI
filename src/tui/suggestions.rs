//! Smart command suggestions based on context
//!
//! Provides contextual suggestions to help users discover features and recover from errors.
//! Suggestions are non-intrusive, actionable, and learn from user dismissals.

use std::collections::HashSet;
use std::time::{Duration, Instant};

/// A suggestion to display to the user
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    /// The suggestion text to display
    pub text: String,
    /// Optional hint about the action (e.g., "Press /compact")
    pub action_hint: Option<String>,
    /// Priority level for display ordering
    pub priority: SuggestionPriority,
    /// Unique identifier for tracking dismissals
    pub id: String,
    /// Time when the suggestion was created
    pub created_at: Instant,
}

/// Priority levels for suggestions
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SuggestionPriority {
    /// Low priority - general tips
    Low,
    /// Medium priority - contextual hints
    Medium,
    /// High priority - warnings and errors
    High,
    /// Critical - immediate action needed
    Critical,
}

impl Suggestion {
    /// Create a new suggestion
    pub fn new(id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            action_hint: None,
            priority: SuggestionPriority::Medium,
            id: id.into(),
            created_at: Instant::now(),
        }
    }

    /// Set the action hint
    pub fn with_action_hint(mut self, hint: impl Into<String>) -> Self {
        self.action_hint = Some(hint.into());
        self
    }

    /// Set the priority
    pub fn with_priority(mut self, priority: SuggestionPriority) -> Self {
        self.priority = priority;
        self
    }

    /// Check if the suggestion has expired (auto-hide after duration)
    pub fn is_expired(&self, duration: Duration) -> bool {
        self.created_at.elapsed() > duration
    }

    /// Get the full display text including action hint
    pub fn display_text(&self) -> String {
        match &self.action_hint {
            Some(hint) => format!("{} ({})", self.text, hint),
            None => self.text.clone(),
        }
    }
}

/// Tracks dismissed suggestion IDs with cooldown
#[derive(Debug)]
struct DismissalTracker {
    /// Set of dismissed suggestion IDs
    dismissed: HashSet<String>,
    /// Last dismissal time for each ID
    dismissal_times: std::collections::HashMap<String, Instant>,
    /// Cooldown duration before a dismissed suggestion can reappear
    cooldown: Duration,
}

impl DismissalTracker {
    fn new(cooldown: Duration) -> Self {
        Self {
            dismissed: HashSet::new(),
            dismissal_times: std::collections::HashMap::new(),
            cooldown,
        }
    }

    fn dismiss(&mut self, id: &str) {
        self.dismissed.insert(id.to_string());
        self.dismissal_times.insert(id.to_string(), Instant::now());
    }

    fn is_dismissed(&self, id: &str) -> bool {
        if !self.dismissed.contains(id) {
            return false;
        }

        // Check if cooldown has expired
        if let Some(time) = self.dismissal_times.get(id) {
            if time.elapsed() > self.cooldown {
                return false; // Cooldown expired, no longer dismissed
            }
        }

        true
    }

    fn clean_expired(&mut self) {
        let expired: Vec<String> = self
            .dismissal_times
            .iter()
            .filter(|(_, time)| time.elapsed() > self.cooldown)
            .map(|(id, _)| id.clone())
            .collect();

        for id in expired {
            self.dismissed.remove(&id);
            self.dismissal_times.remove(&id);
        }
    }
}

/// Token threshold for context size warnings (70%)
const CONTEXT_WARNING_THRESHOLD: f32 = 0.7;
/// Default auto-hide duration for suggestions
const DEFAULT_AUTO_HIDE_DURATION: Duration = Duration::from_secs(5);
/// Cooldown duration for dismissed suggestions
const DISMISSAL_COOLDOWN: Duration = Duration::from_secs(300); // 5 minutes

/// Engine for generating and managing contextual suggestions
#[derive(Debug)]
pub struct SuggestionEngine {
    /// Currently active suggestion
    current: Option<Suggestion>,
    /// Track dismissed suggestions
    dismissal_tracker: DismissalTracker,
    /// Auto-hide duration
    auto_hide_duration: Duration,
    /// Last error message for error-based suggestions
    last_error: Option<String>,
    /// Whether YOLO mode warning has been shown this session
    yolo_warning_shown: bool,
    /// Input history for pattern detection
    input_history: Vec<String>,
    /// File-related keywords to detect
    file_keywords: Vec<&'static str>,
    /// Context size threshold (default: 200k tokens)
    token_limit: u32,
}

impl SuggestionEngine {
    /// Create a new suggestion engine
    pub fn new() -> Self {
        Self {
            current: None,
            dismissal_tracker: DismissalTracker::new(DISMISSAL_COOLDOWN),
            auto_hide_duration: DEFAULT_AUTO_HIDE_DURATION,
            last_error: None,
            yolo_warning_shown: false,
            input_history: Vec::new(),
            file_keywords: vec![
                "file",
                "read",
                "write",
                "edit",
                "open",
                "path",
                "folder",
                "directory",
                "config",
                "json",
                "toml",
                "yaml",
                "md",
                "rs",
                "py",
                "js",
                "ts",
            ],
            token_limit: 200_000,
        }
    }

    /// Get the current suggestion if any
    pub fn current(&self) -> Option<&Suggestion> {
        self.current.as_ref()
    }

    /// Check if there's an active suggestion
    pub fn has_suggestion(&self) -> bool {
        self.current.is_some()
    }

    /// Dismiss the current suggestion
    pub fn dismiss_current(&mut self) {
        if let Some(suggestion) = self.current.take() {
            self.dismissal_tracker.dismiss(&suggestion.id);
        }
    }

    /// Update the engine and check for expired suggestions
    pub fn tick(&mut self) {
        self.dismissal_tracker.clean_expired();

        // Check if current suggestion should auto-hide
        if let Some(suggestion) = &self.current {
            if suggestion.is_expired(self.auto_hide_duration) {
                self.current = None;
            }
        }
    }

    /// Check context size and suggest compaction if needed
    pub fn check_context_size(&mut self, current_tokens: u32) {
        if self.dismissal_tracker.is_dismissed("context_size") {
            return;
        }

        let threshold = (self.token_limit as f32 * CONTEXT_WARNING_THRESHOLD) as u32;
        if current_tokens > threshold {
            let percentage = (current_tokens as f32 / self.token_limit as f32 * 100.0) as u32;
            self.set_suggestion(
                Suggestion::new(
                    "context_size",
                    format!("Context at {}% capacity", percentage),
                )
                .with_action_hint("Type /compact to free up space")
                .with_priority(SuggestionPriority::High),
            );
        }
    }

    /// Check last error and suggest recovery command
    pub fn check_last_error(&mut self, error: &str) {
        if self.last_error.as_deref() == Some(error) {
            return;
        }
        self.last_error = Some(error.to_string());

        // Workspace boundary error
        if error.contains("workspace")
            && (error.contains("boundary") || error.contains("outside") || error.contains("trust"))
        {
            if !self.dismissal_tracker.is_dismissed("workspace_boundary") {
                self.set_suggestion(
                    Suggestion::new(
                        "workspace_boundary",
                        "File operation blocked by workspace boundary",
                    )
                    .with_action_hint("Use /trust to enable access or /yolo for full access")
                    .with_priority(SuggestionPriority::Critical),
                );
            }
            return;
        }

        // Permission denied error
        if error.contains("permission") || error.contains("denied") {
            if !self.dismissal_tracker.is_dismissed("permission_error") {
                self.set_suggestion(
                    Suggestion::new("permission_error", "Permission denied")
                        .with_action_hint("Check file permissions or use /trust")
                        .with_priority(SuggestionPriority::High),
                );
            }
            return;
        }

        // Network/API error
        if error.contains("network")
            || error.contains("connection")
            || error.contains("timeout")
            || error.contains("API")
        {
            if !self.dismissal_tracker.is_dismissed("network_error") {
                self.set_suggestion(
                    Suggestion::new("network_error", "Network or API error occurred")
                        .with_action_hint("Check connection and retry your request")
                        .with_priority(SuggestionPriority::High),
                );
            }
            return;
        }

        // Generic error recovery suggestion
        if !self.dismissal_tracker.is_dismissed("generic_error") {
            self.set_suggestion(
                Suggestion::new("generic_error", "An error occurred")
                    .with_action_hint("Use /clear to reset or retry your request")
                    .with_priority(SuggestionPriority::Medium),
            );
        }
    }

    /// Check input for file-related patterns and suggest @ usage
    pub fn check_input_pattern(&mut self, input: &str) {
        if input.is_empty() {
            return;
        }

        // Check for command-specific inline help
        let trimmed = input.trim_start();

        // /model command - show model suggestions
        if trimmed == "/model " || trimmed.starts_with("/model ") {
            let after_cmd = trimmed.strip_prefix("/model ").unwrap_or("").trim();
            if after_cmd.is_empty() {
                self.set_suggestion(
                    Suggestion::new(
                        "model_suggestions",
                        "Available: MiniMax-M2.1, MiniMax-Text-01, gemini-2.5-flash",
                    )
                    .with_action_hint("Type model name or press Enter for picker")
                    .with_priority(SuggestionPriority::Low),
                );
            }
            return;
        }

        // /mode command - show mode options
        if trimmed == "/mode " || trimmed.starts_with("/mode ") {
            let after_cmd = trimmed.strip_prefix("/mode ").unwrap_or("").trim();
            if after_cmd.is_empty() {
                self.set_suggestion(
                    Suggestion::new(
                        "mode_suggestions",
                        "Options: normal, agent, yolo, rlm, duo, plan",
                    )
                    .with_action_hint("Type mode name or Tab to cycle")
                    .with_priority(SuggestionPriority::Low),
                );
            }
            return;
        }

        // /set command - show setting keys
        if trimmed == "/set " || trimmed.starts_with("/set ") {
            let after_cmd = trimmed.strip_prefix("/set ").unwrap_or("").trim();
            if after_cmd.is_empty() {
                self.set_suggestion(Suggestion::new(
                    "set_suggestions",
                    "Keys: auto_compact, show_thinking, show_tool_details, theme, default_model",
                )
                .with_action_hint("Type key name")
                .with_priority(SuggestionPriority::Low));
            }
            return;
        }

        // Skip if already using @ syntax
        if input.contains('@') {
            return;
        }

        // Skip if it's a command (other than the ones handled above)
        if input.starts_with('/') {
            return;
        }

        if self.dismissal_tracker.is_dismissed("file_suggestion") {
            return;
        }

        // Check for file-related keywords
        let input_lower = input.to_lowercase();
        for keyword in &self.file_keywords {
            if input_lower.contains(keyword) {
                self.set_suggestion(
                    Suggestion::new("file_suggestion", "Use @filename for file paths")
                        .with_action_hint("Type @ followed by a filename for auto-completion")
                        .with_priority(SuggestionPriority::Low),
                );
                return;
            }
        }
    }

    /// Check if user just entered YOLO mode for the first time
    pub fn check_yolo_mode(&mut self, in_yolo_mode: bool) {
        if !in_yolo_mode {
            self.yolo_warning_shown = false;
            return;
        }

        if self.yolo_warning_shown {
            return;
        }

        if self.dismissal_tracker.is_dismissed("yolo_warning") {
            return;
        }

        self.yolo_warning_shown = true;
        self.set_suggestion(
            Suggestion::new("yolo_warning", "YOLO mode enabled - tools auto-approve")
                .with_action_hint("Press Tab to switch modes, /trust to control access")
                .with_priority(SuggestionPriority::High),
        );
    }

    /// Check if a long operation is running and suggest cancellation
    pub fn check_long_operation(&mut self, is_running: bool, elapsed_secs: Option<u64>) {
        if !is_running {
            return;
        }

        let Some(elapsed) = elapsed_secs else {
            return;
        };

        // Only show after 10 seconds of running
        if elapsed < 10 {
            return;
        }

        if self.dismissal_tracker.is_dismissed("long_operation") {
            return;
        }

        self.set_suggestion(
            Suggestion::new("long_operation", "Operation running for a while")
                .with_action_hint("Press Ctrl+C or Esc to cancel")
                .with_priority(SuggestionPriority::Medium),
        );
    }

    /// Check for RLM mode specific suggestions
    pub fn check_rlm_context(&mut self, has_context: bool, is_rlm_mode: bool) {
        if !is_rlm_mode {
            return;
        }

        if has_context {
            return;
        }

        if self.dismissal_tracker.is_dismissed("rlm_no_context") {
            return;
        }

        self.set_suggestion(
            Suggestion::new("rlm_no_context", "RLM mode has no context loaded")
                .with_action_hint("Type @filename to load a file, or chat to use model normally")
                .with_priority(SuggestionPriority::Medium),
        );
    }

    /// Set a new suggestion, replacing the current one if higher priority
    fn set_suggestion(&mut self, suggestion: Suggestion) {
        // Don't show if recently dismissed
        if self.dismissal_tracker.is_dismissed(&suggestion.id) {
            return;
        }

        // Replace if no current suggestion or new one has higher priority
        let should_replace = match &self.current {
            None => true,
            Some(current) => suggestion.priority > current.priority,
        };

        if should_replace {
            self.current = Some(suggestion);
        }
    }

    /// Clear the current suggestion
    pub fn clear(&mut self) {
        self.current = None;
    }

    /// Mark a specific suggestion as dismissed
    #[allow(dead_code)]
    pub fn dismiss(&mut self, id: &str) {
        self.dismissal_tracker.dismiss(id);
        if let Some(current) = &self.current {
            if current.id == id {
                self.current = None;
            }
        }
    }

    /// Set the auto-hide duration
    #[allow(dead_code)]
    pub fn set_auto_hide_duration(&mut self, duration: Duration) {
        self.auto_hide_duration = duration;
    }

    /// Set the token limit
    #[allow(dead_code)]
    pub fn set_token_limit(&mut self, limit: u32) {
        self.token_limit = limit;
    }

    /// Record input for history-based suggestions
    pub fn record_input(&mut self, input: &str) {
        if input.starts_with('/') {
            return; // Don't record commands
        }

        self.input_history.push(input.to_string());
        if self.input_history.len() > 10 {
            self.input_history.remove(0);
        }
    }
}

impl Default for SuggestionEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_suggestion_creation() {
        let suggestion = Suggestion::new("test", "Test message")
            .with_action_hint("Press X")
            .with_priority(SuggestionPriority::High);

        assert_eq!(suggestion.id, "test");
        assert_eq!(suggestion.text, "Test message");
        assert_eq!(suggestion.action_hint, Some("Press X".to_string()));
        assert_eq!(suggestion.priority, SuggestionPriority::High);
    }

    #[test]
    fn test_suggestion_display_text() {
        let suggestion = Suggestion::new("test", "Test message").with_action_hint("Press X");

        assert_eq!(suggestion.display_text(), "Test message (Press X)");
    }

    #[test]
    fn test_dismissal_tracking() {
        let mut tracker = DismissalTracker::new(Duration::from_secs(60));

        assert!(!tracker.is_dismissed("test"));
        tracker.dismiss("test");
        assert!(tracker.is_dismissed("test"));
    }

    #[test]
    fn test_context_size_suggestion() {
        let mut engine = SuggestionEngine::new();
        engine.check_context_size(150_000); // 75% of 200k

        let suggestion = engine.current();
        assert!(suggestion.is_some());
        assert_eq!(suggestion.unwrap().id, "context_size");
    }

    #[test]
    fn test_context_size_no_suggestion_below_threshold() {
        let mut engine = SuggestionEngine::new();
        engine.check_context_size(100_000); // 50% of 200k

        assert!(engine.current().is_none());
    }

    #[test]
    fn test_workspace_boundary_suggestion() {
        let mut engine = SuggestionEngine::new();
        engine.check_last_error("Operation failed: file outside workspace boundary");

        let suggestion = engine.current();
        assert!(suggestion.is_some());
        assert_eq!(suggestion.unwrap().id, "workspace_boundary");
    }

    #[test]
    fn test_file_pattern_suggestion() {
        let mut engine = SuggestionEngine::new();
        engine.check_input_pattern("read the config file please");

        let suggestion = engine.current();
        assert!(suggestion.is_some());
        assert_eq!(suggestion.unwrap().id, "file_suggestion");
    }

    #[test]
    fn test_file_pattern_no_suggestion_with_at() {
        let mut engine = SuggestionEngine::new();
        engine.check_input_pattern("read @config.toml please");

        assert!(engine.current().is_none());
    }

    #[test]
    fn test_yolo_mode_suggestion() {
        let mut engine = SuggestionEngine::new();
        engine.check_yolo_mode(true);

        let suggestion = engine.current();
        assert!(suggestion.is_some());
        assert_eq!(suggestion.unwrap().id, "yolo_warning");
    }

    #[test]
    fn test_yolo_mode_suggestion_only_once() {
        let mut engine = SuggestionEngine::new();
        engine.check_yolo_mode(true);
        engine.dismiss_current();
        engine.check_yolo_mode(false);
        engine.check_yolo_mode(true);

        // Should not show again after dismissal
        assert!(engine.current().is_none());
    }

    #[test]
    fn test_long_operation_suggestion() {
        let mut engine = SuggestionEngine::new();
        engine.check_long_operation(true, Some(15));

        let suggestion = engine.current();
        assert!(suggestion.is_some());
        assert_eq!(suggestion.unwrap().id, "long_operation");
    }

    #[test]
    fn test_long_operation_no_suggestion_short() {
        let mut engine = SuggestionEngine::new();
        engine.check_long_operation(true, Some(5));

        assert!(engine.current().is_none());
    }

    #[test]
    fn test_dismissal_cooldown() {
        let mut engine = SuggestionEngine::new();
        engine.set_auto_hide_duration(Duration::from_millis(1));

        engine.check_context_size(150_000);
        assert!(engine.current().is_some());

        engine.dismiss_current();
        assert!(engine.current().is_none());

        // Immediately trying again should not show
        engine.check_context_size(160_000);
        assert!(engine.current().is_none());
    }

    #[test]
    fn test_priority_replacement() {
        let mut engine = SuggestionEngine::new();

        // Low priority first
        engine.check_input_pattern("read file");
        assert_eq!(engine.current().unwrap().id, "file_suggestion");

        // High priority should replace
        engine.check_last_error("Operation failed: file outside workspace boundary");
        assert_eq!(engine.current().unwrap().id, "workspace_boundary");
    }

    #[test]
    fn test_error_suggestion_priority() {
        let mut engine = SuggestionEngine::new();

        // Generic error
        engine.check_last_error("Something went wrong");
        assert_eq!(
            engine.current().unwrap().priority,
            SuggestionPriority::Medium
        );

        engine.clear();

        // Permission error (High priority)
        engine.check_last_error("Permission denied");
        assert_eq!(engine.current().unwrap().priority, SuggestionPriority::High);

        engine.clear();

        // Workspace boundary (Critical priority)
        engine.check_last_error("outside workspace boundary");
        assert_eq!(
            engine.current().unwrap().priority,
            SuggestionPriority::Critical
        );
    }
}
