use super::configuration::{PreparedConfiguration, PreparedFile};
use super::FieldReceipt;
use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub(super) struct QwenWorkConfig {
    value: Value,
    dotenv: PathBuf,
    env_before: Option<Vec<u8>>,
    runtime: Option<crate::qwenwork_runtime::RuntimeInstallation>,
}
impl QwenWorkConfig {
    pub fn set_runtime(&mut self, runtime: crate::qwenwork_runtime::RuntimeInstallation) {
        self.runtime = Some(runtime);
    }
    pub fn credential(&self, gateway: &str) -> Result<Option<String>> {
        let endpoint = format!("{}/v1/chat/completions", gateway.trim_end_matches('/'));
        let mut credential: Option<String> = None;
        for (id, row) in self.value["models"]
            .as_object()
            .context("千问办公模型快照无效")?
        {
            if !id.starts_with(crate::qwenwork::PREFIX)
                || (row["url"] != endpoint && row["url"] != crate::qwenwork_runtime::MODEL_ENDPOINT)
            {
                continue;
            }
            let parameters: Value =
                serde_json::from_str(row["encrypted_parameters"].as_str().unwrap_or(""))?;
            if parameters["api_key"]["encoding"] != "plainBase64" {
                bail!("千问办公模型凭据已由客户端重新加密，请先取消原连接后接入")
            }
            let key = String::from_utf8(
                STANDARD.decode(
                    parameters["api_key"]["value"]
                        .as_str()
                        .context("千问办公模型凭据无效")?,
                )?,
            )?;
            if key.starts_with(crate::qwenwork_runtime::CREDENTIAL_PREFIX) {
                continue;
            }
            if key.is_empty() || credential.as_ref().is_some_and(|old| old != &key) {
                bail!("千问办公 Token Market 模型凭据不一致，请先确认配置")
            }
            credential = Some(key);
        }
        Ok(credential)
    }
    pub fn parse(before: &Option<Vec<u8>>, path: &Path) -> Result<Self> {
        Self::parse_for_connection(before, path, false)
    }
    pub fn parse_for_connection(
        before: &Option<Vec<u8>>,
        path: &Path,
        overwrite_managed: bool,
    ) -> Result<Self> {
        let value: Value =
            serde_json::from_slice(before.as_deref().context("请先登录初始化千问办公")?)?;
        let models = value["models"]
            .as_object()
            .context("千问办公模型快照无效")?;
        // Reserve a namespace without taking over a manually created provider.
        for (key, row) in models {
            if !overwrite_managed && key.starts_with(crate::qwenwork::PREFIX) {
                let extra: Value = serde_json::from_str(row["extra_params"].as_str().unwrap_or(""))
                    .context("千问办公 Token Market 模型已存在其他配置，请先处理后接入")?;
                if extra["tokenmarket"] != 1 {
                    bail!("千问办公模型标识已被其他配置占用")
                }
            }
        }
        let dotenv = crate::qwenwork::dotenv_path(path)?;
        let env_before = super::configuration_io::read(&dotenv)?;
        crate::qwenwork::parse_dotenv(env_before.as_deref())?;
        Ok(Self {
            value,
            dotenv,
            env_before,
            runtime: None,
        })
    }

    pub fn apply(
        &mut self,
        gateway: &str,
        key: &str,
        models: &[Value],
    ) -> Result<PreparedConfiguration> {
        let mut fields = vec![];
        let _ = gateway;
        let runtime = self
            .runtime
            .as_ref()
            .context("千问办公外置 worker 尚未准备完成")?;
        let placeholder = crate::qwenwork_runtime::placeholder(key);
        let base = "https://tokenmarket.local.invalid/v1";
        // Remove orphaned managed rows from older integrations as well. They
        // may contain a real key and must never be restored into QwenWork.
        self.value["models"]
            .as_object_mut()
            .unwrap()
            .retain(|id, _| !id.starts_with(crate::qwenwork::PREFIX));
        let now = chrono::Utc::now().timestamp_millis();
        for model in models {
            let id = model["id"].as_str().context("模型目录缺少 ID")?;
            let name = model["display_name"]
                .as_str()
                .filter(|n| !n.is_empty())
                .unwrap_or(id);
            let model_key = format!(
                "{}{:x}",
                crate::qwenwork::PREFIX,
                Sha256::digest(id.as_bytes())
            );
            let pointer = format!("/models/{model_key}");
            let before: Option<Value> = None;
            let context = model["context_window_tokens"]
                .as_i64()
                .filter(|v| *v > 0)
                .unwrap_or(0);
            let output = model["max_output_tokens"]
                .as_i64()
                .filter(|v| *v > 0)
                .unwrap_or(0);
            // Only an irreversible fingerprint leaves our encrypted credential store.
            // The original model-policy validator requires a nonempty api_key field.
            let parameters =
                json!({"api_key":{"encoding":"plainBase64","value":STANDARD.encode(&placeholder)}})
                    .to_string();
            let extra = json!({"tokenmarket":1,"baseUrl":base,"displayName":format!("Token Market · {name}"),
                "maxInputTokensK": context as f64/1024.0,"maxOutputTokensK":output as f64/1024.0}).to_string();
            let row = json!({"key":model_key,"legacy_key":null,"source":"byok",
                "display_name":format!("Token Market · {name}"),"provider":"openai","type":null,
                "url":format!("{base}/chat/completions"),"style":"openai","model":id,"format":"openai",
                "is_vl":i32::from(model["input_modalities"].as_array().is_some_and(|a| a.iter().any(|v| v=="image"))),
                "is_reasoning":i32::from(model["supports_reasoning"].as_bool().unwrap_or(false)),
                "max_input_tokens":context,"max_output_tokens":output,"encrypted_parameters":parameters,
                "extra_params":extra,"migrated_from":null,
                "created_at":before.as_ref().and_then(|v| v["created_at"].as_i64()).unwrap_or(now),"updated_at":now});
            self.value["models"][&model_key] = row.clone();
            fields.push(FieldReceipt {
                pointer,
                before,
                applied: row,
                retired: false,
            });
        }
        let mut env = crate::qwenwork::parse_dotenv(self.env_before.as_deref())?;
        let before = env.get("ALLOW_CUSTOM_BYOK").cloned();
        env["ALLOW_CUSTOM_BYOK"] = json!("1");
        let mut env_fields = vec![FieldReceipt {
            pointer: "/ALLOW_CUSTOM_BYOK".into(),
            before,
            applied: json!("1"),
            retired: false,
        }];
        let worker = runtime.worker.to_str().context("外置 worker 路径无效")?;
        env_fields.push(FieldReceipt {
            pointer: "/QW_QODER_WORKER_RUNTIME_PATH".into(),
            before: env
                .get("QW_QODER_WORKER_RUNTIME_PATH")
                .cloned()
                .filter(|value| {
                    !value
                        .as_str()
                        .is_some_and(crate::qwenwork_runtime::is_managed_worker)
                }),
            applied: json!(worker),
            retired: false,
        });
        env["QW_QODER_WORKER_RUNTIME_PATH"] = json!(worker);
        let env_bytes = crate::qwenwork::render_dotenv(
            self.env_before.as_deref(),
            &env,
            &[
                "/ALLOW_CUSTOM_BYOK".into(),
                "/QW_QODER_WORKER_RUNTIME_PATH".into(),
            ],
        )?;
        Ok(PreparedConfiguration {
            bytes: serde_json::to_vec(&self.value)?,
            fields,
            created_containers: vec![],
            additional_files: vec![PreparedFile {
                path: self.dotenv.clone(),
                before: self.env_before.clone(),
                bytes: env_bytes,
                fields: env_fields,
                created_containers: vec![],
            }],
            protocol: "OPENAI_CHAT_COMPLETIONS",
        })
    }
}
