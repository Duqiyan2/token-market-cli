//! Bounded connection diagnostics. Never serialize arbitrary error text, URLs,
//! configuration, process output, credentials, or account identifiers.
use crate::ToolId;
use serde_json::{json, Value};
use std::{
    cell::Cell,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
    time::Instant,
};

const MAX_BYTES: u64 = 1024 * 1024;
static WRITER: Mutex<()> = Mutex::new(());

/// Record failures that happen before entering the shared connection manager.
pub fn preflight_failure(tool: ToolId, error: &anyhow::Error) {
    let mut operation = Operation::current("connection_preflight", tool);
    operation.stage("validate_context");
    operation.finished.set(true);
    operation.record("failed", safe_error(error));
}

pub struct Operation {
    directory: PathBuf,
    id: String,
    operation: &'static str,
    tool: ToolId,
    stage: &'static str,
    started: Instant,
    finished: Cell<bool>,
}

impl Operation {
    pub fn current(operation: &'static str, tool: ToolId) -> Self {
        Self::new(
            crate::config::token_market_home().join("desktop-connections/logs"),
            operation,
            tool,
        )
    }

    pub(crate) fn new(directory: PathBuf, operation: &'static str, tool: ToolId) -> Self {
        let operation = Self {
            directory,
            id: crate::auth::create_pkce().code_verifier,
            operation,
            tool,
            stage: "begin",
            started: Instant::now(),
            finished: Cell::new(false),
        };
        operation.record("started", Value::Null);
        operation
    }

    pub fn stage(&mut self, stage: &'static str) {
        self.stage = stage;
        self.record("started", Value::Null);
    }

    /// Logging is best effort and must never change the connection's result.
    pub fn finish<T>(&self, result: &anyhow::Result<T>) {
        self.finished.set(true);
        match result {
            Ok(_) => self.record("succeeded", Value::Null),
            Err(error) => self.record("failed", safe_error(error)),
        }
    }

    pub(crate) fn failure(&self, error: &anyhow::Error) {
        self.record("step_failed", safe_error(error));
    }

    pub(crate) fn http_response(&self, status: u16) {
        self.record("http_response", json!({"http_status": status}));
    }

    pub(crate) fn credential_reused(&self, reused: bool) {
        self.record("credential_ready", json!({"key_reused": reused}));
    }

    pub(crate) fn model_count(&self, count: usize) {
        self.record("models_loaded", json!({"model_count": count}));
    }

    pub(crate) fn service_rejected(&self) {
        self.record("service_rejected", Value::Null);
    }

    pub(crate) fn credential_state(&self, state: &str) {
        // Untrusted response strings must never become arbitrary log content.
        let state = match state {
            "VALID"
            | "MIGRATION_REQUIRED"
            | "NOT_FOUND"
            | "DISABLED"
            | "EXPIRED"
            | "DELETED"
            | "SECRET_CHANGED"
            | "LOCAL_COPY_REQUIRED"
            | "PENDING"
            | "SAVED"
            | "APPLIED"
            | "ISSUED" => state,
            _ => "UNKNOWN",
        };
        self.record("credential_state", json!({"state": state}));
    }

    pub(crate) fn probe_exit(&self, code: Option<i32>, diagnostics: &Path) {
        let mut bytes = Vec::new();
        if let Ok(file) = fs::File::open(diagnostics) {
            let _ = file.take(65536).read_to_end(&mut bytes);
        }
        let text = String::from_utf8_lossy(&bytes);
        let codes: Vec<_> = [
            "ERR_MODULE_NOT_FOUND",
            "ERR_UNKNOWN_FILE_EXTENSION",
            "ERR_DLOPEN_FAILED",
            "ERR_ACCESS_DENIED",
            "EPERM",
            "ENOENT",
            "EACCES",
            "ECONNREFUSED",
            "ETIMEDOUT",
            "AssertionError",
            "SyntaxError",
            "ReferenceError",
            "TypeError",
        ]
        .into_iter()
        .filter(|code| text.contains(code))
        .collect();
        self.record(
            "process_exit",
            json!({"exit_code":code,"probe_error_codes":codes}),
        );
    }

    fn record(&self, status: &'static str, error: Value) {
        let event = json!({
            "timestamp": crate::config::now_iso8601(),
            "operation_id": self.id,
            "operation": self.operation,
            "tool": self.tool,
            "stage": self.stage,
            "status": status,
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "pid": std::process::id(),
            "core_version": env!("CARGO_PKG_VERSION"),
            "elapsed_ms": self.started.elapsed().as_millis() as u64,
            "error": error,
        });
        let _ = append(&self.directory, &event);
    }
}

impl Drop for Operation {
    fn drop(&mut self) {
        if !self.finished.get() {
            self.record("interrupted", Value::Null);
        }
    }
}

fn safe_error(error: &anyhow::Error) -> Value {
    let mut value = json!({"reason":"unclassified"});
    // Only fixed, known reason codes are retained; never persist error Display
    // or Debug, which can contain HTTP bodies, paths, secrets or JS output.
    for cause in error.chain() {
        let text = cause.to_string();
        for (needle, code) in [
            ("请先打开千问办公并登录", "login_required"),
            ("请先在千问办公完成登录", "login_required"),
            ("千问办公登录状态无法确认", "login_status_invalid"),
            ("独立身份数据库", "identity_database_unsupported"),
            ("模型数据库结构已改变", "database_schema_unsupported"),
            ("无法打开千问办公数据库", "database_unavailable"),
            (".env 存在重复", "duplicate_managed_env"),
            ("千问办公尚未适配本地接入方案", "adaptation_required"),
            ("无法启动千问办公的接入自检", "probe_spawn_failed"),
            ("无法读取千问办公 SDK", "sdk_read_failed"),
            ("无法读取本地代理程序", "bridge_read_failed"),
            ("凭证", "credential_operation_failed"),
            ("无法获取模型目录", "model_catalogue_failed"),
            ("未安装", "installation_missing"),
            ("请先登录", "login_required"),
            ("登录已过期", "session_expired"),
            ("已改变", "stale_context"),
            ("正在进行", "operation_busy"),
            ("符号链接", "symlink_rejected"),
            ("配置回滚失败", "rollback_failed"),
            ("原配置已恢复", "write_failed_rolled_back"),
            ("连接校验失败", "key_access_denied"),
            ("人工修改", "configuration_conflict"),
            ("配置", "configuration_failed"),
        ] {
            if value["reason"] == "unclassified" && text.contains(needle) {
                value["reason"] = json!(code);
            }
        }
        if let Some(io) = cause.downcast_ref::<std::io::Error>() {
            value["io_kind"] = json!(format!("{:?}", io.kind()));
            value["os_error"] = json!(io.raw_os_error());
        }
        if let Some(http) = cause.downcast_ref::<reqwest::Error>() {
            value["http_status"] = json!(http.status().map(|status| status.as_u16()));
            value["timeout"] = json!(http.is_timeout());
            value["connect_error"] = json!(http.is_connect());
        }
        if let Some(parse) = cause.downcast_ref::<serde_json::Error>() {
            value["json_line"] = json!(parse.line());
            value["json_column"] = json!(parse.column());
        }
        if let Some(rusqlite::Error::SqliteFailure(code, _)) = cause.downcast_ref() {
            value["sqlite_code"] = json!(code.extended_code);
        }
    }
    value
}

fn append(directory: &Path, event: &Value) -> anyhow::Result<()> {
    let _guard = WRITER
        .lock()
        .map_err(|_| anyhow::anyhow!("diagnostic lock"))?;
    // Refuse links rather than following a redirected diagnostic destination.
    for path in directory.ancestors().take(3) {
        if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
            return Ok(());
        }
    }
    fs::create_dir_all(directory)?;
    let path = directory.join("connections.jsonl");
    let previous = directory.join("connections.previous.jsonl");
    let lock_path = directory.join("writer.lock");
    for file in [&path, &previous, &lock_path] {
        if fs::symlink_metadata(file).is_ok_and(|m| m.file_type().is_symlink()) {
            return Ok(());
        }
    }
    // Serialize rotation and whole JSON lines across the CLI and desktop processes.
    let mut lock_options = OpenOptions::new();
    lock_options
        .create(true)
        .read(true)
        .write(true)
        .truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        lock_options.mode(0o600);
    }
    let lock = lock_options.open(lock_path)?;
    lock.lock()?;
    if fs::metadata(&path).is_ok_and(|m| m.len() >= MAX_BYTES) {
        if previous.exists() {
            fs::remove_file(&previous)?;
        }
        fs::rename(&path, previous)?;
    }
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    let mut bytes = serde_json::to_vec(event)?;
    bytes.push(b'\n');
    file.write_all(&bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_are_structured_and_never_include_arbitrary_text() {
        let root = tempfile::tempdir().unwrap();
        let mut operation = Operation::new(root.path().into(), "connect", ToolId::QwenWork);
        operation.stage("platform_key");
        let error = anyhow::Error::new(std::io::Error::from_raw_os_error(5))
            .context("Bearer sk-fixture-secret password=private https://host/?token=private");
        operation.finish::<()>(&Err(error));
        let text = fs::read_to_string(root.path().join("connections.jsonl")).unwrap();
        assert!(!text.contains("private") && !text.contains("sk-fixture-secret"));
        let event: Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
        assert_eq!(event["error"]["os_error"], 5);
        assert_eq!(event["stage"], "platform_key");
    }

    #[test]
    fn diagnostics_rotate_and_unwritable_destinations_do_not_change_results() {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join("connections.jsonl"),
            vec![b' '; MAX_BYTES as usize],
        )
        .unwrap();
        let operation = Operation::new(root.path().into(), "connect", ToolId::QwenWork);
        operation.finish(&Ok(()));
        assert!(root.path().join("connections.previous.jsonl").is_file());
        assert!(
            fs::metadata(root.path().join("connections.jsonl"))
                .unwrap()
                .len()
                < 4096
        );
        let blocked = root.path().join("file");
        fs::write(&blocked, "not a directory").unwrap();
        Operation::new(blocked, "connect", ToolId::QwenWork).finish(&Ok(()));
    }
}
