//! MCP server for programmatic email management.
//!
//! Each tool call creates a fresh IMAP connection, performs the operation,
//! and disconnects. For reads, the local index is used when available.

use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
    transport::stdio,
};
use serde::Deserialize;

use crate::accounts::{Account, AccountManager, OutgoingMessage};
use crate::config;
use crate::index::EmailIndex;

// ── Tool input types ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct AccountInput {
    /// Account name from config (omit for default account)
    account: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ListMessagesInput {
    /// Account name from config (omit for default account)
    account: Option<String>,
    /// Mailbox/folder name (default: INBOX)
    mailbox: Option<String>,
    /// Maximum messages to return (default: 20)
    limit: Option<u32>,
    /// Only return unread messages
    unread_only: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ReadMessageInput {
    /// Account name from config (omit for default account)
    account: Option<String>,
    /// Mailbox containing the message
    mailbox: Option<String>,
    /// Message UID
    uid: u32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SearchInput {
    /// Search query string (searches subject, from, and body)
    query: String,
    /// Maximum results (default: 20)
    limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SendMessageInput {
    /// Account to send from (omit for default)
    account: Option<String>,
    /// Recipient email address(es), comma-separated
    to: String,
    /// Email subject
    subject: String,
    /// Email body (plain text)
    body: String,
    /// CC recipients, comma-separated
    cc: Option<String>,
    /// BCC recipients, comma-separated
    bcc: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct MoveMessageInput {
    /// Account name
    account: Option<String>,
    /// Source mailbox
    from_mailbox: String,
    /// Destination mailbox
    to_mailbox: String,
    /// Message UID
    uid: u32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct DeleteMessageInput {
    /// Account name
    account: Option<String>,
    /// Mailbox containing the message
    mailbox: Option<String>,
    /// Message UID
    uid: u32,
    /// Permanently delete instead of moving to trash
    permanent: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct RunScriptInput {
    /// Rhai script source code to execute
    script: String,
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn json_err(e: impl std::fmt::Display) -> String {
    format!(r#"{{"error":"{}"}}"#, e.to_string().replace('"', "'"))
}

async fn with_account<F, Fut>(
    account_name: Option<&str>,
    f: F,
) -> String
where
    F: FnOnce(Account) -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<serde_json::Value>>,
{
    let cfg = match config::load_config() {
        Ok(c) => c,
        Err(e) => return json_err(e),
    };

    let mut manager = AccountManager::from_config(&cfg.accounts);
    let resolved_name = account_name
        .map(String::from)
        .or_else(|| manager.default_name().map(String::from));

    let Some(name) = resolved_name else {
        return json_err("no account specified and no default configured");
    };

    let Some(account) = manager.get_mut(&name) else {
        return json_err(format!("account '{name}' not found"));
    };

    // Take ownership by swapping with a dummy — the account will be dropped after use
    let cfg_clone = account.config.clone();
    let fresh = Account::new(name, cfg_clone);

    match f(fresh).await {
        Ok(val) => serde_json::to_string_pretty(&val).unwrap_or_default(),
        Err(e) => json_err(e),
    }
}

// ── MCP Server ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct HikyakuMcp {
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl HikyakuMcp {
    fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "List all configured email accounts and their provider type")]
    async fn list_accounts(&self) -> String {
        match config::load_config() {
            Ok(cfg) => {
                let accounts: Vec<serde_json::Value> = cfg
                    .accounts
                    .iter()
                    .map(|(name, acct)| {
                        serde_json::json!({
                            "name": name,
                            "address": acct.address,
                            "provider": acct.provider,
                            "default": acct.default,
                        })
                    })
                    .collect();
                serde_json::to_string_pretty(&accounts).unwrap_or_default()
            }
            Err(e) => json_err(e),
        }
    }

    #[tool(description = "List mailboxes/folders for an email account. Connects to IMAP.")]
    async fn list_mailboxes(&self, Parameters(input): Parameters<AccountInput>) -> String {
        with_account(input.account.as_deref(), |mut acct| async move {
            acct.connect().await?;
            let mailboxes = acct.list_mailboxes().await?;
            Ok(serde_json::to_value(&mailboxes)?)
        })
        .await
    }

    #[tool(
        description = "List email messages. Uses local index if available, otherwise connects to IMAP."
    )]
    async fn list_messages(&self, Parameters(input): Parameters<ListMessagesInput>) -> String {
        let mailbox = input.mailbox.as_deref().unwrap_or("INBOX");
        let limit = input.limit.unwrap_or(20);
        let unread_only = input.unread_only.unwrap_or(false);

        // Try local index first
        if let Ok(cfg) = config::load_config() {
            if let Ok(idx) = EmailIndex::open(&cfg.index) {
                let acct_name = input.account.as_deref().unwrap_or("default");
                if let Ok(results) = idx.list_messages(acct_name, mailbox, limit, unread_only) {
                    if !results.is_empty() {
                        return serde_json::to_string_pretty(&results).unwrap_or_default();
                    }
                }
            }
        }

        // Fall back to IMAP
        with_account(input.account.as_deref(), |mut acct| async move {
            acct.connect().await?;
            let msgs = acct.fetch_summaries(mailbox, limit).await?;
            Ok(serde_json::to_value(&msgs)?)
        })
        .await
    }

    #[tool(description = "Read a specific email message by UID. Connects to IMAP to fetch full body.")]
    async fn read_message(&self, Parameters(input): Parameters<ReadMessageInput>) -> String {
        let mailbox = input.mailbox.as_deref().unwrap_or("INBOX");
        let uid = input.uid;

        with_account(input.account.as_deref(), |mut acct| async move {
            acct.connect().await?;
            let msg = acct.fetch_message(mailbox, uid).await?;
            Ok(serde_json::to_value(&msg)?)
        })
        .await
    }

    #[tool(description = "Full-text search emails using the local Tantivy index. Sub-millisecond results.")]
    async fn search_messages(&self, Parameters(input): Parameters<SearchInput>) -> String {
        let limit = input.limit.unwrap_or(20) as usize;

        match config::load_config() {
            Ok(cfg) => match EmailIndex::open(&cfg.index) {
                Ok(idx) => match idx.search(&input.query, limit) {
                    Ok(results) => serde_json::to_string_pretty(&results).unwrap_or_default(),
                    Err(e) => json_err(e),
                },
                Err(e) => json_err(format!("index not available: {e}")),
            },
            Err(e) => json_err(e),
        }
    }

    #[tool(description = "Compose and send an email via SMTP")]
    async fn send_message(&self, Parameters(input): Parameters<SendMessageInput>) -> String {
        with_account(input.account.as_deref(), |mut acct| async move {
            let msg = OutgoingMessage {
                from: acct.config.address.clone(),
                to: input.to.split(',').map(|s| s.trim().to_string()).collect(),
                cc: input
                    .cc
                    .map(|s| s.split(',').map(|s| s.trim().to_string()).collect())
                    .unwrap_or_default(),
                bcc: input
                    .bcc
                    .map(|s| s.split(',').map(|s| s.trim().to_string()).collect())
                    .unwrap_or_default(),
                subject: input.subject,
                text_body: input.body,
            };
            acct.send_message(&msg).await?;
            Ok(serde_json::json!({"status": "sent"}))
        })
        .await
    }

    #[tool(description = "Move a message from one mailbox/folder to another")]
    async fn move_message(&self, Parameters(input): Parameters<MoveMessageInput>) -> String {
        with_account(input.account.as_deref(), |mut acct| async move {
            acct.connect().await?;
            acct.move_message(&input.from_mailbox, input.uid, &input.to_mailbox)
                .await?;
            Ok(serde_json::json!({"status": "moved"}))
        })
        .await
    }

    #[tool(description = "Delete or trash an email message")]
    async fn delete_message(&self, Parameters(input): Parameters<DeleteMessageInput>) -> String {
        let mailbox = input.mailbox.as_deref().unwrap_or("INBOX");
        let permanent = input.permanent.unwrap_or(false);

        with_account(input.account.as_deref(), |mut acct| async move {
            acct.connect().await?;
            acct.delete_message(mailbox, input.uid, permanent).await?;
            Ok(serde_json::json!({"status": "deleted", "permanent": permanent}))
        })
        .await
    }

    #[tool(description = "Execute a Rhai automation script against the email engine")]
    async fn run_script(&self, Parameters(input): Parameters<RunScriptInput>) -> String {
        let engine = crate::automation::ScriptEngine::new();
        match engine.eval(&input.script) {
            Ok(result) => format!(r#"{{"result":"{}"}}"#, result),
            Err(e) => json_err(e),
        }
    }
}

#[tool_handler]
impl ServerHandler for HikyakuMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Hikyaku — terminal email client with GPU rendering, multi-account IMAP/SMTP, \
                 Rhai scripting, and Nord theming. Manages Gmail, Gmail Workspace, and \
                 Protonmail (via bridge) accounts. Full-text search via Tantivy index."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

pub async fn run() -> anyhow::Result<()> {
    let server = HikyakuMcp::new()
        .serve(stdio())
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    server
        .waiting()
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(())
}
