//! Exclusion using the same OS locks as independently installed CLI versions.
use anyhow::{Context, Result};
use std::{fs, path::Path};

/// Own this in the process which performs installation, for the entire protected
/// interval. Dropping this in an exiting desktop is NOT a Windows handoff.
/// A guardian must acquire its own guard before acknowledging READY.
pub struct ClientUpdateGuard {
    locks: Vec<fs::File>,
}

impl ClientUpdateGuard {
    /// Native callers derive these values from config, never renderer input.
    pub fn current() -> Result<Self> {
        Self::acquire(
            &crate::config::home_dir(),
            &crate::config::console_api_base_url(),
        )
    }

    /// Nonblocking: preserve active work, release all partial acquisitions on failure.
    /// `home` is the OS user home (not the `.tokenmarket` directory).
    pub fn acquire(home: &Path, origin: &str) -> Result<Self> {
        let root = home.join(".tokenmarket");
        // Sort and deduplicate actual legacy paths, including DuMate's shared alias.
        let mut paths = std::collections::BTreeMap::new();
        paths.insert(
            root.join("desktop-connections/operation.lock"),
            "工具连接操作正在进行，请完成后重试安装客户端更新",
        );
        for adapter in crate::adapters() {
            paths.insert(
                crate::installation::installation_lock_path(home, adapter.id),
                "工具安装或升级正在进行，请完成后重试安装客户端更新",
            );
        }
        let login_reason = "共享登录操作正在进行，请完成后重试安装客户端更新";
        paths.insert(
            crate::client::login_directory_in(&root, origin).join("operation.lock"),
            login_reason,
        );
        match fs::read_dir(root.join("client-login")) {
            Ok(entries) => {
                for entry in entries {
                    let entry = entry.context("无法检查共享登录操作，请稍后重试")?;
                    if entry.file_type()?.is_dir() {
                        paths.insert(entry.path().join("operation.lock"), login_reason);
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("无法检查共享登录操作，请稍后重试"),
        }
        let mut guard = Self { locks: Vec::new() };
        for (path, reason) in paths {
            fs::create_dir_all(path.parent().unwrap())
                .context("无法保护客户端更新，请检查本机目录权限后重试")?;
            let file = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(path)
                .context("无法保护客户端更新，请检查本机目录权限后重试")?;
            file.try_lock().context(reason)?;
            guard.locks.push(file);
        }
        Ok(guard)
    }
}

impl Drop for ClientUpdateGuard {
    fn drop(&mut self) {
        for file in self.locks.iter().rev() {
            let _ = file.unlock();
        }
    }
}

mod journal;
pub use journal::{ClientUpdateJournal, ClientUpdatePhase};
