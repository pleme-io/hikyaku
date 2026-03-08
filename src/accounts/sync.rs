use std::time::Duration;

use tokio::sync::mpsc;

use crate::config::AccountConfig;
use crate::index::EmailIndex;

/// Events sent from the sync task to the TUI/main thread.
#[derive(Debug)]
pub enum SyncEvent {
    /// New messages arrived in a mailbox.
    NewMessages {
        account: String,
        mailbox: String,
        count: u32,
    },
    /// Sync completed for a mailbox.
    SyncComplete {
        account: String,
        mailbox: String,
    },
    /// An error occurred during sync.
    Error {
        account: String,
        message: String,
    },
    /// Account connected successfully.
    Connected {
        account: String,
    },
}

/// Spawn a background sync task for an account.
///
/// The task:
/// 1. Connects to IMAP
/// 2. Syncs all mailboxes to the local index
/// 3. Enters IDLE mode on INBOX, waiting for new mail
/// 4. On new mail notification, re-syncs and re-enters IDLE
pub fn spawn_sync_task(
    name: String,
    config: AccountConfig,
    index: std::sync::Arc<EmailIndex>,
    tx: mpsc::UnboundedSender<SyncEvent>,
    poll_interval: Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            if let Err(e) = sync_loop(&name, &config, &index, &tx, poll_interval).await {
                tracing::error!(account = %name, error = %e, "sync loop failed");
                let _ = tx.send(SyncEvent::Error {
                    account: name.clone(),
                    message: e.to_string(),
                });
            }

            // Wait before reconnecting
            tokio::time::sleep(Duration::from_secs(30)).await;
            tracing::info!(account = %name, "reconnecting sync task");
        }
    })
}

async fn sync_loop(
    name: &str,
    config: &AccountConfig,
    index: &EmailIndex,
    tx: &mpsc::UnboundedSender<SyncEvent>,
    poll_interval: Duration,
) -> anyhow::Result<()> {
    let mut account = super::Account::new(name.to_string(), config.clone());
    account.connect().await?;

    let _ = tx.send(SyncEvent::Connected {
        account: name.to_string(),
    });

    // Initial sync of all mailboxes
    let mailboxes = account.list_mailboxes().await?;
    for mb in &mailboxes {
        sync_mailbox(&mut account, name, &mb.name, index, tx).await?;
    }

    // Poll-based sync loop (IDLE support TODO — requires consuming session ownership)
    loop {
        tokio::time::sleep(poll_interval).await;

        if account.session.is_none() {
            return Err(anyhow::anyhow!("lost connection"));
        }

        // Re-sync INBOX
        sync_mailbox(&mut account, name, "INBOX", index, tx).await?;
    }
}

async fn sync_mailbox(
    account: &mut super::Account,
    account_name: &str,
    mailbox: &str,
    index: &EmailIndex,
    tx: &mpsc::UnboundedSender<SyncEvent>,
) -> anyhow::Result<()> {
    let highest_uid = index.highest_uid(account_name, mailbox)?;

    let summaries = account.fetch_summaries(mailbox, 100).await?;

    let mut new_count = 0u32;
    for msg in &summaries {
        if highest_uid.is_some_and(|h| msg.uid <= h) {
            continue;
        }

        index.upsert_message(
            account_name,
            mailbox,
            msg.uid,
            &msg.subject,
            &msg.from,
            &msg.date,
            &msg.preview,
            msg.is_read,
            msg.is_flagged,
        )?;

        new_count += 1;
    }

    if new_count > 0 {
        let _ = tx.send(SyncEvent::NewMessages {
            account: account_name.to_string(),
            mailbox: mailbox.to_string(),
            count: new_count,
        });
    }

    let _ = tx.send(SyncEvent::SyncComplete {
        account: account_name.to_string(),
        mailbox: mailbox.to_string(),
    });

    Ok(())
}
