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
fn oh_my_pi_has_its_own_canonical_identity_path_detection_and_launch() {
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("oh-my-pi").expect("Oh My Pi is a supported Tool");
    assert_eq!(serde_json::to_value(tool).unwrap(), json!("oh-my-pi"));
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    let row = inventory
        .list()
        .unwrap()
        .into_iter()
        .find(|r| r.id == tool)
        .unwrap();
    assert_eq!(row.name, "Oh My Pi");
    assert!(!row.installed);
    assert_eq!(
        PathBuf::from(row.config_path),
        root.path().join(".omp/agent/models.yml")
    );
    executable(&root.path().join("bin/omp"));
    inventory.require_installed(tool).unwrap();
    assert!(inventory
        .launch_plan(tool)
        .unwrap()
        .args
        .iter()
        .any(|arg| arg.contains("omp")));
}

async fn platform(
    catalogues: Vec<Value>,
    expected: usize,
) -> (String, tokio::task::JoinHandle<Vec<String>>) {
    platform_mode(catalogues, expected, "sk-omp-fixture", false).await
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
                json!({"success":true,"data":{"status":"VALID","key_id":"9020","owner":{"id":"u1","name":"Ada"},"bound_workspace":{"id":"42","name":"Research"}}})
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

fn read_yaml(path: &Path) -> Value {
    serde_yaml::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
#[tokio::test]
async fn oh_my_pi_imports_all_models_with_inline_key_and_preserves_oauth_store_and_model_roles() {
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("oh-my-pi").unwrap();
    executable(&root.path().join("bin/omp"));
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    let path = inventory.config_path(tool);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let settings=b"modelRoles:\n  default: manual/local\ncustom: keep\nauth:\n  broker:\n    url: http://127.0.0.1:1\n    token: existing-broker-token\n";
    std::fs::write(path.with_file_name("config.yaml"), settings).unwrap();
    let db = b"opaque OAuth store fixture";
    std::fs::write(path.with_file_name("agent.db"), db).unwrap();
    std::fs::write(
        path.with_file_name("auth.json"),
        "unrelated legacy file; do not parse",
    )
    .unwrap();
    let original = json!({"other":"keep","providers":{"tokenmarket":{"custom":"keep","models":[{"id":"user-model","name":"Manual"},{"id":"alpha.model","name":"Old","api":"anthropic-messages","baseUrl":"https://manual.invalid","custom":true}]}}});
    std::fs::write(&path, serde_yaml::to_string(&original).unwrap()).unwrap();
    let (url, server) = platform(vec![catalogue()], 2).await;
    let manager = support::manager(root.path().join("state"), url.clone(), url.clone());
    let connected = manager
        .connect(context(), tool, &inventory, path.clone())
        .await
        .unwrap();
    assert_eq!(connected.models, ["vendor/zeta~1", "alpha.model"]);
    assert_eq!(connected.protocol, "OPENAI_CHAT_COMPLETIONS");
    let value = read_yaml(&path);
    let provider = &value["providers"]["tokenmarket"];
    assert_eq!(provider["baseUrl"], format!("{url}/v1"));
    assert_eq!(provider["apiKey"], "sk-omp-fixture");
    assert_eq!(provider["api"], "openai-completions");
    assert_eq!(provider["authHeader"], true);
    assert_eq!(provider["auth"], "apiKey");
    let rows = provider["models"].as_array().unwrap();
    assert_eq!(rows[0]["name"], "Manual");
    assert_eq!(rows[1]["api"], "openai-completions");
    assert_eq!(rows[1]["baseUrl"], format!("{url}/v1"));
    assert_eq!(rows[1]["custom"], true);
    let zeta = rows.iter().find(|r| r["id"] == "vendor/zeta~1").unwrap();
    assert_eq!(zeta["contextWindow"], 65536);
    assert_eq!(zeta["maxTokens"], 4096);
    assert_eq!(std::fs::read(path.with_file_name("agent.db")).unwrap(), db);
    assert_eq!(
        std::fs::read(path.with_file_name("config.yaml")).unwrap(),
        settings
    );
    assert!(server
        .await
        .unwrap()
        .iter()
        .all(|r| !r.starts_with("POST /v1/")));
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
        Ok("omp 18.15.0".into())
    }
}

#[test]
fn oh_my_pi_installs_the_official_pinned_native_binary_without_bun_or_shell_dependencies() {
    use std::sync::Arc;
    use tokenmarket_core::installation::{InstallStage, InstallationManager};
    for (platform, architecture, asset, sha, command) in [
        (
            HostPlatform::MacOs,
            "aarch64",
            "omp-darwin-arm64",
            "d36dfabf0ee52d6f08efadb59c46f0349545c6f6975522417adae29646e73e34",
            "omp",
        ),
        (
            HostPlatform::Unix,
            "x86_64",
            "omp-linux-x64",
            "6737ee2415555a3656cc7b89c14d1c25c18658ee4c0fd4f7309705149ccb7b0f",
            "omp",
        ),
        (
            HostPlatform::Windows,
            "aarch64",
            "omp-windows-arm64.exe",
            "a9a4d83c509322f52fe9f3b187163d9c0cbe878d8a1ca4883032820c697a8184",
            "omp.exe",
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        let tool = tool_id("oh-my-pi").unwrap();
        let executor = Arc::new(Installer {
            executable: root
                .path()
                .join(".tokenmarket/tools/oh-my-pi/bin")
                .join(command),
            steps: Default::default(),
        });
        let manager = InstallationManager::new(
            environment(root.path(), platform),
            architecture.into(),
            executor.clone(),
        );
        assert!(manager.availability(tool).supported);
        assert_eq!(
            manager.availability(tool).version.as_deref(),
            Some("18.15.0")
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
        let steps = executor.steps.lock().unwrap();
        assert!(steps
            .iter()
            .any(|s| s.download.as_ref().is_some_and(|(url, _)| url
                == &format!(
                    "https://github.com/can1357/oh-my-pi/releases/download/v18.15.0/{asset}"
                ))));
        assert!(steps.iter().any(|s| s.args.join(" ").contains(sha)));
        assert!(!steps
            .iter()
            .any(|s| s.args.iter().any(|arg| arg.contains("bun install")
                || arg.contains("npm install")
                || arg.contains("git-for-windows"))));
        assert!(!root.path().join(".omp").exists());
        assert!(!root.path().join(".pi").exists());
    }
    let root = tempfile::tempdir().unwrap();
    let manager = InstallationManager::new(
        environment(root.path(), HostPlatform::MacOs),
        "riscv64".into(),
        Arc::new(Installer {
            executable: root.path().join("unused"),
            steps: Default::default(),
        }),
    );
    assert!(!manager.availability(tool_id("oh-my-pi").unwrap()).supported);
}

#[tokio::test]
async fn oh_my_pi_reconnect_and_disconnect_restore_owned_fields_after_model_reordering() {
    use tokenmarket_core::connections::{ConnectionTestRequest, DisconnectRequest};
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("oh-my-pi").unwrap();
    executable(&root.path().join("bin/omp"));
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
    let mut current: Value = read_yaml(&path);
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
    let restored: Value = read_yaml(&path);
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
async fn oh_my_pi_catalogue_failure_preserves_existing_models_and_saved_preferences() {
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("oh-my-pi").unwrap();
    executable(&root.path().join("bin/omp"));
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
async fn oh_my_pi_detects_disabled_provider_settings_added_after_connect_before_test() {
    use tokenmarket_core::connections::{ConnectionTestRequest, DisconnectRequest};
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("oh-my-pi").unwrap();
    executable(&root.path().join("bin/omp"));
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    let path = inventory.config_path(tool);
    let (url, server) = platform(vec![catalogue()], 2).await;
    let manager = support::manager(root.path().join("state"), url.clone(), url);
    let connected = manager
        .connect(context(), tool, &inventory, path.clone())
        .await
        .unwrap();
    server.await.unwrap();
    let auth = b"disabledProviders: [tokenmarket]\n";
    std::fs::write(path.with_file_name("config.yml"), auth).unwrap();
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
        .contains("disabledProviders"));
    assert_eq!(
        manager
            .disconnect(tool, &connected.key_id, None, DisconnectRequest::default())
            .await
            .unwrap()
            .state,
        "disconnected"
    );
    assert_eq!(
        std::fs::read(path.with_file_name("config.yml")).unwrap(),
        auth
    );
}

#[tokio::test]
async fn oh_my_pi_recovers_the_config_key_without_reading_or_replacing_oauth() {
    use tokenmarket_core::connections::ConnectionManager;
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("oh-my-pi").unwrap();
    executable(&root.path().join("bin/omp"));
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    let path = inventory.config_path(tool);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let secret = "sk-omp-recover";
    let encoded = secret;
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
    let value: Value = read_yaml(&path);
    assert_eq!(value["providers"]["tokenmarket"]["apiKey"], encoded);
    let requests = server.await.unwrap();
    use sha2::{Digest, Sha256};
    let expected = format!("{:x}", Sha256::digest(secret.as_bytes()));
    for request in requests.iter().filter(|r| r.starts_with("POST")) {
        assert!(request.contains(&expected));
        assert!(!request.contains(secret));
    }
}

#[test]
fn oh_my_pi_resolves_only_the_active_profile_and_default_override() {
    const CASE: &str = "TOKENMARKET_OMP_PROFILE_CASE";
    if let Ok(case) = std::env::var(CASE) {
        let home = PathBuf::from(std::env::var_os("TOKENMARKET_OMP_HOME").unwrap());
        let tool = tool_id("oh-my-pi").unwrap();
        let inventory = ToolInventory::new(environment(&home, HostPlatform::MacOs));
        let expected = match case.as_str() {
            "named" => home.join(".omp/profiles/work/agent/models.yml"),
            "legacy" => home.join(".omp/profiles/legacy/agent/models.yml"),
            "root" => home.join(".custom-omp/profiles/work/agent/models.yml"),
            "defaultderived" => home.join(".omp/agent/models.yml"),
            _ => home.join("custom/models.yml"),
        };
        assert_eq!(inventory.config_path(tool), expected);
        assert_ne!(
            inventory.config_path(tool),
            inventory.config_path(tool_id("pi").unwrap())
        );
        return;
    }
    let root = tempfile::tempdir().unwrap();
    for case in [
        "named",
        "legacy",
        "empty",
        "default",
        "root",
        "defaultderived",
    ] {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap());
        child
            .args([
                "--exact",
                "oh_my_pi_resolves_only_the_active_profile_and_default_override",
                "--nocapture",
            ])
            .env(CASE, case)
            .env("TOKENMARKET_OMP_HOME", root.path())
            .env("PI_CODING_AGENT_DIR", root.path().join("custom"))
            .env_remove("OMP_PROFILE")
            .env_remove("PI_PROFILE")
            .env_remove("PI_CONFIG_DIR");
        match case {
            "named" | "root" => {
                child
                    .env("OMP_PROFILE", "work")
                    .env("PI_PROFILE", "ignored");
            }
            "legacy" => {
                child.env("PI_PROFILE", "legacy");
            }
            "defaultderived" => {
                child
                    .env("OMP_PROFILE", "")
                    .env("PI_PROFILE", "legacy")
                    .env(
                        "PI_CODING_AGENT_DIR",
                        root.path().join(".omp/profiles/legacy/agent"),
                    );
            }
            "empty" => {
                child.env("OMP_PROFILE", "").env("PI_PROFILE", "ignored");
            }
            _ => {
                child
                    .env("OMP_PROFILE", "default")
                    .env("PI_PROFILE", "ignored");
            }
        };
        if case == "root" {
            child.env("PI_CONFIG_DIR", ".custom-omp");
        }
        let output = child.output().unwrap();
        assert!(
            output.status.success(),
            "{case}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}

#[tokio::test]
async fn oh_my_pi_refuses_conflicting_modes_and_invalid_models_without_issuing_or_editing() {
    for (config, settings, expected) in [
        (
            json!({}),
            json!({"disabledProviders":["tokenmarket"]}),
            "disabledProviders",
        ),
        (
            json!({}),
            json!({"auth":{"broker":{"url":"http://127.0.0.1:1"}}}),
            "broker",
        ),
        (
            json!({"providers":{"tokenmarket":{"transport":"pi-native"}}}),
            json!({}),
            "transport",
        ),
        (
            json!({"providers":{"tokenmarket":{"auth":"oauth"}}}),
            json!({}),
            "auth",
        ),
        (
            json!({"providers":{"tokenmarket":{"headers":{"Authorization":"manual"}}}}),
            json!({}),
            "认证",
        ),
        (
            json!({"providers":{"tokenmarket":{"models":[{"id":"duplicate"},{"id":"duplicate"}]}}}),
            json!({}),
            "重复",
        ),
        (
            json!({"providers":{"tokenmarket":{"modelOverrides":{"alpha":{"api":"anthropic-messages"}}}}}),
            json!({}),
            "modelOverrides",
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        let tool = tool_id("oh-my-pi").unwrap();
        executable(&root.path().join("bin/omp"));
        let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
        let path = inventory.config_path(tool);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let config = serde_yaml::to_string(&config).unwrap();
        let settings = serde_yaml::to_string(&settings).unwrap();
        std::fs::write(&path, &config).unwrap();
        std::fs::write(path.with_file_name("config.yaml"), &settings).unwrap();
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
        assert_eq!(std::fs::read_to_string(&path).unwrap(), config);
        assert_eq!(
            std::fs::read_to_string(path.with_file_name("config.yaml")).unwrap(),
            settings
        );
        assert!(manager.list().unwrap().is_empty());
    }
}

#[tokio::test]
async fn oh_my_pi_accepts_empty_yaml_and_preserves_the_existing_yaml_filename() {
    for original in ["", "null\n"] {
        let root = tempfile::tempdir().unwrap();
        let tool = tool_id("oh-my-pi").unwrap();
        executable(&root.path().join("bin/omp"));
        let agent = root.path().join(".omp/agent");
        std::fs::create_dir_all(&agent).unwrap();
        let path = agent.join("models.yaml");
        std::fs::write(&path, original).unwrap();
        std::fs::write(agent.join("config.yml"), "null\n").unwrap();
        let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
        assert_eq!(inventory.config_path(tool), path);
        let (url, server) = platform(vec![catalogue()], 2).await;
        let manager = support::manager(root.path().join("state"), url.clone(), url);
        manager
            .connect(context(), tool, &inventory, path.clone())
            .await
            .unwrap();
        assert!(read_yaml(&path)["providers"]["tokenmarket"]["models"].is_array());
        assert!(!agent.join("models.yml").exists());
        assert_eq!(
            std::fs::read_to_string(agent.join("config.yml")).unwrap(),
            "null\n"
        );
        server.await.unwrap();
    }
}

#[tokio::test]
async fn oh_my_pi_refuses_testing_a_connection_after_the_active_profile_changes() {
    const CHILD: &str = "TOKENMARKET_OMP_PROFILE_DRIFT_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "oh_my_pi_refuses_testing_a_connection_after_the_active_profile_changes",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env("OMP_PROFILE", "work")
            .env_remove("PI_PROFILE")
            .env_remove("PI_CONFIG_DIR")
            .env_remove("PI_CODING_AGENT_DIR")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        return;
    }
    use tokenmarket_core::connections::ConnectionTestRequest;
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("oh-my-pi").unwrap();
    executable(&root.path().join("bin/omp"));
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    let path = inventory.config_path(tool);
    let (url, server) = platform(vec![catalogue()], 2).await;
    let manager = support::manager(root.path().join("state"), url.clone(), url);
    let connected = manager
        .connect(context(), tool, &inventory, path.clone())
        .await
        .unwrap();
    server.await.unwrap();
    let before = std::fs::read(&path).unwrap();
    std::env::set_var("OMP_PROFILE", "personal");
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
        .contains("profile"));
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[tokio::test]
async fn oh_my_pi_disconnect_preserves_a_user_cleared_valid_yaml_document() {
    use tokenmarket_core::connections::DisconnectRequest;
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("oh-my-pi").unwrap();
    executable(&root.path().join("bin/omp"));
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    let path = inventory.config_path(tool);
    let (url, server) = platform(vec![catalogue()], 2).await;
    let manager = support::manager(root.path().join("state"), url.clone(), url);
    let connected = manager
        .connect(context(), tool, &inventory, path.clone())
        .await
        .unwrap();
    server.await.unwrap();
    std::fs::write(&path, "").unwrap();
    assert_eq!(
        manager
            .disconnect(tool, &connected.key_id, None, DisconnectRequest::default())
            .await
            .unwrap()
            .state,
        "disconnected"
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"");
}

#[tokio::test]
async fn oh_my_pi_preserves_config_when_catalogue_limits_conflict_with_an_existing_model_cap() {
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("oh-my-pi").unwrap();
    executable(&root.path().join("bin/omp"));
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    let path = inventory.config_path(tool);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let original="providers:\n  tokenmarket:\n    models:\n      - id: vendor/zeta~1\n        contextWindow: 32768\n        maxContextWindow: 32768\n";
    std::fs::write(&path, original).unwrap();
    let (url, server) = platform(vec![catalogue()], 2).await;
    let manager = support::manager(root.path().join("state"), url.clone(), url);
    let error = manager
        .connect(context(), tool, &inventory, path.clone())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("maxContextWindow"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    assert!(manager.list().unwrap().is_empty());
    server.await.unwrap();
}
