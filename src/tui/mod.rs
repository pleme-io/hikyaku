pub mod app;
pub mod theme;

use std::io;

use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};

use self::app::{App, Focus, View};
use self::theme::Theme;
use crate::config;

pub async fn run() -> anyhow::Result<()> {
    let cfg = config::load_config()?;
    let theme = Theme::nord(); // TODO: load from cfg.theme

    let mut app = App::new(theme, &cfg.accounts);

    // Set up terminal
    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    // Main loop
    let result = run_loop(&mut terminal, &mut app).await;

    // Restore terminal
    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;

    result
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> anyhow::Result<()> {
    while app.running {
        terminal.draw(|frame| draw(frame, app))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                // Ctrl+C always quits
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c')
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
    match key {
        KeyCode::Esc => app.exit_compose(),
        _ => {
            // TODO: handle text input for compose fields
        }
    }
}

// ── Drawing ──────────────────────────────────────────────────────────────────

fn draw(frame: &mut Frame, app: &App) {
    let size = frame.area();

    // Clear with background color
    frame.render_widget(
        Block::default().style(app.theme.background()),
        size,
    );

    // Main layout: sidebar | content | preview
    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),    // main content
            Constraint::Length(1), // status bar
        ])
        .split(size);

    let content_area = main_layout[0];
    let status_area = main_layout[1];

    // Horizontal split: sidebar | messages | preview
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(24), // sidebar
            Constraint::Min(30),   // message list
            Constraint::Min(40),   // preview
        ])
        .split(content_area);

    draw_sidebar(frame, app, columns[0]);
    draw_message_list(frame, app, columns[1]);
    draw_preview(frame, app, columns[2]);
    draw_status_bar(frame, app, status_area);
}

fn draw_sidebar(frame: &mut Frame, app: &App, area: Rect) {
    let is_focused = app.focus == Focus::Sidebar;
    let border_style = if is_focused {
        app.theme.border_focused()
    } else {
        app.theme.border()
    };

    let block = Block::default()
        .title(" Accounts ")
        .borders(Borders::ALL)
        .border_style(border_style)
        .style(app.theme.sidebar_bg());

    let mut items = Vec::new();

    for (i, account) in app.accounts.iter().enumerate() {
        let indicator = if i == app.selected_account {
            "\u{25B6} "
        } else {
            "  "
        };
        let status = if account.connected { "\u{25CF}" } else { "\u{25CB}" };

        let style = if i == app.selected_account {
            app.theme.sidebar_selected()
        } else {
            app.theme.sidebar_item()
        };

        items.push(ListItem::new(Line::from(vec![
            Span::styled(format!("{indicator}{status} "), style),
            Span::styled(&account.name, style),
        ])));

        // Show mailboxes for selected account
        if i == app.selected_account {
            for (j, mailbox) in account.mailboxes.iter().enumerate() {
                let mb_style = if j == app.selected_mailbox {
                    app.theme.sidebar_selected()
                } else {
                    app.theme.sidebar_item()
                };

                let unread = if mailbox.unseen_count > 0 {
                    format!(" ({})", mailbox.unseen_count)
                } else {
                    String::new()
                };

                items.push(ListItem::new(Line::from(vec![
                    Span::styled("    ", mb_style),
                    Span::styled(&mailbox.name, mb_style),
                    Span::styled(unread, app.theme.accent()),
                ])));
            }
        }
    }

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

fn draw_message_list(frame: &mut Frame, app: &App, area: Rect) {
    let is_focused = app.focus == Focus::MessageList;
    let border_style = if is_focused {
        app.theme.border_focused()
    } else {
        app.theme.border()
    };

    let title = format!(
        " {} - {} ",
        app.current_account()
            .map(|a| a.name.as_str())
            .unwrap_or(""),
        app.current_mailbox_name()
    );

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style)
        .style(app.theme.background());

    if app.messages.is_empty() {
        let empty = Paragraph::new("No messages")
            .style(app.theme.text_muted())
            .block(block);
        frame.render_widget(empty, area);
        return;
    }

    let items: Vec<ListItem> = app
        .messages
        .iter()
        .enumerate()
        .map(|(i, msg)| {
            let is_selected = i == app.selected_message;
            let base_style = if is_selected {
                app.theme.message_selected()
            } else if msg.is_read {
                app.theme.message_read()
            } else {
                app.theme.message_unread()
            };

            let indicator = if !msg.is_read { "\u{25CF} " } else { "  " };

            ListItem::new(Line::from(vec![
                Span::styled(indicator, base_style),
                Span::styled(&msg.from, app.theme.message_sender()),
                Span::styled(" - ", app.theme.text_muted()),
                Span::styled(&msg.subject, base_style),
                Span::styled(format!("  {}", msg.date), app.theme.message_date()),
            ]))
        })
        .collect();

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

fn draw_preview(frame: &mut Frame, app: &App, area: Rect) {
    let is_focused = app.focus == Focus::Preview;
    let border_style = if is_focused {
        app.theme.border_focused()
    } else {
        app.theme.border()
    };

    let block = Block::default()
        .title(" Preview ")
        .borders(Borders::ALL)
        .border_style(border_style)
        .style(app.theme.background());

    if app.messages.is_empty() {
        let empty = Paragraph::new("Select a message to preview")
            .style(app.theme.text_muted())
            .block(block);
        frame.render_widget(empty, area);
        return;
    }

    if let Some(msg) = app.messages.get(app.selected_message) {
        // TODO: render full message with HTML via ImageRenderer when available
        let content = vec![
            Line::from(vec![
                Span::styled("From: ", app.theme.preview_header()),
                Span::styled(&msg.from, app.theme.text()),
            ]),
            Line::from(vec![
                Span::styled("Subject: ", app.theme.preview_header()),
                Span::styled(&msg.subject, app.theme.text_bright()),
            ]),
            Line::from(vec![
                Span::styled("Date: ", app.theme.preview_header()),
                Span::styled(&msg.date, app.theme.text_muted()),
            ]),
            Line::from(""),
            Line::from(Span::styled(&msg.preview, app.theme.preview_body())),
        ];

        let paragraph = Paragraph::new(content)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((app.preview_scroll, 0));

        frame.render_widget(paragraph, area);
    } else {
        let empty = Paragraph::new("No message selected")
            .style(app.theme.text_muted())
            .block(block);
        frame.render_widget(empty, area);
    }
}

fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let account_info = app
        .current_account()
        .map(|a| format!(" {} ({}) ", a.name, a.address))
        .unwrap_or_default();

    let help = " q:quit  j/k:nav  Tab:focus  c:compose  ?:help ";

    let status = Line::from(vec![
        Span::styled(account_info, app.theme.status_bar_accent()),
        Span::styled(" | ", app.theme.status_bar()),
        Span::styled(
            app.status_message.as_deref().unwrap_or(""),
            app.theme.status_bar(),
        ),
        Span::styled(
            format!(
                "{:>width$}",
                help,
                width = area.width as usize
            ),
            app.theme.status_bar(),
        ),
    ]);

    let bar = Paragraph::new(status).style(app.theme.status_bar());
    frame.render_widget(bar, area);
}
