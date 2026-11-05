use anyhow::{bail, Result};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokenmarket_core::{
    installation::{InstallExecutor, InstallStage, InstallStep, InstallationManager},
    inventory::{HostPlatform, InstallationEnvironment, ToolInventory},
    ToolId,
};
struct Fixture {
    version: Mutex<String>,
    probes: Mutex<Vec<std::path::PathBuf>>,
    target: Mutex<String>,
    steps: Mutex<Vec<InstallStep>>,
    fail: AtomicBool,
    waiting: AtomicBool,
    bump: AtomicBool,
}
impl InstallExecutor for Fixture {
    fn execute(&self, s: &InstallStep, c: &AtomicBool, _: &mut dyn FnMut(String)) -> Result<()> {
        self.steps.lock().unwrap().push(s.clone());
        while self.waiting.load(Ordering::SeqCst) {
            if c.load(Ordering::SeqCst) {
                bail!("fixture cancelled");
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        if self.bump.load(Ordering::SeqCst) {
            *self.version.lock().unwrap() = self.target.lock().unwrap().clone();
        }
        if self.fail.load(Ordering::SeqCst) {
            bail!("fixture interrupted");
        }
        Ok(())
    }
    fn version(&self, path: &Path) -> Result<String> {
        self.probes.lock().unwrap().push(path.into());
        Ok(self.version.lock().unwrap().clone())
    }
}
fn executable(p: &Path) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, "inert fixture").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}
fn setup(
    home: &Path,
    current: &str,
    external: bool,
) -> (InstallationManager, Arc<Fixture>, InstallationEnvironment) {
    let bin = if external {
        home.join("bin")
    } else {
        home.join(".tokenmarket/tools/codex/bin")
    };
    executable(&bin.join("codex"));
    let env = InstallationEnvironment {
        home: home.into(),
        platform: HostPlatform::MacOs,
        command_directories: vec![bin],
        application_directories: vec![],
        executable_extensions: vec![],
    };
    let exec = Arc::new(Fixture {
        version: Mutex::new(current.into()),
        probes: Mutex::new(vec![]),
        target: Mutex::new("0.170.0".into()),
        steps: Mutex::new(vec![]),
        fail: AtomicBool::new(false),
        waiting: AtomicBool::new(false),
        bump: AtomicBool::new(true),
    });
    (
        InstallationManager::new(env.clone(), "aarch64".into(), exec.clone()),
        exec,
        env,
    )
}
fn done(m: &InstallationManager) -> tokenmarket_core::installation::InstallStatus {
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        let s = m.status(ToolId::Codex).unwrap();
        if s.stage.is_terminal() {
            return s;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
}
#[test]
fn latest_release_replaces_the_bundled_install_pin_as_upgrade_target() {
    let t = tempfile::tempdir().unwrap();
    let (m, e, _) = setup(t.path(), "codex-cli 0.163.0", false);
    let u = m.inspect_update(ToolId::Codex, Ok("0.170.0".into()));
    assert_eq!(u.target_version.as_deref(), Some("0.170.0"));
    assert!(u.can_upgrade);
    assert!(!u.note.contains("兼容"));
    assert!(e.steps.lock().unwrap().is_empty());
    m.start_upgrade_to(ToolId::Codex, "0.170.0").unwrap();
    let s = done(&m);
    assert_eq!(s.stage, InstallStage::Succeeded);
    assert_eq!(s.version.as_deref(), Some("0.170.0"));
    let steps = e.steps.lock().unwrap();
    assert!(steps
        .iter()
        .flat_map(|s| &s.args)
        .any(|a| a.contains("@latest")));
    assert!(!steps.iter().flat_map(|s| &s.args).any(|a| a == "0.166.0"));
}
#[test]
fn failed_check_invalidates_cached_upgrade_and_never_falls_back_to_an_old_pin() {
    let t = tempfile::tempdir().unwrap();
    let (m, e, _) = setup(t.path(), "0.150.0", false);
    m.inspect_update(ToolId::Codex, Ok("0.170.0".into()));
    let u = m.inspect_update(ToolId::Codex, Err(anyhow::anyhow!("offline")));
    assert_eq!(u.state, "check_failed");
    assert!(!u.can_upgrade);
    assert!(u.target_version.is_none());
    assert!(m.start_upgrade(ToolId::Codex).is_err());
    assert!(e.steps.lock().unwrap().is_empty());
}
#[test]
fn latest_targets_require_a_successful_check_and_matching_confirmation() {
    let t = tempfile::tempdir().unwrap();
    let (m, e, _) = setup(t.path(), "0.150.0", false);
    assert!(m.start_upgrade(ToolId::Codex).is_err());
    m.inspect_update(ToolId::Codex, Ok("0.170.0".into()));
    assert!(m.start_upgrade_to(ToolId::Codex, "0.166.0").is_err());
    assert!(e.steps.lock().unwrap().is_empty());
}
#[test]
fn current_newer_and_unknown_versions_are_not_downgraded() {
    for current in ["0.170.0", "0.180.0", "unknown"] {
        let t = tempfile::tempdir().unwrap();
        let (m, e, _) = setup(t.path(), current, false);
        let u = m.inspect_update(ToolId::Codex, Ok("0.170.0".into()));
        assert!(!u.can_upgrade);
        assert!(m.start_upgrade(ToolId::Codex).is_err());
        assert!(e.steps.lock().unwrap().is_empty());
    }
}
#[test]
fn unknown_external_channels_offer_the_official_latest_version_for_manual_update() {
    let t = tempfile::tempdir().unwrap();
    let (m, e, _) = setup(t.path(), "0.150.0", true);
    std::fs::write(t.path().join("bin/codex"), [0x7f, b'E', b'L', b'F']).unwrap();
    let u = m.inspect_update(ToolId::Codex, Ok("0.170.0".into()));
    assert!(u.manual_upgrade);
    assert!(!u.can_upgrade);
    assert_eq!(u.target_version.as_deref(), Some("0.170.0"));
    assert!(e.steps.lock().unwrap().is_empty());
}
#[cfg(unix)]
#[test]
fn global_npm_install_updates_in_its_original_prefix_instead_of_shadowing_it() {
    use std::os::unix::fs::symlink;
    for (tool, package, current, latest) in [
        (ToolId::Codex, "@openai/codex", "0.150.0", "0.170.0"),
        (
            ToolId::ClaudeCode,
            "@anthropic-ai/claude-code",
            "2.1.311",
            "2.1.318",
        ),
    ] {
        let t = tempfile::tempdir().unwrap();
        let (m, e, mut env) = setup(t.path(), current, true);
        drop(m);
        *e.target.lock().unwrap() = latest.into();
        let root = t.path().join("lib/node_modules").join(package);
        executable(&root.join("bin/cli.js"));
        std::fs::write(
            root.join("package.json"),
            format!(r#"{{"name":"{package}"}}"#),
        )
        .unwrap();
        std::fs::remove_file(t.path().join("bin/codex")).unwrap();
        let npm_entry = t.path().join("bin").join(tool.command());
        symlink(root.join("bin/cli.js"), &npm_entry).unwrap();
        let native = t.path().join(".local/bin");
        executable(&native.join(tool.command()));
        env.command_directories.push(native);
        executable(&t.path().join("bin/npm"));
        let m = InstallationManager::new(env, "aarch64".into(), e.clone());
        let u = m.inspect_update(tool, Ok(latest.into()));
        assert_eq!(u.current_version.as_deref(), Some(current));
        assert!(u.can_upgrade);
        m.start_upgrade(tool).unwrap();
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            let status = m.status(tool).unwrap();
            if status.stage.is_terminal() {
                assert_eq!(status.stage, InstallStage::Succeeded);
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        let probes = e.probes.lock().unwrap();
        assert!(!probes.is_empty());
        assert!(probes.iter().any(|p| p == &npm_entry));
        assert_eq!(u.upgrade_path.as_deref(), npm_entry.to_str());
        let steps = e.steps.lock().unwrap();
        assert_eq!(steps.len(), 1);
        let command = steps[0].args.join(" ");
        assert!(command.contains(&format!("{package}@latest")));
        assert!(command.contains("--prefix"));
        assert!(command.contains(t.path().to_str().unwrap()));
    }
}
#[test]
fn interrupted_managed_upgrade_can_be_retried_even_when_it_prints_the_target_version() {
    let t = tempfile::tempdir().unwrap();
    let (m, e, env) = setup(t.path(), "0.150.0", false);
    m.inspect_update(ToolId::Codex, Ok("0.170.0".into()));
    e.fail.store(true, Ordering::SeqCst);
    m.start_upgrade(ToolId::Codex).unwrap();
    assert_eq!(done(&m).stage, InstallStage::Failed);
    assert!(!ToolInventory::new(env.clone()).is_installed(ToolId::Codex));
    assert!(
        m.inspect_update(ToolId::Codex, Ok("0.170.0".into()))
            .can_upgrade
    );
    e.fail.store(false, Ordering::SeqCst);
    m.start_upgrade(ToolId::Codex).unwrap();
    assert_eq!(done(&m).stage, InstallStage::Succeeded);
    assert!(ToolInventory::new(env).is_installed(ToolId::Codex));
}
#[test]
fn retry_refuses_a_version_newer_than_the_checked_target() {
    let t = tempfile::tempdir().unwrap();
    let (m, e, _) = setup(t.path(), "0.150.0", false);
    m.inspect_update(ToolId::Codex, Ok("0.170.0".into()));
    e.fail.store(true, Ordering::SeqCst);
    m.start_upgrade(ToolId::Codex).unwrap();
    done(&m);
    *e.version.lock().unwrap() = "0.180.0".into();
    let count = e.steps.lock().unwrap().len();
    assert!(m.start_upgrade(ToolId::Codex).is_err());
    assert_eq!(e.steps.lock().unwrap().len(), count);
}
#[test]
fn cancelled_upgrade_releases_the_operation_lock_and_allows_retry() {
    let t = tempfile::tempdir().unwrap();
    let (m, e, _) = setup(t.path(), "0.150.0", false);
    m.inspect_update(ToolId::Codex, Ok("0.170.0".into()));
    e.waiting.store(true, Ordering::SeqCst);
    m.start_upgrade(ToolId::Codex).unwrap();
    assert!(m.start_upgrade(ToolId::Codex).is_err());
    assert!(m.connection_guard(ToolId::Codex).is_err());
    m.cancel(ToolId::Codex).unwrap();
    assert_eq!(done(&m).stage, InstallStage::Cancelled);
    e.waiting.store(false, Ordering::SeqCst);
    m.start_upgrade(ToolId::Codex).unwrap();
    assert_eq!(done(&m).stage, InstallStage::Succeeded);
}
#[test]
fn success_exit_with_an_old_version_is_not_reported_as_success() {
    let t = tempfile::tempdir().unwrap();
    let (m, e, env) = setup(t.path(), "0.150.0", false);
    m.inspect_update(ToolId::Codex, Ok("0.170.0".into()));
    e.bump.store(false, Ordering::SeqCst);
    m.start_upgrade(ToolId::Codex).unwrap();
    assert_eq!(done(&m).stage, InstallStage::Failed);
    assert!(!ToolInventory::new(env).is_installed(ToolId::Codex));
}
#[test]
fn installation_upgrades_follow_upstream_without_adapter_version_pins() {
    let t = tempfile::tempdir().unwrap();
    let (_previous, e, mut env) = setup(t.path(), "0.150.0", false);
    executable(&t.path().join("bin/opencode"));
    env.command_directories.insert(0, t.path().join("bin"));
    let m = InstallationManager::new(env, "aarch64".into(), e);
    let u = m.inspect_update(ToolId::OpenCode, Ok("2.0.0".into()));
    assert_eq!(u.state, "available");
    assert!(u.can_upgrade);
    assert_eq!(u.target_version.as_deref(), Some("2.0.0"));
    drop(m);
}
#[tokio::test]
async fn shared_application_checks_return_every_tool_without_installing_anything() {
    let t = tempfile::tempdir().unwrap();
    let (_, e, mut env) = setup(t.path(), "unused", false);
    env.command_directories.clear();
    std::fs::remove_file(t.path().join(".tokenmarket/tools/codex/bin/codex")).unwrap();
    let m = InstallationManager::new(env, "aarch64".into(), e.clone());
    let result = m.check_updates().await;
    let expected: std::collections::BTreeSet<_> = tokenmarket_core::adapters::adapters()
        .iter()
        .map(|a| a.id.to_string())
        .collect();
    assert_eq!(
        result
            .iter()
            .map(|r| r.tool.to_string())
            .collect::<std::collections::BTreeSet<_>>(),
        expected
    );
    assert_eq!(result.len(), expected.len());
    assert!(result.iter().all(|r| r.state == "not_installed"));
    assert!(e.steps.lock().unwrap().is_empty());
}

#[test]
fn claude_above_the_old_pin_still_upgrades_to_the_official_latest_release() {
    let t = tempfile::tempdir().unwrap();
    let (m, e, _) = setup(t.path(), "2.1.311 (Claude Code)", false);
    executable(&t.path().join(".local/bin/claude"));
    *e.target.lock().unwrap() = "2.1.318".into();
    let u = m.inspect_update(ToolId::ClaudeCode, Ok("2.1.318".into()));
    assert!(u.can_upgrade);
    assert_eq!(u.target_version.as_deref(), Some("2.1.318"));
    m.start_upgrade_to(ToolId::ClaudeCode, "2.1.318").unwrap();
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        let s = m.status(ToolId::ClaudeCode).unwrap();
        if s.stage.is_terminal() {
            assert_eq!(s.stage, InstallStage::Succeeded);
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(e
        .steps
        .lock()
        .unwrap()
        .iter()
        .flat_map(|s| &s.args)
        .any(|a| a == "update"));
}
