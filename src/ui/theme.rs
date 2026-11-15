use iced::theme::Palette;
use iced::{color, Theme};

/// A single, minimal dark theme with muted grays and no accent color overload.
pub fn minimal() -> Theme {
    Theme::custom("Minimal", MINIMAL)
}

const MINIMAL: Palette = Palette {
    background: color!(0x18181b),
    text: color!(0xf4f4f5),
    primary: color!(0x52525b),
    success: color!(0x6b9e7c),
    danger: color!(0xb06a6a),
    warning: color!(0xc4a45a),
};
