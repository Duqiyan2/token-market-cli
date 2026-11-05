use super::*;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

const PUBLIC_KEY: &str = include_str!("../../tests/fixtures/client-update/public-key.txt");
const SIGNATURE: &str = include_str!("../../tests/fixtures/client-update/package.txt.sig");

struct ManifestServer {
    url: String,
    requests: Arc<Mutex<Vec<String>>>,
    done: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl ManifestServer {
    fn start(manifest: serde_json::Value) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/stable.json", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let done = Arc::new(AtomicBool::new(false));
        let log = requests.clone();
        let stopping = done.clone();
        let body = manifest.to_string();
        let worker = thread::spawn(move || {
            while !stopping.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream.set_nonblocking(false).unwrap();
                        stream
                            .set_read_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        let mut buffer = [0; 4096];
                        let read = stream.read(&mut buffer).unwrap_or(0);
                        if read == 0 {
                            continue;
                        }
                        log.lock().unwrap().push(
                            String::from_utf8_lossy(&buffer[..read])
                                .lines()
                                .next()
                                .unwrap_or("")
                                .to_string(),
                        );
                        write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(error) => panic!("fixture listener: {error}"),
                }
            }
        });
        Self {
            url,
            requests,
            done,
            worker: Some(worker),
        }
    }
}

impl Drop for ManifestServer {
    fn drop(&mut self) {
        self.done.store(true, Ordering::SeqCst);
        self.worker.take().unwrap().join().unwrap();
    }
}

fn app() -> tauri::App<tauri::test::MockRuntime> {
    let mut context = tauri::test::mock_context(tauri::test::noop_assets());
    context.package_info_mut().version = "1.0.0".parse().unwrap();
    context
        .config_mut()
        .plugins
        .0
        .insert("updater".into(), serde_json::json!({"pubkey":"","requireSignedVersion":true,"dangerousInsecureTransportProtocol":true}));
    tauri::test::mock_builder()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .build(context)
        .unwrap()
}

#[tokio::test]
async fn manual_check_reads_a_signed_static_release_without_fetching_the_artifact() {
    let app = app();
    for target in ["darwin-aarch64", "windows-x86_64"] {
        let server = ManifestServer::start(serde_json::json!({
            "version":"1.1.0", "notes":"修复窗口恢复", "platforms": {
                (target):{"url":"https://updates.example.invalid/package","signature":SIGNATURE.trim()}
            }
        }));
        let executable = tempfile::tempdir().unwrap();
        let mut updater = ClientUpdater::new("1.0.0".into());
        updater.config.endpoint = Some(server.url.clone());
        updater.config.public_key = Some(PUBLIC_KEY.trim().into());
        updater.config.target = Some(target.into());
        updater.config.allow_loopback_http = true;
        updater.config.executable_path = Some(
            executable
                .path()
                .join("Token Market.app/Contents/MacOS/Token Market"),
        );
        let result = updater.check(app.handle()).await;
        assert_eq!(result.status, "available");
        assert_eq!(result.current_version, "1.0.0");
        assert_eq!(result.available_version.as_deref(), Some("1.1.0"));
        assert_eq!(result.release_notes.as_deref(), Some("修复窗口恢复"));
        assert_eq!(
            *server.requests.lock().unwrap(),
            vec!["GET /stable.json HTTP/1.1"]
        );
    }
}

#[tokio::test]
async fn manual_check_only_offers_newer_stable_versions() {
    let app = app();
    for version in ["1.0.0", "0.9.0", "2.0.0-beta.1"] {
        let server = ManifestServer::start(serde_json::json!({"version":version,"platforms":{
            "darwin-aarch64":{"url":"https://updates.example.invalid/macos.tar.gz","signature":SIGNATURE.trim()}
        }}));
        let executable = tempfile::tempdir().unwrap();
        let mut updater = ClientUpdater::new("1.0.0".into());
        updater.config.endpoint = Some(server.url.clone());
        updater.config.public_key = Some(PUBLIC_KEY.trim().into());
        updater.config.target = Some("darwin-aarch64".into());
        updater.config.allow_loopback_http = true;
        updater.config.executable_path = Some(
            executable
                .path()
                .join("Token Market.app/Contents/MacOS/Token Market"),
        );
        let result = updater.check(app.handle()).await;
        assert_eq!(result.status, "current", "{version}: {:?}", result.error);
        assert!(result.available_version.is_none());
    }
}

#[tokio::test]
async fn manual_check_refuses_a_release_without_a_matching_signed_static_artifact() {
    let app = app();
    for manifest in [
        serde_json::json!({"version":"1.1.0","url":"https://updates.example.invalid/wrong-package.exe","signature":SIGNATURE.trim()}),
        serde_json::json!({"version":"1.1.0","platforms":{"windows-x86_64":{"url":"https://updates.example.invalid/windows.exe","signature":SIGNATURE.trim()}}}),
        serde_json::json!({"version":"1.1.0","platforms":{"darwin-aarch64":{"url":"https://updates.example.invalid/macos.tar.gz"}}}),
        serde_json::json!({"version":"1.1.0","platforms":{"darwin-aarch64":{"url":"https://updates.example.invalid/macos.tar.gz","signature":""}}}),
    ] {
        let server = ManifestServer::start(manifest);
        let executable = tempfile::tempdir().unwrap();
        let mut updater = ClientUpdater::new("1.0.0".into());
        updater.config.endpoint = Some(server.url.clone());
        updater.config.public_key = Some(PUBLIC_KEY.trim().into());
        updater.config.target = Some("darwin-aarch64".into());
        updater.config.allow_loopback_http = true;
        updater.config.executable_path = Some(
            executable
                .path()
                .join("Token Market.app/Contents/MacOS/Token Market"),
        );
        let result = updater.check(app.handle()).await;
        assert_eq!(result.status, "failed", "{result:?}");
        assert!(result.available_version.is_none());
        assert!(result.error.is_some());
        assert_eq!(
            *server.requests.lock().unwrap(),
            vec!["GET /stable.json HTTP/1.1"],
            "{result:?}"
        );
    }
}

#[tokio::test]
async fn production_checks_require_https_even_in_debug_builds() {
    let server = ManifestServer::start(serde_json::json!({"version":"1.1.0","platforms":{
        "darwin-aarch64":{"url":"https://updates.example.invalid/macos.tar.gz","signature":SIGNATURE.trim()}
    }}));
    let app = app();
    let executable = tempfile::tempdir().unwrap();
    let mut updater = ClientUpdater::new("1.0.0".into());
    updater.config.endpoint = Some(server.url.clone());
    updater.config.public_key = Some(PUBLIC_KEY.trim().into());
    updater.config.target = Some("darwin-aarch64".into());
    updater.config.executable_path = Some(
        executable
            .path()
            .join("Token Market.app/Contents/MacOS/Token Market"),
    );
    let result = updater.check(app.handle()).await;
    assert_eq!(result.status, "failed");
    assert!(result.error.unwrap().contains("HTTPS"));
    assert!(server.requests.lock().unwrap().is_empty());

    let insecure_artifact = ManifestServer::start(
        serde_json::json!({"version":"1.1.0","platforms":{
            "darwin-aarch64":{"url":"http://updates.example.invalid/macos.tar.gz","signature":SIGNATURE.trim()}
        }}),
    );
    updater.config.endpoint = Some(insecure_artifact.url.clone());
    updater.config.allow_loopback_http = true;
    let result = updater.check(app.handle()).await;
    assert_eq!(result.status, "failed");
    assert!(result.error.unwrap().contains("HTTPS"));
    assert_eq!(
        *insecure_artifact.requests.lock().unwrap(),
        vec!["GET /stable.json HTTP/1.1"]
    );
}

#[tokio::test]
async fn unconfigured_checks_give_an_actionable_publisher_reason() {
    let app = app();
    for (endpoint, public_key, reason) in [
        (None, None, "稳定版清单地址"),
        (
            Some("https://updates.example.invalid/stable.json".to_string()),
            None,
            "签名公钥",
        ),
    ] {
        let mut updater = ClientUpdater::new("1.0.0".into());
        updater.config.target = Some("darwin-aarch64".into());
        updater.config.endpoint = endpoint;
        updater.config.public_key = public_key;
        let result = updater.check(app.handle()).await;
        assert_eq!(result.status, "failed");
        assert!(result.available_version.is_none());
        let error = result.error.unwrap();
        assert!(
            error.contains(reason) && error.contains("请联系发布方"),
            "{error}"
        );
        assert!(!error.contains("TOKEN_MARKET_"), "{error}");
    }
}
