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
fn kimi_code_has_its_own_canonical_identity_path_detection_and_launch() {
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("kimi-code").expect("Kimi Code is a supported Tool");
    assert_eq!(serde_json::to_value(tool).unwrap(), json!("kimi-code"));
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    let row = inventory
        .list()
        .unwrap()
        .into_iter()
        .find(|r| r.id == tool)
        .unwrap();
    assert_eq!(row.name, "Kimi Code");
    assert!(!row.installed);
    assert_eq!(
        PathBuf::from(row.config_path),
        root.path().join(".kimi-code/config.toml")
    );
    executable(&root.path().join("bin/kimi"));
    inventory.require_installed(tool).unwrap();
    assert!(inventory
        .launch_plan(tool)
        .unwrap()
        .args
        .iter()
        .any(|arg| arg.contains("kimi")));
}

async fn platform(
    catalogues: Vec<Value>,
    expected: usize,
) -> (String, tokio::task::JoinHandle<Vec<String>>) {
    platform_mode(catalogues, expected, "sk-kimi-fixture", false).await
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
        {"id":"alpha.model","model_types":["TEXT_GENERATION"],"context_window_tokens":32768},
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
async fn kimi_code_imports_all_models_preserving_default_thinking_and_unrelated_auth() {
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("kimi-code").unwrap();
    executable(&root.path().join("bin/kimi"));
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    let path = inventory.config_path(tool);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let original = r#"# User preference
default_model = "manual"
[thinking]
enabled = true
effort = "high"
[providers.manual]
type="openai"
base_url="https://manual.invalid/v1"
api_key="private"
[models.manual]
provider="manual"
model="manual-id"
max_context_size=32768
"#;
    std::fs::write(&path, original).unwrap();
    let (url, server) = platform(vec![catalogue()], 2).await;
    let manager = support::manager(root.path().join("state"), url.clone(), url.clone());
    let connected = manager
        .connect(context(), tool, &inventory, path.clone())
        .await
        .unwrap();
    assert_eq!(connected.models, ["vendor/zeta~1", "alpha.model"]);
    assert_eq!(connected.protocol, "OPENAI_CHAT_COMPLETIONS");
    let value = read_toml(&path);
    assert_eq!(value["default_model"], "manual");
    assert_eq!(value["thinking"]["enabled"], true);
    assert_eq!(value["providers"]["manual"]["api_key"], "private");
    assert_eq!(value["providers"]["tokenmarket"]["type"], "openai");
    assert_eq!(
        value["providers"]["tokenmarket"]["base_url"],
        format!("{url}/v1")
    );
    assert_eq!(
        value["providers"]["tokenmarket"]["api_key"],
        "sk-kimi-fixture"
    );
    let model = &value["models"]["tokenmarket/vendor/zeta~1"];
    assert_eq!(model["model"], "vendor/zeta~1");
    assert_eq!(model["max_context_size"], 65536);
    assert_eq!(model["capabilities"], json!(["tool_use"]));
    assert!(std::fs::read_to_string(&path)
        .unwrap()
        .contains("# User preference"));
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
        Ok("kimi 2.4.1".into())
    }
}

#[test]
fn kimi_installs_pinned_official_native_artifacts_and_prepares_windows_bash_without_connecting() {
    use std::sync::Arc;
    use tokenmarket_core::installation::{InstallStage, InstallationManager};
    for (platform, arch, asset, sha, command) in [
        (
            HostPlatform::MacOs,
            "aarch64",
            "kimi-code-darwin-arm64.tar.gz",
            "17f848e7b9b159aed9b2ad0df917c4be3c484393d924a6540171ede7b87cc66d",
            "kimi",
        ),
        (
            HostPlatform::Unix,
            "x86_64",
            "kimi-code-linux-x64.tar.gz",
            "f4a225fb8afe0394151e721bc3cd94d71e314fbea7f75900453556da7d8738de",
            "kimi",
        ),
        (
            HostPlatform::Windows,
            "aarch64",
            "kimi-code-win32-arm64.zip",
            "af2ce55589d1dbd0205ef6174cd91e41d79ad1704d7bd812ec710e379311ffb0",
            "kimi.exe",
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        let tool = tool_id("kimi-code").unwrap();
        let executor = Arc::new(Installer {
            executable: root
                .path()
                .join(".tokenmarket/tools/kimi-code/bin")
                .join(command),
            steps: Default::default(),
        });
        let manager = InstallationManager::new(
            environment(root.path(), platform),
            arch.into(),
            executor.clone(),
        );
        assert!(manager.availability(tool).supported);
        assert_eq!(manager.availability(tool).version.as_deref(), Some("2.4.1"));
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
        assert!(steps.iter().any(|s|s.download.as_ref().is_some_and(|(url,_)|url==&format!("https://github.com/MoonshotAI/kimi-code/releases/download/%40moonshot-ai/kimi-code%402.4.1/{asset}"))));
        assert!(steps.iter().any(|s| s.args.join(" ").contains(sha)));
        assert!(!steps.iter().any(|s| s
            .args
            .iter()
            .any(|arg| arg.contains("npm install") || arg.contains("uv tool"))));
        if platform == HostPlatform::Windows {
            assert!(steps.iter().any(|s| s
                .download
                .as_ref()
                .is_some_and(|(url, _)| url
                    .contains("git-for-windows/git/releases/download/v2.56.0.windows.4/"))));
        }
        assert!(!root.path().join(".kimi-code").exists());
    }
}

#[tokio::test]
async fn kimi_recovers_the_literal_config_key_without_implicit_shell_fallback() {
    use tokenmarket_core::connections::ConnectionManager;
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("kimi-code").unwrap();
    executable(&root.path().join("bin/kimi"));
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    let path = inventory.config_path(tool);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let secret = "!never-execute${KEY}";
    let encoded = secret;
    let (url, server) = platform_mode(vec![catalogue()], 3, secret, true).await;
    std::fs::write(&path,toml_edit::ser::to_string(&json!({"providers":{"tokenmarket":{"type":"openai","base_url":format!("{url}/v1"),"api_key":encoded}}})).unwrap()).unwrap();
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
    let value: Value = read_toml(&path);
    assert_eq!(value["providers"]["tokenmarket"]["api_key"], encoded);
    let requests = server.await.unwrap();
    use sha2::{Digest, Sha256};
    let expected = format!("{:x}", Sha256::digest(secret.as_bytes()));
    for request in requests.iter().filter(|r| r.starts_with("POST")) {
        assert!(request.contains(&expected));
        assert!(!request.contains(secret));
    }
}

#[tokio::test]
async fn kimi_reconnect_reuses_key_context_and_thinking_preferences_then_restores_only_owned_fields(
) {
    use tokenmarket_core::connections::{ConnectionTestRequest, DisconnectRequest};
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("kimi-code").unwrap();
    executable(&root.path().join("bin/kimi"));
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    let path = inventory.config_path(tool);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let original = r#"# Keep preference
default_model="tokenmarket/alpha.model"
[thinking]
enabled=true
effort="low"
[providers.tokenmarket]
type="openai"
base_url="https://manual.invalid/v1"
api_key="manual"
[models."tokenmarket/alpha.model"]
provider="tokenmarket"
model="alpha.model"
max_context_size=8192
capabilities=["thinking"]
custom="keep"
"#;
    std::fs::write(&path, original).unwrap();
    let next = json!({"data":[{"id":"alpha.model","model_types":["TEXT_GENERATION"]}]});
    let (url, server) = platform(vec![catalogue(), next], 4).await;
    let manager = support::manager(root.path().join("state"), url.clone(), url);
    let connected = manager
        .connect(context(), tool, &inventory, path.clone())
        .await
        .unwrap();
    let mut text = std::fs::read_to_string(&path).unwrap();
    text.push_str("\n[user_edit]\nnote=\"keep later\"\n");
    std::fs::write(&path, text).unwrap();
    let reconnected = manager
        .reconnect(context(), tool, &connected.key_id, &inventory, path.clone())
        .await
        .unwrap();
    assert!(reconnected.key_reused);
    assert_eq!(reconnected.connection.models, ["alpha.model"]);
    let value = read_toml(&path);
    assert!(value["models"].get("tokenmarket/vendor/zeta~1").is_none());
    assert_eq!(
        value["models"]["tokenmarket/alpha.model"]["max_context_size"],
        32768
    );
    assert_eq!(
        value["models"]["tokenmarket/alpha.model"]["capabilities"],
        json!(["thinking", "tool_use"])
    );
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
    let restored = read_toml(&path);
    assert_eq!(restored["default_model"], "tokenmarket/alpha.model");
    assert_eq!(restored["thinking"]["effort"], "low");
    assert_eq!(
        restored["models"]["tokenmarket/alpha.model"]["max_context_size"],
        8192
    );
    assert_eq!(
        restored["models"]["tokenmarket/alpha.model"]["capabilities"],
        json!(["thinking"])
    );
    assert_eq!(
        restored["models"]["tokenmarket/alpha.model"]["custom"],
        "keep"
    );
    assert_eq!(restored["providers"]["tokenmarket"]["api_key"], "manual");
    assert_eq!(restored["user_edit"]["note"], "keep later");
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
async fn kimi_missing_context_preserves_config_and_retry_reuses_the_already_saved_key() {
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("kimi-code").unwrap();
    executable(&root.path().join("bin/kimi"));
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    let path = inventory.config_path(tool);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let original = "# untouched preferences\n[thinking]\nenabled=false\n";
    std::fs::write(&path, original).unwrap();
    let missing = json!({"data":[{"id":"missing-model","model_types":["TEXT_GENERATION"]}]});
    let (url, server) = platform(vec![missing, catalogue()], 3).await;
    let manager = support::manager(root.path().join("state"), url.clone(), url);
    let error = manager
        .connect(context(), tool, &inventory, path.clone())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("max_context_size"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    assert!(manager.list().unwrap().is_empty());
    manager
        .connect(context(), tool, &inventory, path.clone())
        .await
        .unwrap();
    assert_eq!(read_toml(&path)["thinking"]["enabled"], false);
    let requests = server.await.unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.starts_with("POST /api/desktop/connections/keys "))
            .count(),
        1
    );
    assert!(requests.iter().all(|r| !r.starts_with("POST /v1/")));
}

#[tokio::test]
async fn kimi_only_reuses_a_valid_context_for_the_same_existing_model_identity() {
    for same in [true, false] {
        let root = tempfile::tempdir().unwrap();
        let tool = tool_id("kimi-code").unwrap();
        executable(&root.path().join("bin/kimi"));
        let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
        let path = inventory.config_path(tool);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let original=format!("[models.\"tokenmarket/alpha.model\"]\nprovider=\"tokenmarket\"\nmodel=\"{}\"\nmax_context_size=12345\n",if same{"alpha.model"}else{"different-model"});
        std::fs::write(&path, &original).unwrap();
        let (url, server) = platform(
            vec![json!({"data":[{"id":"alpha.model","model_types":["TEXT_GENERATION"]}]})],
            2,
        )
        .await;
        let manager = support::manager(root.path().join("state"), url.clone(), url);
        let result = manager
            .connect(context(), tool, &inventory, path.clone())
            .await;
        if same {
            result.unwrap();
            assert_eq!(
                read_toml(&path)["models"]["tokenmarket/alpha.model"]["max_context_size"],
                12345
            )
        } else {
            assert!(result.unwrap_err().to_string().contains("max_context_size"));
            assert_eq!(std::fs::read_to_string(&path).unwrap(), original)
        }
        server.await.unwrap();
    }
}

#[tokio::test]
async fn kimi_auth_and_dynamic_route_conflicts_fail_before_key_issuance() {
    for (original,expected) in [
 ("[providers.tokenmarket]\napi_key_env=\"OPENAI_API_KEY\"\n","api_key_env"),
 ("[providers.tokenmarket.oauth]\nstorage=\"file\"\nkey=\"unrelated\"\n","oauth"),
 ("[providers.tokenmarket.source]\nurl=\"https://registry.invalid\"\n","来源"),
 ("[providers.tokenmarket.custom_headers]\nAuthorization=\"manual\"\n","认证"),
 ("[models.\"tokenmarket/alpha\"]\nprotocol=\"anthropic\"\nbase_url=\"https://manual.invalid\"\n","路由"),
 ("thinking=true\n","thinking"),
 ("[models.\"tokenmarket/alpha\"]\ncapabilities=12\n","capabilities"),
 ] {
 let root=tempfile::tempdir().unwrap();let tool=tool_id("kimi-code").unwrap();executable(&root.path().join("bin/kimi"));let inventory=ToolInventory::new(environment(root.path(),HostPlatform::MacOs));let path=inventory.config_path(tool);std::fs::create_dir_all(path.parent().unwrap()).unwrap();std::fs::write(&path,original).unwrap();let manager=support::manager(root.path().join("state"),"http://127.0.0.1:1".into(),"http://127.0.0.1:1".into());let error=manager.connect(context(),tool,&inventory,path.clone()).await.unwrap_err();assert!(error.to_string().contains(expected),"{error}");assert_eq!(std::fs::read_to_string(&path).unwrap(),original);assert!(manager.list().unwrap().is_empty());
 }
}

#[test]
fn kimi_reuses_legacy_pi_git_and_launches_with_the_resolved_private_bash() {
    use std::sync::Arc;
    use tokenmarket_core::installation::{InstallStage, InstallationManager};
    let root = tempfile::tempdir().unwrap();
    let bash = root.path().join(".tokenmarket/tools/pi/git/bin/bash.exe");
    executable(&bash);
    let tool = tool_id("kimi-code").unwrap();
    let target = root
        .path()
        .join(".tokenmarket/tools/kimi-code/bin/kimi.exe");
    let executor = Arc::new(Installer {
        executable: target.clone(),
        steps: Default::default(),
    });
    let env = environment(root.path(), HostPlatform::Windows);
    let manager = InstallationManager::new(env.clone(), "x86_64".into(), executor.clone());
    manager.start(tool).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        let state = manager.status(tool).unwrap();
        if state.stage.is_terminal() {
            assert_eq!(state.stage, InstallStage::Succeeded);
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(!executor.steps.lock().unwrap().iter().any(|step| step
        .download
        .as_ref()
        .is_some_and(|(url, _)| url.contains("git-for-windows"))));
    let mut env = env;
    env.command_directories
        .push(target.parent().unwrap().into());
    let inventory = ToolInventory::new(env);
    assert!(inventory
        .launch_plan(tool)
        .unwrap()
        .environment
        .iter()
        .any(|(name, value)| name == "KIMI_SHELL_PATH" && Path::new(value) == bash));
}

#[tokio::test]
async fn kimi_environment_overrides_are_scoped_and_detected_before_a_paid_test() {
    const CHILD: &str = "TOKENMARKET_KIMI_ENV_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let root = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "kimi_environment_overrides_are_scoped_and_detected_before_a_paid_test",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env("KIMI_CODE_HOME", root.path().join("custom"))
            .env_remove("KIMI_MODEL_NAME")
            .env_remove("KIMI_CODE_CUSTOM_HEADERS")
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
    let tool = tool_id("kimi-code").unwrap();
    executable(&root.path().join("bin/kimi"));
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    let path = PathBuf::from(std::env::var_os("KIMI_CODE_HOME").unwrap()).join("config.toml");
    assert_eq!(inventory.config_path(tool), path);
    assert_eq!(
        tokenmarket_core::adapters::get_adapter(tool).config_path(),
        path
    );
    let (url, server) = platform(vec![catalogue()], 2).await;
    let manager = support::manager(root.path().join("state"), url.clone(), url);
    let connected = manager
        .connect(context(), tool, &inventory, path.clone())
        .await
        .unwrap();
    server.await.unwrap();
    let before = std::fs::read(&path).unwrap();
    for (name, value) in [
        ("KIMI_MODEL_NAME", "temporary-other-model"),
        ("KIMI_CODE_CUSTOM_HEADERS", "Authorization: Bearer manual"),
    ] {
        std::env::set_var(name, value);
        assert_eq!(
            manager.summaries().unwrap()[0].1.state,
            "configuration_changed"
        );
        let error = manager
            .test_connection(
                tool,
                &connected.key_id,
                ConnectionTestRequest {
                    confirmed: true,
                    model: Some("alpha.model".into()),
                },
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains(name));
        assert_eq!(std::fs::read(&path).unwrap(), before);
        std::env::remove_var(name);
    }
}

#[cfg(unix)]
#[tokio::test]
async fn kimi_legacy_major_is_rejected_before_key_issuance_and_inventory_never_probes_versions() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let binary = root.path().join("bin/kimi");
    let marker = root.path().join("version-probed");
    std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
    std::fs::write(
        &binary,
        format!(
            "#!/bin/sh\ntouch '{}'\nprintf '%s\\n' 'kimi, version 0.58.0'\n",
            marker.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
    let tool = tool_id("kimi-code").unwrap();
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    inventory.list().unwrap();
    assert!(!marker.exists());
    let path = inventory.config_path(tool);
    let manager = support::manager(
        root.path().join("state"),
        "http://127.0.0.1:1".into(),
        "http://127.0.0.1:1".into(),
    );
    let error = manager
        .connect(context(), tool, &inventory, path.clone())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("主版本"), "{error}");
    assert!(marker.exists());
    assert!(!path.exists());
    assert!(manager.list().unwrap().is_empty());
}
