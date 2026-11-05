use anyhow::{bail, Result};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokenmarket_core::{
    installation::{
        InstallExecutor, InstallStage, InstallStatus, InstallStep, InstallationManager,
    },
    inventory::{HostPlatform, InstallationEnvironment, ToolInventory},
    ToolId,
};

struct Installer {
    executable: PathBuf,
    version: String,
    steps: Mutex<Vec<InstallStep>>,
    fail: AtomicBool,
    wait: AtomicBool,
    probes: AtomicUsize,
}
impl InstallExecutor for Installer {
    fn execute(
        &self,
        step: &InstallStep,
        cancelled: &AtomicBool,
        output: &mut dyn FnMut(String),
    ) -> Result<()> {
        self.steps.lock().unwrap().push(step.clone());
        while self.wait.load(Ordering::SeqCst) && !cancelled.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(5));
        }
        if cancelled.load(Ordering::SeqCst) {
            bail!("cancelled")
        }
        if self.fail.load(Ordering::SeqCst) {
            bail!("official download failed")
        }
        if !step.program.is_empty() {
            executable(&self.executable);
            output("official native installation finished".into());
        }
        Ok(())
    }
    fn version(&self, _: &Path) -> Result<String> {
        self.probes.fetch_add(1, Ordering::SeqCst);
        Ok(self.version.clone())
    }
}
fn executable(path: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "fixture executable; never run").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}
fn environment(home: &Path, platform: HostPlatform) -> InstallationEnvironment {
    InstallationEnvironment {
        platform,
        home: home.into(),
        command_directories: vec![],
        application_directories: vec![],
        executable_extensions: vec![".EXE".into(), ".CMD".into()],
    }
}
fn fixture(
    home: &Path,
    platform: HostPlatform,
    tool: ToolId,
    version: &str,
) -> (InstallationManager, Arc<Installer>) {
    let command = match (tool, platform) {
        (ToolId::ClaudeCode, HostPlatform::Windows) => ".local/bin/claude.exe",
        (ToolId::ClaudeCode, _) => ".local/bin/claude",
        (ToolId::Codex, HostPlatform::Windows) => ".tokenmarket/tools/codex/bin/codex.exe",
        (ToolId::Codex, _) => ".tokenmarket/tools/codex/bin/codex",
        _ => unreachable!(),
    };
    let installer = Arc::new(Installer {
        executable: home.join(command),
        version: version.into(),
        steps: Mutex::new(vec![]),
        fail: AtomicBool::new(false),
        wait: AtomicBool::new(false),
        probes: AtomicUsize::new(0),
    });
    (
        InstallationManager::new(
            environment(home, platform),
            "x86_64".into(),
            installer.clone(),
        ),
        installer,
    )
}
fn finished(manager: &InstallationManager, tool: ToolId) -> InstallStatus {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let status = manager.status(tool).unwrap();
        if status.stage.is_terminal() {
            return status;
        }
        assert!(Instant::now() < deadline, "installation did not finish");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn claude_and_codex_follow_upstream_install_routes_and_preserve_configuration() {
    for platform in [
        HostPlatform::MacOs,
        HostPlatform::Unix,
        HostPlatform::Windows,
    ] {
        for (tool, version) in [(ToolId::ClaudeCode, "2.1.309"), (ToolId::Codex, "0.166.0")] {
            let root = tempfile::tempdir().unwrap();
            let home = root.path().join("user's & home");
            let config = home.join("existing-agent-config");
            std::fs::create_dir_all(&home).unwrap();
            std::fs::write(&config, "original configuration").unwrap();
            let (manager, installer) = fixture(&home, platform, tool, version);
            let availability = manager.availability(tool);
            assert!(availability.supported, "{tool}: {:?}", availability.reason);
            assert_eq!(availability.version.as_deref(), Some("latest"));
            manager.start(tool).unwrap();
            let status = finished(&manager, tool);
            assert_eq!(status.stage, InstallStage::Succeeded, "{}", status.detail);
            assert!(status.installed);
            assert!(installer.executable.exists());
            assert_eq!(
                std::fs::read_to_string(&config).unwrap(),
                "original configuration"
            );
            assert!(!home.join(".tokenmarket/desktop-connections").exists());
            assert!(!home.join(".tokenmarket/client-keys").exists());
            let steps = installer.steps.lock().unwrap();
            assert_eq!(steps.len(), 1);
            let command = steps[0].args.join(" ");
            if tool == ToolId::ClaudeCode && platform != HostPlatform::Windows {
                assert!(command.contains("https://claude.ai/install.sh"));
            } else {
                assert!(command.contains("@latest"));
            }
            assert!(!command.contains("--release"));
        }
    }
}

#[test]
fn compatible_installs_are_reused_and_unknown_versions_are_not_upgraded() {
    for (tool, version) in [(ToolId::ClaudeCode, "2.1.309"), (ToolId::Codex, "0.166.0")] {
        for compatible in [true, false] {
            let root = tempfile::tempdir().unwrap();
            let reported = if compatible { version } else { "unknown" };
            let (manager, installer) = fixture(root.path(), HostPlatform::MacOs, tool, reported);
            executable(&installer.executable);
            manager.start(tool).unwrap();
            let status = finished(&manager, tool);
            assert_eq!(
                status.stage,
                if compatible {
                    InstallStage::Succeeded
                } else {
                    InstallStage::Failed
                }
            );
            assert!(installer.steps.lock().unwrap().is_empty());
            assert_eq!(
                std::fs::read_to_string(&installer.executable).unwrap(),
                "fixture executable; never run"
            );
            if !compatible {
                assert!(status.detail.contains("未自动升级"));
            }
        }
    }
}

#[test]
fn native_installers_retain_failures_support_retry_and_cancel_without_configuration_changes() {
    for (tool, version) in [(ToolId::ClaudeCode, "2.1.309"), (ToolId::Codex, "0.166.0")] {
        let root = tempfile::tempdir().unwrap();
        let (manager, installer) = fixture(root.path(), HostPlatform::MacOs, tool, version);
        let runtime = root
            .path()
            .join(".codex/packages/standalone/retained-runtime");
        std::fs::create_dir_all(runtime.parent().unwrap()).unwrap();
        std::fs::write(&runtime, "existing runtime helper").unwrap();
        installer.fail.store(true, Ordering::SeqCst);
        manager.start(tool).unwrap();
        let status = finished(&manager, tool);
        assert_eq!(status.stage, InstallStage::Failed);
        assert!(status.detail.contains("official download failed"));
        installer.fail.store(false, Ordering::SeqCst);
        manager.start(tool).unwrap();
        assert_eq!(finished(&manager, tool).stage, InstallStage::Succeeded);
        std::fs::remove_file(&installer.executable).unwrap();
        installer.wait.store(true, Ordering::SeqCst);
        manager.start(tool).unwrap();
        manager.cancel(tool).unwrap();
        assert_eq!(finished(&manager, tool).stage, InstallStage::Cancelled);
        assert_eq!(
            std::fs::read_to_string(runtime).unwrap(),
            "existing runtime helper"
        );
        assert!(!root
            .path()
            .join(".tokenmarket/desktop-connections")
            .exists());
    }
}

#[test]
fn inventory_exposes_native_install_actions_without_executing_version_probes() {
    let root = tempfile::tempdir().unwrap();
    let (manager, installer) = fixture(root.path(), HostPlatform::MacOs, ToolId::Codex, "0.166.0");
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    for tool in [ToolId::ClaudeCode, ToolId::Codex] {
        let entry = inventory
            .list()
            .unwrap()
            .into_iter()
            .find(|entry| entry.id == tool)
            .unwrap();
        assert!(!entry.installed);
        assert!(entry.installation.supported);
        assert!(!entry.can_connect);
    }
    assert!(manager.installed_version(ToolId::Codex).unwrap().is_none());
    let unsupported = InstallationManager::new(
        environment(root.path(), HostPlatform::Unix),
        "armv7".into(),
        installer.clone(),
    );
    for tool in [ToolId::ClaudeCode, ToolId::Codex] {
        let availability = unsupported.availability(tool);
        assert!(!availability.supported);
        assert!(availability.reason.unwrap().contains("armv7"));
        assert!(unsupported.start(tool).is_err());
    }
    assert_eq!(installer.probes.load(Ordering::SeqCst), 0);
    assert!(installer.steps.lock().unwrap().is_empty());
}
