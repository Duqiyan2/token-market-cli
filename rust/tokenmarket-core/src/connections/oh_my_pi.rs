//! Oh My Pi v18.15.0 configuration; distinct from Pi's JSON and stored-auth priority.
use super::provider_config::ProviderDocument;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
pub(crate) fn active_profile() -> Result<Option<String>> {
    normalize_profile(
        &std::env::var("OMP_PROFILE")
            .ok()
            .or_else(|| std::env::var("PI_PROFILE").ok())
            .unwrap_or_default(),
    )
}
fn normalize_profile(profile: &str) -> Result<Option<String>> {
    let name = profile.trim();
    if name.is_empty() || name == "default" {
        return Ok(None);
    }
    let base = name.split('.').next().unwrap().to_ascii_uppercase();
    if name.len() > 64
        || name == "."
        || name == ".."
        || name.ends_with('.')
        || !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-' | '.'))
        || !name
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        || matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (base.len() == 4
            && (base.starts_with("COM") || base.starts_with("LPT"))
            && base.as_bytes()[3].is_ascii_digit())
    {
        bail!("Oh My Pi profile 名称无效，请检查 OMP_PROFILE/PI_PROFILE")
    }
    Ok(Some(name.into()))
}
pub(crate) fn config_path(home: &Path) -> PathBuf {
    let root = home.join(
        std::env::var("PI_CONFIG_DIR")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| ".omp".into())
            .trim_start_matches(['/', '\\']),
    );
    let agent = match active_profile().ok().flatten() {
        Some(profile) => root.join("profiles").join(profile).join("agent"),
        None => std::env::var_os("PI_CODING_AGENT_DIR")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .filter(|path| {
                let lower = normalize_profile(&std::env::var("PI_PROFILE").unwrap_or_default())
                    .ok()
                    .flatten();
                lower.is_none_or(|profile| {
                    *path != root.join("profiles").join(profile).join("agent")
                })
            })
            .map(|p| {
                if p.is_absolute() {
                    p
                } else {
                    std::env::current_dir().unwrap_or_default().join(p)
                }
            })
            .unwrap_or_else(|| root.join("agent")),
    };
    let canonical = agent.join("models.yml");
    if !canonical.exists() && agent.join("models.yaml").exists() {
        agent.join("models.yaml")
    } else {
        canonical
    }
}

fn check_profile_path(agent: &Path) -> Result<()> {
    let parent = agent.parent().context("Oh My Pi profile 目录无效")?;
    let root = if parent
        .parent()
        .is_some_and(|p| p.file_name().is_some_and(|n| n == "profiles"))
    {
        parent.parent().and_then(Path::parent).unwrap_or(parent)
    } else {
        parent
    };
    let expected = if let Some(profile) = active_profile()? {
        root.join("profiles").join(profile).join("agent")
    } else {
        let lower = normalize_profile(&std::env::var("PI_PROFILE").unwrap_or_default())
            .ok()
            .flatten();
        std::env::var_os("PI_CODING_AGENT_DIR")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .filter(|path| {
                lower.is_none_or(|profile| {
                    *path != root.join("profiles").join(profile).join("agent")
                })
            })
            .map(|path| {
                if path.is_absolute() {
                    path
                } else {
                    std::env::current_dir().unwrap_or_default().join(path)
                }
            })
            .unwrap_or_else(|| root.join("agent"))
    };
    if expected != agent {
        bail!("Oh My Pi active profile/配置目录已改变，请返回原 profile 取消接入后重新检测")
    }
    Ok(())
}

pub(super) fn parse(bytes: Option<&[u8]>) -> Result<Value> {
    let value: Value = serde_yaml::from_slice(bytes.unwrap_or(b""))
        .context("Oh My Pi 配置不是有效 YAML，请先修复")?;
    Ok(if value.is_null() { json!({}) } else { value })
}
pub(super) fn validate(value: &Value, path: &Path) -> Result<()> {
    active_profile()?;
    if !matches!(
        path.extension().and_then(|s| s.to_str()),
        Some("yml" | "yaml")
    ) {
        bail!("Oh My Pi 模型配置必须使用 models.yml/models.yaml")
    }
    let agent = path.parent().context("Oh My Pi 配置目录无效")?;
    check_profile_path(agent)?;
    if path.extension().is_some_and(|s| s == "yaml") && agent.join("models.yml").exists() {
        bail!("Oh My Pi models.yml 优先于 models.yaml，请重新检测生效配置")
    }
    if !agent.join("models.yml").exists()
        && !agent.join("models.yaml").exists()
        && agent.join("models.json").exists()
    {
        bail!("Oh My Pi 存在旧 models.json，请先运行 Oh My Pi 完成官方 YAML 迁移后重新检测，原文件未改动")
    }
    let settings = read_effective_settings(agent)?;
    check_disabled(&settings)?;
    let broker_url = std::env::var("OMP_AUTH_BROKER_URL")
        .ok()
        .filter(|v| !v.is_empty())
        .or_else(|| {
            setting(&settings, "auth.broker.url")
                .and_then(Value::as_str)
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
        });
    if broker_url.is_some() {
        let token = std::env::var("OMP_AUTH_BROKER_TOKEN")
            .ok()
            .filter(|v| !v.is_empty())
            .or_else(|| {
                setting(&settings, "auth.broker.token")
                    .and_then(Value::as_str)
                    .filter(|v| !v.is_empty())
                    .map(str::to_owned)
            });
        let token_root = if active_profile()?.is_none()
            && std::env::var_os("PI_CODING_AGENT_DIR").is_some_and(|v| !v.is_empty())
        {
            crate::config::home_dir().join(
                std::env::var("PI_CONFIG_DIR")
                    .ok()
                    .filter(|v| !v.is_empty())
                    .unwrap_or_else(|| ".omp".into())
                    .trim_start_matches(['/', '\\']),
            )
        } else {
            agent.parent().unwrap_or(agent).to_path_buf()
        };
        let token_file = match std::fs::read_to_string(token_root.join("auth-broker.token")) {
            Ok(value) => !value.trim().is_empty(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(_) => bail!("无法读取 Oh My Pi auth broker token 文件"),
        };
        if token.is_none() && !token_file {
            bail!("Oh My Pi auth broker 已配置但缺少 token；请先完成 broker 配置或关闭该模式")
        }
    }
    let Some(providers) = value.get("providers") else {
        return Ok(());
    };
    let providers = providers
        .as_object()
        .context("Oh My Pi providers 必须是对象")?;
    let Some(provider) = providers.get("tokenmarket") else {
        return Ok(());
    };
    let provider = provider
        .as_object()
        .context("Oh My Pi Token Market provider 必须是对象")?;
    for field in ["baseUrl", "api", "apiKey", "auth"] {
        if provider.get(field).is_some_and(|v| !v.is_string()) {
            bail!("Oh My Pi Token Market provider 字段无效")
        }
    }
    if provider.contains_key("transport") {
        bail!("Oh My Pi Token Market provider 已配置网关 transport，不能用普通 Chat Completions 接入替换")
    }
    if provider
        .get("auth")
        .and_then(Value::as_str)
        .is_some_and(|v| v != "apiKey")
    {
        bail!("Oh My Pi Token Market provider 已配置其他 auth 模式，请先确认认证方式")
    }
    reject_auth_headers(&json!(provider))?;
    if let Some(rows) = provider.get("models") {
        let rows = rows.as_array().context("Oh My Pi models 必须是数组")?;
        let mut seen = std::collections::HashSet::new();
        for row in rows {
            let id = row["id"]
                .as_str()
                .filter(|v| !v.is_empty())
                .context("Oh My Pi 模型缺少有效 ID")?;
            if !seen.insert(id) {
                bail!("Oh My Pi 模型列表存在重复 ID")
            };
            reject_auth_headers(row)?;
            validate_model_limits(
                row,
                provider.get("modelOverrides").and_then(|rows| rows.get(id)),
            )?;
        }
    }
    if let Some(overrides) = provider.get("modelOverrides") {
        for row in overrides
            .as_object()
            .context("Oh My Pi modelOverrides 必须是对象")?
            .values()
        {
            if ["api", "baseUrl", "transport", "auth"]
                .iter()
                .any(|field| row.get(field).is_some())
            {
                bail!("Oh My Pi modelOverrides 配置了其他请求路由，请先确认 Token Market provider")
            }
            reject_auth_headers(row)?;
        }
    }
    if let Some(key) = provider
        .get("apiKey")
        .and_then(Value::as_str)
        .filter(|v| !v.starts_with('!'))
    {
        check_literal_key(key)?;
    }
    Ok(())
}
fn reject_auth_headers(value: &Value) -> Result<()> {
    if let Some(headers) = value.get("headers") {
        let headers = headers.as_object().context("Oh My Pi headers 必须是对象")?;
        if headers.keys().any(|key| {
            key.eq_ignore_ascii_case("authorization") || key.eq_ignore_ascii_case("x-api-key")
        }) {
            bail!("Oh My Pi Token Market 配置了认证请求头，请先确认认证方式")
        }
    }
    Ok(())
}
fn check_literal_key(key: &str) -> Result<()> {
    if key.starts_with('!') {
        bail!("Oh My Pi 无法将此 Key 安全写为字面凭据，原配置未改动")
    }
    if std::env::var(key)
        .ok()
        .is_some_and(|value| !value.is_empty() && value != key)
    {
        bail!("Oh My Pi 的 Key 被同名环境变量覆盖，请先解决凭据冲突")
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
    let Some(key) = provider["apiKey"]
        .as_str()
        .filter(|v| !v.is_empty() && !v.starts_with('!'))
    else {
        return Ok(None);
    };
    check_literal_key(key)?;
    Ok(Some(key.into()))
}
pub(super) fn apply(
    document: &mut ProviderDocument,
    gateway: &str,
    key: &str,
    models: &[Value],
) -> Result<()> {
    check_literal_key(key)?;
    for (field, value) in [
        (
            "baseUrl",
            json!(format!("{}/v1", gateway.trim_end_matches('/'))),
        ),
        ("apiKey", json!(key)),
        ("api", json!("openai-completions")),
        ("auth", json!("apiKey")),
        ("authHeader", json!(true)),
    ] {
        document.set(&["providers", "tokenmarket", field], value)?;
    }
    for model in models {
        let id = model["id"].as_str().context("模型目录缺少 ID")?;
        let mut metadata = json!({"name":model["display_name"].as_str().filter(|v|!v.is_empty()).unwrap_or(id),"api":"openai-completions","baseUrl":format!("{}/v1",gateway.trim_end_matches('/'))});
        super::model_plan::project_metadata(model, &mut metadata);
        document.set_model_fields(
            &["providers", "tokenmarket", "models"],
            id,
            metadata.as_object().unwrap(),
        )?;
    }
    let provider = &document.value["providers"]["tokenmarket"];
    for row in provider["models"].as_array().into_iter().flatten() {
        validate_model_limits(
            row,
            row["id"]
                .as_str()
                .and_then(|id| provider["modelOverrides"].get(id)),
        )?;
    }
    Ok(())
}
fn validate_model_limits(row: &Value, overrides: Option<&Value>) -> Result<()> {
    let effective = |field: &str| {
        overrides
            .and_then(|value| value.get(field))
            .or_else(|| row.get(field))
    };
    for field in ["contextWindow", "maxTokens"] {
        if effective(field)
            .is_some_and(|value| value.as_f64().is_none_or(|n| !n.is_finite() || n <= 0.0))
        {
            bail!("Oh My Pi {field} 必须是正数，原配置已保留")
        }
    }
    if let Some(cap) = effective("maxContextWindow") {
        let cap = cap
            .as_u64()
            .filter(|cap| *cap > 0 && *cap <= 9007199254740991)
            .context("Oh My Pi maxContextWindow 必须是正安全整数")?;
        if effective("contextWindow")
            .and_then(Value::as_f64)
            .is_some_and(|context| context > cap as f64)
        {
            bail!("Oh My Pi maxContextWindow 小于导入的 contextWindow，请先确认模型上限，原配置已保留")
        }
    }
    Ok(())
}
fn setting<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut nested = Some(value);
    for part in path.split('.') {
        nested = nested.and_then(|value| value.get(part));
    }
    nested.or_else(|| value.get(path))
}
fn read_settings(path: &Path) -> Result<Option<Value>> {
    if std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        bail!("Oh My Pi 设置是符号链接，请先确认实际路径")
    }
    let bytes = match std::fs::read(path) {
        Ok(v) => v,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => bail!("无法读取 Oh My Pi 设置"),
    };
    let value: Value = if matches!(
        path.extension().and_then(|v| v.to_str()),
        Some("yml" | "yaml")
    ) {
        parse(Some(&bytes))?
    } else {
        super::restore::Format::for_path(path)?
            .parse(Some(&bytes))
            .context("Oh My Pi 设置无效，请先修复")?
    };
    if !value.is_object() {
        bail!("Oh My Pi 设置必须是对象")
    };
    Ok(Some(value))
}
fn read_effective_settings(agent: &Path) -> Result<Value> {
    let global = if agent.join("config.yml").exists() {
        agent.join("config.yml")
    } else if agent.join("config.yaml").exists() {
        agent.join("config.yaml")
    } else {
        agent.join("settings.json")
    };
    let mut effective = read_settings(&global)?.unwrap_or_else(|| json!({}));
    let cwd = std::env::current_dir().context("无法读取 Oh My Pi 当前目录")?;
    let mut layers = vec![cwd.join(".omp/settings.json"), cwd.join(".omp/config.yml")];
    if let Some(paths) = std::env::var_os("PI_CONFIG_FILES") {
        layers.extend(std::env::split_paths(&paths));
    }
    for path in layers {
        if let Some(layer) = read_settings(&path)? {
            if let Some(disabled) = layer.get("disabledProviders") {
                effective["disabledProviders"] = disabled.clone();
            }
        }
    }
    Ok(effective)
}
fn check_disabled(settings: &Value) -> Result<()> {
    let Some(disabled) = settings.get("disabledProviders") else {
        return Ok(());
    };
    let rows = disabled
        .as_array()
        .context("Oh My Pi disabledProviders 必须是数组")?;
    let cwd = std::env::current_dir()?
        .canonicalize()
        .unwrap_or(std::env::current_dir()?);
    for row in rows {
        let selected = if let Some(name) = row.as_str() {
            vec![name]
        } else if let Some(entry) = row.as_object() {
            let mut paths = vec![];
            for key in ["path", "paths", "pathPrefix", "pathPrefixes"] {
                match entry.get(key) {
                    Some(Value::String(v)) => paths.push(v.as_str()),
                    Some(Value::Array(v)) => paths.extend(v.iter().filter_map(Value::as_str)),
                    _ => {}
                }
            }
            let matched = paths.iter().any(|path| {
                let path = if let Some(rest) = path.strip_prefix("~/") {
                    crate::config::home_dir().join(rest)
                } else {
                    PathBuf::from(path)
                };
                let path = if path.is_absolute() {
                    path
                } else {
                    cwd.join(path)
                };
                cwd.starts_with(path.canonicalize().unwrap_or(path))
            });
            if !matched {
                vec![]
            } else {
                ["providers", "values", "items"]
                    .iter()
                    .filter_map(|key| entry.get(*key))
                    .flat_map(|value| {
                        value
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                    })
                    .collect()
            }
        } else {
            vec![]
        };
        if selected.contains(&"tokenmarket") {
            bail!("Oh My Pi disabledProviders 已禁用 tokenmarket，请先调整该 provider 设置")
        }
    }
    Ok(())
}
