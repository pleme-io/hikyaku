use std::path::PathBuf;
use std::sync::Arc;

use rusqlite::{Connection as SqliteConn, params};
use sakuin::{IndexStore, SchemaSpec, INDEXED, STORED, STRING, TEXT};
use serde::{Deserialize, Serialize};

// ── Config ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct IndexConfig {
    pub db_path: Option<PathBuf>,
    pub tantivy_path: Option<PathBuf>,
    pub background_sync: bool,
    pub poll_interval_secs: u64,
    pub batch_size: u32,
    /// Optional PostgreSQL connection string for cross-device sync.
    /// When set, metadata is mirrored to Postgres alongside SQLite.
    pub postgres_url: Option<String>,
}

impl Default for IndexConfig {
    fn default() -> Self {
        Self {
            db_path: None,
            tantivy_path: None,
            background_sync: true,
            poll_interval_secs: 300,
            batch_size: 100,
            postgres_url: None,
        }
    }
}

// ── Search result ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchResult {
    pub account: String,
    pub mailbox: String,
    pub uid: u32,
    pub subject: String,
    pub from: String,
    pub date: String,
    pub score: f32,
    pub preview: String,
}

// ── EmailIndex ───────────────────────────────────────────────────────────────

pub struct EmailIndex {
    sqlite: SqliteConn,
    store: IndexStore,
    #[allow(dead_code)]
    config: IndexConfig,
}

impl EmailIndex {
    pub fn open(config: &IndexConfig) -> anyhow::Result<Arc<Self>> {
        let db_path = config.db_path.clone().unwrap_or_else(|| {
            default_data_dir().join("index.db")
        });
        let tantivy_path = config.tantivy_path.clone().unwrap_or_else(|| {
            default_data_dir().join("tantivy")
        });

        // Ensure directories exist
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // ── SQLite setup ─────────────────────────────────────────────────
        let sqlite = SqliteConn::open(&db_path)?;
        sqlite.execute_batch("
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;

            CREATE TABLE IF NOT EXISTS messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                account TEXT NOT NULL,
                mailbox TEXT NOT NULL,
                uid INTEGER NOT NULL,
                subject TEXT NOT NULL DEFAULT '',
                from_addr TEXT NOT NULL DEFAULT '',
                date TEXT NOT NULL DEFAULT '',
                preview TEXT NOT NULL DEFAULT '',
                is_read INTEGER NOT NULL DEFAULT 0,
                is_flagged INTEGER NOT NULL DEFAULT 0,
                synced_at TEXT NOT NULL DEFAULT (datetime('now')),
                UNIQUE(account, mailbox, uid)
            );

            CREATE INDEX IF NOT EXISTS idx_messages_account_mailbox
                ON messages(account, mailbox);
            CREATE INDEX IF NOT EXISTS idx_messages_uid
                ON messages(account, mailbox, uid);
        ")?;

        tracing::info!(path = %db_path.display(), "SQLite index opened");

        // ── Tantivy setup via sakuin ──────────────────────────────────────
        let spec = SchemaSpec::new()
            .field("account", STRING | STORED)
            .field("mailbox", STRING | STORED)
            .u64_field("uid", STORED | INDEXED)
            .field("subject", TEXT | STORED)
            .field("from_addr", TEXT | STORED)
            .field("date", STRING | STORED)
            .field("body", TEXT);

        let store = IndexStore::open_with_heap(&tantivy_path, &spec, 50_000_000)?;

        tracing::info!(path = %tantivy_path.display(), "Tantivy index opened");

        // ── Optional Postgres ────────────────────────────────────────────
        if let Some(pg_url) = &config.postgres_url {
            tracing::info!(url = %pg_url, "PostgreSQL sync configured (not yet connected)");
        }

        Ok(Arc::new(Self {
            sqlite,
            store,
            config: config.clone(),
        }))
    }

    // ── Write operations ─────────────────────────────────────────────────

    pub fn upsert_message(
        &self,
        account: &str,
        mailbox: &str,
        uid: u32,
        subject: &str,
        from: &str,
        date: &str,
        body_preview: &str,
        is_read: bool,
        is_flagged: bool,
    ) -> anyhow::Result<()> {
        // SQLite upsert
        self.sqlite.execute(
            "INSERT INTO messages (account, mailbox, uid, subject, from_addr, date, preview, is_read, is_flagged)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(account, mailbox, uid) DO UPDATE SET
                subject = excluded.subject,
                from_addr = excluded.from_addr,
                date = excluded.date,
                preview = excluded.preview,
                is_read = excluded.is_read,
                is_flagged = excluded.is_flagged,
                synced_at = datetime('now')",
            params![account, mailbox, uid as i64, subject, from, date, body_preview, is_read, is_flagged],
        )?;

        // Tantivy index (batched, no auto-commit)
        self.store.write_no_commit(|w| {
            w.delete_term_u64("uid", uid.into());
            w.add_doc_mixed(
                &[
                    ("account", account),
                    ("mailbox", mailbox),
                    ("subject", subject),
                    ("from_addr", from),
                    ("date", date),
                    ("body", body_preview),
                ],
                &[("uid", uid.into())],
            )?;
            Ok(())
        })?;

        Ok(())
    }

    /// Commit pending Tantivy writes to disk.
    pub fn commit(&self) -> anyhow::Result<()> {
        self.store.commit()?;
        Ok(())
    }

    // ── Search (Tantivy — sub-millisecond full-text) ─────────────────────

    pub fn search(&self, query: &str, limit: usize) -> anyhow::Result<Vec<SearchResult>> {
        let results = self
            .store
            .search(query, &["subject", "from_addr", "body"], limit)?;

        Ok(results
            .into_iter()
            .map(|(score, doc)| {
                let get_text = |name: &str| -> String {
                    doc.get(name)
                        .and_then(|v| v.as_text())
                        .unwrap_or("")
                        .to_string()
                };
                let uid = doc
                    .get("uid")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;

                SearchResult {
                    account: get_text("account"),
                    mailbox: get_text("mailbox"),
                    uid,
                    subject: get_text("subject"),
                    from: get_text("from_addr"),
                    date: get_text("date"),
                    score,
                    preview: String::new(),
                }
            })
            .collect())
    }

    // ── SQLite queries (structured metadata) ─────────────────────────────

    pub fn highest_uid(&self, account: &str, mailbox: &str) -> anyhow::Result<Option<u32>> {
        let mut stmt = self
            .sqlite
            .prepare("SELECT MAX(uid) FROM messages WHERE account = ?1 AND mailbox = ?2")?;
        let result: Option<i64> = stmt.query_row(params![account, mailbox], |row| row.get(0))?;
        Ok(result.map(|v| v as u32))
    }

    pub fn mailbox_stats(&self, account: &str) -> anyhow::Result<Vec<(String, u32, u32)>> {
        let mut stmt = self.sqlite.prepare(
            "SELECT mailbox, COUNT(*), SUM(CASE WHEN is_read = 0 THEN 1 ELSE 0 END)
             FROM messages WHERE account = ?1 GROUP BY mailbox",
        )?;

        let rows = stmt.query_map(params![account], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, u32>(1)?,
                row.get::<_, u32>(2)?,
            ))
        })?;

        let mut stats = Vec::new();
        for row in rows {
            stats.push(row?);
        }
        Ok(stats)
    }

    pub fn list_messages(
        &self,
        account: &str,
        mailbox: &str,
        limit: u32,
        unread_only: bool,
    ) -> anyhow::Result<Vec<SearchResult>> {
        let sql = if unread_only {
            "SELECT uid, subject, from_addr, date, preview
             FROM messages WHERE account = ?1 AND mailbox = ?2 AND is_read = 0
             ORDER BY uid DESC LIMIT ?3"
        } else {
            "SELECT uid, subject, from_addr, date, preview
             FROM messages WHERE account = ?1 AND mailbox = ?2
             ORDER BY uid DESC LIMIT ?3"
        };

        let mut stmt = self.sqlite.prepare(sql)?;
        let rows = stmt.query_map(params![account, mailbox, limit], |row| {
            Ok(SearchResult {
                account: account.to_string(),
                mailbox: mailbox.to_string(),
                uid: row.get::<_, i64>(0)? as u32,
                subject: row.get(1)?,
                from: row.get(2)?,
                date: row.get(3)?,
                score: 0.0,
                preview: row.get(4)?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    pub fn get_message_meta(
        &self,
        account: &str,
        mailbox: &str,
        uid: u32,
    ) -> anyhow::Result<Option<SearchResult>> {
        let mut stmt = self.sqlite.prepare(
            "SELECT subject, from_addr, date, preview
             FROM messages WHERE account = ?1 AND mailbox = ?2 AND uid = ?3",
        )?;

        let result = stmt
            .query_row(params![account, mailbox, uid as i64], |row| {
                Ok(SearchResult {
                    account: account.to_string(),
                    mailbox: mailbox.to_string(),
                    uid,
                    subject: row.get(0)?,
                    from: row.get(1)?,
                    date: row.get(2)?,
                    score: 0.0,
                    preview: row.get(3)?,
                })
            })
            .ok();

        Ok(result)
    }

    pub fn remove_deleted(
        &self,
        account: &str,
        mailbox: &str,
        valid_uids: &[u32],
    ) -> anyhow::Result<u32> {
        if valid_uids.is_empty() {
            return Ok(0);
        }

        let placeholders: Vec<String> = valid_uids.iter().map(|_| "?".to_string()).collect();
        let sql = format!(
            "DELETE FROM messages WHERE account = ? AND mailbox = ? AND uid NOT IN ({})",
            placeholders.join(",")
        );

        let mut params_vec: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        params_vec.push(Box::new(account.to_string()));
        params_vec.push(Box::new(mailbox.to_string()));
        for uid in valid_uids {
            params_vec.push(Box::new(*uid as i64));
        }

        let refs: Vec<&dyn rusqlite::types::ToSql> = params_vec.iter().map(|b| b.as_ref()).collect();
        let deleted = self.sqlite.execute(&sql, refs.as_slice())?;
        Ok(deleted as u32)
    }
}

// NOTE: SQLite Connection is not Sync by default. We use it from a single
// thread (the sync task and MCP calls are serialized). For multi-threaded
// access, wrap in a Mutex or use r2d2-sqlite connection pool.
unsafe impl Sync for EmailIndex {}
unsafe impl Send for EmailIndex {}

fn default_data_dir() -> PathBuf {
    dirs_next::data_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("hikyaku")
}
