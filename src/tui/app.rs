use std::collections::HashMap;

use crate::accounts::{Mailbox, MessageSummary};
use crate::tui::theme::Theme;

/// Active view in the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Inbox,
    Thread,
    Compose,
}

/// Which pane has focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Sidebar,
    MessageList,
    Preview,
}

/// TUI application state.
pub struct App {
    pub theme: Theme,
    pub view: View,
    pub focus: Focus,
    pub running: bool,

    // Sidebar
    pub accounts: Vec<AccountEntry>,
    pub selected_account: usize,
    pub selected_mailbox: usize,

    // Message list
    pub messages: Vec<MessageSummary>,
    pub selected_message: usize,
    pub message_scroll: usize,

    // Preview
    pub preview_scroll: u16,

    // Compose
    pub compose: ComposeState,

    // Status
    pub status_message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AccountEntry {
    pub name: String,
    pub address: String,
    pub mailboxes: Vec<Mailbox>,
    pub connected: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ComposeState {
    pub to: String,
    pub cc: String,
    pub bcc: String,
    pub subject: String,
    pub body: String,
    pub from_account: Option<String>,
    pub active_field: ComposeField,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ComposeField {
    #[default]
    To,
    Cc,
    Bcc,
    Subject,
    Body,
}

impl App {
    pub fn new(theme: Theme, account_configs: &HashMap<String, crate::config::AccountConfig>) -> Self {
        let accounts: Vec<AccountEntry> = account_configs
            .iter()
            .map(|(name, cfg)| AccountEntry {
                name: name.clone(),
                address: cfg.address.clone(),
                mailboxes: Vec::new(),
                connected: false,
            })
            .collect();

        Self {
            theme,
            view: View::Inbox,
            focus: Focus::MessageList,
            running: true,
            accounts,
            selected_account: 0,
            selected_mailbox: 0,
            messages: Vec::new(),
            selected_message: 0,
            message_scroll: 0,
            preview_scroll: 0,
            compose: ComposeState::default(),
            status_message: Some("Press ? for help".into()),
        }
    }

    pub fn current_account(&self) -> Option<&AccountEntry> {
        self.accounts.get(self.selected_account)
    }

    pub fn current_mailbox_name(&self) -> &str {
        self.current_account()
            .and_then(|a| a.mailboxes.get(self.selected_mailbox))
            .map(|m| m.name.as_str())
            .unwrap_or("INBOX")
    }

    // ── Navigation ───────────────────────────────────────────────────────

    pub fn move_up(&mut self) {
        match self.focus {
            Focus::Sidebar => {
                if self.selected_mailbox > 0 {
                    self.selected_mailbox -= 1;
                }
            }
            Focus::MessageList => {
                if self.selected_message > 0 {
                    self.selected_message -= 1;
                }
            }
            Focus::Preview => {
                self.preview_scroll = self.preview_scroll.saturating_sub(1);
            }
        }
    }

    pub fn move_down(&mut self) {
        match self.focus {
            Focus::Sidebar => {
                if let Some(account) = self.current_account() {
                    if self.selected_mailbox + 1 < account.mailboxes.len() {
                        self.selected_mailbox += 1;
                    }
                }
            }
            Focus::MessageList => {
                if self.selected_message + 1 < self.messages.len() {
                    self.selected_message += 1;
                }
            }
            Focus::Preview => {
                self.preview_scroll += 1;
            }
        }
    }

    pub fn cycle_focus_forward(&mut self) {
        self.focus = match self.focus {
            Focus::Sidebar => Focus::MessageList,
            Focus::MessageList => Focus::Preview,
            Focus::Preview => Focus::Sidebar,
        };
    }

    pub fn cycle_focus_backward(&mut self) {
        self.focus = match self.focus {
            Focus::Sidebar => Focus::Preview,
            Focus::MessageList => Focus::Sidebar,
            Focus::Preview => Focus::MessageList,
        };
    }

    pub fn next_account(&mut self) {
        if !self.accounts.is_empty() {
            self.selected_account = (self.selected_account + 1) % self.accounts.len();
            self.selected_mailbox = 0;
            self.selected_message = 0;
        }
    }

    pub fn prev_account(&mut self) {
        if !self.accounts.is_empty() {
            self.selected_account = if self.selected_account == 0 {
                self.accounts.len() - 1
            } else {
                self.selected_account - 1
            };
            self.selected_mailbox = 0;
            self.selected_message = 0;
        }
    }

    pub fn enter_compose(&mut self) {
        self.view = View::Compose;
        self.compose = ComposeState {
            from_account: self.current_account().map(|a| a.name.clone()),
            ..Default::default()
        };
    }

    pub fn exit_compose(&mut self) {
        self.view = View::Inbox;
    }

    pub fn open_message(&mut self) {
        if !self.messages.is_empty() {
            self.view = View::Thread;
            self.preview_scroll = 0;
        }
    }

    pub fn back(&mut self) {
        match self.view {
            View::Thread => self.view = View::Inbox,
            View::Compose => self.view = View::Inbox,
            View::Inbox => {}
        }
    }
}
