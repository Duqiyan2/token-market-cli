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
        for (name, applied) in fields {
            self.fields.push(FieldReceipt {
                pointer: format!("{selector}/{}", escaped(name)),
                before: row.insert(name.clone(), applied.clone()),
                applied: applied.clone(),
                retired: false,
            });
        }
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
