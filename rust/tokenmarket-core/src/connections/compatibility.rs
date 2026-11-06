//! Read-only revalidation after any installation change, independent of its source.
use super::{configuration::ConfigDocument, configuration_io, ConnectionManager, ConnectionRecord};
use crate::{installation::updates::ToolUpdate, inventory::ToolInventory, ToolId};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionCompatibility {
    pub state: &'static str,
    pub message: &'static str,
    pub key_id: String,
    pub connection_id: String,
}

fn assessment(record: &ConnectionRecord, state: &'static str) -> ConnectionCompatibility {
    ConnectionCompatibility {
        state,
        message: match state {
            "reconnect_required" => "工具安装已变化，请重新接入以更新适配",
            "unsupported" => "当前版本暂不支持接入，请联系管理员",
            "unknown" => "接入兼容性暂无法确认，请联系管理员",
            _ => "原接入配置仍符合当前适配规则",
        },
        key_id: record.key_id.clone(),
        connection_id: record.connection_id.clone(),
    }
}

impl ConnectionManager {
    pub fn assess_tool_updates(
        &self,
        inventory: &ToolInventory,
        updates: &mut [ToolUpdate],
    ) -> anyhow::Result<()> {
        if self.list()?.is_empty() {
            return Ok(());
        }
        // Migration only adds observation metadata to our encrypted record;
        // it must not race a reconnect in this or another client process.
        let Ok(_gate) = self.gate.try_lock() else {
            return Ok(());
        };
        let _operation = self.operation_lock()?;
        let mut records = self.list()?;
        let mut migrated = false;
        for update in updates {
            let Some(record) = records
                .iter_mut()
                .find(|record| record.tool == update.tool && self.matches_environment(record))
            else {
                continue;
            };
            let mut status = assess(record, inventory, update.current_version.as_deref());
            let missing_baseline = record
                .configuration_location
                .as_ref()
                .is_some_and(|location| location.installation.is_none());
            if missing_baseline
                && status
                    .as_ref()
                    .is_some_and(|status| matches!(status.state, "observed" | "compatible"))
            {
                if let (Some(mut snapshot), Some(version), Some(location)) = (
                    inventory.installation_snapshot(record.tool),
                    update.current_version.as_ref(),
                    record.configuration_location.as_mut(),
                ) {
                    snapshot.version = Some(version.clone());
                    location.installation = Some(snapshot);
                    migrated = true;
                    status = None;
                }
            }
            // An inconclusive assessment must leave the existing connection alone.
            update.connection_compatibility = status.filter(|status| status.state != "unknown");
        }
        if migrated {
            self.private.write(
                &self.directory.join("connections.json"),
                &serde_json::to_vec_pretty(&records)?,
            )?;
        }
        Ok(())
    }
}

fn assess(
    record: &ConnectionRecord,
    inventory: &ToolInventory,
    current_version: Option<&str>,
) -> Option<ConnectionCompatibility> {
    let current = inventory.installation_snapshot(record.tool)?;
    let previous = record
        .configuration_location
        .as_ref()
        .and_then(|location| location.installation.as_ref());
    if previous.is_some_and(|previous| {
        previous.same_installation(&current)
            && previous
                .version
                .as_deref()
                .zip(current_version)
                .is_none_or(|(old, new)| {
                    crate::installation::updates::compare_versions(old, new)
                        == Some(std::cmp::Ordering::Equal)
                })
    }) {
        return None;
    }
    let result = |state| Some(assessment(record, state));
    let Some(version) = current_version else {
        return result("unknown");
    };
    let major = crate::installation::opencode::major_version(version);
    if record.tool == ToolId::OpenCode && major.is_some_and(|major| major >= 2)
        || record.tool == ToolId::KimiCode && crate::installation::legacy_kimi_version(version)
    {
        return result("unsupported");
    }
    if record.tool == ToolId::QwenWork {
        let Some(application) = inventory.application_path(record.tool) else {
            return result("unknown");
        };
        return result(
            match crate::qwenwork_runtime::assess_installed_runtime(
                &application,
                &inventory.config_path(record.tool),
            ) {
                Ok(true) => "reconnect_required",
                Ok(false) => "compatible",
                Err(error) if error.chain().any(|cause| cause.is::<std::io::Error>()) => "unknown",
                Err(error)
                    if error.chain().any(|cause| {
                        cause
                            .to_string()
                            .contains(crate::qwenwork_runtime::ADAPTATION_REQUIRED)
                    }) =>
                {
                    "unsupported"
                }
                Err(_) => "unknown",
            },
        );
    }
    // Reuse ownership checks: a version change never silently rewrites config.
    if super::restore::verify_applied(record).is_err() {
        return result("reconnect_required");
    }
    if record.tool == ToolId::DuMate {
        let Some(application) = inventory.application_path(record.tool) else {
            return result("unknown");
        };
        if let Err(error) =
            crate::dumate::unified::validate_engine(&application, inventory.platform())
        {
            return result(if error.chain().any(|cause| cause.is::<std::io::Error>()) {
                "unknown"
            } else {
                "unsupported"
            });
        }
    }
    let Some(location) = record.configuration_location.as_ref() else {
        return result("unknown");
    };
    let path = inventory.config_path(record.tool);
    if configuration_io::read(&path)
        .and_then(|before| {
            ConfigDocument::parse_for_connection(record.tool, &before, &path, true).map(|_| ())
        })
        .is_err()
    {
        return result("unknown");
    }
    if record.tool == ToolId::DuMate {
        return result("compatible");
    }
    // Generic adapters retain configuration within a known major family.
    // An unverified new major is not declared unsupported.
    let old_major = location
        .installation
        .as_ref()
        .and_then(|snapshot| snapshot.version.as_deref())
        .and_then(crate::installation::opencode::major_version);
    if previous.is_none() {
        // A first observation is not evidence that the user upgraded. Record
        // it as a baseline, without inventing a version-change warning.
        return result("observed");
    }
    result(if major.is_some() && major == old_major {
        "compatible"
    } else {
        "unknown"
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::{HostPlatform, InstallationEnvironment};
    use serde_json::json;

    fn fixture(
        tool: ToolId,
        version: &str,
    ) -> (tempfile::TempDir, ToolInventory, ConnectionRecord) {
        let root = tempfile::tempdir().unwrap();
        let bin = root.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let command = bin.join(tool.command());
        std::fs::write(&command, "inert installation fixture").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&command, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let inventory = ToolInventory::new(InstallationEnvironment {
            platform: HostPlatform::MacOs,
            home: root.path().into(),
            command_directories: vec![bin],
            application_directories: vec![],
            executable_extensions: vec![],
        });
        inventory
            .installation_snapshot(tool)
            .unwrap()
            .remember_version(version);
        let path = inventory.config_path(tool);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            if tool == ToolId::OpenCode {
                "{\"model\":\"fixture-model\"}"
            } else {
                "model = \"fixture-model\"\n"
            },
        )
        .unwrap();
        let mut record: ConnectionRecord = serde_json::from_value(json!({
            "version":2,"connection_id":"fixture-connection","tool":tool,
            "device_id":"fixture-device","key_id":"1","owner":{"id":"1","name":"Fixture"},
            "bound_workspace":{"id":"2","name":"Workspace"},"protocol":"OPENAI_CHAT_COMPLETIONS",
            "models":[],"model_snapshot":[],"gateway_url":"https://fixture.invalid",
            "platform_url":"https://console.fixture.invalid","created_at":"2026-11-06T00:00:00Z",
            "receipts":[],"api_key":"fixture-secret"
        }))
        .unwrap();
        record.configuration_location = Some(inventory.configuration_location(tool).unwrap());
        record.receipts.push(super::super::FileReceipt {
            path,
            existed: true,
            backup_path: None,
            fields: vec![super::super::FieldReceipt {
                pointer: "/model".into(),
                before: None,
                applied: json!("fixture-model"),
                retired: false,
            }],
            created_containers: vec![],
        });
        (root, inventory, record)
    }

    #[test]
    fn external_cli_updates_are_rechecked_without_connecting_or_executing_the_tool() {
        let (root, inventory, record) = fixture(ToolId::Codex, "0.166.0");
        assert!(assess(&record, &inventory, Some("0.166.0")).is_none());
        std::fs::write(
            root.path().join("bin/codex"),
            "externally replaced installation fixture",
        )
        .unwrap();
        assert_eq!(
            assess(&record, &inventory, Some("0.167.0")).unwrap().state,
            "compatible"
        );
        assert_eq!(
            assess(&record, &inventory, Some("2.0.0")).unwrap().state,
            "unknown"
        );
        assert_eq!(assess(&record, &inventory, None).unwrap().state, "unknown");
        assert!(!root.path().join(".tokenmarket").exists());
    }

    #[test]
    fn version_changes_are_detected_even_when_file_metadata_is_unchanged() {
        let (_root, inventory, record) = fixture(ToolId::Codex, "0.166.0");
        assert_eq!(
            assess(&record, &inventory, Some("2.0.0")).unwrap().state,
            "unknown"
        );
    }

    #[test]
    fn known_unsupported_upgrades_and_downgrades_are_distinct_from_unknown_versions() {
        for (tool, old, current) in [
            (ToolId::OpenCode, "1.19.5", "2.0.0"),
            (ToolId::KimiCode, "2.4.1", "1.9.0"),
        ] {
            let (_root, inventory, record) = fixture(tool, old);
            let status = assess(&record, &inventory, Some(current)).unwrap();
            assert_eq!(status.state, "unsupported");
            assert_eq!(status.message, "当前版本暂不支持接入，请联系管理员");
        }
    }

    #[derive(Default)]
    struct Credentials(std::sync::Mutex<Option<String>>);
    impl crate::desktop::DesktopCredentialStore for Credentials {
        fn read(&self) -> anyhow::Result<Option<String>> {
            Ok(self.0.lock().unwrap().clone())
        }
        fn write(&self, value: &str) -> anyhow::Result<()> {
            *self.0.lock().unwrap() = Some(value.into());
            Ok(())
        }
        fn delete(&self) -> anyhow::Result<()> {
            *self.0.lock().unwrap() = None;
            Ok(())
        }
    }

    struct Versions(std::sync::Mutex<String>);
    impl crate::installation::InstallExecutor for Versions {
        fn execute(
            &self,
            _: &crate::installation::InstallStep,
            _: &std::sync::atomic::AtomicBool,
            _: &mut dyn FnMut(String),
        ) -> anyhow::Result<()> {
            panic!("compatibility checks must not install or execute tools")
        }
        fn version(&self, _: &std::path::Path) -> anyhow::Result<String> {
            Ok(self.0.lock().unwrap().clone())
        }
    }

    #[test]
    fn historical_records_gain_a_baseline_without_a_false_upgrade_warning_and_offline_updates_are_detected(
    ) {
        let (root, inventory, mut record) = fixture(ToolId::Codex, "0.166.0");
        record.configuration_location.as_mut().unwrap().installation = None;
        let manager = ConnectionManager::with_credentials(
            root.path().join("state"),
            record.platform_url.clone(),
            record.gateway_url.clone(),
            std::sync::Arc::new(Credentials::default()),
        );
        manager
            .private
            .write(
                &manager.directory.join("connections.json"),
                &serde_json::to_vec(&vec![record]).unwrap(),
            )
            .unwrap();
        let executor = std::sync::Arc::new(Versions(std::sync::Mutex::new("0.166.0".into())));
        let installer = crate::installation::InstallationManager::new(
            InstallationEnvironment {
                platform: HostPlatform::MacOs,
                home: root.path().into(),
                command_directories: vec![root.path().join("bin")],
                application_directories: vec![],
                executable_extensions: vec![],
            },
            "aarch64".into(),
            executor.clone(),
        );
        let mut updates =
            vec![installer.inspect_update(ToolId::Codex, Err(anyhow::anyhow!("offline")))];
        manager
            .assess_tool_updates(&inventory, &mut updates)
            .unwrap();
        assert!(updates[0].connection_compatibility.is_none());
        assert!(manager.list().unwrap()[0]
            .configuration_location
            .as_ref()
            .unwrap()
            .installation
            .is_some());
        std::fs::write(
            root.path().join("bin/codex"),
            "new externally installed CLI",
        )
        .unwrap();
        *executor.0.lock().unwrap() = "0.167.0".into();
        let mut updates =
            vec![installer.inspect_update(ToolId::Codex, Err(anyhow::anyhow!("offline")))];
        manager
            .assess_tool_updates(&inventory, &mut updates)
            .unwrap();
        assert_eq!(updates[0].state, "check_failed");
        assert_eq!(
            updates[0].connection_compatibility.as_ref().unwrap().state,
            "compatible"
        );
        // An unverified major or unreadable version leaves the saved connection
        // and configuration intact and emits no actionable assessment.
        let saved = std::fs::read(manager.directory.join("connections.json")).unwrap();
        for version in [Some("2.0.0"), None] {
            updates[0].current_version = version.map(str::to_owned);
            manager
                .assess_tool_updates(&inventory, &mut updates)
                .unwrap();
            assert!(updates[0].connection_compatibility.is_none());
            assert_eq!(
                std::fs::read(manager.directory.join("connections.json")).unwrap(),
                saved
            );
        }
        assert_eq!(
            std::fs::read_to_string(inventory.config_path(ToolId::Codex)).unwrap(),
            "model = \"fixture-model\"\n"
        );
    }
}
