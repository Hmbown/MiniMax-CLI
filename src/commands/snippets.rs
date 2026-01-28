//! Snippet commands: snippet, snippets

use std::fmt::Write;

use crate::snippets::SnippetRegistry;
use crate::tui::app::{App, AppAction};

use super::CommandResult;

/// List all available snippets
pub fn list_snippets(_app: &mut App) -> CommandResult {
    let registry = SnippetRegistry::load();

    if registry.is_empty() {
        return CommandResult::message(
            "No snippets available.\n\n\
             Snippets are loaded from ~/.minimax/snippets.toml".to_string(),
        );
    }

    let mut output = format!("Available snippets ({}):\n", registry.len());
    output.push_str("─────────────────────────────\n");
    
    for snippet in registry.list() {
        let _ = writeln!(output, "  /snippet {:12} - {}", snippet.name, snippet.description);
    }
    
    let _ = write!(
        output,
        "\nUse /snippet <name> to insert a snippet\nSnippets location: {}",
        SnippetRegistry::default_path().display()
    );

    if registry.is_using_defaults() {
        output.push_str("\n\n(Using built-in defaults - create ~/.minimax/snippets.toml to customize)");
    }

    CommandResult::message(output)
}

/// Insert a snippet into the input field
pub fn insert_snippet(_app: &mut App, name: Option<&str>) -> CommandResult {
    let name = match name {
        Some(n) => n.trim(),
        None => {
            return CommandResult::error("Usage: /snippet <name>");
        }
    };

    let registry = SnippetRegistry::load();

    if let Some(snippet) = registry.get(name) {
        // Return action to set the input text
        CommandResult::action(AppAction::SetInput(snippet.template.clone()))
    } else {
        // Not found - suggest similar names
        let similar = registry.find_similar(name, 2);
        
        let mut msg = format!("Snippet '{}' not found.", name);
        
        if !similar.is_empty() {
            msg.push_str(&format!("\n\nDid you mean: {}?", similar.join(", ")));
        }
        
        let available: Vec<String> = registry.list().iter().map(|s| s.name.clone()).collect();
        if !available.is_empty() {
            msg.push_str(&format!("\n\nAvailable snippets: {}", available.join(", ")));
        }
        
        msg.push_str(&format!("\n\nUse /snippets to see all available snippets."));
        
        CommandResult::error(msg)
    }
}

/// Get snippet names for tab completion
pub fn snippet_names() -> Vec<String> {
    let registry = SnippetRegistry::load();
    registry.list().iter().map(|s| s.name.clone()).collect()
}
