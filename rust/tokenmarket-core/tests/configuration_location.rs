mod support;
use serde_json::{json, Value};
use std::path::PathBuf;
use tokenmarket_core::{
    adapters::tool_id,
    connections::{ConnectContext, ConnectionTestRequest, DisconnectRequest},
    inventory::{HostPlatform, InstallationEnvironment, ToolInventory},
};
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

fn context() -> ConnectContext {
    ConnectContext {
        session_token: "desktop-secret".into(),
        owner_id: "u1".into(),
        workspace_id: "42".into(),
    }
}

#[tokio::test]
async fn provider_roots_are_rechecked_before_summaries_or_model_requests_and_old_receipts_restore()
{
    const CHILD: &str = "TOKENMARKET_ROOT_SCOPE_CHILD";
    if let Ok(id) = std::env::var(CHILD) {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        let tool = tool_id(&id).unwrap();
        let key = match id.as_str() {
            "grok-build" => "GROK_HOME",
            "hermes" => "HERMES_HOME",
            "pi" => "PI_CODING_AGENT_DIR",
            _ => unreachable!(),
        };
        let old = std::env::var_os(key).unwrap();
        let binary = home.join("bin").join(tool.command());
        std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
        std::fs::write(&binary, "fixture; never execute").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let inventory = ToolInventory::new(InstallationEnvironment {
            platform: HostPlatform::MacOs,
            home: home.clone(),
            command_directories: vec![home.join("bin")],
            application_directories: vec![],
            executable_extensions: vec![".EXE".into()],
        });
        let path = inventory.config_path(tool);
        let (url,server)=platform(vec![json!({"data":[{"id":"alpha.model","context_window_tokens":32768,"model_types":["TEXT_GENERATION"]}]})],2).await;
        let directory = home.join("state");
        let manager = support::manager(directory.clone(), url.clone(), url.clone());
        let connected = manager
            .connect(context(), tool, &inventory, path.clone())
            .await
            .unwrap();
        let requests = server.await.unwrap();
        assert!(requests.iter().all(|r| !r.starts_with("POST /v1/")));
        let before = std::fs::read(&path).unwrap();
        drop(manager);
        let manager = support::manager(directory, url.clone(), url);
        for next in [Some(home.join("other-active-root")), None] {
            if let Some(next) = next {
                std::env::set_var(key, next);
            } else {
                std::env::remove_var(key);
            }
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
            assert!(error.to_string().contains("活动配置"), "{error}");
            assert_eq!(std::fs::read(&path).unwrap(), before);
        }
        std::env::set_var(key, &old);
        assert_eq!(
            manager.summaries().unwrap()[0].1.state,
            "configured_untested"
        );
        std::env::set_var(key, home.join("other-active-root"));
        assert_eq!(
            manager
                .disconnect(tool, &connected.key_id, None, DisconnectRequest::default())
                .await
                .unwrap()
                .state,
            "disconnected"
        );
        return;
    }
    for (id, key) in [
        ("grok-build", "GROK_HOME"),
        ("hermes", "HERMES_HOME"),
        ("pi", "PI_CODING_AGENT_DIR"),
    ] {
        let root = tempfile::tempdir().unwrap();
        let output=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","provider_roots_are_rechecked_before_summaries_or_model_requests_and_old_receipts_restore","--nocapture"]).env(CHILD,id).env("HOME",root.path()).env("USERPROFILE",root.path()).env(key,root.path().join("active-root")).output().unwrap();
        assert!(
            output.status.success(),
            "{id}: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
