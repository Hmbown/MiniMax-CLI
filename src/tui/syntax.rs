//! Lightweight regex-based syntax highlighter for code blocks.

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use regex::Regex;

use crate::palette;

/// Supported programming languages for syntax highlighting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Go,
    Java,
    C,
    Cpp,
    Bash,
    Json,
    Yaml,
    Toml,
    Markdown,
}

impl Language {
    /// Parse a language identifier string into a Language variant.
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "rust" | "rs" => Some(Self::Rust),
            "python" | "py" => Some(Self::Python),
            "javascript" | "js" => Some(Self::JavaScript),
            "typescript" | "ts" => Some(Self::TypeScript),
            "go" | "golang" => Some(Self::Go),
            "java" => Some(Self::Java),
            "c" => Some(Self::C),
            "cpp" | "c++" | "cxx" | "cc" => Some(Self::Cpp),
            "bash" | "sh" | "shell" | "zsh" => Some(Self::Bash),
            "json" => Some(Self::Json),
            "yaml" | "yml" => Some(Self::Yaml),
            "toml" => Some(Self::Toml),
            "markdown" | "md" => Some(Self::Markdown),
            _ => None,
        }
    }
}

/// Token types for syntax highlighting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenType {
    Keyword,
    String,
    Comment,
    Number,
    Function,
    Type,
    Plain,
}

impl TokenType {
    fn style(&self) -> Style {
        match self {
            TokenType::Keyword => Style::default().fg(palette::MINIMAX_BLUE),
            TokenType::String => Style::default().fg(palette::MINIMAX_GREEN),
            TokenType::Comment => Style::default().fg(palette::TEXT_DIM),
            TokenType::Number => Style::default().fg(palette::MINIMAX_ORANGE),
            TokenType::Function => Style::default().fg(palette::MINIMAX_YELLOW),
            TokenType::Type => Style::default().fg(palette::MINIMAX_MAGENTA),
            TokenType::Plain => Style::default().fg(palette::TEXT_PRIMARY),
        }
    }
}

/// Highlight code and return a vector of styled lines.
pub fn highlight_code(code: &str, language: &str) -> Vec<Line<'static>> {
    let lang = Language::from_str(language);
    
    if lang.is_none() {
        // Fallback to plain text for unsupported languages
        return code
            .lines()
            .map(|line| Line::from(Span::styled(line.to_string(), TokenType::Plain.style())))
            .collect();
    }
    
    let lang = lang.unwrap();
    code.lines().map(|line| highlight_line(line, lang)).collect()
}

/// Highlight a single line of code.
fn highlight_line(line: &str, lang: Language) -> Line<'static> {
    if line.is_empty() {
        return Line::from("");
    }
    
    let patterns = get_patterns(lang);
    let mut spans = Vec::new();
    let mut remaining = line;
    
    while !remaining.is_empty() {
        let mut best_match: Option<(usize, usize, TokenType)> = None;
        
        // Find the earliest matching pattern
        for (regex, token_type) in &patterns {
            if let Some(mat) = regex.find(remaining) {
                let start = mat.start();
                let end = mat.end();
                
                // Prioritize earlier matches, then longer matches at same position
                if best_match.is_none() 
                    || start < best_match.unwrap().0 
                    || (start == best_match.unwrap().0 && end > best_match.unwrap().1) {
                    best_match = Some((start, end, *token_type));
                }
            }
        }
        
        if let Some((start, end, token_type)) = best_match {
            // Add any plain text before the match
            if start > 0 {
                let plain = &remaining[..start];
                spans.push(Span::styled(plain.to_string(), TokenType::Plain.style()));
            }
            
            // Add the highlighted token
            let token = &remaining[start..end];
            spans.push(Span::styled(token.to_string(), token_type.style()));
            
            // Continue with the rest
            remaining = &remaining[end..];
        } else {
            // No more matches, add remaining as plain text
            spans.push(Span::styled(remaining.to_string(), TokenType::Plain.style()));
            break;
        }
    }
    
    Line::from(spans)
}

/// Get regex patterns for a specific language.
fn get_patterns(lang: Language) -> Vec<(Regex, TokenType)> {
    let mut patterns = Vec::new();
    
    // Comments (language-specific)
    match lang {
        Language::Python | Language::Bash | Language::Yaml | Language::Toml => {
            // # single-line comments
            if let Ok(re) = Regex::new("#[^\\n]*") {
                patterns.push((re, TokenType::Comment));
            }
        }
        Language::Rust | Language::JavaScript | Language::TypeScript | Language::Go | Language::Java | Language::C | Language::Cpp => {
            // // single-line comments
            if let Ok(re) = Regex::new("//[^\\n]*") {
                patterns.push((re, TokenType::Comment));
            }
            // /* */ multi-line comments (single line)
            if let Ok(re) = Regex::new("/\\*.*?\\*/") {
                patterns.push((re, TokenType::Comment));
            }
        }
        _ => {}
    }
    
    // Strings (common across most languages)
    // Double-quoted strings
    if let Ok(re) = Regex::new("\"([^\"\\\\]|\\\\.)*\"") {
        patterns.push((re, TokenType::String));
    }
    // Single-quoted strings (for languages that support them)
    if !matches!(lang, Language::Json) {
        if let Ok(re) = Regex::new("'([^'\\\\]|\\\\.)*'") {
            patterns.push((re, TokenType::String));
        }
    }
    // Backtick strings (JavaScript/TypeScript)
    if matches!(lang, Language::JavaScript | Language::TypeScript) {
        if let Ok(re) = Regex::new("`([^`\\\\]|\\\\.)*`") {
            patterns.push((re, TokenType::String));
        }
    }
    // Raw strings (Rust)
    if matches!(lang, Language::Rust) {
        if let Ok(re) = Regex::new("r#*\"[^\"]*\"#*") {
            patterns.push((re, TokenType::String));
        }
    }
    
    // Numbers (common pattern)
    if let Ok(re) = Regex::new(r"\b\d+\.?\d*([eE][+-]?\d+)?\b") {
        patterns.push((re, TokenType::Number));
    }
    if let Ok(re) = Regex::new(r"\b0x[0-9a-fA-F]+\b") {
        patterns.push((re, TokenType::Number));
    }
    
    // Keywords and types (language-specific)
    let (keywords, types) = get_keywords_and_types(lang);
    
    // Add keyword patterns (match whole words)
    for kw in keywords {
        let pattern = format!("\\b{}\\b", regex::escape(kw));
        if let Ok(re) = Regex::new(&pattern) {
            patterns.push((re, TokenType::Keyword));
        }
    }
    
    // Add type patterns
    for ty in types {
        let pattern = format!("\\b{}\\b", regex::escape(ty));
        if let Ok(re) = Regex::new(&pattern) {
            patterns.push((re, TokenType::Type));
        }
    }
    
    // Function calls (identifier followed by opening parenthesis)
    // This is a heuristic that works for many languages
    if let Ok(re) = Regex::new(r"\b([a-zA-Z_][a-zA-Z0-9_]*)\s*(?=\()") {
        patterns.push((re, TokenType::Function));
    }
    
    patterns
}

/// Get keywords and types for a specific language.
fn get_keywords_and_types(lang: Language) -> (Vec<&'static str>, Vec<&'static str>) {
    let keywords: Vec<&str>;
    let types: Vec<&str>;
    
    match lang {
        Language::Rust => {
            keywords = vec![
                "fn", "let", "mut", "const", "static", "if", "else", "match",
                "for", "while", "loop", "break", "continue", "return", "struct",
                "enum", "impl", "trait", "use", "pub", "mod", "crate", "super",
                "self", "where", "async", "await", "move", "ref", "type",
                "unsafe", "async", "await", "dyn", "box", "as", "in",
            ];
            types = vec![
                "bool", "char", "i8", "i16", "i32", "i64", "i128", "isize",
                "u8", "u16", "u32", "u64", "u128", "usize", "f32", "f64",
                "String", "str", "Vec", "Option", "Result", "Box", "Rc", "Arc",
                "HashMap", "BTreeMap", "HashSet", "BTreeSet", "VecDeque", "LinkedList",
            ];
        }
        Language::Python => {
            keywords = vec![
                "def", "class", "if", "elif", "else", "for", "while", "break",
                "continue", "return", "import", "from", "as", "try", "except",
                "finally", "raise", "with", "pass", "lambda", "yield", "async",
                "await", "global", "nonlocal", "assert", "del", "None", "True",
                "False", "and", "or", "not", "is", "in",
            ];
            types = vec![
                "int", "float", "str", "bool", "list", "dict", "tuple", "set",
                "frozenset", "bytes", "bytearray", "object", "type", "NoneType",
            ];
        }
        Language::JavaScript | Language::TypeScript => {
            keywords = vec![
                "function", "const", "let", "var", "if", "else", "for", "while",
                "do", "break", "continue", "return", "switch", "case", "default",
                "try", "catch", "finally", "throw", "new", "this", "typeof",
                "instanceof", "void", "delete", "in", "of", "async", "await",
                "import", "export", "from", "class", "extends", "super", "static",
                "get", "set", "yield", "true", "false", "null", "undefined",
            ];
            types = vec![
                "number", "string", "boolean", "object", "symbol", "bigint",
                "any", "unknown", "never", "void", "Array", "Promise", "Map",
                "Set", "Date", "RegExp", "Error", "Function", "String", "Number",
                "Boolean", "Object",
            ];
        }
        Language::Go => {
            keywords = vec![
                "func", "var", "const", "type", "struct", "interface", "map",
                "chan", "if", "else", "for", "range", "switch", "case", "default",
                "break", "continue", "return", "goto", "fallthrough", "defer",
                "go", "select", "import", "package",
            ];
            types = vec![
                "bool", "string", "int", "int8", "int16", "int32", "int64",
                "uint", "uint8", "uint16", "uint32", "uint64", "uintptr",
                "byte", "rune", "float32", "float64", "complex64", "complex128",
                "error", "any",
            ];
        }
        Language::Java => {
            keywords = vec![
                "public", "private", "protected", "static", "final", "abstract",
                "class", "interface", "extends", "implements", "void", "if",
                "else", "for", "while", "do", "switch", "case", "default",
                "break", "continue", "return", "try", "catch", "finally",
                "throw", "throws", "new", "this", "super", "instanceof",
                "import", "package", "synchronized", "volatile", "transient",
                "native", "strictfp", "assert", "const", "goto",
            ];
            types = vec![
                "byte", "short", "int", "long", "float", "double", "char",
                "boolean", "String", "Object", "Class", "Integer", "Long",
                "Double", "Float", "Boolean", "Character", "Byte", "Short",
                "Void", "List", "Map", "Set", "ArrayList", "HashMap", "HashSet",
            ];
        }
        Language::C | Language::Cpp => {
            let c_keywords = vec![
                "auto", "break", "case", "char", "const", "continue", "default",
                "do", "double", "else", "enum", "extern", "float", "for", "goto",
                "if", "inline", "int", "long", "register", "restrict", "return",
                "short", "signed", "sizeof", "static", "struct", "switch",
                "typedef", "union", "unsigned", "void", "volatile", "while",
                "_Alignas", "_Alignof", "_Atomic", "_Bool", "_Complex",
                "_Generic", "_Imaginary", "_Noreturn", "_Static_assert", "_Thread_local",
            ];
            let cpp_keywords = if matches!(lang, Language::Cpp) {
                vec![
                    "class", "public", "private", "protected", "virtual", "override",
                    "final", "namespace", "using", "template", "typename", "new",
                    "delete", "try", "catch", "throw", "const_cast", "dynamic_cast",
                    "reinterpret_cast", "static_cast", "explicit", "friend",
                    "mutable", "operator", "this", "true", "false", "nullptr",
                    "constexpr", "decltype", "noexcept", "static_assert", "thread_local",
                ]
            } else {
                vec![]
            };
            keywords = c_keywords.into_iter().chain(cpp_keywords).collect();
            types = vec![
                "bool", "char", "short", "int", "long", "float", "double",
                "void", "size_t", "ssize_t", "ptrdiff_t", "intptr_t", "uintptr_t",
                "int8_t", "int16_t", "int32_t", "int64_t", "uint8_t", "uint16_t",
                "uint32_t", "uint64_t", "string", "vector", "map", "set",
                "array", "unique_ptr", "shared_ptr", "weak_ptr", "optional",
                "variant", "tuple",
            ];
        }
        Language::Bash => {
            keywords = vec![
                "if", "then", "else", "elif", "fi", "for", "while", "until",
                "do", "done", "case", "esac", "in", "function", "return",
                "break", "continue", "exit", "export", "source", "alias",
                "declare", "local", "readonly", "shift", "test", "true", "false",
            ];
            types = vec![];
        }
        Language::Json | Language::Yaml | Language::Toml => {
            keywords = vec!["true", "false", "null", "yes", "no", "on", "off"];
            types = vec![];
        }
        Language::Markdown => {
            keywords = vec![];
            types = vec![];
        }
    }
    
    (keywords, types)
}

/// Extract code blocks from markdown text.
/// Returns a vector of (is_code_block, text) tuples.
/// For code blocks, the text includes the language identifier on the first line.
pub fn extract_code_blocks(text: &str) -> Vec<(bool, String)> {
    let mut result = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    
    while i < lines.len() {
        let line = lines[i];
        
        if line.trim_start().starts_with("```") {
            // Found code block start
            let lang = line.trim_start()[3..].trim();
            let mut code_lines = Vec::new();
            i += 1;
            
            // Collect code block content
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                code_lines.push(lines[i]);
                i += 1;
            }
            
            // Skip the closing ```
            if i < lines.len() {
                i += 1;
            }
            
            let code = code_lines.join("\n");
            result.push((true, format!("{}\n{}", lang, code)));
        } else {
            // Regular text
            result.push((false, line.to_string()));
            i += 1;
        }
    }
    
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_from_str() {
        assert_eq!(Language::from_str("rust"), Some(Language::Rust));
        assert_eq!(Language::from_str("rs"), Some(Language::Rust));
        assert_eq!(Language::from_str("python"), Some(Language::Python));
        assert_eq!(Language::from_str("py"), Some(Language::Python));
        assert_eq!(Language::from_str("javascript"), Some(Language::JavaScript));
        assert_eq!(Language::from_str("js"), Some(Language::JavaScript));
        assert_eq!(Language::from_str("typescript"), Some(Language::TypeScript));
        assert_eq!(Language::from_str("ts"), Some(Language::TypeScript));
        assert_eq!(Language::from_str("unknown_lang"), None);
    }

    #[test]
    fn test_basic_keyword_highlighting() {
        let code = "fn main() {\n    let x = 5;\n}";
        let lines = highlight_code(code, "rust");
        
        // First line should have 'fn' highlighted as keyword (blue)
        assert!(!lines.is_empty());
        let first_line = &lines[0];
        let spans: Vec<_> = first_line.spans.iter().collect();
        
        // Find the 'fn' span
        let fn_span = spans.iter().find(|s| s.content == "fn");
        assert!(fn_span.is_some(), "Should find 'fn' keyword in output");
    }

    #[test]
    fn test_string_highlighting() {
        let code = r#"let s = "hello world";"#;
        let lines = highlight_code(code, "rust");
        
        assert!(!lines.is_empty());
        let first_line = &lines[0];
        let spans: Vec<_> = first_line.spans.iter().collect();
        
        // Find the string span
        let string_span = spans.iter().find(|s| s.content.contains("hello"));
        assert!(string_span.is_some(), "Should find string literal in output");
    }

    #[test]
    fn test_comment_highlighting() {
        let code = "// This is a comment\nlet x = 5;";
        let lines = highlight_code(code, "rust");
        
        assert!(!lines.is_empty());
        let first_line = &lines[0];
        let spans: Vec<_> = first_line.spans.iter().collect();
        
        // Check that comment is highlighted
        let comment_span = spans.iter().find(|s| s.content.contains("comment"));
        assert!(comment_span.is_some(), "Should find comment in output");
    }

    #[test]
    fn test_number_highlighting() {
        let code = "let x = 42;\nlet y = 3.14;";
        let lines = highlight_code(code, "rust");
        
        assert!(!lines.is_empty());
        let first_line = &lines[0];
        let spans: Vec<_> = first_line.spans.iter().collect();
        
        // Find the number span
        let number_span = spans.iter().find(|s| s.content == "42");
        assert!(number_span.is_some(), "Should find number in output");
    }

    #[test]
    fn test_multi_language_support() {
        // Test Python
        let python_code = "def foo():\n    return 42";
        let lines = highlight_code(python_code, "python");
        assert!(!lines.is_empty());
        
        // Check for 'def' keyword
        let first_line = &lines[0];
        let spans: Vec<_> = first_line.spans.iter().collect();
        let def_span = spans.iter().find(|s| s.content == "def");
        assert!(def_span.is_some(), "Should find 'def' keyword in Python");
        
        // Test JavaScript
        let js_code = "function foo() { return 42; }";
        let lines = highlight_code(js_code, "javascript");
        assert!(!lines.is_empty());
        
        let first_line = &lines[0];
        let spans: Vec<_> = first_line.spans.iter().collect();
        let func_span = spans.iter().find(|s| s.content == "function");
        assert!(func_span.is_some(), "Should find 'function' keyword in JavaScript");
    }

    #[test]
    fn test_unsupported_language_fallback() {
        let code = "some code\nmore code";
        let lines = highlight_code(code, "unknown_lang");
        
        assert_eq!(lines.len(), 2);
        // Both lines should be plain text (single span each)
        assert_eq!(lines[0].spans.len(), 1);
        assert_eq!(lines[1].spans.len(), 1);
    }

    #[test]
    fn test_extract_code_blocks() {
        let text = "Some text\n```rust\nfn main() {}\n```\nMore text";
        let blocks = extract_code_blocks(text);
        
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0], (false, "Some text".to_string()));
        assert_eq!(blocks[1].0, true);
        assert!(blocks[1].1.starts_with("rust\n"));
        assert_eq!(blocks[2], (false, "More text".to_string()));
    }

    #[test]
    fn test_python_comments() {
        let code = "# This is a comment\nx = 5";
        let lines = highlight_code(code, "python");
        
        assert!(!lines.is_empty());
        let first_line = &lines[0];
        let spans: Vec<_> = first_line.spans.iter().collect();
        
        // Find the comment span (starts with #)
        let comment_span = spans.iter().find(|s| s.content.starts_with('#'));
        assert!(comment_span.is_some(), "Should find Python comment");
    }

    #[test]
    fn test_json_highlighting() {
        let code = r#"{"key": "value", "number": 42}"#;
        let lines = highlight_code(code, "json");
        
        assert!(!lines.is_empty());
        // JSON should be parsed (strings highlighted, numbers highlighted)
        let first_line = &lines[0];
        assert!(!first_line.spans.is_empty());
    }

    #[test]
    fn test_type_highlighting() {
        let code = "let x: i32 = 5;";
        let lines = highlight_code(code, "rust");
        
        assert!(!lines.is_empty());
        let first_line = &lines[0];
        let spans: Vec<_> = first_line.spans.iter().collect();
        
        // Find the i32 type span
        let type_span = spans.iter().find(|s| s.content == "i32");
        assert!(type_span.is_some(), "Should find type 'i32' in output");
    }
}
