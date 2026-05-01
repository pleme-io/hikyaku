//! Resolved color palette for the hikyaku TUI.
//!
//! Several base16 slots and semantic accessors are pre-populated for
//! future TUI expansion (compose pane, attachment list, thread view).
//! `dead_code` is silenced at file scope since hikyaku ships as a
//! single binary — items not yet wired up still belong here.

#![allow(dead_code)]

//!
//! Re-shaped to expose `crossterm::style::Color` directly so the
//! egaku-term drawers can pick fields off as plain colors. Modifier
//! flags (bold, etc.) are returned alongside fg/bg as small `Style`
//! tuples — egaku-term doesn't take a Style abstraction; the caller
//! queues `SetAttribute(Bold)` when needed.
//!
//! The full Nord palette (nord0..nord15) is preserved as the source
//! of truth; semantic accessors compose from those slots.

use crossterm::style::{Attribute, Color};

/// Tuple of (foreground, background, attribute) for one styled span.
/// Background can be `None` to inherit, attribute is `Reset` for plain.
#[derive(Debug, Clone, Copy)]
pub struct Style {
    pub fg: Color,
    pub bg: Option<Color>,
    pub attr: Attribute,
}

impl Style {
    pub const fn fg(color: Color) -> Self {
        Self {
            fg: color,
            bg: None,
            attr: Attribute::Reset,
        }
    }

    pub const fn bg(self, color: Color) -> Self {
        Self {
            bg: Some(color),
            ..self
        }
    }

    pub const fn bold(self) -> Self {
        Self {
            attr: Attribute::Bold,
            ..self
        }
    }
}

/// Nord palette + semantic accessors.
#[derive(Debug, Clone)]
pub struct Theme {
    // Polar Night
    pub nord0: Color,
    pub nord1: Color,
    pub nord2: Color,
    pub nord3: Color,
    // Snow Storm
    pub nord4: Color,
    pub nord5: Color,
    pub nord6: Color,
    // Frost
    pub nord7: Color,
    pub nord8: Color,
    pub nord9: Color,
    pub nord10: Color,
    // Aurora
    pub nord11: Color,
    pub nord12: Color,
    pub nord13: Color,
    pub nord14: Color,
    pub nord15: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self::nord()
    }
}

impl Theme {
    pub const fn nord() -> Self {
        Self {
            nord0: Color::Rgb { r: 46, g: 52, b: 64 },
            nord1: Color::Rgb { r: 59, g: 66, b: 82 },
            nord2: Color::Rgb { r: 67, g: 76, b: 94 },
            nord3: Color::Rgb { r: 76, g: 86, b: 106 },
            nord4: Color::Rgb { r: 216, g: 222, b: 233 },
            nord5: Color::Rgb { r: 229, g: 233, b: 240 },
            nord6: Color::Rgb { r: 236, g: 239, b: 244 },
            nord7: Color::Rgb { r: 143, g: 188, b: 187 },
            nord8: Color::Rgb { r: 136, g: 192, b: 208 },
            nord9: Color::Rgb { r: 129, g: 161, b: 193 },
            nord10: Color::Rgb { r: 94, g: 129, b: 172 },
            nord11: Color::Rgb { r: 191, g: 97, b: 106 },
            nord12: Color::Rgb { r: 208, g: 135, b: 112 },
            nord13: Color::Rgb { r: 235, g: 203, b: 139 },
            nord14: Color::Rgb { r: 163, g: 190, b: 140 },
            nord15: Color::Rgb { r: 180, g: 142, b: 173 },
        }
    }

    // Background / foreground / accent colors used directly by the
    // egaku-term `Palette` adapter. Returning Color (not Style) so the
    // adapter doesn't have to peek inside.

    pub const fn bg(&self) -> Color { self.nord0 }
    pub const fn fg(&self) -> Color { self.nord4 }
    pub const fn fg_bright(&self) -> Color { self.nord6 }
    pub const fn fg_muted(&self) -> Color { self.nord3 }
    pub const fn accent(&self) -> Color { self.nord8 }
    pub const fn selection_bg(&self) -> Color { self.nord2 }
    pub const fn surface_bg(&self) -> Color { self.nord1 }
    pub const fn border_color(&self) -> Color { self.nord3 }
    pub const fn border_focused_color(&self) -> Color { self.nord8 }
    pub const fn error_color(&self) -> Color { self.nord11 }
    pub const fn warning_color(&self) -> Color { self.nord12 }
    pub const fn info_color(&self) -> Color { self.nord13 }
    pub const fn success_color(&self) -> Color { self.nord14 }

    // Style bundles for spans.

    pub fn text(&self) -> Style { Style::fg(self.nord4) }
    pub fn text_bright(&self) -> Style { Style::fg(self.nord6) }
    pub fn text_muted(&self) -> Style { Style::fg(self.nord3) }
    pub fn accent_style(&self) -> Style { Style::fg(self.nord8) }
    pub fn message_unread(&self) -> Style { Style::fg(self.nord6).bold() }
    pub fn message_read(&self) -> Style { Style::fg(self.nord4) }
    pub fn message_sender(&self) -> Style { Style::fg(self.nord8) }
    pub fn message_date(&self) -> Style { Style::fg(self.nord3) }
    pub fn message_selected(&self) -> Style { Style::fg(self.nord6).bg(self.nord2) }
    pub fn sidebar_item(&self) -> Style { Style::fg(self.nord4).bg(self.nord1) }
    pub fn sidebar_selected(&self) -> Style { Style::fg(self.nord8).bg(self.nord2).bold() }
    pub fn sidebar_header(&self) -> Style { Style::fg(self.nord9).bg(self.nord1).bold() }
    pub fn preview_header(&self) -> Style { Style::fg(self.nord9) }
    pub fn preview_body(&self) -> Style { Style::fg(self.nord4) }
    pub fn status_bar(&self) -> Style { Style::fg(self.nord4).bg(self.nord1) }
    pub fn status_bar_accent(&self) -> Style { Style::fg(self.nord8).bg(self.nord1) }
    pub fn error(&self) -> Style { Style::fg(self.nord11) }
    pub fn warning(&self) -> Style { Style::fg(self.nord12) }
    pub fn info(&self) -> Style { Style::fg(self.nord13) }
    pub fn success(&self) -> Style { Style::fg(self.nord14) }
}
