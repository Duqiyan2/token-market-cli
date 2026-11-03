//! Enterprise local models are a root array, independently owned by modelKey.
//! Import does not call DuMate's billable probe or mutate session selections.
use super::{configuration::PreparedConfiguration, provider_config::escaped, FieldReceipt};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub(super) struct EnterpriseConfig {
    rows: Vec<Value>,
}
fn limits(model: &Value) -> Option<(u64, u64)> {
    let context = model["context_window_tokens"]
        .as_u64()
        .filter(|n| *n > 0 && *n <= i64::MAX as u64)?;
    let output = model["max_output_tokens"]
        .as_u64()
        .filter(|n| *n > 0 && *n <= i64::MAX as u64)?;
    Some((context, output.min(context)))
}
impl EnterpriseConfig {
    pub fn parse(before: &Option<Vec<u8>>) -> Result<Self> {
        let value: Value = match before {
            Some(bytes) => {
                serde_json::from_slice(bytes).context("DuMate 企业模型配置不是有效 JSON")?
            }
            None => json!([]),
        };
        let rows = value.as_array().context("DuMate 企业模型配置必须是数组")?;
        let mut keys = std::collections::HashSet::new();
        for row in rows {
            let key = row
                .get("modelKey")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .context("DuMate 企业模型记录缺少 modelKey")?;
            if !row.is_object() || !keys.insert(key) {
                bail!("DuMate 企业模型记录存在重复标识")
            }
        }
        Ok(Self { rows: rows.clone() })
    }
    pub fn select_models(&self, models: &[Value]) -> Result<Vec<Value>> {
        let rows: Vec<_> = models
            .iter()
            .filter(|m| limits(m).is_some())
            .cloned()
            .collect();
        if rows.is_empty() {
            bail!("DuMate 企业版需要具有有效上下文与输出限制元数据的文本模型")
        }
        Ok(rows)
    }
    pub fn credential(&self, gateway: &str) -> Result<Option<String>> {
        let base = format!("{}/v1", gateway.trim_end_matches('/'));
        let mut keys = std::collections::HashSet::new();
        for row in &self.rows {
            if row["modelKey"]
                .as_str()
                .is_some_and(|s| s.starts_with("personal:tokenmarket-"))
                && row["baseURL"]
                    .as_str()
                    .is_some_and(|s| s.trim_end_matches('/') == base)
            {
                if let Some(key) = row["apiKey"].as_str().filter(|s| !s.is_empty()) {
                    keys.insert(key.to_owned());
                }
            }
        }
        if keys.len() > 1 {
            bail!("DuMate 企业版存在多个 Token Market 凭据，请先确认原连接")
        }
        Ok(keys.into_iter().next())
    }
    pub fn apply(
        &mut self,
        gateway: &str,
        key: &str,
        models: &[Value],
    ) -> Result<PreparedConfiguration> {
        let scope = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&json!([
                gateway.trim_end_matches('/'),
                key
            ]))?)
        );
        let mut fields = vec![];
        let mut created_containers = vec![];
        for model in models {
            let id = model["id"]
                .as_str()
                .filter(|s| !s.is_empty())
                .context("DuMate 企业模型缺少 ID")?;
            let (context, output) =
                limits(model).context("DuMate 企业模型缺少有效上下文/输出元数据")?;
            let digest = format!(
                "{:x}",
                Sha256::digest(serde_json::to_vec(&json!([
                    id,
                    context,
                    output,
                    model["input_modalities"]
                ]))?)
            );
            let model_key = format!("personal:tokenmarket-{}-{}", &scope[..12], &digest[..12]);
            let selector = format!("/@modelKey={}", escaped(&model_key));
            let index =
                if let Some(index) = self.rows.iter().position(|r| r["modelKey"] == model_key) {
                    index
                } else {
                    self.rows.push(json!({"modelKey":model_key}));
                    created_containers.push(selector.clone());
                    self.rows.len() - 1
                };
            let row = self.rows[index]
                .as_object_mut()
                .context("DuMate 企业模型必须是对象")?;
            // Do not own dynamic probe results. A new credential/gateway has a new
            // modelKey and normalizes to untested; identical reconnects keep real checks.
            let values = json!({
                "displayName":format!("Token Market · {id}"),
                "modelID":id, "protocol":"openai_chat_completions",
                "baseURL":format!("{}/v1",gateway.trim_end_matches('/')), "apiKey":key,
                "modelCategory":"LLM", "thinkingMode":"unknown",
                "supportsImageUnderstanding": model["input_modalities"].as_array().is_some_and(|v| v.iter().any(|s| s == "image")),
                "contextTokens":context, "outputTokens":output
            });
            for (name, applied) in values.as_object().unwrap() {
                fields.push(FieldReceipt {
                    pointer: format!("{selector}/{}", escaped(name)),
                    before: row.insert(name.clone(), applied.clone()),
                    applied: applied.clone(),
                    retired: false,
                });
            }
        }
        Ok(PreparedConfiguration {
            bytes: serde_json::to_vec_pretty(&self.rows)?,
            fields,
            created_containers,
            additional_files: vec![],
            protocol: "OPENAI_CHAT_COMPLETIONS",
        })
    }
}

/// A confirmed, completed protocol probe supplies the evidence for this derived
/// state. Connect never writes it. DuMate 1.0.89's edit UI drops modelKey and
/// otherwise cannot persist successful checks for unchanged imported models.
pub(super) fn persist_check(
    record: &super::ConnectionRecord,
    model: &str,
    passed: bool,
) -> Result<()> {
    super::restore::verify_applied(record)?;
    let receipt = record
        .receipts
        .first()
        .context("企业连接缺少模型配置记录")?;
    crate::dumate::require_stopped(&receipt.path)?;
    let selector = receipt
        .fields
        .iter()
        .find(|f| !f.retired && f.pointer.ends_with("/modelID") && f.applied == model)
        .and_then(|f| f.pointer.strip_suffix("/modelID"))
        .and_then(|p| p.strip_prefix("/@modelKey="))
        .context("企业连接缺少所选模型标识")?;
    let key = selector.replace("~1", "/").replace("~0", "~");
    let before = std::fs::read(&receipt.path).context("无法读取 DuMate 企业模型配置")?;
    let mut config = EnterpriseConfig::parse(&Some(before.clone()))?;
    let row = config
        .rows
        .iter_mut()
        .find(|r| r["modelKey"] == key)
        .and_then(Value::as_object_mut)
        .context("DuMate 企业模型已改变，请重新接入")?;
    row.insert(
        "accessStatus".into(),
        json!(if passed { "normal" } else { "abnormal" }),
    );
    row.insert(
        "accessStatusMessage".into(),
        json!(if passed {
            "Token Market 已验证一次完整接口文本响应"
        } else {
            "Token Market 接口文本检查未通过，请手动重试"
        }),
    );
    row.insert(
        "connectionCheckedAt".into(),
        json!(chrono::Utc::now().timestamp_millis()),
    );
    if passed {
        // This request really used max_tokens and received a complete assistant
        // response. These are probe results, never fabricated capability metadata.
        row.insert("maxOutputTokensField".into(), json!("max_tokens"));
        row.insert("maxOutputTokensFieldSource".into(), json!("probe_accepted"));
    }
    if std::fs::read(&receipt.path)? != before {
        bail!("DuMate 企业模型配置已改变，未写入检查结果")
    }
    super::atomic_write(&receipt.path, &serde_json::to_vec_pretty(&config.rows)?)
        .context("模型请求已结束，但 DuMate 检查状态未能保存，请检查本地权限后重试")
}
