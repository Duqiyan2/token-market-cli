mod support;
use anyhow::Result;
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
    version: String,
    steps: Mutex<Vec<InstallStep>>,
    waiting: AtomicBool,
}
impl InstallExecutor for FixtureInstaller {
    fn execute(
        &self,
        step: &InstallStep,
        cancelled: &AtomicBool,
        output: &mut dyn FnMut(String),
    ) -> Result<()> {
        self.steps.lock().unwrap().push(step.clone());
        output("official archive verified".into());
        if !step.program.is_empty() {
            executable(&self.executable);
            while self.waiting.load(Ordering::SeqCst) && !cancelled.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        Ok(())
    }
    fn version(&self, _: &Path) -> Result<String> {
        Ok(self.version.clone())
    }
}
fn executable(path: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "fixture; never execute").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}
fn fixture(
    root: &Path,
    platform: HostPlatform,
    version: &str,
) -> (InstallationManager, Arc<FixtureInstaller>) {
    let environment = InstallationEnvironment {
        platform,
        home: root.into(),
        command_directories: vec![root.join("bin")],
        application_directories: vec![],
        executable_extensions: vec![".EXE".into()],
    };
    let executor = Arc::new(FixtureInstaller {
        executable: root.join("bin").join(if platform == HostPlatform::Windows {
            "opencode.exe"
        } else {
            "opencode"
        }),
        version: version.into(),
        steps: Mutex::new(vec![]),
        waiting: AtomicBool::new(false),
    });
    (
        InstallationManager::new(environment, "x86_64".into(), executor.clone()),
        executor,
    )
}
fn finished(manager: &InstallationManager) -> tokenmarket_core::installation::InstallStatus {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let status = manager.status(ToolId::OpenCode).unwrap();
        if status.stage.is_terminal() {
            return status;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
}
#[test]
fn installs_an_official_opencode_v1_archive_and_detects_it_before_reporting_success() {
    let root = tempfile::tempdir().unwrap();
    let (manager, executor) = fixture(root.path(), HostPlatform::MacOs, "1.19.5");
    assert!(manager.availability(ToolId::OpenCode).supported);
    manager.start(ToolId::OpenCode).unwrap();
    let state = finished(&manager);
    assert_eq!(state.stage, InstallStage::Succeeded);
    assert!(state.installed);
    assert_eq!(state.version.as_deref(), Some("1.19.5"));
    let steps = executor.steps.lock().unwrap();
    assert_eq!(steps.len(), 1);
    let command = steps[0].args.join(" ");
    assert!(command.contains("https://opencode.ai/install"));
    assert!(command.contains("opencode-ai@latest"));
    assert!(!root
        .path()
        .join(".tokenmarket/desktop-connections")
        .exists());
}

#[test]
fn an_existing_stable_v1_is_reused_without_upgrading_to_the_catalogue_pin() {
    let root = tempfile::tempdir().unwrap();
    let (manager, executor) = fixture(root.path(), HostPlatform::MacOs, "1.18.9");
    executable(&executor.executable);
    manager.start(ToolId::OpenCode).unwrap();
    assert_eq!(finished(&manager).stage, InstallStage::Succeeded);
    assert!(executor.steps.lock().unwrap().is_empty());
}

#[cfg(unix)]
#[test]
fn installer_result_without_a_valid_version_does_not_publish_a_connection() {
    let root = tempfile::tempdir().unwrap();
    let (manager, _executor) = fixture(root.path(), HostPlatform::MacOs, "unknown");
    manager.start(ToolId::OpenCode).unwrap();
    assert_eq!(finished(&manager).stage, InstallStage::Failed);
    assert!(!root
        .path()
        .join(".tokenmarket/desktop-connections")
        .exists());
}

#[tokio::test]
async fn an_active_install_blocks_connect_even_after_its_binary_becomes_visible() {
    use tokenmarket_core::{connections::ConnectContext, inventory::ToolInventory};
    let root = tempfile::tempdir().unwrap();
    let (installer, executor) = fixture(root.path(), HostPlatform::MacOs, "1.19.5");
    executor.waiting.store(true, Ordering::SeqCst);
    installer.start(ToolId::OpenCode).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while !executor.executable.exists() {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let inventory = ToolInventory::new(InstallationEnvironment {
        platform: HostPlatform::MacOs,
        home: root.path().into(),
        command_directories: vec![root.path().join("bin")],
        application_directories: vec![],
        executable_extensions: vec![],
    });
    let connection = support::manager(
        root.path().join("state"),
        "http://127.0.0.1:1".into(),
        "http://127.0.0.1:1".into(),
    );
    let config = root.path().join("opencode.jsonc");
    std::fs::write(&config, "{\"model\":\"manual/original\"}").unwrap();
    let error = connection
        .connect(
            ConnectContext {
                session_token: "fixture-session".into(),
                owner_id: "owner".into(),
                workspace_id: "space".into(),
            },
            ToolId::OpenCode,
            &inventory,
            config.clone(),
        )
        .await
        .unwrap_err();
    installer.cancel(ToolId::OpenCode).unwrap();
    assert_eq!(finished(&installer).stage, InstallStage::Cancelled);
    let message = error.to_string();
    assert!(
        message.contains("正在安装") || message.contains("安装尚未完成"),
        "unexpected installation guard error: {message}"
    );
    assert_eq!(
        std::fs::read_to_string(config).unwrap(),
        "{\"model\":\"manual/original\"}"
    );
    assert!(connection.list().unwrap().is_empty());
}
