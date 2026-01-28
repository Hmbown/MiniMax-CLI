//! Header bar widget displaying mode, model, context usage, and streaming state.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};
use unicode_width::UnicodeWidthStr;

use crate::models::context_window_for_model;
use crate::palette;
use crate::tui::app::{AppMode, PinnedMessage};

use super::Renderable;

/// Data required to render the header bar.
pub struct HeaderData<'a> {
    pub mode: AppMode,
    pub model: &'a str,
    pub context_used: u32,
    pub context_max: Option<u32>,
    pub is_streaming: bool,
    pub background: ratatui::style::Color,
    pub shell_mode: bool,
    pub pins: Vec<&'a PinnedMessage>,
}

impl<'a> HeaderData<'a> {
    /// Create header data from common app fields.
    #[must_use]
    pub fn new(
        mode: AppMode,
        model: &'a str,
        context_used: u32,
        is_streaming: bool,
        background: ratatui::style::Color,
    ) -> Self {
        let context_max = context_window_for_model(model);
        Self {
            mode,
            model,
            context_used,
            context_max,
            is_streaming,
            background,
            shell_mode: false,
            pins: Vec::new(),
        }
    }

    /// Set shell mode status.
    #[must_use]
    pub fn with_shell_mode(mut self, shell_mode: bool) -> Self {
        self.shell_mode = shell_mode;
        self
    }

    /// Set pinned messages.
    #[must_use]
    pub fn with_pins(mut self, pins: Vec<&'a PinnedMessage>) -> Self {
        self.pins = pins;
        self
    }

    /// Calculate context usage as a percentage (0-100).
    fn context_percent(&self) -> u8 {
        match self.context_max {
            Some(max) if max > 0 => {
                let used = u64::from(self.context_used);
                let max = u64::from(max);
                let percent = (used.saturating_mul(100) / max).min(100);
                percent as u8
            }
            _ => 0,
        }
    }

    /// Get the remaining context percentage.
    fn context_remaining_percent(&self) -> u8 {
        100u8.saturating_sub(self.context_percent())
    }
}

/// Header bar widget (1-2 lines height).
///
/// Layout: `[MODE] | model-name | Context: XX% | [streaming indicator]`
/// If pins exist, a second line shows: `📌 [source] preview`
pub struct HeaderWidget<'a> {
    data: HeaderData<'a>,
}

impl<'a> HeaderWidget<'a> {
    #[must_use]
    pub fn new(data: HeaderData<'a>) -> Self {
        Self { data }
    }

    /// Build the mode badge span with color coding.
    fn mode_badge(&self) -> Span<'static> {
        // Show SHELL mode when shell_mode is enabled
        if self.data.shell_mode {
            return Span::styled(
                " SHELL ",
                Style::default()
                    .fg(palette::TEXT_PRIMARY)
                    .bg(palette::MINIMAX_GREEN)
                    .add_modifier(Modifier::BOLD),
            );
        }

        let (label, bg_color) = match self.data.mode {
            AppMode::Normal => ("NORMAL", palette::MINIMAX_SLATE),
            AppMode::Plan => ("PLAN", palette::MINIMAX_ORANGE),
            AppMode::Agent => ("AGENT", palette::MINIMAX_BLUE),
            AppMode::Yolo => ("YOLO", palette::STATUS_ERROR),
            AppMode::Rlm => ("RLM", palette::MINIMAX_INK),
            AppMode::Duo => ("DUO", palette::MINIMAX_MAGENTA),
        };

        Span::styled(
            format!(" {label} "),
            Style::default()
                .fg(palette::TEXT_PRIMARY)
                .bg(bg_color)
                .add_modifier(Modifier::BOLD),
        )
    }

    /// Build the model name span.
    fn model_span(&self) -> Span<'static> {
        let display_name = if self.data.model.len() > 20 {
            format!("{}...", &self.data.model[..17])
        } else {
            self.data.model.to_string()
        };

        Span::styled(display_name, Style::default().fg(palette::TEXT_MUTED))
    }

    /// Build the context meter span with color based on usage.
    fn context_meter(&self) -> Span<'static> {
        let remaining = self.data.context_remaining_percent();
        let used_percent = self.data.context_percent();

        let color = if remaining <= 10 {
            palette::STATUS_ERROR
        } else if remaining <= 25 {
            palette::STATUS_WARNING
        } else {
            palette::STATUS_INFO
        };

        let bar_width = 8;
        let filled = ((used_percent as usize) * bar_width / 100).min(bar_width);
        let empty = bar_width.saturating_sub(filled);
        let bar: String = format!("[{}{}]", "#".repeat(filled), ".".repeat(empty));

        Span::styled(format!("{bar} {remaining}%"), Style::default().fg(color))
    }

    /// Build the streaming indicator span.
    fn streaming_indicator(&self) -> Option<Span<'static>> {
        if !self.data.is_streaming {
            return None;
        }

        Some(Span::styled(
            " streaming... ",
            Style::default()
                .fg(palette::STATUS_INFO)
                .add_modifier(Modifier::BOLD),
        ))
    }
    /// Build a pin display span for the pins line.
    fn pin_spans(&self) -> Vec<Span<'static>> {
        if self.data.pins.is_empty() {
            return Vec::new();
        }

        let mut spans = Vec::new();
        spans.push(Span::styled(
            "📌 ",
            Style::default().fg(palette::MINIMAX_YELLOW),
        ));

        for (idx, pin) in self.data.pins.iter().enumerate() {
            let source_color = match pin.source {
                crate::tui::app::PinSource::User => palette::MINIMAX_ORANGE,
                crate::tui::app::PinSource::Assistant => palette::MINIMAX_BLUE,
            };
            let source_label = match pin.source {
                crate::tui::app::PinSource::User => "You",
                crate::tui::app::PinSource::Assistant => "MiniMax",
            };
            let preview = pin.preview();

            spans.push(Span::styled(
                format!("[{}]", source_label),
                Style::default().fg(source_color),
            ));
            spans.push(Span::styled(
                format!(" {}", preview),
                Style::default().fg(palette::TEXT_MUTED),
            ));

            if idx < self.data.pins.len() - 1 {
                spans.push(Span::styled(
                    " | ",
                    Style::default().fg(palette::TEXT_MUTED),
                ));
            }
        }

        spans
    }
}

impl Renderable for HeaderWidget<'_> {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        let has_pins = !self.data.pins.is_empty();
        let header_height = if has_pins { 2 } else { 1 };

        if area.height < header_height {
            // Not enough space, render minimal header
            let line = Line::from(vec![self.mode_badge()]);
            let paragraph = Paragraph::new(line).style(Style::default().bg(self.data.background));
            paragraph.render(area, buf);
            return;
        }

        // Render main header line
        let mut left_spans = vec![
            self.mode_badge(),
            Span::styled(" | ", Style::default().fg(palette::TEXT_MUTED)),
            self.model_span(),
        ];

        let context_span = self.context_meter();
        let streaming_span = self.streaming_indicator();

        let left_width: usize = left_spans.iter().map(|s| s.content.width()).sum();
        let context_width = context_span.content.width();
        let right_width = streaming_span.as_ref().map_or(0, |s| s.content.width());

        let total_content = left_width + context_width + right_width + 4;
        let available = area.width as usize;

        let mut spans = Vec::new();

        if available >= total_content {
            spans.append(&mut left_spans);
            spans.push(Span::styled(
                " | ",
                Style::default().fg(palette::TEXT_MUTED),
            ));
            spans.push(context_span);

            if let Some(streaming) = streaming_span {
                let padding_needed =
                    available.saturating_sub(left_width + 3 + context_width + right_width);
                if padding_needed > 0 {
                    spans.push(Span::raw(" ".repeat(padding_needed)));
                }
                spans.push(streaming);
            }
        } else if available >= left_width + context_width + 3 {
            spans.append(&mut left_spans);
            spans.push(Span::styled(
                " | ",
                Style::default().fg(palette::TEXT_MUTED),
            ));
            spans.push(context_span);
        } else if available >= left_width {
            spans.append(&mut left_spans);
        } else {
            spans.push(self.mode_badge());
        }

        let line = Line::from(spans);
        let paragraph = Paragraph::new(line).style(Style::default().bg(self.data.background));

        let header_area = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: 1,
        };
        paragraph.render(header_area, buf);

        // Render pins line if there are pins and enough space
        if has_pins && area.height >= 2 {
            let pin_spans = self.pin_spans();
            let pin_line = Line::from(pin_spans);
            let pin_paragraph =
                Paragraph::new(pin_line).style(Style::default().bg(self.data.background));

            let pin_area = Rect {
                x: area.x,
                y: area.y + 1,
                width: area.width,
                height: 1,
            };
            pin_paragraph.render(pin_area, buf);
        }
    }

    fn desired_height(&self, _width: u16) -> u16 {
        if self.data.pins.is_empty() {
            1
        } else {
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_percent_calculation() {
        let data = HeaderData {
            mode: AppMode::Normal,
            model: "minimax-m2.1",
            context_used: 64_000,
            context_max: Some(128_000),
            is_streaming: false,
            background: palette::MINIMAX_INK,
            shell_mode: false,
            pins: Vec::new(),
        };
        assert_eq!(data.context_percent(), 50);
        assert_eq!(data.context_remaining_percent(), 50);
    }

    #[test]
    fn test_context_percent_zero() {
        let data = HeaderData {
            mode: AppMode::Normal,
            model: "minimax-m2.1",
            context_used: 0,
            context_max: Some(128_000),
            is_streaming: false,
            background: palette::MINIMAX_INK,
            shell_mode: false,
            pins: Vec::new(),
        };
        assert_eq!(data.context_percent(), 0);
        assert_eq!(data.context_remaining_percent(), 100);
    }

    #[test]
    fn test_context_percent_no_max() {
        let data = HeaderData {
            mode: AppMode::Normal,
            model: "unknown-model",
            context_used: 1000,
            context_max: None,
            is_streaming: false,
            background: palette::MINIMAX_INK,
            shell_mode: false,
            pins: Vec::new(),
        };
        assert_eq!(data.context_percent(), 0);
        assert_eq!(data.context_remaining_percent(), 100);
    }
}
