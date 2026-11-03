//! DuMate personal legacy_custom route. Persist preferences, never its generated opencode.json.
use super::provider_config::ProviderDocument;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

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
        .filter(|n| *n > 1 && *n <= i64::MAX as u64)?;
    let output = model["max_output_tokens"]
        .as_u64()
        .filter(|n| *n > 0 && *n <= i64::MAX as u64)?;
    Some((context, output.min(context - 1)))
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

pub(super) fn credential(value: &Value, gateway: &str) -> Result<Option<String>> {
    let provider = &value["customLLMProvider"];
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
