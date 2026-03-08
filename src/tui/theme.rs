use ratatui::style::{Color, Modifier, Style};

/// Resolved color palette for the TUI.
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
    pub fn nord() -> Self {
        Self {
            nord0: Color::Rgb(46, 52, 64),
            nord1: Color::Rgb(59, 66, 82),
            nord2: Color::Rgb(67, 76, 94),
            nord3: Color::Rgb(76, 86, 106),
            nord4: Color::Rgb(216, 222, 233),
            nord5: Color::Rgb(229, 233, 240),
            nord6: Color::Rgb(236, 239, 244),
            nord7: Color::Rgb(143, 188, 187),
            nord8: Color::Rgb(136, 192, 208),
            nord9: Color::Rgb(129, 161, 193),
            nord10: Color::Rgb(94, 129, 172),
            nord11: Color::Rgb(191, 97, 106),
            nord12: Color::Rgb(208, 135, 112),
            nord13: Color::Rgb(235, 203, 139),
            nord14: Color::Rgb(163, 190, 140),
            nord15: Color::Rgb(180, 142, 173),
        }
    }

    // ── Semantic styles ──────────────────────────────────────────────────

    pub fn background(&self) -> Style {
        Style::default().bg(self.nord0)
    }

    pub fn surface(&self) -> Style {
        Style::default().bg(self.nord1)
    }

    pub fn text(&self) -> Style {
        Style::default().fg(self.nord4)
    }

    pub fn text_bright(&self) -> Style {
        Style::default().fg(self.nord6)
    }

    pub fn text_muted(&self) -> Style {
        Style::default().fg(self.nord3)
    }

    pub fn accent(&self) -> Style {
        Style::default().fg(self.nord8)
    }

    pub fn selected(&self) -> Style {
        Style::default().bg(self.nord2).fg(self.nord6)
    }

    pub fn border(&self) -> Style {
        Style::default().fg(self.nord3)
    }

    pub fn border_focused(&self) -> Style {
        Style::default().fg(self.nord8)
    }

    pub fn status_bar(&self) -> Style {
        Style::default().bg(self.nord1).fg(self.nord4)
    }

    pub fn status_bar_accent(&self) -> Style {
        Style::default().bg(self.nord1).fg(self.nord8)
    }

    // ── Message list styles ──────────────────────────────────────────────

    pub fn message_unread(&self) -> Style {
        Style::default()
            .fg(self.nord6)
            .add_modifier(Modifier::BOLD)
    }

    pub fn message_read(&self) -> Style {
        Style::default().fg(self.nord4)
    }

    pub fn message_sender(&self) -> Style {
        Style::default().fg(self.nord8)
    }

    pub fn message_date(&self) -> Style {
        Style::default().fg(self.nord3)
    }

    pub fn message_selected(&self) -> Style {
        Style::default().bg(self.nord2).fg(self.nord6)
    }

    // ── Sidebar styles ───────────────────────────────────────────────────

    pub fn sidebar_bg(&self) -> Style {
        Style::default().bg(self.nord1)
    }

    pub fn sidebar_item(&self) -> Style {
        Style::default().fg(self.nord4).bg(self.nord1)
    }

    pub fn sidebar_selected(&self) -> Style {
        Style::default()
            .bg(self.nord2)
            .fg(self.nord8)
            .add_modifier(Modifier::BOLD)
    }

    pub fn sidebar_header(&self) -> Style {
        Style::default()
            .fg(self.nord9)
            .bg(self.nord1)
            .add_modifier(Modifier::BOLD)
    }

    // ── Preview pane styles ──────────────────────────────────────────────

    pub fn preview_header(&self) -> Style {
        Style::default().fg(self.nord9)
    }

    pub fn preview_body(&self) -> Style {
        Style::default().fg(self.nord4)
    }

    pub fn preview_quote(&self) -> Style {
        Style::default().fg(self.nord3)
    }

    // ── Compose styles ───────────────────────────────────────────────────

    pub fn compose_label(&self) -> Style {
        Style::default().fg(self.nord9)
    }

    pub fn compose_input(&self) -> Style {
        Style::default().fg(self.nord4)
    }

    // ── Semantic colors ──────────────────────────────────────────────────

    pub fn error(&self) -> Style {
        Style::default().fg(self.nord11)
    }

    pub fn warning(&self) -> Style {
        Style::default().fg(self.nord12)
    }

    pub fn info(&self) -> Style {
        Style::default().fg(self.nord13)
    }

    pub fn success(&self) -> Style {
        Style::default().fg(self.nord14)
    }
}
