//! MiniMax color palette and semantic roles.

use ratatui::style::Color;

pub const MINIMAX_BLUE_RGB: (u8, u8, u8) = (20, 86, 240); // #1456F0
pub const MINIMAX_RED_RGB: (u8, u8, u8) = (242, 63, 93); // #F23F5D
pub const MINIMAX_ORANGE_RGB: (u8, u8, u8) = (255, 99, 58); // #FF633A
pub const MINIMAX_MAGENTA_RGB: (u8, u8, u8) = (228, 23, 127); // #E4177F
pub const MINIMAX_INK_RGB: (u8, u8, u8) = (24, 30, 37); // #181E25
pub const MINIMAX_BLACK_RGB: (u8, u8, u8) = (10, 13, 13); // #0A0D0D
pub const MINIMAX_SLATE_RGB: (u8, u8, u8) = (53, 60, 67); // #353C43
pub const MINIMAX_SILVER_RGB: (u8, u8, u8) = (201, 205, 212); // #C9CDD4
pub const MINIMAX_SNOW_RGB: (u8, u8, u8) = (247, 248, 250); // #F7F8FA
pub const MINIMAX_GREEN_RGB: (u8, u8, u8) = (74, 222, 128); // #4ADE80

pub const MINIMAX_BLUE: Color =
    Color::Rgb(MINIMAX_BLUE_RGB.0, MINIMAX_BLUE_RGB.1, MINIMAX_BLUE_RGB.2);
pub const MINIMAX_RED: Color = Color::Rgb(MINIMAX_RED_RGB.0, MINIMAX_RED_RGB.1, MINIMAX_RED_RGB.2);
pub const MINIMAX_ORANGE: Color = Color::Rgb(
    MINIMAX_ORANGE_RGB.0,
    MINIMAX_ORANGE_RGB.1,
    MINIMAX_ORANGE_RGB.2,
);
pub const MINIMAX_MAGENTA: Color = Color::Rgb(
    MINIMAX_MAGENTA_RGB.0,
    MINIMAX_MAGENTA_RGB.1,
    MINIMAX_MAGENTA_RGB.2,
);
pub const MINIMAX_INK: Color = Color::Rgb(MINIMAX_INK_RGB.0, MINIMAX_INK_RGB.1, MINIMAX_INK_RGB.2);
pub const MINIMAX_BLACK: Color = Color::Rgb(
    MINIMAX_BLACK_RGB.0,
    MINIMAX_BLACK_RGB.1,
    MINIMAX_BLACK_RGB.2,
);
pub const MINIMAX_SLATE: Color = Color::Rgb(
    MINIMAX_SLATE_RGB.0,
    MINIMAX_SLATE_RGB.1,
    MINIMAX_SLATE_RGB.2,
);
pub const MINIMAX_SILVER: Color = Color::Rgb(
    MINIMAX_SILVER_RGB.0,
    MINIMAX_SILVER_RGB.1,
    MINIMAX_SILVER_RGB.2,
);
pub const MINIMAX_SNOW: Color =
    Color::Rgb(MINIMAX_SNOW_RGB.0, MINIMAX_SNOW_RGB.1, MINIMAX_SNOW_RGB.2);
pub const MINIMAX_GREEN: Color = Color::Rgb(
    MINIMAX_GREEN_RGB.0,
    MINIMAX_GREEN_RGB.1,
    MINIMAX_GREEN_RGB.2,
);

pub const TEXT_PRIMARY: Color = MINIMAX_SNOW;
pub const TEXT_MUTED: Color = MINIMAX_SILVER;
pub const TEXT_DIM: Color = MINIMAX_SLATE;

pub const STATUS_SUCCESS: Color = MINIMAX_GREEN;
pub const STATUS_WARNING: Color = MINIMAX_ORANGE;
pub const STATUS_ERROR: Color = MINIMAX_RED;
pub const STATUS_INFO: Color = MINIMAX_BLUE;

pub const SELECTION_BG: Color = MINIMAX_SLATE;
pub const COMPOSER_BG: Color = MINIMAX_INK;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiTheme {
    pub name: &'static str,
    pub composer_bg: Color,
    pub selection_bg: Color,
    pub header_bg: Color,
}

pub fn ui_theme(name: &str) -> UiTheme {
    match name.to_ascii_lowercase().as_str() {
        "dark" => UiTheme {
            name: "dark",
            composer_bg: MINIMAX_BLACK,
            selection_bg: MINIMAX_INK,
            header_bg: MINIMAX_BLACK,
        },
        "light" => UiTheme {
            name: "light",
            composer_bg: MINIMAX_SLATE,
            selection_bg: MINIMAX_SILVER,
            header_bg: MINIMAX_SLATE,
        },
        _ => UiTheme {
            name: "default",
            composer_bg: COMPOSER_BG,
            selection_bg: SELECTION_BG,
            header_bg: MINIMAX_BLACK,
        },
    }
}
