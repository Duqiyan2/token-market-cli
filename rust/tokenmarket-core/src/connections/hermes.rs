use super::provider_config::ProviderDocument;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::{fs, path::Path};

pub(super) fn validate(value: &Value, path: &Path) -> Result<()> {
    validate_auth_store(path)?;
    if let Some(legacy) = value.get("custom_providers").and_then(Value::as_array) {
        if legacy.iter().any(|entry| {
            entry["name"].as_str().is_some_and(|name| {
                name.chars()
                    .filter(char::is_ascii_alphanumeric)
                    .flat_map(char::to_lowercase)
                    .collect::<String>()
                    == "tokenmarket"
            })
        }) {
            bail!("Hermes custom_providers 中已有同名 Token Market provider，请先明确生效配置")
        }
    }
    let Some(providers) = value.get("providers") else {
        return Ok(());
    };
    let providers = providers
        .as_object()
        .context("Hermes providers 必须是 YAML 字典")?;
    let Some(provider) = providers.get("tokenmarket") else {
        return Ok(());
    };
    let provider = provider
        .as_object()
        .context("Hermes Token Market provider 必须是 YAML 字典")?;
    for field in [
        "name",
        "api",
        "url",
        "base_url",
        "api_key",
        "apiKey",
        "transport",
        "api_mode",
        "apiMode",
    ] {
        if provider
            .get(field)
            .is_some_and(|value| !value.is_string() && !value.is_null())
        {
            bail!("Hermes Token Market provider 的 {field} 字段无效")
        }
    }
    for field in ["key_cmd", "key_env", "api_key_env", "keyEnv", "apiKeyEnv"] {
        if provider.get(field).is_some_and(|value| {
            !value.is_null() && value.as_str().is_none_or(|value| !value.trim().is_empty())
        }) {
            bail!("Hermes Token Market provider 使用 {field} 凭据来源，请先确认并移除冲突；未执行凭据助手")
        }
    }
    if provider.get("enabled").is_some_and(|value| {
        value.is_null()
            || value == &json!(0)
            || value == &json!(false)
            || value.as_str().is_some_and(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "false" | "0" | "no" | "off"
                )
            })
    }) {
        bail!("Hermes Token Market provider 已停用，请先确认 enabled 配置")
    }
    if provider
        .get("extra_headers")
        .and_then(Value::as_object)
        .is_some_and(|headers| {
            headers
                .keys()
                .any(|key| key.eq_ignore_ascii_case("authorization"))
        })
    {
        bail!("Hermes Token Market provider 设置了 Authorization 请求头，请先确认凭据方式")
    }
    if let Some(models) = provider.get("models") {
        if !models.is_object() {
            bail!("Hermes Token Market models 必须是模型字典，请先确认配置")
        }
    }
    if value
        .pointer("/model/provider")
        .and_then(Value::as_str)
        .is_some_and(|provider| {
            matches!(
                provider.to_ascii_lowercase().as_str(),
                "custom:tokenmarket" | "custom:token-market"
            )
        })
    {
        for field in ["base_url", "api_key", "key_env", "api_key_env", "key_cmd"] {
            if value["model"].get(field).is_some_and(|value| {
                !value.is_null() && value.as_str().is_none_or(|value| !value.trim().is_empty())
            }) {
                bail!("Hermes model.{field} 会覆盖 Token Market provider，请先确认实际路由与凭据")
            }
        }
    }
    Ok(())
}

pub(super) fn credential(value: &Value, gateway: &str) -> Result<Option<String>> {
    let provider = &value["providers"]["tokenmarket"];
    let base = provider["api"]
        .as_str()
        .or_else(|| provider["url"].as_str())
        .or_else(|| provider["base_url"].as_str());
    if base.is_some_and(|base| {
        base.trim_end_matches('/') == format!("{}/v1", gateway.trim_end_matches('/'))
    }) {
        Ok(provider["api_key"]
            .as_str()
            .or_else(|| provider["apiKey"].as_str())
            .filter(|key| !key.is_empty())
            .map(str::to_owned))
    } else {
        Ok(None)
    }
}

pub(super) fn apply(
    document: &mut ProviderDocument,
    gateway: &str,
    key: &str,
    models: &[Value],
) -> Result<()> {
    for (field, applied) in [
        ("name", json!("Token Market")),
        (
            "api",
            json!(format!("{}/v1", gateway.trim_end_matches('/'))),
        ),
        ("api_key", json!(key)),
        ("transport", json!("chat_completions")),
    ] {
        document.set(&["providers", "tokenmarket", field], applied)?;
    }
    for field in ["api_mode", "apiMode"] {
        if document.value["providers"]["tokenmarket"]
            .get(field)
            .is_some()
        {
            document.set(
                &["providers", "tokenmarket", field],
                json!("chat_completions"),
            )?;
        }
    }
    for model in models {
        let id = model["id"].as_str().context("模型目录缺少模型 ID")?;
        // Hermes treats null context_length as unspecified and auto-detects it.
        // Owning this leaf registers metadata-free rows without owning their maps.
        let context = model["context_window_tokens"]
            .as_u64()
            .filter(|context| *context > 0)
            .map_or(Value::Null, Value::from);
        document.set(
            &["providers", "tokenmarket", "models", id, "context_length"],
            context,
        )?;
    }
    Ok(())
}

// Hermes resolves saved pools before inline api_key. Do not modify auth.json or
// execute any auth helper; let users resolve an existing account explicitly.
fn validate_auth_store(config: &Path) -> Result<()> {
    let home = config.parent().context("Hermes 配置目录无效")?;
    let mut stores = vec![home.join("auth.json")];
    if home
        .parent()
        .is_some_and(|parent| parent.file_name().is_some_and(|name| name == "profiles"))
    {
        if let Some(root) = home.parent().and_then(Path::parent) {
            stores.push(root.join("auth.json"));
        }
    }
    for path in stores {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => bail!("无法读取 Hermes auth.json，请先检查认证配置"),
        };
        let auth: Value =
            serde_json::from_slice(&bytes).context("Hermes auth.json 格式损坏，请先修复")?;
        let auth = auth.as_object().context("Hermes auth.json 必须是对象")?;
        if let Some(pool) = auth.get("credential_pool") {
            let pool = pool
                .as_object()
                .context("Hermes credential_pool 必须是对象")?;
            for key in ["tokenmarket", "custom:token-market"] {
                let Some(rows) = pool.get(key) else { continue };
                let rows = rows
                    .as_array()
                    .context("Hermes Token Market 认证池格式无效")?;
                // Config-seeded legacy rows are refreshed by Hermes from this
                // provider's current inline key. Manual/env/OAuth rows may win.
                if rows.iter().any(|row| {
                    key == "tokenmarket" || row["source"].as_str() != Some("config:Token Market")
                }) {
                    bail!("Hermes auth.json 中已有 Token Market 认证池，可能覆盖配置的 Key；请先确认并移除冲突")
                }
            }
        }
    }
    Ok(())
}
