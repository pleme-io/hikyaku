use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Google OAuth2 Device Authorization Grant flow.
///
/// 1. User runs `hikyaku auth <account>`
/// 2. We request a device code from Google
/// 3. User visits URL and enters code
/// 4. We poll for the access token
/// 5. Token stored at `~/.config/hikyaku/tokens/<account>.json`
/// 6. Auto-refreshed on expiry

const GOOGLE_DEVICE_AUTH_URL: &str = "https://oauth2.googleapis.com/device/code";
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const GMAIL_SCOPE: &str = "https://mail.google.com/";

// Default client ID for open-source email clients.
// Users can override via config.
const DEFAULT_CLIENT_ID: &str = "YOUR_CLIENT_ID";

#[derive(Debug, Serialize, Deserialize)]
pub struct OAuth2Config {
    #[serde(default = "default_client_id")]
    pub client_id: String,
    #[serde(default)]
    pub client_secret: String,
}

fn default_client_id() -> String {
    DEFAULT_CLIENT_ID.to_string()
}

impl Default for OAuth2Config {
    fn default() -> Self {
        Self {
            client_id: default_client_id(),
            client_secret: String::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_url: String,
    expires_in: u64,
    interval: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: u64,
    token_type: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct TokenError {
    error: String,
    error_description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredToken {
    access_token: String,
    refresh_token: String,
    expires_at: i64,
    client_id: String,
    client_secret: String,
}

fn token_path(account: &str) -> PathBuf {
    let config_dir = dirs_next::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"));
    config_dir
        .join("hikyaku")
        .join("tokens")
        .join(format!("{account}.json"))
}

/// Run the interactive device authorization flow.
pub async fn device_auth_flow(
    account: &str,
    oauth2_config: &OAuth2Config,
) -> anyhow::Result<String> {
    let client = reqwest::Client::new();

    // Step 1: Request device code
    let resp = client
        .post(GOOGLE_DEVICE_AUTH_URL)
        .form(&[
            ("client_id", oauth2_config.client_id.as_str()),
            ("scope", GMAIL_SCOPE),
        ])
        .send()
        .await?;

    let device: DeviceCodeResponse = resp.json().await?;

    // Step 2: Show user the verification URL and code
    println!("\n  To authorize hikyaku for this account:");
    println!("  1. Visit: {}", device.verification_url);
    println!("  2. Enter code: {}\n", device.user_code);

    // Try to open the URL in the default browser
    if open::that(&device.verification_url).is_ok() {
        println!("  (Browser opened automatically)");
    }

    println!("  Waiting for authorization...");

    // Step 3: Poll for token
    let interval = std::time::Duration::from_secs(device.interval.max(5));
    let deadline = std::time::Instant::now()
        + std::time::Duration::from_secs(device.expires_in);

    loop {
        tokio::time::sleep(interval).await;

        if std::time::Instant::now() > deadline {
            anyhow::bail!("authorization timed out");
        }

        let resp = client
            .post(GOOGLE_TOKEN_URL)
            .form(&[
                ("client_id", oauth2_config.client_id.as_str()),
                ("client_secret", oauth2_config.client_secret.as_str()),
                ("device_code", device.device_code.as_str()),
                (
                    "grant_type",
                    "urn:ietf:params:oauth:grant-type:device_code",
                ),
            ])
            .send()
            .await?;

        let status = resp.status();
        let body = resp.text().await?;

        if status.is_success() {
            let token: TokenResponse = serde_json::from_str(&body)?;

            // Store the token
            let stored = StoredToken {
                access_token: token.access_token.clone(),
                refresh_token: token
                    .refresh_token
                    .unwrap_or_default(),
                expires_at: chrono::Utc::now().timestamp() + token.expires_in as i64,
                client_id: oauth2_config.client_id.clone(),
                client_secret: oauth2_config.client_secret.clone(),
            };

            let path = token_path(account);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, serde_json::to_string_pretty(&stored)?)?;

            println!("  Authorization successful! Token saved.");
            return Ok(token.access_token);
        }

        // Check if we should keep polling
        if let Ok(err) = serde_json::from_str::<TokenError>(&body) {
            match err.error.as_str() {
                "authorization_pending" => continue,
                "slow_down" => {
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    continue;
                }
                _ => {
                    anyhow::bail!(
                        "OAuth2 error: {} — {}",
                        err.error,
                        err.error_description.unwrap_or_default()
                    );
                }
            }
        }

        anyhow::bail!("unexpected response: {body}");
    }
}

/// Load a stored token, refreshing if expired.
pub async fn load_or_refresh_token(account: &str) -> anyhow::Result<String> {
    let path = token_path(account);
    let data = std::fs::read_to_string(&path)?;
    let mut stored: StoredToken = serde_json::from_str(&data)?;

    // Check if token is still valid (with 60s buffer)
    let now = chrono::Utc::now().timestamp();
    if now < stored.expires_at - 60 {
        return Ok(stored.access_token);
    }

    // Refresh the token
    tracing::info!(account, "refreshing OAuth2 token");

    let client = reqwest::Client::new();
    let resp = client
        .post(GOOGLE_TOKEN_URL)
        .form(&[
            ("client_id", stored.client_id.as_str()),
            ("client_secret", stored.client_secret.as_str()),
            ("refresh_token", stored.refresh_token.as_str()),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await?;

    if !resp.status().is_success() {
        let body = resp.text().await?;
        anyhow::bail!("token refresh failed: {body}");
    }

    let token: TokenResponse = resp.json().await?;

    stored.access_token = token.access_token.clone();
    stored.expires_at = chrono::Utc::now().timestamp() + token.expires_in as i64;
    if let Some(rt) = token.refresh_token {
        stored.refresh_token = rt;
    }

    std::fs::write(&path, serde_json::to_string_pretty(&stored)?)?;

    Ok(token.access_token)
}
