mod support;
use std::path::Path;
use tokenmarket_core::{
    connections::ConnectContext,
    inventory::{HostPlatform, InstallationEnvironment, ToolInventory},
    ToolId,
};

#[cfg(unix)]
fn script(path: &Path, version: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, format!("#!/bin/sh\nprintf '%s\\n' '{version}'\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}
#[cfg(unix)]
#[tokio::test]
async fn known_opencode_v2_is_rejected_before_key_issuance_or_configuration_mutation() {
    let root = tempfile::tempdir().unwrap();
    script(&root.path().join("bin/opencode"), "2.0.1");
    let probe_marker = root.path().join("version-probed");
    std::fs::write(
        root.path().join("bin/opencode"),
        format!(
            "#!/bin/sh\ntouch '{}'\nprintf '%s\\n' '2.0.1'\n",
            probe_marker.display()
        ),
    )
    .unwrap();
    let inventory = ToolInventory::new(InstallationEnvironment {
        platform: HostPlatform::MacOs,
        home: root.path().into(),
        command_directories: vec![root.path().join("bin")],
        application_directories: vec![],
        executable_extensions: vec![],
    });
    inventory.list().unwrap();
    assert!(
        !probe_marker.exists(),
        "inventory must not execute version probes"
    );
    let config = root.path().join("opencode.jsonc");
    let original = "{\n// user's choice\n\"model\":\"manual/original\",\n}\n";
    std::fs::write(&config, original).unwrap();
    let manager = support::manager(
        root.path().join("state"),
        "http://127.0.0.1:1".into(),
        "http://127.0.0.1:1".into(),
    );
    let result = manager
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
        .await;
    assert!(result.unwrap_err().to_string().contains("OpenCode 2"));
    assert!(probe_marker.exists());
    assert_eq!(std::fs::read_to_string(config).unwrap(), original);
    assert!(manager.list().unwrap().is_empty());
    assert!(!root.path().join("state/keys.json").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn a_v2_application_is_not_hidden_by_a_compatible_v1_command() {
    let root = tempfile::tempdir().unwrap();
    script(&root.path().join("bin/opencode"), "1.19.5");
    let contents = root.path().join("Applications/OpenCode.app/Contents");
    script(
        &contents.join("MacOS/OpenCode"),
        "this GUI must not be launched",
    );
    std::fs::write(contents.join("Info.plist"), "<plist version=\"1.0\"><dict><key>CFBundleExecutable</key><string>OpenCode</string><key>CFBundleShortVersionString</key><string>2.0</string></dict></plist>").unwrap();
    let inventory = ToolInventory::new(InstallationEnvironment {
        platform: HostPlatform::MacOs,
        home: root.path().into(),
        command_directories: vec![root.path().join("bin")],
        application_directories: vec![root.path().join("Applications")],
        executable_extensions: vec![],
    });
    assert!(inventory.is_installed(ToolId::OpenCode));
    let config = root.path().join("opencode.jsonc");
    std::fs::write(&config, "{\"model\":\"manual/model\"}").unwrap();
    let manager = support::manager(
        root.path().join("state"),
        "http://127.0.0.1:1".into(),
        "http://127.0.0.1:1".into(),
    );
    let error = manager
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
    assert!(error.to_string().contains("OpenCode 2"));
    assert_eq!(
        std::fs::read_to_string(config).unwrap(),
        "{\"model\":\"manual/model\"}"
    );
    assert!(manager.list().unwrap().is_empty());
}
