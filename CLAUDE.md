# Hikyaku (飛脚) -- GPU Email Client

> **★★★ CSE / Knowable Construction.** This repo operates under
> **Constructive Substrate Engineering** — canonical specification at
> [`pleme-io/theory/CONSTRUCTIVE-SUBSTRATE-ENGINEERING.md`](https://github.com/pleme-io/theory/blob/main/CONSTRUCTIVE-SUBSTRATE-ENGINEERING.md).
> The Compounding Directive (operational rules: solve once, load-bearing
> fixes only, idiom-first, models stay current, direction beats velocity)
> is in the org-level pleme-io/CLAUDE.md ★★★ section. Read both before
> non-trivial changes. Email client with IMAP/SMTP, local search index,
> MCP server, Rhai scripting — typed surfaces all the way down.

GPU-rendered email client with IMAP/SMTP, local search index, MCP server,
and Rhai scripting. Currently TUI-based (ratatui); migrating to GPU rendering
via garasu/madori.

## Build & Test

```bash
cargo build                        # compile
cargo clippy                       # lint
cargo test                         # unit tests
cargo run -- launch                # launch TUI
cargo run -- mcp                   # start MCP server (stdio)
cargo run -- check                 # check account connectivity
cargo run -- auth <account>        # OAuth2 device auth flow
cargo run -- script path/to.rhai   # run a Rhai script
cargo run -- search "query"        # search local index
cargo run -- sync                  # force sync all accounts to index
```

Nix: `nix build`, `nix run`, `nix develop`

## Competitive Position

| Competitor | Weakness hikyaku addresses |
|-----------|---------------------------|
| aerc (Go, TUI) | GPU rendering, MCP automation, Rhai scripting, HTML preview |
| neomutt (C, TUI) | Modern Rust, GPU rendering, MCP, structured config (not muttrc) |
| Himalaya (Rust, TUI/CLI) | Full GPU GUI, built-in MCP server, visual HTML preview |
| Thunderbird (C++/JS, GUI) | Lightweight, vim-modal, Nix-configured, MCP-drivable |
| Notmuch (C, tag-based) | GPU rendering, IMAP-native, integrated MCP for AI workflows |

Unique: GPU-rendered email with inline HTML preview (DOM+CSS layout via garasu),
MCP server for AI email workflows, Rhai automation, Nix-managed config.

## Architecture

### Data Flow

```
IMAP Server --+                           +-- TUI (ratatui, current)
              |                           |
  async-imap -+--> AccountManager --+---> +--> GPU (garasu/madori, planned)
              |                     |     |
SMTP (lettre) +<-- send_message <---+     +-- MCP (rmcp, stdio)
                                    |
                                    +--> EmailIndex (sakuin + SQLite)
                                    |         |
                              Rhai engine     +--> full-text search (tantivy)
                             (automation)     +--> metadata (SeaORM/SQLite)
```

### Module Map

| Path | Purpose | Key Types |
|------|---------|-----------|
| `src/main.rs` | CLI entry, subcommand dispatch | `Cli`, `SubCmd` |
| `src/config.rs` | shikumi config with hot-reload | `HikyakuConfig`, `AccountConfig`, `IndexConfig` |
| `src/error.rs` | Error types | `HikyakuError` |
| `src/accounts/mod.rs` | IMAP/SMTP account management | `AccountManager`, `Account`, `Mailbox`, `MessageSummary` |
| `src/accounts/oauth2.rs` | Gmail OAuth2 device flow | `OAuth2Config`, `device_auth_flow()` |
| `src/accounts/sync.rs` | Background IMAP sync | `SyncTask`, IDLE push |
| `src/index/mod.rs` | Local search index | `EmailIndex`, `SearchResult` |
| `src/mcp.rs` | MCP server via rmcp | 9 tools (list, read, send, search, etc.) |
| `src/automation/mod.rs` | Rhai scripting engine | `ScriptEngine`, builtin functions |
| `src/tui/mod.rs` | Ratatui TUI main loop | `run()`, event loop |
| `src/tui/app.rs` | TUI application state | `App`, views, focus, navigation |
| `src/tui/theme.rs` | Nord color palette | `HikyakuTheme`, semantic styles |
| `src/render/mod.rs` | Image rendering | Kitty/Sixel graphics, HTML-to-image |
| `module/default.nix` | Home-manager module | `blackmatter.components.hikyaku.*` |

### Account System

All providers use IMAP/SMTP with provider-specific auth:

| Provider | Auth | IMAP | SMTP | Notes |
|----------|------|------|------|-------|
| Gmail | OAuth2 XOAUTH2 SASL | imap.gmail.com:993 | smtp.gmail.com:587 | Device flow via `hikyaku auth` |
| Gmail Workspace | OAuth2 XOAUTH2 SASL | imap.gmail.com:993 | smtp.gmail.com:587 | Same as Gmail |
| Protonmail | Password (bridge) | 127.0.0.1:1143 | 127.0.0.1:1025 | Via Proton Bridge, accept invalid certs |
| Generic IMAP | Password | configurable | configurable | Standard LOGIN |

Passwords resolved via `password_command` (shell command, e.g., `cat /run/secrets/...`).
OAuth2 tokens cached in `~/.local/share/hikyaku/tokens/`.

### Local Index

Two-layer index for fast offline search:

**Tantivy** (via sakuin): Full-text search over subject, from, body preview.
**SQLite** (via SeaORM): Structured metadata -- UID, account, mailbox, flags, dates.

Data dir: `~/.local/share/hikyaku/index/`

The index enables:
- Instant search across all accounts without IMAP SEARCH
- Offline message browsing (metadata + preview)
- Tag-based filtering (Notmuch-style)
- Unread/flagged status tracking

### Background Sync

`hikyaku sync` does a full IMAP fetch and indexes all messages. The planned
daemon mode will use IMAP IDLE for push updates and periodic CONDSTORE-based
delta sync.

## GUI Layout

### Current (TUI -- ratatui)

```
+----------+--------------------+--------------------+
| Accounts | Message List       | Preview            |
| Mailboxes|   From | Subject   | (with inline imgs) |
|          |                    |                    |
+----------+--------------------+--------------------+
| Status bar                                         |
+----------------------------------------------------+
```

Three-pane layout. `Tab` cycles focus. `j/k` navigation. Inline images via
Kitty graphics protocol (Ghostty, Kitty) or Sixel (xterm, foot) or halfblocks
(fallback). Auto-detected or configurable via `rendering.graphics_protocol`.

### Target (GPU -- garasu/madori)

```
+----------+--------------------+--------------------+
| Accounts | Message List       | Reading Pane       |
| Mailboxes|   [*] From Subject | HTML rendered via   |
|  INBOX   |   [ ] From Subject | garasu (DOM+CSS    |
|  Sent    |   [ ] From Subject | layout engine)     |
|  Drafts  |                    |                    |
|  Archive |                    | Attachments:       |
|  Tags... |                    |  [file.pdf]        |
+----------+--------------------+--------------------+
| Mode: NORMAL | INBOX (12 unread) | account: work    |
+----------+--------------------+--------------------+
```

GPU rendering enables:
- Rich HTML email display (not just plain text)
- Inline image rendering (photos, charts, diagrams)
- Smooth scrolling through long emails
- Font rendering with ligatures and emoji
- Custom WGSL shader effects

### HTML Email Rendering

The key differentiator. Most TUI email clients show plain text or pipe to a
browser. Hikyaku renders HTML emails natively in the GPU pipeline:

1. Parse HTML (html5ever)
2. Apply CSS cascade (lightningcss)
3. Layout (taffy flexbox/grid)
4. Render to garasu GPU pipeline

This is effectively the aranami (nami) DOM+CSS engine embedded in hikyaku.
Share the rendering core as a library (`nami-core`) that both aranami and
hikyaku consume.

## Configuration (shikumi)

File: `~/.config/hikyaku/hikyaku.yaml`
Env override: `$HIKYAKU_CONFIG`
Env prefix: `HIKYAKU_`
Hot-reload: shikumi `ConfigStore::load_and_watch()` (symlink-aware)

```yaml
accounts:
  personal:
    provider: gmail
    address: "user@gmail.com"
    oauth2: true
    password_command: "cat /run/secrets/gmail-token"
  work:
    provider: gmail_workspace
    address: "user@company.com"
    oauth2: true
    password_command: "cat /run/secrets/work-token"
  proton:
    provider: protonmail
    address: "user@proton.me"
    imap_host: "127.0.0.1"
    imap_port: 1143
    smtp_host: "127.0.0.1"
    smtp_port: 1025
    password_command: "cat /run/secrets/proton-bridge"

theme:
  name: nord

rendering:
  graphics_protocol: auto   # auto | kitty | sixel | halfblocks
  inline_images: true

index:
  batch_size: 500
  data_dir: "~/.local/share/hikyaku/index"

keybindings: {}
```

## Hotkey System (awase)

Three modes: Normal (index), Normal (read), Insert (compose), Command.

### Normal Mode -- Index View
| Key | Action |
|-----|--------|
| `j` / `k` | Next/previous message |
| `J` / `K` | Next/previous mailbox |
| `Enter` | Open message (switch to read view) |
| `c` | Compose new message |
| `d` | Delete message |
| `a` | Archive message |
| `r` | Reply to message |
| `R` | Reply all |
| `f` | Forward message |
| `s` | Star/flag message |
| `t` | Tag message |
| `u` | Toggle read/unread |
| `/` | Search |
| `:` | Command mode |
| `Tab` | Cycle focus: accounts -> mailboxes -> messages -> preview |
| `g g` | Jump to top |
| `G` | Jump to bottom |
| `Ctrl-u` / `Ctrl-d` | Half-page scroll |

### Normal Mode -- Read View
| Key | Action |
|-----|--------|
| `j` / `k` | Scroll message body |
| `q` | Back to index |
| `r` | Reply |
| `R` | Reply all |
| `f` | Forward |
| `a` | Archive |
| `d` | Delete |
| `\|` | Pipe message to shell command |
| `o` | Open attachment |
| `n` / `p` | Next/previous message |

### Insert Mode -- Compose
| Key | Action |
|-----|--------|
| `Esc` | Back to Normal mode (save draft) |
| `Ctrl-s` | Send message |
| `Ctrl-a` | Attach file |
| `Tab` | Cycle To/CC/BCC/Subject/Body fields |

### Command Mode
| Command | Action |
|---------|--------|
| `:search query` | Search all accounts |
| `:folder name` | Switch to mailbox/folder |
| `:tag name` | Apply tag to selected message |
| `:send` | Send composed message |
| `:attach path` | Attach file |
| `:account name` | Switch active account |
| `:sync` | Force sync all accounts |
| `:quit` / `:q` | Quit hikyaku |

## MCP Server (rmcp)

9 tools, stdio transport. Follows the ayatsuri MCP pattern.

| Tool | Parameters | Description |
|------|-----------|-------------|
| `list_accounts` | | List configured accounts |
| `list_mailboxes` | account | List mailboxes for an account |
| `list_messages` | account, mailbox, [limit] | List message summaries |
| `read_message` | account, mailbox, uid | Read full message |
| `search_messages` | query, [account], [limit] | Search local index |
| `send_message` | to, subject, body, [account], [cc], [bcc] | Send email |
| `move_message` | account, mailbox, uid, target | Move to folder |
| `delete_message` | account, mailbox, uid | Delete message |
| `run_script` | script | Execute Rhai automation |

## Rhai Scripting (soushi pattern)

Scripts loaded from `~/.config/hikyaku/init.rhai` and `~/.config/hikyaku/scripts/`.
Hot-reload supported via file watcher.

### Builtin Functions

```rhai
// Navigation
hikyaku.inbox()                    // List inbox messages
hikyaku.read(id)                   // Read message by ID
hikyaku.folder(name)               // Switch to folder
hikyaku.search(query)              // Search messages

// Actions
hikyaku.send(to, subject, body)    // Send email
hikyaku.reply(id)                  // Reply to message
hikyaku.reply_all(id)              // Reply all
hikyaku.forward(id, to)            // Forward message
hikyaku.archive(id)                // Archive message
hikyaku.delete(id)                 // Delete message
hikyaku.tag(id, tag)               // Apply tag
hikyaku.move_to(id, folder)        // Move to folder
hikyaku.mark_read(id)              // Mark as read
hikyaku.mark_unread(id)            // Mark as unread

// Utilities
log(msg)                           // Log message
exec(cmd)                          // Execute shell command
notify(title, body)                // Desktop notification

// Event hooks
fn on_receive(msg) { ... }        // Called for new messages
fn on_startup() { ... }           // Called on launch
fn on_shutdown() { ... }          // Called on exit
```

### Example: Auto-archive GitHub notifications

```rhai
fn on_receive(msg) {
    if msg.from.contains("notifications@github.com") && msg.is_read {
        hikyaku.archive(msg.id);
    }
}
```

## Nix Integration

### flake.nix

```
packages.${system}.default  -- hikyaku binary (multi-system: aarch64-darwin, x86_64-linux, etc.)
overlays.default            -- pkgs.hikyaku
homeManagerModules.default  -- blackmatter.components.hikyaku.*
devShells.${system}.default -- dev environment
```

### Home-Manager Module

`blackmatter.components.hikyaku`:
- `enable` -- install hikyaku
- `package` -- hikyaku package (default: pkgs.hikyaku)
- `settings` -- attrs -> `~/.config/hikyaku/hikyaku.yaml`
- `scripting.initScript` -- lines -> `~/.config/hikyaku/init.rhai`
- `scripting.extraScripts` -- attrsOf lines -> `~/.config/hikyaku/scripts/<name>.rhai`
- `scripting.hotReload` -- bool (default: true)

Secrets: passwords via `password_command` pointing to sops-managed paths.
Never store credentials in Nix config directly.

## Key Dependencies

| Crate | Purpose |
|-------|---------|
| `shikumi` | Config discovery, hot-reload, ArcSwap store |
| `rmcp` 0.15 | MCP server (stdio transport) |
| `rhai` 1.23 | Scripting engine |
| `ratatui` + `crossterm` | TUI framework (current) |
| `ratatui-image` | Kitty/Sixel inline image rendering (current) |
| `async-imap` | IMAP client (tokio runtime) |
| `lettre` | SMTP sending |
| `mail-parser` | Email MIME parsing |
| `sakuin` | Full-text search (tantivy wrapper) |
| `sea-orm` + `rusqlite` | Structured metadata storage |
| `chromiumoxide` | HTML-to-image rendering (current approach) |
| `tokio` | Async runtime |
| `schemars` | JSON Schema for MCP tool parameters |

### Planned Dependencies (GPU migration)

| Crate | Purpose |
|-------|---------|
| `garasu` | GPU context, text rendering, shaders |
| `madori` | App framework (event loop, render loop) |
| `egaku` | Widget toolkit (text input, lists, tabs, splits) |
| `irodzuki` | GPU theming (base16 -> uniforms) |
| `mojiban` | Rich text rendering (markdown in email) |
| `hasami` | Clipboard (copy email text/addresses) |
| `kaname` | MCP server framework (replace raw rmcp) |
| `soushi` | Rhai scripting (replace raw rhai setup) |
| `awase` | Hotkey system (modal vim bindings) |
| `tsunagu` | Daemon mode (background sync) |
| `tsuuchi` | Desktop notifications |
| `todoku` | HTTP client (OAuth2 token refresh) |

## Implementation Roadmap

### Phase 1 -- Core Email (complete)
- [x] shikumi config with hot-reload
- [x] Multi-account IMAP connection (Gmail, Workspace, Protonmail)
- [x] OAuth2 device flow for Gmail
- [x] Message listing and reading
- [x] SMTP sending via lettre
- [x] Three-pane TUI layout (ratatui)
- [x] Inline image rendering (Kitty/Sixel/halfblocks)
- [x] HTML email rendering (chromiumoxide -> image)
- [x] MCP server (9 tools, rmcp stdio)
- [x] Rhai scripting engine with builtins
- [x] Local search index (sakuin + SQLite)
- [x] Background sync (`hikyaku sync`)
- [x] Home-manager module with settings + scripting

### Phase 2 -- Search & Sync
- [ ] IMAP IDLE for push notifications
- [ ] CONDSTORE delta sync (only fetch changed messages)
- [ ] Tag system (virtual folders via tags, Notmuch-style)
- [ ] Thread view (group messages by In-Reply-To/References headers)
- [ ] Attachment handling (download, open, save)
- [ ] Draft management (save/resume/delete)

### Phase 3 -- GPU Migration
- [ ] Replace ratatui with garasu/madori/egaku
- [ ] Implement GPU three-pane layout
- [ ] HTML email rendering via nami-core (DOM+CSS -> garasu)
- [ ] Inline images via garasu texture upload
- [ ] irodzuki theming (Nord -> GPU uniforms)
- [ ] Font rendering with ligatures and emoji

### Phase 4 -- Advanced Features
- [ ] awase hotkey system (Normal/Insert/Command modes)
- [ ] Compose editor with markdown preview (mojiban)
- [ ] Contact autocomplete
- [ ] PGP/GPG encryption + signing
- [ ] CalDAV integration (meeting invites)
- [ ] tsunagu daemon mode (background IMAP IDLE)
- [ ] tsuuchi desktop notifications

### Phase 5 -- Polish
- [ ] Multiple identity support (send from different addresses)
- [ ] Custom filters/rules (server-side IMAP SIEVE, client-side Rhai)
- [ ] Email templates
- [ ] Conversation view (threaded display)
- [ ] Keyboard-driven attachment preview
- [ ] Accessibility: screen reader, font scaling

## Design Decisions

### Why ratatui first (not garasu from day one)?
Email requires complex IMAP protocol handling, OAuth2 flows, MIME parsing, and
search indexing. Starting with ratatui let us validate the email layer without
GPU rendering complexity. The GPU migration (Phase 3) replaces only the view
layer -- the account, index, MCP, and scripting modules are unchanged.

### Why chromiumoxide for HTML (not a native renderer)?
Current approach: render HTML to PNG via headless Chrome, display as inline image.
This works but requires Chrome. Target approach: nami-core (html5ever + lightningcss
+ taffy + garasu) for native rendering without browser dependency. The migration
is planned for Phase 3.

### Why sakuin + SQLite (not just IMAP SEARCH)?
IMAP SEARCH is slow (server-side), limited (no full-text in body), and requires
connectivity. A local tantivy index provides instant search across all accounts,
offline browsing, and tag-based filtering. SQLite stores structured metadata
(UIDs, flags, dates) for fast listing without IMAP roundtrips.

### Why `password_command` (not direct credential storage)?
Security. Passwords and OAuth2 tokens are resolved at runtime via shell commands
(e.g., `cat /run/secrets/...`). This integrates with sops, 1Password CLI, or
any secret manager. Credentials never appear in config files or Nix store.

### Why async-imap (not imap-rs)?
async-imap is the tokio-native IMAP client. Since hikyaku's event loop is
async (tokio runtime for TUI, MCP, and sync), using an async IMAP client avoids
blocking the event loop with synchronous I/O.
