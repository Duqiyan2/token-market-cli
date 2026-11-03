use super::provider_config::ProviderDocument;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

pub(super) fn validate(value: &Value) -> Result<()> {
    if value
        .pointer("/models/extra_headers")
        .and_then(Value::as_object)
        .is_some_and(|headers| {
            headers
                .keys()
                .any(|key| key.eq_ignore_ascii_case("authorization"))
        })
    {
        bail!("Grok Build 全局配置了 Authorization 请求头，请先确认认证方式")
    }
    let Some(models) = value.get("model") else {
        return Ok(());
    };
    let models = models
        .as_object()
        .context("Grok Build model 必须是 TOML 表")?;
    for (name, model) in models {
        if !name.starts_with("tokenmarket/") {
            continue;
        }
        let model = model
            .as_object()
            .context("Grok Build 模型配置必须是 TOML 表")?;
        for field in ["model", "base_url", "api_key", "api_backend", "name"] {
            if model.get(field).is_some_and(|v| !v.is_string()) {
                bail!("Grok Build Token Market 模型配置字段无效")
            }
        }
        if model.get("auth_provider").is_some() {
            bail!("Grok Build Token Market 模型使用其他凭据助手，请先确认认证方式")
        }
        for field in ["extra_headers", "env_http_headers"] {
            if model
                .get(field)
                .and_then(Value::as_object)
                .is_some_and(|headers| {
                    headers
                        .keys()
                        .any(|key| key.eq_ignore_ascii_case("authorization"))
                })
            {
                bail!("Grok Build Token Market 模型配置了 Authorization 请求头，请先确认认证方式")
            }
        }
    }
    Ok(())
}

pub(super) fn credential(value: &Value, gateway: &str) -> Result<Option<String>> {
    let mut credential: Option<String> = None;
    for (name, model) in value
        .get("model")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        if !name.starts_with("tokenmarket/")
            || model["base_url"].as_str().is_none_or(|base| {
                base.trim_end_matches('/') != format!("{}/v1", gateway.trim_end_matches('/'))
            })
        {
            continue;
        }
        let Some(key) = model["api_key"].as_str().filter(|key| !key.is_empty()) else {
            continue;
        };
        if credential.as_deref().is_some_and(|old| old != key) {
            bail!("Grok Build 存在多份 Token Market 凭据，请先确认实际连接")
        }
        credential = Some(key.into());
    }
    Ok(credential)
}

pub(super) fn apply(
    document: &mut ProviderDocument,
    gateway: &str,
    key: &str,
    models: &[Value],
) -> Result<()> {
    for model in models {
        let id = model["id"].as_str().context("模型目录缺少模型 ID")?;
        let alias = format!("tokenmarket/{id}");
        for (field, applied) in [
            ("model", json!(id)),
            (
                "name",
                json!(model["display_name"]
                    .as_str()
                    .filter(|name| !name.is_empty())
                    .unwrap_or(id)),
            ),
            (
                "base_url",
                json!(format!("{}/v1", gateway.trim_end_matches('/'))),
            ),
            ("api_key", json!(key)),
            ("api_backend", json!("chat_completions")),
        ] {
            document.set(&["model", &alias, field], applied)?;
        }
        for (source, field) in [
            ("context_window_tokens", "context_window"),
            ("max_output_tokens", "max_completion_tokens"),
        ] {
            if let Some(tokens) = model[source].as_u64().filter(|tokens| *tokens > 0) {
                document.set(&["model", &alias, field], json!(tokens))?;
            }
        }
    }
    Ok(())
}
