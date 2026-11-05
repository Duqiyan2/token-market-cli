mod support;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tokenmarket_core::{
    adapters::tool_id,
    connections::ConnectContext,
    inventory::{HostPlatform, InstallationEnvironment, ToolInventory},
};
fn executable(path: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "fixture; never execute").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}
fn environment(root: &Path, platform: HostPlatform) -> InstallationEnvironment {
    InstallationEnvironment {
        platform,
        home: root.into(),
        command_directories: vec![root.join("bin")],
        application_directories: vec![],
        executable_extensions: vec![".EXE".into(), ".CMD".into()],
    }
}
#[test]
fn pi_has_its_own_canonical_identity_path_detection_and_launch() {
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("pi").expect("Pi is a supported Tool");
    assert_eq!(serde_json::to_value(tool).unwrap(), json!("pi"));
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    let row = inventory
        .list()
        .unwrap()
        .into_iter()
        .find(|r| r.id == tool)
        .unwrap();
    assert_eq!(row.name, "Pi");
    assert!(!row.installed);
    assert_eq!(
        PathBuf::from(row.config_path),
        root.path().join(".pi/agent/models.json")
    );
    executable(&root.path().join("bin/pi"));
    inventory.require_installed(tool).unwrap();
    assert!(inventory
        .launch_plan(tool)
        .unwrap()
        .args
        .iter()
        .any(|arg| arg.contains("pi")));
}

async fn platform(
    catalogues: Vec<Value>,
    expected: usize,
) -> (String, tokio::task::JoinHandle<Vec<String>>) {
    platform_mode(catalogues, expected, "sk-pi-fixture", false).await
}
async fn platform_mode(
    catalogues: Vec<Value>,
    expected: usize,
    key: &str,
    direct: bool,
) -> (String, tokio::task::JoinHandle<Vec<String>>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let key = key.to_owned();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let mut requests = vec![];
        let mut queried = 0;
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = vec![];
            loop {
                let mut chunk = [0; 4096];
                let n = socket.read(&mut chunk).await.unwrap();
                assert_ne!(n, 0);
                bytes.extend_from_slice(&chunk[..n]);
                if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&bytes[..end]);
                    let length = header
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
                assert!(request.contains(&format!("Bearer {key}")));
                let response = catalogues[queried].clone();
                queried += 1;
                response
            } else if request.starts_with("POST /v1/chat/completions ") {
                assert!(request.contains(&format!("Bearer {key}")));
                json!({"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"OK"}}]})
            } else if direct {
                assert!(request.starts_with("POST /api/client/keys/inspect "));
                json!({"success":true,"data":{"naming_version":1,"credential_matches":true,"status":"VALID","key_id":"9020","owner":{"id":"u1","name":"Ada"},"bound_workspace":{"id":"42","name":"Research"}}})
            } else {
                assert!(request.contains("Bearer desktop-secret"));
                json!({"success":true,"data":{"api_key":key,"key_id":"9018","owner":{"id":"u1","name":"Ada"},"bound_workspace":{"id":"42","name":"Research"}}})
            };
            let body = response.to_string();
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            requests.push(request);
            if requests.len() == expected {
                break;
            }
        }
        requests
    });
    (url, task)
}

fn catalogue() -> Value {
    json!({"data":[
        {"id":"vendor/zeta~1","display_name":"Zeta","model_types":["TEXT_GENERATION"],"context_window_tokens":65536,"max_output_tokens":4096},
        {"id":"alpha.model","model_types":["TEXT_GENERATION"]},
        {"id":"embedding","model_types":["EMBEDDING"]}
    ]})
}

fn context() -> ConnectContext {
    ConnectContext {
        session_token: "desktop-secret".into(),
        owner_id: "u1".into(),
        workspace_id: "42".into(),
    }
}

#[tokio::test]
async fn pi_imports_the_full_catalogue_without_changing_preferences_or_unrelated_credentials() {
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("pi").unwrap();
    executable(&root.path().join("bin/pi"));
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    let path = inventory.config_path(tool);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let settings = br#"{"defaultProvider":"manual","defaultModel":"local"}"#;
    let auth=br#"{"anthropic":{"type":"oauth","access":"private","refresh":"other","expires":9999999999999}}"#;
    std::fs::write(path.with_file_name("settings.json"), settings).unwrap();
    std::fs::write(path.with_file_name("auth.json"), auth).unwrap();
    std::fs::write(&path,json!({"custom":"keep","providers":{"tokenmarket":{"unknown":"keep","models":[{"id":"manual","name":"Local","extra":true},{"id":"alpha.model","name":"old","extra":"preserve","api":"anthropic-messages","baseUrl":"https://manual.invalid"}]}}}).to_string()).unwrap();
    let (url, server) = platform(vec![catalogue()], 2).await;
    let manager = support::manager(root.path().join("state"), url.clone(), url.clone());
    let connection = manager
        .connect(context(), tool, &inventory, path.clone())
        .await
        .unwrap();
    assert_eq!(connection.models, ["vendor/zeta~1", "alpha.model"]);
    assert_eq!(connection.protocol, "OPENAI_CHAT_COMPLETIONS");
    let value: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let provider = &value["providers"]["tokenmarket"];
    assert_eq!(provider["baseUrl"], format!("{url}/v1"));
    assert_eq!(provider["api"], "openai-completions");
    assert_eq!(provider["apiKey"], "sk-pi-fixture");
    assert_eq!(provider["authHeader"], true);
    let rows = provider["models"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["name"], "Local");
    assert_eq!(rows[1]["extra"], "preserve");
    assert_eq!(rows[1]["api"], "openai-completions");
    assert_eq!(rows[1]["baseUrl"], format!("{url}/v1"));
    let model = rows.iter().find(|r| r["id"] == "vendor/zeta~1").unwrap();
    assert_eq!(model["contextWindow"], 65536);
    assert_eq!(model["maxTokens"], 4096);
    assert_eq!(
        std::fs::read(path.with_file_name("settings.json")).unwrap(),
        settings
    );
    assert_eq!(
        std::fs::read(path.with_file_name("auth.json")).unwrap(),
        auth
    );
    assert!(server
        .await
        .unwrap()
        .iter()
        .all(|r| !r.starts_with("POST /v1/")));
}

#[tokio::test]
async fn pi_reconnect_and_disconnect_restore_owned_fields_after_model_reordering() {
    use tokenmarket_core::connections::{ConnectionTestRequest, DisconnectRequest};
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("pi").unwrap();
    executable(&root.path().join("bin/pi"));
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    let path = inventory.config_path(tool);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let original = json!({"providers":{"tokenmarket":{"models":[{"id":"alpha.model","name":"manual name","custom":"original"},{"id":"user-model","name":"manual"}]}},"other":{"keep":true}});
    std::fs::write(&path, original.to_string()).unwrap();
    let next = json!({"data":[{"id":"alpha.model","model_types":["TEXT_GENERATION"]}]});
    let (url, server) = platform(vec![catalogue(), next], 4).await;
    let manager = support::manager(root.path().join("state"), url.clone(), url);
    let connected = manager
        .connect(context(), tool, &inventory, path.clone())
        .await
        .unwrap();
    let mut current: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let rows = current["providers"]["tokenmarket"]["models"]
        .as_array_mut()
        .unwrap();
    rows.reverse();
    rows.iter_mut()
        .find(|r| r["id"] == "vendor/zeta~1")
        .unwrap()["userAdded"] = json!("keep");
    std::fs::write(&path, current.to_string()).unwrap();
    let reconnected = manager
        .reconnect(context(), tool, &connected.key_id, &inventory, path.clone())
        .await
        .unwrap();
    assert!(reconnected.key_reused);
    assert_eq!(reconnected.connection.models, ["alpha.model"]);
    let before = std::fs::read(&path).unwrap();
    let tested = manager
        .test_connection(
            tool,
            &connected.key_id,
            ConnectionTestRequest {
                confirmed: true,
                model: Some("alpha.model".into()),
            },
        )
        .await
        .unwrap();
    assert!(tested.passed);
    assert!(!tested.streaming_verified && !tested.tool_call_verified);
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(
        manager
            .disconnect(tool, &connected.key_id, None, DisconnectRequest::default())
            .await
            .unwrap()
            .state,
        "disconnected"
    );
    let restored: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let rows = restored["providers"]["tokenmarket"]["models"]
        .as_array()
        .unwrap();
    assert_eq!(
        rows.iter().find(|r| r["id"] == "alpha.model").unwrap(),
        &original["providers"]["tokenmarket"]["models"][0]
    );
    assert_eq!(
        rows.iter().find(|r| r["id"] == "vendor/zeta~1").unwrap(),
        &json!({"id":"vendor/zeta~1","userAdded":"keep"})
    );
    assert!(restored["providers"]["tokenmarket"].get("apiKey").is_none());
    assert_eq!(restored["other"]["keep"], true);
    let requests = server.await.unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.starts_with("POST /api/desktop/connections/keys "))
            .count(),
        1
    );
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.starts_with("POST /v1/chat/completions "))
            .count(),
        1
    );
}

#[tokio::test]
async fn pi_auth_conflicts_and_invalid_models_leave_every_file_and_connection_untouched() {
    for auth in [
        json!({"tokenmarket":{"type":"api_key","key":"other"}}),
        json!({"other":{"type":"api_key","key":17}}),
        json!({"tokenmarket":{"type":"oauth","access":"private","refresh":"other","expires":9999999999999i64}}),
    ] {
        let root = tempfile::tempdir().unwrap();
        let tool = tool_id("pi").unwrap();
        executable(&root.path().join("bin/pi"));
        let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
        let path = inventory.config_path(tool);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let original = b"{\"providers\":{}}";
        let auth = auth.to_string();
        std::fs::write(&path, original).unwrap();
        std::fs::write(path.with_file_name("auth.json"), &auth).unwrap();
        let manager = support::manager(
            root.path().join("state"),
            "http://127.0.0.1:1".into(),
            "http://127.0.0.1:1".into(),
        );
        assert!(manager
            .connect(context(), tool, &inventory, path.clone())
            .await
            .unwrap_err()
            .to_string()
            .contains("auth.json"));
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert_eq!(
            std::fs::read_to_string(path.with_file_name("auth.json")).unwrap(),
            auth
        );
        assert!(manager.list().unwrap().is_empty());
    }
    for (config, expected) in [
        (
            json!({"providers":{"tokenmarket":{"oauth":"radius"}}}),
            "OAuth",
        ),
        (
            json!({"providers":{"tokenmarket":{"headers":{"Authorization":"manual"}}}}),
            "认证",
        ),
        (
            json!({"providers":{"tokenmarket":{"models":[{"id":"duplicate"},{"id":"duplicate"}]}}}),
            "重复",
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        let tool = tool_id("pi").unwrap();
        executable(&root.path().join("bin/pi"));
        let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
        let path = inventory.config_path(tool);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let original = config.to_string();
        std::fs::write(&path, &original).unwrap();
        let manager = support::manager(
            root.path().join("state"),
            "http://127.0.0.1:1".into(),
            "http://127.0.0.1:1".into(),
        );
        let error = manager
            .connect(context(), tool, &inventory, path.clone())
            .await
            .unwrap_err();
        assert!(error.to_string().contains(expected), "{error}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        assert!(manager.list().unwrap().is_empty());
    }
}

struct Installer {
    executable: PathBuf,
    steps: std::sync::Mutex<Vec<tokenmarket_core::installation::InstallStep>>,
}
impl tokenmarket_core::installation::InstallExecutor for Installer {
    fn execute(
        &self,
        step: &tokenmarket_core::installation::InstallStep,
        _: &std::sync::atomic::AtomicBool,
        _: &mut dyn FnMut(String),
    ) -> anyhow::Result<()> {
        self.steps.lock().unwrap().push(step.clone());
        if !step.program.is_empty() {
            executable(&self.executable);
        }
        Ok(())
    }
    fn version(&self, _: &Path) -> anyhow::Result<String> {
        Ok("pi 1.4.0".into())
    }
}
#[test]
fn pi_installs_verified_official_standalone_archives_and_redetects_without_connection() {
    use std::sync::Arc;
    use tokenmarket_core::installation::{InstallStage, InstallationManager};
    for (platform, architecture, asset, sha, command) in [
        (
            HostPlatform::MacOs,
            "aarch64",
            "pi-darwin-arm64.tar.gz",
            "79d9979d76a2a5adb7c28e27e9b44629928ccd0dc9fbb6ae7217860bd5e8be5d",
            "pi",
        ),
        (
            HostPlatform::Unix,
            "x86_64",
            "pi-linux-x64.tar.gz",
            "f2e4e4329603223d03260cdb8bd5033dbdc1f73ba58ab8939db2e56546e115d2",
            "pi",
        ),
        (
            HostPlatform::Windows,
            "aarch64",
            "pi-windows-arm64.zip",
            "17ab17ee0e9088545c7dfa56d29490c45bd915425610b0db0cd937632f2df714",
            "pi.exe",
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        let tool = tool_id("pi").unwrap();
        let installer = Arc::new(Installer {
            executable: root
                .path()
                .join(".tokenmarket/tools/pi/runtime")
                .join(command),
            steps: Default::default(),
        });
        let manager = InstallationManager::new(
            environment(root.path(), platform),
            architecture.into(),
            installer.clone(),
        );
        assert!(manager.availability(tool).supported);
        assert_eq!(manager.availability(tool).version.as_deref(), Some("1.4.0"));
        manager.start(tool).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            let state = manager.status(tool).unwrap();
            if state.stage.is_terminal() {
                assert_eq!(state.stage, InstallStage::Succeeded);
                assert!(state.installed);
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let steps = installer.steps.lock().unwrap();
        assert!(steps
            .iter()
            .any(|s| s.download.as_ref().is_some_and(|(url, _)| url
                == &format!(
                    "https://github.com/earendil-works/pi/releases/download/v1.4.0/{asset}"
                ))));
        assert!(steps.iter().any(|s| s.args.join(" ").contains(sha)));
        assert!(!steps.iter().any(|s| s
            .args
            .iter()
            .any(|a| a.contains("npm install") || a.contains("pi.dev/install"))));
        assert!(!root.path().join(".pi").exists());
        if platform == HostPlatform::Windows {
            assert!(steps
                .iter()
                .any(|s| s.download.as_ref().is_some_and(|(url, _)| url.contains(
                    "git-for-windows/git/releases/download/v2.56.0.windows.4/PortableGit"
                ))));
        }
    }
}

#[test]
fn pi_agent_directory_override_is_shared_by_inventory_and_adapter() {
    const MARKER: &str = "TOKENMARKET_PI_OVERRIDE_CHILD";
    if std::env::var_os(MARKER).is_some() {
        let path =
            PathBuf::from(std::env::var_os("PI_CODING_AGENT_DIR").unwrap()).join("models.json");
        let inventory = ToolInventory::new(environment(
            Path::new("unused-fixture"),
            HostPlatform::MacOs,
        ));
        let tool = tool_id("pi").unwrap();
        assert_eq!(inventory.config_path(tool), path);
        assert_eq!(
            tokenmarket_core::adapters::get_adapter(tool).config_path(),
            path
        );
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "pi_agent_directory_override_is_shared_by_inventory_and_adapter",
            "--nocapture",
        ])
        .env(MARKER, "1")
        .env("PI_CODING_AGENT_DIR", root.path().join("custom agent"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[tokio::test]
async fn pi_catalogue_failure_preserves_existing_models_and_saved_preferences() {
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("pi").unwrap();
    executable(&root.path().join("bin/pi"));
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    let path = inventory.config_path(tool);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let original = b"{\"providers\":{\"manual\":{\"models\":[{\"id\":\"local\"}]}}}";
    std::fs::write(&path, original).unwrap();
    let (url, server) = platform(
        vec![json!({"data":[{"id":"embedding","model_types":["EMBEDDING"]}]})],
        2,
    )
    .await;
    let manager = support::manager(root.path().join("state"), url.clone(), url);
    assert!(manager
        .connect(context(), tool, &inventory, path.clone())
        .await
        .unwrap_err()
        .to_string()
        .contains("文本生成"));
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert!(manager.list().unwrap().is_empty());
    server.await.unwrap();
}

#[tokio::test]
async fn pi_detects_authentication_added_after_connect_before_offering_a_test() {
    use tokenmarket_core::connections::{ConnectionTestRequest, DisconnectRequest};
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("pi").unwrap();
    executable(&root.path().join("bin/pi"));
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    let path = inventory.config_path(tool);
    let (url, server) = platform(vec![catalogue()], 2).await;
    let manager = support::manager(root.path().join("state"), url.clone(), url);
    let connected = manager
        .connect(context(), tool, &inventory, path.clone())
        .await
        .unwrap();
    server.await.unwrap();
    let auth = b"{\"tokenmarket\":{\"type\":\"api_key\",\"key\":\"manual\"}}";
    std::fs::write(path.with_file_name("auth.json"), auth).unwrap();
    assert_eq!(
        manager.summaries().unwrap()[0].1.state,
        "configuration_changed"
    );
    assert!(manager
        .test_connection(
            tool,
            &connected.key_id,
            ConnectionTestRequest {
                confirmed: true,
                model: Some("alpha.model".into())
            }
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("auth.json"));
    assert_eq!(
        manager
            .disconnect(tool, &connected.key_id, None, DisconnectRequest::default())
            .await
            .unwrap()
            .state,
        "disconnected"
    );
    assert_eq!(
        std::fs::read(path.with_file_name("auth.json")).unwrap(),
        auth
    );
}

#[tokio::test]
async fn pi_recovers_the_exact_literal_credential_from_models_without_executing_key_syntax() {
    use tokenmarket_core::connections::ConnectionManager;
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("pi").unwrap();
    executable(&root.path().join("bin/pi"));
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    let path = inventory.config_path(tool);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let secret = "!never-execute${KEY}";
    let encoded = "$!never-execute$${KEY}";
    let (url, server) = platform_mode(vec![catalogue()], 3, secret, true).await;
    std::fs::write(&path,json!({"providers":{"tokenmarket":{"baseUrl":format!("{url}/v1"),"apiKey":encoded,"models":[]}}}).to_string()).unwrap();
    let manager = ConnectionManager::with_credentials(
        root.path().join("state"),
        url.clone(),
        url,
        support::credentials(&root.path().join("state")),
    );
    let connected = manager
        .connect(context(), tool, &inventory, path.clone())
        .await
        .unwrap();
    assert_eq!(connected.key_id, "9020");
    let value: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(value["providers"]["tokenmarket"]["apiKey"], encoded);
    let requests = server.await.unwrap();
    use sha2::{Digest, Sha256};
    let expected = format!("{:x}", Sha256::digest(secret.as_bytes()));
    for request in requests.iter().filter(|r| r.starts_with("POST")) {
        assert!(request.contains(&expected));
        assert!(!request.contains(secret));
    }
}
