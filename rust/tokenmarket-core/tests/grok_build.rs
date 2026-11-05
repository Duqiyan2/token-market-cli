mod support;

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{atomic::AtomicBool, Arc};
use tokenmarket_core::installation::{
    InstallExecutor, InstallStage, InstallStep, InstallationManager,
};
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

async fn platform(
    catalogues: Vec<Value>,
    expected: usize,
) -> (String, tokio::task::JoinHandle<Vec<String>>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
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
                assert!(request.contains("Bearer sk-grok-fixture"));
                let response = catalogues[queried].clone();
                queried += 1;
                response
            } else if request.starts_with("POST /v1/chat/completions ") {
                assert!(request.contains("Bearer sk-grok-fixture"));
                json!({"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"OK"}}]})
            } else {
                assert!(request.contains("Bearer desktop-secret"));
                json!({"success":true,"data":{"api_key":"sk-grok-fixture","key_id":"9018","owner":{"id":"u1","name":"Ada"},"bound_workspace":{"id":"42","name":"Research"}}})
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

fn read_toml(path: &Path) -> Value {
    toml_edit::de::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[tokio::test]
async fn connecting_grok_imports_all_models_without_changing_the_saved_choice_or_calling_a_model() {
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("grok-build").unwrap();
    executable(&root.path().join("bin/grok"));
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    let path = inventory.config_path(tool);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "# User preference\n[models]\ndefault = \"manual\"\n[model.manual]\nmodel = \"manual-model\"\nname = \"Keep me\"\n").unwrap();
    let (url, server) = platform(vec![catalogue()], 2).await;
    let manager = support::manager(root.path().join("state"), url.clone(), url.clone());
    let connection = manager
        .connect(context(), tool, &inventory, path.clone())
        .await
        .unwrap();
    assert_eq!(connection.models, ["vendor/zeta~1", "alpha.model"]);
    assert_eq!(connection.protocol, "OPENAI_CHAT_COMPLETIONS");
    let value = read_toml(&path);
    assert_eq!(value["models"]["default"], "manual");
    assert_eq!(value["model"]["manual"]["name"], "Keep me");
    let model = &value["model"]["tokenmarket/vendor/zeta~1"];
    assert_eq!(model["base_url"], format!("{url}/v1"));
    assert_eq!(model["api_key"], "sk-grok-fixture");
    assert_eq!(model["api_backend"], "chat_completions");
    assert_eq!(model["model"], "vendor/zeta~1");
    assert_eq!(model["context_window"], 65536);
    assert_eq!(model["max_completion_tokens"], 4096);
    assert!(std::fs::read_to_string(&path)
        .unwrap()
        .contains("# User preference"));
    let requests = server.await.unwrap();
    assert!(requests.iter().all(|r| !r.starts_with("POST /v1/")));
}

#[tokio::test]
async fn grok_reconnect_reuses_the_key_and_disconnect_preserves_user_edits() {
    use tokenmarket_core::connections::{ConnectionTestRequest, DisconnectRequest};
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("grok-build").unwrap();
    executable(&root.path().join("bin/grok"));
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    let path = inventory.config_path(tool);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "# User preference\n[models]\ndefault = \"manual\"\n[model.manual]\nmodel = \"manual-model\"\n").unwrap();
    let next = json!({"data":[{"id":"alpha.model","model_types":["TEXT_GENERATION"]}]});
    let (url, server) = platform(vec![catalogue(), next], 4).await;
    let manager = support::manager(root.path().join("state"), url.clone(), url.clone());
    let connected = manager
        .connect(context(), tool, &inventory, path.clone())
        .await
        .unwrap();
    let mut text = std::fs::read_to_string(&path).unwrap();
    text.push_str("\n[user_extra]\nnote = \"keep later edit\"\n");
    std::fs::write(&path, text).unwrap();
    let reconnected = manager
        .reconnect(context(), tool, &connected.key_id, &inventory, path.clone())
        .await
        .unwrap();
    assert!(reconnected.key_reused);
    assert_eq!(reconnected.connection.models, ["alpha.model"]);
    assert!(read_toml(&path)["model"]
        .get("tokenmarket/vendor/zeta~1")
        .is_none());
    let before_test = std::fs::read(&path).unwrap();
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
    assert_eq!(std::fs::read(&path).unwrap(), before_test);
    let disconnected = manager
        .disconnect(tool, &connected.key_id, None, DisconnectRequest::default())
        .await
        .unwrap();
    assert_eq!(disconnected.state, "disconnected");
    let restored = read_toml(&path);
    assert_eq!(restored["models"]["default"], "manual");
    assert_eq!(restored["user_extra"]["note"], "keep later edit");
    assert!(restored["model"].get("tokenmarket/alpha.model").is_none());
    assert!(std::fs::read_to_string(&path)
        .unwrap()
        .contains("# User preference"));
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
async fn grok_refuses_conflicting_authorization_before_issuing_a_key() {
    for original in [
        "[model.\"tokenmarket/alpha\"]\nmodel = \"alpha\"\n[model.\"tokenmarket/alpha\".extra_headers]\nAuthorization = \"Bearer manual\"\n",
        "[models.extra_headers]\nAuthorization = \"Bearer manual\"\n",
    ] {
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("grok-build").unwrap();
    executable(&root.path().join("bin/grok"));
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    let path = inventory.config_path(tool);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, original).unwrap();
    let manager = support::manager(
        root.path().join("state"),
        "http://127.0.0.1:1".into(),
        "http://127.0.0.1:1".into(),
    );
    let error = manager
        .connect(context(), tool, &inventory, path.clone())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("Authorization"));
    assert_eq!(std::fs::read_to_string(path).unwrap(), original);
    assert!(manager.list().unwrap().is_empty());
    }
}

#[tokio::test]
async fn grok_catalogue_failure_preserves_an_existing_configuration() {
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("grok-build").unwrap();
    executable(&root.path().join("bin/grok"));
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    let path = inventory.config_path(tool);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let original = "[models]\ndefault = \"manual\"\n";
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
    assert_eq!(std::fs::read_to_string(path).unwrap(), original);
    assert!(manager.list().unwrap().is_empty());
    server.await.unwrap();
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
fn grok_build_is_listed_and_detected_without_running_it() {
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("grok-build").expect("Grok Build is a supported Tool");
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    let before = inventory
        .list()
        .unwrap()
        .into_iter()
        .find(|e| e.id == tool)
        .unwrap();
    assert_eq!(before.name, "Grok Build");
    assert!(!before.installed);
    assert_eq!(
        PathBuf::from(before.config_path),
        root.path().join(".grok/config.toml")
    );
    executable(&root.path().join("bin/grok"));
    inventory.require_installed(tool).unwrap();
    assert!(inventory
        .launch_plan(tool)
        .unwrap()
        .args
        .iter()
        .any(|arg| arg.contains("grok")));
}

struct Installer {
    executable: PathBuf,
}
impl InstallExecutor for Installer {
    fn execute(
        &self,
        step: &InstallStep,
        _: &AtomicBool,
        output: &mut dyn FnMut(String),
    ) -> anyhow::Result<()> {
        if !step.program.is_empty() {
            executable(&self.executable);
        }
        output("Official Grok installation fixture".into());
        Ok(())
    }
    fn version(&self, _: &Path) -> anyhow::Result<String> {
        Ok("grok 1.0.70".into())
    }
}

#[test]
fn grok_installs_from_the_official_channel_and_is_redetected_without_connecting() {
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("grok-build").unwrap();
    let manager = InstallationManager::new(
        environment(root.path(), HostPlatform::MacOs),
        "aarch64".into(),
        Arc::new(Installer {
            executable: root.path().join(".tokenmarket/tools/grok-build/bin/grok"),
        }),
    );
    assert!(manager.availability(tool).supported);
    assert_eq!(
        manager.availability(tool).version.as_deref(),
        Some("latest")
    );
    manager.start(tool).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        let status = manager.status(tool).unwrap();
        if status.stage.is_terminal() {
            assert_eq!(status.stage, InstallStage::Succeeded);
            assert!(status.installed);
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(!root.path().join(".grok/config.toml").exists());
    assert!(!root
        .path()
        .join(".tokenmarket/desktop-connections")
        .exists());
}
