use serde_json::json;
use tokenmarket_core::{
    desktop::DesktopSession,
    usage::{UsageClient, UsageKind, UsageQuery},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn session() -> DesktopSession {
    serde_json::from_value(json!({"session_token":"private-fixture-session","expires_at":"2099-01-01T00:00:00Z","user":{"id":"42","name":"Ada"},"default_workspace":{"id":"7","name":"Workspace"}})).unwrap()
}
fn query() -> UsageQuery {
    serde_json::from_value(json!({"ownerId":"42","workspaceId":"7","startTime":1,"endTime":100,"tool":"codex","model":"model-a","status":"ALL","cursor":null,"page":1,"pageSize":20})).unwrap()
}
#[tokio::test]
async fn native_identity_and_exact_filters_are_sent_without_model_keys() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut buf = [0; 4096];
            let n = socket.read(&mut buf).await.unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&buf[..n]);
            if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                let header = String::from_utf8_lossy(&bytes[..end]);
                let len: usize = header
                    .lines()
                    .find_map(|s| {
                        s.to_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse().unwrap())
                    })
                    .unwrap();
                if bytes.len() >= end + 4 + len {
                    assert!(header.starts_with("POST /api/client/usage/logs "));
                    assert!(header
                        .to_lowercase()
                        .contains("authorization: bearer private-fixture-session"));
                    assert!(header.to_lowercase().contains("x-workspace-id: 7"));
                    let body: serde_json::Value =
                        serde_json::from_slice(&bytes[end + 4..end + 4 + len]).unwrap();
                    assert_eq!(body["deviceId"], "native-device-12345678");
                    assert_eq!(body["workspaceId"], "7");
                    assert_eq!(body["tool"], "codex");
                    assert_eq!(body["model"], "model-a");
                    assert!(body.get("apiKeyIds").is_none());
                    assert!(body.get("session_token").is_none());
                    break;
                }
            }
        }
        let data=json!({"success":true,"data":{"tools":[{"id":"codex","name":"Codex","keyCount":2}],"granularity":"HOUR","buckets":[{"date":"1970-01-01","bucketStart":1,"requestCount":21,"costMicro":"9007199254740993","inputTokens":100,"outputTokens":20,"totalTokens":120,"cacheReadTokens":80,"cacheWriteTokens":0,"inputTokenCount":100}],"groups":[{"tool":"codex","model":"model-a","requestCount":21,"costMicro":"9007199254740993","totalTokens":120,"successCount":20,"speedOutputTokens":100,"generationMs":2000,"ttftTotalMs":200,"ttftCount":1}],"items":[],"nextCursor":null,"page":1,"pageSize":20,"asOf":100}}).to_string();
        socket
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    data.len(),
                    data
                )
                .as_bytes(),
            )
            .await
            .unwrap();
    });
    let response = UsageClient::new(base)
        .query(
            &session(),
            "native-device-12345678",
            &query(),
            UsageKind::Logs,
        )
        .await
        .unwrap();
    assert_eq!(response.tools[0].key_count, 2);
    assert_eq!(response.granularity.as_deref(), Some("HOUR"));
    assert_eq!(response.buckets[0].bucket_start, Some(1));
    assert_eq!(response.buckets[0].cost_micro, "9007199254740993");
    let groups = response.groups.as_ref().unwrap();
    assert_eq!(groups[0].model, "model-a");
    assert_eq!(groups[0].cost_micro, "9007199254740993");
    server.await.unwrap();
}
#[tokio::test]
async fn changed_identity_invalid_device_or_negative_range_fails_before_network() {
    let client = UsageClient::new("http://127.0.0.1:1".into());
    let mut q = query();
    q.workspace_id = "8".into();
    assert!(client
        .query(&session(), "native-device-12345678", &q, UsageKind::Logs)
        .await
        .unwrap_err()
        .to_string()
        .contains("当前空间"));
    q = query();
    q.owner_id = "43".into();
    assert!(q.validate(&session()).is_err());
    q = query();
    q.start_time = -1;
    assert!(q.validate(&session()).is_err());
    q.start_time = 0;
    q.end_time = 40 * 86400000;
    assert!(q.validate(&session()).is_ok());
    assert!(client
        .query(&session(), "bad", &query(), UsageKind::Buckets)
        .await
        .unwrap_err()
        .to_string()
        .contains("设备标识"));
}
#[tokio::test]
async fn unavailable_service_does_not_invent_empty_usage() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut b = [0; 8192];
        socket.read(&mut b).await.unwrap();
        socket
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
    });
    let error = UsageClient::new(base)
        .query(
            &session(),
            "native-device-12345678",
            &query(),
            UsageKind::Buckets,
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("暂未启用"));
    server.await.unwrap();
}
