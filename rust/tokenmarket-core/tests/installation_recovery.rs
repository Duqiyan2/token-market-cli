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
    installation::{recipes, InstallExecutor, InstallStatus, InstallStep, InstallationManager},
    inventory::{HostPlatform, InstallationEnvironment, ToolInventory},
    ToolId,
};
fn executable(path: &Path, version: &str, runtime: Option<&Path>) -> Result<()> {
    std::fs::create_dir_all(path.parent().unwrap())?;
    // This is an inert fixture, not an installed third-party Agent.
    let runtime_check = runtime
        .map(|r| {
            format!(
                "if [ -f '{}' ]; then echo FIXTURE_RUNTIME_READY; exit 0; fi\n",
                r.to_string_lossy().replace('\'', "'\"'\"'")
            )
        })
        .unwrap_or_default();
    std::fs::write(path,format!("#!/bin/sh\nif [ \"$1\" = --version ]; then echo '{version}'; exit 0; fi\n{runtime_check}echo 'FIXTURE_RUNTIME_MISSING' >&2\nexit 42\n"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}
fn environment(home: &Path, platform: HostPlatform) -> InstallationEnvironment {
    let mut dirs = vec![home.join("bin")];
    dirs.extend(recipes::managed_command_directories(home));
    InstallationEnvironment {
        platform,
        home: home.into(),
        command_directories: dirs,
        application_directories: vec![],
        executable_extensions: vec![".EXE".into(), ".CMD".into()],
    }
}
struct PartialInstaller {
    command: PathBuf,
    runtime: PathBuf,
    broken_version: bool,
    cancel_first: bool,
    initial: AtomicBool,
    entered: AtomicBool,
    execution_count: Mutex<usize>,
}
impl InstallExecutor for PartialInstaller {
    fn execute(&self, s: &InstallStep, c: &AtomicBool, o: &mut dyn FnMut(String)) -> Result<()> {
        *self.execution_count.lock().unwrap() += 1;
        if s.download.is_some() {
            o("fixture: downloaded installer (no network)".into());
            return Ok(());
        }
        executable(&self.command, "2026.10.9", Some(&self.runtime))?;
        self.entered.store(true, Ordering::SeqCst);
        o("fixture: managed command published, required runtime not yet prepared".into());
        if self.initial.load(Ordering::SeqCst) {
            if self.cancel_first {
                while !c.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(2));
                }
                bail!("fixture cancelled after publishing managed command");
            }
            bail!("fixture interrupted after publishing managed command");
        }
        std::fs::write(&self.runtime, "fixture runtime ready")?;
        Ok(())
    }
    fn version(&self, _: &Path) -> Result<String> {
        if self.broken_version && !self.runtime.exists() {
            bail!("fixture required runtime missing; --version cannot run");
        }
        Ok("2026.10.9".into())
    }
}
fn finished(m: &InstallationManager, t: ToolId) -> InstallStatus {
    let until = Instant::now() + Duration::from_secs(3);
    loop {
        let s = m.status(t).unwrap();
        if s.stage.is_terminal() {
            return s;
        }
        assert!(Instant::now() < until, "timeout");
        std::thread::sleep(Duration::from_millis(2));
    }
}
#[test]
fn interrupted_owned_installs_are_not_usable_and_retry_repairs_them_even_after_restart() {
    for cancelled in [false, true] {
        for broken in [false, true] {
            for restart in [false, true] {
                let root = tempfile::tempdir().unwrap();
                let home = root.path();
                let command = home.join(".tokenmarket/tools/openclaw/bin/openclaw");
                let runtime = home.join("fixture-required-runtime");
                let external = Arc::new(PartialInstaller {
                    command: command.clone(),
                    runtime: runtime.clone(),
                    broken_version: broken,
                    cancel_first: cancelled,
                    initial: AtomicBool::new(true),
                    entered: AtomicBool::new(false),
                    execution_count: Mutex::new(0),
                });
                let env = environment(home, HostPlatform::Unix);
                let manager =
                    InstallationManager::new(env.clone(), "x86_64".into(), external.clone());
                manager.start(ToolId::OpenClaw).unwrap();
                if cancelled {
                    let until = Instant::now() + Duration::from_secs(3);
                    while !external.entered.load(Ordering::SeqCst) {
                        assert!(Instant::now() < until);
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    manager.cancel(ToolId::OpenClaw).unwrap();
                }
                let first = finished(&manager, ToolId::OpenClaw);
                assert_eq!(
                    first.stage,
                    if cancelled {
                        tokenmarket_core::installation::InstallStage::Cancelled
                    } else {
                        tokenmarket_core::installation::InstallStage::Failed
                    }
                );
                assert!(
                    !first.installed,
                    "a partial launcher is not a usable installation"
                );
                assert!(!ToolInventory::new(env.clone()).is_installed(ToolId::OpenClaw));
                assert!(ToolInventory::new(env.clone())
                    .require_installed(ToolId::OpenClaw)
                    .is_err());
                external.initial.store(false, Ordering::SeqCst);
                let manager = if restart {
                    InstallationManager::new(env.clone(), "x86_64".into(), external.clone())
                } else {
                    manager
                };
                manager.start(ToolId::OpenClaw).unwrap();
                let retry = finished(&manager, ToolId::OpenClaw);
                assert_eq!(
                    retry.stage,
                    tokenmarket_core::installation::InstallStage::Succeeded,
                    "{cancelled}/{broken}/{restart}: {}",
                    retry.detail
                );
                assert!(retry.installed && runtime.exists());
                assert!(ToolInventory::new(env).is_installed(ToolId::OpenClaw));
            }
        }
    }
}

#[test]
fn an_external_install_is_not_hidden_or_repaired_by_an_owned_incomplete_attempt() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path();
    let command = home.join(".tokenmarket/tools/openclaw/bin/openclaw");
    let runtime = home.join("required-runtime");
    let executor = Arc::new(PartialInstaller {
        command,
        runtime: runtime.clone(),
        broken_version: false,
        cancel_first: false,
        initial: AtomicBool::new(true),
        entered: AtomicBool::new(false),
        execution_count: Mutex::new(0),
    });
    let env = environment(home, HostPlatform::Unix);
    let manager = InstallationManager::new(env.clone(), "x86_64".into(), executor.clone());
    manager.start(ToolId::OpenClaw).unwrap();
    finished(&manager, ToolId::OpenClaw);
    let external = home.join("bin/openclaw");
    executable(&external, "2026.10.9", None).unwrap();
    let bytes = std::fs::read(&external).unwrap();
    assert!(ToolInventory::new(env.clone()).is_installed(ToolId::OpenClaw));
    let recreated = InstallationManager::new(env, "x86_64".into(), executor);
    recreated.start(ToolId::OpenClaw).unwrap();
    assert_eq!(
        finished(&recreated, ToolId::OpenClaw).stage,
        tokenmarket_core::installation::InstallStage::Succeeded
    );
    assert_eq!(std::fs::read(external).unwrap(), bytes);
    assert!(!runtime.exists());
}
