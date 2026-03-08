use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use shikumi::{ConfigDiscovery, ConfigStore};

use crate::index::IndexConfig;

// ── Top-level config ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct HikyakuConfig {
    pub accounts: HashMap<String, AccountConfig>,
    pub theme: ThemeConfig,
    pub keybindings: HashMap<String, String>,
    pub scripting: ScriptingConfig,
    pub rendering: RenderingConfig,
    pub index: IndexConfig,
}

impl Default for HikyakuConfig {
    fn default() -> Self {
        Self {
            accounts: HashMap::new(),
            theme: ThemeConfig::default(),
            keybindings: default_keybindings(),
            scripting: ScriptingConfig::default(),
            rendering: RenderingConfig::default(),
            index: IndexConfig::default(),
        }
    }
}

fn default_keybindings() -> HashMap<String, String> {
    let mut kb = HashMap::new();
    kb.insert("quit".into(), "q".into());
    kb.insert("down".into(), "j".into());
    kb.insert("up".into(), "k".into());
    kb.insert("open".into(), "Enter".into());
    kb.insert("back".into(), "Escape".into());
    kb.insert("compose".into(), "c".into());
    kb.insert("reply".into(), "r".into());
    kb.insert("reply_all".into(), "R".into());
    kb.insert("forward".into(), "f".into());
    kb.insert("delete".into(), "d".into());
    kb.insert("archive".into(), "e".into());
    kb.insert("mark_read".into(), "m".into());
    kb.insert("search".into(), "/".into());
    kb.insert("refresh".into(), "g".into());
    kb.insert("next_account".into(), "Tab".into());
    kb.insert("prev_account".into(), "BackTab".into());
    kb
}

// ── Account config ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AccountConfig {
    pub provider: ProviderKind,
    pub address: String,
    #[serde(default = "default_imap_host")]
    pub imap_host: String,
    #[serde(default = "default_imap_port")]
    pub imap_port: u16,
    #[serde(default = "default_smtp_host")]
    pub smtp_host: String,
    #[serde(default = "default_smtp_port")]
    pub smtp_port: u16,
    #[serde(default)]
    pub oauth2: bool,
    pub password_command: Option<String>,
    #[serde(default)]
    pub default: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Gmail,
    GmailWorkspace,
    Protonmail,
    Imap,
}

impl ProviderKind {
    pub fn default_imap_host(&self) -> &str {
        match self {
            Self::Gmail | Self::GmailWorkspace => "imap.gmail.com",
            Self::Protonmail => "127.0.0.1",
            Self::Imap => "localhost",
        }
    }

    pub fn default_imap_port(&self) -> u16 {
        match self {
            Self::Gmail | Self::GmailWorkspace => 993,
            Self::Protonmail => 1143,
            Self::Imap => 993,
        }
    }

    pub fn default_smtp_host(&self) -> &str {
        match self {
            Self::Gmail | Self::GmailWorkspace => "smtp.gmail.com",
            Self::Protonmail => "127.0.0.1",
            Self::Imap => "localhost",
        }
    }

    pub fn default_smtp_port(&self) -> u16 {
        match self {
            Self::Gmail | Self::GmailWorkspace => 587,
            Self::Protonmail => 1025,
            Self::Imap => 587,
        }
    }
}

fn default_imap_host() -> String {
    "imap.gmail.com".into()
}
fn default_imap_port() -> u16 {
    993
}
fn default_smtp_host() -> String {
    "smtp.gmail.com".into()
}
fn default_smtp_port() -> u16 {
    587
}

// ── Theme config ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ThemeConfig {
    pub name: String,
    pub path: Option<PathBuf>,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            name: "nord".into(),
            path: None,
        }
    }
}

// ── Scripting config ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ScriptingConfig {
    pub init_script: Option<PathBuf>,
    pub script_dirs: Vec<PathBuf>,
    pub hot_reload: bool,
}

impl Default for ScriptingConfig {
    fn default() -> Self {
        Self {
            init_script: None,
            script_dirs: Vec::new(),
            hot_reload: true,
        }
    }
}

// ── Rendering config ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct RenderingConfig {
    pub graphics_protocol: GraphicsProtocol,
    pub html_renderer: HtmlRenderer,
    pub inline_images: bool,
    pub max_image_width: u32,
    pub max_image_height: u32,
}

impl Default for RenderingConfig {
    fn default() -> Self {
        Self {
            graphics_protocol: GraphicsProtocol::Auto,
            html_renderer: HtmlRenderer::Plaintext,
            inline_images: true,
            max_image_width: 800,
            max_image_height: 600,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphicsProtocol {
    #[default]
    Auto,
    Kitty,
    Sixel,
    Halfblocks,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HtmlRenderer {
    Chromium,
    #[default]
    Plaintext,
}

// ── Config loading (shikumi) ─────────────────────────────────────────────────

/// Load config once (no file watching).
pub fn load_config() -> anyhow::Result<HikyakuConfig> {
    let path = ConfigDiscovery::new("hikyaku")
        .env_override("HIKYAKU_CONFIG")
        .discover()?;

    let store = ConfigStore::<HikyakuConfig>::load(&path, "HIKYAKU_")?;
    Ok(store.get().as_ref().clone())
}

/// Load config with hot-reload support. Returns the store for shared access.
pub fn load_config_watched() -> anyhow::Result<ConfigStore<HikyakuConfig>> {
    let path = ConfigDiscovery::new("hikyaku")
        .env_override("HIKYAKU_CONFIG")
        .discover()?;

    let store = ConfigStore::<HikyakuConfig>::load_and_watch(&path, "HIKYAKU_", |_new_config| {
        tracing::info!("configuration reloaded");
    })?;

    Ok(store)
}
