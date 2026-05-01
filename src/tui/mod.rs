//! Hikyaku TUI — sidebar | message list | preview, plus a status bar.
//!
//! Rendered through [`egaku-term`](https://github.com/pleme-io/egaku-term):
//! the renderer borrows stdout (lifecycle is managed by `Terminal::enter`),
//! each pane is a [`bordered_block_with`] + manually-painted body inside
//! [`block_inner`], and the status bar is a [`status_line_with`].
//!
//! The async event loop stays in this module — egaku-term's sync `App`
//! runtime would conflict with the async-imap/sync paths the binary will
//! eventually drive. Drawers are sync; that's fine inside a tokio loop.

pub mod app;
pub mod theme;

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use egaku::Rect;
use egaku_term::crossterm::{
    QueueableCommand,
    cursor::MoveTo,
    style::{Print, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor},
};
use egaku_term::{Terminal, draw, theme::Palette};

use self::app::{App, Focus, View};
use self::theme::{Style, Theme};
use crate::config;

pub async fn run() -> anyhow::Result<()> {
    let cfg = config::load_config()?;
    let theme = Theme::nord(); // TODO: load from cfg.theme

    let mut app = App::new(theme, &cfg.accounts);

    // egaku-term owns terminal lifecycle: raw mode + alt screen + hide
    // cursor + Drop-safe restore (incl. on panic).
    let mut term = Terminal::enter()?;

    run_loop(&mut term, &mut app).await
}

async fn run_loop(term: &mut Terminal, app: &mut App) -> anyhow::Result<()> {
    while app.running {
        term.clear()?;
        draw_frame(term, app)?;
        term.flush()?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && key.code == KeyCode::Char('c')
                {
                    app.running = false;
                    continue;
                }
                match app.view {
                    View::Inbox | View::Thread => handle_navigation_key(app, key.code),
                    View::Compose => handle_compose_key(app, key.code),
                }
            }
        }
    }
    Ok(())
}

fn handle_navigation_key(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Char('q') => app.running = false,
        KeyCode::Char('j') | KeyCode::Down => app.move_down(),
        KeyCode::Char('k') | KeyCode::Up => app.move_up(),
        KeyCode::Tab => app.cycle_focus_forward(),
        KeyCode::BackTab => app.cycle_focus_backward(),
        KeyCode::Enter => app.open_message(),
        KeyCode::Esc => app.back(),
        KeyCode::Char('c') => app.enter_compose(),
        KeyCode::Char('n') => app.next_account(),
        KeyCode::Char('p') => app.prev_account(),
        _ => {}
    }
}

fn handle_compose_key(app: &mut App, key: KeyCode) {
    if key == KeyCode::Esc {
        app.exit_compose();
    }
}

// ── Drawing ──────────────────────────────────────────────────────────────────

fn draw_frame(term: &mut Terminal, app: &App) -> anyhow::Result<()> {
    let (cols, rows) = term.size().map_err(map_err)?;
    if cols == 0 || rows == 0 {
        return Ok(());
    }
    let cols_f = f32::from(cols);
    let rows_f = f32::from(rows);

    // Vertical: content (rows-1) | status (1)
    let content_h = rows_f - 1.0;

    // Horizontal: sidebar(24) | messages(min 30) | preview(rest)
    let sidebar_w: f32 = 24.0;
    let messages_w: f32 = ((cols_f - sidebar_w) * 0.4).max(30.0).min(cols_f - sidebar_w - 1.0);
    let preview_w = cols_f - sidebar_w - messages_w;

    fill_bg(term, app, cols, rows)?;

    let sidebar = Rect::new(0.0, 0.0, sidebar_w, content_h);
    let messages = Rect::new(sidebar_w, 0.0, messages_w, content_h);
    let preview = Rect::new(sidebar_w + messages_w, 0.0, preview_w, content_h);
    let status = Rect::new(0.0, content_h, cols_f, 1.0);

    draw_sidebar(term, app, sidebar)?;
    draw_message_list(term, app, messages)?;
    draw_preview(term, app, preview)?;
    draw_status_bar(term, app, status)?;
    Ok(())
}

fn fill_bg(term: &mut Terminal, app: &App, cols: u16, rows: u16) -> anyhow::Result<()> {
    let blank = " ".repeat(usize::from(cols));
    term.out()
        .queue(SetBackgroundColor(app.theme.bg()))?
        .queue(SetForegroundColor(app.theme.fg()))?;
    for r in 0..rows {
        term.out().queue(MoveTo(0, r))?.queue(Print(&blank))?;
    }
    term.out().queue(ResetColor)?;
    Ok(())
}

fn palette(theme: &Theme) -> Palette {
    Palette {
        background: theme.bg(),
        foreground: theme.fg(),
        accent: theme.accent(),
        error: theme.error_color(),
        warning: theme.warning_color(),
        success: theme.success_color(),
        selection: theme.selection_bg(),
        muted: theme.fg_muted(),
        border: theme.border_color(),
    }
}

fn draw_sidebar(term: &mut Terminal, app: &App, rect: Rect) -> anyhow::Result<()> {
    let pal = palette(&app.theme);
    let focused = app.focus == Focus::Sidebar;
    draw::bordered_block_with(term, rect, " Accounts ", focused, &pal).map_err(map_err)?;

    let inner = draw::block_inner(rect);
    let (ix, iy, iw, ih) = cells(inner);
    if iw == 0 || ih == 0 {
        return Ok(());
    }

    let mut row = 0u16;
    for (i, account) in app.accounts.iter().enumerate() {
        if row >= ih {
            break;
        }
        let indicator = if i == app.selected_account { "▶ " } else { "  " };
        let status = if account.connected { "●" } else { "○" };

        let style = if i == app.selected_account {
            app.theme.sidebar_selected()
        } else {
            app.theme.sidebar_item()
        };
        let line = format!("{indicator}{status} {}", account.name);
        paint_styled(term, ix, iy + row, iw, &line, style)?;
        row += 1;

        if i == app.selected_account {
            for (j, mailbox) in account.mailboxes.iter().enumerate() {
                if row >= ih {
                    break;
                }
                let style = if j == app.selected_mailbox {
                    app.theme.sidebar_selected()
                } else {
                    app.theme.sidebar_item()
                };
                let unread = if mailbox.unseen_count > 0 {
                    format!(" ({})", mailbox.unseen_count)
                } else {
                    String::new()
                };
                let line = format!("    {}{}", mailbox.name, unread);
                paint_styled(term, ix, iy + row, iw, &line, style)?;
                row += 1;
            }
        }
    }
    Ok(())
}

fn draw_message_list(term: &mut Terminal, app: &App, rect: Rect) -> anyhow::Result<()> {
    let pal = palette(&app.theme);
    let focused = app.focus == Focus::MessageList;
    let title = format!(
        " {} - {} ",
        app.current_account().map_or("", |a| a.name.as_str()),
        app.current_mailbox_name()
    );
    draw::bordered_block_with(term, rect, &title, focused, &pal).map_err(map_err)?;

    let inner = draw::block_inner(rect);
    let (ix, iy, iw, ih) = cells(inner);
    if iw == 0 || ih == 0 {
        return Ok(());
    }

    if app.messages.is_empty() {
        paint_styled(term, ix, iy, iw, "No messages", app.theme.text_muted())?;
        return Ok(());
    }

    for (i, msg) in app.messages.iter().enumerate().take(usize::from(ih)) {
        let row = u16::try_from(i).unwrap_or(u16::MAX);
        let is_selected = i == app.selected_message;
        let base = if is_selected {
            app.theme.message_selected()
        } else if msg.is_read {
            app.theme.message_read()
        } else {
            app.theme.message_unread()
        };
        let indicator = if msg.is_read { "  " } else { "● " };
        let line = format!(
            "{indicator}{} - {}  {}",
            msg.from, msg.subject, msg.date
        );
        paint_styled(term, ix, iy + row, iw, &line, base)?;
    }
    Ok(())
}

fn draw_preview(term: &mut Terminal, app: &App, rect: Rect) -> anyhow::Result<()> {
    let pal = palette(&app.theme);
    let focused = app.focus == Focus::Preview;
    draw::bordered_block_with(term, rect, " Preview ", focused, &pal).map_err(map_err)?;

    let inner = draw::block_inner(rect);
    let (ix, iy, iw, ih) = cells(inner);
    if iw == 0 || ih == 0 {
        return Ok(());
    }

    if app.messages.is_empty() {
        paint_styled(
            term,
            ix,
            iy,
            iw,
            "Select a message to preview",
            app.theme.text_muted(),
        )?;
        return Ok(());
    }

    let Some(msg) = app.messages.get(app.selected_message) else {
        paint_styled(term, ix, iy, iw, "No message selected", app.theme.text_muted())?;
        return Ok(());
    };

    // Header rows
    let mut row: u16 = 0;
    for (label, value, body_style) in [
        ("From: ", msg.from.as_str(), app.theme.text()),
        ("Subject: ", msg.subject.as_str(), app.theme.text_bright()),
        ("Date: ", msg.date.as_str(), app.theme.text_muted()),
    ] {
        if row >= ih {
            return Ok(());
        }
        paint_styled(term, ix, iy + row, iw, label, app.theme.preview_header())?;
        let label_w = u16::try_from(label.len()).unwrap_or(iw).min(iw);
        if label_w < iw {
            paint_styled(term, ix + label_w, iy + row, iw - label_w, value, body_style)?;
        }
        row += 1;
    }
    if row < ih {
        row += 1; // blank line
    }

    // Body — wrap with egaku-term, scroll by `app.preview_scroll`.
    let body_lines = draw::wrap_text(&msg.preview, iw);
    let scroll = usize::from(app.preview_scroll);
    for (i, line) in body_lines.iter().skip(scroll).enumerate() {
        let r = row + u16::try_from(i).unwrap_or(u16::MAX);
        if r >= ih {
            break;
        }
        paint_styled(term, ix, iy + r, iw, line, app.theme.preview_body())?;
    }
    Ok(())
}

fn draw_status_bar(term: &mut Terminal, app: &App, rect: Rect) -> anyhow::Result<()> {
    let mut pal = palette(&app.theme);
    pal.background = app.theme.surface_bg();
    pal.foreground = app.theme.fg();
    pal.selection = app.theme.surface_bg();

    let account_info = app
        .current_account()
        .map_or(String::new(), |a| format!(" {} ({}) ", a.name, a.address));
    let separator = if account_info.is_empty() { "" } else { " | " };
    let status_msg = app.status_message.as_deref().unwrap_or("");
    let left = format!("{account_info}{separator}{status_msg}");
    let right = " q:quit  j/k:nav  Tab:focus  c:compose  ?:help ".to_string();

    draw::status_line_with(term, rect, &left, &right, &pal).map_err(map_err)
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn paint_styled(
    term: &mut Terminal,
    col: u16,
    row: u16,
    max: u16,
    text: &str,
    style: Style,
) -> anyhow::Result<()> {
    if max == 0 {
        return Ok(());
    }
    let line: String = text.chars().take(usize::from(max)).collect();
    term.out()
        .queue(MoveTo(col, row))?
        .queue(SetForegroundColor(style.fg))?;
    if let Some(bg) = style.bg {
        term.out().queue(SetBackgroundColor(bg))?;
    }
    if !matches!(style.attr, Attribute::Reset) {
        term.out().queue(SetAttribute(style.attr))?;
    }
    term.out().queue(Print(line))?;
    if !matches!(style.attr, Attribute::Reset) {
        term.out().queue(SetAttribute(Attribute::Reset))?;
    }
    term.out().queue(ResetColor)?;
    Ok(())
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn cells(rect: Rect) -> (u16, u16, u16, u16) {
    let to_u16 = |f: f32| f.max(0.0).round().min(f32::from(u16::MAX)) as u16;
    (
        to_u16(rect.x),
        to_u16(rect.y),
        to_u16(rect.width),
        to_u16(rect.height),
    )
}

fn map_err(e: egaku_term::Error) -> anyhow::Error {
    anyhow::anyhow!("{e}")
}

use crossterm::style::Attribute;
