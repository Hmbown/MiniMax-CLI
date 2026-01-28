//! Quick Snippets System - Reusable text templates for common tasks
//!
//! Snippets are stored at ~/.minimax/snippets.toml and provide quick access
//! to common prompt templates like code reviews, explanations, etc.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// A single snippet with name, description, and template text
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Snippet {
    /// Unique name for the snippet (used as key)
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// The template text to insert
    pub template: String,
}

/// Collection of snippets loaded from config
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SnippetsConfig {
    /// Map of snippet name to snippet data
    #[serde(default)]
    pub snippets: HashMap<String, SnippetEntry>,
}

/// Individual snippet entry in TOML format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnippetEntry {
    /// The template text
    pub template: String,
    /// Optional description (falls back to defaults)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Snippet registry that manages loaded snippets
#[derive(Debug, Clone)]
pub struct SnippetRegistry {
    snippets: HashMap<String, Snippet>,
    use_defaults: bool,
}

impl SnippetRegistry {
    /// Get the default snippets file path
    pub fn default_path() -> PathBuf {
        dirs::home_dir()
            .map(|p| p.join(".minimax").join("snippets.toml"))
            .unwrap_or_else(|| PathBuf::from(".minimax/snippets.toml"))
    }

    /// Load snippets from disk, or use defaults if not found
    pub fn load() -> Self {
        let path = Self::default_path();
        Self::load_from(&path)
    }

    /// Load snippets from a specific path
    pub fn load_from(path: &PathBuf) -> Self {
        if !path.exists() {
            return Self::with_defaults();
        }

        match Self::try_load_from(path) {
            Ok(registry) => registry,
            Err(e) => {
                eprintln!("Warning: Failed to load snippets from {}: {e}", path.display());
                Self::with_defaults()
            }
        }
    }

    /// Try to load snippets from disk
    fn try_load_from(path: &PathBuf) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read snippets from {}", path.display()))?;
        
        let config: SnippetsConfig = toml::from_str(&content)
            .with_context(|| format!("Failed to parse snippets from {}", path.display()))?;

        let mut snippets = HashMap::new();
        
        // Convert config entries to Snippet structs
        for (name, entry) in config.snippets {
            let description = entry.description.unwrap_or_else(|| {
                // Try to get default description if available
                Self::default_snippets()
                    .get(&name)
                    .map(|s| s.description.clone())
                    .unwrap_or_else(|| "Custom snippet".to_string())
            });
            
            snippets.insert(name.clone(), Snippet {
                name,
                description,
                template: entry.template,
            });
        }

        Ok(Self {
            snippets,
            use_defaults: false,
        })
    }

    /// Create a registry with default built-in snippets
    pub fn with_defaults() -> Self {
        Self {
            snippets: Self::default_snippets(),
            use_defaults: true,
        }
    }

    /// Get the default built-in snippets
    fn default_snippets() -> HashMap<String, Snippet> {
        let mut snippets = HashMap::new();
        
        snippets.insert("review".to_string(), Snippet {
            name: "review".to_string(),
            description: "Code review request".to_string(),
            template: "Please review this code for bugs, performance issues, and style improvements:".to_string(),
        });
        
        snippets.insert("explain".to_string(), Snippet {
            name: "explain".to_string(),
            description: "Ask for explanation".to_string(),
            template: "Please explain how this works in detail:".to_string(),
        });
        
        snippets.insert("test".to_string(), Snippet {
            name: "test".to_string(),
            description: "Generate tests".to_string(),
            template: "Write unit tests for this code:".to_string(),
        });
        
        snippets.insert("doc".to_string(), Snippet {
            name: "doc".to_string(),
            description: "Generate docs".to_string(),
            template: "Add documentation comments to this code:".to_string(),
        });
        
        snippets.insert("refactor".to_string(), Snippet {
            name: "refactor".to_string(),
            description: "Refactoring request".to_string(),
            template: "Refactor this code to improve readability and maintainability:".to_string(),
        });
        
        snippets.insert("optimize".to_string(), Snippet {
            name: "optimize".to_string(),
            description: "Performance optimization".to_string(),
            template: "Optimize this code for better performance:".to_string(),
        });
        
        snippets
    }

    /// Get a snippet by name
    pub fn get(&self, name: &str) -> Option<&Snippet> {
        self.snippets.get(name)
    }

    /// List all available snippets
    pub fn list(&self) -> Vec<&Snippet> {
        let mut snippets: Vec<&Snippet> = self.snippets.values().collect();
        // Sort by name for consistent ordering
        snippets.sort_by(|a, b| a.name.cmp(&b.name));
        snippets
    }

    /// Check if using default snippets
    pub fn is_using_defaults(&self) -> bool {
        self.use_defaults
    }

    /// Get the number of loaded snippets
    pub fn len(&self) -> usize {
        self.snippets.len()
    }

    /// Check if no snippets are loaded
    pub fn is_empty(&self) -> bool {
        self.snippets.is_empty()
    }

    /// Find snippets with names similar to the given name (for "Did you mean?" suggestions)
    pub fn find_similar(&self, name: &str, max_distance: usize) -> Vec<String> {
        let name_lower = name.to_lowercase();
        let mut similar = Vec::new();
        
        for snippet_name in self.snippets.keys() {
            let distance = edit_distance(&name_lower, &snippet_name.to_lowercase());
            if distance <= max_distance && distance > 0 {
                similar.push(snippet_name.clone());
            }
        }
        
        similar.sort_by_key(|n| edit_distance(&name_lower, &n.to_lowercase()));
        similar
    }
}

impl Default for SnippetRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}

/// Get a snippet by name (convenience function that loads defaults)
pub fn get_snippet(name: &str) -> Option<Snippet> {
    let registry = SnippetRegistry::load();
    registry.get(name).cloned()
}

/// List all available snippets (convenience function)
pub fn list_snippets() -> Vec<Snippet> {
    let registry = SnippetRegistry::load();
    registry.list().into_iter().cloned().collect()
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

/// Create default snippets.toml content
pub fn default_snippets_toml() -> String {
    r#"[snippets]
review = { template = "Please review this code for bugs, performance issues, and style improvements:", description = "Code review request" }
explain = { template = "Please explain how this works in detail:", description = "Ask for explanation" }
test = { template = "Write unit tests for this code:", description = "Generate tests" }
doc = { template = "Add documentation comments to this code:", description = "Generate docs" }
refactor = { template = "Refactor this code to improve readability and maintainability:", description = "Refactoring request" }
optimize = { template = "Optimize this code for better performance:", description = "Performance optimization" }
"#.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_snippets() {
        let registry = SnippetRegistry::with_defaults();
        assert!(!registry.is_empty());
        assert!(registry.get("review").is_some());
        assert!(registry.get("explain").is_some());
        assert!(registry.get("test").is_some());
    }

    #[test]
    fn test_get_snippet() {
        let registry = SnippetRegistry::with_defaults();
        
        let review = registry.get("review").unwrap();
        assert_eq!(review.name, "review");
        assert!(review.template.contains("review this code"));
        
        let explain = registry.get("explain").unwrap();
        assert_eq!(explain.name, "explain");
        assert!(explain.template.contains("explain how this works"));
    }

    #[test]
    fn test_list_snippets_sorted() {
        let registry = SnippetRegistry::with_defaults();
        let snippets = registry.list();
        
        // Should be sorted by name
        for i in 1..snippets.len() {
            assert!(snippets[i - 1].name <= snippets[i].name);
        }
    }

    #[test]
    fn test_find_similar() {
        let registry = SnippetRegistry::with_defaults();
        
        // Typo: "reviw" should match "review"
        let similar = registry.find_similar("reviw", 2);
        assert!(similar.contains(&"review".to_string()));
        
        // Typo: "explin" should match "explain"
        let similar2 = registry.find_similar("explin", 2);
        assert!(similar2.contains(&"explain".to_string()));
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
    fn test_default_snippets_toml() {
        let toml = default_snippets_toml();
        assert!(toml.contains("review"));
        assert!(toml.contains("explain"));
        assert!(toml.contains("test"));
        assert!(toml.contains("doc"));
        assert!(toml.contains("refactor"));
        assert!(toml.contains("optimize"));
    }
}
