use tokenmarket_core::{
    inventory::{HostPlatform, InstallationEnvironment, ToolInventory},
    ToolId,
};

fn environment(root: &std::path::Path, platform: HostPlatform) -> InstallationEnvironment {
    InstallationEnvironment {
        platform,
        home: root.join("home"),
        command_directories: vec![root.join("bin")],
        application_directories: vec![root.join("Applications")],
        executable_extensions: vec![".EXE".into(), ".CMD".into(), ".BAT".into(), ".COM".into()],
    }
}

#[test]
fn missing_tools_offer_official_downloads_and_refuse_connection_even_with_leftover_config() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("home/.dsh/profiles/desktop")).unwrap();
    std::fs::create_dir_all(root.path().join("Applications/WorkBuddy.app")).unwrap();
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    let entries = inventory.list().unwrap();
    assert_eq!(entries.len(), 15);
    assert_eq!(
        entries.iter().map(|e| e.name).collect::<Vec<_>>(),
        [
            "Claude Code",
            "Codex",
            "WorkBuddy",
            "DeepSeek Harness",
            "OpenClaw",
            "OpenCode",
            "Grok Build",
            "Hermes",
            "Pi",
            "MiniMax Code",
            "Oh My Pi",
            "Kimi Code",
            "百度搭子 DuMate（个人版）",
            "百度搭子 DuMate（企业版）",
            "千问办公"
        ]
    );
    assert!(entries.iter().all(|e| !e.installed && !e.can_connect));
    assert_eq!(
        entries[3].download_url,
        "https://www.deepseek.com/download/"
    );
    for entry in entries {
        assert!(entry.download_url.starts_with("https://"));
        assert!(inventory.require_installed(entry.id).is_err());
    }
    assert!(!root.path().join("home/.tokenmarket").exists());
}

fn executable(path: &std::path::Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "fixture; never execute").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn command_installation_is_freshly_detected_and_removed_without_running_the_tool() {
    let root = tempfile::tempdir().unwrap();
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    for (tool, command) in [
        (ToolId::ClaudeCode, "claude"),
        (ToolId::Codex, "codex"),
        (ToolId::WorkBuddy, "workbuddy"),
        (ToolId::DeepSeekHarness, "dsh"),
        (ToolId::OpenClaw, "openclaw"),
        (ToolId::OpenCode, "opencode"),
        (ToolId::GrokBuild, "grok"),
        (ToolId::Pi, "pi"),
        (ToolId::OhMyPi, "omp"),
        (ToolId::KimiCode, "kimi"),
    ] {
        assert!(inventory.require_installed(tool).is_err());
        let binary = root.path().join("bin").join(command);
        executable(&binary);
        assert!(
            inventory
                .list()
                .unwrap()
                .into_iter()
                .find(|e| e.id == tool)
                .unwrap()
                .can_connect
        );
        inventory.require_installed(tool).unwrap();
        std::fs::remove_file(binary).unwrap();
        assert!(inventory.require_installed(tool).is_err());
    }
}

#[cfg(unix)]
#[test]
fn macos_bundles_require_the_declared_executable() {
    let root = tempfile::tempdir().unwrap();
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    for (tool, bundle, binary) in [
        (ToolId::WorkBuddy, "WorkBuddy.app", "Electron"),
        (
            ToolId::DeepSeekHarness,
            "DeepSeek Harness.app",
            "DeepSeek Harness",
        ),
        (ToolId::OpenClaw, "OpenClaw.app", "OpenClaw"),
        (ToolId::OpenCode, "OpenCode.app", "OpenCode"),
    ] {
        let contents = root
            .path()
            .join("Applications")
            .join(bundle)
            .join("Contents");
        std::fs::create_dir_all(&contents).unwrap();
        std::fs::write(contents.join("Info.plist"), format!(r#"<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleExecutable</key><string>{binary}</string></dict></plist>"#)).unwrap();
        assert!(inventory.require_installed(tool).is_err());
        executable(&contents.join("MacOS").join(binary));
        inventory.require_installed(tool).unwrap();
    }
}

#[test]
fn windows_qwenwork_detects_the_active_version_behind_the_official_launcher() {
    let root = tempfile::tempdir().unwrap();
    let programs = root.path().join("home/AppData/Local/Programs");
    let install = programs.join("QwenWorkCN");
    let main = install.join("1.5.0-26110306/QwenWorkCN.exe");
    executable(&install.join("Launcher.exe"));
    executable(&main);
    std::fs::create_dir_all(main.parent().unwrap().join("resources")).unwrap();
    std::fs::write(
        main.parent().unwrap().join("resources/app.asar"),
        "inert archive",
    )
    .unwrap();
    std::fs::write(
        install.join("updater.cfg"),
        r#"{"currentVersion":"1.5.0-26110306","appExecutableName":"QwenWorkCN.exe","pendingActivation":false,"installingVersions":[]}"#,
    ).unwrap();
    let mut environment = environment(root.path(), HostPlatform::Windows);
    environment.application_directories = vec![programs];
    let inventory = ToolInventory::new(environment);
    assert!(
        inventory.is_installed(ToolId::QwenWork),
        "official Launcher.exe + updater.cfg + versioned executable was reported missing"
    );
    assert_eq!(inventory.application_path(ToolId::QwenWork), Some(main));
    let entry = inventory
        .list()
        .unwrap()
        .into_iter()
        .find(|e| e.id == ToolId::QwenWork)
        .unwrap();
    assert!(entry.installed);
    assert!(entry.can_connect);
}

#[test]
fn windows_qwenwork_tracks_active_versions_and_ignores_incomplete_or_unrelated_launchers() {
    let root = tempfile::tempdir().unwrap();
    let install = root.path().join("custom install/千问办公 & QwenWorkCN");
    let mut environment = environment(root.path(), HostPlatform::Windows);
    // A registered custom InstallLocation is already the application root.
    environment.application_directories = vec![install.clone()];
    let inventory = ToolInventory::new(environment);
    executable(&install.join("Launcher.exe"));
    assert!(!inventory.is_installed(ToolId::QwenWork));
    let old = install.join("1.5.0-26110306/QwenWorkCN.exe");
    let staged = install.join("1.6.0-26110401/QwenWorkCN.exe");
    executable(&old);
    executable(&staged);
    let config = install.join("updater.cfg");
    let write_version = |version: &str, executable: &str| {
        std::fs::write(
            &config,
            serde_json::json!({
                "currentVersion":version,"appExecutableName":executable,
                "installingVersions":["1.6.0-26110401"]
            })
            .to_string(),
        )
        .unwrap();
    };
    write_version("1.5.0-26110306", "QwenWorkCN.exe");
    assert_eq!(inventory.application_path(ToolId::QwenWork), Some(old));
    write_version("1.6.0-26110401", "QwenWorkCN.exe");
    assert_eq!(
        inventory.application_path(ToolId::QwenWork),
        Some(staged.clone())
    );
    std::fs::remove_file(staged).unwrap();
    assert!(!inventory.is_installed(ToolId::QwenWork));
    for version in [
        "",
        "..",
        "../1.5.0-26110306",
        "1.5.0/../../outside",
        r"1.5.0\..\outside",
        r"C:\outside",
        "/outside",
    ] {
        write_version(version, "QwenWorkCN.exe");
        assert!(
            !inventory.is_installed(ToolId::QwenWork),
            "invalid version: {version}"
        );
    }
    write_version("1.5.0-26110306", "OtherApp.exe");
    assert!(!inventory.is_installed(ToolId::QwenWork));
    std::fs::write(config, "invalid JSON").unwrap();
    assert!(!inventory.is_installed(ToolId::QwenWork));
}

#[test]
fn windows_inventory_recognizes_pathext_launchers_and_installed_desktop_executables() {
    let root = tempfile::tempdir().unwrap();
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::Windows));
    for (tool, launcher) in [
        (ToolId::ClaudeCode, "CLAUDE.EXE"),
        (ToolId::Codex, "codex.cmd"),
        (ToolId::DeepSeekHarness, "dsh.BAT"),
        (ToolId::OpenClaw, "openclaw.com"),
        (ToolId::OpenCode, "opencode.exe"),
    ] {
        executable(&root.path().join("bin").join(launcher));
        inventory.require_installed(tool).unwrap();
    }
    let desktop = root.path().join("Applications/WorkBuddy/WorkBuddy.exe");
    executable(&desktop);
    inventory.require_installed(ToolId::WorkBuddy).unwrap();
    let harness = root
        .path()
        .join("Applications/DeepSeek Harness/DeepSeek Harness.exe");
    executable(&harness);
    std::fs::remove_file(root.path().join("bin/dsh.BAT")).unwrap();
    inventory
        .require_installed(ToolId::DeepSeekHarness)
        .unwrap();
    // A suffix omitted from PATHEXT must not silently become a command.
    let mut restricted = environment(root.path(), HostPlatform::Windows);
    restricted.executable_extensions = vec![".EXE".into()];
    assert!(ToolInventory::new(restricted)
        .require_installed(ToolId::Codex)
        .is_err());
}

#[test]
fn launch_plan_uses_fresh_windows_cmd_directory_without_interpolating_its_shell_characters() {
    let root = tempfile::tempdir().unwrap();
    let mut old = environment(root.path(), HostPlatform::Windows);
    let newly_installed = root.path().join("new install & plugins");
    executable(&newly_installed.join("OPENCLAW.CMD"));
    assert!(ToolInventory::new(old.clone())
        .launch_plan(ToolId::OpenClaw)
        .is_err());
    old.command_directories.insert(0, newly_installed.clone());
    let plan = ToolInventory::new(old)
        .launch_plan(ToolId::OpenClaw)
        .unwrap();
    assert_eq!(plan.program, "cmd");
    assert_eq!(plan.args, ["/c", "start", "", "cmd", "/k", "openclaw tui"]);
    assert!(plan.environment[0]
        .1
        .starts_with(&newly_installed.to_string_lossy().to_string()));
    assert!(!plan.args.iter().any(|v| v.contains("plugins")));
    assert!(!plan.wait_for_exit);
}

#[cfg(unix)]
#[test]
fn macos_launch_plan_uses_nvm_target_and_quotes_paths_for_terminal_and_applescript() {
    let root = tempfile::tempdir().unwrap();
    let mut env = environment(root.path(), HostPlatform::MacOs);
    let directory = root
        .path()
        .join(".nvm/versions/node/v22 'quoted' & new/bin");
    executable(&directory.join("codex"));
    env.command_directories.insert(0, directory.clone());
    let plan = ToolInventory::new(env).launch_plan(ToolId::Codex).unwrap();
    assert_eq!(plan.program, "osascript");
    assert!(plan.args[3].contains("v22") && plan.args[3].contains("codex"));
    assert!(plan.args[3].contains("'\\\"'\\\"'quoted'\\\"'\\\"'"));
    assert!(plan.args[3].contains("env PATH="));
    assert!(plan.wait_for_exit);
}

#[test]
fn linux_terminal_launch_resolves_private_wrappers_and_process_environment_without_a_shell_search()
{
    let root = tempfile::tempdir().unwrap();
    let mut env = environment(root.path(), HostPlatform::Unix);
    let prefix = root.path().join("home/.tokenmarket/tools/minimax-code/bin");
    env.command_directories = vec![prefix.clone()];
    executable(&prefix.join("mcode"));
    let inventory = ToolInventory::new(env);
    let plan = inventory.launch_plan(ToolId::MiniMaxCode).unwrap();
    assert_eq!(std::path::PathBuf::from(plan.program), prefix.join("mcode"));
    assert!(plan.args.is_empty());
    assert!(plan.wait_for_exit);
    assert!(plan
        .environment
        .iter()
        .any(|(key, value)| key == "PATH"
            && value.starts_with(&prefix.to_string_lossy().to_string())));
    assert!(plan
        .environment
        .iter()
        .any(|(key, value)| key == "MINIMAX_DATA_DIR" && value.ends_with(".minimax")));
}

#[cfg(unix)]
#[test]
fn installation_locations_preserve_path_order_and_deduplicate_aliases_without_running_tools() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let (npm, local, alias) = (
        home.join(".npm-global/bin"),
        home.join(".local/bin"),
        root.path().join("alias"),
    );
    let package = home.join(".npm-global/lib/node_modules/@openai/codex");
    std::fs::create_dir_all(package.join("bin")).unwrap();
    std::fs::create_dir_all(&npm).unwrap();
    std::fs::create_dir_all(&local).unwrap();
    std::fs::create_dir_all(&alias).unwrap();
    let marker = root.path().join("should-not-run");
    std::fs::write(
        package.join("bin/codex.js"),
        format!("#!/bin/sh\ntouch '{}'\n", marker.display()),
    )
    .unwrap();
    std::fs::set_permissions(
        package.join("bin/codex.js"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    std::fs::write(
        package.join("package.json"),
        r#"{"name":"@openai/codex","version":"0.163.0"}"#,
    )
    .unwrap();
    symlink(package.join("bin/codex.js"), npm.join("codex")).unwrap();
    symlink(npm.join("codex"), alias.join("codex")).unwrap();
    std::fs::write(local.join("codex"), [0xcf, 0xfa, 0xed, 0xfe, 0, 0, 0, 0]).unwrap();
    std::fs::set_permissions(local.join("codex"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let env = InstallationEnvironment {
        home: home.clone(),
        platform: HostPlatform::MacOs,
        command_directories: vec![local.clone(), npm.clone(), npm.clone(), alias],
        application_directories: vec![],
        executable_extensions: vec![],
    };
    let inventory = ToolInventory::new(env);
    let entries = inventory.list().unwrap();
    let codex = entries.iter().find(|e| e.id == ToolId::Codex).unwrap();
    assert_eq!(codex.installations.len(), 2);
    assert_eq!(
        codex.installations[0].path,
        local.join("codex").display().to_string()
    );
    assert!(codex.installations[0].selected);
    assert_eq!(codex.installations[0].method, "原生程序");
    assert_eq!(codex.installations[1].method, "npm");
    assert!(!codex.installations[1].selected);
    assert_eq!(
        inventory
            .terminal_launch_plan(ToolId::Codex)
            .unwrap()
            .program,
        codex.installations[0].path
    );
    assert!(!marker.exists());
    assert!(!home.join(".tokenmarket").exists());
}
#[cfg(unix)]
#[test]
fn claude_and_codex_keep_path_order_while_identifying_verified_npm() {
    for platform in [
        HostPlatform::MacOs,
        HostPlatform::Windows,
        HostPlatform::Unix,
    ] {
        for (tool, package) in [
            (ToolId::ClaudeCode, "@anthropic-ai/claude-code"),
            (ToolId::Codex, "@openai/codex"),
        ] {
            let root = tempfile::tempdir().unwrap();
            let mut env = environment(root.path(), platform);
            let native = root.path().join("native");
            let npm = root.path().join("npm");
            let command = tool.command();
            let (native_path, npm_path, package_root) = if platform == HostPlatform::Windows {
                (
                    native.join(format!("{command}.exe")),
                    npm.join(format!("{command}.cmd")),
                    npm.join("node_modules").join(package),
                )
            } else {
                (
                    native.join(command),
                    npm.join("bin").join(command),
                    npm.join("lib/node_modules").join(package),
                )
            };
            executable(&native_path);
            executable(&npm_path);
            #[cfg(unix)]
            if platform != HostPlatform::Windows {
                let target = package_root.join("bin/cli.js");
                executable(&target);
                std::fs::remove_file(&npm_path).unwrap();
                std::os::unix::fs::symlink(target, &npm_path).unwrap();
            }
            std::fs::create_dir_all(&package_root).unwrap();
            let metadata = package_root.join("package.json");
            std::fs::write(&metadata, r#"{"name":"unrelated-package"}"#).unwrap();
            env.command_directories = vec![native, npm_path.parent().unwrap().into()];
            let inventory = ToolInventory::new(env);
            assert_eq!(
                inventory.installation_locations(tool)[0].path,
                native_path.to_string_lossy()
            );
            std::fs::write(&metadata, format!(r#"{{"name":"{package}"}}"#)).unwrap();
            let locations = inventory.installation_locations(tool);
            assert_eq!(
                locations[0].path,
                native_path.to_string_lossy(),
                "{platform:?}: {tool}"
            );
            assert_eq!(locations[1].method, "npm");
            assert!(locations[0].selected);
            assert!(!locations[1].selected);
            let launch = inventory.terminal_launch_plan(tool).unwrap();
            if platform == HostPlatform::Windows {
                let path = launch
                    .environment
                    .iter()
                    .find(|(key, _)| key == "PATH")
                    .unwrap();
                assert_eq!(
                    path.1.split(';').next().unwrap(),
                    native_path.parent().unwrap().to_str().unwrap()
                );
            } else {
                assert_eq!(launch.program, native_path.to_string_lossy());
            }
            std::fs::remove_file(&npm_path).unwrap();
            assert_eq!(
                inventory.installation_locations(tool)[0].path,
                native_path.to_string_lossy()
            );
        }
    }
}

#[test]
fn config_directory_is_the_effective_config_parent_and_missing_tools_describe_their_recipe() {
    let root = tempfile::tempdir().unwrap();
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    let entries = inventory.list().unwrap();
    for entry in &entries {
        assert_eq!(
            std::path::Path::new(&entry.config_path)
                .parent()
                .unwrap()
                .display()
                .to_string(),
            entry.config_directory
        );
        assert_eq!(
            std::path::Path::new(&entry.config_path).is_file(),
            entry.config_exists
        );
        assert!(entry.installations.is_empty());
    }
    assert!(entries
        .iter()
        .find(|e| e.id == ToolId::Codex)
        .unwrap()
        .installation_method
        .contains("npm"));
    assert!(entries
        .iter()
        .find(|e| e.id == ToolId::Pi)
        .unwrap()
        .installation_method
        .contains("npm"));
    assert!(entries
        .iter()
        .find(|e| e.id == ToolId::GrokBuild)
        .unwrap()
        .installation_method
        .contains("官方安装脚本"));
    assert!(entries
        .iter()
        .find(|e| e.id == ToolId::Hermes)
        .unwrap()
        .installation_method
        .contains("安装脚本"));
}
