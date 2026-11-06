//! The desktop's auxiliary-model client does not use the SDK worker. Give its
//! credential broker a loopback capability and base URL, never a platform key.
use super::*;
use crate::qwenwork_runtime::HostRoute;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

pub(super) async fn run(args: &[String]) -> Result<()> {
    if args.len() != 4 {
        bail!("本地桌面代理参数无效")
    }
    let route: HostRoute = serde_json::from_slice(&std::fs::read(&args[2])?)?;
    route.validate()?;
    let pid: u32 = args[3].parse()?;
    let manager = Arc::new(ConnectionManager::current());
    active_record(&manager, &route.fingerprint)?;
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, route.port)).await?;
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let parent = Parent::new(pid)?;
    tokio::spawn(async move {
        while parent.alive() {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        let _ = stop.send(());
    });
    serve(listener, manager, route, stopped).await
}

#[cfg(unix)]
struct Parent(i32);
#[cfg(unix)]
impl Parent {
    fn new(pid: u32) -> Result<Self> {
        if pid < 2 || pid > i32::MAX as u32 {
            bail!("无效宿主进程")
        }
        Ok(Self(pid as i32))
    }
    fn alive(&self) -> bool {
        unsafe extern "C" {
            fn kill(pid: i32, signal: i32) -> i32;
        }
        unsafe { kill(self.0, 0) == 0 }
    }
}
#[cfg(windows)]
struct Parent(isize);
#[cfg(windows)]
impl Parent {
    fn new(pid: u32) -> Result<Self> {
        use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE};
        let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        if handle.is_null() {
            bail!("无法确认千问办公宿主进程")
        }
        Ok(Self(handle as isize))
    }
    fn alive(&self) -> bool {
        unsafe { windows_sys::Win32::System::Threading::WaitForSingleObject(self.0 as _, 0) == 258 }
    }
}
#[cfg(windows)]
impl Drop for Parent {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0 as _);
        }
    }
}

async fn serve(
    listener: TcpListener,
    manager: Arc<ConnectionManager>,
    route: HostRoute,
    mut stop: tokio::sync::oneshot::Receiver<()>,
) -> Result<()> {
    let host = listener.local_addr()?.to_string();
    let record = Arc::new(active_record(&manager, &route.fingerprint)?);
    if !record.receipts.iter().any(|file| {
        file.fields.iter().any(|field| {
            !field.retired && field.pointer == "/capability" && field.applied == route.capability
        })
    }) {
        bail!("Desktop route changed; restart QwenWork")
    }
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(600))
        .build()?;
    let limit = Arc::new(tokio::sync::Semaphore::new(16));
    let mut tasks = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            _ = &mut stop => { tasks.abort_all(); return Ok(()) },
            Some(_) = tasks.join_next(), if !tasks.is_empty() => {},
            accepted = listener.accept() => {
                let (mut socket, peer) = accepted?;
                if !peer.ip().is_loopback() { continue }
                let Ok(permit) = limit.clone().try_acquire_owned() else {
                    let _ = error(&mut socket, 429, "Local relay is busy").await; continue
                };
                let (record, route, host, client) = (record.clone(), route.clone(), host.clone(), client.clone());
                tasks.spawn(async move {
                    let _permit = permit;
                    let _ = handle(&mut socket, &record, &route, &host, &client).await;
                });
            }
        }
    }
}

async fn json_response(socket: &mut TcpStream, value: Value) -> Result<()> {
    let body = serde_json::to_vec(&value)?;
    socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).as_bytes()).await?;
    socket.write_all(&body).await?;
    Ok(())
}

async fn handle(
    socket: &mut TcpStream,
    record: &ConnectionRecord,
    route: &HostRoute,
    host: &str,
    client: &reqwest::Client,
) -> Result<()> {
    let req = match tokio::time::timeout(Duration::from_secs(30), request(socket)).await {
        Ok(Ok(req)) => req,
        _ => return error(socket, 400, "Invalid desktop relay request").await,
    };
    let header = |name: &str| {
        req.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    };
    let Some(path) = req
        .path
        .strip_prefix(&format!("/tokenmarket/{}/", route.capability))
    else {
        return error(socket, 403, "Desktop relay authorization failed").await;
    };
    if header("host") != Some(host) || header("origin").is_some() {
        return error(socket, 403, "Desktop relay authorization failed").await;
    }
    if req.method == "GET" && path == "health" {
        return json_response(
            socket,
            json!({"contract":"tokenmarket-desktop-v1","fingerprint":route.fingerprint}),
        )
        .await;
    }
    if req.method != "POST" {
        return error(socket, 405, "Unsupported desktop relay method").await;
    }
    let mut body: Value = match serde_json::from_slice(&req.body) {
        Ok(Value::Object(body)) => Value::Object(body),
        _ => return error(socket, 400, "Invalid model request").await,
    };
    if path == "api/desktop-sessions" {
        // The app supplies its official login token here. It is deliberately
        // neither persisted nor forwarded. Only the unguessable URL grants access.
        let session = match session_id(body["session_id"].as_str()) {
            Ok(session) => session,
            Err(_) => return error(socket, 400, "Invalid session identifier").await,
        };
        // The desktop captures fetch and does not send its session ID on inference.
        // Its credential broker is scoped by session, so return a scoped URL. No
        // mutable process-wide "current session" can mix simultaneous chats.
        let base_url = session.map_or_else(
            || route.base_url(),
            |session| {
                format!(
                    "{}/sessions/{}",
                    route.base_url(),
                    URL_SAFE_NO_PAD.encode(session)
                )
            },
        );
        let response_session = session
            .map(str::to_owned)
            .unwrap_or_else(|| crate::auth::create_pkce().code_verifier);
        return json_response(
            socket,
            json!({"access_token":route.capability,"base_url":base_url,
            "session_id":response_session,"expires_at_ms":0,"ttl_ms":0}),
        )
        .await;
    }
    if !same_secret(
        header("authorization").unwrap_or("").as_bytes(),
        format!("Bearer {}", route.capability).as_bytes(),
    ) {
        return error(socket, 403, "Desktop relay authorization failed").await;
    }
    let scoped_session;
    let (path, name, session) = if let Some(scoped) = path.strip_prefix("sessions/") {
        let Some((encoded, path)) = scoped.split_once('/') else {
            return error(socket, 400, "Invalid session route").await;
        };
        scoped_session = match URL_SAFE_NO_PAD
            .decode(encoded)
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
        {
            Some(session) => session,
            None => return error(socket, 400, "Invalid session route").await,
        };
        (path, "X-Session-Id", Some(scoped_session.as_str()))
    } else if let Some(value) = header("x-session-id").filter(|value| !value.trim().is_empty()) {
        (path, "X-Session-Id", Some(value))
    } else {
        (path, "session-id", header("session-id"))
    };
    let session = match session_id(session) {
        Ok(session) => session,
        Err(_) => return error(socket, 400, "Invalid session identifier").await,
    };
    let protocol = match path {
        "v1/chat/completions" => "OPENAI_CHAT_COMPLETIONS",
        "v1/messages" => "ANTHROPIC_MESSAGES",
        "v1/responses" => "OPENAI_RESPONSES",
        _ => return error(socket, 422, "This model interface is not supported by Token Market; official model fallback is disabled").await,
    };
    let supports = |model: &str| {
        record
            .model_snapshot
            .iter()
            .find(|m| m["id"] == model)
            .map(|row| {
                if let Some(types) = row["model_types"].as_array() {
                    return types.iter().any(|kind| kind == "TEXT_GENERATION");
                }
                row["endpoints"]
                    .as_array()
                    .map(|endpoints| {
                        endpoints.iter().any(|p| {
                            p == protocol
                                || protocol == "ANTHROPIC_MESSAGES" && p == "CLAUDE_MESSAGES"
                        })
                    })
                    .unwrap_or(protocol == "OPENAI_CHAT_COMPLETIONS")
            })
            .unwrap_or(protocol == "OPENAI_CHAT_COMPLETIONS")
    };
    if !record.models.iter().any(|model| supports(model)) {
        return error(socket, 422, "This model interface is not supported by Token Market; official model fallback is disabled").await;
    }
    if body.get("api_key").is_some()
        || body.get("custom_model").is_some()
        || !(if protocol == "OPENAI_RESPONSES" {
            body["input"].is_array() || body["input"].is_string()
        } else {
            body["messages"].is_array()
        })
        || !body["model"].is_string()
    {
        return error(socket, 400, "Invalid model request").await;
    }
    // Desktop-owned aliases such as lite/flash have no meaning at our gateway.
    // Use an explicit connection model; managed model IDs pass through unchanged.
    if !record.models.iter().any(|m| body["model"] == *m) {
        let model =
            if record.models.contains(&route.default_model) && supports(&route.default_model) {
                Some(&route.default_model)
            } else {
                record.models.iter().find(|model| supports(model))
            };
        body["model"] = json!(model);
    }
    if !record
        .models
        .iter()
        .any(|m| body["model"] == *m && supports(m))
    {
        return error(
            socket,
            409,
            "Auxiliary model is no longer available; reconnect the tool",
        )
        .await;
    }
    forward(
        socket,
        record,
        serde_json::to_vec(&body)?,
        client,
        &format!("/{path}"),
        session.map(|value| (name, value)),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn saved_reconnect_preserves_running_desktop_and_new_processes_require_new_capability() {
        let temp = tempfile::tempdir().unwrap();
        let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let manager = super::super::tests::seed(
            temp.path(),
            &format!("http://{}", upstream.local_addr().unwrap()),
        );
        let mut records = manager.list().unwrap();
        records[0].model_snapshot =
            vec![json!({"id":"fixture-model","model_types":["TEXT_GENERATION"]})];
        let route_file = temp.path().join("routing.json");
        std::fs::write(&route_file, br#"{"capability":"fixture-capability"}"#).unwrap();
        records[0].receipts = vec![super::super::super::FileReceipt {
            path: route_file.clone(),
            existed: false,
            backup_path: None,
            created_containers: vec![],
            fields: vec![super::super::super::FieldReceipt {
                pointer: "/capability".into(),
                before: None,
                applied: json!("fixture-capability"),
                retired: false,
            }],
        }];
        manager
            .private
            .write(
                &temp.path().join("connections.json"),
                &serde_json::to_vec(&records).unwrap(),
            )
            .unwrap();
        let fixture = tokio::spawn(async move {
            for path in ["/v1/responses", "/v1/messages"] {
                let (mut socket, _) = upstream.accept().await.unwrap();
                let req = request(&mut socket).await.unwrap();
                assert_eq!(req.path, path);
                assert_eq!(
                    serde_json::from_slice::<Value>(&req.body).unwrap()["model"],
                    "fixture-model"
                );
                if path == "/v1/messages" {
                    assert!(req
                        .headers
                        .iter()
                        .any(|(k, v)| k == "anthropic-version" && v == "2023-06-01"));
                }
                json_response(&mut socket, json!({"id":"fixture","model":"fixture-model"}))
                    .await
                    .unwrap();
            }
        });
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let route = HostRoute {
            fingerprint: qwenwork_runtime::fingerprint("sk-platform-key-never-given-to-qwen"),
            port: listener.local_addr().unwrap().port(),
            default_model: "fixture-model".into(),
            capability: "fixture-capability".into(),
        };
        let base = route.base_url();
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let relay = tokio::spawn(serve(listener, manager.clone(), route.clone(), stopped));
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        for (path, body) in [
            ("v1/responses", json!({"model":"lite","input":"fixture"})),
            (
                "v1/messages",
                json!({"model":"lite","messages":[],"max_tokens":16}),
            ),
        ] {
            let response = client
                .post(format!("{base}/{path}"))
                .bearer_auth(&route.capability)
                .json(&body)
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), 200);
            response.bytes().await.unwrap();
        }
        fixture.await.unwrap();
        // Saving is deferred until restart. The existing desktop process keeps
        // its loaded connection; a new process must use the new capability.
        records[0].receipts[0].fields[0].applied = json!("new-capability");
        std::fs::write(&route_file, br#"{"capability":"new-capability"}"#).unwrap();
        manager
            .private
            .write(
                &temp.path().join("connections.json"),
                &serde_json::to_vec(&records).unwrap(),
            )
            .unwrap();
        assert_eq!(
            client
                .post(format!("{base}/api/desktop-sessions"))
                .json(&json!({}))
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
        stop.send(()).unwrap();
        relay.await.unwrap().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let (_stop, stopped) = tokio::sync::oneshot::channel();
        let stale = tokio::time::timeout(
            Duration::from_secs(1),
            serve(listener, manager, route, stopped),
        )
        .await
        .unwrap();
        assert!(
            stale.is_err(),
            "a new desktop process cannot reuse the previous process capability"
        );
    }

    #[tokio::test]
    async fn desktop_mint_and_auxiliary_models_use_only_the_native_platform_credential() {
        let temp = tempfile::tempdir().unwrap();
        let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let gateway = format!("http://{}", upstream.local_addr().unwrap());
        let manager = super::super::tests::seed(temp.path(), &gateway);
        let app = std::path::Path::new("/Applications/QwenWorkCN.app");
        let use_desktop = app.join("Contents/Resources/app.asar").is_file();
        let fixture = tokio::spawn(async move {
            for _ in 0..if use_desktop { 15 } else { 2 } {
                let (mut socket, _) = upstream.accept().await.unwrap();
                let req = request(&mut socket).await.unwrap();
                assert_eq!(req.path, "/v1/chat/completions");
                assert!(req.headers.iter().any(|(k, v)| k == "authorization"
                    && v == "Bearer sk-platform-key-never-given-to-qwen"));
                let body: Value = serde_json::from_slice(&req.body).unwrap();
                let expected_session = if use_desktop {
                    body["messages"][0]["content"]
                        .as_str()
                        .unwrap()
                        .split(':')
                        .next()
                        .unwrap()
                } else {
                    "fixture-session"
                };
                assert!(req
                    .headers
                    .iter()
                    .any(|(k, v)| k == "x-session-id" && v == expected_session));
                assert_eq!(body["model"], "fixture-model");
                assert_eq!(body["stream"], false);
                let text = String::from_utf8(req.body).unwrap();
                assert!(!text.contains("official-login-secret"));
                let body = json!({"model":"fixture-model","choices":[{"message":{"role":"assistant","content":"OK"}}]}).to_string();
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
        });
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let route = HostRoute {
            fingerprint: qwenwork_runtime::fingerprint("sk-platform-key-never-given-to-qwen"),
            port: listener.local_addr().unwrap().port(),
            default_model: "fixture-model".into(),
            capability: "local-desktop-capability-abcdefghijklmnopqrstuvwxyz123456".into(),
        };
        let base = route.base_url();
        let mut records = manager.list().unwrap();
        let proof = &records[0].receipts[0].path;
        std::fs::write(
            proof,
            serde_json::to_vec(&json!({"model":"fixture-model","capability":route.capability}))
                .unwrap(),
        )
        .unwrap();
        records[0].receipts[0]
            .fields
            .push(super::super::super::FieldReceipt {
                pointer: "/capability".into(),
                before: None,
                applied: json!(route.capability),
                retired: false,
            });
        manager
            .private
            .write(
                &temp.path().join("connections.json"),
                &serde_json::to_vec(&records).unwrap(),
            )
            .unwrap();
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let relay = tokio::spawn(serve(listener, manager.clone(), route.clone(), stopped));
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let minted: Value = client
            .post(format!("{base}/api/desktop-sessions"))
            .bearer_auth("official-login-secret")
            .json(&json!({"session_id":"fixture-session"}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(minted["session_id"], "fixture-session");
        assert_eq!(minted["access_token"], route.capability);
        let scoped_base = minted["base_url"].as_str().unwrap();
        assert!(scoped_base.starts_with(&format!("{base}/sessions/")));
        assert!(!minted.to_string().contains("sk-platform"));
        if use_desktop {
            let output = tokio::task::spawn_blocking({
                let base = base.clone();
                let cap = route.capability.clone();
                move || {
                    std::process::Command::new(app.join("Contents/MacOS/QwenWorkCN"))
                        .env("ELECTRON_RUN_AS_NODE", "1")
                        .env_remove("NODE_OPTIONS")
                        .env_remove("NODE_PATH")
                        .arg(
                            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                                .join("../../tests/helpers/qwenwork-host-relay-probe.mjs"),
                        )
                        .arg(app.join("Contents/Resources/app.asar"))
                        .arg(base)
                        .arg(cap)
                        .output()
                        .unwrap()
                }
            })
            .await
            .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(String::from_utf8_lossy(&output.stdout).contains("PASS:"));
        } else {
            for alias in ["lite", "flash"] {
                let response = client.post(format!("{scoped_base}/v1/chat/completions")).bearer_auth(&route.capability)
                .json(&json!({"model":alias,"messages":[{"role":"user","content":"fixture"}],"stream":false}))
                .send().await.unwrap();
                assert_eq!(response.status(), 200);
                assert!(response.text().await.unwrap().contains("OK"));
            }
        }
        fixture.await.unwrap();
        for path in ["v1/messages", "vendors/image/generation", "v1/responses"] {
            assert_eq!(
                client
                    .post(format!("{base}/{path}"))
                    .bearer_auth(&route.capability)
                    .json(&json!({}))
                    .send()
                    .await
                    .unwrap()
                    .status(),
                422
            );
        }
        assert_eq!(
            client
                .post(format!("{base}/v1/chat/completions"))
                .bearer_auth("wrong")
                .json(&json!({}))
                .send()
                .await
                .unwrap()
                .status(),
            403
        );
        manager
            .private
            .write(&temp.path().join("connections.json"), b"[]")
            .unwrap();
        assert_eq!(
            client
                .post(format!("{base}/api/desktop-sessions"))
                .json(&json!({}))
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
        stop.send(()).unwrap();
        relay.await.unwrap().unwrap();
    }
}
