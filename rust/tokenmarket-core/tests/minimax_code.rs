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
fn minimax_canonical_id_inventory_detection_and_launch_use_mcode_and_data_directory() {
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("minimax-code").expect("MiniMax Code is supported");
    assert_eq!(serde_json::to_value(tool).unwrap(), json!("minimax-code"));
    assert_eq!(
        serde_json::from_value::<tokenmarket_core::ToolId>(json!("minimax-code")).unwrap(),
        tool
    );
    assert_eq!(tool.to_string(), "minimax-code");
    assert!(tool_id("mini-max-code").is_err());
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    let row = inventory
        .list()
        .unwrap()
        .into_iter()
        .find(|r| r.id == tool)
        .unwrap();
    assert_eq!(serde_json::to_value(&row).unwrap()["id"], "minimax-code");
    assert_eq!(row.name, "MiniMax Code");
    assert_eq!(
        PathBuf::from(row.config_path),
        root.path().join(".minimax/config.yaml")
    );
    assert!(!row.installed);
    executable(&root.path().join("bin/mcode"));
    assert!(inventory.is_installed(tool));
    assert!(inventory
        .launch_plan(tool)
        .unwrap()
        .args
        .iter()
        .any(|s| s.contains("mcode")));
}

async fn platform(
    catalogues: Vec<Value>,
    expected: usize,
) -> (String, tokio::task::JoinHandle<Vec<String>>) {
    platform_mode(catalogues, expected, "sk-minimax-fixture", false).await
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

fn read_config(path: &Path) -> Value {
    serde_yaml::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
fn write_config(path: &Path, value: &Value) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, serde_yaml::to_string(value).unwrap()).unwrap();
}
#[tokio::test]
async fn minimax_imports_all_text_models_into_custom_provider_without_selecting_or_testing() {
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("minimax-code").unwrap();
    executable(&root.path().join("bin/mcode"));
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    let path = inventory.config_path(tool);
    let original = json!({"defaultModel":"custom_provider:manual/local","defaultModelVariant":"careful","provider":{"minimax":{"options":{"apiKey":"session"}}},"minimax_api":{"apiKey":"other"},"custom_provider":{"manual":{"models":{"local":{}}},"tokenmarket":{"unknown":"keep","models":{"alpha.model":{"name":"old","enabled":true,"limit":{"input":123},"options":{"custom":"keep"}},"user-model":{"name":"keep"}}}}});
    write_config(&path, &original);
    let (url, server) = platform(vec![catalogue()], 2).await;
    let manager = support::manager(root.path().join("state"), url.clone(), url.clone());
    let connection = manager
        .connect(context(), tool, &inventory, path.clone())
        .await
        .unwrap();
    assert_eq!(connection.models, ["vendor/zeta~1", "alpha.model"]);
    assert_eq!(connection.protocol, "OPENAI_CHAT_COMPLETIONS");
    let actual = read_config(&path);
    let p = &actual["custom_provider"]["tokenmarket"];
    assert_eq!(p["options"]["baseURL"], format!("{url}/v1"));
    assert_eq!(p["options"]["apiKey"], "sk-minimax-fixture");
    assert_eq!(p["api"], "openai-completions");
    assert_eq!(p["kind"], "custom");
    assert_eq!(p["enabled"], true);
    assert_eq!(p["models"]["vendor/zeta~1"]["name"], "Zeta");
    assert_eq!(
        p["models"]["vendor/zeta~1"]["limit"],
        json!({"context":65536,"output":4096})
    );
    assert_eq!(p["models"]["alpha.model"]["limit"]["input"], 123);
    assert_eq!(p["models"]["alpha.model"]["options"]["custom"], "keep");
    assert_eq!(
        p["models"]["user-model"],
        original["custom_provider"]["tokenmarket"]["models"]["user-model"]
    );
    assert!(p["models"]["embedding"].is_null());
    assert!(p["models"]["vendor/zeta~1"]["tool_call"].is_null());
    for key in [
        "defaultModel",
        "defaultModelVariant",
        "provider",
        "minimax_api",
    ] {
        assert_eq!(actual[key], original[key]);
    }
    assert!(server
        .await
        .unwrap()
        .iter()
        .all(|r| !r.starts_with("POST /v1/")));
}

struct Installer {
    executable: PathBuf,
    steps: std::sync::Mutex<Vec<tokenmarket_core::installation::InstallStep>>,
    version: std::sync::Mutex<String>,
    fail: std::sync::atomic::AtomicBool,
    evidence: std::sync::atomic::AtomicBool,
}
impl tokenmarket_core::installation::InstallExecutor for Installer {
    fn execute(
        &self,
        step: &tokenmarket_core::installation::InstallStep,
        _: &std::sync::atomic::AtomicBool,
        output: &mut dyn FnMut(String),
    ) -> anyhow::Result<()> {
        self.steps.lock().unwrap().push(step.clone());
        output("fixture installer output".into());
        if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
            anyhow::bail!("fixture checksum/dependency failure")
        }
        if self.evidence.load(std::sync::atomic::Ordering::SeqCst) {
            executable(&self.executable);
        }
        Ok(())
    }
    fn version(&self, _: &Path) -> anyhow::Result<String> {
        Ok(self.version.lock().unwrap().clone())
    }
}
fn finished(
    manager: &tokenmarket_core::installation::InstallationManager,
    tool: tokenmarket_core::ToolId,
) -> tokenmarket_core::installation::InstallStatus {
    finished_with_timeout(manager, tool, std::time::Duration::from_secs(3))
}
fn finished_with_timeout(
    manager: &tokenmarket_core::installation::InstallationManager,
    tool: tokenmarket_core::ToolId,
    timeout: std::time::Duration,
) -> tokenmarket_core::installation::InstallStatus {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let s = manager.status(tool).unwrap();
        if s.stage.is_terminal() {
            return s;
        }
        assert!(std::time::Instant::now() < deadline, "{s:?}");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}
#[test]
fn minimax_uses_the_official_installer_with_npm_fallback_on_each_platform() {
    use std::sync::{atomic::AtomicBool, Arc, Mutex};
    use tokenmarket_core::installation::{InstallStage, InstallationManager};
    let tool = tool_id("minimax-code").unwrap();
    for platform in [
        HostPlatform::MacOs,
        HostPlatform::Unix,
        HostPlatform::Windows,
    ] {
        for arch in ["aarch64", "x86_64"] {
            let root = tempfile::tempdir().unwrap();
            let suffix = if platform == HostPlatform::Windows {
                "mcode.cmd"
            } else {
                "mcode"
            };
            let installer = Arc::new(Installer {
                executable: root
                    .path()
                    .join(".tokenmarket/tools/minimax-code/bin")
                    .join(suffix),
                steps: Mutex::new(vec![]),
                version: Mutex::new("0.7.2".into()),
                fail: AtomicBool::new(false),
                evidence: AtomicBool::new(true),
            });
            let manager = InstallationManager::new(
                environment(root.path(), platform),
                arch.into(),
                installer.clone(),
            );
            assert!(manager.availability(tool).supported, "{platform:?}/{arch}");
            manager.start(tool).unwrap();
            let s = finished(&manager, tool);
            assert_eq!(s.stage, InstallStage::Succeeded);
            assert!(s.installed);
            let steps = installer.steps.lock().unwrap();
            assert_eq!(steps.len(), 1);
            let execution = steps[0].args.join(" ");
            assert!(execution.contains("https://filecdn.minimax.chat/public/install."));
            assert!(execution.contains("@minimax-ai/code@latest"));
            assert!(execution.contains("--include=optional"));
            assert!(execution.contains("--allow-scripts=@minimax-ai/code,better-sqlite3"));
            assert!(!root.path().join(".minimax").exists());
        }
    }
}

#[tokio::test]
async fn minimax_refuses_credential_conflicts_and_broken_configuration_before_issuance() {
    for config in [
        json!({"custom_provider":{"tokenmarket":{"kind":"oauth","options":{"apiKey":"other"}}}}),
        json!({"custom_provider":{"tokenmarket":{"options":{"authMode":"managed-login"}}}}),
        json!({"custom_provider":{"tokenmarket":{"options":{"headers":{"aUtHoRiZaTiOn":"other"}}}}}),
        json!({"custom_provider":{"tokenmarket":{"models":{"local":{"options":{"apiKey":"other"}}}}}}),
        json!({"custom_provider":{"tokenmarket":{"models":{"local":{"headers":{"x-api-key":"other"}}}}}}),
        json!({"custom_provider":{"tokenmarket":{"enabled":false}}}),
        json!({"custom_provider":{"tokenmarket":{"options":{"apiKey":9}}}}),
        json!({"custom_provider":{"tokenmarket":{"models":[]}}}),
    ] {
        let root = tempfile::tempdir().unwrap();
        let tool = tool_id("minimax-code").unwrap();
        executable(&root.path().join("bin/mcode"));
        let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
        let path = inventory.config_path(tool);
        write_config(&path, &config);
        let before = std::fs::read(&path).unwrap();
        let manager = support::manager(
            root.path().join("state"),
            "http://127.0.0.1:1".into(),
            "http://127.0.0.1:1".into(),
        );
        let error = manager
            .connect(context(), tool, &inventory, path.clone())
            .await
            .unwrap_err();
        assert!(error.to_string().contains("MiniMax Code"), "{error}");
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(manager.list().unwrap().is_empty());
    }
}

#[test]
fn minimax_launch_keeps_the_managed_data_root_effective_without_changing_defaults() {
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("minimax-code").unwrap();
    for platform in [HostPlatform::MacOs, HostPlatform::Windows] {
        executable(
            &root
                .path()
                .join("bin")
                .join(if platform == HostPlatform::Windows {
                    "mcode.cmd"
                } else {
                    "mcode"
                }),
        );
        let inventory = ToolInventory::new(environment(root.path(), platform));
        let plan = inventory.launch_plan(tool).unwrap();
        if platform == HostPlatform::Windows {
            assert!(plan.environment.contains(&(
                "MINIMAX_DATA_DIR".into(),
                root.path().join(".minimax").to_string_lossy().into_owned()
            )));
        } else {
            assert!(plan
                .args
                .iter()
                .any(|s| s.contains("MINIMAX_DATA_DIR=") && s.contains(".minimax")));
        }
    }
}

#[tokio::test]
async fn minimax_reconnect_test_and_disconnect_preserve_per_leaf_model_edits_and_preferences() {
    use tokenmarket_core::connections::{ConnectionTestRequest, DisconnectRequest};
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("minimax-code").unwrap();
    executable(&root.path().join("bin/mcode"));
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    let path = inventory.config_path(tool);
    let original = json!({"defaultModel":"custom_provider:manual/local","defaultModelVariant":"keep","custom_provider":{"tokenmarket":{"options":{"custom":"untouched"},"models":{"alpha.model":{"name":"original","limit":{"input":123},"user":"initial"},"user-local":{"name":"manual"}}}}});
    write_config(&path, &original);
    let next = json!({"data":[{"id":"alpha.model","display_name":"Alpha","context_window_tokens":32000,"model_types":["TEXT_GENERATION"]}]});
    let (url, server) = platform(vec![catalogue(), next], 4).await;
    let manager = support::manager(root.path().join("state"), url.clone(), url);
    let connected = manager
        .connect(context(), tool, &inventory, path.clone())
        .await
        .unwrap();
    let mut current = read_config(&path);
    current["custom_provider"]["tokenmarket"]["models"]["vendor/zeta~1"]["limit"]["userLimit"] =
        json!(42);
    current["custom_provider"]["tokenmarket"]["models"]["vendor/zeta~1"]["custom"] = json!(true);
    write_config(&path, &current);
    let next = manager
        .reconnect(context(), tool, &connected.key_id, &inventory, path.clone())
        .await
        .unwrap();
    assert!(next.key_reused);
    assert_eq!(next.connection.models, ["alpha.model"]);
    let before = std::fs::read(&path).unwrap();
    assert!(manager
        .test_connection(
            tool,
            &connected.key_id,
            ConnectionTestRequest {
                confirmed: false,
                model: Some("alpha.model".into())
            }
        )
        .await
        .is_err());
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
    let restored = read_config(&path);
    assert_eq!(restored["defaultModel"], original["defaultModel"]);
    assert_eq!(
        restored["defaultModelVariant"],
        original["defaultModelVariant"]
    );
    let models = &restored["custom_provider"]["tokenmarket"]["models"];
    assert_eq!(
        models["alpha.model"],
        original["custom_provider"]["tokenmarket"]["models"]["alpha.model"]
    );
    assert_eq!(
        models["vendor/zeta~1"],
        json!({"limit":{"userLimit":42},"custom":true})
    );
    assert_eq!(models["user-local"]["name"], "manual");
    assert_eq!(
        restored["custom_provider"]["tokenmarket"]["options"],
        json!({"custom":"untouched"})
    );
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
    assert!(requests
        .iter()
        .filter(|r| r.starts_with("POST /api/desktop/"))
        .all(|r| r.contains("minimax-code") && !r.contains("mini-max-code")));
}

#[tokio::test]
async fn minimax_recovers_only_the_matching_gateway_literal_key_and_keeps_it_literal() {
    use tokenmarket_core::connections::ConnectionManager;
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("minimax-code").unwrap();
    executable(&root.path().join("bin/mcode"));
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    let path = inventory.config_path(tool);
    let key = "!never-execute${KEY}";
    let (url, server) = platform_mode(vec![catalogue()], 3, key, true).await;
    write_config(
        &path,
        &json!({"custom_provider":{"tokenmarket":{"api":"openai-completions","options":{"baseURL":format!("{url}/v1"),"apiKey":key},"models":{}}}}),
    );
    let manager = ConnectionManager::with_credentials(
        root.path().join("state"),
        url.clone(),
        url,
        support::credentials(&root.path().join("state")),
    );
    let connection = manager
        .connect(context(), tool, &inventory, path.clone())
        .await
        .unwrap();
    assert_eq!(connection.key_id, "9020");
    assert_eq!(
        read_config(&path)["custom_provider"]["tokenmarket"]["options"]["apiKey"],
        key
    );
    use sha2::{Digest, Sha256};
    let hash = format!("{:x}", Sha256::digest(key.as_bytes()));
    for request in server
        .await
        .unwrap()
        .iter()
        .filter(|r| r.starts_with("POST"))
    {
        assert!(request.contains(&hash));
        assert!(request.contains("\"tool\":\"minimax-code\""));
        assert!(!request.contains(key));
    }
}
#[tokio::test]
async fn minimax_catalogue_failure_retains_the_existing_connection_and_configuration() {
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("minimax-code").unwrap();
    executable(&root.path().join("bin/mcode"));
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    let path = inventory.config_path(tool);
    let (url, server) = platform(
        vec![
            catalogue(),
            json!({"data":[{"id":"vector","model_types":["EMBEDDING"]}]}),
        ],
        3,
    )
    .await;
    let manager = support::manager(root.path().join("state"), url.clone(), url);
    let connected = manager
        .connect(context(), tool, &inventory, path.clone())
        .await
        .unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(manager
        .reconnect(context(), tool, &connected.key_id, &inventory, path.clone())
        .await
        .unwrap_err()
        .to_string()
        .contains("文本生成"));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(manager.list().unwrap()[0].models, connected.models);
    assert!(server
        .await
        .unwrap()
        .iter()
        .all(|r| !r.starts_with("POST /v1/")));
}
#[test]
fn minimax_installation_failure_retry_existing_version_and_missing_evidence_do_not_mutate_configuration(
) {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    };
    use tokenmarket_core::installation::{InstallStage, InstallationManager};
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("minimax-code").unwrap();
    let path = root.path().join(".minimax/config.yaml");
    write_config(&path, &json!({"defaultModel":"keep"}));
    let before = std::fs::read(&path).unwrap();
    let installer = Arc::new(Installer {
        executable: root
            .path()
            .join(".tokenmarket/tools/minimax-code/bin/mcode"),
        steps: Mutex::new(vec![]),
        version: Mutex::new("0.7.2".into()),
        fail: AtomicBool::new(true),
        evidence: AtomicBool::new(false),
    });
    let manager = InstallationManager::new(
        environment(root.path(), HostPlatform::MacOs),
        "aarch64".into(),
        installer.clone(),
    );
    manager.start(tool).unwrap();
    let failed = finished(&manager, tool);
    assert_eq!(failed.stage, InstallStage::Failed);
    assert!(failed.detail.contains("fixture checksum"));
    assert!(!failed.output.is_empty());
    installer.fail.store(false, Ordering::SeqCst);
    manager.start(tool).unwrap();
    assert_eq!(finished(&manager, tool).stage, InstallStage::Failed);
    installer.evidence.store(true, Ordering::SeqCst);
    manager.start(tool).unwrap();
    assert_eq!(finished(&manager, tool).stage, InstallStage::Succeeded);
    let count = installer.steps.lock().unwrap().len();
    manager.start(tool).unwrap();
    assert_eq!(finished(&manager, tool).stage, InstallStage::Succeeded);
    assert_eq!(installer.steps.lock().unwrap().len(), count);
    *installer.version.lock().unwrap() = "0.7.4".into();
    manager.start(tool).unwrap();
    let incompatible = finished(&manager, tool);
    assert_eq!(incompatible.stage, InstallStage::Succeeded);
    assert_eq!(installer.steps.lock().unwrap().len(), count);
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(!root
        .path()
        .join(".tokenmarket/desktop-connections")
        .exists());
}
#[test]
fn minimax_official_installer_failure_preserves_configuration_without_real_execution() {
    #[cfg(windows)]
    const MODULE_PATH_CHILD: &str = "TOKENMARKET_MINIMAX_MODULE_PATH_CHILD";
    #[cfg(windows)]
    if std::env::var_os(MODULE_PATH_CHILD).is_none() {
        // Windows PowerShell preserves an inherited PSModulePath that omits its
        // own Modules directory, as when pwsh 7 launches a Rust intermediate.
        // Isolate this process environment so parallel tests remain unaffected.
        let root = tempfile::tempdir().unwrap();
        let modules = root.path().join("conflicting modules ' & path");
        std::fs::create_dir_all(&modules).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "minimax_official_installer_failure_preserves_configuration_without_real_execution",
                "--nocapture",
            ])
            .env(MODULE_PATH_CHILD, &modules)
            .env("PSModulePath", &modules)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "conflicting-module-path child failed: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    use tokenmarket_core::installation::{InstallExecutor, InstallStage, InstallationManager};
    struct Corrupt;
    impl InstallExecutor for Corrupt {
        fn execute(
            &self,
            s: &tokenmarket_core::installation::InstallStep,
            c: &std::sync::atomic::AtomicBool,
            o: &mut dyn FnMut(String),
        ) -> anyhow::Result<()> {
            if let Some((_, path)) = &s.download {
                std::fs::create_dir_all(path.parent().unwrap())?;
                std::fs::write(path, b"corrupt fixture archive")?;
                Ok(())
            } else {
                let _ = (s, c);
                o("fixture blocks all real installer execution".into());
                anyhow::bail!("fixture: official installer unavailable")
            }
        }
        fn version(&self, _: &Path) -> anyhow::Result<String> {
            anyhow::bail!("must not probe a missing executable")
        }
    }
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("minimax-code").unwrap();
    // Execute the host's verifier: Windows must exercise PowerShell, not /bin/sh.
    let platform = if cfg!(windows) {
        HostPlatform::Windows
    } else {
        HostPlatform::MacOs
    };
    let manager = InstallationManager::new(
        environment(root.path(), platform),
        std::env::consts::ARCH.into(),
        std::sync::Arc::new(Corrupt),
    );
    manager.start(tool).unwrap();
    // Real PowerShell startup and built-in module loading can exceed the fake
    // executor's three-second budget on a busy native runner.
    let s = finished_with_timeout(&manager, tool, std::time::Duration::from_secs(30));
    assert_eq!(s.stage, InstallStage::Failed);
    assert!(
        s.output
            .iter()
            .any(|s| s.contains("fixture blocks all real installer execution")),
        "{s:?}"
    );
    assert!(!root
        .path()
        .join(".tokenmarket/tools/minimax-code/runtime")
        .exists());
    assert!(!root.path().join(".minimax").exists());
    #[cfg(windows)]
    assert_eq!(
        std::env::var_os("PSModulePath"),
        std::env::var_os(MODULE_PATH_CHILD),
        "the installer's module-path repair must affect only its child process"
    );
}

#[tokio::test]
async fn minimax_active_data_overrides_and_guarded_profile_use_one_effective_configuration() {
    const MARKER: &str = "TOKENMARKET_MINIMAX_PATH_CHILD";
    if let Ok(expected) = std::env::var(MARKER) {
        let root = PathBuf::from(std::env::var_os("HOME").unwrap());
        let expected = PathBuf::from(expected).join("config.yaml");
        let tool = tool_id("minimax-code").unwrap();
        executable(&root.join("bin/mcode"));
        let inventory = ToolInventory::new(environment(&root, HostPlatform::MacOs));
        assert_eq!(inventory.config_path(tool), expected);
        assert_eq!(
            tokenmarket_core::adapters::get_adapter(tool).config_path(),
            expected
        );
        let (url, server) = platform(vec![catalogue()], 2).await;
        let manager = support::manager(root.join("state"), url.clone(), url);
        let connected = manager
            .connect(context(), tool, &inventory, expected.clone())
            .await
            .unwrap();
        assert_eq!(
            read_config(&expected)["custom_provider"]["tokenmarket"]["api"],
            "openai-completions"
        );
        std::env::set_var("MINIMAX_DATA_DIR", root.join("other-active-profile"));
        assert_eq!(
            manager.summaries().unwrap()[0].1.state,
            "configuration_changed"
        );
        assert!(manager
            .test_connection(
                tool,
                &connected.key_id,
                tokenmarket_core::connections::ConnectionTestRequest {
                    confirmed: true,
                    model: Some("alpha.model".into())
                }
            )
            .await
            .unwrap_err()
            .to_string()
            .contains("活动配置"));
        assert_eq!(
            manager
                .disconnect(tool, &connected.key_id, None, Default::default())
                .await
                .unwrap()
                .state,
            "disconnected"
        );
        assert!(!root.join(".minimax-code/config.yaml").exists());
        server.await.unwrap();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let cases = [
        (
            "public",
            vec![
                ("MINIMAX_DATA_DIR", "explicit profile"),
                ("MAVIS_DATA_DIR", "legacy ignored"),
            ],
            "explicit profile",
        ),
        (
            "legacy",
            vec![
                ("MINIMAX_DATA_DIR", " "),
                ("MAVIS_DATA_DIR", "legacy active"),
            ],
            "legacy active",
        ),
        (
            "profile",
            vec![
                ("__MAVIS_ALLOW_LEGACY_RUNTIME_ENV", "1"),
                ("__MAVIS_RUNTIME_PROFILE", "work"),
            ],
            ".minimax-work",
        ),
        (
            "ignored",
            vec![
                ("__MAVIS_RUNTIME_PROFILE", "ignored"),
                ("MCODE_INSTALL_DIR", "install only"),
            ],
            ".minimax",
        ),
    ];
    for (case, vars, expected) in cases {
        let home = root.path().join(case);
        std::fs::create_dir_all(&home).unwrap();
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "minimax_active_data_overrides_and_guarded_profile_use_one_effective_configuration",
                "--nocapture",
            ])
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .env(MARKER, home.join(expected));
        for key in [
            "MINIMAX_DATA_DIR",
            "MAVIS_DATA_DIR",
            "MCODE_INSTALL_DIR",
            "__MAVIS_ALLOW_LEGACY_RUNTIME_ENV",
            "__MAVIS_RUNTIME_DATA_DIR",
            "__MAVIS_RUNTIME_PROFILE",
        ] {
            command.env_remove(key);
        }
        for (key, value) in vars {
            command.env(
                key,
                if key.ends_with("DATA_DIR") && !value.trim().is_empty() {
                    home.join(value).to_string_lossy().into_owned()
                } else {
                    value.to_owned()
                },
            );
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{case}: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[tokio::test]
async fn minimax_detects_a_new_credential_override_before_any_test_and_preserves_it_on_disconnect()
{
    use tokenmarket_core::connections::{ConnectionTestRequest, DisconnectRequest};
    let root = tempfile::tempdir().unwrap();
    let tool = tool_id("minimax-code").unwrap();
    executable(&root.path().join("bin/mcode"));
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    let path = inventory.config_path(tool);
    let (url, server) = platform(vec![catalogue()], 2).await;
    let manager = support::manager(root.path().join("state"), url.clone(), url);
    let connected = manager
        .connect(context(), tool, &inventory, path.clone())
        .await
        .unwrap();
    server.await.unwrap();
    let mut changed = read_config(&path);
    changed["custom_provider"]["tokenmarket"]["options"]["headers"] =
        json!({"Authorization":"user-later"});
    write_config(&path, &changed);
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
        .contains("MiniMax Code"));
    assert_eq!(
        manager
            .disconnect(tool, &connected.key_id, None, DisconnectRequest::default())
            .await
            .unwrap()
            .state,
        "disconnected"
    );
    assert_eq!(
        read_config(&path)["custom_provider"]["tokenmarket"]["options"]["headers"]["Authorization"],
        "user-later"
    );
}
