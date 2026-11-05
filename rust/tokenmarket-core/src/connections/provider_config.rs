//! Managed provider documents reuse the restoration engine's formats and field receipts.
use super::{configuration::PreparedConfiguration, restore::Format, FieldReceipt};
use crate::ToolId;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub(super) struct ProviderConfig {
    tool: ToolId,
    document: ProviderDocument,
}

pub(super) struct ProviderDocument {
    pub value: Value,
    path: PathBuf,
    original: Option<Vec<u8>>,
    fields: Vec<FieldReceipt>,
    created_containers: Vec<String>,
}

pub(super) fn escaped(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

impl ProviderDocument {
    /// Own individual projected leaves, preserving tool/user options beside them.
    pub fn set_metadata(&mut self, path: &[&str], metadata: &Value) -> Result<()> {
        for (name, value) in metadata.as_object().context("模型元数据必须是对象")? {
            let mut child = path.to_vec();
            child.push(name);
            if value.is_object() {
                self.set_metadata(&child, value)?;
            } else {
                self.set(&child, value.clone())?;
            }
        }
        Ok(())
    }
    pub fn set(&mut self, path: &[&str], applied: Value) -> Result<()> {
        let mut value = &mut self.value;
        let mut pointer = String::new();
        for (index, segment) in path.iter().enumerate() {
            pointer.push('/');
            pointer.push_str(&escaped(segment));
            let object = value.as_object_mut().context("工具配置父级必须是对象")?;
            if index + 1 == path.len() {
                self.fields.push(FieldReceipt {
                    pointer,
                    before: object.insert((*segment).into(), applied.clone()),
                    applied,
                    retired: false,
                });
                return Ok(());
            }
            if !object.contains_key(*segment) {
                object.insert((*segment).into(), json!({}));
                self.created_containers.push(pointer.clone());
            }
            value = object.get_mut(*segment).unwrap();
        }
        bail!("工具配置字段路径为空")
    }

    /// Set owned fields on an array row selected by model ID. Receipts survive reordering.
    pub fn set_model_fields(
        &mut self,
        path: &[&str],
        id: &str,
        fields: &serde_json::Map<String, Value>,
    ) -> Result<()> {
        let mut value = &mut self.value;
        let mut pointer = String::new();
        for (index, segment) in path.iter().enumerate() {
            pointer.push('/');
            pointer.push_str(&escaped(segment));
            let object = value
                .as_object_mut()
                .context("工具模型列表父级必须是对象")?;
            if !object.contains_key(*segment) {
                object.insert(
                    (*segment).into(),
                    if index + 1 == path.len() {
                        json!([])
                    } else {
                        json!({})
                    },
                );
                self.created_containers.push(pointer.clone());
            }
            value = object.get_mut(*segment).unwrap();
        }
        let rows = value.as_array_mut().context("工具模型配置必须是数组")?;
        let matches: Vec<_> = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row["id"].as_str() == Some(id))
            .map(|(i, _)| i)
            .collect();
        if matches.len() > 1 {
            bail!("工具模型列表存在重复 ID")
        }
        let selector = format!("{pointer}/@id={}", escaped(id));
        let index = if let Some(index) = matches.first() {
            *index
        } else {
            rows.push(json!({"id": id}));
            self.created_containers.push(selector.clone());
            rows.len() - 1
        };
        let row = rows[index]
            .as_object_mut()
            .context("工具模型配置必须是对象")?;
        set_row_metadata(
            row,
            &selector,
            fields,
            &mut self.fields,
            &mut self.created_containers,
        )?;
        Ok(())
    }

    pub fn finish(&self) -> Result<PreparedConfiguration> {
        let touched = self
            .fields
            .iter()
            .map(|field| field.pointer.clone())
            .collect::<Vec<_>>();
        Ok(PreparedConfiguration {
            bytes: Format::for_path(&self.path)?.render(
                self.original.as_deref(),
                &self.value,
                &touched,
            )?,
            fields: self.fields.clone(),
            created_containers: self.created_containers.clone(),
            additional_files: vec![],
            protocol: "OPENAI_CHAT_COMPLETIONS",
        })
    }
}

pub(super) fn set_row_metadata(
    row: &mut serde_json::Map<String, Value>,
    prefix: &str,
    metadata: &serde_json::Map<String, Value>,
    fields: &mut Vec<FieldReceipt>,
    created: &mut Vec<String>,
) -> Result<()> {
    for (name, applied) in metadata {
        let pointer = format!("{prefix}/{}", escaped(name));
        if let Some(children) = applied.as_object() {
            if !row.contains_key(name) {
                row.insert(name.clone(), json!({}));
                created.push(pointer.clone());
            }
            let target = row
                .get_mut(name)
                .unwrap()
                .as_object_mut()
                .context("工具模型元数据父级必须是对象")?;
            set_row_metadata(target, &pointer, children, fields, created)?;
        } else {
            fields.push(FieldReceipt {
                pointer,
                before: row.insert(name.clone(), applied.clone()),
                applied: applied.clone(),
                retired: false,
            });
        }
    }
    Ok(())
}

impl ProviderConfig {
    pub fn parse(tool: ToolId, before: &Option<Vec<u8>>, path: &Path) -> Result<Self> {
        if tool == ToolId::GrokBuild
            && path.extension().and_then(|extension| extension.to_str()) != Some("toml")
        {
            bail!("Grok Build 配置必须使用 TOML 文件")
        }
        if tool == ToolId::Hermes
            && !matches!(
                path.extension().and_then(|extension| extension.to_str()),
                Some("yaml" | "yml")
            )
        {
            bail!("Hermes 配置必须使用 YAML 文件")
        }
        let value = Format::for_path(path)?.parse(before.as_deref())?;
        if !value.is_object() {
            bail!("工具 provider 配置必须是对象")
        }
        match tool {
            ToolId::DuMate => super::dumate::validate(&value)?,
            ToolId::GrokBuild => super::grok::validate(&value)?,
            ToolId::KimiCode => super::kimi::validate(&value, path)?,
            ToolId::Hermes => super::hermes::validate(&value, path)?,
            ToolId::Pi => super::pi::validate(&value, path)?,
            ToolId::MiniMaxCode => super::minimax::validate(&value, path)?,
            ToolId::OhMyPi => super::oh_my_pi::validate(&value, path)?,
            _ => bail!("此工具没有受支持的 provider 配置"),
        }
        Ok(Self {
            tool,
            document: ProviderDocument {
                value,
                path: path.into(),
                original: before.clone(),
                fields: vec![],
                created_containers: vec![],
            },
        })
    }

    pub fn credential(&self, gateway: &str) -> Result<Option<String>> {
        match self.tool {
            ToolId::DuMate => super::dumate::credential(&self.document.value, gateway),
            ToolId::GrokBuild => super::grok::credential(&self.document.value, gateway),
            ToolId::KimiCode => super::kimi::credential(&self.document.value, gateway),
            ToolId::Hermes => super::hermes::credential(&self.document.value, gateway),
            ToolId::Pi => super::pi::credential(&self.document.value, gateway),
            ToolId::MiniMaxCode => super::minimax::credential(&self.document.value, gateway),
            ToolId::OhMyPi => super::oh_my_pi::credential(&self.document.value, gateway),
            _ => bail!("此工具没有受支持的 provider 配置"),
        }
    }

    pub fn select_models(&self, models: &[Value]) -> Result<Vec<Value>> {
        if self.tool == ToolId::DuMate {
            return super::dumate::select(&self.document.value, models);
        }
        Ok(models.to_vec())
    }

    pub fn apply(
        &mut self,
        gateway: &str,
        key: &str,
        models: &[Value],
    ) -> Result<PreparedConfiguration> {
        match self.tool {
            ToolId::DuMate => super::dumate::apply(&mut self.document, gateway, key, models)?,
            ToolId::GrokBuild => super::grok::apply(&mut self.document, gateway, key, models)?,
            ToolId::KimiCode => {
                super::kimi::validate(&self.document.value, &self.document.path)?;
                super::kimi::apply(&mut self.document, gateway, key, models)?;
            }
            ToolId::Hermes => {
                super::hermes::validate(&self.document.value, &self.document.path)?;
                super::hermes::apply(&mut self.document, gateway, key, models)?;
            }
            ToolId::MiniMaxCode => {
                super::minimax::validate(&self.document.value, &self.document.path)?;
                super::minimax::apply(&mut self.document, gateway, key, models)?;
            }
            ToolId::OhMyPi => {
                super::oh_my_pi::validate(&self.document.value, &self.document.path)?;
                super::oh_my_pi::apply(&mut self.document, gateway, key, models)?;
            }
            ToolId::Pi => {
                super::pi::validate(&self.document.value, &self.document.path)?;
                super::pi::apply(&mut self.document, gateway, key, models)?;
            }
            _ => bail!("此工具没有受支持的 provider 配置"),
        }
        self.document.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(path: &str) -> ProviderDocument {
        ProviderDocument {
            value: json!({}),
            path: PathBuf::from(path),
            original: None,
            fields: vec![],
            created_containers: vec![],
        }
    }

    #[test]
    fn provider_adapters_write_native_capabilities_and_receipts() {
        let glm = crate::model_catalog::complete_metadata(
            &json!({"id":"glm-5.3","max_input_tokens":128000,"max_output_tokens":8192,"input_modalities":["text","image"]}),
        );
        let kimi = crate::model_catalog::complete_metadata(&json!({"id":"kimi-k2.6"}));
        let fixed = crate::model_catalog::complete_metadata(&json!({"id":"minimax-m2.5"}));
        let models = [glm, kimi, fixed];
        let mut doc = document("config.toml");
        super::super::kimi::apply(&mut doc, "https://fixture.invalid", "fixture-key", &models)
            .unwrap();
        assert_eq!(
            doc.value["models"]["tokenmarket/glm-5.3"]["support_efforts"],
            json!(["low", "high", "max"])
        );
        assert_eq!(
            doc.value["models"]["tokenmarket/glm-5.3"]["default_effort"],
            "high"
        );
        assert_eq!(
            doc.value["models"]["tokenmarket/glm-5.3"]["max_input_size"],
            128000
        );
        assert!(doc.value["models"]["tokenmarket/glm-5.3"]["capabilities"]
            .as_array()
            .unwrap()
            .contains(&json!("always_thinking")));
        assert!(
            !doc.value["models"]["tokenmarket/kimi-k2.6"]["capabilities"]
                .as_array()
                .unwrap()
                .contains(&json!("always_thinking"))
        );
        assert_eq!(
            doc.value["models"]["tokenmarket/kimi-k2.6"]["support_efforts"],
            json!([])
        );
        let prepared = doc.finish().unwrap();
        assert!(std::str::from_utf8(&prepared.bytes)
            .unwrap()
            .contains("support_efforts"));
        assert!(prepared
            .fields
            .iter()
            .any(|f| f.pointer == "/models/tokenmarket~1glm-5.3/max_input_size"));

        let mut doc = document("config.yaml");
        super::super::minimax::apply(&mut doc, "https://fixture.invalid", "fixture-key", &models)
            .unwrap();
        assert_eq!(
            doc.value["custom_provider"]["tokenmarket"]["models"]["glm-5.3"]["thinking"]
                ["effortOptions"],
            json!(["low", "high", "max"])
        );
        assert_eq!(
            doc.value["custom_provider"]["tokenmarket"]["models"]["kimi-k2.6"]["thinking_config"]
                ["mode"],
            "switchable"
        );
        assert_eq!(
            doc.value["custom_provider"]["tokenmarket"]["models"]["minimax-m2.5"]
                ["thinking_config"]["mode"],
            "forced_on"
        );
        assert!(doc.finish().is_ok());

        let mut doc = document("config.toml");
        super::super::grok::apply(&mut doc, "https://fixture.invalid", "fixture-key", &models)
            .unwrap();
        assert_eq!(
            doc.value["model"]["tokenmarket/glm-5.3"]["reasoning_effort"],
            "high"
        );
        assert_eq!(
            doc.value["model"]["tokenmarket/glm-5.3"]["reasoning_efforts"][2]["value"],
            "max"
        );
        assert_eq!(
            doc.value["model"]["tokenmarket/kimi-k2.6"]["supports_reasoning_effort"],
            false
        );
        assert!(doc.finish().is_ok());

        let mut doc = document("config.yaml");
        super::super::hermes::apply(&mut doc, "https://fixture.invalid", "fixture-key", &models)
            .unwrap();
        assert_eq!(
            doc.value["providers"]["tokenmarket"]["models"]["glm-5.3"]["supports_vision"],
            true
        );
        assert_eq!(
            doc.value["model_overrides"]["tokenmarket"]["glm-5.3"]["supports_reasoning"],
            true
        );
        assert_eq!(
            doc.value["agent"]["reasoning_overrides"]["tokenmarket/glm-5.3"]["effort"],
            "high"
        );
        assert!(doc.finish().is_ok());
    }

    #[test]
    fn array_model_metadata_owns_leaves_and_preserves_unmanaged_compatibility() {
        let mut doc = document("models.json");
        doc.value = json!({"providers":{"tokenmarket":{"models":[{"id":"glm-5.3","compat":{"custom":"keep"}}]}}});
        let model = crate::model_catalog::complete_metadata(&json!({"id":"glm-5.3"}));
        super::super::pi::apply(&mut doc, "https://fixture.invalid", "fixture-key", &[model])
            .unwrap();
        assert_eq!(
            doc.value["providers"]["tokenmarket"]["models"][0]["compat"]["custom"],
            "keep"
        );
        assert!(doc.fields.iter().any(|f| f.pointer
            == "/providers/tokenmarket/models/@id=glm-5.3/compat/supportsReasoningEffort"));
        assert!(!doc.fields.iter().any(|f| f.pointer.ends_with("/compat")));
    }
}
