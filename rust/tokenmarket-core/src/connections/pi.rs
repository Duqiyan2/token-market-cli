//! Pi v1.4.0 custom providers; auth.json has priority over models.json.
use super::provider_config::ProviderDocument;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::path::Path;

pub(super) fn validate(value: &Value, path: &Path) -> Result<()> {
    if path.extension().and_then(|s| s.to_str()) != Some("json") {
        bail!("Pi 模型配置必须使用 JSON 文件")
    }
    let auth_path = path.with_file_name("auth.json");
    if std::fs::symlink_metadata(&auth_path).is_ok_and(|m| m.file_type().is_symlink()) {
        bail!("Pi auth.json 是符号链接，请先确认实际凭据路径")
    }
    match std::fs::read(&auth_path) {
        Ok(bytes) => {
            let auth: Value =
                serde_json::from_slice(&bytes).context("Pi auth.json 不是有效 JSON，请先修复")?;
            let auth = auth.as_object().context("Pi auth.json 必须是对象")?;
            // Pi validates all stored credentials before resolving any provider.
            for credential in auth.values() {
                let valid = match credential["type"].as_str() {
                    Some("api_key") => {
                        credential.get("key").is_none_or(Value::is_string)
                            && credential.get("env").is_none_or(|env| {
                                env.as_object()
                                    .is_some_and(|env| env.values().all(Value::is_string))
                            })
                    }
                    Some("oauth") => {
                        credential["access"].is_string()
                            && credential["refresh"].is_string()
                            && credential["expires"].is_number()
                    }
                    _ => false,
                };
                if !valid {
                    bail!("Pi auth.json 凭据结构无效，请先在 Pi 中修复")
                }
            }
            if auth.contains_key("tokenmarket") {
                bail!("Pi auth.json 中的 tokenmarket 凭据优先于 models.json；请先在 Pi 中 /logout 该 provider 后重试，原凭据未改动")
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => bail!("无法读取 Pi auth.json，未接入"),
    }
    let Some(providers) = value.get("providers") else {
        return Ok(());
    };
    let providers = providers.as_object().context("Pi providers 必须是对象")?;
    let Some(provider) = providers.get("tokenmarket") else {
        return Ok(());
    };
    let provider = provider
        .as_object()
        .context("Pi Token Market provider 必须是对象")?;
    for field in ["baseUrl", "api", "apiKey"] {
        if provider.get(field).is_some_and(|v| !v.is_string()) {
            bail!("Pi Token Market provider 字段无效")
        }
    }
    if provider.contains_key("oauth") {
        bail!("Pi Token Market provider 已配置 OAuth，请先确认认证方式")
    }
    reject_auth_overrides(&json!(provider))?;
    if let Some(rows) = provider.get("models") {
        let rows = rows.as_array().context("Pi models 必须是数组")?;
        let mut seen = std::collections::HashSet::new();
        for row in rows {
            let id = row["id"]
                .as_str()
                .filter(|v| !v.is_empty())
                .context("Pi 模型必须包含有效 ID")?;
            if !row.is_object() || !seen.insert(id) {
                bail!("Pi 模型列表存在重复或无效 ID")
            }
            reject_auth_overrides(row)?;
        }
    }
    Ok(())
}
fn reject_auth_overrides(value: &Value) -> Result<()> {
    // Model-level overrides would bypass the managed provider routing/credentials.
    if value
        .get("headers")
        .and_then(Value::as_object)
        .is_some_and(|headers| {
            headers.keys().any(|k| {
                k.eq_ignore_ascii_case("authorization") || k.eq_ignore_ascii_case("x-api-key")
            })
        })
    {
        bail!("Pi Token Market 配置了认证请求头，请先确认认证方式")
    }
    Ok(())
}

pub(super) fn credential(value: &Value, gateway: &str) -> Result<Option<String>> {
    let provider = &value["providers"]["tokenmarket"];
    if provider["baseUrl"]
        .as_str()
        .is_none_or(|v| v.trim_end_matches('/') != format!("{}/v1", gateway.trim_end_matches('/')))
    {
        return Ok(None);
    }
    // Never execute command/env syntax while recovering a secret.
    Ok(provider["apiKey"]
        .as_str()
        .and_then(literal_secret)
        .filter(|key| !key.is_empty()))
}
fn literal_secret(value: &str) -> Option<String> {
    if value.starts_with('!') {
        return None;
    }
    let mut result = String::new();
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c == '$' {
            match chars.next() {
                Some('$') => result.push('$'),
                Some('!') => result.push('!'),
                _ => return None,
            }
        } else {
            result.push(c);
        }
    }
    Some(result)
}
pub(super) fn apply(
    document: &mut ProviderDocument,
    gateway: &str,
    key: &str,
    models: &[Value],
) -> Result<()> {
    let mut literal = key.replace('$', "$$");
    if literal.starts_with('!') {
        literal = format!("$!{}", &literal[1..])
    }
    for (field, value) in [
        (
            "baseUrl",
            json!(format!("{}/v1", gateway.trim_end_matches('/'))),
        ),
        ("api", json!("openai-completions")),
        ("apiKey", json!(literal)),
        ("authHeader", json!(true)),
    ] {
        document.set(&["providers", "tokenmarket", field], value)?;
    }
    for model in models {
        let id = model["id"].as_str().context("模型目录缺少 ID")?;
        let mut metadata = json!({"api":"openai-completions","baseUrl":format!("{}/v1",gateway.trim_end_matches('/')),"name":model["display_name"].as_str().filter(|v|!v.is_empty()).unwrap_or(id)});
        metadata.as_object_mut().unwrap().extend(
            crate::model_projection::settings(crate::ToolId::Pi, model)
                .as_object()
                .unwrap()
                .clone(),
        );
        document.set_model_fields(
            &["providers", "tokenmarket", "models"],
            id,
            metadata.as_object().unwrap(),
        )?;
    }
    Ok(())
}
