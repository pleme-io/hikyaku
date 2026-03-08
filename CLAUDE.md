# Hikyaku — GPU-Rendered Terminal Email Client

## Build & Test

```bash
cargo build          # compile
cargo clippy         # lint
cargo test           # unit tests
cargo run -- launch  # launch TUI
cargo run -- mcp     # start MCP server (stdio)
cargo run -- check   # check account connectivity
cargo run -- script path/to/script.rhai  # run a Rhai script
```

Nix: `nix build`, `nix run`, `nix develop`

## Architecture

### Module Map

| Path | Purpose |
|------|---------|
| `src/main.rs` | CLI entry, subcommand dispatch (launch, mcp, check, script) |
| `src/config.rs` | Shikumi-based config: discovery, hot-reload, provider chain |
| `src/error.rs` | Error types |
| `src/mcp.rs` | MCP server via rmcp (stdio transport, 9 tools) |
| `src/accounts/mod.rs` | IMAP/SMTP account abstraction, connection management |
| `src/tui/mod.rs` | Ratatui TUI: main loop, drawing, event handling |
| `src/tui/app.rs` | Application state (views, focus, navigation) |
| `src/tui/theme.rs` | Nord color palette, semantic style methods |
| `src/automation/mod.rs` | Rhai scripting engine, builtin functions |
| `src/render/mod.rs` | Kitty/Sixel graphics via ratatui-image, HTML rendering |
| `module/default.nix` | Home-manager module (accounts, theme, scripting) |
| `themes/nord.toml` | Default Nord theme definition |

### Configuration (Shikumi)

Config discovery chain (first found wins):
1. `$HIKYAKU_CONFIG` env var
2. `$XDG_CONFIG_HOME/hikyaku/hikyaku.yaml`
3. `$HOME/.config/hikyaku/hikyaku.yaml`
4. `$HOME/.hikyaku` (legacy)

Provider chain (later wins): serde defaults -> env vars (`HIKYAKU_*`) -> config file.
Hot-reload via shikumi's `ConfigStore::load_and_watch()`.

### MCP Server

9 tools exposed via rmcp (stdio transport), following the ayatsuri pattern:
- `list_accounts`, `list_mailboxes`, `list_messages`, `read_message`
- `search_messages`, `send_message`, `move_message`, `delete_message`
- `run_script` (execute Rhai automation)

### Accounts

All providers (Gmail, Gmail Workspace, Protonmail) use IMAP/SMTP:
- Gmail: IMAP with OAuth2 XOAUTH2 SASL
- Protonmail: IMAP via Proton Bridge (localhost:1143, accepts invalid certs)
- Generic IMAP: standard login

Passwords resolved via `password_command` (shell command, e.g. `cat /run/secrets/...`).

### TUI Layout

```
+----------+--------------------+--------------------+
| Accounts | Message List       | Preview            |
| Mailboxes|                    | (with GPU images)  |
|          |                    |                    |
+----------+--------------------+--------------------+
| Status bar                                         |
+----------------------------------------------------+
```

Three-pane layout. Tab cycles focus. j/k for navigation.

### GPU Rendering (Ghostty/Kitty)

The render module uses `ratatui-image` to display images inline via:
- Kitty graphics protocol (Ghostty, Kitty)
- Sixel (xterm, foot)
- Halfblocks (fallback)

HTML emails are rendered to images and displayed inline. Protocol is
auto-detected or configurable via `rendering.graphics_protocol`.

### Rhai Scripting

Scripts loaded from `~/.config/hikyaku/init.rhai` and `~/.config/hikyaku/scripts/`.
Hot-reload supported. Built-in functions:
- `log(msg)`, `exec(cmd)`, `notify(title, body)`
- `move_to(mailbox)`, `tag(label)`, `mark_read()`, `mark_unread()`
- `archive()`, `delete()`, `forward(address)`

### Nix Integration

- `flake.nix`: `rustPlatform.buildRustPackage`, multi-system
- `module/default.nix`: home-manager module at `blackmatter.components.hikyaku`
- Config generated from Nix attrs -> YAML, scripts from Nix -> `.rhai` files
- Secrets via `password_command` pointing to sops-managed paths

## Key Dependencies

| Crate | Purpose |
|-------|---------|
| `shikumi` | Config discovery, hot-reload, ArcSwap store |
| `rmcp` | MCP server (stdio transport) |
| `rhai` | Scripting engine |
| `ratatui` + `crossterm` | TUI framework |
| `ratatui-image` | Kitty/Sixel inline image rendering |
| `async-imap` | IMAP client (tokio runtime) |
| `lettre` | SMTP sending |
| `mail-parser` | Email MIME parsing |
| `oauth2` | Gmail OAuth2 flows |
| `tokio` | Async runtime |

## Patterns from Ecosystem

- **Shikumi pattern**: ConfigDiscovery -> ProviderChain -> ConfigStore (from shikumi)
- **Ayatsuri MCP pattern**: rmcp with tool_router/tool_handler macros (from ayatsuri)
- **Ayatsuri scripting pattern**: Rhai with registered builtins (from ayatsuri)
- **Ayatsuri HM module pattern**: settings -> YAML, scripting -> .rhai files (from ayatsuri)
