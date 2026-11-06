mod support;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tokenmarket_core::{
    adapters::tool_id,
    connections::{
        ConflictResolution, ConnectContext, ConnectionTestRequest, DisconnectRequest, RestoreChoice,
    },
    inventory::{HostPlatform, InstallationEnvironment, ToolInventory},
    ToolId,
};

struct FixtureRuntime;
struct IncompatibleRuntime;
struct UpgradedFixtureRuntime;
impl tokenmarket_core::qwenwork_runtime::RuntimeInstaller for UpgradedFixtureRuntime {
    fn prepare(
        &self,
        application: &Path,
        database: &Path,
        gateway: &str,
    ) -> anyhow::Result<tokenmarket_core::qwenwork_runtime::RuntimeInstallation> {
        let previous = tokenmarket_core::qwenwork_runtime::RuntimeInstaller::prepare(
            &FixtureRuntime,
            application,
            database,
            gateway,
        )?;
        let worker = previous.worker.with_file_name("upgraded-worker.mjs");
        std::fs::write(&worker, "// inert upgraded runtime fixture")?;
        Ok(tokenmarket_core::qwenwork_runtime::RuntimeInstallation { worker })
    }
}
impl tokenmarket_core::qwenwork_runtime::RuntimeInstaller for IncompatibleRuntime {
    fn prepare(
        &self,
        _: &Path,
        _: &Path,
        _: &str,
    ) -> anyhow::Result<tokenmarket_core::qwenwork_runtime::RuntimeInstallation> {
        anyhow::bail!(tokenmarket_core::qwenwork_runtime::ADAPTATION_REQUIRED)
    }
}
impl tokenmarket_core::qwenwork_runtime::RuntimeInstaller for FixtureRuntime {
    fn prepare(
        &self,
        application: &Path,
        _: &Path,
        _: &str,
    ) -> anyhow::Result<tokenmarket_core::qwenwork_runtime::RuntimeInstallation> {
        let worker = application
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join(".qwenworkcn/fixture-worker.mjs");
        std::fs::write(&worker, "// inert runtime fixture, never executed")?;
        Ok(tokenmarket_core::qwenwork_runtime::RuntimeInstallation { worker })
    }
}
fn qwen_manager(
    directory: PathBuf,
    platform: String,
    gateway: String,
) -> tokenmarket_core::connections::ConnectionManager {
    support::manager(directory, platform, gateway)
        .with_qwenwork_runtime(std::sync::Arc::new(FixtureRuntime))
}

fn environment(root: &Path, platform: HostPlatform) -> InstallationEnvironment {
    InstallationEnvironment {
        platform,
        home: root.into(),
        command_directories: vec![root.join("bin")],
        application_directories: vec![root.join("Applications")],
        executable_extensions: vec![".EXE".into()],
    }
}
fn fixture(root: &Path) -> (ToolInventory, PathBuf, PathBuf) {
    let app = root.join("Applications/QwenWorkCN.app/Contents");
    std::fs::create_dir_all(app.join("MacOS")).unwrap();
    std::fs::write(app.join("MacOS/QwenWorkCN"), "inert fixture").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            app.join("MacOS/QwenWorkCN"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }
    std::fs::write(app.join("Info.plist"),r#"<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleExecutable</key><string>QwenWorkCN</string><key>CFBundleIdentifier</key><string>cn.qwenwork.desktop.mac</string><key>CFBundleShortVersionString</key><string>1.5.0</string></dict></plist>"#).unwrap();
    let inventory = ToolInventory::new(environment(root, HostPlatform::MacOs));
    let path = inventory.config_path(ToolId::QwenWork);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch(include_str!("fixtures/qwenwork-schema.sql"))
        .unwrap();
    db.execute("INSERT INTO byok_custom_models(key,display_name,model,encrypted_parameters) VALUES ('byok-user','User model','user/local','user-encrypted-secret')",[]).unwrap();
    db.execute(
        "INSERT INTO app_settings VALUES ('selected-model','user-default')",
        [],
    )
    .unwrap();
    std::fs::create_dir_all(root.join(".qwenworkcn")).unwrap();
    std::fs::write(
        root.join(".qwenworkcn/.status.json"),
        json!({"logged_in":true,"version":"1.5.0","product":"qwenworkcn"}).to_string(),
    )
    .unwrap();
    let env = root.join(".qwenworkcn/.env");
    (inventory, path, env)
}
fn model_count(path: &Path) -> i64 {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row("SELECT count(*) FROM byok_custom_models", [], |r| r.get(0))
        .unwrap()
}
async fn platform(
    catalogues: Vec<Value>,
    expected: usize,
) -> (String, tokio::task::JoinHandle<Vec<String>>) {
    platform_mode(catalogues, expected, "sk-qwenwork-fixture", false).await
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

#[tokio::test]
async fn qwenwork_auxiliary_model_survives_catalogue_reordering_and_growth() {
    let root = tempfile::tempdir().unwrap();
    let (inventory, path, env) = fixture(root.path());
    let mut reordered = catalogue();
    reordered["data"].as_array_mut().unwrap().reverse();
    reordered["data"].as_array_mut().unwrap().insert(
        0,
        json!({"id":"a-new-model","model_types":["TEXT_GENERATION"]}),
    );
    let (url, server) = platform(vec![catalogue(), reordered.clone(), reordered], 4).await;
    let manager = qwen_manager(root.path().join("state"), url.clone(), url);
    let connected = manager
        .connect(context(), ToolId::QwenWork, &inventory, path.clone())
        .await
        .unwrap();
    let route = || {
        let text = std::fs::read_to_string(&env).unwrap();
        let file = text
            .lines()
            .find_map(|line| line.strip_prefix("TOKENMARKET_QWENWORK_ROUTE="))
            .unwrap();
        serde_json::from_slice::<Value>(&std::fs::read(file).unwrap()).unwrap()
    };
    assert_eq!(
        route()["default_model"],
        "alpha.model",
        "API catalogue order must not choose the auxiliary model"
    );
    manager
        .reconnect(
            context(),
            ToolId::QwenWork,
            &connected.key_id,
            &inventory,
            path.clone(),
        )
        .await
        .unwrap();
    assert_eq!(
        route()["default_model"],
        "alpha.model",
        "an existing available auxiliary model must survive catalogue growth"
    );
    let manager = manager.with_qwenwork_runtime(std::sync::Arc::new(UpgradedFixtureRuntime));
    manager
        .reconnect(
            context(),
            ToolId::QwenWork,
            &connected.key_id,
            &inventory,
            path,
        )
        .await
        .unwrap();
    assert_eq!(
        route()["default_model"],
        "alpha.model",
        "SDK/runtime upgrades must preserve the available auxiliary model"
    );
    server.await.unwrap();
}

#[tokio::test]
async fn qwenwork_connection_never_writes_the_platform_key_into_tool_configuration() {
    let root = tempfile::tempdir().unwrap();
    let (inventory, path, env) = fixture(root.path());
    let (url, server) = platform(vec![catalogue()], 2).await;
    let manager = qwen_manager(root.path().join("state"), url.clone(), url);
    manager
        .connect(context(), ToolId::QwenWork, &inventory, path.clone())
        .await
        .unwrap();
    server.await.unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    let mut stmt = db.prepare("SELECT encrypted_parameters,extra_params,url FROM byok_custom_models WHERE key LIKE 'byok-tokenmarket-%'").unwrap();
    let values = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .unwrap();
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode("sk-qwenwork-fixture");
    for value in values {
        let (parameters, extra, endpoint) = value.unwrap();
        assert!(
            !parameters.contains(&encoded),
            "platform key was persisted in the tool database"
        );
        assert!(!parameters.contains("sk-qwenwork-fixture"));
        assert!(!extra.contains("sk-qwenwork-fixture"));
        assert!(endpoint.contains("tokenmarket.local.invalid"));
    }
    assert!(!std::fs::read_to_string(env)
        .unwrap()
        .contains("sk-qwenwork-fixture"));
    assert_eq!(
        manager.list().unwrap()[0].credential(),
        "sk-qwenwork-fixture"
    );
}

#[tokio::test]
async fn qwenwork_migrates_legacy_key_rows_and_never_restores_them_on_disconnect() {
    use base64::Engine;
    for runtime in [
        "tokenmarket-direct-sdk",
        "tokenmarket-local-sdk-v2/previous",
    ] {
        let root = tempfile::tempdir().unwrap();
        let (inventory, path, env) = fixture(root.path());
        let legacy_worker = root.path().join(format!(
            ".qwenworkcn/{runtime}/dist/_worker/qoder-worker-runtime.obf.mjs"
        ));
        std::fs::write(
            &env,
            format!("QW_QODER_WORKER_RUNTIME_PATH={}\n", legacy_worker.display()),
        )
        .unwrap();
        let (url, server) = platform(vec![catalogue()], 2).await;
        let encoded = base64::engine::general_purpose::STANDARD.encode("sk-qwenwork-fixture");
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute("INSERT INTO byok_custom_models(key,display_name,model,url,provider,style,encrypted_parameters,extra_params) VALUES (?1,'Legacy model','old-model',?2,'openai','openai',?3,'{\"tokenmarket\":1}')",
        rusqlite::params!["byok-tokenmarket-legacy",format!("{url}/v1/chat/completions"),json!({"api_key":{"encoding":"plainBase64","value":encoded}}).to_string()]).unwrap();
        drop(db);
        let manager = qwen_manager(root.path().join("state"), url.clone(), url);
        let connected = manager
            .connect(context(), ToolId::QwenWork, &inventory, path.clone())
            .await
            .unwrap();
        server.await.unwrap();
        assert_eq!(model_count(&path), 3);
        assert!(!std::fs::read(&path)
            .unwrap()
            .windows(encoded.len())
            .any(|v| v == encoded.as_bytes()));
        manager
            .disconnect(
                ToolId::QwenWork,
                &connected.key_id,
                None,
                DisconnectRequest::default(),
            )
            .await
            .unwrap();
        assert_eq!(model_count(&path), 1);
        assert!(!std::fs::read_to_string(env)
            .unwrap()
            .contains("QW_QODER_WORKER_RUNTIME_PATH="));
        assert!(!std::fs::read(&path)
            .unwrap()
            .windows(encoded.len())
            .any(|v| v == encoded.as_bytes()));
    }
}

#[tokio::test]
async fn qwenwork_changed_sdk_fails_before_key_issuance_or_configuration_changes() {
    let root = tempfile::tempdir().unwrap();
    let (inventory, path, env) = fixture(root.path());
    let before = std::fs::read(&path).unwrap();
    let manager = support::manager(
        root.path().join("state"),
        "https://console.invalid".into(),
        "https://gateway.invalid".into(),
    )
    .with_qwenwork_runtime(std::sync::Arc::new(IncompatibleRuntime));
    let error = manager
        .connect(context(), ToolId::QwenWork, &inventory, path.clone())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("联系管理员适配"));
    assert_eq!(std::fs::read(path).unwrap(), before);
    assert!(!env.exists());
    assert!(manager.list().unwrap().is_empty());
    assert!(!root.path().join("state/issuances").exists());
}

#[tokio::test]
async fn qwenwork_failed_reconnect_keeps_the_edited_configuration_and_saved_key() {
    for sdk_failure in [true, false] {
        let root = tempfile::tempdir().unwrap();
        let (inventory, path, env) = fixture(root.path());
        let catalogues = if sdk_failure {
            vec![catalogue()]
        } else {
            vec![catalogue(), json!({"data":[]})]
        };
        let (url, server) = platform(catalogues, if sdk_failure { 2 } else { 3 }).await;
        let manager = qwen_manager(root.path().join("state"), url.clone(), url);
        let connected = manager
            .connect(context(), ToolId::QwenWork, &inventory, path.clone())
            .await
            .unwrap();
        std::fs::write(&env, "ALLOW_CUSTOM_BYOK=0\nUSER_SETTING=keep\n").unwrap();
        rusqlite::Connection::open(&path).unwrap().execute("UPDATE byok_custom_models SET url='https://edited.invalid' WHERE key LIKE 'byok-tokenmarket-%'",[]).unwrap();
        let before_db = std::fs::read(&path).unwrap();
        let before_env = std::fs::read(&env).unwrap();
        let before_record = std::fs::read(root.path().join("state/connections.json")).unwrap();
        let manager = if sdk_failure {
            manager.with_qwenwork_runtime(std::sync::Arc::new(IncompatibleRuntime))
        } else {
            manager
        };
        let error = manager
            .reconnect(
                context(),
                ToolId::QwenWork,
                &connected.key_id,
                &inventory,
                path.clone(),
            )
            .await
            .unwrap_err();
        if sdk_failure {
            assert!(error.to_string().contains("联系管理员适配"));
        }
        server.await.unwrap();
        assert_eq!(std::fs::read(path).unwrap(), before_db);
        assert_eq!(std::fs::read(env).unwrap(), before_env);
        assert_eq!(
            std::fs::read(root.path().join("state/connections.json")).unwrap(),
            before_record
        );
        assert_eq!(
            manager.list().unwrap()[0].credential(),
            "sk-qwenwork-fixture"
        );
    }
}

#[test]
fn qwenwork_discovers_only_official_app_and_keeps_platform_config_paths() {
    let root = tempfile::tempdir().unwrap();
    assert_eq!(tool_id("qwenwork").unwrap(), ToolId::QwenWork);
    assert_eq!(serde_json::to_value(ToolId::QwenWork).unwrap(), "qwenwork");
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    std::fs::create_dir_all(root.path().join("bin")).unwrap();
    std::fs::write(root.path().join("bin/qwenwork"), "inert standalone CLI").unwrap();
    assert!(!inventory.is_installed(ToolId::QwenWork));
    let (inventory, path, _) = fixture(root.path());
    assert!(inventory.is_installed(ToolId::QwenWork));
    assert_eq!(
        path,
        root.path()
            .join("Library/Application Support/QwenWorkCN/data/agents.db")
    );
    assert!(inventory
        .terminal_launch_plan(ToolId::QwenWork)
        .unwrap()
        .args
        .iter()
        .any(|s| s.ends_with("QwenWorkCN.app")));
    let windows = ToolInventory::new(environment(root.path(), HostPlatform::Windows));
    assert_eq!(
        windows.config_path(ToolId::QwenWork),
        root.path()
            .join("AppData/Roaming/QwenWorkCN/data/agents.db")
    );
    assert!(inventory.connection_guard(ToolId::QwenWork).is_ok());
    let installer = tokenmarket_core::installation::InstallationManager::new(
        environment(root.path(), HostPlatform::MacOs),
        "aarch64".into(),
        std::sync::Arc::new(tokenmarket_core::installation::NativeInstallExecutor),
    );
    let update =
        installer.inspect_update(ToolId::QwenWork, Err(anyhow::anyhow!("no verified feed")));
    assert_eq!(update.current_version.as_deref(), Some("1.5.0"));
    assert_eq!(update.state, "check_failed");
    assert!(!update.can_upgrade);
    assert!(!update.manual_upgrade);
    let update = installer.inspect_update(ToolId::QwenWork, Ok("latest".into()));
    assert!(update.can_upgrade);
    assert_eq!(update.target_version.as_deref(), Some("latest"));
    assert!(update.latest_version.is_none());
    assert!(!update.note.contains("重新接入"));
    assert!(!update.note.contains("修复配置"));
    assert!(!update.note.contains("1.5.0"));

    std::fs::write(
        path.with_file_name("agents.identity-v2.db"),
        "identity database",
    )
    .unwrap();
    assert!(inventory
        .connection_guard(ToolId::QwenWork)
        .err()
        .unwrap()
        .to_string()
        .contains("独立身份"));
}

#[tokio::test]
async fn qwenwork_connect_test_and_restore_preserve_other_models_settings_and_env() {
    let root = tempfile::tempdir().unwrap();
    let (inventory, path, env) = fixture(root.path());
    let initial_env = "# local settings\r\nOTHER='keep'\r\nENGINE_MAAS_MINT_URL=https://user.invalid/mint\r\nQODERCN_SERVER_ENDPOINT=https://user.invalid\r\nALLOW_CUSTOM_BYOK=0\r\n";
    std::fs::write(&env, initial_env).unwrap();
    let (url, server) = platform(vec![catalogue()], 3).await;
    let manager = qwen_manager(root.path().join("state"), url.clone(), url.clone());
    let connected = manager
        .connect(context(), ToolId::QwenWork, &inventory, path.clone())
        .await
        .unwrap();
    assert_eq!(connected.models, vec!["vendor/zeta~1", "alpha.model"]);
    assert_eq!(model_count(&path), 3);
    let updated_env = std::fs::read_to_string(&env).unwrap();
    assert!(updated_env.starts_with("# local settings\r\nOTHER='keep'\r\n"));
    assert!(updated_env.contains("ALLOW_CUSTOM_BYOK=1\r\n"));
    assert!(updated_env.contains("QW_QODER_WORKER_RUNTIME_PATH="));
    assert!(
        updated_env.contains("ENGINE_MAAS_MINT_URL=http://127.0.0.1:"),
        "desktop auxiliary models must use the local credential and model relay"
    );
    assert!(updated_env.contains("TOKENMARKET_QWENWORK_ROUTE="));
    assert!(updated_env.contains("QODERCN_SERVER_ENDPOINT=https://tokenmarket.local.invalid"));
    let db = rusqlite::Connection::open(&path).unwrap();
    let (endpoint,parameters,limit):(String,String,i64)=db.query_row("SELECT url,encrypted_parameters,max_input_tokens FROM byok_custom_models WHERE model='vendor/zeta~1'",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
    assert_eq!(endpoint, tokenmarket_core::qwenwork_runtime::MODEL_ENDPOINT);
    assert_eq!(limit, 65536);
    let encoded: Value = serde_json::from_str(&parameters).unwrap();
    use base64::Engine;
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(encoded["api_key"]["value"].as_str().unwrap())
            .unwrap(),
        tokenmarket_core::qwenwork_runtime::placeholder("sk-qwenwork-fixture").as_bytes()
    );
    assert_eq!(
        manager.summaries().unwrap()[0].1.state,
        "configured_untested"
    );
    let tested = manager
        .test_connection(
            ToolId::QwenWork,
            &connected.key_id,
            ConnectionTestRequest {
                confirmed: true,
                model: Some("vendor/zeta~1".into()),
            },
        )
        .await
        .unwrap();
    assert!(tested.passed);
    assert!(!tested.streaming_verified);
    assert!(!tested.tool_call_verified);
    let requests = server.await.unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.starts_with("POST /v1/chat/completions"))
            .count(),
        1
    );
    db.execute("INSERT INTO byok_custom_models(key,display_name,model) VALUES ('byok-new-user','New user','new-user')",[]).unwrap();
    db.execute("UPDATE app_settings SET value='edited-user-default'", [])
        .unwrap();
    assert_eq!(
        manager
            .disconnect(
                ToolId::QwenWork,
                &connected.key_id,
                None,
                DisconnectRequest::default()
            )
            .await
            .unwrap()
            .state,
        "disconnected"
    );
    assert_eq!(model_count(&path), 2);
    assert_eq!(
        db.query_row("SELECT value FROM app_settings", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "edited-user-default"
    );
    assert_eq!(
        db.query_row(
            "SELECT encrypted_parameters FROM byok_custom_models WHERE key='byok-user'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "user-encrypted-secret"
    );
    assert_eq!(std::fs::read_to_string(env).unwrap(), initial_env);
}

#[tokio::test]
async fn qwenwork_reconnect_after_upgrade_retires_removed_models_and_restores_first_baseline() {
    let root = tempfile::tempdir().unwrap();
    let (inventory, path, env) = fixture(root.path());
    let (url, server) = platform(
        vec![
            catalogue(),
            json!({"data":[{"id":"replacement","model_types":["TEXT_GENERATION"]}]}),
        ],
        3,
    )
    .await;
    let manager = qwen_manager(root.path().join("state"), url.clone(), url);
    let connected = manager
        .connect(context(), ToolId::QwenWork, &inventory, path.clone())
        .await
        .unwrap();
    let app_info = root
        .path()
        .join("Applications/QwenWorkCN.app/Contents/Info.plist");
    let info = std::fs::read_to_string(&app_info)
        .unwrap()
        .replace("1.5.0", "1.6.0");
    std::fs::write(app_info, info).unwrap();
    std::fs::write(
        root.path().join(".qwenworkcn/.status.json"),
        json!({"logged_in":true,"version":"1.6.0","product":"qwenworkcn"}).to_string(),
    )
    .unwrap();
    let reconnected = manager
        .reconnect(
            context(),
            ToolId::QwenWork,
            &connected.key_id,
            &inventory,
            path.clone(),
        )
        .await
        .unwrap();
    assert!(reconnected.key_reused);
    assert_eq!(reconnected.connection.models, vec!["replacement"]);
    assert_eq!(model_count(&path), 2);
    server.await.unwrap();
    std::fs::write(&env, "ALLOW_CUSTOM_BYOK=1\n# user-added comment\n").unwrap();
    manager
        .disconnect(
            ToolId::QwenWork,
            &connected.key_id,
            None,
            DisconnectRequest::default(),
        )
        .await
        .unwrap();
    assert_eq!(model_count(&path), 1);
    assert_eq!(
        std::fs::read_to_string(env).unwrap(),
        "# user-added comment\n"
    );
}

#[tokio::test]
async fn qwenwork_reconnect_repairs_missing_configuration_after_upgrade_downgrade_or_reinstall() {
    for version in ["1.6.0", "1.4.0", "1.5.0"] {
        let root = tempfile::tempdir().unwrap();
        let (inventory, path, env) = fixture(root.path());
        std::fs::write(
            &env,
            "ALLOW_CUSTOM_BYOK=0\nQW_QODER_WORKER_RUNTIME_PATH=/previous/custom-worker.mjs\n",
        )
        .unwrap();
        let (url, server) = platform(vec![catalogue(), catalogue()], 3).await;
        let manager = qwen_manager(root.path().join("state"), url.clone(), url);
        let connected = manager
            .connect(context(), ToolId::QwenWork, &inventory, path.clone())
            .await
            .unwrap();
        rusqlite::Connection::open(&path)
            .unwrap()
            .execute(
                "DELETE FROM byok_custom_models WHERE key LIKE 'byok-tokenmarket-%'",
                [],
            )
            .unwrap();
        let worker = root.path().join(".qwenworkcn/fixture-worker.mjs");
        std::fs::remove_file(&worker).unwrap();
        if version == "1.5.0" {
            std::fs::remove_file(&env).unwrap();
        } else {
            std::fs::write(&env, "# current installation\nUSER_SETTING=keep\n").unwrap();
        }
        let info = root
            .path()
            .join("Applications/QwenWorkCN.app/Contents/Info.plist");
        std::fs::write(
            &info,
            std::fs::read_to_string(&info)
                .unwrap()
                .replace("1.5.0", version),
        )
        .unwrap();
        let result = manager
            .reconnect(
                context(),
                ToolId::QwenWork,
                &connected.key_id,
                &inventory,
                path.clone(),
            )
            .await
            .unwrap();
        assert!(result.key_reused);
        assert_eq!(result.connection.key_id, connected.key_id);
        assert_eq!(model_count(&path), 3);
        assert!(worker.is_file());
        assert!(std::fs::read_to_string(&env)
            .unwrap()
            .contains("QW_QODER_WORKER_RUNTIME_PATH="));
        server.await.unwrap();
        manager
            .disconnect(
                ToolId::QwenWork,
                &connected.key_id,
                None,
                DisconnectRequest::default(),
            )
            .await
            .unwrap();
        assert_eq!(model_count(&path), 1);
        if version == "1.5.0" {
            assert!(!env.exists());
        } else {
            assert_eq!(
                std::fs::read_to_string(&env).unwrap(),
                "# current installation\nUSER_SETTING=keep\n"
            );
        }
    }
}

#[tokio::test]
async fn qwenwork_reconnect_replaces_present_managed_edits_and_preserves_other_settings() {
    let root = tempfile::tempdir().unwrap();
    let (inventory, path, env) = fixture(root.path());
    let (url, server) = platform(vec![catalogue(), catalogue()], 3).await;
    let manager = qwen_manager(root.path().join("state"), url.clone(), url);
    let connected = manager
        .connect(context(), ToolId::QwenWork, &inventory, path.clone())
        .await
        .unwrap();
    let changed="ALLOW_CUSTOM_BYOK=0\nQW_QODER_WORKER_RUNTIME_PATH=/user/custom-worker.mjs\nUSER_SETTING=keep\n";
    std::fs::write(&env, changed).unwrap();
    rusqlite::Connection::open(&path).unwrap().execute("UPDATE byok_custom_models SET url='https://user.invalid/edited', extra_params='invalid JSON',encrypted_parameters='edited' WHERE key LIKE 'byok-tokenmarket-%'",[]).unwrap();
    let result = manager
        .reconnect(
            context(),
            ToolId::QwenWork,
            &connected.key_id,
            &inventory,
            path.clone(),
        )
        .await
        .unwrap();
    assert!(result.key_reused);
    server.await.unwrap();
    let current = std::fs::read_to_string(env).unwrap();
    assert!(current.contains("ALLOW_CUSTOM_BYOK=1"));
    assert!(current.contains("USER_SETTING=keep"));
    assert!(!current.contains("/user/custom-worker.mjs"));
    assert_eq!(model_count(&path), 3);
    let db = rusqlite::Connection::open(path).unwrap();
    assert_eq!(db.query_row("SELECT count(*) FROM byok_custom_models WHERE key LIKE 'byok-tokenmarket-%' AND url=?1",[tokenmarket_core::qwenwork_runtime::MODEL_ENDPOINT],|r|r.get::<_,i64>(0)).unwrap(),2);
    assert_eq!(
        db.query_row(
            "SELECT encrypted_parameters FROM byok_custom_models WHERE key='byok-user'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "user-encrypted-secret"
    );
}

#[tokio::test]
async fn qwenwork_model_edits_block_traffic_and_require_explicit_restore_choice() {
    let root = tempfile::tempdir().unwrap();
    let (inventory, path, _) = fixture(root.path());
    let (url, server) = platform(vec![catalogue()], 2).await;
    let manager = qwen_manager(root.path().join("state"), url.clone(), url);
    let connected = manager
        .connect(context(), ToolId::QwenWork, &inventory, path.clone())
        .await
        .unwrap();
    server.await.unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute("UPDATE byok_custom_models SET url='https://user.example/changed' WHERE model='alpha.model'",[]).unwrap();
    assert_eq!(
        manager.summaries().unwrap()[0].1.state,
        "configuration_changed"
    );
    assert!(manager
        .test_connection(
            ToolId::QwenWork,
            &connected.key_id,
            ConnectionTestRequest {
                confirmed: true,
                model: Some("alpha.model".into())
            }
        )
        .await
        .is_err());
    let preview = manager
        .disconnect(
            ToolId::QwenWork,
            &connected.key_id,
            None,
            DisconnectRequest::default(),
        )
        .await
        .unwrap();
    assert_eq!(preview.state, "conflicts");
    assert_eq!(model_count(&path), 3);
    let resolutions = preview
        .conflicts
        .iter()
        .map(|c| ConflictResolution {
            file: c.file,
            field: c.field,
            choice: RestoreChoice::RestoreOriginal,
        })
        .collect();
    let result = manager
        .disconnect(
            ToolId::QwenWork,
            &connected.key_id,
            None,
            DisconnectRequest {
                revision: preview.revision,
                resolutions,
            },
        )
        .await
        .unwrap();
    assert_eq!(result.state, "disconnected");
    assert_eq!(model_count(&path), 1);
}

#[tokio::test]
async fn qwenwork_failed_connect_records_stage_without_credentials() {
    let root = tempfile::tempdir().unwrap();
    let (inventory, path, _) = fixture(root.path());
    std::fs::remove_file(root.path().join(".qwenworkcn/.status.json")).unwrap();
    let state = root.path().join("state");
    let manager = qwen_manager(
        state.clone(),
        "http://127.0.0.1:1".into(),
        "http://127.0.0.1:1".into(),
    );
    let error = manager
        .connect(context(), ToolId::QwenWork, &inventory, path)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("登录"));
    let log = std::fs::read_to_string(state.join("logs/connections.jsonl"))
        .expect("failed connection must leave a persistent diagnostic");
    let events: Vec<Value> = log
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let failed = events
        .iter()
        .find(|event| event["status"] == "failed")
        .unwrap();
    assert_eq!(failed["tool"], "qwenwork");
    assert_eq!(failed["stage"], "tool_readiness");
    assert_eq!(failed["error"]["reason"], "login_required");
    assert_eq!(failed["error"]["io_kind"], "NotFound");
    for secret in [
        "desktop-secret",
        "user-encrypted-secret",
        "\"u1\"",
        "Research",
    ] {
        assert!(
            !log.contains(secret),
            "diagnostics leaked a credential or identity"
        );
    }
    assert!(manager.list().unwrap().is_empty());
}

#[tokio::test]
async fn qwenwork_preflight_does_not_issue_keys_for_invalid_login_or_database() {
    for (case, expected) in [
        ("logged_out", "完成登录"),
        ("wrong_product", "完成登录"),
        ("uninitialized", "初始化"),
        ("changed_schema", "数据库结构已改变"),
    ] {
        let root = tempfile::tempdir().unwrap();
        let (inventory, path, env) = fixture(root.path());
        match case {
            "logged_out" | "wrong_product" => std::fs::write(
                root.path().join(".qwenworkcn/.status.json"),
                json!({"logged_in":case != "logged_out","version":"1.6.0",
                    "product":if case == "wrong_product" { "other" } else { "qwenworkcn" }})
                .to_string(),
            )
            .unwrap(),
            "uninitialized" => std::fs::remove_file(&path).unwrap(),
            "changed_schema" => rusqlite::Connection::open(&path)
                .unwrap()
                .execute_batch(
                    "ALTER TABLE byok_custom_models RENAME COLUMN model TO incompatible_model",
                )
                .unwrap(),
            _ => unreachable!(),
        }
        let manager = qwen_manager(
            root.path().join("state"),
            "http://127.0.0.1:1".into(),
            "http://127.0.0.1:1".into(),
        );
        let error = manager
            .connect(context(), ToolId::QwenWork, &inventory, path.clone())
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{case}: {error}");
        if case == "uninitialized" {
            assert!(!path.exists());
        } else {
            assert_eq!(model_count(&path), 1);
        }
        assert!(!env.exists());
        assert!(manager.list().unwrap().is_empty());
    }
}

#[tokio::test]
async fn qwenwork_keep_current_preserves_the_edited_model_and_keeps_byok_visible() {
    let root = tempfile::tempdir().unwrap();
    let (inventory, path, _) = fixture(root.path());
    let (url, server) = platform(vec![catalogue()], 2).await;
    let manager = qwen_manager(root.path().join("state"), url.clone(), url);
    let connected = manager
        .connect(context(), ToolId::QwenWork, &inventory, path.clone())
        .await
        .unwrap();
    server.await.unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute("UPDATE byok_custom_models SET url='https://user.example/changed' WHERE model='alpha.model'",[]).unwrap();
    assert_eq!(
        manager.summaries().unwrap()[0].1.state,
        "configuration_changed"
    );
    assert!(manager
        .test_connection(
            ToolId::QwenWork,
            &connected.key_id,
            ConnectionTestRequest {
                confirmed: true,
                model: Some("alpha.model".into())
            }
        )
        .await
        .is_err());
    let preview = manager
        .disconnect(
            ToolId::QwenWork,
            &connected.key_id,
            None,
            DisconnectRequest::default(),
        )
        .await
        .unwrap();
    assert_eq!(preview.state, "conflicts");
    assert_eq!(model_count(&path), 3);
    let resolutions = preview
        .conflicts
        .iter()
        .map(|c| ConflictResolution {
            file: c.file,
            field: c.field,
            choice: RestoreChoice::KeepCurrent,
        })
        .collect();
    let result = manager
        .disconnect(
            ToolId::QwenWork,
            &connected.key_id,
            None,
            DisconnectRequest {
                revision: preview.revision,
                resolutions,
            },
        )
        .await
        .unwrap();
    assert_eq!(result.state, "disconnected");
    assert_eq!(model_count(&path), 2);
    assert_eq!(
        std::fs::read_to_string(root.path().join(".qwenworkcn/.env")).unwrap(),
        "ALLOW_CUSTOM_BYOK=1\n"
    );
}
