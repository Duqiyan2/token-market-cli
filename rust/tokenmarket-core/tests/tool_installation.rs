use anyhow::{bail, Result};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokenmarket_core::{
    installation::{InstallExecutor, InstallStage, InstallStep, InstallationManager},
    inventory::{HostPlatform, InstallationEnvironment},
    ToolId,
};

struct FixtureInstaller {
    executable: PathBuf,
    fail: AtomicBool,
    no_install: AtomicBool,
    wait: AtomicBool,
    commands: Mutex<Vec<InstallStep>>,
    version: Mutex<String>,
}
impl InstallExecutor for FixtureInstaller {
    fn execute(
        &self,
        step: &InstallStep,
        cancelled: &AtomicBool,
        output: &mut dyn FnMut(String),
    ) -> Result<()> {
        self.commands.lock().unwrap().push(step.clone());
        output("准备工具运行依赖".into());
        while self.wait.load(Ordering::SeqCst) && !cancelled.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(5));
        }
        if cancelled.load(Ordering::SeqCst) {
            bail!("cancelled")
        }
        if self.fail.load(Ordering::SeqCst) {
            bail!("fixture dependency download failed")
        }
        if !self.no_install.load(Ordering::SeqCst) {
            std::fs::create_dir_all(self.executable.parent().unwrap())?;
            std::fs::write(&self.executable, "fixture; never execute")?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&self.executable, std::fs::Permissions::from_mode(0o755))?;
            }
        }
        Ok(())
    }
    fn version(&self, _command: &Path) -> Result<String> {
        Ok(self.version.lock().unwrap().clone())
    }
}
fn fixture(root: &Path) -> (InstallationManager, Arc<FixtureInstaller>) {
    fixture_on(root, HostPlatform::MacOs)
}
fn fixture_on(root: &Path, platform: HostPlatform) -> (InstallationManager, Arc<FixtureInstaller>) {
    let executor = Arc::new(FixtureInstaller {
        executable: root.join("bin").join(if platform == HostPlatform::Windows {
            "openclaw.cmd"
        } else {
            "openclaw"
        }),
        fail: AtomicBool::new(false),
        no_install: AtomicBool::new(false),
        wait: AtomicBool::new(false),
        commands: Mutex::new(vec![]),
        version: Mutex::new("2026.10.9".into()),
    });
    let environment = InstallationEnvironment {
        platform,
        home: root.into(),
        command_directories: vec![root.join("bin")],
        application_directories: vec![],
        executable_extensions: vec![".CMD".into()],
    };
    (
        InstallationManager::new(environment, "aarch64".into(), executor.clone()),
        executor,
    )
}
fn finished(manager: &InstallationManager) -> tokenmarket_core::installation::InstallStatus {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let status = manager.status(ToolId::OpenClaw).unwrap();
        if status.stage.is_terminal() {
            return status;
        }
        assert!(Instant::now() < deadline, "installation did not finish");
        std::thread::sleep(Duration::from_millis(5));
    }
}
#[test]
fn installs_openclaw_and_reports_success_only_after_detecting_the_executable() {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join(".openclaw/openclaw.json");
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    std::fs::write(&config, "original configuration").unwrap();
    let (manager, executor) = fixture(root.path());
    assert!(manager.availability(ToolId::OpenClaw).supported);
    manager.start(ToolId::OpenClaw).unwrap();
    let status = finished(&manager);
    assert_eq!(status.stage, InstallStage::Succeeded);
    assert!(status.installed);
    assert_eq!(
        std::fs::read_to_string(config).unwrap(),
        "original configuration"
    );
    assert!(!root
        .path()
        .join(".tokenmarket/desktop-connections")
        .exists());
    let commands = executor.commands.lock().unwrap();
    assert_eq!(commands.len(), 1);
    assert!(commands[0].args.join(" ").contains("openclaw@latest"));
}

#[test]
fn installer_success_without_an_executable_is_a_retryable_failure() {
    let root = tempfile::tempdir().unwrap();
    let (manager, executor) = fixture(root.path());
    executor.no_install.store(true, Ordering::SeqCst);
    manager.start(ToolId::OpenClaw).unwrap();
    let status = finished(&manager);
    assert_eq!(status.stage, InstallStage::Failed);
    assert!(!status.installed);
    assert!(status.detail.contains("未检测到"));
    executor.no_install.store(false, Ordering::SeqCst);
    manager.start(ToolId::OpenClaw).unwrap();
    assert_eq!(finished(&manager).stage, InstallStage::Succeeded);
}

#[test]
fn failure_retains_output_and_can_be_retried_without_touching_connections() {
    let root = tempfile::tempdir().unwrap();
    let (manager, executor) = fixture(root.path());
    executor.fail.store(true, Ordering::SeqCst);
    manager.start(ToolId::OpenClaw).unwrap();
    let status = finished(&manager);
    assert_eq!(status.stage, InstallStage::Failed);
    assert!(status.detail.contains("dependency download failed"));
    assert!(status.output.iter().any(|line| line.contains("运行依赖")));
    executor.fail.store(false, Ordering::SeqCst);
    manager.start(ToolId::OpenClaw).unwrap();
    assert_eq!(finished(&manager).stage, InstallStage::Succeeded);
    assert!(!root
        .path()
        .join(".tokenmarket/desktop-connections")
        .exists());
}

#[test]
fn cancelling_an_install_preserves_prepared_dependencies_and_rejects_a_duplicate() {
    let root = tempfile::tempdir().unwrap();
    let (manager, executor) = fixture(root.path());
    let dependency = root.path().join("prepared-runtime");
    std::fs::write(&dependency, "existing runtime").unwrap();
    executor.wait.store(true, Ordering::SeqCst);
    manager.start(ToolId::OpenClaw).unwrap();
    assert!(manager.start(ToolId::OpenClaw).is_err());
    manager.cancel(ToolId::OpenClaw).unwrap();
    assert_eq!(finished(&manager).stage, InstallStage::Cancelled);
    assert_eq!(
        std::fs::read_to_string(dependency).unwrap(),
        "existing runtime"
    );
}

#[test]
fn reuse_of_an_existing_compatible_install_never_runs_an_installer() {
    let root = tempfile::tempdir().unwrap();
    let (manager, executor) = fixture(root.path());
    std::fs::create_dir_all(executor.executable.parent().unwrap()).unwrap();
    std::fs::write(&executor.executable, "existing tool").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&executor.executable, std::fs::Permissions::from_mode(0o755))
            .unwrap();
    }
    manager.start(ToolId::OpenClaw).unwrap();
    assert_eq!(finished(&manager).stage, InstallStage::Succeeded);
    assert!(executor.commands.lock().unwrap().is_empty());
}

#[test]
fn separate_client_instances_cannot_install_the_same_tool_concurrently() {
    let root = tempfile::tempdir().unwrap();
    let (first, executor) = fixture(root.path());
    let (second, _) = fixture(root.path());
    executor.wait.store(true, Ordering::SeqCst);
    first.start(ToolId::OpenClaw).unwrap();
    assert!(second.start(ToolId::OpenClaw).is_err());
    first.cancel(ToolId::OpenClaw).unwrap();
    assert_eq!(finished(&first).stage, InstallStage::Cancelled);
}

#[test]
fn an_existing_version_is_reused_without_an_implicit_upgrade() {
    let root = tempfile::tempdir().unwrap();
    let (manager, executor) = fixture(root.path());
    std::fs::create_dir_all(executor.executable.parent().unwrap()).unwrap();
    std::fs::write(&executor.executable, "original installation").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&executor.executable, std::fs::Permissions::from_mode(0o755))
            .unwrap();
    }
    *executor.version.lock().unwrap() = "2026.9.1".into();
    manager.start(ToolId::OpenClaw).unwrap();
    let status = finished(&manager);
    assert_eq!(status.stage, InstallStage::Succeeded);
    assert!(executor.commands.lock().unwrap().is_empty());
    assert_eq!(
        std::fs::read_to_string(&executor.executable).unwrap(),
        "original installation"
    );
}

#[test]
fn windows_install_uses_system_npm_without_private_runtime_or_path_mutations() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("Quoted ' & home");
    let (manager, executor) = fixture_on(&home, HostPlatform::Windows);
    manager.start(ToolId::OpenClaw).unwrap();
    assert_eq!(finished(&manager).stage, InstallStage::Succeeded);
    let steps = executor.commands.lock().unwrap();
    let script = steps.last().unwrap().args.last().unwrap();
    assert!(script.contains("openclaw@latest"));
    assert!(!script.contains("-NodeOnly"));
    assert!(!script.contains("gateway"));
    assert!(!script.contains("setx"));
}

#[test]
fn unsupported_architecture_is_explained_without_running_an_installer() {
    let root = tempfile::tempdir().unwrap();
    let (_, executor) = fixture(root.path());
    let environment = InstallationEnvironment {
        platform: HostPlatform::Unix,
        home: root.path().into(),
        command_directories: vec![],
        application_directories: vec![],
        executable_extensions: vec![],
    };
    let manager = InstallationManager::new(environment, "armv7".into(), executor.clone());
    let available = manager.availability(ToolId::OpenClaw);
    assert!(!available.supported);
    assert!(available.reason.unwrap().contains("armv7"));
    assert!(manager.start(ToolId::OpenClaw).is_err());
    assert!(executor.commands.lock().unwrap().is_empty());
    assert!(!root.path().join(".tokenmarket").exists());
}
