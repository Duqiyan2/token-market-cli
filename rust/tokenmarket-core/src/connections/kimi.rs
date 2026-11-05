//! Stable TypeScript Kimi Code 2.4.1, not the retired Python/uv contract.
use super::provider_config::ProviderDocument;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::path::Path;
pub(super) fn validate(value: &Value, path: &Path) -> Result<()> {
    if path.extension().and_then(|s| s.to_str()) != Some("toml") {
        bail!("Kimi Code 配置必须使用 TOML 文件")
    }
    if std::env::var("KIMI_MODEL_NAME")
        .ok()
        .is_some_and(|v| !v.trim().is_empty())
    {
        bail!("KIMI_MODEL_NAME 会覆盖 Kimi Code 配置模型，请先移除临时模型覆盖后接入")
    }
    if std::env::var("KIMI_CODE_CUSTOM_HEADERS")
        .ok()
        .is_some_and(|headers| {
            headers
                .lines()
                .filter_map(|line| line.split_once(':'))
                .any(|(name, _)| auth_header(name.trim()))
        })
    {
        bail!("KIMI_CODE_CUSTOM_HEADERS 配置了认证请求头，请先确认认证方式")
    }
    if value.get("default_model").is_some_and(|v| !v.is_string())
        || value.get("thinking").is_some_and(|v| !v.is_object())
    {
        bail!("Kimi Code default_model/thinking 配置结构无效，请先修复")
    }
    if let Some(providers) = value.get("providers") {
        let providers = providers
            .as_object()
            .context("Kimi Code providers 必须是 TOML 表")?;
        if let Some(provider) = providers.get("tokenmarket") {
            let provider = provider
                .as_object()
                .context("Kimi Code Token Market provider 必须是 TOML 表")?;
            for field in ["type", "base_url", "api_key"] {
                if provider.get(field).is_some_and(|v| !v.is_string()) {
                    bail!("Kimi Code Token Market provider 字段无效")
                }
            }
            if ["api_key_env", "oauth", "source"]
                .iter()
                .any(|field| provider.contains_key(*field))
            {
                bail!("Kimi Code Token Market provider 已配置 api_key_env/oauth/动态来源，请先确认凭据与配置来源")
            }
            if let Some(headers) = provider.get("custom_headers") {
                let headers = headers
                    .as_object()
                    .context("Kimi Code custom_headers 必须是 TOML 表")?;
                if headers.keys().any(|key| auth_header(key)) {
                    bail!("Kimi Code Token Market 配置了认证请求头，请先确认认证方式")
                }
            }
        }
    }
    if let Some(models) = value.get("models") {
        for (alias, model) in models
            .as_object()
            .context("Kimi Code models 必须是 TOML 表")?
        {
            if !alias.starts_with("tokenmarket/") {
                continue;
            }
            let model = model
                .as_object()
                .context("Kimi Code Token Market 模型必须是 TOML 表")?;
            for field in ["provider", "model", "display_name"] {
                if model.get(field).is_some_and(|v| !v.is_string()) {
                    bail!("Kimi Code 模型字段无效")
                }
            }
            if ["protocol", "base_url"]
                .iter()
                .any(|field| model.contains_key(*field))
            {
                bail!("Kimi Code Token Market 模型设置了独立请求路由，请先确认模型级配置")
            }
            if let Some(capabilities) = model.get("capabilities") {
                if !capabilities
                    .as_array()
                    .is_some_and(|v| v.iter().all(Value::is_string))
                {
                    bail!("Kimi Code capabilities 必须是字符串数组")
                }
            }
        }
    }
    Ok(())
}
fn auth_header(name: &str) -> bool {
    name.eq_ignore_ascii_case("authorization") || name.eq_ignore_ascii_case("x-api-key")
}
pub(super) fn credential(value: &Value, gateway: &str) -> Result<Option<String>> {
    let provider = &value["providers"]["tokenmarket"];
    Ok(provider["base_url"]
        .as_str()
        .filter(|v| v.trim_end_matches('/') == format!("{}/v1", gateway.trim_end_matches('/')))
        .and_then(|_| provider["api_key"].as_str())
        .filter(|v| !v.is_empty())
        .map(str::to_owned))
}
pub(super) fn apply(
    document: &mut ProviderDocument,
    gateway: &str,
    key: &str,
    models: &[Value],
) -> Result<()> {
    // Validate every required context value before preparing any managed field.
    let mut windows = vec![];
    for model in models {
        let id = model["id"].as_str().context("模型目录缺少 ID")?;
        let alias = format!("tokenmarket/{id}");
        let existing = &document.value["models"][&alias];
        let same_model = existing["provider"] == "tokenmarket" && existing["model"] == id;
        let context=model["context_window_tokens"].as_u64().filter(|n|*n>0&&*n<=i64::MAX as u64).or_else(||same_model.then(||existing["max_context_size"].as_u64()).flatten().filter(|n|*n>0&&*n<=i64::MAX as u64)).with_context(||format!("Kimi Code 模型 {id} 缺少必需的正整数 max_context_size（context_window_tokens），原配置已保留；已保存 Key 可用于重试"))?;
        windows.push(context);
    }
    for (field, value) in [
        ("type", json!("openai")),
        (
            "base_url",
            json!(format!("{}/v1", gateway.trim_end_matches('/'))),
        ),
        ("api_key", json!(key)),
    ] {
        document.set(&["providers", "tokenmarket", field], value)?;
    }
    for (model, context) in models.iter().zip(windows) {
        let id = model["id"].as_str().unwrap();
        let alias = format!("tokenmarket/{id}");
        let mut capabilities = document.value["models"][&alias]["capabilities"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        // Token Market's TEXT_GENERATION catalogue is the eligible Agent-model contract.
        // This declaration does not prove the installed Kimi runtime's tool-call behavior.
        if !capabilities.iter().any(|cap| cap == "tool_use") {
            capabilities.push(json!("tool_use"));
        }
        if let Some(supported) = model["supports_reasoning"].as_bool() {
            capabilities.retain(|cap| cap != "thinking" && cap != "always_thinking");
            if supported {
                capabilities.push(json!("thinking"));
                if crate::model_projection::thinking_mode(model) == "always" {
                    capabilities.push(json!("always_thinking"));
                }
            }
        }
        if let Some(input) = model["input_modalities"]
            .as_array()
            .filter(|a| !a.is_empty())
        {
            for (modality, tag) in [
                ("image", "image_in"),
                ("video", "video_in"),
                ("audio", "audio_in"),
            ] {
                capabilities.retain(|cap| cap != tag);
                if input.iter().any(|v| v == modality) {
                    capabilities.push(json!(tag));
                }
            }
        }
        if let Some(input) = model["max_input_tokens"]
            .as_u64()
            .filter(|n| *n > 0 && *n <= i64::MAX as u64)
        {
            document.set(
                &["models", &alias, "max_input_size"],
                json!(input.min(context)),
            )?;
        }
        if model["reasoning_efforts"].is_object()
            && crate::model_catalog::efforts(&model["reasoning_efforts"])
        {
            let levels = crate::model_projection::effort_levels(model);
            document.set(
                &["models", &alias, "support_efforts"],
                json!(levels
                    .iter()
                    .map(|l| model["reasoning_efforts"][*l].clone())
                    .collect::<Vec<_>>()),
            )?;
            if let Some(default) = crate::model_projection::default_effort(model) {
                document.set(&["models", &alias, "default_effort"], json!(default))?;
            }
            if let Some(off) = model["reasoning_efforts"]["off"].as_str() {
                document.set(&["models", &alias, "off_effort"], json!(off))?;
            }
        }
        for (field, value) in [
            ("provider", json!("tokenmarket")),
            ("model", json!(id)),
            (
                "display_name",
                json!(model["display_name"]
                    .as_str()
                    .filter(|v| !v.is_empty())
                    .unwrap_or(id)),
            ),
            ("max_context_size", json!(context)),
            ("capabilities", json!(capabilities)),
        ] {
            document.set(&["models", &alias, field], value)?;
        }
    }
    Ok(())
}
