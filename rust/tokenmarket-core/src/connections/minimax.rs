//! MiniMax Code's tagged 0.7.2 release-package custom provider contract.
use super::provider_config::ProviderDocument;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

pub(super) fn validate(value: &Value) -> Result<()> {
    let Some(providers) = value.get("custom_provider") else {
        return Ok(());
    };
    let providers = providers
        .as_object()
        .context("MiniMax Code custom_provider 必须是对象")?;
    let Some(provider) = providers.get("tokenmarket") else {
        return Ok(());
    };
    let provider = provider
        .as_object()
        .context("MiniMax Code Token Market provider 必须是对象")?;
    for field in ["api", "name", "kind"] {
        if provider.get(field).is_some_and(|v| !v.is_string()) {
            bail!("MiniMax Code provider 字段无效")
        }
    }
    if provider.get("kind").is_some_and(|v| v != "custom") {
        bail!("MiniMax Code Token Market provider 使用其他认证类型；请先确认凭据，原配置未改动")
    }
    if provider.get("enabled").is_some_and(|v| v != true) {
        bail!("MiniMax Code Token Market provider 已禁用或配置无效；请先确认启用方式，原配置未改动")
    }
    if let Some(options) = provider.get("options") {
        let options = options
            .as_object()
            .context("MiniMax Code provider options 必须是对象")?;
        for field in ["apiKey", "baseURL", "authMode"] {
            if options.get(field).is_some_and(|v| !v.is_string()) {
                bail!("MiniMax Code provider 认证字段无效")
            }
        }
        if options.get("authMode").is_some_and(|v| v != "api-key") {
            bail!("MiniMax Code Token Market provider 使用其他认证方式；请先确认凭据，原配置未改动")
        }
        reject_headers(options.get("headers"))?;
    }
    if let Some(models) = provider.get("models") {
        for (id, model) in models
            .as_object()
            .context("MiniMax Code models 必须是对象")?
        {
            if id.trim().is_empty() || !model.is_object() {
                bail!("MiniMax Code 模型配置无效")
            }
            reject_headers(model.get("headers"))?;
            if model.get("name").is_some_and(|v| !v.is_string()) {
                bail!("MiniMax Code 模型名称无效")
            }
            if let Some(limit) = model.get("limit") {
                let limit = limit
                    .as_object()
                    .context("MiniMax Code 模型 limit 必须是对象")?;
                for field in ["context", "output", "input"] {
                    if limit.get(field).is_some_and(|v| {
                        v.as_u64()
                            .is_none_or(|n| n == 0 || n > 9_007_199_254_740_991)
                    }) {
                        bail!("MiniMax Code 模型 limit 必须是正安全整数")
                    }
                }
            }
            if let Some(overrides) = model.get("provider") {
                let overrides = overrides
                    .as_object()
                    .context("MiniMax Code 模型 provider 必须是对象")?;
                if overrides
                    .get("api")
                    .is_some_and(|v| v != "openai-completions")
                    || overrides.contains_key("npm")
                {
                    bail!("MiniMax Code 模型存在其他协议适配器，请先确认后重试")
                }
            }
            if let Some(options) = model.get("options") {
                let options = options
                    .as_object()
                    .context("MiniMax Code 模型 options 必须是对象")?;
                if ["apiKey", "baseURL", "authMode"]
                    .iter()
                    .any(|f| options.contains_key(*f))
                {
                    bail!("MiniMax Code 模型存在独立路由或凭据，请先确认后重试")
                }
                reject_headers(options.get("headers"))?;
            }
        }
    }
    Ok(())
}
fn reject_headers(value: Option<&Value>) -> Result<()> {
    if let Some(value) = value {
        let headers = value
            .as_object()
            .context("MiniMax Code headers 必须是对象")?;
        if headers
            .keys()
            .any(|k| k.eq_ignore_ascii_case("authorization") || k.eq_ignore_ascii_case("x-api-key"))
        {
            bail!("MiniMax Code Token Market 配置了认证请求头，请先确认凭据，原配置未改动")
        }
    }
    Ok(())
}
pub(super) fn credential(value: &Value, gateway: &str) -> Result<Option<String>> {
    let options = &value["custom_provider"]["tokenmarket"]["options"];
    if options["baseURL"]
        .as_str()
        .is_none_or(|v| v.trim_end_matches('/') != format!("{}/v1", gateway.trim_end_matches('/')))
    {
        return Ok(None);
    }
    Ok(options["apiKey"]
        .as_str()
        .filter(|v| !v.trim().is_empty())
        .map(str::to_owned))
}
pub(super) fn apply(
    document: &mut ProviderDocument,
    gateway: &str,
    key: &str,
    models: &[Value],
) -> Result<()> {
    validate(&document.value)?;
    for (field, value) in [
        ("api", json!("openai-completions")),
        ("name", json!("Token Market")),
        ("kind", json!("custom")),
        ("enabled", json!(true)),
    ] {
        document.set(&["custom_provider", "tokenmarket", field], value)?;
    }
    for (field, value) in [
        (
            "baseURL",
            json!(format!("{}/v1", gateway.trim_end_matches('/'))),
        ),
        ("apiKey", json!(key)),
        ("authMode", json!("api-key")),
    ] {
        document.set(&["custom_provider", "tokenmarket", "options", field], value)?;
    }
    for model in models {
        let id = model["id"].as_str().context("模型目录缺少 ID")?;
        document.set(
            &["custom_provider", "tokenmarket", "models", id, "name"],
            json!(model["display_name"]
                .as_str()
                .filter(|v| !v.is_empty())
                .unwrap_or(id)),
        )?;
        for (source, field) in [
            ("context_window_tokens", "context"),
            ("max_output_tokens", "output"),
        ] {
            if let Some(tokens) = model[source]
                .as_u64()
                .filter(|v| *v > 0 && *v <= 9_007_199_254_740_991)
            {
                document.set(
                    &[
                        "custom_provider",
                        "tokenmarket",
                        "models",
                        id,
                        "limit",
                        field,
                    ],
                    json!(tokens),
                )?;
            }
        }
    }
    Ok(())
}
