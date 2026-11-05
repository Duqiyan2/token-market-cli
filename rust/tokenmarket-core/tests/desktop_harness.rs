mod support;
use serde_json::{json, Value};
use tempfile::tempdir;
use tokenmarket_core::{
    connections::ConnectContext,
    inventory::{HostPlatform, InstallationEnvironment, ToolInventory},
    ToolId,
};
async fn platform(
    models: serde_json::Value,
    expected_requests: usize,
) -> (String, tokio::task::JoinHandle<Vec<String>>) {
    platform_catalogues(vec![models], expected_requests).await
}
async fn platform_catalogues(
    catalogues: Vec<Value>,
    expected_requests: usize,
) -> (String, tokio::task::JoinHandle<Vec<String>>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let mut requests = vec![];
        let mut catalogue_index = 0;
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut data = vec![];
            loop {
                let mut chunk = [0; 4096];
                let n = socket.read(&mut chunk).await.unwrap();
                if n == 0 {
                    break;
                }
                data.extend_from_slice(&chunk[..n]);
                if let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&data[..end]);
                    let length: usize = header
                        .lines()
                        .find_map(|line| {
                            line.to_lowercase()
                                .strip_prefix("content-length:")
                                .map(|s| s.trim().parse().unwrap())
                        })
                        .unwrap_or(0);
                    if data.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let request = String::from_utf8(data).unwrap();
            let first = request.lines().next().unwrap().to_string();
            requests.push(first.clone());
            let models = &catalogues[catalogue_index.min(catalogues.len() - 1)];
            let response = if first.starts_with("GET /v1/models ") {
                catalogue_index += 1;
                assert!(request.contains("Bearer sk-fixture"));
                models.clone()
            } else if first.contains("/disable ") {
                json!({"success":true,"data":true})
            } else {
                assert!(request.contains("Bearer desktop-secret"));
                assert!(request.contains("\"workspace_id\":\"42\""));
                json!({"success":true,"data":{"api_key":"sk-fixture","key_id":"9001","owner":{"id":"u1","name":"Ada"},"bound_workspace":{"id":"42","name":"Research"}}})
            };
            let body = response.to_string();
            let status = if first.starts_with("GET ") && models["fail_http"] == true {
                "503 Service Unavailable"
            } else {
                "200 OK"
            };
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            if requests.len() == expected_requests {
                break;
            }
        }
        requests
    });
    (address, task)
}
fn inventory(root: &std::path::Path) -> ToolInventory {
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let executable = bin.join("dsh");
    std::fs::write(&executable, "fixture").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    ToolInventory::new(InstallationEnvironment {
        platform: HostPlatform::MacOs,
        home: root.into(),
        command_directories: vec![bin],
        application_directories: vec![],
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
fn yaml(path: &std::path::Path) -> Value {
    serde_yaml::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
fn model_provider<'a>(profile: &'a Value, id: &str) -> &'a Value {
    profile
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == "llm-pi-ai")
        .unwrap()["config"]["providers"]
        .as_object()
        .unwrap()
        .values()
        .find(|provider| provider["models"][0]["id"] == id)
        .unwrap()
}

#[tokio::test]
async fn harness_connect_imports_every_chat_model_in_catalogue_order_and_preserves_both_documents()
{
    let root = tempdir().unwrap();
    let inventory = inventory(root.path());
    let profile = root.path().join("profiles/desktop/cordis.patch.yml");
    std::fs::create_dir_all(profile.parent().unwrap()).unwrap();
    let credentials = root.path().join(".credentials.yaml");
    let old_profile = json!([{"id":"plugin-other","config":{"keep":true}},{"id":"llm-pi-ai","unknown":"keep","config":{"providers":{"official":{"apiKeyEnv":"OFFICIAL_KEY"},"tokenmarket":{"custom":"keep"}}}}]);
    let old_credentials = json!({"version":1,"refs":{"OFFICIAL_KEY":"private-other"},"records":{"official":{"secret":"keep"}},"custom":"keep"});
    std::fs::write(&profile, serde_yaml::to_string(&old_profile).unwrap()).unwrap();
    std::fs::write(
        &credentials,
        serde_yaml::to_string(&old_credentials).unwrap(),
    )
    .unwrap();
    let catalogue = json!({"data":[{"id":"zeta","model_types":["TEXT_GENERATION"],"context_window_tokens":65536,"max_output_tokens":8192,"input_modalities":["text","image"]},{"id":"responses-only","model_types":["IMAGE_GENERATION"]},{"id":"alpha","display_name":"Alpha","model_types":["TEXT_GENERATION"]},{"id":"missing-types"}]});
    let (url, server) = platform(catalogue.clone(), 2).await;
    let manager = support::manager(
        root.path().join(".tokenmarket/desktop-connections"),
        url.clone(),
        url.clone(),
    );
    let summary = manager
        .connect(
            context(),
            ToolId::DeepSeekHarness,
            &inventory,
            profile.clone(),
        )
        .await
        .unwrap();
    assert_eq!(summary.models, vec!["zeta", "alpha"]);
    let applied = yaml(&profile);
    assert_eq!(applied[0], old_profile[0]);
    assert_eq!(applied[1]["unknown"], "keep");
    assert_eq!(
        applied[1]["config"]["providers"]["official"],
        old_profile[1]["config"]["providers"]["official"]
    );
    assert_eq!(
        applied[1]["config"]["providers"]["tokenmarket"],
        json!({"custom":"keep"})
    );
    let route = model_provider(&applied, "zeta");
    assert_eq!(route["displayName"], "Token Market:zeta");
    assert_eq!(route["models"].as_array().unwrap().len(), 1);
    assert!(route.get("reasoning").is_none());
    let alpha = model_provider(&applied, "alpha");
    assert_eq!(alpha["displayName"], "Token Market:Alpha");
    assert_eq!(alpha["models"], json!([{"id":"alpha","name":"Alpha"}]));
    assert_eq!(route["api"], "openai-completions");
    assert_eq!(route["baseURL"], format!("{url}/v1"));
    assert_eq!(
        route["models"],
        json!([{"id":"zeta","contextWindow":65536,"maxTokens":8192,"input":["text","image"]}])
    );
    assert!(applied[1]["config"].get("model").is_none());
    let mut expected = old_credentials.clone();
    expected["refs"]["TOKEN_MARKET_API_KEY"] = json!("sk-fixture");
    assert_eq!(yaml(&credentials), expected);
    let record = manager.list().unwrap().remove(0);
    assert_eq!(record.protocol, "OPENAI_CHAT_COMPLETIONS");
    assert_eq!(
        record.model_snapshot,
        catalogue["data"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["id"] == "zeta" || m["id"] == "alpha")
            .cloned()
            .collect::<Vec<_>>()
    );
    assert_eq!(record.receipts.len(), 2);
    assert!(record.receipts.iter().all(|r| r.backup_path.is_some()));
    // Read as an independent YAML consumer: serde_json's arbitrary-precision
    // deserializer can otherwise hide its own numeric serialization wrappers.
    let native_profile: serde_yaml::Value =
        serde_yaml::from_slice(&std::fs::read(&profile).unwrap()).unwrap();
    let native_provider = native_profile[1]["config"]["providers"]
        .as_mapping()
        .unwrap()
        .values()
        .find(|provider| provider["models"][0]["id"].as_str() == Some("zeta"))
        .unwrap();
    assert_eq!(
        native_provider["models"][0]["contextWindow"].as_u64(),
        Some(65536)
    );
    assert_eq!(
        native_provider["models"][0]["maxTokens"].as_u64(),
        Some(8192)
    );
    let native_credentials: serde_yaml::Value =
        serde_yaml::from_slice(&std::fs::read(&credentials).unwrap()).unwrap();
    assert_eq!(native_credentials["version"].as_u64(), Some(1));
    assert_eq!(
        server.await.unwrap(),
        vec![
            "POST /api/desktop/connections/keys HTTP/1.1",
            "GET /v1/models HTTP/1.1"
        ]
    );
}

#[tokio::test]
async fn harness_preserves_legacy_credentials_and_completes_known_model_metadata() {
    let root = tempdir().unwrap();
    let inventory = inventory(root.path());
    let profile = root.path().join("cordis.patch.yml");
    let credentials = root.path().join(".credentials.yaml");
    std::fs::write(
        &credentials,
        "OTHER_KEY: keep-secret\nTOKEN_MARKET_API_KEY: old-secret\n",
    )
    .unwrap();
    let (url,server)=platform(json!({"data":[{"id":"custom-one","model_types":["TEXT_GENERATION"]},{"id":"glm-5.3","model_types":["TEXT_GENERATION"],"context_window_tokens":0,"max_output_tokens":0}]}),2).await;
    let manager = support::manager(root.path().join("state"), url.clone(), url);
    let summary = manager
        .connect(
            context(),
            ToolId::DeepSeekHarness,
            &inventory,
            profile.clone(),
        )
        .await
        .unwrap();
    assert_eq!(summary.models, vec!["custom-one", "glm-5.3"]);
    assert_eq!(
        yaml(&credentials),
        json!({"OTHER_KEY":"keep-secret","TOKEN_MARKET_API_KEY":"sk-fixture"})
    );
    let applied = yaml(&profile);
    let models = json!([
        model_provider(&applied, "custom-one")["models"][0],
        model_provider(&applied, "glm-5.3")["models"][0]
    ]);
    assert!(model_provider(&applied, "custom-one")
        .get("reasoning")
        .is_none());
    assert_eq!(model_provider(&applied, "glm-5.3")["reasoning"], "high");
    assert_eq!(models[0], json!({"id":"custom-one"}));
    assert_eq!(models[1]["id"], "glm-5.3");
    assert_eq!(models[1]["contextWindow"], 1_000_000);
    assert_eq!(models[1]["maxTokens"], 65_536);
    assert_eq!(
        models[1]["reasoningEfforts"],
        json!({"low":"low","high":"high","max":"max"})
    );
    assert_eq!(models[1]["compat"]["thinkingFormat"], "openai");
    assert_eq!(
        manager.list().unwrap()[0].model_snapshot[1]["max_output_tokens"],
        131_072
    );
    assert!(manager.list().unwrap()[0].receipts[1]
        .fields
        .iter()
        .any(|f| f.pointer == "/TOKEN_MARKET_API_KEY" && f.before == Some(json!("old-secret"))));
    assert_eq!(server.await.unwrap().len(), 2);
}

#[tokio::test]
async fn harness_writes_per_model_reasoning_and_keeps_server_limits_and_key_visibility() {
    let root = tempdir().unwrap();
    let inventory = inventory(root.path());
    let profile = root.path().join("cordis.patch.yml");
    let (url, server) = platform(json!({"data":[
        {"id":"deepseek-v4.1-flash","model_types":["TEXT_GENERATION"],"context_window_tokens":262144,"max_output_tokens":8192},
        {"id":"kimi-k2.6","model_types":["TEXT_GENERATION"]},
        {"id":"glm-5.3","model_types":["TEXT_GENERATION"],"supports_reasoning":false},
        {"id":"gpt-6.1-sol","model_types":["IMAGE_GENERATION"]}
    ]}), 2).await;
    let manager = support::manager(root.path().join("state"), url.clone(), url);
    let summary = manager
        .connect(
            context(),
            ToolId::DeepSeekHarness,
            &inventory,
            profile.clone(),
        )
        .await
        .unwrap();
    assert_eq!(
        summary.models,
        ["deepseek-v4.1-flash", "kimi-k2.6", "glm-5.3"]
    );
    let applied = yaml(&profile);
    let rows = json!([
        model_provider(&applied, "deepseek-v4.1-flash")["models"][0],
        model_provider(&applied, "kimi-k2.6")["models"][0],
        model_provider(&applied, "glm-5.3")["models"][0]
    ]);
    assert_eq!(
        applied[0]["config"]["providers"].as_object().unwrap().len(),
        3
    );
    assert_eq!(
        model_provider(&applied, "deepseek-v4.1-flash")["reasoning"],
        "high"
    );
    assert_eq!(model_provider(&applied, "kimi-k2.6")["reasoning"], "high");
    assert!(model_provider(&applied, "glm-5.3")
        .get("reasoning")
        .is_none());
    assert_eq!(rows.as_array().unwrap().len(), 3);
    assert_eq!(rows[0]["contextWindow"], 262144);
    assert_eq!(rows[0]["maxTokens"], 8192);
    assert_eq!(
        rows[0]["reasoningEfforts"],
        json!({"off":null,"low":"low","high":"high","max":"max"})
    );
    assert_eq!(rows[0]["compat"]["thinkingFormat"], "deepseek");
    assert_eq!(
        rows[1]["reasoningEfforts"],
        json!({"off":null,"high":"high"})
    );
    assert_eq!(rows[1]["compat"]["supportsReasoningEffort"], false);
    assert_eq!(rows[2]["reasoningEfforts"], false);
    assert!(rows[2].get("compat").is_none());
    assert_eq!(server.await.unwrap().len(), 2);
}

#[tokio::test]
async fn harness_catalogue_failure_keeps_both_files_and_retains_new_key() {
    for catalogue in [
        json!({"data":[]}),
        json!({"data":[{"id":"glm-5.3","model_types":["IMAGE_GENERATION"]},{"id":"unknown"}]}),
        json!({"error":{"message":"unavailable"}}),
        json!({"fail_http":true}),
    ] {
        let root = tempdir().unwrap();
        let inventory = inventory(root.path());
        let profile = root.path().join("cordis.patch.yml");
        let credentials = root.path().join(".credentials.yaml");
        let original_profile = "- id: other\n  config: {keep: true}\n";
        let original_credentials = "version: 1\nrefs: {OTHER_KEY: secret}\nrecords: {}\n";
        std::fs::write(&profile, original_profile).unwrap();
        std::fs::write(&credentials, original_credentials).unwrap();
        let (url, server) = platform(catalogue, 2).await;
        let manager = support::manager(root.path().join("state"), url.clone(), url);
        assert!(manager
            .connect(
                context(),
                ToolId::DeepSeekHarness,
                &inventory,
                profile.clone()
            )
            .await
            .is_err());
        assert_eq!(std::fs::read_to_string(profile).unwrap(), original_profile);
        assert_eq!(
            std::fs::read_to_string(credentials).unwrap(),
            original_credentials
        );
        assert!(manager.list().unwrap().is_empty());
        assert_eq!(
            server.await.unwrap(),
            vec![
                "POST /api/desktop/connections/keys HTTP/1.1",
                "GET /v1/models HTTP/1.1"
            ]
        );
    }
}

#[tokio::test]
async fn malformed_profile_or_credentials_fails_before_issuance() {
    for (profile_text, credentials_text) in [
        ("- [broken", "{}"),
        ("[]", "version: 2\nrefs: {}\nrecords: {}"),
        ("[]", "[broken"),
        ("- id: llm-pi-ai\n- id: llm-pi-ai", "{}"),
    ] {
        let root = tempdir().unwrap();
        let inventory = inventory(root.path());
        let profile = root.path().join("cordis.patch.yml");
        let credentials = root.path().join(".credentials.yaml");
        std::fs::write(&profile, profile_text).unwrap();
        std::fs::write(&credentials, credentials_text).unwrap();
        let manager = support::manager(
            root.path().join("state"),
            "http://127.0.0.1:1".into(),
            "http://127.0.0.1:1".into(),
        );
        let error = manager
            .connect(
                context(),
                ToolId::DeepSeekHarness,
                &inventory,
                profile.clone(),
            )
            .await
            .unwrap_err()
            .to_string();
        assert!(!error.contains("取得接入 Key"));
        assert_eq!(std::fs::read_to_string(profile).unwrap(), profile_text);
        assert_eq!(
            std::fs::read_to_string(credentials).unwrap(),
            credentials_text
        );
        assert!(manager.list().unwrap().is_empty());
    }
}

#[cfg(unix)]
#[tokio::test]
async fn second_file_write_failure_rolls_back_profile_and_retains_credentials() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempdir().unwrap();
    let inventory = inventory(root.path());
    let home = root.path().join("home");
    let profile = home.join("profiles/desktop/cordis.patch.yml");
    std::fs::create_dir_all(profile.parent().unwrap()).unwrap();
    let credentials = home.join(".credentials.yaml");
    let original_profile = "- id: other\n  config: {keep: true}\n";
    let original_credentials = "OTHER_KEY: keep-secret\n";
    std::fs::write(&profile, original_profile).unwrap();
    std::fs::write(&credentials, original_credentials).unwrap();
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o500)).unwrap();
    let (url, server) = platform(
        json!({"data":[{"id":"available","model_types":["TEXT_GENERATION"]}]}),
        2,
    )
    .await;
    let manager = support::manager(root.path().join("state"), url.clone(), url);
    let result = manager
        .connect(
            context(),
            ToolId::DeepSeekHarness,
            &inventory,
            profile.clone(),
        )
        .await;
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(result.is_err());
    assert_eq!(std::fs::read_to_string(profile).unwrap(), original_profile);
    assert_eq!(
        std::fs::read_to_string(credentials).unwrap(),
        original_credentials
    );
    assert!(manager.list().unwrap().is_empty());
    assert_eq!(server.await.unwrap().len(), 2);
}

#[tokio::test]
async fn harness_disconnect_removes_adopted_platform_provider_and_legacy_credential() {
    use tokenmarket_core::connections::DisconnectRequest;
    let root = tempdir().unwrap();
    let inventory = inventory(root.path());
    let profile = root.path().join("cordis.patch.yml");
    let credentials = root.path().join(".credentials.yaml");
    let (url, server) = platform(
        json!({"data":[{"id":"current","model_types":["TEXT_GENERATION"]}]}),
        3,
    )
    .await;
    let original = json!([{"id":"llm-pi-ai","config":{"providers":{"official":{"custom":"keep"},"tokenmarket":{"api":"openai-completions","baseURL":format!("{url}/v1"),"apiKeyEnv":"TOKEN_MARKET_API_KEY","models":[{"id":"legacy"}]}}}}]);
    std::fs::write(&profile, serde_yaml::to_string(&original).unwrap()).unwrap();
    std::fs::write(&credentials,serde_yaml::to_string(&json!({"version":1,"refs":{"TOKEN_MARKET_API_KEY":"legacy-secret","OTHER":"keep"},"records":{}})).unwrap()).unwrap();
    let manager = support::manager(root.path().join("state"), url.clone(), url);
    manager
        .connect(
            context(),
            ToolId::DeepSeekHarness,
            &inventory,
            profile.clone(),
        )
        .await
        .unwrap();
    manager
        .reconnect(
            context(),
            ToolId::DeepSeekHarness,
            "9001",
            &inventory,
            profile.clone(),
        )
        .await
        .unwrap();
    server.await.unwrap();
    let outcome = manager
        .disconnect(
            ToolId::DeepSeekHarness,
            "9001",
            None,
            DisconnectRequest::default(),
        )
        .await
        .unwrap();
    assert_eq!(outcome.state, "disconnected");
    assert_eq!(
        yaml(&profile),
        json!([{"id":"llm-pi-ai","config":{"providers":{"official":{"custom":"keep"}}}}])
    );
    assert_eq!(
        yaml(&credentials),
        json!({"version":1,"refs":{"OTHER":"keep"},"records":{}})
    );
    assert!(manager.list().unwrap().is_empty());
}

#[tokio::test]
async fn harness_disconnect_restores_both_owned_scopes_and_preserves_later_provider_plugin_and_credential_edits(
) {
    use tokenmarket_core::connections::DisconnectRequest;
    let root = tempdir().unwrap();
    let inventory = inventory(root.path());
    let profile = root.path().join("cordis.patch.yml");
    let credentials = root.path().join(".credentials.yaml");
    let original = json!([{"id":"llm-pi-ai","custom":"keep","config":{"providers":{"official":{"key":"other"},"tokenmarket":{"custom":"before"}}}},{"id":"existing-plugin"}]);
    let original_credentials = json!({"version":1,"refs":{"TOKEN_MARKET_API_KEY":null,"OTHER":"secret"},"records":{"official":{"key":"keep"}}});
    std::fs::write(&profile, serde_yaml::to_string(&original).unwrap()).unwrap();
    std::fs::write(
        &credentials,
        serde_yaml::to_string(&original_credentials).unwrap(),
    )
    .unwrap();
    let (url, server) = platform(
        json!({"data":[{"id":"available","model_types":["TEXT_GENERATION"]}]}),
        2,
    )
    .await;
    let manager = support::manager(root.path().join("state"), url.clone(), url);
    manager
        .connect(
            context(),
            ToolId::DeepSeekHarness,
            &inventory,
            profile.clone(),
        )
        .await
        .unwrap();
    server.await.unwrap();
    let mut changed = yaml(&profile);
    changed
        .as_array_mut()
        .unwrap()
        .insert(0, json!({"id":"new-plugin","config":{"manual":true}}));
    changed[1]["config"]["providers"]["tokenmarket"]["manual"] = json!(true);
    changed[1]["config"]["providers"]["new-provider"] = json!({"key":"new-secret"});
    std::fs::write(&profile, serde_yaml::to_string(&changed).unwrap()).unwrap();
    let mut changed_credentials = yaml(&credentials);
    changed_credentials["refs"]["NEW"] = json!("new-secret");
    changed_credentials["records"]["new-provider"] = json!({"key":"new-secret"});
    std::fs::write(
        &credentials,
        serde_yaml::to_string(&changed_credentials).unwrap(),
    )
    .unwrap();
    // Reopen from disk so receipts must preserve null and selector identity through serialization.
    let outcome = manager
        .disconnect(
            ToolId::DeepSeekHarness,
            "9001",
            None,
            DisconnectRequest::default(),
        )
        .await
        .unwrap();
    assert_eq!(outcome.state, "disconnected");
    assert!(manager.list().unwrap().is_empty());
    let mut expected = original.clone();
    expected
        .as_array_mut()
        .unwrap()
        .insert(0, json!({"id":"new-plugin","config":{"manual":true}}));
    expected[1]["config"]["providers"]["tokenmarket"]["manual"] = json!(true);
    expected[1]["config"]["providers"]["new-provider"] = json!({"key":"new-secret"});
    assert_eq!(yaml(&profile), expected);
    let mut expected_credentials = original_credentials;
    expected_credentials["refs"]["NEW"] = json!("new-secret");
    expected_credentials["records"]["new-provider"] = json!({"key":"new-secret"});
    assert_eq!(yaml(&credentials), expected_credentials);
    let native: serde_yaml::Value =
        serde_yaml::from_slice(&std::fs::read(&credentials).unwrap()).unwrap();
    assert_eq!(native["version"].as_u64(), Some(1));
}

#[tokio::test]
async fn harness_disconnect_removes_new_owned_profile_and_versioned_credential_bootstrap() {
    use tokenmarket_core::connections::DisconnectRequest;
    let root = tempdir().unwrap();
    let inventory = inventory(root.path());
    let profile = root.path().join("profiles/desktop/cordis.patch.yml");
    let credentials = root.path().join(".credentials.yaml");
    let (url, server) = platform(
        json!({"data":[{"id":"available","model_types":["TEXT_GENERATION"]}]}),
        2,
    )
    .await;
    let manager = support::manager(root.path().join("state"), url.clone(), url);
    manager
        .connect(
            context(),
            ToolId::DeepSeekHarness,
            &inventory,
            profile.clone(),
        )
        .await
        .unwrap();
    server.await.unwrap();
    assert_eq!(
        yaml(&credentials),
        json!({"version":1,"refs":{"TOKEN_MARKET_API_KEY":"sk-fixture"},"records":{}})
    );
    let outcome = manager
        .disconnect(
            ToolId::DeepSeekHarness,
            "9001",
            None,
            DisconnectRequest::default(),
        )
        .await
        .unwrap();
    assert_eq!(outcome.state, "disconnected");
    assert!(!profile.exists());
    assert!(!credentials.exists());
    assert!(manager.list().unwrap().is_empty());
}

#[tokio::test]
async fn harness_reconnect_retires_unavailable_providers_and_invalid_defaults() {
    use tokenmarket_core::connections::DisconnectRequest;
    let root = tempdir().unwrap();
    let inventory = inventory(root.path());
    let profile = root.path().join("cordis.patch.yml");
    let (url, server) = platform_catalogues(vec![
        json!({"data":[{"id":"glm-5.3","model_types":["TEXT_GENERATION"]},{"id":"deepseek-v4.1-flash","model_types":["TEXT_GENERATION"]}]}),
        json!({"data":[{"id":"glm-5.3","display_name":"Renamed GLM","supports_reasoning":false,"model_types":["TEXT_GENERATION"]}]}),
    ], 3).await;
    let manager = support::manager(root.path().join("state"), url.clone(), url);
    manager
        .connect(
            context(),
            ToolId::DeepSeekHarness,
            &inventory,
            profile.clone(),
        )
        .await
        .unwrap();
    let first = yaml(&profile);
    let id = first[0]["config"]["providers"]
        .as_object()
        .unwrap()
        .iter()
        .find(|(_, p)| p["models"][0]["id"] == "glm-5.3")
        .unwrap()
        .0
        .clone();
    let mut edited = first;
    edited[0]["config"]["providers"][&id]["custom"] = json!("keep");
    std::fs::write(&profile, serde_yaml::to_string(&edited).unwrap()).unwrap();
    manager
        .reconnect(
            context(),
            ToolId::DeepSeekHarness,
            "9001",
            &inventory,
            profile.clone(),
        )
        .await
        .unwrap();
    server.await.unwrap();
    let applied = yaml(&profile);
    let providers = applied[0]["config"]["providers"].as_object().unwrap();
    assert_eq!(providers.len(), 1);
    assert!(
        providers.contains_key(&id),
        "renaming must not change provider identity"
    );
    let glm = model_provider(&applied, "glm-5.3");
    assert_eq!(glm["displayName"], "Token Market:Renamed GLM");
    assert!(glm.get("reasoning").is_none());
    assert_eq!(glm["custom"], "keep");
    let outcome = manager
        .disconnect(
            ToolId::DeepSeekHarness,
            "9001",
            None,
            DisconnectRequest::default(),
        )
        .await
        .unwrap();
    assert_eq!(outcome.state, "disconnected");
    assert_eq!(
        yaml(&profile)[0]["config"]["providers"][&id],
        json!({"custom":"keep"})
    );
    assert!(!root.path().join(".credentials.yaml").exists());
}

#[tokio::test]
async fn harness_reconnect_migrates_previous_shared_provider_receipts() {
    use tokenmarket_core::connections::DisconnectRequest;
    let root = tempdir().unwrap();
    let inventory = inventory(root.path());
    let profile = root.path().join("cordis.patch.yml");
    let original = json!([{"id":"llm-pi-ai","config":{"providers":{"official":{"custom":"keep"},"tokenmarket":{"custom":"before"}}}}]);
    std::fs::write(&profile, serde_yaml::to_string(&original).unwrap()).unwrap();
    let (url, server) = platform(
        json!({"data":[{"id":"glm-5.3","model_types":["TEXT_GENERATION"]}]}),
        3,
    )
    .await;
    let state = root.path().join("state");
    let manager = support::manager(state.clone(), url.clone(), url);
    manager
        .connect(
            context(),
            ToolId::DeepSeekHarness,
            &inventory,
            profile.clone(),
        )
        .await
        .unwrap();
    // Recreate a previous-release connection: one shared route, with its original
    // leaf receipts and the same encrypted connection/backup storage.
    let mut applied = yaml(&profile);
    let providers = applied[0]["config"]["providers"].as_object_mut().unwrap();
    let id = providers
        .iter()
        .find(|(_, p)| p["models"][0]["id"] == "glm-5.3")
        .unwrap()
        .0
        .clone();
    let mut route = providers.remove(&id).unwrap();
    route.as_object_mut().unwrap().remove("displayName");
    route.as_object_mut().unwrap().remove("reasoning");
    route["custom"] = json!("before");
    providers.insert("tokenmarket".into(), route);
    std::fs::write(&profile, serde_yaml::to_string(&applied).unwrap()).unwrap();
    let store = state.join("connections.json");
    let mut records: Value = serde_json::from_slice(&support::read_private(&store)).unwrap();
    let receipt = &mut records[0]["receipts"][0];
    receipt["fields"].as_array_mut().unwrap().retain(|f| {
        !f["pointer"].as_str().unwrap().ends_with("/displayName")
            && !f["pointer"].as_str().unwrap().ends_with("/reasoning")
    });
    for field in receipt["fields"].as_array_mut().unwrap() {
        field["pointer"] = json!(field["pointer"]
            .as_str()
            .unwrap()
            .replace(&id, "tokenmarket"));
    }
    receipt["created_containers"]
        .as_array_mut()
        .unwrap()
        .retain(|v| !v.as_str().unwrap().contains(&id));
    std::fs::write(&store, records.to_string()).unwrap();
    manager
        .reconnect(
            context(),
            ToolId::DeepSeekHarness,
            "9001",
            &inventory,
            profile.clone(),
        )
        .await
        .unwrap();
    server.await.unwrap();
    let applied = yaml(&profile);
    assert_eq!(
        applied[0]["config"]["providers"]["tokenmarket"],
        json!({"custom":"before"})
    );
    assert_eq!(model_provider(&applied, "glm-5.3")["reasoning"], "high");
    assert_eq!(
        applied[0]["config"]["providers"].as_object().unwrap().len(),
        3
    );
    let outcome = manager
        .disconnect(
            ToolId::DeepSeekHarness,
            "9001",
            None,
            DisconnectRequest::default(),
        )
        .await
        .unwrap();
    assert_eq!(outcome.state, "disconnected");
    assert_eq!(yaml(&profile), original);
}
