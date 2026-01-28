//! Interactive "Getting Started" tutorial for first-time users.

use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

use crate::palette;

/// A single step in the tutorial.
#[derive(Debug, Clone)]
pub struct TutorialStep {
    pub title: &'static str,
    pub content: &'static str,
    pub hint: Option<&'static str>,
}

impl TutorialStep {
    pub const fn new(title: &'static str, content: &'static str) -> Self {
        Self {
            title,
            content,
            hint: None,
        }
    }

    pub const fn with_hint(title: &'static str, content: &'static str, hint: &'static str) -> Self {
        Self {
            title,
            content,
            hint: Some(hint),
        }
    }
}

/// All tutorial steps in order.
pub const TUTORIAL_STEPS: &[TutorialStep] = &[
    TutorialStep::new(
        "Welcome",
        "Welcome to MiniMax CLI! Let's get you started with the basics.",
    ),
    TutorialStep::with_hint(
        "Modes",
        "MiniMax has multiple modes for different workflows.",
        "Press Tab to cycle: Normal → Plan → Agent → YOLO → RLM → Duo",
    ),
    TutorialStep::with_hint(
        "File References",
        "Quickly reference files in your workspace.",
        "Type @ to trigger fuzzy file completion",
    ),
    TutorialStep::with_hint(
        "Shell Mode",
        "Execute shell commands directly.",
        "Press Ctrl+X to toggle shell mode",
    ),
    TutorialStep::with_hint(
        "Help",
        "Get help anytime you need it.",
        "Press F1 or Ctrl+/ to open help",
    ),
    TutorialStep::with_hint(
        "Commands",
        "Access powerful slash commands.",
        "Type / for commands like /help, /settings, /clear",
    ),
    TutorialStep::new(
        "Try It",
        "You're all set! Try asking:\n• 'Explain this codebase'\n• 'What files are here?'\n• 'Help me refactor main.rs'",
    ),
];

/// State for the interactive tutorial.
#[derive(Debug, Clone)]
pub struct Tutorial {
    pub active: bool,
    pub current_step: usize,
    pub dont_show_again: bool,
}

impl Default for Tutorial {
    fn default() -> Self {
        Self {
            active: false,
            current_step: 0,
            dont_show_again: false,
        }
    }
}

impl Tutorial {
    /// Create a new tutorial instance, loading preference from settings.
    pub fn new(enabled: bool) -> Self {
        Self {
            active: false,
            current_step: 0,
            dont_show_again: !enabled,
        }
    }

    /// Start the tutorial.
    pub fn start(&mut self) {
        self.active = true;
        self.current_step = 0;
    }

    /// Skip/close the tutorial.
    pub fn skip(&mut self) {
        self.active = false;
    }

    /// Go to the next step. Returns true if there are more steps.
    pub fn next(&mut self) -> bool {
        if self.current_step + 1 < TUTORIAL_STEPS.len() {
            self.current_step += 1;
            true
        } else {
            self.active = false;
            false
        }
    }

    /// Go to the previous step. Returns true if successful.
    pub fn previous(&mut self) -> bool {
        if self.current_step > 0 {
            self.current_step -= 1;
            true
        } else {
            false
        }
    }

    /// Get the current step.
    pub fn current(&self) -> Option<&TutorialStep> {
        TUTORIAL_STEPS.get(self.current_step)
    }

    /// Check if on the first step.
    pub fn is_first(&self) -> bool {
        self.current_step == 0
    }

    /// Check if on the last step.
    pub fn is_last(&self) -> bool {
        self.current_step == TUTORIAL_STEPS.len().saturating_sub(1)
    }

    /// Get progress as "Step X of Y".
    pub fn progress_text(&self) -> String {
        format!("Step {} of {}", self.current_step + 1, TUTORIAL_STEPS.len())
    }

    /// Toggle the "don't show again" checkbox.
    pub fn toggle_dont_show(&mut self) {
        self.dont_show_again = !self.dont_show_again;
    }

    /// Check if tutorial should be shown on startup.
    pub fn should_show_on_startup(&self) -> bool {
        !self.dont_show_again
    }

    /// Save the "don't show again" preference.
    pub fn save_preference(&self) {
        // Load current settings, update show_tutorial, and save
        if let Ok(mut settings) = crate::settings::Settings::load() {
            settings.show_tutorial = !self.dont_show_again;
            let _ = settings.save();
        }
    }
}

/// Render the tutorial overlay.
pub fn render_tutorial(f: &mut ratatui::Frame, area: Rect, tutorial: &Tutorial) {
    if !tutorial.active {
        return;
    }

    // Clear the background with a dim overlay
    let clear = Clear;
    clear.render(area, f.buffer_mut());

    // Center the tutorial popup
    let popup_width = 65.min(area.width.saturating_sub(4));
    let popup_height = 20.min(area.height.saturating_sub(4));

    let popup_area = Rect {
        x: (area.width - popup_width) / 2,
        y: (area.height - popup_height) / 2,
        width: popup_width,
        height: popup_height,
    };

    // Get current step
    let step = match tutorial.current() {
        Some(s) => s,
        None => return,
    };

    // Build the content
    let mut lines = vec![
        // Title
        Line::from(vec![Span::styled(
            step.title,
            Style::default()
                .fg(palette::MINIMAX_BLUE)
                .bold()
                .add_modifier(Modifier::UNDERLINED),
        )]),
        Line::from(""),
    ];

    // Content
    for content_line in step.content.lines() {
        lines.push(Line::from(vec![Span::styled(
            content_line,
            Style::default().fg(palette::TEXT_PRIMARY),
        )]));
    }

    // Hint if available
    if let Some(hint) = step.hint {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("💡 ", Style::default().fg(palette::MINIMAX_ORANGE)),
            Span::styled(hint, Style::default().fg(palette::MINIMAX_ORANGE).italic()),
        ]));
    }

    // Spacer before controls
    lines.push(Line::from(""));

    // Progress indicator
    let progress = tutorial.progress_text();
    lines.push(Line::from(vec![Span::styled(
        progress,
        Style::default().fg(palette::TEXT_MUTED),
    )]));

    // Navigation buttons
    lines.push(Line::from(""));

    let mut nav_spans = vec![];

    // Previous button (dimmed if on first step)
    if tutorial.is_first() {
        nav_spans.push(Span::styled(
            "[ Previous ]",
            Style::default().fg(palette::TEXT_DIM),
        ));
    } else {
        nav_spans.push(Span::styled(
            "[ (p)revious ]",
            Style::default().fg(palette::TEXT_PRIMARY),
        ));
    }

    nav_spans.push(Span::raw("  "));

    // Next or Finish button
    if tutorial.is_last() {
        nav_spans.push(Span::styled(
            "[ (f)inish ]",
            Style::default().fg(palette::STATUS_SUCCESS).bold(),
        ));
    } else {
        nav_spans.push(Span::styled(
            "[ (n)ext ]",
            Style::default().fg(palette::MINIMAX_BLUE),
        ));
    }

    nav_spans.push(Span::raw("  "));

    // Skip button
    nav_spans.push(Span::styled(
        "[ (s)kip ]",
        Style::default().fg(palette::TEXT_MUTED),
    ));

    lines.push(Line::from(nav_spans));

    // Don't show again checkbox
    lines.push(Line::from(""));
    let checkbox = if tutorial.dont_show_again {
        "[x]"
    } else {
        "[ ]"
    };
    lines.push(Line::from(vec![
        Span::styled(
            format!("{} ", checkbox),
            Style::default().fg(palette::TEXT_MUTED),
        ),
        Span::styled(
            "Don't show this again (d)",
            Style::default().fg(palette::TEXT_MUTED),
        ),
    ]));

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .title(
                    Line::from(vec![Span::styled(
                        " Getting Started ",
                        Style::default().fg(palette::MINIMAX_BLUE).bold(),
                    )])
                    .alignment(Alignment::Center),
                )
                .borders(Borders::ALL)
                .border_style(Style::default().fg(palette::MINIMAX_BLUE)),
        )
        .alignment(Alignment::Center);

    f.render_widget(paragraph, popup_area);
}

/// Handle tutorial key events.
/// Returns true if the key was consumed by the tutorial.
pub fn handle_tutorial_key(tutorial: &mut Tutorial, key: crossterm::event::KeyEvent) -> bool {
    use crossterm::event::{KeyCode, KeyModifiers};

    match key.code {
        // Next step
        KeyCode::Char('n') | KeyCode::Char('N') => {
            if tutorial.is_last() {
                tutorial.save_preference();
            }
            tutorial.next();
            true
        }
        // Previous step
        KeyCode::Char('p') | KeyCode::Char('P') => {
            tutorial.previous();
            true
        }
        // Skip/quit
        KeyCode::Char('s')
        | KeyCode::Char('S')
        | KeyCode::Char('q')
        | KeyCode::Char('Q')
        | KeyCode::Esc => {
            tutorial.save_preference();
            tutorial.skip();
            true
        }
        // Finish on last step
        KeyCode::Char('f') | KeyCode::Char('F') | KeyCode::Enter => {
            if tutorial.is_last() {
                tutorial.save_preference();
            }
            tutorial.next();
            true
        }
        // Toggle don't show again
        KeyCode::Char('d') | KeyCode::Char('D') => {
            tutorial.toggle_dont_show();
            true
        }
        // Ctrl+C to exit even during tutorial
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            false // Let caller handle exit
        }
        _ => true, // Consume all other keys
    }
}
