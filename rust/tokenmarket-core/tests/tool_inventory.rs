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
fn installation_locations_preserve_selection_and_deduplicate_aliases_without_running_tools() {
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
        command_directories: vec![npm.clone(), local.clone(), npm.clone(), alias],
        application_directories: vec![],
        executable_extensions: vec![],
    };
    let inventory = ToolInventory::new(env);
    let entries = inventory.list().unwrap();
    let codex = entries.iter().find(|e| e.id == ToolId::Codex).unwrap();
    assert_eq!(codex.installations.len(), 2);
    assert_eq!(
        codex.installations[0].path,
        npm.join("codex").display().to_string()
    );
    assert!(codex.installations[0].selected);
    assert_eq!(codex.installations[0].method, "npm");
    assert_eq!(codex.installations[1].method, "原生程序");
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
        .contains("官方安装器"));
    assert!(entries
        .iter()
        .find(|e| e.id == ToolId::Pi)
        .unwrap()
        .installation_method
        .contains("独立安装包"));
    assert!(entries
        .iter()
        .find(|e| e.id == ToolId::GrokBuild)
        .unwrap()
        .installation_method
        .contains("npm"));
    assert!(entries
        .iter()
        .find(|e| e.id == ToolId::Hermes)
        .unwrap()
        .installation_method
        .contains("安装脚本"));
}
