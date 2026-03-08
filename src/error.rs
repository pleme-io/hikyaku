use thiserror::Error;

#[derive(Debug, Error)]
pub enum HikyakuError {
    #[error("configuration error: {0}")]
    Config(String),

    #[error("IMAP error: {0}")]
    Imap(String),

    #[error("SMTP error: {0}")]
    Smtp(String),

    #[error("OAuth2 error: {0}")]
    OAuth2(String),

    #[error("authentication failed for account {account}: {reason}")]
    Auth { account: String, reason: String },

    #[error("account not found: {0}")]
    AccountNotFound(String),

    #[error("mailbox not found: {0}")]
    MailboxNotFound(String),

    #[error("render error: {0}")]
    Render(String),

    #[error("scripting error: {0}")]
    Script(String),

    #[error("IPC error: {0}")]
    Ipc(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Shikumi(#[from] shikumi::ShikumiError),
}

pub type Result<T> = std::result::Result<T, HikyakuError>;
