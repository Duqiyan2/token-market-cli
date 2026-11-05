use anyhow::Result;
use std::{
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc, Mutex},
    time::{Duration, Instant},
};
use tokenmarket_core::{
    installation::{recipes, InstallExecutor, InstallStage, InstallStep, InstallationManager},
    inventory::{HostPlatform, InstallationEnvironment, ToolInventory},
    ToolId,
};
fn executable(p: &Path, contents: impl AsRef<[u8]>) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, contents).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}
fn env(home: &Path, platform: HostPlatform) -> InstallationEnvironment {
    InstallationEnvironment {
        home: home.into(),
        platform,
        command_directories: vec![
            home.join("bin"),
            home.join(".local/bin"),
            home.join(".npm-global/bin"),
        ],
        application_directories: vec![],
        executable_extensions: vec![".EXE".into(), ".CMD".into()],
    }
}
#[derive(Default)]
struct Executor {
    steps: Mutex<Vec<InstallStep>>,
    remove: Mutex<Option<PathBuf>>,
    fail: Mutex<bool>,
}
impl InstallExecutor for Executor {
    fn execute(&self, s: &InstallStep, _: &AtomicBool, _: &mut dyn FnMut(String)) -> Result<()> {
        self.steps.lock().unwrap().push(s.clone());
        if let Some(p) = self.remove.lock().unwrap().take() {
            std::fs::remove_file(p)?;
        }
        Ok(())
    }
    fn version(&self, _: &Path) -> Result<String> {
        if *self.fail.lock().unwrap() {
            anyhow::bail!("runtime missing")
        }
        Ok("1.0.0".into())
    }
}
fn manager(env: InstallationEnvironment, e: Arc<Executor>) -> InstallationManager {
    InstallationManager::new(env, "aarch64".into(), e)
}
fn done(m: &InstallationManager, t: ToolId) -> tokenmarket_core::installation::InstallStatus {
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        let s = m.status(t).unwrap();
        if s.stage.is_terminal() {
            return s;
        }
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(5));
    }
}
#[test]
fn install_policy_matches_ccswitch_instead_of_runtime_or_pins() {
    for platform in [
        HostPlatform::MacOs,
        HostPlatform::Unix,
        HostPlatform::Windows,
    ] {
        let root = tempfile::tempdir().unwrap();
        let e = env(root.path(), platform);
        for tool in [
            ToolId::ClaudeCode,
            ToolId::Codex,
            ToolId::GrokBuild,
            ToolId::OpenCode,
            ToolId::OpenClaw,
            ToolId::Hermes,
            ToolId::Pi,
            ToolId::MiniMaxCode,
            ToolId::KimiCode,
            ToolId::OhMyPi,
            ToolId::DeepSeekHarness,
        ] {
            let p = recipes::plan(tool, &e, "aarch64").unwrap();
            assert_eq!(p.version, "latest");
            assert!(!root.path().join(".tokenmarket").exists());
            let cmd = p
                .steps
                .iter()
                .flat_map(|s| &s.args)
                .cloned()
                .collect::<Vec<_>>()
                .join(" ");
            let primary_script = matches!(tool, ToolId::GrokBuild | ToolId::MiniMaxCode)
                || (platform != HostPlatform::Windows
                    && matches!(tool, ToolId::ClaudeCode | ToolId::OpenCode));
            if primary_script {
                assert!(cmd.contains("https://"));
                assert!(cmd.contains("@latest"));
                assert_eq!(
                    p.method,
                    tokenmarket_core::installation::InstallMethod::ScriptWithNpmFallback
                );
            }
            if matches!(tool, ToolId::Hermes | ToolId::KimiCode | ToolId::OhMyPi) {
                assert!(cmd.contains("https://"));
                assert!(!cmd.contains("@latest"));
            }
            if matches!(
                tool,
                ToolId::Codex | ToolId::OpenClaw | ToolId::Pi | ToolId::DeepSeekHarness
            ) {
                assert!(cmd.contains("@latest"));
                assert!(!cmd.contains("install.sh"));
            }
        }
    }
}
#[cfg(unix)]
fn npm_install(home: &Path, tool: ToolId, package: &str) -> PathBuf {
    use std::os::unix::fs::symlink;
    let p = home.join(".npm-global/lib/node_modules").join(package);
    executable(&p.join("bin/cli.js"), "fixture");
    std::fs::write(p.join("package.json"), format!(r#"{{"name":"{package}"}}"#)).unwrap();
    let entry = home.join(".npm-global/bin").join(tool.command());
    std::fs::create_dir_all(entry.parent().unwrap()).unwrap();
    symlink(p.join("bin/cli.js"), &entry).unwrap();
    executable(&home.join(".npm-global/bin/npm"), "fixture");
    entry
}
#[cfg(unix)]
#[test]
fn exact_npm_uninstall_preserves_other_installs_and_data_and_uses_original_prefix() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path();
    let tool = ToolId::Codex;
    let entry = npm_install(home, tool, "@openai/codex");
    let native = home.join(".local/bin/codex");
    executable(&native, [0xcf, 0xfa, 0xed, 0xfe]);
    let data = home.join(".codex/config.toml");
    executable(&data, "user config");
    let exec = Arc::new(Executor::default());
    let m = manager(env(home, HostPlatform::MacOs), exec.clone());
    let choices = m.uninstall_candidates(tool);
    assert_eq!(choices.len(), 2);
    let plan = choices
        .iter()
        .find(|c| c.installation.path == entry.to_str().unwrap())
        .unwrap()
        .plan
        .clone()
        .unwrap();
    assert!(plan.command.contains("--prefix"));
    assert!(plan.command.contains(".npm-global"));
    assert!(exec.steps.lock().unwrap().is_empty());
    let mut tampered = plan.clone();
    tampered.command = "rm -rf /".into();
    assert!(m.start_uninstall(tool, tampered).is_err());
    assert!(entry.exists());
    *exec.remove.lock().unwrap() = Some(entry.clone());
    m.start_uninstall(tool, plan).unwrap();
    let status = done(&m, tool);
    assert_eq!(status.stage, InstallStage::Succeeded);
    assert!(status.installed);
    assert!(native.exists());
    assert_eq!(std::fs::read_to_string(data).unwrap(), "user config");
    let steps = exec.steps.lock().unwrap();
    assert_eq!(steps.len(), 1);
    assert_eq!(
        steps[0].program,
        home.join(".npm-global/bin/npm").to_str().unwrap()
    );
}
#[cfg(unix)]
#[test]
fn native_uninstall_rechecks_preview_and_removes_only_program_files() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let home = root.path();
    let real = home.join(".local/share/claude/versions/2.1.311");
    executable(&real, [0xcf, 0xfa, 0xed, 0xfe]);
    let entry = home.join(".local/bin/claude");
    std::fs::create_dir_all(entry.parent().unwrap()).unwrap();
    symlink(&real, &entry).unwrap();
    let config = home.join(".claude/settings.json");
    executable(&config, "keep");
    let m = manager(
        env(home, HostPlatform::MacOs),
        Arc::new(Executor::default()),
    );
    let plan = m.uninstall_candidates(ToolId::ClaudeCode)[0]
        .plan
        .clone()
        .unwrap();
    let other = home.join(".local/share/claude/versions/2.1.312");
    executable(&other, [0xcf, 0xfa, 0xed, 0xfe]);
    std::fs::remove_file(&entry).unwrap();
    symlink(&other, &entry).unwrap();
    assert!(m.start_uninstall(ToolId::ClaudeCode, plan).is_err());
    assert!(real.exists());
    assert!(other.exists());
    let plan = m.uninstall_candidates(ToolId::ClaudeCode)[0]
        .plan
        .clone()
        .unwrap();
    m.start_uninstall(ToolId::ClaudeCode, plan).unwrap();
    assert_eq!(done(&m, ToolId::ClaudeCode).stage, InstallStage::Succeeded);
    assert!(!entry.exists());
    assert!(!other.exists());
    assert!(real.exists());
    assert_eq!(std::fs::read_to_string(config).unwrap(), "keep");
}
#[test]
fn unknown_native_and_unverified_package_cannot_be_uninstalled() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path();
    executable(&home.join("bin/codex"), [0x7f, b'E', b'L', b'F']);
    let m = manager(env(home, HostPlatform::Unix), Arc::new(Executor::default()));
    let choices = m.uninstall_candidates(ToolId::Codex);
    assert_eq!(choices.len(), 1);
    assert!(choices[0].plan.is_none());
    assert!(choices[0].unavailable_reason.is_some());
}
#[cfg(unix)]
#[test]
fn installer_exit_zero_is_not_uninstall_success_and_dangling_links_are_not_removed() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let home = root.path();
    let entry = npm_install(home, ToolId::Codex, "@openai/codex");
    let e = Arc::new(Executor::default());
    let m = manager(env(home, HostPlatform::MacOs), e);
    let p = m.uninstall_candidates(ToolId::Codex)[0]
        .plan
        .clone()
        .unwrap();
    m.start_uninstall(ToolId::Codex, p).unwrap();
    assert_eq!(done(&m, ToolId::Codex).stage, InstallStage::Failed);
    assert!(std::fs::symlink_metadata(&entry).is_ok());
    let target = home.join(".npm-global/lib/node_modules/@openai/codex/bin/cli.js");
    std::fs::remove_file(&target).unwrap();
    let unrelated = home.join("other");
    executable(&unrelated, "safe");
    std::fs::remove_file(&entry).unwrap();
    symlink(&unrelated, &entry).unwrap();
    assert!(m.uninstall_candidates(ToolId::Codex)[0].plan.is_none());
    assert!(unrelated.exists());
}
#[cfg(unix)]
#[test]
fn path_precedence_is_not_replaced_with_npm_priority_and_broken_defaults_are_visible() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path();
    let native = home.join("bin/codex");
    executable(&native, [0x7f, b'E', b'L', b'F']);
    npm_install(home, ToolId::Codex, "@openai/codex");
    let e = Arc::new(Executor::default());
    let env = env(home, HostPlatform::MacOs);
    let inventory = ToolInventory::new(env.clone());
    assert_eq!(
        inventory.installation_locations(ToolId::Codex)[0].path,
        native.to_str().unwrap()
    );
    *e.fail.lock().unwrap() = true;
    let m = manager(env, e);
    let u = m.inspect_update(ToolId::Codex, Ok("2.0.0".into()));
    assert_eq!(u.state, "broken");
    assert!(!u.can_upgrade);
    assert_eq!(u.installations.len(), 2);
}
#[cfg(unix)]
#[test]
fn openclaw_removes_only_gateway_service_before_package_uninstall() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path();
    let entry = npm_install(home, ToolId::OpenClaw, "openclaw");
    let e = Arc::new(Executor::default());
    let m = manager(env(home, HostPlatform::MacOs), e.clone());
    let plan = m.uninstall_candidates(ToolId::OpenClaw)[0]
        .plan
        .clone()
        .unwrap();
    assert!(plan.command.contains("--service"));
    assert!(!plan.command.contains("--all"));
    m.start_uninstall(ToolId::OpenClaw, plan).unwrap();
    assert_eq!(done(&m, ToolId::OpenClaw).stage, InstallStage::Failed);
    let steps = e.steps.lock().unwrap();
    assert_eq!(steps.len(), 2);
    assert_eq!(steps[0].program, entry.to_str().unwrap());
    assert_eq!(
        steps[0].args,
        ["uninstall", "--service", "--yes", "--non-interactive"]
    );
}

#[cfg(unix)]
#[test]
fn official_script_and_npm_fallback_execute_offline_and_preserve_configuration() {
    use tokenmarket_core::installation::NativeInstallExecutor;
    for fail_official in [false, true] {
        let root = tempfile::Builder::new()
            .prefix("lifecycle ' $ ` ")
            .tempdir()
            .unwrap();
        let home = root.path();
        let mut e = env(home, HostPlatform::MacOs);
        let bin = home.join("bin");
        let target = bin.join("claude");
        let source = home.join("fake-agent");
        let npm_log = home.join("npm-args");
        executable(&source, "#!/bin/sh\nprintf '9.0.0\\n'\n");
        executable(
            &bin.join("curl"),
            r#"#!/bin/sh
last=''
for arg in "$@"; do last=$arg; done
if [ "$FIXTURE_FAIL_OFFICIAL" = 1 ]; then printf 'exit 19\n' > "$last"; else printf 'cp "$FIXTURE_SOURCE" "$FIXTURE_TARGET"\nchmod 755 "$FIXTURE_TARGET"\n' > "$last"; fi
"#,
        );
        executable(
            &bin.join("npm"),
            r#"#!/bin/sh
printf '%s\n' "$@" > "$FIXTURE_NPM_LOG"
cp "$FIXTURE_SOURCE" "$FIXTURE_TARGET"
chmod 755 "$FIXTURE_TARGET"
"#,
        );
        let config = home.join(".claude/settings.json");
        executable(&config, "original settings");
        e.command_directories = vec![bin.clone()];
        struct Offline {
            vars: Vec<(String, String)>,
        }
        impl InstallExecutor for Offline {
            fn execute(
                &self,
                s: &InstallStep,
                c: &AtomicBool,
                o: &mut dyn FnMut(String),
            ) -> Result<()> {
                let mut s = s.clone();
                s.environment.extend(self.vars.clone());
                NativeInstallExecutor.execute(&s, c, o)
            }
            fn version(&self, p: &Path) -> Result<String> {
                NativeInstallExecutor.version(p)
            }
        }
        let exec = Arc::new(Offline {
            vars: vec![
                ("PATH".into(), format!("{}:/usr/bin:/bin", bin.display())),
                ("HOME".into(), home.to_string_lossy().into()),
                (
                    "FIXTURE_FAIL_OFFICIAL".into(),
                    if fail_official { "1" } else { "0" }.into(),
                ),
                ("FIXTURE_SOURCE".into(), source.to_string_lossy().into()),
                ("FIXTURE_TARGET".into(), target.to_string_lossy().into()),
                ("FIXTURE_NPM_LOG".into(), npm_log.to_string_lossy().into()),
            ],
        });
        let m = InstallationManager::new(e, "aarch64".into(), exec);
        m.start(ToolId::ClaudeCode).unwrap();
        let state = done(&m, ToolId::ClaudeCode);
        assert_eq!(state.stage, InstallStage::Succeeded, "{state:?}");
        assert_eq!(state.version.as_deref(), Some("9.0.0"));
        assert_eq!(npm_log.exists(), fail_official);
        if fail_official {
            assert!(std::fs::read_to_string(&npm_log)
                .unwrap()
                .contains("@anthropic-ai/claude-code@latest"));
        }
        assert_eq!(
            std::fs::read_to_string(config).unwrap(),
            "original settings"
        );
    }
}

#[cfg(unix)]
#[test]
fn brew_uninstall_requires_a_verified_owner_and_never_uses_nested_npm() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let home = root.path();
    let package = home.join("brew/Caskroom/codex/1/bin/codex");
    executable(&package, [0xcf, 0xfa, 0xed, 0xfe]);
    let launcher = home.join("brew/bin/codex");
    std::fs::create_dir_all(launcher.parent().unwrap()).unwrap();
    symlink(&package, &launcher).unwrap();
    executable(&home.join("brew/bin/brew"), "fixture");
    let mut env = env(home, HostPlatform::MacOs);
    env.command_directories = vec![home.join("brew/bin")];
    let m = manager(env, Arc::new(Executor::default()));
    let c = m.uninstall_candidates(ToolId::Codex);
    assert!(c[0].plan.as_ref().unwrap().command.contains("--cask"));
    assert!(!c[0].plan.as_ref().unwrap().command.contains("npm"));
    let unrelated = home.join("brew/Caskroom/unrelated/1/bin/codex");
    executable(&unrelated, [0xcf, 0xfa, 0xed, 0xfe]);
    std::fs::remove_file(&launcher).unwrap();
    symlink(unrelated, launcher).unwrap();
    assert!(m.uninstall_candidates(ToolId::Codex)[0].plan.is_none());
}

#[test]
fn windows_npm_uninstall_requires_the_wrapper_to_reference_the_official_package() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path();
    let mut env = env(home, HostPlatform::Windows);
    let prefix = home.join("npm");
    env.command_directories = vec![prefix.clone()];
    let entry = prefix.join("codex.cmd");
    executable(&entry, "@echo unrelated");
    let package = prefix.join("node_modules/@openai/codex");
    std::fs::create_dir_all(&package).unwrap();
    std::fs::write(package.join("package.json"), r#"{"name":"@openai/codex"}"#).unwrap();
    executable(&prefix.join("npm.cmd"), "fixture");
    let m = manager(env, Arc::new(Executor::default()));
    assert!(m.uninstall_candidates(ToolId::Codex)[0].plan.is_none());
    executable(
        &entry,
        r#"@node "%~dp0\node_modules\@openai\codex\bin\codex.js" %*"#,
    );
    assert!(m.uninstall_candidates(ToolId::Codex)[0].plan.is_some());
}

#[cfg(unix)]
#[test]
fn grok_uninstall_removes_its_historical_binaries_and_aliases_but_keeps_user_files() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let home = root.path();
    let grok = home.join(".grok");
    let a = grok.join("bin/grok-1.0.0");
    let b = grok.join("downloads/grok-1.0.1-macos-aarch64");
    for p in [&a, &b] {
        executable(p, [0xcf, 0xfa, 0xed, 0xfe]);
    }
    let entry = grok.join("bin/grok");
    symlink(&a, &entry).unwrap();
    symlink(&a, grok.join("bin/agent")).unwrap();
    let config = grok.join("config.toml");
    executable(&config, "[cli]\ninstaller='internal'\n");
    let user = grok.join("bin/grok-user-notes");
    executable(&user, "keep");
    let mut env = env(home, HostPlatform::MacOs);
    env.command_directories = vec![grok.join("bin")];
    let m = manager(env, Arc::new(Executor::default()));
    let plan = m.uninstall_candidates(ToolId::GrokBuild)[0]
        .plan
        .clone()
        .unwrap();
    m.start_uninstall(ToolId::GrokBuild, plan).unwrap();
    assert_eq!(done(&m, ToolId::GrokBuild).stage, InstallStage::Succeeded);
    assert!(!a.exists() && !b.exists() && !entry.exists());
    assert!(config.exists() && user.exists());
    assert!(std::fs::symlink_metadata(grok.join("bin/agent")).is_err());
}

#[test]
fn windows_minimax_custom_receipt_keeps_the_installer_at_its_original_root() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path();
    let custom = home.join("custom minimax");
    executable(&custom.join("mcode.cmd"), "fixture");
    std::fs::write(
        custom.join("install.json"),
        serde_json::json!({"product":"minimax-code","updateOwner":"npm-prefix","prefix":custom})
            .to_string(),
    )
    .unwrap();
    let mut env = env(home, HostPlatform::Windows);
    env.command_directories = vec![custom.clone()];
    let m = manager(env, Arc::new(Executor::default()));
    let u = m.inspect_update(ToolId::MiniMaxCode, Ok("2.0.0".into()));
    assert!(u.can_upgrade, "{}", u.note);
    let command = u.upgrade_command.unwrap();
    assert!(command.contains("MCODE_INSTALL_DIR"));
    assert!(command.contains("custom minimax"));
    assert!(command.contains("https://filecdn.minimax.chat/public/install.ps1"));
}

#[cfg(unix)]
#[test]
fn an_upgrade_confirmation_is_bound_to_the_original_files_and_plan() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path();
    let entry = npm_install(home, ToolId::Codex, "@openai/codex");
    let e = Arc::new(Executor::default());
    let m = manager(env(home, HostPlatform::MacOs), e.clone());
    let u = m.inspect_update(ToolId::Codex, Ok("2.0.0".into()));
    assert!(u.can_upgrade);
    let revision = u.upgrade_revision.unwrap();
    assert!(m
        .start_upgrade_confirmed(ToolId::Codex, "2.0.0", "forged")
        .is_err());
    std::fs::write(entry.canonicalize().unwrap(), "changed after confirmation").unwrap();
    assert!(m
        .start_upgrade_confirmed(ToolId::Codex, "2.0.0", &revision)
        .is_err());
    assert!(e.steps.lock().unwrap().is_empty());
}

#[cfg(unix)]
#[test]
fn codex_standalone_update_preserves_its_install_and_data_directories() {
    use std::os::unix::fs::symlink;
    for platform in [HostPlatform::MacOs, HostPlatform::Windows] {
        let root = tempfile::tempdir().unwrap();
        let home = root.path();
        let command = if platform == HostPlatform::Windows {
            "codex.exe"
        } else {
            "codex"
        };
        let real = home
            .join("custom-codex-home/packages/standalone/releases/1.0.0/bin")
            .join(command);
        executable(&real, [0xcf, 0xfa, 0xed, 0xfe]);
        std::fs::create_dir_all(home.join("bin")).unwrap();
        symlink(&real, home.join("bin").join(command)).unwrap();
        let m = manager(env(home, platform), Arc::new(Executor::default()));
        let u = m.inspect_update(ToolId::Codex, Ok("2.0.0".into()));
        assert!(u.can_upgrade, "{}", u.note);
        let command = u.upgrade_command.unwrap();
        assert!(command.contains("CODEX_INSTALL_DIR") && command.contains("CODEX_HOME"));
        assert!(command.contains("custom-codex-home"));
        assert!(!command.contains("@openai/codex@latest"));
    }
}
