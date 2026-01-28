//! Error hint system for providing actionable error messages with recovery hints.
//!
//! This module analyzes error messages and provides user-friendly suggestions
//! for resolving common issues.

/// Classification of error types for targeted hints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorType {
    /// API rate limit exceeded
    RateLimit,
    /// Invalid or missing API key
    InvalidApiKey,
    /// Network connectivity issues
    NetworkError,
    /// Context too long for the model
    ContextTooLong,
    /// Tool execution failed
    ToolExecutionError,
    /// File not found
    FileNotFound,
    /// Permission denied
    PermissionDenied,
}

/// A hint containing an error type, message, and suggested fix.
#[derive(Debug, Clone)]
pub struct ErrorHint {
    /// The type of error that occurred
    pub error_type: ErrorType,
    /// A user-friendly error message
    pub message: String,
    /// A suggested fix or recovery action
    pub suggestion: String,
}

impl ErrorHint {
    /// Create a new error hint.
    #[must_use]
    pub fn new(
        error_type: ErrorType,
        message: impl Into<String>,
        suggestion: impl Into<String>,
    ) -> Self {
        Self {
            error_type,
            message: message.into(),
            suggestion: suggestion.into(),
        }
    }
}

impl ErrorType {
    pub const fn label(self) -> &'static str {
        match self {
            ErrorType::RateLimit => "rate limit",
            ErrorType::InvalidApiKey => "auth",
            ErrorType::NetworkError => "network",
            ErrorType::ContextTooLong => "context",
            ErrorType::ToolExecutionError => "tool",
            ErrorType::FileNotFound => "file",
            ErrorType::PermissionDenied => "permission",
        }
    }
}

/// Analyze an error message and return an appropriate hint with recovery suggestion.
///
/// # Examples
///
/// ```
/// use minimax_cli::error_hints::{get_error_hint, ErrorType};
///
/// let error = "HTTP 429: rate limit exceeded";
/// if let Some(hint) = get_error_hint(error) {
///     assert_eq!(hint.error_type, ErrorType::RateLimit);
/// }
/// ```
#[must_use]
pub fn get_error_hint(error: &str) -> Option<ErrorHint> {
    let error_lower = error.to_lowercase();

    // Rate limit errors
    if error_lower.contains("429")
        || error_lower.contains("rate limit")
        || error_lower.contains("too many requests")
        || error_lower.contains("throttled")
    {
        return Some(ErrorHint::new(
            ErrorType::RateLimit,
            "Rate limit hit",
            "Wait a moment and retry, or check your quota with /usage",
        ));
    }

    // Invalid API key errors
    if error_lower.contains("401")
        || error_lower.contains("unauthorized")
        || error_lower.contains("invalid api key")
        || error_lower.contains("authentication failed")
        || error_lower.contains("invalid key")
    {
        return Some(ErrorHint::new(
            ErrorType::InvalidApiKey,
            "Invalid API key",
            "Run /logout to reconfigure or /setup for wizard",
        ));
    }

    // Path escape / workspace errors (check before network since "resolve" can overlap)
    if error_lower.contains("path escapes workspace") || error_lower.contains("outside workspace") {
        return Some(ErrorHint::new(
            ErrorType::PermissionDenied,
            "Path outside workspace",
            "Run /trust to enable access outside workspace, or start with --yolo flag",
        ));
    }

    // Network errors
    if error_lower.contains("network")
        || error_lower.contains("connection")
        || error_lower.contains("timeout")
        || error_lower.contains("dns")
        || error_lower.contains("unreachable")
        || error_lower.contains("refused")
        || error_lower.contains("could not connect")
        || error_lower.contains("failed to resolve")
    {
        return Some(ErrorHint::new(
            ErrorType::NetworkError,
            "Network error",
            "Check your connection and try again",
        ));
    }

    // Context too long errors
    if error_lower.contains("context")
        && (error_lower.contains("too long")
            || error_lower.contains("token limit")
            || error_lower.contains("maximum length")
            || error_lower.contains("exceeds"))
    {
        return Some(ErrorHint::new(
            ErrorType::ContextTooLong,
            "Context full",
            "Use /compact to reduce size, or start fresh with /clear",
        ));
    }

    // File not found errors
    if (error_lower.contains("no such file")
        || error_lower.contains("file not found")
        || error_lower.contains("os error 2")
        || error_lower.contains("could not find"))
        && !error_lower.contains("workspace")
    {
        return Some(ErrorHint::new(
            ErrorType::FileNotFound,
            "File not found",
            "Use @filename for file completion, or check the path",
        ));
    }

    // Permission denied errors
    if error_lower.contains("permission denied")
        || error_lower.contains("os error 13")
        || error_lower.contains("access denied")
        || error_lower.contains("not allowed")
    {
        return Some(ErrorHint::new(
            ErrorType::PermissionDenied,
            "Permission denied",
            "Run /trust to enable access, or start with --yolo flag",
        ));
    }

    // Tool execution errors - pattern match for specific tool types
    if error_lower.contains("failed to execute tool")
        || error_lower.contains("tool error")
        || error_lower.contains("execution failed")
    {
        return Some(get_tool_error_hint(&error_lower));
    }

    // API errors (5xx)
    if error_lower.contains("500")
        || error_lower.contains("502")
        || error_lower.contains("503")
        || error_lower.contains("504")
        || error_lower.contains("internal server error")
        || error_lower.contains("bad gateway")
        || error_lower.contains("service unavailable")
    {
        return Some(ErrorHint::new(
            ErrorType::NetworkError,
            "API service error",
            "The API service is experiencing issues. Wait a moment and retry",
        ));
    }

    None
}

/// Get a specific hint for tool execution errors based on the tool type.
fn get_tool_error_hint(error_lower: &str) -> ErrorHint {
    // Shell tool errors
    if error_lower.contains("shell") || error_lower.contains("exec_shell") {
        return ErrorHint::new(
            ErrorType::ToolExecutionError,
            "Shell execution failed",
            "Shell commands require YOLO mode or explicit approval. Start with --yolo or type /yolo",
        );
    }

    // Git errors
    if error_lower.contains("git") {
        return ErrorHint::new(
            ErrorType::ToolExecutionError,
            "Git command failed",
            "Check that git is installed and you're in a git repository",
        );
    }

    // File operation errors
    if error_lower.contains("read_file") || error_lower.contains("write_file") {
        return ErrorHint::new(
            ErrorType::ToolExecutionError,
            "File operation failed",
            "Check file permissions and path. Use @ for file completion",
        );
    }

    // Search errors
    if error_lower.contains("search")
        || error_lower.contains("grep")
        || error_lower.contains("find")
    {
        return ErrorHint::new(
            ErrorType::ToolExecutionError,
            "Search failed",
            "Check your search pattern syntax and try again",
        );
    }

    // MCP tool errors
    if error_lower.contains("mcp") {
        return ErrorHint::new(
            ErrorType::ToolExecutionError,
            "MCP tool failed",
            "Check your MCP configuration with /mcp status",
        );
    }

    // Sub-agent errors
    if error_lower.contains("sub-agent") || error_lower.contains("subagent") {
        return ErrorHint::new(
            ErrorType::ToolExecutionError,
            "Sub-agent failed",
            "Check the agent logs or reduce the task complexity",
        );
    }

    // Default tool error
    ErrorHint::new(
        ErrorType::ToolExecutionError,
        "Tool execution failed",
        "Check the error details and try again with different input",
    )
}

/// Check if an error is recoverable (user can retry/fix it).
#[must_use]
pub fn is_recoverable(error: &str) -> bool {
    let error_lower = error.to_lowercase();

    // Rate limits are recoverable
    if error_lower.contains("rate limit") || error_lower.contains("429") {
        return true;
    }

    // Network errors are often transient
    if error_lower.contains("timeout")
        || error_lower.contains("network")
        || error_lower.contains("connection")
    {
        return true;
    }

    // Context too long can be fixed
    if error_lower.contains("context") && error_lower.contains("too long") {
        return true;
    }

    // Permission errors can be fixed
    if error_lower.contains("permission") || error_lower.contains("trust") {
        return true;
    }

    // File not found can be fixed
    if error_lower.contains("file not found") || error_lower.contains("no such file") {
        return true;
    }

    // API key errors require user action but are recoverable
    if error_lower.contains("unauthorized") || error_lower.contains("invalid key") {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limit_hint() {
        let error = "HTTP 429: rate limit exceeded";
        let hint = get_error_hint(error).unwrap();
        assert_eq!(hint.error_type, ErrorType::RateLimit);
        assert!(hint.suggestion.contains("/usage"));
    }

    #[test]
    fn test_api_key_hint() {
        let error = "HTTP 401: Unauthorized - Invalid API key";
        let hint = get_error_hint(error).unwrap();
        assert_eq!(hint.error_type, ErrorType::InvalidApiKey);
        assert!(hint.suggestion.contains("/logout"));
    }

    #[test]
    fn test_network_hint() {
        let error = "Failed to connect: network unreachable";
        let hint = get_error_hint(error).unwrap();
        assert_eq!(hint.error_type, ErrorType::NetworkError);
    }

    #[test]
    fn test_context_too_long_hint() {
        let error = "Context too long: exceeds maximum token limit of 200000";
        let hint = get_error_hint(error).unwrap();
        assert_eq!(hint.error_type, ErrorType::ContextTooLong);
        assert!(hint.suggestion.contains("/compact"));
    }

    #[test]
    fn test_file_not_found_hint() {
        let error = "No such file or directory: foo.txt";
        let hint = get_error_hint(error).unwrap();
        assert_eq!(hint.error_type, ErrorType::FileNotFound);
    }

    #[test]
    fn test_permission_denied_hint() {
        let error = "Permission denied: cannot write to file";
        let hint = get_error_hint(error).unwrap();
        assert_eq!(hint.error_type, ErrorType::PermissionDenied);
        assert!(hint.suggestion.contains("/trust"));
    }

    #[test]
    fn test_path_escape_hint() {
        let error = "Failed to resolve path '/etc/passwd': path escapes workspace";
        let hint = get_error_hint(error).unwrap();
        assert_eq!(hint.error_type, ErrorType::PermissionDenied);
    }

    #[test]
    fn test_recoverable_errors() {
        assert!(is_recoverable("Rate limit exceeded"));
        assert!(is_recoverable("Network timeout"));
        assert!(is_recoverable("Context too long"));
        assert!(!is_recoverable("Some random critical failure"));
    }

    #[test]
    fn test_no_hint_for_unknown_error() {
        let error = "Some completely unknown error message";
        assert!(get_error_hint(error).is_none());
    }
}
