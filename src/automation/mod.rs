use std::path::Path;
use std::sync::{Arc, RwLock};

use rhai::{Dynamic, Engine, Scope, AST};

/// Rhai scripting engine for email automation.
///
/// Exposes functions for email operations that can be called from `.rhai` scripts:
///   - `log(msg)` — log a message
///   - `exec(cmd)` — run a shell command
///   - `on_receive(filter, handler)` — register a handler for incoming mail
///   - `move_to(mailbox)` — move current message to mailbox
///   - `tag(label)` — apply a label/tag
///   - `mark_read()` — mark current message as read
///   - `mark_unread()` — mark current message as unread
///   - `archive()` — archive current message
///   - `delete()` — delete current message
///   - `notify(title, body)` — send a desktop notification
///   - `forward(address)` — forward current message
pub struct ScriptEngine {
    engine: Arc<RwLock<Engine>>,
    scripts: Vec<(String, AST)>,
}

impl ScriptEngine {
    pub fn new() -> Self {
        let mut engine = Engine::new();

        // Register utility module
        register_builtins(&mut engine);

        Self {
            engine: Arc::new(RwLock::new(engine)),
            scripts: Vec::new(),
        }
    }

    /// Load and compile a script file.
    pub fn load_script(&mut self, path: &Path) -> anyhow::Result<()> {
        let engine = self.engine.read().map_err(|e| anyhow::anyhow!("{e}"))?;
        let ast = engine.compile_file(path.into())?;
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        drop(engine);

        self.scripts.push((name, ast));
        tracing::info!(path = %path.display(), "loaded script");
        Ok(())
    }

    /// Load all `.rhai` scripts from a directory.
    pub fn load_directory(&mut self, dir: &Path) -> anyhow::Result<()> {
        if !dir.is_dir() {
            return Ok(());
        }

        let mut entries: Vec<_> = std::fs::read_dir(dir)?
            .filter_map(Result::ok)
            .filter(|e| {
                e.path()
                    .extension()
                    .is_some_and(|ext| ext == "rhai")
            })
            .collect();

        entries.sort_by_key(|e| e.path());

        for entry in entries {
            if let Err(e) = self.load_script(&entry.path()) {
                tracing::warn!(path = %entry.path().display(), error = %e, "failed to load script");
            }
        }

        Ok(())
    }

    /// Execute a script string and return the result.
    pub fn eval(&self, script: &str) -> anyhow::Result<Dynamic> {
        let engine = self.engine.read().map_err(|e| anyhow::anyhow!("{e}"))?;
        let mut scope = Scope::new();
        let result = engine.eval_with_scope::<Dynamic>(&mut scope, script)?;
        Ok(result)
    }

    /// Run all loaded init scripts.
    pub fn run_init(&self) -> anyhow::Result<()> {
        let engine = self.engine.read().map_err(|e| anyhow::anyhow!("{e}"))?;

        for (name, ast) in &self.scripts {
            let mut scope = Scope::new();
            if let Err(e) = engine.run_ast_with_scope(&mut scope, ast) {
                tracing::error!(script = %name, error = %e, "script execution failed");
            }
        }

        Ok(())
    }
}

fn register_builtins(engine: &mut Engine) {
    // Logging
    engine.register_fn("log", |msg: &str| {
        tracing::info!(target: "hikyaku::script", "{}", msg);
    });

    // Shell execution
    engine.register_fn("exec", |cmd: &str| -> Dynamic {
        match std::process::Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .output()
        {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                Dynamic::from(stdout)
            }
            Err(e) => {
                tracing::error!(cmd = %cmd, error = %e, "exec failed");
                Dynamic::from(String::new())
            }
        }
    });

    // Desktop notification
    engine.register_fn("notify", |title: &str, body: &str| {
        tracing::info!(target: "hikyaku::notify", title = %title, body = %body, "notification");
        // TODO: integrate with notify-rust or osascript
    });

    // Email operations (stubs — wired up when account context is available)
    engine.register_fn("move_to", |_mailbox: &str| {
        tracing::debug!("move_to called from script (no-op without context)");
    });

    engine.register_fn("tag", |_label: &str| {
        tracing::debug!("tag called from script (no-op without context)");
    });

    engine.register_fn("mark_read", || {
        tracing::debug!("mark_read called from script (no-op without context)");
    });

    engine.register_fn("mark_unread", || {
        tracing::debug!("mark_unread called from script (no-op without context)");
    });

    engine.register_fn("archive", || {
        tracing::debug!("archive called from script (no-op without context)");
    });

    engine.register_fn("delete", || {
        tracing::debug!("delete called from script (no-op without context)");
    });

    engine.register_fn("forward", |_address: &str| {
        tracing::debug!("forward called from script (no-op without context)");
    });
}
