//! Usage command for API quota tracking

use crate::tui::app::App;

use super::CommandResult;

/// Show API usage and quota information
///
/// Note: MiniMax doesn't currently provide a quota API endpoint,
/// so this shows estimated usage based on session tracking.
pub fn usage(app: &mut App) -> CommandResult {
    let mut output = String::new();

    output.push_str("API Usage Information\n");
    output.push_str("═══════════════════════════════════════\n\n");

    // Current session
    output.push_str("Current Session:\n");
    output.push_str(&format!("  Tokens used:     {}\n", app.total_tokens));
    output.push_str(&format!("  Est. cost:       ${:.4}\n", app.session_cost));
    output.push_str(&format!(
        "  Messages sent:   {}\n",
        app.api_messages.len() / 2
    ));
    output.push_str(&format!("  Model:           {}\n\n", app.model));

    // Media generation costs
    output.push_str("MiniMax Media Pricing (reference):\n");
    output.push_str("  Text (M2.1):     $0.20 / 1M input tokens\n");
    output.push_str("  Image gen:       $0.007 / image\n");
    output.push_str("  TTS (HD):        $0.0035 / 1K chars\n");
    output.push_str("  Video (768P):    $0.40 / 6s\n");
    output.push_str("  Video (1080P):   $0.80 / 6s\n");
    output.push_str("  Music:           $1.00 / 5min\n\n");

    // Tips
    output.push_str("Tips:\n");
    output.push_str("  • Check your MiniMax dashboard for exact quota\n");
    output.push_str("  • Visit: https://platform.minimax.io\n");
    output.push_str("  • Use /compact to reduce context when near limits\n");

    CommandResult::message(output)
}
