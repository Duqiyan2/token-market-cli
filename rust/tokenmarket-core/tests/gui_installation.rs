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
    installation::{
        InstallExecutor, InstallStage, InstallStatus, InstallStep, InstallationManager,
    },
    inventory::{HostPlatform, InstallationEnvironment, ToolInventory},
    ToolId,
};

#[derive(Default)]
struct GuiInstaller {
    steps: Mutex<Vec<InstallStep>>,
    fail: AtomicBool,
}
impl InstallExecutor for GuiInstaller {
    fn execute(
        &self,
        step: &InstallStep,
        _: &AtomicBool,
        output: &mut dyn FnMut(String),
    ) -> Result<()> {
        self.steps.lock().unwrap().push(step.clone());
        if self.fail.load(Ordering::SeqCst) {
            bail!("official package download failed")
        }
        output("official installer opened; waiting for native user steps".into());
        Ok(())
    }
    fn version(&self, _: &Path) -> Result<String> {
        panic!("GUI installation must not launch the application to probe its version")
    }
    fn application_version(&self, application: &Path) -> Result<Option<String>> {
        if application.is_file() {
            // Windows ProductVersion is an external metadata boundary.
            return Ok(Some("5.9.3".into()));
        }
        tokenmarket_core::installation::NativeInstallExecutor.application_version(application)
    }
}
fn environment(home: &Path, platform: HostPlatform) -> InstallationEnvironment {
    InstallationEnvironment {
        platform,
        home: home.into(),
        command_directories: vec![home.join("bin")],
        application_directories: vec![home.join("Applications")],
        executable_extensions: vec![".EXE".into(), ".CMD".into()],
    }
}
fn until(
    manager: &InstallationManager,
    tool: ToolId,
    predicate: impl Fn(&InstallStatus) -> bool,
) -> InstallStatus {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let status = manager.status(tool).unwrap();
        if predicate(&status) {
            return status;
        }
        assert!(
            Instant::now() < deadline,
            "unexpected installation state: {status:?}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn application(home: &Path, name: &str, version: &str) -> PathBuf {
    let bundle = home.join("Applications").join(format!("{name}.app"));
    let executable = bundle.join("Contents/MacOS/app");
    std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
    std::fs::write(&executable, "application fixture; never execute").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let mut info = plist::Dictionary::new();
    info.insert(
        "CFBundleExecutable".into(),
        plist::Value::String("app".into()),
    );
    info.insert(
        "CFBundleShortVersionString".into(),
        plist::Value::String(version.into()),
    );
    plist::Value::Dictionary(info)
        .to_file_xml(bundle.join("Contents/Info.plist"))
        .unwrap();
    bundle
}

#[test]
fn workbuddy_waits_for_actual_application_after_native_handoff_and_preserves_configuration() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("User ' & home");
    std::fs::create_dir_all(&home).unwrap();
    let config = home.join("existing-models.json");
    std::fs::write(&config, "original model preference").unwrap();
    let executor = Arc::new(GuiInstaller::default());
    let manager = InstallationManager::new(
        environment(&home, HostPlatform::MacOs),
        "aarch64".into(),
        executor.clone(),
    );
    assert!(manager.availability(ToolId::WorkBuddy).supported);
    manager.start(ToolId::WorkBuddy).unwrap();
    let waiting = until(&manager, ToolId::WorkBuddy, |state| {
        state.stage == InstallStage::WaitingForUser
    });
    assert!(!waiting.installed);
    assert!(waiting.detail.contains("原生安装器"));
    assert!(!ToolInventory::new(environment(&home, HostPlatform::MacOs))
        .is_installed(ToolId::WorkBuddy));
    application(&home, "WorkBuddy", "5.9.3");
    let done = until(&manager, ToolId::WorkBuddy, |state| {
        state.stage.is_terminal()
    });
    assert_eq!(done.stage, InstallStage::Succeeded, "{}", done.detail);
    assert!(done.installed);
    assert_eq!(
        std::fs::read_to_string(config).unwrap(),
        "original model preference"
    );
    assert!(!home.join(".tokenmarket/desktop-connections").exists());
    let steps = executor.steps.lock().unwrap();
    assert!(steps
        .iter()
        .any(|step| step.stage == InstallStage::WaitingForUser));
    assert!(steps
        .iter()
        .filter_map(|step| step.download.as_ref())
        .all(|(url, _)| url.contains("WorkBuddy-darwin-arm64-5.9.3.42398134-d4692421.dmg")));
}

fn packaged_version(bundle: &Path, version: &str) {
    asar_version(&bundle.join("Contents/Resources"), version);
}
fn asar_version(resources: &Path, version: &str) {
    // Electron ASAR's size pickle, header pickle and root package.json payload.
    let package = format!("{{\"version\":\"{version}\"}}").into_bytes();
    let header = format!(
        "{{\"files\":{{\"package.json\":{{\"size\":{},\"offset\":\"0\"}}}}}}",
        package.len()
    )
    .into_bytes();
    let padding = (4 - header.len() % 4) % 4;
    let header_size = 8 + header.len() + padding;
    let mut bytes = vec![];
    bytes.extend_from_slice(&4u32.to_le_bytes());
    bytes.extend_from_slice(&(header_size as u32).to_le_bytes());
    bytes.extend_from_slice(&((header_size - 4) as u32).to_le_bytes());
    bytes.extend_from_slice(&(header.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&header);
    bytes.extend(std::iter::repeat_n(0, padding));
    bytes.extend_from_slice(&package);
    std::fs::create_dir_all(resources).unwrap();
    std::fs::write(resources.join("app.asar"), bytes).unwrap();
}

#[test]
fn native_wait_can_be_stopped_without_stopping_the_installer_and_late_installation_is_reused() {
    let root = tempfile::tempdir().unwrap();
    let executor = Arc::new(GuiInstaller::default());
    let manager = InstallationManager::new(
        environment(root.path(), HostPlatform::MacOs),
        "aarch64".into(),
        executor.clone(),
    );
    executor.fail.store(true, Ordering::SeqCst);
    manager.start(ToolId::WorkBuddy).unwrap();
    let failed = until(&manager, ToolId::WorkBuddy, |state| {
        state.stage.is_terminal()
    });
    assert_eq!(failed.stage, InstallStage::Failed);
    assert!(failed.detail.contains("official package download failed"));
    executor.fail.store(false, Ordering::SeqCst);
    manager.start(ToolId::WorkBuddy).unwrap();
    until(&manager, ToolId::WorkBuddy, |state| {
        state.stage == InstallStage::WaitingForUser
    });
    let handoffs = executor.steps.lock().unwrap().len();
    manager.cancel(ToolId::WorkBuddy).unwrap();
    let cancelled = until(&manager, ToolId::WorkBuddy, |state| {
        state.stage.is_terminal()
    });
    assert_eq!(cancelled.stage, InstallStage::Cancelled);
    assert!(cancelled.detail.contains("请在原生安装器中取消"));
    assert_eq!(executor.steps.lock().unwrap().len(), handoffs);
    application(root.path(), "WorkBuddy", "5.9.3");
    manager.start(ToolId::WorkBuddy).unwrap();
    assert_eq!(
        until(&manager, ToolId::WorkBuddy, |state| state
            .stage
            .is_terminal())
        .stage,
        InstallStage::Succeeded
    );
    assert_eq!(executor.steps.lock().unwrap().len(), handoffs);
}

#[test]
fn existing_unapproved_gui_versions_are_not_replaced_or_launched() {
    for (tool, name, version) in [
        (ToolId::WorkBuddy, "WorkBuddy", "5.9.2"),
        (ToolId::DeepSeekHarness, "DeepSeek Harness", "0.2.1-rc.6"),
    ] {
        let root = tempfile::tempdir().unwrap();
        let bundle = application(root.path(), name, version);
        if tool == ToolId::DeepSeekHarness {
            packaged_version(&bundle, version);
        }
        let executor = Arc::new(GuiInstaller::default());
        let manager = InstallationManager::new(
            environment(root.path(), HostPlatform::MacOs),
            "aarch64".into(),
            executor.clone(),
        );
        manager.start(tool).unwrap();
        let done = until(&manager, tool, |state| state.stage.is_terminal());
        assert_eq!(done.stage, InstallStage::Failed);
        assert!(done.detail.contains("未自动升级"));
        assert!(executor.steps.lock().unwrap().is_empty());
        assert_eq!(
            manager.installed_version(tool).unwrap().as_deref(),
            Some(version)
        );
    }
}

#[test]
fn windows_native_handoff_waits_for_real_application_and_keeps_paths_as_data() {
    for (tool, name) in [
        (ToolId::WorkBuddy, "WorkBuddy"),
        (ToolId::DeepSeekHarness, "DeepSeek Harness"),
    ] {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("User ' & home");
        let executor = Arc::new(GuiInstaller::default());
        let manager = InstallationManager::new(
            environment(&home, HostPlatform::Windows),
            "x86_64".into(),
            executor.clone(),
        );
        manager.start(tool).unwrap();
        let waiting = until(&manager, tool, |state| {
            state.stage == InstallStage::WaitingForUser
        });
        assert!(!waiting.installed);
        let folder = home.join("Applications").join(name);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            folder.join(format!("{name}.exe")),
            "application fixture; never execute",
        )
        .unwrap();
        if tool == ToolId::DeepSeekHarness {
            asar_version(&folder.join("resources"), "0.2.1-rc.5");
        }
        let done = until(&manager, tool, |state| state.stage.is_terminal());
        assert_eq!(done.stage, InstallStage::Succeeded, "{}", done.detail);
        let steps = executor.steps.lock().unwrap();
        let launch = steps.last().unwrap().args.last().unwrap();
        assert!(launch.contains("Start-Process -FilePath"));
        assert!(launch.contains("User '' & home"));
        assert!(!launch.contains("-Wait"));
        if tool == ToolId::DeepSeekHarness {
            assert!(steps.iter().any(|step| step
                .args
                .iter()
                .any(|arg| arg.contains("Get-FileHash") && arg.contains("SHA512"))));
        }
    }
}

#[cfg(windows)]
#[test]
fn native_windows_application_metadata_handles_literal_paths_and_rejects_read_errors() {
    use tokenmarket_core::installation::NativeInstallExecutor;

    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("User ' & home");
    std::fs::create_dir_all(&directory).unwrap();
    let missing = directory.join("missing.exe");
    assert!(NativeInstallExecutor.application_version(&missing).is_err());

    // Copy an OS-supplied PE rather than running any installed Agent or installer.
    let original = PathBuf::from(std::env::var_os("SystemRoot").unwrap())
        .join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let expected = NativeInstallExecutor
        .application_version(&original)
        .unwrap()
        .expect("the OS PowerShell executable has version metadata");
    assert!(expected.starts_with(|c: char| c.is_ascii_digit()));
    let copied = directory.join("application.exe");
    std::fs::copy(original, &copied).unwrap();
    assert_eq!(
        NativeInstallExecutor.application_version(&copied).unwrap(),
        Some(expected)
    );
}

#[test]
fn unavailable_workbuddy_platforms_offer_accurate_official_alternatives() {
    let root = tempfile::tempdir().unwrap();
    let executor = Arc::new(GuiInstaller::default());
    let manager = InstallationManager::new(
        environment(root.path(), HostPlatform::Unix),
        "x86_64".into(),
        executor.clone(),
    );
    let available = manager.availability(ToolId::WorkBuddy);
    assert!(!available.supported);
    assert!(available.reason.unwrap().contains("统信 UOS / 银河麒麟"));
    assert!(manager.start(ToolId::WorkBuddy).is_err());
    assert!(executor.steps.lock().unwrap().is_empty());
}

#[cfg(target_os = "macos")]
struct CorruptArchiveInstaller {
    handoffs: std::sync::atomic::AtomicUsize,
}
#[cfg(target_os = "macos")]
impl InstallExecutor for CorruptArchiveInstaller {
    fn execute(
        &self,
        step: &InstallStep,
        cancelled: &AtomicBool,
        output: &mut dyn FnMut(String),
    ) -> Result<()> {
        if let Some((_, path)) = &step.download {
            std::fs::create_dir_all(path.parent().unwrap())?;
            std::fs::write(path, "corrupted fixture archive")?;
            return Ok(());
        }
        if step.stage == InstallStage::WaitingForUser {
            self.handoffs.fetch_add(1, Ordering::SeqCst);
            return Ok(());
        }
        tokenmarket_core::installation::NativeInstallExecutor.execute(step, cancelled, output)
    }
    fn version(&self, _: &Path) -> Result<String> {
        panic!("no tool was installed")
    }
}

#[cfg(target_os = "macos")]
#[test]
fn a_corrupted_harness_archive_is_rejected_before_extraction_or_native_handoff() {
    let root = tempfile::tempdir().unwrap();
    let executor = Arc::new(CorruptArchiveInstaller {
        handoffs: std::sync::atomic::AtomicUsize::new(0),
    });
    let manager = InstallationManager::new(
        environment(root.path(), HostPlatform::MacOs),
        "aarch64".into(),
        executor.clone(),
    );
    manager.start(ToolId::DeepSeekHarness).unwrap();
    let done = until(&manager, ToolId::DeepSeekHarness, |state| {
        state.stage.is_terminal()
    });
    assert_eq!(done.stage, InstallStage::Failed);
    assert!(done
        .output
        .iter()
        .any(|line| line.contains("SHA512 verification failed")));
    assert_eq!(executor.handoffs.load(Ordering::SeqCst), 0);
    assert!(!root
        .path()
        .join(".tokenmarket/installers/deepseek-harness/0.2.1-rc.5/unpacked")
        .exists());
}

#[test]
fn harness_approved_rc_is_verified_and_detected_without_registering_or_running_dsh() {
    let root = tempfile::tempdir().unwrap();
    let executor = Arc::new(GuiInstaller::default());
    let manager = InstallationManager::new(
        environment(root.path(), HostPlatform::MacOs),
        "aarch64".into(),
        executor.clone(),
    );
    let availability = manager.availability(ToolId::DeepSeekHarness);
    assert!(availability.supported, "{:?}", availability.reason);
    assert_eq!(availability.version.as_deref(), Some("0.2.1-rc.5"));
    manager.start(ToolId::DeepSeekHarness).unwrap();
    until(&manager, ToolId::DeepSeekHarness, |state| {
        state.stage == InstallStage::WaitingForUser
    });
    // The bundle's sanitized numeric version is not evidence for the approved RC.
    let bundle = application(root.path(), "DeepSeek Harness", "0.2.1");
    packaged_version(&bundle, "0.2.1-rc.5");
    let done = until(&manager, ToolId::DeepSeekHarness, |state| {
        state.stage.is_terminal()
    });
    assert_eq!(done.stage, InstallStage::Succeeded, "{}", done.detail);
    assert_eq!(
        manager
            .installed_version(ToolId::DeepSeekHarness)
            .unwrap()
            .as_deref(),
        Some("0.2.1-rc.5")
    );
    assert!(!root.path().join("bin/dsh").exists());
    assert!(
        ToolInventory::new(environment(root.path(), HostPlatform::MacOs))
            .is_installed(ToolId::DeepSeekHarness)
    );
    let steps = executor.steps.lock().unwrap();
    assert!(steps.iter().any(|step| step.download.as_ref().is_some_and(|(url,_)| url == "https://download.deepseek.com/dsh-desk/bin/mac-arm64/deepseek-harness-0.2.1-rc.5-mac-arm64.zip")));
    assert!(steps
        .iter()
        .any(|step| step.args.iter().any(|arg| arg.contains("shasum -a 512"))));
    assert_eq!(steps.last().unwrap().program, "/usr/bin/open");
    assert!(!steps.last().unwrap().args[0].ends_with(".app"));
}

struct HarnessCliInstaller {
    command: PathBuf,
    steps: Mutex<Vec<InstallStep>>,
}
impl InstallExecutor for HarnessCliInstaller {
    fn execute(&self, step: &InstallStep, _: &AtomicBool, _: &mut dyn FnMut(String)) -> Result<()> {
        self.steps.lock().unwrap().push(step.clone());
        std::fs::create_dir_all(self.command.parent().unwrap())?;
        std::fs::write(&self.command, "persistent CLI fixture; never execute")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&self.command, std::fs::Permissions::from_mode(0o755))?;
        }
        Ok(())
    }
    fn version(&self, _: &Path) -> Result<String> {
        Ok("0.2.1-rc.5".into())
    }
}

#[test]
fn harness_has_a_persistent_official_cli_installation_when_a_desktop_package_is_unavailable() {
    for (platform, architecture) in [
        (HostPlatform::Unix, "x86_64"),
        (HostPlatform::Unix, "aarch64"),
        (HostPlatform::MacOs, "x86_64"),
    ] {
        let root = tempfile::tempdir().unwrap();
        let executor = Arc::new(HarnessCliInstaller {
            command: root
                .path()
                .join(".tokenmarket/tools/deepseek-harness/bin/dsh"),
            steps: Mutex::new(vec![]),
        });
        let manager = InstallationManager::new(
            environment(root.path(), platform),
            architecture.into(),
            executor.clone(),
        );
        let available = manager.availability(ToolId::DeepSeekHarness);
        assert!(available.supported, "{:?}", available.reason);
        manager.start(ToolId::DeepSeekHarness).unwrap();
        let done = until(&manager, ToolId::DeepSeekHarness, |state| {
            state.stage.is_terminal()
        });
        assert_eq!(done.stage, InstallStage::Succeeded, "{}", done.detail);
        assert!(done.installed);
        let steps = executor.steps.lock().unwrap();
        assert!(steps.iter().any(|step| step
            .args
            .iter()
            .any(|arg| arg.contains("@deepseek-ai/dsh@0.2.1-rc.5"))));
        assert!(!steps
            .iter()
            .any(|step| step.stage == InstallStage::WaitingForUser));
        assert!(!root.path().join(".dsh").exists());
    }
}

#[test]
fn managed_harness_cli_uses_the_home_patch_instead_of_an_electron_only_profile() {
    let root = tempfile::tempdir().unwrap();
    let command_directory = root.path().join(".tokenmarket/tools/deepseek-harness/bin");
    std::fs::create_dir_all(&command_directory).unwrap();
    let command = command_directory.join("dsh");
    std::fs::write(&command, "managed CLI fixture; never execute").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&command, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let desktop_profile = root.path().join(".dsh/profiles/desktop/cordis.patch.yml");
    std::fs::create_dir_all(desktop_profile.parent().unwrap()).unwrap();
    std::fs::write(&desktop_profile, "retained desktop profile").unwrap();
    let mut env = environment(root.path(), HostPlatform::Unix);
    env.command_directories.push(command_directory);
    let inventory = ToolInventory::new(env);
    assert_eq!(
        inventory.config_path(ToolId::DeepSeekHarness),
        root.path().join(".dsh/cordis.patch.yml")
    );
    assert_eq!(
        std::fs::read_to_string(desktop_profile).unwrap(),
        "retained desktop profile"
    );
}
