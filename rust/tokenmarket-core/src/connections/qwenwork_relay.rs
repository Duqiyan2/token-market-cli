//! Loopback-only, per-worker inference relay. Platform credentials never leave
//! this native process except in Authorization to the configured Token Market gateway.
use super::{ConnectionManager, ConnectionRecord};
use crate::{qwenwork_runtime, ToolId};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

const MAX_BODY: usize = 16 * 1024 * 1024;
const MAX_HEADERS: usize = 16 * 1024;
mod host;

pub fn is_invocation() -> bool {
    matches!(
        std::env::args().nth(1).as_deref(),
        Some("__qwenwork-relay" | "__qwenwork-host-relay")
    )
}
pub fn run_blocking() -> Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(run_stdio())
}
pub async fn run_stdio() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("__qwenwork-host-relay") {
        return host::run(&args).await;
    }
    if args.len() != 3 || args[2].len() != 64 || !args[2].bytes().all(|c| c.is_ascii_hexdigit()) {
        bail!("本地代理参数无效")
    }
    let manager = Arc::new(ConnectionManager::current());
    active_record(&manager, &args[2])?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let base = format!("http://127.0.0.1:{}/v1", listener.local_addr()?.port());
    let token = crate::auth::create_pkce().code_verifier;
    // This is a short-lived loopback capability, never a platform API key.
    println!("{}", json!({"baseUrl":base,"token":token}));
    std::io::stdout().flush()?;
    let (stop, stopped) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let mut byte = [0u8; 1];
        let _ = std::io::stdin().read(&mut byte);
        let _ = stop.send(());
    });
    serve(listener, manager, args[2].clone(), token, stopped).await
}

fn active_record(manager: &ConnectionManager, expected: &str) -> Result<ConnectionRecord> {
    let record = manager
        .list()?
        .into_iter()
        .find(|r| r.tool == ToolId::QwenWork)
        .context("千问办公连接已取消，请重新接入")?;
    if record.credential().is_empty()
        || !manager.matches_environment(&record)
        || qwenwork_runtime::fingerprint(record.credential()) != expected
        || super::restore::verify_applied(&record).is_err()
    {
        bail!("千问办公连接已改变，请重新接入")
    }
    let gateway = url::Url::parse(&record.gateway_url)?;
    if !gateway.username().is_empty()
        || gateway.password().is_some()
        || gateway.query().is_some()
        || gateway.fragment().is_some()
        || !(gateway.scheme() == "https"
            || gateway.scheme() == "http"
                && matches!(gateway.host_str(), Some("127.0.0.1" | "localhost")))
    {
        bail!("本地代理上游地址无效")
    }
    Ok(record)
}

async fn serve(
    listener: TcpListener,
    manager: Arc<ConnectionManager>,
    fingerprint: String,
    token: String,
    mut stop: tokio::sync::oneshot::Receiver<()>,
) -> Result<()> {
    let host = listener.local_addr()?.to_string();
    // Saved changes take effect on restart. Existing workers keep the
    // connection they loaded; fresh relays still validate current saved state.
    let record = Arc::new(active_record(&manager, &fingerprint)?);
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(600))
        .build()?;
    let limit = Arc::new(tokio::sync::Semaphore::new(4));
    let mut tasks = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            _ = &mut stop => { tasks.abort_all(); return Ok(()) },
            Some(_) = tasks.join_next(), if !tasks.is_empty() => {},
            accepted = listener.accept() => {
                let (mut socket,peer) = accepted?;
                if !peer.ip().is_loopback() { continue }
                let permit = match limit.clone().try_acquire_owned() {
                    Ok(permit) => permit,
                    Err(_) => { let _=error(&mut socket,429,"Local relay is busy").await; continue }
                };
                let (record, token, host, client) = (record.clone(),token.clone(),host.clone(),client.clone());
                tasks.spawn(async move {
                    let _permit = permit;
                    let _ = handle(&mut socket, &record, &token, &host, &client).await;
                });
            }
        }
    }
}

fn same_secret(actual: &[u8], expected: &[u8]) -> bool {
    actual.len() == expected.len()
        && actual
            .iter()
            .zip(expected)
            .fold(0u8, |v, (a, b)| v | (a ^ b))
            == 0
}
struct Request {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}
async fn request(socket: &mut TcpStream) -> Result<Request> {
    let mut bytes = vec![];
    let end = loop {
        if let Some(pos) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
            break pos;
        }
        if bytes.len() > MAX_HEADERS {
            bail!("headers too large")
        }
        let mut buffer = [0; 4096];
        let n = socket.read(&mut buffer).await?;
        if n == 0 {
            bail!("closed")
        }
        bytes.extend_from_slice(&buffer[..n]);
    };
    if end > MAX_HEADERS {
        bail!("headers too large")
    }
    let text = std::str::from_utf8(&bytes[..end])?;
    let mut lines = text.split("\r\n");
    let parts: Vec<_> = lines
        .next()
        .context("missing request line")?
        .split(' ')
        .collect();
    if parts.len() != 3 || parts[2] != "HTTP/1.1" {
        bail!("unsupported request")
    }
    let method = parts[0].to_owned();
    let path = parts[1].to_owned();
    let mut headers = vec![];
    for line in lines {
        let (name, value) = line.split_once(':').context("invalid header")?;
        let name = name.to_ascii_lowercase();
        if headers.iter().any(|(key, _)| key == &name) {
            bail!("duplicate header")
        }
        headers.push((name, value.trim().to_owned()));
    }
    if headers.iter().any(|(name, _)| name == "transfer-encoding") {
        bail!("unsupported request encoding")
    }
    let length = headers
        .iter()
        .find(|(k, _)| k == "content-length")
        .map(|h| h.1.parse::<usize>())
        .transpose()?
        .unwrap_or(0);
    if length > MAX_BODY {
        bail!("request too large")
    }
    let mut body = bytes[end + 4..].to_vec();
    if body.len() > length {
        bail!("pipelining is not supported")
    }
    while body.len() < length {
        let mut buffer = vec![0u8; (length - body.len()).min(64 * 1024)];
        let n = socket.read(&mut buffer).await?;
        if n == 0 {
            bail!("closed")
        }
        body.extend_from_slice(&buffer[..n]);
    }
    Ok(Request {
        method,
        path,
        headers,
        body,
    })
}

async fn error(socket: &mut TcpStream, status: u16, message: &str) -> Result<()> {
    let body = json!({"error":{"message":message,"type":"tokenmarket_local_relay"}}).to_string();
    socket.write_all(format!("HTTP/1.1 {status} Error\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await?;
    Ok(())
}

async fn handle(
    socket: &mut TcpStream,
    record: &ConnectionRecord,
    token: &str,
    host: &str,
    client: &reqwest::Client,
) -> Result<()> {
    let request = match tokio::time::timeout(Duration::from_secs(30), request(socket)).await {
        Ok(Ok(r)) => r,
        _ => return error(socket, 400, "Invalid local relay request").await,
    };
    if request.method != "POST" || request.path != "/v1/chat/completions" {
        return error(socket, 404, "Unsupported local model route").await;
    }
    let header = |name: &str| {
        request
            .headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    };
    if header("host") != Some(host)
        || header("origin").is_some()
        || !same_secret(
            header("authorization").unwrap_or("").as_bytes(),
            format!("Bearer {token}").as_bytes(),
        )
    {
        return error(socket, 403, "Local relay authorization failed").await;
    }
    let body: Value = match serde_json::from_slice(&request.body) {
        Ok(value) => value,
        Err(_) => return error(socket, 400, "Invalid model request").await,
    };
    if !body["model"]
        .as_str()
        .is_some_and(|model| record.models.iter().any(|allowed| allowed == model))
        || body.get("custom_model").is_some()
        || body.get("api_key").is_some()
    {
        return error(socket, 403, "Model is outside this connection").await;
    }
    let (name, value) =
        if let Some(value) = header("x-session-id").filter(|value| !value.trim().is_empty()) {
            ("X-Session-Id", Some(value))
        } else {
            ("session-id", header("session-id"))
        };
    let session = match session_id(value) {
        Ok(session) => session.map(str::to_owned),
        Err(_) => return error(socket, 400, "Invalid session identifier").await,
    };
    forward(
        socket,
        record,
        request.body,
        client,
        "/v1/chat/completions",
        session.as_deref().map(|value| (name, value)),
    )
    .await
}

// Read only an ID supplied by this request, never one cached for a worker or key.
fn session_id(value: Option<&str>) -> Result<Option<&str>> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if value.len() > 256 || !value.bytes().all(|byte| (0x20..=0x7e).contains(&byte)) {
        bail!("Invalid session identifier")
    }
    Ok(Some(value))
}

async fn forward(
    socket: &mut TcpStream,
    record: &ConnectionRecord,
    body: Vec<u8>,
    client: &reqwest::Client,
    path: &str,
    session: Option<(&str, &str)>,
) -> Result<()> {
    let endpoint = format!("{}{path}", record.gateway_url.trim_end_matches('/'));
    let mut outbound = client
        .post(endpoint)
        .bearer_auth(record.credential())
        .header("Content-Type", "application/json");
    if path == "/v1/messages" {
        outbound = outbound.header("anthropic-version", "2023-06-01");
    }
    if let Some((name, value)) = session {
        outbound = outbound.header(name, value);
    }
    let outbound = outbound.body(body).send();
    // An aborted SDK request must also drop the upstream operation, including
    // while it is waiting for headers or the next stream event.
    let mut extra = [0u8; 1];
    let response = tokio::select! {
        response = outbound => response,
        _ = socket.read(&mut extra) => return Ok(()),
    };
    let mut response = match response {
        Ok(response) if response.status().is_success() => response,
        Ok(response) => {
            return error(
                socket,
                response.status().as_u16(),
                "Token Market rejected the model request",
            )
            .await
        }
        Err(_) => return error(socket, 502, "Token Market could not be reached").await,
    };
    let kind = if response
        .headers()
        .get("content-type")
        .is_some_and(|v| v.as_bytes().starts_with(b"text/event-stream"))
    {
        "text/event-stream"
    } else {
        "application/json"
    };
    socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n").as_bytes()).await?;
    let mut redactor = Redactor::new(record.credential().as_bytes());
    loop {
        let chunk = tokio::select! {
            chunk = response.chunk() => chunk?,
            _ = socket.read(&mut extra) => return Ok(()),
        };
        let Some(chunk) = chunk else { break };
        write_chunk(socket, &redactor.push(&chunk, false)).await?;
    }
    write_chunk(socket, &redactor.push(&[], true)).await?;
    socket.write_all(b"0\r\n\r\n").await?;
    Ok(())
}
async fn write_chunk(socket: &mut TcpStream, bytes: &[u8]) -> Result<()> {
    if !bytes.is_empty() {
        socket
            .write_all(format!("{:x}\r\n", bytes.len()).as_bytes())
            .await?;
        socket.write_all(bytes).await?;
        socket.write_all(b"\r\n").await?;
    }
    Ok(())
}

/// Retain enough trailing bytes to redact even a credential split across chunks.
struct Redactor {
    secret: Vec<u8>,
    pending: Vec<u8>,
}
impl Redactor {
    fn new(secret: &[u8]) -> Self {
        Self {
            secret: secret.into(),
            pending: vec![],
        }
    }
    fn push(&mut self, bytes: &[u8], done: bool) -> Vec<u8> {
        self.pending.extend_from_slice(bytes);
        let mut out = vec![];
        while !self.pending.is_empty() && (done || self.pending.len() >= self.secret.len()) {
            if self.pending.starts_with(&self.secret) {
                out.extend_from_slice(b"[REDACTED]");
                self.pending.drain(..self.secret.len());
            } else {
                let n = if done {
                    self.pending.len()
                } else {
                    self.pending.len() - self.secret.len() + 1
                };
                let next = self
                    .pending
                    .windows(self.secret.len())
                    .position(|w| w == self.secret)
                    .unwrap_or(n)
                    .min(n);
                out.extend(self.pending.drain(..next));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        connections::ConnectionRecord,
        desktop::{DesktopCredentialStore, DesktopIdentity},
    };
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    };
    #[derive(Default)]
    struct Vault(Mutex<Option<String>>);
    #[test]
    fn session_identifiers_preserve_identity_and_enforce_header_limits() {
        assert_eq!(
            session_id(Some(" session:restored-123 ")).unwrap(),
            Some("session:restored-123")
        );
        assert_eq!(session_id(None).unwrap(), None);
        assert_eq!(session_id(Some("  ")).unwrap(), None);
        assert!(session_id(Some(&"s".repeat(256))).is_ok());
        for value in [
            "s".repeat(257),
            "id\r\nX-Other: value".into(),
            "会话".into(),
        ] {
            assert!(session_id(Some(&value)).is_err());
        }
    }
    impl DesktopCredentialStore for Vault {
        fn read(&self) -> Result<Option<String>> {
            Ok(self.0.lock().unwrap().clone())
        }
        fn write(&self, v: &str) -> Result<()> {
            *self.0.lock().unwrap() = Some(v.into());
            Ok(())
        }
        fn delete(&self) -> Result<()> {
            *self.0.lock().unwrap() = None;
            Ok(())
        }
    }
    pub(super) fn seed(directory: &std::path::Path, gateway: &str) -> Arc<ConnectionManager> {
        let manager = Arc::new(ConnectionManager::with_credentials(
            directory.into(),
            "https://console.fixture".into(),
            gateway.into(),
            Arc::new(Vault::default()),
        ));
        let proof = directory.join("proof.json");
        std::fs::write(&proof, br#"{"model":"fixture-model"}"#).unwrap();
        let record = ConnectionRecord {
            version: 2,
            connection_id: "fixture".into(),
            tool: ToolId::QwenWork,
            device_id: "fixture".into(),
            key_id: "123".into(),
            owner: DesktopIdentity {
                id: "u".into(),
                name: "User".into(),
            },
            bound_workspace: DesktopIdentity {
                id: "w".into(),
                name: "Workspace".into(),
            },
            protocol: "OPENAI_CHAT_COMPLETIONS".into(),
            models: vec!["fixture-model".into()],
            model_snapshot: vec![],
            gateway_url: gateway.into(),
            platform_url: "https://console.fixture".into(),
            created_at: "fixture".into(),
            configuration_location: None,
            receipts: vec![super::super::FileReceipt {
                path: proof,
                existed: false,
                backup_path: None,
                created_containers: vec![],
                fields: vec![super::super::FieldReceipt {
                    pointer: "/model".into(),
                    before: None,
                    applied: json!("fixture-model"),
                    retired: false,
                }],
            }],
            api_key: "sk-platform-key-never-given-to-qwen".into(),
        };
        manager
            .private
            .write(
                &directory.join("connections.json"),
                &serde_json::to_vec(&vec![record]).unwrap(),
            )
            .unwrap();
        manager
    }
    #[test]
    fn redaction_covers_every_chunk_boundary() {
        let secret = b"sk-platform-secret";
        let bytes = b"before sk-platform-secret after sk-platform-secret!";
        for split in 0..bytes.len() {
            let mut redactor = Redactor::new(secret);
            let mut out = redactor.push(&bytes[..split], false);
            out.extend(redactor.push(&bytes[split..], false));
            out.extend(redactor.push(&[], true));
            assert_eq!(out, b"before [REDACTED] after [REDACTED]!");
        }
    }
    #[tokio::test]
    async fn saved_disconnect_preserves_running_worker_but_prevents_fresh_relays() {
        let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let gateway = format!("http://{}", upstream.local_addr().unwrap());
        let hits = Arc::new(AtomicUsize::new(0));
        let hit_count = hits.clone();
        let fixture = tokio::spawn(async move {
            for _ in 0..2 {
                let (mut stream, _) = upstream.accept().await.unwrap();
                let received = request(&mut stream).await.unwrap();
                assert_eq!(
                    received
                        .headers
                        .iter()
                        .find(|(n, _)| n == "authorization")
                        .unwrap()
                        .1,
                    "Bearer sk-platform-key-never-given-to-qwen"
                );
                assert!(!String::from_utf8_lossy(&received.body).contains("sk-platform-key"));
                hit_count.fetch_add(1, Ordering::SeqCst);
                let body="data: {\"content\":\"OK sk-platform-key-never-given-to-qwen\"}\n\ndata: [DONE]\n\n";
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
        });
        let temp = tempfile::tempdir().unwrap();
        let manager = seed(temp.path(), &gateway);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let fingerprint = qwenwork_runtime::fingerprint("sk-platform-key-never-given-to-qwen");
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let relay = tokio::spawn(serve(
            listener,
            manager.clone(),
            fingerprint,
            "local-capability-only".into(),
            stopped,
        ));
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let body = json!({"model":"fixture-model","messages":[{"role":"user","content":"fixture"}],"stream":true});
        let url = format!("{base}/v1/chat/completions");
        assert_eq!(
            client.post(&url).json(&body).send().await.unwrap().status(),
            403
        );
        assert_eq!(
            client
                .post(&url)
                .bearer_auth("local-capability-only")
                .header("Origin", "https://attacker.invalid")
                .json(&body)
                .send()
                .await
                .unwrap()
                .status(),
            403
        );
        assert_eq!(
            client
                .post(&url)
                .bearer_auth("local-capability-only")
                .json(&json!({"model":"unmanaged-model"}))
                .send()
                .await
                .unwrap()
                .status(),
            403
        );
        let response = client
            .post(&url)
            .bearer_auth("local-capability-only")
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let output = response.text().await.unwrap();
        assert!(output.contains("OK [REDACTED]"));
        assert!(!output.contains("sk-platform-key"));
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        manager
            .private
            .write(&temp.path().join("connections.json"), b"[]")
            .unwrap();
        assert_eq!(
            client
                .post(&url)
                .bearer_auth("local-capability-only")
                .json(&body)
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
        fixture.await.unwrap();
        assert_eq!(hits.load(Ordering::SeqCst), 2);
        assert!(active_record(
            &manager,
            &qwenwork_runtime::fingerprint("sk-platform-key-never-given-to-qwen")
        )
        .is_err());
        stop.send(()).unwrap();
        relay.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn sdk_cancellation_closes_upstream_before_headers_and_between_stream_events() {
        for send_headers in [false, true] {
            let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let gateway = format!("http://{}", upstream.local_addr().unwrap());
            let (ready, received) = tokio::sync::oneshot::channel();
            let fixture = tokio::spawn(async move {
                let (mut socket, _) = upstream.accept().await.unwrap();
                request(&mut socket).await.unwrap();
                if send_headers {
                    socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: 100\r\nConnection: close\r\n\r\n").await.unwrap();
                }
                ready.send(()).unwrap();
                let mut byte = [0u8; 1];
                assert_eq!(
                    tokio::time::timeout(Duration::from_secs(3), socket.read(&mut byte))
                        .await
                        .unwrap()
                        .unwrap(),
                    0
                );
            });
            let temp = tempfile::tempdir().unwrap();
            let manager = seed(temp.path(), &gateway);
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let (stop, stopped) = tokio::sync::oneshot::channel();
            let relay = tokio::spawn(serve(
                listener,
                manager,
                qwenwork_runtime::fingerprint("sk-platform-key-never-given-to-qwen"),
                "fixture-capability".into(),
                stopped,
            ));
            let mut downstream = TcpStream::connect(address).await.unwrap();
            let body = json!({"model":"fixture-model","messages":[],"stream":true}).to_string();
            downstream.write_all(format!("POST /v1/chat/completions HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer fixture-capability\r\nContent-Length: {}\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            received.await.unwrap();
            if send_headers {
                let mut bytes = vec![];
                while !bytes.windows(4).any(|b| b == b"\r\n\r\n") {
                    let mut buffer = [0u8; 1024];
                    let n = downstream.read(&mut buffer).await.unwrap();
                    assert_ne!(n, 0);
                    bytes.extend_from_slice(&buffer[..n]);
                }
            }
            drop(downstream);
            fixture.await.unwrap();
            stop.send(()).unwrap();
            relay.await.unwrap().unwrap();
        }
    }

    #[tokio::test]
    async fn installed_sdk_uses_the_native_relay_without_obtaining_the_platform_key() {
        let app = std::path::Path::new("/Applications/QwenWorkCN.app");
        let path=app.join("Contents/Resources/app.asar.unpacked/node_modules/@qoder-ai/qoder-agent-sdk/dist/_worker/qoder-worker-runtime.obf.mjs");
        if !path.exists() {
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let gateway = format!("http://{}", upstream.local_addr().unwrap());
        let manager = seed(temp.path(), &gateway);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        let fingerprint = qwenwork_runtime::fingerprint("sk-platform-key-never-given-to-qwen");
        let token = "local-opaque-capability-abcdefghijklmnopqrstuvwxyz1234567890";
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let relay = tokio::spawn(serve(
            listener,
            manager,
            fingerprint.clone(),
            token.into(),
            stopped,
        ));
        let fixture = tokio::spawn(async move {
            for round in 0..4 {
                let (mut stream, _) = upstream.accept().await.unwrap();
                let received = request(&mut stream).await.unwrap();
                let expected_session = if round == 2 {
                    "second-session"
                } else {
                    "fixture-session"
                };
                assert!(received
                    .headers
                    .iter()
                    .any(|(k, v)| k == "session-id" && v == expected_session));
                assert_eq!(
                    received
                        .headers
                        .iter()
                        .find(|(k, _)| k == "authorization")
                        .unwrap()
                        .1,
                    "Bearer sk-platform-key-never-given-to-qwen"
                );
                let body: Value = serde_json::from_slice(&received.body).unwrap();
                assert!(!String::from_utf8_lossy(&received.body).contains("sk-platform-key"));
                if round == 1 {
                    assert_eq!(
                        body["messages"].as_array().unwrap().last().unwrap()["tool_call_id"],
                        "call_fixture"
                    )
                }
                let delta = if round == 0 {
                    json!({"tool_calls":[{"index":0,"id":"call_fixture","type":"function","function":{"name":"fixture_time","arguments":"{}"}}]})
                } else {
                    json!({"content":"OK"})
                };
                let chunk=json!({"id":"fixture","object":"chat.completion.chunk","created":1,"model":"fixture-model","choices":[{"index":0,"delta":delta,"finish_reason":null}]}).to_string();
                let finish=json!({"id":"fixture","object":"chat.completion.chunk","created":1,"model":"fixture-model","choices":[{"index":0,"delta":{},"finish_reason":if round==0{"tool_calls"}else{"stop"}}]}).to_string();
                let body = format!("data: {chunk}\n\ndata: {finish}\n\ndata: [DONE]\n\n");
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
        });
        // The bootstrap child carries only the loopback capability. The actual
        // platform key remains in the native parent process's encrypted vault.
        let bridge = temp.path().join("bootstrap");
        let bootstrap = json!({"baseUrl":base,"token":token}).to_string();
        std::fs::write(&bridge,format!("#!/usr/bin/env node\nprocess.stdout.write({}+\"\\n\");process.stdin.resume();process.stdin.on('end',()=>process.exit(0));\n",serde_json::to_string(&bootstrap).unwrap())).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&bridge, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let source = std::fs::read_to_string(path).unwrap();
        let mut patched = qwenwork_runtime::render_worker(&source, &bridge, &gateway).unwrap();
        patched = patched.replace(
            "r={kind:\"qoder\"};",
            "throw new Error(\"Cloud model fallback is forbidden\");r={kind:\"qoder\"};",
        );
        let end = patched.rfind("SD(),qar(").unwrap();
        patched.truncate(end);
        patched.push_str("\nvyn();Nnt();export {hnt as requestDispatcher,Otd as parseModelPolicy,wdl as assembleModel};\n");
        assert!(!patched.contains("sk-platform-key-never-given-to-qwen"));
        let worker = temp.path().join("worker.mjs");
        std::fs::write(&worker, patched).unwrap();
        let probe = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/helpers/qwenwork-local-relay-probe.mjs");
        let output = tokio::task::spawn_blocking(move || {
            std::process::Command::new("node")
                .arg(probe)
                .arg(worker)
                .arg(fingerprint)
                .output()
                .unwrap()
        })
        .await
        .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("PASS: real SDK"));
        fixture.await.unwrap();
        stop.send(()).unwrap();
        relay.await.unwrap().unwrap();
    }
}
