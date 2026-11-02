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
            ToolId::GrokBuild => super::grok::validate(&value)?,
            ToolId::Hermes => super::hermes::validate(&value, path)?,
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
            ToolId::GrokBuild => super::grok::credential(&self.document.value, gateway),
            ToolId::Hermes => super::hermes::credential(&self.document.value, gateway),
            _ => bail!("此工具没有受支持的 provider 配置"),
        }
    }

    pub fn apply(
        &mut self,
        gateway: &str,
        key: &str,
        models: &[Value],
    ) -> Result<PreparedConfiguration> {
        match self.tool {
            ToolId::GrokBuild => super::grok::apply(&mut self.document, gateway, key, models)?,
            ToolId::Hermes => super::hermes::apply(&mut self.document, gateway, key, models)?,
            _ => bail!("此工具没有受支持的 provider 配置"),
        }
        self.document.finish()
    }
}
