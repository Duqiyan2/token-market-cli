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
    server_with_revocation(catalogue, None).await
}
async fn server_with_revocation(
    catalogue: Value,
    revoke: Option<Arc<std::sync::atomic::AtomicU8>>,
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
                if let Some(grants) = &revoke {
                    grants.store(0, std::sync::atomic::Ordering::SeqCst);
                }
                catalogue.clone()
            } else if request.starts_with("POST /v1/chat/completions ") {
                catalogue.get("_mock_chat_response").cloned().unwrap_or_else(|| json!({"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"OK"}}]}))
            } else {
                assert!(
                    request.contains("\"tool\":\"dumate\"")
                        || request.contains("\"tool\":\"dumate-enterprise\"")
                );
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

fn enterprise_auth(home: &Path, user: &str, origin: &str, profile_id: &str, cookies: Value) {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
    let key = [17_u8; 32];
    let nonce = [29_u8; 12]; // fixture only, never used outside this isolated test directory
    let mut encrypted = serde_json::to_vec(&cookies).unwrap();
    let tag = LessSafeKey::new(UnboundKey::new(&AES_256_GCM, &key).unwrap())
        .seal_in_place_separate_tag(
            Nonce::assume_unique_for_key(nonce),
            Aad::empty(),
            &mut encrypted,
        )
        .unwrap();
    let mut blob = nonce.to_vec();
    blob.extend_from_slice(tag.as_ref());
    blob.extend(encrypted);
    write(
        &data(home).join("auth.json"),
        &json!({
            "activeProviderType":"enterprise", "activeProfileId":profile_id, "enterpriseApiBaseUrl":origin, "cookies": STANDARD.encode(&blob),
            "accountProfiles":[{"profileId":profile_id,"loginType":"enterprise","bceUserId":user,
                "enterpriseApiBaseUrl":origin,"encryptedCookies":STANDARD.encode(&blob)}]
        }),
    );
    std::fs::write(data(home).join(".cookie-key"), key).unwrap();
}
fn enterprise_cookies() -> Value {
    json!([
        {"name":"session","value":"dummy-enterprise-session","domain":"127.0.0.1","path":"/","secure":false},
        {"name":"bce-user-info","value":"\"dummy-csrf\"","domain":"127.0.0.1","path":"/api"},
        {"name":"wrong-domain","value":"exclude","domain":"elsewhere.invalid","path":"/"},
        {"name":"wrong-path","value":"exclude","domain":"127.0.0.1","path":"/private"},
        {"name":"expired","value":"exclude","domain":"127.0.0.1","expirationDate":1,"path":"/"},
        {"name":"secure-only","value":"exclude","domain":"127.0.0.1","secure":true,"path":"/"}
    ])
}
async fn permission_server() -> (
    String,
    Arc<std::sync::atomic::AtomicU8>,
    Arc<Mutex<Vec<String>>>,
    tokio::task::JoinHandle<()>,
) {
    use std::sync::atomic::{AtomicU8, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let grants = Arc::new(AtomicU8::new(3));
    let state = grants.clone();
    let requests = Arc::new(Mutex::new(vec![]));
    let captured = requests.clone();
    let task = tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = vec![0; 8192];
            let n = socket.read(&mut bytes).await.unwrap();
            let request = String::from_utf8(bytes[..n].to_vec()).unwrap();
            assert!(request.starts_with("GET /api/dumate-enterprise/app/v1/model"));
            assert!(!request.to_ascii_lowercase().contains("authorization:"));
            assert!(request.contains("session=dummy-enterprise-session"));
            assert!(request
                .to_ascii_lowercase()
                .contains("csrftoken: dummy-csrf"));
            assert!(!request.contains("exclude"));
            let white = request.starts_with("GET /api/dumate-enterprise/app/v1/modelwhite/");
            let flags = state.load(Ordering::SeqCst);
            let body = if white {
                json!({"switchs":{"model_controller":flags&2!=0}})
            } else {
                json!({"appCreateModelEnabled":flags&1!=0})
            }
            .to_string();
            let status = if flags & 4 != 0 {
                "302 Found\r\nLocation: https://elsewhere.invalid/"
            } else {
                "200 OK"
            };
            captured.lock().unwrap().push(request);
            socket.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
        }
    });
    (url, grants, requests, task)
}

#[tokio::test]
async fn enterprise_imports_multiple_untested_models_and_restores_after_vendor_probe_and_reorder() {
    let root = tempfile::tempdir().unwrap();
    let inventory = fixture(root.path());
    let (enterprise, _, permissions, permission_task) = permission_server().await;
    enterprise_auth(
        root.path(),
        "enterprise-user",
        &enterprise,
        "tenant-a",
        enterprise_cookies(),
    );
    let tool = ToolId::DuMateEnterprise;
    let path = inventory.config_path(tool);
    assert!(path.ends_with("enterprise-user/config/desktop-main/personal-models.json"));
    assert!(inventory.is_installed(tool));
    let unrelated = json!({"modelKey":"personal:my-model-12345678","modelID":"mine","future":true});
    write(&path, &json!([unrelated]));
    let personal = path.with_file_name("dumate-app.json");
    write(
        &personal,
        &json!({"customLLMProvider":{"model":"keep-personal"}}),
    );
    let (url, requests, task) = server(catalogue()).await;
    let manager = support::manager(root.path().join("state"), url.clone(), url);
    let connection = manager
        .connect(context(), tool, &inventory, path.clone())
        .await
        .unwrap();
    assert_eq!(connection.models, ["glm-5.3", "existing"]);
    assert_eq!(permissions.lock().unwrap().len(), 4); // before issuance and before publish
    let mut configured: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(configured.as_array().unwrap().len(), 3);
    for row in configured.as_array().unwrap().iter().skip(1) {
        assert_eq!(row["protocol"], "openai_chat_completions");
        assert!(row.get("accessStatus").is_none());
        assert!(row.get("maxOutputTokensFieldSource").is_none());
        assert_eq!(row["apiKey"], "sk-dumate-fixture");
        assert_eq!(row["modelCategory"], "LLM");
    }
    // The official checker owns these dynamic results. Reconnect must preserve them.
    configured[1]["accessStatus"] = json!("normal");
    configured[1]["connectionCheckedAt"] = json!(123);
    configured[1]["maxOutputTokensField"] = json!("max_tokens");
    configured[1]["maxOutputTokensFieldSource"] = json!("probe_accepted");
    configured.as_array_mut().unwrap().reverse();
    write(&path, &configured);
    let again = manager
        .reconnect(
            context(),
            tool,
            &connection.key_id,
            &inventory,
            path.clone(),
        )
        .await
        .unwrap();
    assert!(again.key_reused);
    let after: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(after, configured);
    assert!(requests
        .lock()
        .unwrap()
        .iter()
        .all(|r| !r.starts_with("POST /v1/chat/")));
    // Only a confirmed real probe activates the selected imported model.
    assert!(manager
        .test_connection(
            tool,
            &connection.key_id,
            ConnectionTestRequest {
                confirmed: false,
                model: Some("existing".into())
            }
        )
        .await
        .is_err());
    assert_eq!(
        std::fs::read(&path).unwrap(),
        serde_json::to_vec_pretty(&after).unwrap()
    );
    let checked = manager
        .test_connection(
            tool,
            &connection.key_id,
            ConnectionTestRequest {
                confirmed: true,
                model: Some("existing".into()),
            },
        )
        .await
        .unwrap();
    assert!(checked.passed);
    let activated: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let selected = activated
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["modelID"] == "existing")
        .unwrap();
    assert_eq!(selected["accessStatus"], "normal");
    assert_eq!(selected["maxOutputTokensField"], "max_tokens");
    assert_eq!(selected["maxOutputTokensFieldSource"], "probe_accepted");
    assert_eq!(
        requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.starts_with("POST /v1/chat/"))
            .count(),
        1
    );
    assert_eq!(
        activated
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["modelID"] == "glm-5.3")
            .unwrap()["accessStatus"],
        "normal"
    ); // previous vendor probe preserved
       // Same user/path on another enterprise profile is a different identity.
    enterprise_auth(
        root.path(),
        "enterprise-user",
        &enterprise,
        "tenant-b",
        enterprise_cookies(),
    );
    assert_eq!(
        manager
            .summaries()
            .unwrap()
            .iter()
            .find(|(id, _)| *id == tool)
            .unwrap()
            .1
            .state,
        "configuration_changed"
    );
    assert!(manager
        .test_connection(
            tool,
            &connection.key_id,
            ConnectionTestRequest {
                confirmed: true,
                model: Some("existing".into())
            }
        )
        .await
        .is_err());
    // Cleanup is permitted even after switching accounts and revocation.
    manager
        .disconnect(tool, &connection.key_id, None, DisconnectRequest::default())
        .await
        .unwrap();
    let restored: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(restored, json!([unrelated]));
    let kept: Value = serde_json::from_slice(&std::fs::read(personal).unwrap()).unwrap();
    assert_eq!(kept["customLLMProvider"]["model"], "keep-personal");
    task.abort();
    permission_task.abort();
}

#[tokio::test]
async fn enterprise_permissions_fail_closed_before_issuance_and_explicit_test() {
    use std::sync::atomic::Ordering;
    let root = tempfile::tempdir().unwrap();
    let inventory = fixture(root.path());
    let (enterprise, grants, permissions, permission_task) = permission_server().await;
    enterprise_auth(
        root.path(),
        "enterprise-user",
        &enterprise,
        "tenant-a",
        enterprise_cookies(),
    );
    let (url, requests, task) = server(catalogue()).await;
    let manager = support::manager(root.path().join("state"), url.clone(), url);
    let tool = ToolId::DuMateEnterprise;
    let path = inventory.config_path(tool);
    for flags in [0, 1, 2, 7] {
        // either missing grant or HTTP redirect
        grants.store(flags, Ordering::SeqCst);
        assert!(manager
            .connect(context(), tool, &inventory, path.clone())
            .await
            .is_err());
        assert!(!path.exists());
        assert!(requests.lock().unwrap().is_empty());
    }
    grants.store(3, Ordering::SeqCst);
    let connection = manager
        .connect(context(), tool, &inventory, path.clone())
        .await
        .unwrap();
    let before = std::fs::read(&path).unwrap();
    let request_count = requests.lock().unwrap().len();
    grants.store(0, Ordering::SeqCst);
    assert!(manager
        .test_connection(
            tool,
            &connection.key_id,
            ConnectionTestRequest {
                confirmed: true,
                model: Some("existing".into())
            }
        )
        .await
        .is_err());
    assert!(manager
        .reconnect(
            context(),
            tool,
            &connection.key_id,
            &inventory,
            path.clone()
        )
        .await
        .is_err());
    assert_eq!(requests.lock().unwrap().len(), request_count);
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(!permissions.lock().unwrap().is_empty());
    task.abort();
    permission_task.abort();
}

async fn origin_switch_fixture() -> (
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
    let mut first_catalogue = catalogue();
    first_catalogue["data"][1]["id"] = json!("glm-5.2");
    let (first_url, _, first_task) = server(first_catalogue).await;
    let mut second_catalogue = catalogue();
    second_catalogue["data"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id":"glm-5.2", "model_types":["TEXT_GENERATION"],
            "context_window_tokens":128000, "max_output_tokens":32000
        }));
    let (second_url, _, second_task) = server(second_catalogue).await;
    let directory = root.path().join("state");
    let first_manager = support::manager(directory.clone(), first_url.clone(), first_url);
    let second_manager = support::manager(directory, second_url.clone(), second_url);
    let connected = first_manager
        .connect(context(), ToolId::DuMate, &inventory, path.clone())
        .await
        .unwrap();
    assert_eq!(connected.models, ["glm-5.2"]);
    (
        root,
        inventory,
        path,
        first_manager,
        second_manager,
        connected.key_id,
        first_task,
        second_task,
    )
}

#[tokio::test]
async fn other_environment_connection_is_identified_and_cannot_be_tested() {
    let (_root, _inventory, _path, first_manager, second_manager, key_id, first_task, second_task) =
        origin_switch_fixture().await;
    assert_eq!(
        first_manager.summaries().unwrap()[0].1.state,
        "configured_untested"
    );
    assert!(
        first_manager
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
        second_manager.summaries().unwrap()[0].1.state,
        "environment_mismatch"
    );
    assert!(second_manager.summaries().unwrap()[0]
        .1
        .test_result
        .is_none());
    let error = second_manager
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
    first_task.abort();
    second_task.abort();
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
async fn reconnect_to_another_origin_selects_its_default_model() {
    let (_root, inventory, path, first_manager, second_manager, key_id, first_task, second_task) =
        origin_switch_fixture().await;
    let result = second_manager
        .reconnect(context(), ToolId::DuMate, &key_id, &inventory, path.clone())
        .await
        .unwrap();
    assert!(!result.key_reused);
    assert_eq!(result.connection.models, ["glm-5.3"]);
    let configured: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(configured["customLLMProvider"]["model"], "glm-5.3");
    assert_eq!(
        second_manager.summaries().unwrap()[0].1.state,
        "configured_untested"
    );
    assert_eq!(
        first_manager.summaries().unwrap()[0].1.state,
        "environment_mismatch"
    );
    first_task.abort();
    second_task.abort();
}

#[tokio::test]
async fn enterprise_revocation_during_catalogue_prevents_publish_and_keeps_original_preferences() {
    let root = tempfile::tempdir().unwrap();
    let inventory = fixture(root.path());
    let (enterprise, grants, _, permission_task) = permission_server().await;
    enterprise_auth(
        root.path(),
        "enterprise-user",
        &enterprise,
        "tenant-a",
        enterprise_cookies(),
    );
    let (url, requests, task) = server_with_revocation(catalogue(), Some(grants)).await;
    let manager = support::manager(root.path().join("state"), url.clone(), url);
    let tool = ToolId::DuMateEnterprise;
    let path = inventory.config_path(tool);
    let original = b"[ {\"modelKey\":\"personal:original\",\"displayName\":\"original\"} ]\n";
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, original).unwrap();
    assert!(manager
        .connect(context(), tool, &inventory, path.clone())
        .await
        .is_err());
    assert_eq!(std::fs::read(path).unwrap(), original);
    assert!(manager.summaries().unwrap().is_empty());
    let calls = requests.lock().unwrap();
    assert_eq!(calls.len(), 2); // issued + catalogue, never model inference
    assert!(calls
        .iter()
        .any(|r| r.contains("\"tool\":\"dumate-enterprise\"")));
    assert!(calls
        .iter()
        .all(|r| !r.contains("dummy-enterprise-session") && !r.contains("dummy-csrf")));
    task.abort();
    permission_task.abort();
}

#[tokio::test]
async fn enterprise_invalid_session_never_repairs_vendor_credentials_or_contacts_services() {
    let root = tempfile::tempdir().unwrap();
    let inventory = fixture(root.path());
    let (enterprise, _, permissions, permission_task) = permission_server().await;
    enterprise_auth(
        root.path(),
        "enterprise-user",
        &enterprise,
        "tenant-a",
        enterprise_cookies(),
    );
    let auth_path = data(root.path()).join("auth.json");
    let before = std::fs::read(&auth_path).unwrap();
    let (url, requests, task) = server(catalogue()).await;
    let manager = support::manager(root.path().join("state"), url.clone(), url);
    let tool = ToolId::DuMateEnterprise;
    let path = inventory.config_path(tool);
    for invalid_key in [vec![1_u8; 31], vec![1_u8; 32]] {
        std::fs::write(data(root.path()).join(".cookie-key"), &invalid_key).unwrap();
        assert!(manager
            .connect(context(), tool, &inventory, path.clone())
            .await
            .is_err());
        assert_eq!(
            std::fs::read(data(root.path()).join(".cookie-key")).unwrap(),
            invalid_key
        );
        assert_eq!(std::fs::read(&auth_path).unwrap(), before);
        assert!(!path.exists());
    }
    assert!(permissions.lock().unwrap().is_empty());
    assert!(requests.lock().unwrap().is_empty());
    task.abort();
    permission_task.abort();
}

#[tokio::test]
async fn enterprise_checks_only_the_selected_model_and_removes_a_new_file_on_disconnect() {
    for passed in [true, false] {
        let root = tempfile::tempdir().unwrap();
        let inventory = fixture(root.path());
        let (enterprise, _, _, permission_task) = permission_server().await;
        enterprise_auth(
            root.path(),
            "enterprise-user",
            &enterprise,
            "tenant-a",
            enterprise_cookies(),
        );
        let mut models = catalogue();
        if !passed {
            models["_mock_chat_response"] = json!({"choices":[]});
        }
        let (url, requests, task) = server(models).await;
        let manager = support::manager(root.path().join("state"), url.clone(), url);
        let tool = ToolId::DuMateEnterprise;
        let path = inventory.config_path(tool);
        let connection = manager
            .connect(context(), tool, &inventory, path.clone())
            .await
            .unwrap();
        let result = manager
            .test_connection(
                tool,
                &connection.key_id,
                ConnectionTestRequest {
                    confirmed: true,
                    model: Some("existing".into()),
                },
            )
            .await
            .unwrap();
        assert_eq!(result.passed, passed);
        let configured: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let row = configured
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["modelID"] == "existing")
            .unwrap();
        assert_eq!(
            row["accessStatus"],
            if passed { "normal" } else { "abnormal" }
        );
        assert_eq!(row.get("maxOutputTokensField").is_some(), passed);
        assert!(configured
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["modelID"] == "glm-5.3")
            .unwrap()
            .get("accessStatus")
            .is_none());
        assert_eq!(
            requests
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r.starts_with("POST /v1/chat/"))
                .count(),
            1
        );
        manager
            .disconnect(tool, &connection.key_id, None, DisconnectRequest::default())
            .await
            .unwrap();
        assert!(!path.exists());
        task.abort();
        permission_task.abort();
    }
}
