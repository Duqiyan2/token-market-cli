mod support;
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tokenmarket_core::{
    connections::{ConnectContext, ConnectionTestRequest, DisconnectRequest},
    inventory::{HostPlatform, InstallationEnvironment, ToolInventory},
    ToolId,
};

fn write(path: &Path, value: &Value) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}
fn data(home: &Path) -> PathBuf {
    home.join("Library/Application Support/qianfan-desktop-app")
}
fn auth(home: &Path, user: &str, mode: &str) {
    write(
        &data(home).join("auth.json"),
        &json!({"activeProviderType":mode,"activeProfileId":"active","accountProfiles":[{"profileId":"active","loginType":mode,"bceUserId":user}],"cookies":"never displayed"}),
    );
}
fn fixture(home: &Path) -> ToolInventory {
    let bundle = home.join("Applications/DuMate.app/Contents");
    std::fs::create_dir_all(bundle.join("MacOS")).unwrap();
    let binary = bundle.join("MacOS/DuMate");
    std::fs::write(&binary, "fixture; never execute").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    std::fs::write(bundle.join("Info.plist"),r#"<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleExecutable</key><string>DuMate</string><key>CFBundleIdentifier</key><string>com.baidu.qianfan.desktop</string><key>CFBundleShortVersionString</key><string>1.0.89</string></dict></plist>"#).unwrap();
    auth(home, "personal-user", "bce");
    ToolInventory::new(InstallationEnvironment {
        platform: HostPlatform::MacOs,
        home: home.into(),
        command_directories: vec![home.join("bin")],
        application_directories: vec![home.join("Applications")],
        executable_extensions: vec![],
    })
}
fn context() -> ConnectContext {
    ConnectContext {
        session_token: "desktop-secret".into(),
        owner_id: "u1".into(),
        workspace_id: "42".into(),
    }
}
fn catalogue() -> Value {
    json!({"data":[
        {"id":"embed","model_types":["EMBEDDING"],"context_window_tokens":65536,"max_output_tokens":4096},
        {"id":"glm-5.3","model_types":["TEXT_GENERATION"],"context_window_tokens":200000,"max_output_tokens":128000},
        {"id":"existing","model_types":["TEXT_GENERATION"],"context_window_tokens":65536,"max_output_tokens":4096}
    ]})
}

async fn server(
    catalogue: Value,
) -> (String, Arc<Mutex<Vec<String>>>, tokio::task::JoinHandle<()>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let requests = Arc::new(Mutex::new(vec![]));
    let captured = requests.clone();
    let task = tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = vec![];
            loop {
                let mut chunk = [0; 4096];
                let n = socket.read(&mut chunk).await.unwrap();
                if n == 0 {
                    break;
                }
                bytes.extend_from_slice(&chunk[..n]);
                if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                    let length = String::from_utf8_lossy(&bytes[..end])
                        .lines()
                        .find_map(|line| {
                            line.to_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    if bytes.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let request = String::from_utf8(bytes).unwrap();
            let response = if request.starts_with("GET /v1/models ") {
                catalogue.clone()
            } else if request.starts_with("POST /v1/chat/completions ") {
                json!({"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"OK"}}]})
            } else {
                assert!(request.contains("\"tool\":\"dumate\""));
                json!({"success":true,"data":{"api_key":"sk-dumate-fixture","key_id":"9030","owner":{"id":"u1","name":"Ada"},"bound_workspace":{"id":"42","name":"Research"}}})
            };
            captured.lock().unwrap().push(request);
            let body = response.to_string();
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
        }
    });
    (url, requests, task)
}

#[test]
fn dumate_uses_personal_account_preferences_and_gui_installation() {
    let root = tempfile::tempdir().unwrap();
    let inventory = fixture(root.path());
    assert_eq!(tokenmarket_core::tool_id("dumate").unwrap(), ToolId::DuMate);
    assert_eq!(serde_json::to_value(ToolId::DuMate).unwrap(), "dumate");
    assert_eq!(
        inventory.config_path(ToolId::DuMate),
        data(root.path())
            .join("qianfan_desk_xdg/personal-user/config/desktop-main/dumate-app.json")
    );
    assert!(inventory.is_installed(ToolId::DuMate));
    assert_eq!(
        inventory
            .terminal_launch_plan(ToolId::DuMate)
            .unwrap()
            .program,
        "open"
    );
    let env = InstallationEnvironment {
        platform: HostPlatform::Windows,
        home: root.path().into(),
        command_directories: vec![],
        application_directories: vec![root.path().join("Programs")],
        executable_extensions: vec![".EXE".into()],
    };
    let windows = ToolInventory::new(env.clone());
    assert!(!windows.is_installed(ToolId::DuMate));
    std::fs::create_dir_all(root.path().join("Programs/DuMate")).unwrap();
    std::fs::write(root.path().join("Programs/DuMate/DuMate.exe"), "fixture").unwrap();
    assert!(windows.is_installed(ToolId::DuMate));
    write(
        &root
            .path()
            .join("AppData/Roaming/qianfan-desktop-app/auth.json"),
        &json!({"activeProviderType":"bce","activeProfileId":"p","accountProfiles":[{"profileId":"p","loginType":"bce","bceUserId":"windows-user"}]}),
    );
    assert!(windows
        .config_path(ToolId::DuMate)
        .ends_with("qianfan_desk_xdg/windows-user/config/desktop-main/dumate-app.json"));
    let plan =
        tokenmarket_core::installation::recipes::plan(ToolId::DuMate, &env, "x86_64").unwrap();
    assert_eq!(plan.version, "1.0.89");
    assert!(plan.steps[0]
        .download
        .as_ref()
        .unwrap()
        .0
        .ends_with("DuMate-Setup-1.0.89-x64.exe"));
    assert!(plan.steps[1].args.join(" ").contains("SHA512"));
    assert_eq!(
        plan.steps[2].stage,
        tokenmarket_core::installation::InstallStage::WaitingForUser
    );
    assert!(
        tokenmarket_core::installation::recipes::plan(ToolId::DuMate, &env, "aarch64").is_err()
    );
}

#[tokio::test]
async fn personal_connection_prefers_default_restores_provider_and_never_tests_implicitly() {
    let root = tempfile::tempdir().unwrap();
    let inventory = fixture(root.path());
    let path = inventory.config_path(ToolId::DuMate);
    let original = json!({"theme":"dark","customLLMProvider":{"baseURL":"https://old.invalid/v1","apiKey":"old-secret","model":"existing","contextLimit":100000,"outputLimit":5000,"futureOption":true}});
    write(&path, &original);
    let generated = path
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("opencode/opencode.json");
    write(&generated, &json!({"generated":"leave alone"}));
    let (url, requests, task) = server(catalogue()).await;
    let manager = support::manager(root.path().join("state"), url.clone(), url.clone());
    let connection = manager
        .connect(context(), ToolId::DuMate, &inventory, path.clone())
        .await
        .unwrap();
    assert_eq!(connection.models, ["glm-5.3"]);
    assert_eq!(connection.protocol, "OPENAI_CHAT_COMPLETIONS");
    let mut configured: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        configured["customLLMProvider"]["baseURL"],
        format!("{url}/v1")
    );
    assert_eq!(
        configured["customLLMProvider"]["apiKey"],
        "sk-dumate-fixture"
    );
    assert_eq!(configured["customLLMProvider"]["contextLimit"], 200000);
    assert_eq!(configured["customLLMProvider"]["futureOption"], true);
    let replaced = manager
        .reconnect(
            context(),
            ToolId::DuMate,
            &connection.key_id,
            &inventory,
            path.clone(),
        )
        .await
        .unwrap();
    assert!(replaced.key_reused);
    assert_eq!(requests.lock().unwrap().len(), 3); // issue + two catalogues, no model call
    let tested = manager
        .test_connection(
            ToolId::DuMate,
            &connection.key_id,
            ConnectionTestRequest {
                confirmed: true,
                model: None,
            },
        )
        .await
        .unwrap();
    assert!(tested.passed);
    assert_eq!(tested.model, "glm-5.3");
    configured["theme"] = json!("light");
    write(&path, &configured);
    manager
        .disconnect(
            ToolId::DuMate,
            &connection.key_id,
            None,
            DisconnectRequest::default(),
        )
        .await
        .unwrap();
    let restored: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(restored["theme"], "light");
    assert_eq!(restored["customLLMProvider"], original["customLLMProvider"]);
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(generated).unwrap()).unwrap(),
        json!({"generated":"leave alone"})
    );
    assert_eq!(
        requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.starts_with("POST /v1/"))
            .count(),
        1
    );
    task.abort();
}

#[tokio::test]
async fn default_model_and_missing_metadata_never_produce_a_fake_catalogue() {
    for valid in [true, false] {
        let root = tempfile::tempdir().unwrap();
        let inventory = fixture(root.path());
        let path = inventory.config_path(ToolId::DuMate);
        write(&path, &json!({"customLLMProvider":null,"theme":"dark"}));
        let before = std::fs::read(&path).unwrap();
        let mut catalog = catalogue();
        if !valid {
            for row in catalog["data"].as_array_mut().unwrap() {
                row.as_object_mut().unwrap().remove("max_output_tokens");
            }
        }
        let (url, requests, task) = server(catalog).await;
        let manager = support::manager(root.path().join("state"), url.clone(), url);
        let result = manager
            .connect(context(), ToolId::DuMate, &inventory, path.clone())
            .await;
        if valid {
            let connection = result.unwrap();
            assert_eq!(connection.models, ["glm-5.3"]);
            // An account switch invalidates Test, but Disconnect still restores its original account file.
            auth(root.path(), "another-user", "bce");
            assert_eq!(
                manager.summaries().unwrap()[0].1.state,
                "configuration_changed"
            );
            assert!(manager
                .test_connection(
                    ToolId::DuMate,
                    &connection.key_id,
                    ConnectionTestRequest {
                        confirmed: true,
                        model: None
                    }
                )
                .await
                .is_err());
            manager
                .disconnect(
                    ToolId::DuMate,
                    &connection.key_id,
                    None,
                    DisconnectRequest::default(),
                )
                .await
                .unwrap();
            assert_eq!(std::fs::read(&path).unwrap(), before);
            assert!(!inventory.config_path(ToolId::DuMate).exists());
        } else {
            assert!(result.unwrap_err().to_string().contains("元数据"));
            assert_eq!(std::fs::read(&path).unwrap(), before);
            assert!(manager.list().unwrap().is_empty());
        }
        assert_eq!(requests.lock().unwrap().len(), 2);
        task.abort();
    }
}

#[tokio::test]
async fn wrong_account_version_or_malformed_preferences_fail_before_key_issuance() {
    let root = tempfile::tempdir().unwrap();
    let inventory = fixture(root.path());
    let path = inventory.config_path(ToolId::DuMate);
    let manager = support::manager(
        root.path().join("state"),
        "http://127.0.0.1:1".into(),
        "http://127.0.0.1:1".into(),
    );
    for (user, mode) in [("enterprise-user", "enterprise"), ("../escape", "bce")] {
        auth(root.path(), user, mode);
        assert!(manager
            .connect(context(), ToolId::DuMate, &inventory, path.clone())
            .await
            .is_err());
        assert!(!root.path().join("state/keys.json").exists());
    }
    auth(root.path(), "personal-user", "bce");
    write(&path, &json!({"customLLMProvider":"invalid"}));
    assert!(manager
        .connect(context(), ToolId::DuMate, &inventory, path.clone())
        .await
        .unwrap_err()
        .to_string()
        .contains("对象"));
    assert!(!root.path().join("state/keys.json").exists());
    let plist = root
        .path()
        .join("Applications/DuMate.app/Contents/Info.plist");
    let text = std::fs::read_to_string(&plist)
        .unwrap()
        .replace("1.0.89", "2.0.0");
    std::fs::write(plist, text).unwrap();
    assert!(manager
        .connect(context(), ToolId::DuMate, &inventory, path)
        .await
        .unwrap_err()
        .to_string()
        .contains("1.0.89"));
    assert!(!root.path().join("state/keys.json").exists());
}

async fn environment_switch_fixture() -> (
    tempfile::TempDir,
    ToolInventory,
    PathBuf,
    tokenmarket_core::connections::ConnectionManager,
    tokenmarket_core::connections::ConnectionManager,
    String,
    tokio::task::JoinHandle<()>,
    tokio::task::JoinHandle<()>,
) {
    let root = tempfile::tempdir().unwrap();
    let inventory = fixture(root.path());
    let path = inventory.config_path(ToolId::DuMate);
    let mut test_catalogue = catalogue();
    test_catalogue["data"][1]["id"] = json!("glm-5.2");
    let (test_url, _, test_task) = server(test_catalogue).await;
    let mut production_catalogue = catalogue();
    production_catalogue["data"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id":"glm-5.2", "model_types":["TEXT_GENERATION"],
            "context_window_tokens":128000, "max_output_tokens":32000
        }));
    let (production_url, _, production_task) = server(production_catalogue).await;
    let directory = root.path().join("state");
    let test_manager = support::manager(directory.clone(), test_url.clone(), test_url);
    let production_manager = support::manager(directory, production_url.clone(), production_url);
    let connected = test_manager
        .connect(context(), ToolId::DuMate, &inventory, path.clone())
        .await
        .unwrap();
    assert_eq!(connected.models, ["glm-5.2"]);
    (
        root,
        inventory,
        path,
        test_manager,
        production_manager,
        connected.key_id,
        test_task,
        production_task,
    )
}

#[tokio::test]
async fn other_environment_connection_is_identified_and_cannot_be_tested() {
    let (_root, _inventory, _path, test_manager, production, key_id, test_task, production_task) =
        environment_switch_fixture().await;
    assert_eq!(
        test_manager.summaries().unwrap()[0].1.state,
        "configured_untested"
    );
    assert!(
        test_manager
            .test_connection(
                ToolId::DuMate,
                &key_id,
                ConnectionTestRequest {
                    confirmed: true,
                    model: None
                }
            )
            .await
            .unwrap()
            .passed
    );
    assert_eq!(
        production.summaries().unwrap()[0].1.state,
        "environment_mismatch"
    );
    assert!(production.summaries().unwrap()[0].1.test_result.is_none());
    let error = production
        .test_connection(
            ToolId::DuMate,
            &key_id,
            ConnectionTestRequest {
                confirmed: true,
                model: None,
            },
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("环境"));
    test_task.abort();
    production_task.abort();
}

#[tokio::test]
async fn unavailable_default_preserves_a_valid_existing_model() {
    let root = tempfile::tempdir().unwrap();
    let inventory = fixture(root.path());
    let path = inventory.config_path(ToolId::DuMate);
    write(&path, &json!({"customLLMProvider":{"model":"existing"}}));
    let mut catalog = catalogue();
    // A listed default without limits is unavailable for DuMate.
    catalog["data"][1]
        .as_object_mut()
        .unwrap()
        .remove("max_output_tokens");
    let (url, _, task) = server(catalog).await;
    let manager = support::manager(root.path().join("state"), url.clone(), url);
    let connection = manager
        .connect(context(), ToolId::DuMate, &inventory, path)
        .await
        .unwrap();
    assert_eq!(connection.models, ["existing"]);
    task.abort();
}

#[tokio::test]
async fn reconnect_from_test_environment_selects_production_default_model() {
    let (_root, inventory, path, test_manager, production, key_id, test_task, production_task) =
        environment_switch_fixture().await;
    let result = production
        .reconnect(context(), ToolId::DuMate, &key_id, &inventory, path.clone())
        .await
        .unwrap();
    assert!(!result.key_reused);
    assert_eq!(result.connection.models, ["glm-5.3"]);
    let configured: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(configured["customLLMProvider"]["model"], "glm-5.3");
    assert_eq!(
        production.summaries().unwrap()[0].1.state,
        "configured_untested"
    );
    assert_eq!(
        test_manager.summaries().unwrap()[0].1.state,
        "environment_mismatch"
    );
    test_task.abort();
    production_task.abort();
}
