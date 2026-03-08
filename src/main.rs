mod accounts;
mod automation;
mod config;
mod error;
mod index;
mod mcp;
mod render;
mod tui;

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "hikyaku", version, about = "GPU-rendered terminal email client")]
struct Cli {
    #[command(subcommand)]
    command: SubCmd,
}

#[derive(Subcommand)]
enum SubCmd {
    /// Launch the TUI email client
    Launch,

    /// Start the MCP server (stdio transport)
    Mcp,

    /// Check connectivity for all configured accounts
    Check,

    /// Authenticate an account via OAuth2 device flow
    Auth {
        /// Account name from config
        account: String,
    },

    /// Run a Rhai script
    Script {
        /// Path to the .rhai script file
        path: String,
    },

    /// Search the local index
    Search {
        /// Search query
        query: String,
        /// Max results
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },

    /// Force sync all accounts to the local index
    Sync,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        SubCmd::Launch => {
            init_tracing_stderr();
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(tui::run())?;
        }
        SubCmd::Mcp => {
            init_tracing_stderr();
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(mcp::run())?;
        }
        SubCmd::Check => {
            init_tracing();
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(check_accounts())?;
        }
        SubCmd::Auth { account } => {
            init_tracing();
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(auth_account(&account))?;
        }
        SubCmd::Script { path } => {
            init_tracing();
            run_script(&path)?;
        }
        SubCmd::Search { query, limit } => {
            init_tracing();
            search_index(&query, limit)?;
        }
        SubCmd::Sync => {
            init_tracing();
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(sync_accounts())?;
        }
    }

    Ok(())
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
}

fn init_tracing_stderr() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
}

async fn check_accounts() -> anyhow::Result<()> {
    let cfg = config::load_config()?;

    if cfg.accounts.is_empty() {
        println!("No accounts configured.");
        println!("Add accounts to ~/.config/hikyaku/hikyaku.yaml");
        return Ok(());
    }

    println!("Checking {} account(s)...\n", cfg.accounts.len());

    let mut manager = accounts::AccountManager::from_config(&cfg.accounts);
    let results = manager.connect_all().await;

    for (name, result) in results {
        match result {
            Ok(()) => println!("  \u{2713} {name}: connected"),
            Err(e) => println!("  \u{2717} {name}: {e}"),
        }
    }

    Ok(())
}

async fn auth_account(account_name: &str) -> anyhow::Result<()> {
    let cfg = config::load_config()?;

    let account_cfg = cfg
        .accounts
        .get(account_name)
        .ok_or_else(|| anyhow::anyhow!("account '{account_name}' not found in config"))?;

    if !account_cfg.oauth2 {
        anyhow::bail!(
            "account '{account_name}' does not use OAuth2. \
             Set `oauth2: true` in the account config."
        );
    }

    let oauth2_config = accounts::oauth2::OAuth2Config::default();
    accounts::oauth2::device_auth_flow(account_name, &oauth2_config).await?;

    // Test the connection
    println!("\nTesting connection...");
    let mut account = accounts::Account::new(account_name.to_string(), account_cfg.clone());
    account.connect().await?;
    println!("  \u{2713} Successfully connected to {}", account_cfg.address);

    Ok(())
}

fn run_script(path: &str) -> anyhow::Result<()> {
    let mut engine = automation::ScriptEngine::new();
    engine.load_script(std::path::Path::new(path))?;
    engine.run_init()?;
    Ok(())
}

fn search_index(query: &str, limit: usize) -> anyhow::Result<()> {
    let cfg = config::load_config()?;
    let idx = index::EmailIndex::open(&cfg.index)?;
    let results = idx.search(query, limit)?;

    if results.is_empty() {
        println!("No results found.");
        return Ok(());
    }

    println!("Found {} result(s):\n", results.len());
    for r in &results {
        let read_indicator = if r.score > 0.0 {
            format!(" ({:.2})", r.score)
        } else {
            String::new()
        };
        println!(
            "  [{}/{}] UID {}{}",
            r.account, r.mailbox, r.uid, read_indicator
        );
        println!("    From: {}", r.from);
        println!("    Subject: {}", r.subject);
        println!("    Date: {}", r.date);
        println!();
    }

    Ok(())
}

async fn sync_accounts() -> anyhow::Result<()> {
    let cfg = config::load_config()?;
    let idx = index::EmailIndex::open(&cfg.index)?;

    if cfg.accounts.is_empty() {
        println!("No accounts configured.");
        return Ok(());
    }

    println!("Syncing {} account(s)...\n", cfg.accounts.len());

    for (name, account_cfg) in &cfg.accounts {
        print!("  {name}...");
        let mut account = accounts::Account::new(name.clone(), account_cfg.clone());

        match account.connect().await {
            Ok(()) => {
                let mailboxes = account.list_mailboxes().await?;
                let mut total = 0u32;

                for mb in &mailboxes {
                    let summaries = account.fetch_summaries(&mb.name, cfg.index.batch_size).await?;
                    for msg in &summaries {
                        idx.upsert_message(
                            name,
                            &mb.name,
                            msg.uid,
                            &msg.subject,
                            &msg.from,
                            &msg.date,
                            &msg.preview,
                            msg.is_read,
                            msg.is_flagged,
                        )?;
                        total += 1;
                    }
                }

                idx.commit()?;
                println!(" {total} messages indexed across {} mailboxes", mailboxes.len());
            }
            Err(e) => println!(" error: {e}"),
        }
    }

    println!("\nSync complete.");
    Ok(())
}
