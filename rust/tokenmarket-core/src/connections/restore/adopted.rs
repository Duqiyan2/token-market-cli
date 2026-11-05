//! Old clients left platform configuration without a connection receipt. Adopting
//! it must not turn cancellation into restoration of that same platform access.
use super::{ConnectionRecord, Format};
use crate::{
    connections::{private_storage::PrivateStorage, FieldReceipt, FileReceipt},
    ToolId,
};
use anyhow::Result;
use serde_json::{json, Value};

fn escape(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

fn same_route(value: &Value, expected: &str) -> bool {
    value
        .as_str()
        .is_some_and(|value| value.trim_end_matches('/') == expected)
}

/// Adopt an identified legacy namespace, retaining ordinary conflict detection
/// and any fields added by the user after the recorded original configuration.
fn adopt_fields(receipt: &mut FileReceipt, pointer: &str, original: &Value, keep_id: bool) {
    if let Some(field) = receipt
        .fields
        .iter_mut()
        .find(|field| field.pointer == pointer)
    {
        field.before = None;
        return;
    }
    if let Some(properties) = original.as_object() {
        for (name, value) in properties {
            if keep_id && matches!(name.as_str(), "id" | "modelKey") {
                continue;
            }
            adopt_fields(
                receipt,
                &format!("{pointer}/{}", escape(name)),
                value,
                false,
            );
        }
    } else if let Some(rows) = original
        .as_array()
        .filter(|rows| !rows.is_empty() && rows.iter().all(|row| row["id"].is_string()))
    {
        for row in rows {
            adopt_fields(
                receipt,
                &format!("{pointer}/@id={}", escape(row["id"].as_str().unwrap())),
                row,
                true,
            );
        }
    } else {
        receipt.fields.push(FieldReceipt {
            pointer: pointer.into(),
            before: None,
            applied: original.clone(),
            retired: false,
        });
        return;
    }
    if !receipt
        .created_containers
        .iter()
        .any(|container| container == pointer)
    {
        receipt.created_containers.push(pointer.into());
    }
}

pub(super) fn normalize(
    record: &ConnectionRecord,
    receipts: &mut [FileReceipt],
    private: &PrivateStorage,
) -> Result<()> {
    if record.tool == ToolId::QwenWork {
        return Ok(());
    }
    let originals = receipts
        .iter()
        .map(|receipt| {
            receipt
                .backup_path
                .as_ref()
                .map(|path| {
                    Format::for_tool(record.tool, &receipt.path)?.parse(Some(&private.read(path)?))
                })
                .transpose()
        })
        .collect::<Result<Vec<_>>>()?;
    let platform_route = format!("{}/v1", record.gateway_url.trim_end_matches('/'));
    let adopted_harness = record.tool == ToolId::DeepSeekHarness
        && originals
            .iter()
            .flatten()
            .filter_map(Value::as_array)
            .flatten()
            .any(|row| {
                row["id"] == "llm-pi-ai"
                    && row["config"]["providers"]
                        .as_object()
                        .is_some_and(|providers| {
                            providers.iter().any(|(id, provider)| {
                                crate::model_catalog::harness_managed_provider(id)
                                    && same_route(&provider["baseURL"], &platform_route)
                                    && provider["apiKeyEnv"] == "TOKEN_MARKET_API_KEY"
                            })
                        })
            });
    for (receipt, original) in receipts.iter_mut().zip(originals) {
        let Some(original) = original else { continue };
        if record.tool == ToolId::WorkBuddy {
            let prefix = if original.is_array() { "" } else { "/models" };
            let rows = if prefix.is_empty() {
                original.as_array()
            } else {
                original["models"].as_array()
            };
            let route = format!(
                "{}/v1/chat/completions",
                record.gateway_url.trim_end_matches('/')
            );
            for row in rows
                .into_iter()
                .flatten()
                .filter(|row| row["vendor"] == "Token Market" && same_route(&row["url"], &route))
            {
                let Some(id) = row["id"].as_str() else {
                    continue;
                };
                adopt_fields(receipt, &format!("{prefix}/@id={}", escape(id)), row, true);
                if original["availableModels"]
                    .as_array()
                    .is_some_and(|ids| ids.iter().any(|value| value == id))
                {
                    let pointer = format!("/availableModels/@value={}", escape(id));
                    if let Some(field) = receipt
                        .fields
                        .iter_mut()
                        .find(|field| field.pointer == pointer)
                    {
                        field.before = None;
                    } else {
                        receipt.fields.push(FieldReceipt {
                            pointer,
                            before: None,
                            applied: json!(id),
                            retired: false,
                        });
                    }
                }
            }
        } else if adopted_harness {
            if let Some(rows) = original.as_array() {
                if let Some(row) = rows.iter().find(|row| row["id"] == "llm-pi-ai") {
                    if let Some(providers) =
                        row.pointer("/config/providers").and_then(Value::as_object)
                    {
                        for (id, provider) in providers.iter().filter(|(id, provider)| {
                            crate::model_catalog::harness_managed_provider(id)
                                && same_route(&provider["baseURL"], &platform_route)
                                && provider["apiKeyEnv"] == "TOKEN_MARKET_API_KEY"
                        }) {
                            adopt_fields(
                                receipt,
                                &format!("/@id=llm-pi-ai/config/providers/{}", escape(id)),
                                provider,
                                false,
                            );
                        }
                    }
                }
            } else if receipt
                .path
                .file_name()
                .is_some_and(|name| name == ".credentials.yaml")
            {
                // The existing credential field's applied value still guards edits.
                for field in &mut receipt.fields {
                    if matches!(
                        field.pointer.as_str(),
                        "/refs/TOKEN_MARKET_API_KEY" | "/TOKEN_MARKET_API_KEY"
                    ) {
                        field.before = None;
                    }
                }
            }
        } else {
            let provider = match record.tool {
                ToolId::Codex => Some(("/model_providers/tokenmarket", vec!["base_url"])),
                ToolId::OpenClaw => Some(("/models/providers/tokenmarket", vec!["baseUrl"])),
                ToolId::OpenCode => Some(("/provider/tokenmarket", vec!["options/baseURL"])),
                ToolId::Hermes => Some(("/providers/tokenmarket", vec!["api", "url", "base_url"])),
                ToolId::Pi | ToolId::OhMyPi => Some(("/providers/tokenmarket", vec!["baseUrl"])),
                ToolId::KimiCode => Some(("/providers/tokenmarket", vec!["base_url"])),
                ToolId::MiniMaxCode => {
                    Some(("/custom_provider/tokenmarket", vec!["options/baseURL"]))
                }
                ToolId::DuMate => Some(("/customLLMProvider", vec!["baseURL"])),
                _ => None,
            };
            if let Some((pointer, routes)) = provider {
                if let Some(value) = original.pointer(pointer).filter(|value| {
                    routes.iter().any(|route| {
                        value
                            .pointer(&format!("/{route}"))
                            .is_some_and(|url| same_route(url, &platform_route))
                    })
                }) {
                    adopt_fields(receipt, pointer, value, false);
                    if record.tool == ToolId::Hermes {
                        if let Some(metadata) = original.pointer("/model_overrides/tokenmarket") {
                            adopt_fields(receipt, "/model_overrides/tokenmarket", metadata, false);
                        }
                        if let Some(overrides) = original
                            .pointer("/agent/reasoning_overrides")
                            .and_then(Value::as_object)
                        {
                            for (id, setting) in overrides
                                .iter()
                                .filter(|(id, _)| id.starts_with("tokenmarket/"))
                            {
                                adopt_fields(
                                    receipt,
                                    &format!("/agent/reasoning_overrides/{}", escape(id)),
                                    setting,
                                    false,
                                );
                            }
                        }
                    }
                    if record.tool == ToolId::Codex && original["model_provider"] == "tokenmarket" {
                        for pointer in ["/model_provider", "/model"] {
                            if let Some(value) = original.pointer(pointer) {
                                adopt_fields(receipt, pointer, value, false);
                            }
                        }
                    }
                    if record.tool == ToolId::KimiCode {
                        if let Some(models) = original["models"].as_object() {
                            for (alias, model) in models.iter().filter(|(alias, model)| {
                                alias.starts_with("tokenmarket/")
                                    && model["provider"] == "tokenmarket"
                            }) {
                                adopt_fields(
                                    receipt,
                                    &format!("/models/{}", escape(alias)),
                                    model,
                                    false,
                                );
                            }
                        }
                    }
                }
            }
            if record.tool == ToolId::ClaudeCode
                && same_route(
                    &original["env"]["ANTHROPIC_BASE_URL"],
                    record.gateway_url.trim_end_matches('/'),
                )
            {
                let mut pointers = crate::adapters::claude_model_environment("")
                    .into_iter()
                    .map(|(name, _)| format!("/env/{name}"))
                    .collect::<Vec<_>>();
                pointers.extend(
                    [
                        "/model",
                        "/env/ANTHROPIC_BASE_URL",
                        "/env/ANTHROPIC_API_KEY",
                        "/env/ANTHROPIC_AUTH_TOKEN",
                        "/env/CLAUDE_CODE_MAX_CONTEXT_TOKENS",
                    ]
                    .map(str::to_owned),
                );
                for pointer in pointers {
                    if let Some(value) = original.pointer(&pointer) {
                        adopt_fields(receipt, &pointer, value, false);
                    }
                }
            }
            if record.tool == ToolId::GrokBuild {
                if let Some(models) = original["model"].as_object() {
                    for (alias, model) in models.iter().filter(|(alias, model)| {
                        alias.starts_with("tokenmarket/")
                            && same_route(&model["base_url"], &platform_route)
                    }) {
                        adopt_fields(receipt, &format!("/model/{}", escape(alias)), model, false);
                    }
                }
            }
            if record.tool == ToolId::DuMateEnterprise {
                for row in original.as_array().into_iter().flatten().filter(|row| {
                    row["modelKey"]
                        .as_str()
                        .is_some_and(|key| key.starts_with("personal:tokenmarket-"))
                        && same_route(&row["baseURL"], &platform_route)
                }) {
                    adopt_fields(
                        receipt,
                        &format!("/@modelKey={}", escape(row["modelKey"].as_str().unwrap())),
                        row,
                        true,
                    );
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connections::{
        configuration::{ConfigDocument, PreparedFile},
        DisconnectRequest,
    };
    use crate::desktop::{DesktopCredentialStore, DesktopIdentity};
    use std::sync::{Arc, Mutex};
    #[derive(Default)]
    struct Credentials(Mutex<Option<String>>);
    impl DesktopCredentialStore for Credentials {
        fn read(&self) -> Result<Option<String>> {
            Ok(self.0.lock().unwrap().clone())
        }
        fn write(&self, value: &str) -> Result<()> {
            *self.0.lock().unwrap() = Some(value.into());
            Ok(())
        }
        fn delete(&self) -> Result<()> {
            *self.0.lock().unwrap() = None;
            Ok(())
        }
    }
    fn exercise(tool: ToolId, name: &str, same_environment: bool) -> bool {
        let root = tempfile::tempdir().unwrap();
        let path = match tool {
            ToolId::MiniMaxCode => crate::adapters::minimax_data_dir(root.path()).join(name),
            ToolId::OhMyPi => crate::connections::oh_my_pi::config_path(root.path()),
            _ => root.path().join(name),
        };
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let gateway = "https://gateway.example";
        let model = |id| {
            vec![
                json!({"id":id,"model_types":["TEXT_GENERATION"],"context_window_tokens":64000,"max_output_tokens":8000}),
            ]
        };
        let prepared = ConfigDocument::parse(tool, &None, &path)
            .unwrap()
            .apply(
                if same_environment {
                    gateway
                } else {
                    "https://other-gateway.example"
                },
                "legacy-key-fixture",
                &model("legacy-model-fixture"),
            )
            .unwrap();
        let format = Format::for_tool(tool, &path).unwrap();
        let mut original = format.parse(Some(&prepared.bytes)).unwrap();
        let pointer = if let Some(rows) = original.as_array_mut() {
            if tool == ToolId::DuMateEnterprise {
                rows.push(json!({"modelKey":"manual-row","note":"manual-marker"}));
                "/@modelKey=manual-row"
            } else {
                rows.push(json!({"id":"manual-row","note":"manual-marker"}));
                "/@id=manual-row"
            }
        } else {
            original["custom"] = json!("manual-marker");
            "/custom"
        };
        std::fs::write(
            &path,
            format
                .render(Some(&prepared.bytes), &original, &[pointer.into()])
                .unwrap(),
        )
        .unwrap();
        for file in prepared.additional_files {
            std::fs::write(file.path, file.bytes).unwrap();
        }
        let before = Some(std::fs::read(&path).unwrap());
        let next = ConfigDocument::parse(tool, &before, &path)
            .unwrap()
            .apply(
                gateway,
                "current-key-fixture",
                &model("current-model-fixture"),
            )
            .unwrap();
        let mut files = vec![PreparedFile {
            path: path.clone(),
            before,
            bytes: next.bytes,
            fields: next.fields,
            created_containers: next.created_containers,
        }];
        files.extend(next.additional_files);
        let private = PrivateStorage::new(Arc::new(Credentials::default()));
        let mut receipts = vec![];
        for (index, file) in files.into_iter().enumerate() {
            let backup = root.path().join(format!("backup-{index}"));
            if let Some(bytes) = &file.before {
                private.write(&backup, bytes).unwrap();
            }
            std::fs::write(&file.path, file.bytes).unwrap();
            receipts.push(FileReceipt {
                path: file.path,
                existed: file.before.is_some(),
                backup_path: file.before.map(|_| backup),
                fields: file.fields,
                created_containers: file.created_containers,
            });
        }
        let identity = DesktopIdentity {
            id: "1".into(),
            name: "fixture".into(),
        };
        let record = ConnectionRecord {
            version: 2,
            connection_id: "fixture".into(),
            tool,
            device_id: "fixture".into(),
            key_id: "1".into(),
            owner: identity.clone(),
            bound_workspace: identity,
            protocol: next.protocol.into(),
            models: vec!["current-model-fixture".into()],
            model_snapshot: vec![],
            gateway_url: gateway.into(),
            platform_url: gateway.into(),
            created_at: String::new(),
            configuration_location: None,
            receipts,
            api_key: "current-key-fixture".into(),
        };
        let plan = super::super::prepare(&record, &DisconnectRequest::default(), &private).unwrap();
        assert!(plan.conflicts.is_empty(), "unexpected conflict for {tool}");
        let content = plan
            .changes
            .iter()
            .filter_map(|file| file.after.as_ref())
            .map(|bytes| String::from_utf8_lossy(bytes))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            content.contains("manual-marker"),
            "manual settings removed for {tool}"
        );
        let current_removed = !["current-key-fixture", "current-model-fixture"]
            .iter()
            .any(|value| content.contains(value));
        let legacy_retained = ["legacy-key-fixture", "legacy-model-fixture"]
            .iter()
            .all(|value| content.contains(value));
        current_removed
            && if same_environment {
                !["legacy-key-fixture", "legacy-model-fixture"]
                    .iter()
                    .any(|value| content.contains(value))
            } else {
                legacy_retained
            }
    }
    #[test]
    fn all_adapters_remove_adopted_platform_configuration_on_disconnect() {
        let cases = [
            (ToolId::ClaudeCode, "settings.json"),
            (ToolId::Codex, "config.toml"),
            (ToolId::WorkBuddy, "models.json"),
            (ToolId::DeepSeekHarness, "cordis.patch.yml"),
            (ToolId::OpenClaw, "openclaw.json"),
            (ToolId::OpenCode, "opencode.json"),
            (ToolId::GrokBuild, "config.toml"),
            (ToolId::Hermes, "config.yaml"),
            (ToolId::Pi, "models.json"),
            (ToolId::MiniMaxCode, "config.yaml"),
            (ToolId::OhMyPi, "models.yml"),
            (ToolId::KimiCode, "config.toml"),
            (ToolId::DuMate, "preference.json"),
            (ToolId::DuMateEnterprise, "models.json"),
        ];
        let failed = cases
            .into_iter()
            .flat_map(|(tool, path)| {
                [true, false]
                    .into_iter()
                    .filter_map(move |same_environment| {
                        (!exercise(tool, path, same_environment))
                            .then_some(format!("{tool} same_environment={same_environment}"))
                    })
            })
            .collect::<Vec<_>>();
        assert!(
            failed.is_empty(),
            "legacy platform configuration restored for: {}",
            failed.join(", ")
        );
    }
}
