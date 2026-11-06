//! DuMate cold-start takeover. Persist preferences and adapter; never generated opencode.json.
use super::provider_config::ProviderDocument;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;

pub(super) fn validate_unified_replacement(record: &super::ConnectionRecord) -> Result<()> {
    for receipt in &record.receipts {
        if receipt
            .path
            .file_name()
            .is_some_and(|name| name == crate::dumate::unified::PLUGIN_NAME)
        {
            let current = super::configuration_io::read(&receipt.path)?;
            let expected = receipt
                .fields
                .iter()
                .find(|f| f.pointer == "/$content")
                .and_then(|f| f.applied.as_str());
            if current.as_deref().map(std::str::from_utf8).transpose()? != expected {
                bail!("统一接管扩展已被修改，请先取消接入后重新接入；修改内容会保留为停用备份")
            }
        }
    }
    Ok(())
}

/// Cancel our adapter as an asset: preserve user amendments in a disabled backup,
/// rather than leaving executable code behind after its policy is removed.
pub(super) fn prepare_unified_disconnect(
    record: &super::ConnectionRecord,
) -> Result<(super::ConnectionRecord, Vec<super::restore::Change>)> {
    let mut record = record.clone();
    let mut extra = Vec::new();
    let mut keep = Vec::new();
    for mut receipt in std::mem::take(&mut record.receipts) {
        if !receipt
            .path
            .file_name()
            .is_some_and(|name| name == crate::dumate::unified::PLUGIN_NAME)
        {
            keep.push(receipt);
            continue;
        }
        let current = super::configuration_io::read(&receipt.path)?;
        if let Some(bytes) = current.as_ref() {
            let text = std::str::from_utf8(bytes)?;
            if !text.starts_with(crate::dumate::unified::MARKER) {
                continue;
            } // The replacement belongs to its author.
            let applied = receipt
                .fields
                .iter()
                .find(|f| f.pointer == "/$content")
                .and_then(|f| f.applied.as_str());
            if Some(text) != applied {
                let account = receipt
                    .path
                    .parent()
                    .and_then(Path::parent)
                    .and_then(Path::parent)
                    .and_then(Path::parent)
                    .context("统一接管路径无效")?;
                let state = account.join("state/tokenmarket-dumate");
                for directory in [account.join("state"), state.clone()] {
                    if std::fs::symlink_metadata(directory)
                        .is_ok_and(|m| m.file_type().is_symlink())
                    {
                        bail!("扩展备份目录为符号链接，未执行取消接入")
                    }
                }
                let backup = state.join(format!(
                    "retired-unified-{:x}.js.disabled",
                    Sha256::digest(bytes)
                ));
                let before = super::configuration_io::read(&backup)?;
                if before.as_ref().is_some_and(|b| b != bytes) {
                    bail!("扩展停用备份存在冲突")
                }
                extra.push(super::restore::Change {
                    path: backup,
                    before,
                    after: Some(bytes.clone()),
                });
            }
            for field in &mut receipt.fields {
                if field.pointer == "/$content" {
                    field.applied = json!(text);
                    field.before = None;
                }
            }
        }
        receipt.existed = false;
        receipt.backup_path = None;
        keep.push(receipt);
    }
    record.receipts = keep;
    Ok((record, extra))
}

pub(super) fn unified_files(
    path: &Path,
    preference: &[u8],
) -> Result<Vec<super::configuration::PreparedFile>> {
    use super::{configuration::PreparedFile, FieldReceipt};
    let root = path
        .parent()
        .and_then(Path::parent)
        .context("搭子账号配置目录无效")?;
    let plugin = root
        .join("opencode/plugins")
        .join(crate::dumate::unified::PLUGIN_NAME);
    let policy = root
        .join("opencode")
        .join(crate::dumate::unified::POLICY_NAME);
    for directory in [
        root.to_path_buf(),
        root.join("opencode"),
        root.join("opencode/plugins"),
    ] {
        if std::fs::symlink_metadata(directory).is_ok_and(|m| m.file_type().is_symlink()) {
            bail!("搭子扩展目录为符号链接，请先确认实际路径")
        }
    }
    if let Ok(entries) = std::fs::read_dir(root.join("opencode/plugins")) {
        for entry in entries {
            let entry = entry?;
            if entry.path() != plugin
                && entry
                    .path()
                    .extension()
                    .is_some_and(|e| e == "js" || e == "ts" || e == "mjs")
            {
                bail!("搭子存在其他引擎扩展，暂不能保证统一接管；请先处理扩展兼容性，原配置已保留")
            }
        }
    }
    let before_plugin = super::configuration_io::read(&plugin)?;
    let before_text = before_plugin
        .as_deref()
        .map(std::str::from_utf8)
        .transpose()?;
    if before_text.is_some_and(|s| !s.starts_with(crate::dumate::unified::MARKER)) {
        bail!("同名统一接管扩展属于其他内容，已保留原文件")
    }
    let before_policy = super::configuration_io::read(&policy)?;
    let mut document: Value = match before_policy.as_deref() {
        Some(b) => serde_json::from_slice(b)?,
        None => json!({}),
    };
    if !document.is_object()
        || document
            .get("unified")
            .is_some_and(|v| v["owner"] != "cn.tokenmarket")
    {
        bail!("统一接管策略文件存在冲突，原文件已保留")
    }
    let mut provider = document["unified"]["provider"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    let selected = serde_json::from_slice::<Value>(preference)?["customLLMProvider"].clone();
    provider.extend(selected.as_object().context("文本模型配置无效")?.clone());
    let provider = Value::Object(provider);
    let fields = ["baseURL", "apiKey", "model", "contextLimit", "outputLimit"]
        .iter()
        .map(|key| provider[*key].clone())
        .collect::<Vec<_>>();
    let fingerprint = format!("{:x}", Sha256::digest(serde_json::to_vec(&fields)?));
    let next = json!({"protocol":3,"owner":"cn.tokenmarket","providerFingerprint":fingerprint,"baseURL":provider["baseURL"],"model":provider["model"],"provider":provider});
    let prior = document
        .as_object_mut()
        .unwrap()
        .insert("unified".into(), next.clone());
    Ok(vec![
        PreparedFile {
            path: plugin,
            before: before_plugin.clone(),
            bytes: crate::dumate::unified::SOURCE.as_bytes().to_vec(),
            fields: vec![FieldReceipt {
                pointer: "/$content".into(),
                before: before_text.map(|s| json!(s)),
                applied: json!(crate::dumate::unified::SOURCE),
                retired: false,
            }],
            created_containers: vec![],
        },
        PreparedFile {
            path: policy,
            before: before_policy,
            bytes: serde_json::to_vec_pretty(&document)?,
            fields: vec![FieldReceipt {
                pointer: "/unified".into(),
                before: prior,
                applied: next,
                retired: false,
            }],
            created_containers: vec![],
        },
    ])
}

/// The native global override also captures model-mm. Keep it off so media stays
/// on the official route; the private policy carries the text credential instead.
pub(super) fn preserve_official_media(
    prepared: &mut super::configuration::PreparedConfiguration,
) -> Result<()> {
    let mut value: Value = serde_json::from_slice(&prepared.bytes)?;
    value["customLLMProvider"] = Value::Null;
    prepared.bytes = serde_json::to_vec_pretty(&value)?;
    for field in &mut prepared.fields {
        if field.pointer == "/customLLMProvider" {
            field.applied = Value::Null;
        }
    }
    Ok(())
}

pub(super) fn validate(value: &Value) -> Result<()> {
    if let Some(provider) = value.get("customLLMProvider").filter(|v| !v.is_null()) {
        let provider = provider
            .as_object()
            .context("DuMate customLLMProvider 必须是对象")?;
        for field in ["baseURL", "apiKey", "model"] {
            if provider.get(field).is_some_and(|v| !v.is_string()) {
                bail!("DuMate 自定义模型字段无效，请先修复配置")
            }
        }
        for field in ["contextLimit", "outputLimit"] {
            if provider
                .get(field)
                .is_some_and(|v| v.as_i64().is_none_or(|n| n <= 0))
            {
                bail!("DuMate 自定义模型上下文/输出限制必须是正整数")
            }
        }
    }
    Ok(())
}

fn limits(model: &Value) -> Option<(u64, u64)> {
    let context = model["context_window_tokens"]
        .as_u64()
        .filter(|n| *n > 1 && *n <= 9_007_199_254_740_991)?;
    let output = model["max_output_tokens"]
        .as_u64()
        .filter(|n| *n > 0 && *n <= 9_007_199_254_740_991)?;
    Some((context, output.min(context - 1)))
}

pub(super) fn eligible(models: &[Value]) -> Result<Vec<Value>> {
    let models: Vec<_> = models
        .iter()
        .filter(|row| limits(row).is_some())
        .cloned()
        .collect();
    if models.is_empty() {
        bail!("DuMate 个人版需要一个具有有效上下文和输出限制的文本模型；原配置已保留")
    }
    Ok(models)
}

pub(super) fn select(value: &Value, models: &[Value]) -> Result<Vec<Value>> {
    let existing = value["customLLMProvider"]["model"].as_str();
    let default = option_env!("TOKEN_MARKET_BUILD_DESKTOP_DEFAULT_MODEL").unwrap_or("glm-5.3");
    let eligible = |row: &&Value| limits(row).is_some();
    let selected = models.iter().filter(eligible).find(|row| row["id"] == default)
        .or_else(|| models.iter().filter(eligible).find(|row| row["id"].as_str() == existing))
        .or_else(|| models.iter().find(eligible))
        .context("DuMate 个人版需要一个具有 context_window_tokens 和 max_output_tokens 正整数元数据的文本模型；原配置已保留")?;
    Ok(vec![selected.clone()])
}

pub(super) fn credential(value: &Value, gateway: &str, path: &Path) -> Result<Option<String>> {
    let policy = path
        .parent()
        .and_then(Path::parent)
        .map(|root| {
            root.join("opencode")
                .join(crate::dumate::unified::POLICY_NAME)
        })
        .map(|file| super::configuration_io::read(&file))
        .transpose()?
        .flatten()
        .map(|bytes| serde_json::from_slice::<Value>(&bytes))
        .transpose()?;
    let provider = if let Some(policy) = policy
        .as_ref()
        .filter(|v| v["unified"]["owner"] == "cn.tokenmarket" && v["unified"]["protocol"] == 3)
    {
        &policy["unified"]["provider"]
    } else {
        &value["customLLMProvider"]
    };
    Ok(provider["baseURL"]
        .as_str()
        .filter(|url| url.trim_end_matches('/') == format!("{}/v1", gateway.trim_end_matches('/')))
        .and_then(|_| provider["apiKey"].as_str())
        .filter(|key| !key.is_empty())
        .map(str::to_owned))
}

pub(super) fn apply(
    document: &mut ProviderDocument,
    gateway: &str,
    key: &str,
    models: &[Value],
) -> Result<()> {
    validate(&document.value)?;
    if models.len() != 1 {
        bail!("DuMate 个人版一次只能配置一个模型")
    }
    let model = &models[0];
    let id = model["id"].as_str().context("DuMate 模型缺少 ID")?;
    let (context, output) = limits(model).context("DuMate 模型缺少有效上下文/输出元数据")?;
    let mut provider = document.value["customLLMProvider"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    for (name, value) in [
        (
            "baseURL",
            json!(format!("{}/v1", gateway.trim_end_matches('/'))),
        ),
        ("apiKey", json!(key)),
        ("model", json!(id)),
        ("contextLimit", json!(context)),
        ("outputLimit", json!(output)),
    ] {
        provider.insert(name.into(), value);
    }
    // One upstream preference is the ownership unit. Null/absent original preferences
    // and user-edited providers round-trip through the shared conflict/restore engine.
    document.set(&["customLLMProvider"], Value::Object(provider))
}
