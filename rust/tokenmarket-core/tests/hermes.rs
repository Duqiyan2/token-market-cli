mod support;
use serde_json::{json, Value};
use std::path::Path;
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
fn context() -> ConnectContext {
    ConnectContext {
        session_token: "desktop-secret".into(),
        owner_id: "u1".into(),
        workspace_id: "42".into(),
    }
}
fn read(path: &Path) -> Value {
    serde_yaml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}
async fn platform(
    catalogues: Vec<Value>,
    expected: usize,
) -> (String, tokio::task::JoinHandle<Vec<String>>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let mut requests = vec![];
        let mut catalogue = 0;
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = vec![];
            loop {
                let mut chunk = [0; 4096];
                let n = socket.read(&mut chunk).await.unwrap();
                assert_ne!(n, 0);
                bytes.extend_from_slice(&chunk[..n]);
                if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                    let h = String::from_utf8_lossy(&bytes[..end]);
                    let len: usize = h
                        .lines()
                        .find_map(|l| {
                            l.to_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse().unwrap())
                        })
                        .unwrap_or(0);
                    if bytes.len() >= end + 4 + len {
                        break;
                    }
                }
            }
            let request = String::from_utf8(bytes).unwrap();
            let response = if request.starts_with("GET /v1/models ") {
                assert!(request.contains("Bearer sk-hermes-fixture"));
                let value = catalogues[catalogue].clone();
                catalogue += 1;
                value
            } else if request.starts_with("POST /v1/chat/completions ") {
                assert!(request.contains("Bearer sk-hermes-fixture"));
                json!({"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"OK"}}]})
            } else {
                assert!(request.contains("Bearer desktop-secret"));
                json!({"success":true,"data":{"api_key":"sk-hermes-fixture","key_id":"9019","owner":{"id":"u1","name":"Ada"},"bound_workspace":{"id":"42","name":"Research"}}})
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
    json!({"data":[{"id":"vendor/zeta~1","display_name":"Zeta","model_types":["TEXT_GENERATION"],"context_window_tokens":65536},{"id":"alpha.model","model_types":["TEXT_GENERATION"]},{"id":"embedding","model_types":["EMBEDDING"]}]})
}

#[test]
fn hermes_is_a_distinct_tool_with_platform_correct_data_and_command_detection() {
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("hermes").expect("Hermes is supported");
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    let entry = inventory
        .list()
        .unwrap()
        .into_iter()
        .find(|entry| entry.id == tool)
        .unwrap();
    assert_eq!(entry.name, "Hermes");
    assert!(!entry.installed);
    let expected = std::env::var_os("HERMES_HOME")
        .filter(|v| !v.to_string_lossy().trim().is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| root.path().join(".hermes"));
    assert_eq!(
        std::path::PathBuf::from(entry.config_path),
        expected.join("config.yaml")
    );
    executable(&root.path().join("bin/hermes"));
    inventory.require_installed(tool).unwrap();
    assert!(inventory
        .launch_plan(tool)
        .unwrap()
        .args
        .iter()
        .any(|arg| arg.contains("hermes")));
}

#[test]
fn hermes_windows_inventory_uses_local_app_data_and_the_native_cli_launcher() {
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("hermes").unwrap();
    let data = std::env::var_os("HERMES_HOME")
        .filter(|value| !value.to_string_lossy().trim().is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("LOCALAPPDATA")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| root.path().join("AppData/Local"))
                .join("hermes")
        });
    let mut env = environment(root.path(), HostPlatform::Windows);
    env.command_directories.push(data.join("bin"));
    let inventory = ToolInventory::new(env);
    assert_eq!(inventory.config_path(tool), data.join("config.yaml"));
    executable(&data.join("bin/hermes.cmd"));
    inventory.require_installed(tool).unwrap();
    let plan = inventory.launch_plan(tool).unwrap();
    assert!(plan.args.iter().any(|argument| argument == "hermes"));
}

#[tokio::test]
async fn hermes_connect_imports_model_leaves_without_rewriting_user_defaults() {
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("hermes").unwrap();
    executable(&root.path().join("bin/hermes"));
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    let path = root.path().join("config.yaml");
    std::fs::write(&path,"model:\n  provider: custom:manual\n  default: manual/model\nproviders:\n  manual:\n    api: https://manual.invalid/v1\n    api_key: manual-key\n").unwrap();
    let (url, server) = platform(vec![catalogue()], 2).await;
    let manager = support::manager(root.path().join("state"), url.clone(), url.clone());
    let connected = manager
        .connect(context(), tool, &inventory, path.clone())
        .await
        .unwrap();
    assert_eq!(connected.models, ["vendor/zeta~1", "alpha.model"]);
    assert_eq!(connected.protocol, "OPENAI_CHAT_COMPLETIONS");
    let value = read(&path);
    assert_eq!(value["model"]["provider"], "custom:manual");
    assert_eq!(value["model"]["default"], "manual/model");
    assert_eq!(value["providers"]["manual"]["api_key"], "manual-key");
    let p = &value["providers"]["tokenmarket"];
    assert_eq!(p["api"], format!("{url}/v1"));
    assert_eq!(p["api_key"], "sk-hermes-fixture");
    assert_eq!(p["transport"], "chat_completions");
    assert_eq!(p["models"]["vendor/zeta~1"]["context_length"], 65536);
    assert!(p["models"].get("alpha.model").is_some());
    assert!(p.get("default_model").is_none());
    assert!(server
        .await
        .unwrap()
        .iter()
        .all(|request| !request.starts_with("POST /v1/")));
}

#[tokio::test]
async fn hermes_reconnect_and_disconnect_preserve_user_models_and_restore_only_owned_leaves() {
    use tokenmarket_core::connections::{ConnectionTestRequest, DisconnectRequest};
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("hermes").unwrap();
    executable(&root.path().join("bin/hermes"));
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    let path = root.path().join("config.yaml");
    let original = json!({"model":{"provider":"custom:manual","default":"manual/model"},"providers":{"tokenmarket":{"api":"https://manual.invalid/v1","api_key":"original-secret","default_model":"manual-model","apiMode":"responses","models":{"vendor/zeta~1":{"context_length":2048,"manual":"original"}}}}});
    std::fs::write(&path, serde_yaml::to_string(&original).unwrap()).unwrap();
    let next = json!({"data":[{"id":"alpha.model","model_types":["TEXT_GENERATION"]}]});
    let (url, server) = platform(vec![catalogue(), next], 4).await;
    let manager = support::manager(root.path().join("state"), url.clone(), url);
    let connection = manager
        .connect(context(), tool, &inventory, path.clone())
        .await
        .unwrap();
    assert_eq!(
        read(&path)["providers"]["tokenmarket"]["apiMode"],
        "chat_completions"
    );
    // A legacy pool seeded by Hermes itself must remain reconnectable.
    std::fs::write(root.path().join("auth.json"), json!({"credential_pool":{"custom:token-market":[{"source":"config:Token Market","access_token":"sk-hermes-fixture"}]}}).to_string()).unwrap();
    let mut edited = read(&path);
    edited["providers"]["tokenmarket"]["models"]["alpha.model"]["manual"] =
        json!("keep new row field");
    edited["providers"]["tokenmarket"]["models"]["manual-added"] = json!({"context_length":12345});
    edited["user_extra"] = json!(true);
    std::fs::write(&path, serde_yaml::to_string(&edited).unwrap()).unwrap();
    let reconnected = manager
        .reconnect(
            context(),
            tool,
            &connection.key_id,
            &inventory,
            path.clone(),
        )
        .await
        .unwrap();
    assert!(reconnected.key_reused);
    let current = read(&path);
    assert_eq!(
        current["providers"]["tokenmarket"]["models"]["vendor/zeta~1"]["context_length"],
        2048
    );
    assert_eq!(
        current["providers"]["tokenmarket"]["models"]["alpha.model"]["manual"],
        "keep new row field"
    );
    assert_eq!(
        current["providers"]["tokenmarket"]["default_model"],
        "manual-model"
    );
    let before = std::fs::read(&path).unwrap();
    let tested = manager
        .test_connection(
            tool,
            &connection.key_id,
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
            .disconnect(tool, &connection.key_id, None, DisconnectRequest::default())
            .await
            .unwrap()
            .state,
        "disconnected"
    );
    let value = read(&path);
    assert_eq!(
        value["providers"]["tokenmarket"]["api_key"],
        "original-secret"
    );
    assert_eq!(
        value["providers"]["tokenmarket"]["models"]["vendor/zeta~1"],
        original["providers"]["tokenmarket"]["models"]["vendor/zeta~1"]
    );
    assert_eq!(
        value["providers"]["tokenmarket"]["models"]["alpha.model"],
        json!({"manual":"keep new row field"})
    );
    assert_eq!(
        value["providers"]["tokenmarket"]["models"]["manual-added"]["context_length"],
        12345
    );
    assert_eq!(value["model"], original["model"]);
    assert_eq!(value["user_extra"], true);
    let requests = server.await.unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.starts_with("POST /api/desktop/connections/keys "))
            .count(),
        1
    );
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.starts_with("POST /v1/chat/completions "))
            .count(),
        1
    );
}

#[tokio::test]
async fn hermes_refuses_competing_credentials_or_a_disabled_provider_before_issuance() {
    for provider in [
        json!({"key_cmd":"must-not-run"}),
        json!({"key_env":"CORP_SECRET"}),
        json!({"api_key_env":"CORP_SECRET"}),
        json!({"keyEnv":"CORP_SECRET"}),
        json!({"extra_headers":{"Authorization":"Bearer manual"}}),
        json!({"enabled":false}),
        json!({"enabled":0}),
        json!({"enabled":null}),
    ] {
        let root = tempfile::tempdir().unwrap();
        let tool = tool_id("hermes").unwrap();
        executable(&root.path().join("bin/hermes"));
        let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
        let path = root.path().join("config.yaml");
        let original =
            serde_yaml::to_string(&json!({"providers":{"tokenmarket":provider}})).unwrap();
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
        assert!(error.to_string().contains("Hermes"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        assert!(manager.list().unwrap().is_empty());
        assert!(!root.path().join("state/keys.json").exists());
    }
}

#[tokio::test]
async fn hermes_refuses_saved_competing_auth_pools_without_mutating_auth_or_issuing_a_key() {
    for (profile, store) in [
        (false, json!({"credential_pool":{"tokenmarket":[{"source":"manual:one","access_token":"manual-secret"}]}}).to_string()),
        (false, json!({"credential_pool":{"custom:token-market":[{"source":"manual:one","access_token":"manual-secret"}]}}).to_string()),
        (true, json!({"credential_pool":{"tokenmarket":[{"source":"manual:one","access_token":"manual-secret"}]}}).to_string()),
        (false, "invalid json".into()),
    ] {
        let root = tempfile::tempdir().unwrap();
        let tool = tool_id("hermes").unwrap();
        executable(&root.path().join("bin/hermes"));
        let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
        let path = if profile {root.path().join("profiles/work/config.yaml")} else {root.path().join("config.yaml")};
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "model: {default: manual/model}\n").unwrap();
        let auth = root.path().join("auth.json");
        std::fs::write(&auth, &store).unwrap();
        let manager = support::manager(root.path().join("state"), "http://127.0.0.1:1".into(), "http://127.0.0.1:1".into());
        let error = manager.connect(context(), tool, &inventory, path.clone()).await.unwrap_err();
        assert!(error.to_string().contains("Hermes"), "{error:#}");
        assert_eq!(std::fs::read_to_string(&auth).unwrap(), store);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "model: {default: manual/model}\n");
        assert!(manager.list().unwrap().is_empty());
    }
}

#[tokio::test]
async fn hermes_empty_compatible_catalogue_preserves_existing_configuration() {
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("hermes").unwrap();
    executable(&root.path().join("bin/hermes"));
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    let path = root.path().join("config.yaml");
    let original = "model:\n  default: manual/model\n";
    std::fs::write(&path, original).unwrap();
    let (url, server) = platform(
        vec![json!({"data":[{"id":"embed","model_types":["EMBEDDING"]}]})],
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
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    assert!(manager.list().unwrap().is_empty());
    server.await.unwrap();
}

struct Installer {
    executable: std::path::PathBuf,
    steps: std::sync::Mutex<Vec<tokenmarket_core::installation::InstallStep>>,
}
impl tokenmarket_core::installation::InstallExecutor for Installer {
    fn execute(
        &self,
        step: &tokenmarket_core::installation::InstallStep,
        _: &std::sync::atomic::AtomicBool,
        output: &mut dyn FnMut(String),
    ) -> anyhow::Result<()> {
        self.steps.lock().unwrap().push(step.clone());
        if !step.program.is_empty() {
            executable(&self.executable);
        }
        output("Official Hermes dependency fixture".into());
        Ok(())
    }
    fn version(&self, _: &Path) -> anyhow::Result<String> {
        Ok("Hermes Agent v0.22.5".into())
    }
}
#[test]
fn hermes_install_uses_the_official_current_script_and_skips_configuration_maintenance() {
    use std::{
        sync::Arc,
        time::{Duration, Instant},
    };
    use tokenmarket_core::installation::{InstallStage, InstallationManager};
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("hermes").unwrap();
    let executor = Arc::new(Installer {
        executable: root
            .path()
            .join(".tokenmarket/tools/hermes/checkout/.hermes/bin/hermes"),
        steps: std::sync::Mutex::new(vec![]),
    });
    let manager = InstallationManager::new(
        environment(root.path(), HostPlatform::MacOs),
        "aarch64".into(),
        executor.clone(),
    );
    assert!(manager.availability(tool).supported);
    manager.start(tool).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let status = manager.status(tool).unwrap();
        if status.stage.is_terminal() {
            assert_eq!(status.stage, InstallStage::Succeeded);
            assert!(status.installed);
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    let steps = executor.steps.lock().unwrap();
    let execution = steps.last().unwrap().args.join(" ");
    assert!(execution.contains(
        "https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.sh"
    ));
    assert!(!execution.contains("--commit"));
    assert!(!execution.contains("stage_config"));
    assert!(!execution.contains("stage_products"));
    assert!(!execution.contains("_run_post_update_maintenance"));
    assert!(!root.path().join(".hermes/config.yaml").exists());
    assert!(!root
        .path()
        .join(".tokenmarket/desktop-connections")
        .exists());
}
