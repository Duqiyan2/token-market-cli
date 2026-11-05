//! QwenWorkCN local BYOK contract. Database I/O exposes a row snapshot,
//! never SQLite file bytes: transactions only change our named model records.
use crate::inventory::HostPlatform;
use anyhow::{bail, Context, Result};
use rusqlite::{types::ValueRef, Connection, OpenFlags, TransactionBehavior};
use serde_json::{json, Map, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub(crate) const PREFIX: &str = "byok-tokenmarket-";
const COLUMNS: &[&str] = &[
    "key",
    "legacy_key",
    "source",
    "display_name",
    "provider",
    "type",
    "url",
    "style",
    "model",
    "format",
    "is_vl",
    "is_reasoning",
    "max_input_tokens",
    "max_output_tokens",
    "encrypted_parameters",
    "extra_params",
    "migrated_from",
    "created_at",
    "updated_at",
];

pub(crate) fn database_path(home: &Path, platform: HostPlatform) -> PathBuf {
    let base = match platform {
        HostPlatform::MacOs => home.join("Library/Application Support"),
        HostPlatform::Windows => {
            if home == crate::config::home_dir() {
                std::env::var_os("APPDATA")
                    .filter(|s| !s.is_empty())
                    .map(PathBuf::from)
                    .unwrap_or_else(|| home.join("AppData/Roaming"))
            } else {
                home.join("AppData/Roaming")
            }
        }
        HostPlatform::Unix => home.join(".config"),
    };
    base.join("QwenWorkCN/data/agents.db")
}

pub(crate) fn dotenv_path(database: &Path) -> Result<PathBuf> {
    let current_home = crate::config::home_dir();
    for platform in [
        HostPlatform::MacOs,
        HostPlatform::Windows,
        HostPlatform::Unix,
    ] {
        if database == database_path(&current_home, platform) {
            return Ok(current_home.join(".qwenworkcn/.env"));
        }
    }
    let home = match database.ancestors().nth(4) {
        Some(home)
            if database.ends_with("Library/Application Support/QwenWorkCN/data/agents.db")
                || database.ends_with("AppData/Roaming/QwenWorkCN/data/agents.db") =>
        {
            home.parent()
        }
        _ if database.ends_with(".config/QwenWorkCN/data/agents.db") => database.ancestors().nth(4),
        _ => None,
    }
    .context("无法确认千问办公设备配置目录")?;
    Ok(home.join(".qwenworkcn/.env"))
}

pub(crate) fn require_ready(home: &Path, platform: HostPlatform) -> Result<()> {
    if platform == HostPlatform::Unix {
        bail!("千问办公接入当前支持 macOS / Windows 客户端")
    }
    let status: Value = serde_json::from_slice(
        &fs::read(home.join(".qwenworkcn/.status.json"))
            .context("请先打开千问办公并登录，再完全退出后接入")?,
    )
    .context("千问办公登录状态无法确认，请重新登录后接入")?;
    if status["logged_in"] != true || status["product"] != "qwenworkcn" {
        bail!("请先在千问办公完成登录，再完全退出后接入")
    }
    let db = database_path(home, platform);
    if db.with_file_name("agents.identity-v2.db").exists()
        || db.with_file_name(".local-identity-format.json").exists()
    {
        bail!(
            "{}（独立身份数据库）",
            crate::qwenwork_runtime::ADAPTATION_REQUIRED
        )
    }
    read_snapshot(&db)?;
    Ok(())
}

/// Never stop a user's running app. Fixtures in a separate home have no live cache.
pub(crate) fn require_stopped(database: &Path) -> Result<()> {
    let platform = if cfg!(target_os = "macos") {
        HostPlatform::MacOs
    } else if cfg!(windows) {
        HostPlatform::Windows
    } else {
        HostPlatform::Unix
    };
    if database != database_path(&crate::config::home_dir(), platform) {
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        let status = std::process::Command::new("/usr/bin/pgrep")
            .args(["-f", "/QwenWorkCN.app/Contents/"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .context("无法确认千问办公是否已退出")?;
        if status.code() != Some(1) {
            bail!("请先完全退出千问办公（含后台进程），再接入或取消接入")
        }
    }
    #[cfg(windows)]
    {
        let status = std::process::Command::new("powershell.exe").args([
            "-NoProfile", "-NonInteractive", "-Command",
            "$ErrorActionPreference='Stop'; if (Get-Process -Name QwenWorkCN -ErrorAction SilentlyContinue) { exit 2 }; exit 0",
        ]).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status()
            .context("无法确认千问办公是否已退出")?;
        if !status.success() {
            bail!("请先完全退出千问办公，再接入或取消接入")
        }
    }
    Ok(())
}

pub(crate) fn is_database(path: &Path) -> bool {
    path.file_name().is_some_and(|v| v == "agents.db")
}

fn open(path: &Path, write: bool) -> Result<Connection> {
    for suffix in ["", "-wal", "-shm"] {
        let file = path.with_file_name(format!("agents.db{suffix}"));
        if fs::symlink_metadata(file).is_ok_and(|m| m.file_type().is_symlink()) {
            bail!("千问办公数据库是符号链接，未修改配置")
        }
    }
    let flags = if write {
        OpenFlags::SQLITE_OPEN_READ_WRITE
    } else {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    };
    let db = Connection::open_with_flags(path, flags)
        .context("无法打开千问办公数据库，请先登录初始化客户端")?;
    db.busy_timeout(Duration::from_secs(5))?;
    let columns: Vec<String> = db
        .prepare("PRAGMA table_info(byok_custom_models)")?
        .query_map([], |r| r.get(1))?
        .collect::<rusqlite::Result<_>>()?;
    if columns != COLUMNS {
        bail!(
            "{}（模型数据库结构已改变）",
            crate::qwenwork_runtime::ADAPTATION_REQUIRED
        )
    }
    Ok(db)
}

fn snapshot(db: &Connection) -> Result<Value> {
    let mut stmt = db.prepare(&format!(
        "SELECT {} FROM byok_custom_models ORDER BY key",
        COLUMNS.join(",")
    ))?;
    let mut rows = stmt.query([])?;
    let mut models = Map::new();
    while let Some(row) = rows.next()? {
        let mut entry = Map::new();
        for (i, name) in COLUMNS.iter().enumerate() {
            let value = match row.get_ref(i)? {
                ValueRef::Null => Value::Null,
                ValueRef::Integer(n) => json!(n),
                ValueRef::Text(s) => json!(std::str::from_utf8(s)?),
                _ => bail!(
                    "{}（模型数据类型已改变）",
                    crate::qwenwork_runtime::ADAPTATION_REQUIRED
                ),
            };
            entry.insert((*name).into(), value);
        }
        let key = entry["key"]
            .as_str()
            .context("千问办公模型标识无效")?
            .to_owned();
        models.insert(key, Value::Object(entry));
    }
    Ok(json!({"models":models}))
}

pub(crate) fn read_snapshot(path: &Path) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(&snapshot(&open(path, false)?)?)?)
}

/// Compare and apply the row delta while holding SQLite's write transaction.
/// Other tables and unchanged providers never receive UPDATE/DELETE statements.
pub(crate) fn write_snapshot(path: &Path, expected: &[u8], after: &[u8]) -> Result<()> {
    require_stopped(path)?;
    let before: Value = serde_json::from_slice(expected)?;
    let after: Value = serde_json::from_slice(after)?;
    let old = before["models"]
        .as_object()
        .context("千问办公恢复快照无效")?;
    let new = after["models"]
        .as_object()
        .context("千问办公恢复快照无效")?;
    let mut db = open(path, true)?;
    // Zero replaced credential cells. Keep the operation a logical row update.
    db.pragma_update(None, "secure_delete", "ON")?;
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if snapshot(&tx)? != before {
        bail!("千问办公模型配置已改变，请重新操作")
    }
    let keys: std::collections::BTreeSet<_> = old.keys().chain(new.keys()).collect();
    for key in keys {
        if old.get(key) == new.get(key) {
            continue;
        }
        if !key.starts_with(PREFIX) {
            bail!("拒绝修改非 Token Market 千问办公模型")
        }
        if let Some(row) = new.get(key) {
            if row["key"] != *key {
                bail!("千问办公模型标识与快照不匹配")
            }
            let values: Vec<rusqlite::types::Value> = COLUMNS
                .iter()
                .map(|column| {
                    Ok(match row.get(*column).context("千问办公模型字段缺失")? {
                        Value::Null => rusqlite::types::Value::Null,
                        Value::String(s) => rusqlite::types::Value::Text(s.clone()),
                        Value::Number(n) => rusqlite::types::Value::Integer(
                            n.as_i64().context("千问办公模型数值无效")?,
                        ),
                        _ => bail!("千问办公模型字段类型无效"),
                    })
                })
                .collect::<Result<_>>()?;
            tx.execute(&format!("INSERT INTO byok_custom_models ({}) VALUES ({}) ON CONFLICT(key) DO UPDATE SET {}",
                COLUMNS.join(","), vec!["?";COLUMNS.len()].join(","),
                COLUMNS.iter().skip(1).map(|c| format!("{c}=excluded.{c}")).collect::<Vec<_>>().join(",")),
                rusqlite::params_from_iter(values))?;
        } else {
            tx.execute("DELETE FROM byok_custom_models WHERE key=?", [key])?;
        }
    }
    tx.commit()?;
    Ok(())
}

pub(crate) fn parse_dotenv(bytes: Option<&[u8]>) -> Result<Value> {
    let bytes = bytes.unwrap_or_default();
    if bytes.len() > 65536 {
        bail!("千问办公 .env 超过客户端大小上限")
    }
    let text = std::str::from_utf8(bytes).context("千问办公 .env 编码无效")?;
    let mut value = Map::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        if let Some((name, content)) = line.split_once('=') {
            let name = name.trim();
            if matches!(name, "ALLOW_CUSTOM_BYOK" | "QW_QODER_WORKER_RUNTIME_PATH")
                && value.contains_key(name)
            {
                bail!("千问办公 .env 存在重复 BYOK 开关，请先合并配置")
            }
            value.insert(name.into(), json!(content.trim()));
        }
    }
    Ok(Value::Object(value))
}

pub(crate) fn render_dotenv(
    bytes: Option<&[u8]>,
    value: &Value,
    touched: &[String],
) -> Result<Vec<u8>> {
    let original = std::str::from_utf8(bytes.unwrap_or_default())?;
    let newline = if original.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut text = original.to_owned();
    for pointer in touched {
        let key = pointer
            .strip_prefix('/')
            .context("千问办公 .env 恢复字段无效")?;
        if !matches!(key, "ALLOW_CUSTOM_BYOK" | "QW_QODER_WORKER_RUNTIME_PATH") {
            bail!("拒绝修改其他千问办公环境变量")
        }
        if value
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(|v| v.contains(['\r', '\n', '\0']))
        {
            bail!("千问办公环境变量内容无效")
        }
        let mut found = false;
        let mut result = String::new();
        for line in text.split_inclusive('\n') {
            if line
                .trim()
                .split_once('=')
                .is_some_and(|(k, _)| k.trim() == key)
            {
                found = true;
                if let Some(v) = value.get(key) {
                    result.push_str(&format!("{key}={}", v.as_str().context("BYOK 开关无效")?));
                    if line.ends_with('\n') {
                        result.push_str(newline);
                    }
                }
            } else {
                result.push_str(line);
            }
        }
        if !found {
            if let Some(v) = value.get(key) {
                if !result.is_empty() && !result.ends_with('\n') {
                    result.push_str(newline);
                }
                result.push_str(&format!(
                    "{key}={}{newline}",
                    v.as_str().context("BYOK 开关无效")?
                ));
            }
        }
        text = result;
    }
    if text.len() > 65536 {
        bail!("千问办公 .env 超过客户端大小上限")
    }
    Ok(text.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_delta_is_atomic_and_never_updates_an_unowned_provider() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("agents.db");
        let db = Connection::open(&path).unwrap();
        db.execute_batch(include_str!("../tests/fixtures/qwenwork-schema.sql"))
            .unwrap();
        db.execute("INSERT INTO byok_custom_models(key,display_name,model) VALUES ('byok-user','User','local')", []).unwrap();
        let bytes = read_snapshot(&path).unwrap();
        let before: Value = serde_json::from_slice(&bytes).unwrap();
        let mut after = before.clone();
        after["models"]["byok-user"]["model"] = json!("must-not-change");
        assert!(write_snapshot(&path, &bytes, &serde_json::to_vec(&after).unwrap()).is_err());
        assert_eq!(read_snapshot(&path).unwrap(), bytes);

        let mut after = before.clone();
        for key in ["byok-tokenmarket-a", "byok-tokenmarket-b"] {
            let mut row = before["models"]["byok-user"].clone();
            row["key"] = json!(key);
            after["models"][key] = row;
        }
        // The second INSERT fails after the first has run. Neither may remain.
        after["models"]["byok-tokenmarket-b"]["model"] = Value::Null;
        assert!(write_snapshot(&path, &bytes, &serde_json::to_vec(&after).unwrap()).is_err());
        assert_eq!(read_snapshot(&path).unwrap(), bytes);

        db.execute("UPDATE byok_custom_models SET display_name='User edit'", [])
            .unwrap();
        assert!(write_snapshot(&path, &bytes, &bytes).is_err());
        assert!(String::from_utf8(read_snapshot(&path).unwrap())
            .unwrap()
            .contains("User edit"));
    }

    #[test]
    fn dotenv_preserves_comments_and_rejects_ambiguous_or_oversized_inputs() {
        let input = b"# settings\r\nOTHER='value'\r\nALLOW_CUSTOM_BYOK=\"false\"\r\n";
        let original = parse_dotenv(Some(input)).unwrap();
        let mut applied = original.clone();
        applied["ALLOW_CUSTOM_BYOK"] = json!("1");
        let touched = ["/ALLOW_CUSTOM_BYOK".into()];
        let bytes = render_dotenv(Some(input), &applied, &touched).unwrap();
        assert_eq!(
            bytes,
            b"# settings\r\nOTHER='value'\r\nALLOW_CUSTOM_BYOK=1\r\n"
        );
        assert_eq!(
            render_dotenv(Some(&bytes), &original, &touched).unwrap(),
            input
        );
        assert!(parse_dotenv(Some(b"ALLOW_CUSTOM_BYOK=0\nALLOW_CUSTOM_BYOK=1\n")).is_err());
        assert!(parse_dotenv(Some(&vec![b'x'; 65537])).is_err());
    }
}
