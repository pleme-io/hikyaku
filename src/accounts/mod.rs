pub mod oauth2;
pub mod sync;

use std::collections::HashMap;

use futures::StreamExt;
use futures::TryStreamExt;
use mail_parser::MimeHeaders;

use crate::config::{AccountConfig, ProviderKind};

// ── Types ────────────────────────────────────────────────────────────────────

pub(crate) type ImapStream = tokio_native_tls::TlsStream<tokio::net::TcpStream>;
pub(crate) type ImapSession = async_imap::Session<ImapStream>;

#[derive(Debug, Clone, serde::Serialize)]
pub struct Mailbox {
    pub name: String,
    pub delimiter: Option<String>,
    pub message_count: u32,
    pub unseen_count: u32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MessageSummary {
    pub uid: u32,
    pub subject: String,
    pub from: String,
    pub date: String,
    pub is_read: bool,
    pub is_flagged: bool,
    pub has_attachment: bool,
    pub preview: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Message {
    pub uid: u32,
    pub subject: String,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub date: String,
    pub text_body: Option<String>,
    pub html_body: Option<String>,
    pub attachments: Vec<AttachmentMeta>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AttachmentMeta {
    pub filename: String,
    pub content_type: String,
    pub size: usize,
}

#[derive(Debug, Clone)]
pub struct Attachment {
    pub filename: String,
    pub content_type: String,
    pub size: usize,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct OutgoingMessage {
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub text_body: String,
}

// ── Account ──────────────────────────────────────────────────────────────────

pub struct Account {
    pub name: String,
    pub config: AccountConfig,
    pub(crate) session: Option<ImapSession>,
    selected_mailbox: Option<String>,
}

impl Account {
    pub fn new(name: String, config: AccountConfig) -> Self {
        Self {
            name,
            config,
            session: None,
            selected_mailbox: None,
        }
    }

    pub fn is_connected(&self) -> bool {
        self.session.is_some()
    }

    pub async fn connect(&mut self) -> anyhow::Result<()> {
        let host = &self.config.imap_host;
        let port = self.config.imap_port;

        tracing::info!(account = %self.name, host, port, "connecting to IMAP");

        let password = self.resolve_credential().await?;

        let mut tls_builder = native_tls::TlsConnector::builder();
        if self.config.provider == ProviderKind::Protonmail {
            tls_builder.danger_accept_invalid_certs(true);
        }
        let tls = tokio_native_tls::TlsConnector::from(tls_builder.build()?);

        let tcp = tokio::net::TcpStream::connect((host.as_str(), port)).await?;
        let tls_stream = tls.connect(host, tcp).await?;
        let client = async_imap::Client::new(tls_stream);

        let session = if self.config.oauth2 {
            let auth_string = format!(
                "user={}\x01auth=Bearer {}\x01\x01",
                self.config.address, password
            );
            client
                .authenticate("XOAUTH2", ImapOAuth2(auth_string))
                .await
        } else {
            client.login(&self.config.address, &password).await
        }
        .map_err(|e| anyhow::anyhow!("IMAP login failed: {}", e.0))?;

        self.session = Some(session);
        self.selected_mailbox = None;
        tracing::info!(account = %self.name, "connected");
        Ok(())
    }

    pub async fn disconnect(&mut self) {
        if let Some(mut session) = self.session.take() {
            let _ = session.logout().await;
        }
        self.selected_mailbox = None;
    }

    async fn ensure_session(&mut self) -> anyhow::Result<&mut ImapSession> {
        if self.session.is_none() {
            self.connect().await?;
        }
        self.session
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("not connected"))
    }

    async fn select_mailbox(&mut self, mailbox: &str) -> anyhow::Result<async_imap::types::Mailbox> {
        if self.selected_mailbox.as_deref() == Some(mailbox) {
            // Already selected — re-select to refresh counts
        }
        let session = self.ensure_session().await?;
        let mb = session.select(mailbox).await?;
        self.selected_mailbox = Some(mailbox.to_string());
        Ok(mb)
    }

    // ── Real IMAP operations ─────────────────────────────────────────────

    pub async fn list_mailboxes(&mut self) -> anyhow::Result<Vec<Mailbox>> {
        let session = self.ensure_session().await?;
        let names: Vec<_> = session.list(Some(""), Some("*")).await?.try_collect().await?;

        let mut mailboxes = Vec::new();
        for name in &names {
            let is_noselect = name
                .attributes()
                .iter()
                .any(|a| matches!(a, async_imap::types::NameAttribute::NoSelect));
            if is_noselect {
                continue;
            }

            mailboxes.push(Mailbox {
                name: name.name().to_string(),
                delimiter: name.delimiter().map(String::from),
                message_count: 0,
                unseen_count: 0,
            });
        }

        // Get counts for each mailbox via STATUS
        for mb in &mut mailboxes {
            if let Ok(status) = session
                .status(&mb.name, "(MESSAGES UNSEEN)")
                .await
            {
                mb.message_count = status.exists;
                mb.unseen_count = status.unseen.unwrap_or(0);
            }
        }

        Ok(mailboxes)
    }

    pub async fn fetch_summaries(
        &mut self,
        mailbox: &str,
        limit: u32,
    ) -> anyhow::Result<Vec<MessageSummary>> {
        let mb_info = self.select_mailbox(mailbox).await?;
        let total = mb_info.exists;

        if total == 0 {
            return Ok(Vec::new());
        }

        let start = if total > limit { total - limit + 1 } else { 1 };
        let range = format!("{start}:{total}");

        let session = self.session.as_mut().unwrap();
        let mut stream = session
            .fetch(&range, "(UID FLAGS ENVELOPE BODY.PEEK[TEXT]<0.200>)")
            .await?;

        let mut summaries = Vec::new();
        while let Some(result) = stream.next().await {
            let fetch = result?;
            let uid = fetch.uid.unwrap_or(0);

            let flags: Vec<_> = fetch.flags().collect();
            let is_read = flags.iter().any(|f| matches!(f, async_imap::types::Flag::Seen));
            let is_flagged = flags
                .iter()
                .any(|f| matches!(f, async_imap::types::Flag::Flagged));

            let (subject, from, date) = if let Some(env) = fetch.envelope() {
                let subject = env
                    .subject
                    .as_ref()
                    .map(|s| String::from_utf8_lossy(s).to_string())
                    .unwrap_or_default();
                let from = env
                    .from
                    .as_ref()
                    .and_then(|addrs| addrs.first())
                    .map(format_address)
                    .unwrap_or_default();
                let date = env
                    .date
                    .as_ref()
                    .map(|d| String::from_utf8_lossy(d).to_string())
                    .unwrap_or_default();
                (subject, from, date)
            } else {
                (String::new(), String::new(), String::new())
            };

            let preview = fetch
                .text()
                .map(|t| {
                    let s = String::from_utf8_lossy(t);
                    s.chars().take(200).collect::<String>()
                })
                .unwrap_or_default();

            summaries.push(MessageSummary {
                uid,
                subject,
                from,
                date,
                is_read,
                is_flagged,
                has_attachment: false,
                preview,
            });
        }

        summaries.reverse(); // newest first
        Ok(summaries)
    }

    pub async fn fetch_message(
        &mut self,
        mailbox: &str,
        uid: u32,
    ) -> anyhow::Result<Message> {
        self.select_mailbox(mailbox).await?;

        let session = self.session.as_mut().unwrap();
        let mut stream = session
            .uid_fetch(uid.to_string(), "BODY[]")
            .await?;

        let fetch = stream
            .next()
            .await
            .ok_or_else(|| anyhow::anyhow!("message UID {uid} not found"))??;

        let raw = fetch
            .body()
            .ok_or_else(|| anyhow::anyhow!("empty body for UID {uid}"))?;

        let parsed = mail_parser::MessageParser::default()
            .parse(raw)
            .ok_or_else(|| anyhow::anyhow!("failed to parse message UID {uid}"))?;

        let subject = parsed.subject().unwrap_or("").to_string();
        let from = parsed
            .from()
            .and_then(|a| a.first())
            .map(|a| {
                a.name()
                    .map(|n| format!("{n} <{}>", a.address().unwrap_or("")))
                    .unwrap_or_else(|| a.address().unwrap_or("").to_string())
            })
            .unwrap_or_default();

        let to: Vec<String> = parsed
            .to()
            .map(|list| {
                list.iter()
                    .map(|a| a.address().unwrap_or("").to_string())
                    .collect()
            })
            .unwrap_or_default();

        let cc: Vec<String> = parsed
            .cc()
            .map(|list| {
                list.iter()
                    .map(|a| a.address().unwrap_or("").to_string())
                    .collect()
            })
            .unwrap_or_default();

        let date = parsed
            .date()
            .map(|d| d.to_rfc3339())
            .unwrap_or_default();

        let text_body = parsed.body_text(0).map(|s| s.to_string());
        let html_body = parsed.body_html(0).map(|s| s.to_string());

        let attachments: Vec<AttachmentMeta> = parsed
            .attachments()
            .map(|a| AttachmentMeta {
                filename: a
                    .attachment_name()
                    .unwrap_or("unnamed")
                    .to_string(),
                content_type: a.content_type().map(|ct| {
                    format!("{}/{}", ct.ctype(), ct.subtype().unwrap_or(""))
                }).unwrap_or_default(),
                size: a.len(),
            })
            .collect();

        Ok(Message {
            uid,
            subject,
            from,
            to,
            cc,
            date,
            text_body,
            html_body,
            attachments,
        })
    }

    pub async fn send_message(&mut self, msg: &OutgoingMessage) -> anyhow::Result<()> {
        let password = self.resolve_credential().await?;

        let mut builder = lettre::Message::builder()
            .from(msg.from.parse()?)
            .subject(&msg.subject);

        for to in &msg.to {
            builder = builder.to(to.parse()?);
        }
        for cc in &msg.cc {
            builder = builder.cc(cc.parse()?);
        }
        for bcc in &msg.bcc {
            builder = builder.bcc(bcc.parse()?);
        }

        let email = builder.body(msg.text_body.clone())?;

        let creds = if self.config.oauth2 {
            // For OAuth2 SMTP, use XOAUTH2 mechanism
            // lettre doesn't natively support XOAUTH2, so we use password auth
            // with the access token as the password (works with Gmail)
            lettre::transport::smtp::authentication::Credentials::new(
                self.config.address.clone(),
                password,
            )
        } else {
            lettre::transport::smtp::authentication::Credentials::new(
                self.config.address.clone(),
                password,
            )
        };

        let mailer = lettre::AsyncSmtpTransport::<lettre::Tokio1Executor>::starttls_relay(
            &self.config.smtp_host,
        )?
        .port(self.config.smtp_port)
        .credentials(creds)
        .build();

        use lettre::AsyncTransport;
        mailer.send(email).await?;
        tracing::info!(account = %self.name, "message sent");

        Ok(())
    }

    pub async fn move_message(
        &mut self,
        mailbox: &str,
        uid: u32,
        dest: &str,
    ) -> anyhow::Result<()> {
        self.select_mailbox(mailbox).await?;
        let session = self.session.as_mut().unwrap();

        // COPY to destination, then mark deleted in source
        session.uid_copy(uid.to_string(), dest).await?;
        let _: Vec<_> = session
            .uid_store(uid.to_string(), "+FLAGS (\\Deleted)")
            .await?
            .try_collect()
            .await?;
        let _: Vec<_> = session.expunge().await?.try_collect().await?;
        tracing::info!(uid, from = mailbox, to = dest, "message moved");

        Ok(())
    }

    pub async fn delete_message(
        &mut self,
        mailbox: &str,
        uid: u32,
        permanent: bool,
    ) -> anyhow::Result<()> {
        self.select_mailbox(mailbox).await?;
        let session = self.session.as_mut().unwrap();

        if permanent {
            let _: Vec<_> = session
                .uid_store(uid.to_string(), "+FLAGS (\\Deleted)")
                .await?
                .try_collect()
                .await?;
            let _: Vec<_> = session.expunge().await?.try_collect().await?;
        } else {
            // Move to Trash
            session.uid_copy(uid.to_string(), "Trash").await?;
            let _: Vec<_> = session
                .uid_store(uid.to_string(), "+FLAGS (\\Deleted)")
                .await?
                .try_collect()
                .await?;
            let _: Vec<_> = session.expunge().await?.try_collect().await?;
        }

        tracing::info!(uid, mailbox, permanent, "message deleted");
        Ok(())
    }

    pub async fn search(&mut self, mailbox: &str, query: &str) -> anyhow::Result<Vec<u32>> {
        self.select_mailbox(mailbox).await?;
        let session = self.session.as_mut().unwrap();

        let search_query = format!("OR SUBJECT \"{}\" FROM \"{}\"", query, query);
        let result = session.search(&search_query).await?;

        let uids: Vec<u32> = result.iter().copied().collect();
        Ok(uids)
    }

    // ── Credential resolution ────────────────────────────────────────────

    async fn resolve_credential(&self) -> anyhow::Result<String> {
        // First try OAuth2 token refresh
        if self.config.oauth2 {
            if let Ok(token) = oauth2::load_or_refresh_token(&self.name).await {
                return Ok(token);
            }
        }

        // Fall back to password_command
        if let Some(cmd) = &self.config.password_command {
            let output = tokio::process::Command::new("sh")
                .arg("-c")
                .arg(cmd)
                .output()
                .await?;

            if !output.status.success() {
                anyhow::bail!(
                    "password command failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }

            return Ok(String::from_utf8(output.stdout)?.trim().to_string());
        }

        anyhow::bail!(
            "no credential source for account '{}' — set password_command or run `hikyaku auth {}`",
            self.name,
            self.name
        );
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

struct ImapOAuth2(String);

impl async_imap::Authenticator for ImapOAuth2 {
    type Response = String;
    fn process(&mut self, _data: &[u8]) -> Self::Response {
        self.0.clone()
    }
}

fn format_address(addr: &imap_proto::Address) -> String {
    let name = addr
        .name
        .as_ref()
        .map(|n| String::from_utf8_lossy(n).to_string());
    let mailbox = addr
        .mailbox
        .as_ref()
        .map(|m| String::from_utf8_lossy(m).to_string())
        .unwrap_or_default();
    let host = addr
        .host
        .as_ref()
        .map(|h| String::from_utf8_lossy(h).to_string())
        .unwrap_or_default();
    let email = format!("{mailbox}@{host}");

    match name {
        Some(n) if !n.is_empty() => format!("{n} <{email}>"),
        _ => email,
    }
}

// ── AccountManager ───────────────────────────────────────────────────────────

pub struct AccountManager {
    accounts: HashMap<String, Account>,
    default_account: Option<String>,
}

impl AccountManager {
    pub fn from_config(accounts: &HashMap<String, AccountConfig>) -> Self {
        let mut manager = Self {
            accounts: HashMap::new(),
            default_account: None,
        };

        for (name, config) in accounts {
            if config.default {
                manager.default_account = Some(name.clone());
            }
            manager
                .accounts
                .insert(name.clone(), Account::new(name.clone(), config.clone()));
        }

        if manager.default_account.is_none() {
            manager.default_account = manager.accounts.keys().next().cloned();
        }

        manager
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut Account> {
        self.accounts.get_mut(name)
    }

    pub fn default_name(&self) -> Option<&str> {
        self.default_account.as_deref()
    }

    pub fn resolve_mut(&mut self, name: Option<&str>) -> Option<&mut Account> {
        let key = name
            .map(String::from)
            .or_else(|| self.default_account.clone())?;
        self.accounts.get_mut(&key)
    }

    pub fn account_names(&self) -> Vec<String> {
        self.accounts.keys().cloned().collect()
    }

    pub async fn connect_all(&mut self) -> Vec<(String, anyhow::Result<()>)> {
        let mut results = Vec::new();
        let names: Vec<String> = self.accounts.keys().cloned().collect();
        for name in names {
            if let Some(account) = self.accounts.get_mut(&name) {
                let result = account.connect().await;
                results.push((name, result));
            }
        }
        results
    }
}
