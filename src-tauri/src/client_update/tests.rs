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
use tauri::Listener;

const PUBLIC_KEY: &str = include_str!("../../tests/fixtures/client-update/public-key.txt");
const SIGNATURE: &str = include_str!("../../tests/fixtures/client-update/package.txt.sig");

struct ManifestServer {
    url: String,
    requests: Arc<Mutex<Vec<String>>>,
    done: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
    artifact: Arc<Mutex<(u16, Vec<u8>)>>,
    release_download: Arc<AtomicBool>,
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
        let body = manifest
            .to_string()
            .replace("$ARTIFACT", &url.replace("stable.json", "package"));
        let artifact = Arc::new(Mutex::new((
            200,
            include_bytes!("../../tests/fixtures/client-update/package.txt").to_vec(),
        )));
        let package = artifact.clone();
        let release_download = Arc::new(AtomicBool::new(true));
        let release = release_download.clone();
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
                        let is_package =
                            String::from_utf8_lossy(&buffer[..read]).starts_with("GET /package ");
                        log.lock().unwrap().push(
                            String::from_utf8_lossy(&buffer[..read])
                                .lines()
                                .next()
                                .unwrap_or("")
                                .to_string(),
                        );
                        while is_package
                            && !release.load(Ordering::SeqCst)
                            && !stopping.load(Ordering::SeqCst)
                        {
                            thread::sleep(Duration::from_millis(2));
                        }
                        let (status, response) = if is_package {
                            package.lock().unwrap().clone()
                        } else {
                            (200, body.as_bytes().to_vec())
                        };
                        if status == 0 {
                            continue;
                        } // Simulate a connection lost before headers.
                        write!(stream, "HTTP/1.1 {status} Response\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", response.len()).unwrap();
                        let _ = stream.write_all(&response);
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
            artifact,
            release_download,
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

fn download_fixture() -> (ManifestServer, tempfile::TempDir, ClientUpdater) {
    download_fixture_with("1.1.0", SIGNATURE.trim())
}

fn download_fixture_with(
    version: &str,
    signature: &str,
) -> (ManifestServer, tempfile::TempDir, ClientUpdater) {
    let server = ManifestServer::start(serde_json::json!({"version":version,"platforms":{
        "darwin-aarch64":{"url":"$ARTIFACT","signature":signature}
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
    (server, executable, updater)
}

#[tokio::test]
async fn confirmed_download_verifies_signed_bytes_without_installing_or_exposing_the_package() {
    let app = app();
    let (server, executable, updater) = download_fixture();
    assert_eq!(updater.check(app.handle()).await.status, "available");
    assert_eq!(
        *server.requests.lock().unwrap(),
        vec!["GET /stable.json HTTP/1.1"]
    );
    let result = updater.download(app.handle(), "1.1.0").await.unwrap();
    assert_eq!(result.status, "ready", "{result:?}");
    assert_eq!(result.available_version.as_deref(), Some("1.1.0"));
    let public = serde_json::to_string(&result).unwrap();
    assert!(!public.contains("http") && !public.contains(SIGNATURE.trim()));
    assert_eq!(std::fs::read_dir(executable.path()).unwrap().count(), 0);
    assert_eq!(
        *server.requests.lock().unwrap(),
        vec!["GET /stable.json HTTP/1.1", "GET /package HTTP/1.1"]
    );
}

#[tokio::test]
async fn download_rejects_untrusted_bytes_and_allows_a_deliberate_retry() {
    let app = app();
    let (server, executable, updater) = download_fixture();
    assert_eq!(updater.check(app.handle()).await.status, "available");
    let observed = Arc::new(Mutex::new(Vec::new()));
    let events = observed.clone();
    let listener = app.listen("desktop-client-update-status", move |event| {
        events.lock().unwrap().push(
            serde_json::from_str::<serde_json::Value>(event.payload()).unwrap()["status"]
                .as_str()
                .unwrap()
                .to_owned(),
        );
    });
    for (status, bytes) in [
        (0, Vec::new()),
        (503, b"unavailable".to_vec()),
        (200, b"altered package bytes".to_vec()),
    ] {
        *server.artifact.lock().unwrap() = (status, bytes);
        let result = updater.download(app.handle(), "1.1.0").await.unwrap();
        assert_eq!(result.status, "failed");
        assert!(result.error.unwrap().contains("重试下载"));
        assert_eq!(result.current_version, "1.0.0");
        assert!(!observed
            .lock()
            .unwrap()
            .iter()
            .any(|status| status == "ready"));
        assert_eq!(std::fs::read_dir(executable.path()).unwrap().count(), 0);
    }
    // A completed transfer is explicitly reported as verifying, never as trusted.
    assert!(observed
        .lock()
        .unwrap()
        .iter()
        .any(|status| status == "verifying"));
    *server.artifact.lock().unwrap() = (
        200,
        include_bytes!("../../tests/fixtures/client-update/package.txt").to_vec(),
    );
    assert_eq!(
        updater
            .download(app.handle(), "1.1.0")
            .await
            .unwrap()
            .status,
        "ready"
    );
    app.unlisten(listener);
}

#[tokio::test]
async fn wrong_missing_or_version_mismatched_signatures_never_produce_ready() {
    let app = app();
    for (version, signature) in [
        (
            "1.1.0",
            include_str!("../../tests/fixtures/client-update/wrong-key.sig").trim(),
        ),
        ("1.1.0", "malformed-signature"),
        ("1.1.0", ""),
        ("1.2.0", SIGNATURE.trim()),
    ] {
        let (server, executable, updater) = download_fixture_with(version, signature);
        let checked = updater.check(app.handle()).await;
        if checked.status == "available" {
            let result = updater.download(app.handle(), version).await.unwrap();
            assert_eq!(result.status, "failed", "{version}: {result:?}");
            assert!(result.error.is_some());
        } else {
            assert_eq!(checked.status, "failed");
            assert!(updater.download(app.handle(), version).await.is_err());
            assert_eq!(server.requests.lock().unwrap().len(), 1);
        }
        assert_ne!(updater.status().status, "ready");
        assert_eq!(std::fs::read_dir(executable.path()).unwrap().count(), 0);
    }
}

#[tokio::test]
async fn checks_cannot_replace_an_inflight_or_verified_candidate() {
    let app = app();
    let (server, _executable, updater) = download_fixture();
    assert_eq!(updater.check(app.handle()).await.status, "available");
    server.release_download.store(false, Ordering::SeqCst);
    let download = updater.download(app.handle(), "1.1.0");
    let competing_check = async {
        for _ in 0..100 {
            if updater.status().status == "downloading" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(updater.status().status, "downloading");
        let checked = tokio::time::timeout(Duration::from_secs(1), updater.check(app.handle()))
            .await
            .unwrap();
        assert_eq!(checked.status, "downloading");
        assert_eq!(checked.available_version.as_deref(), Some("1.1.0"));
        server.release_download.store(true, Ordering::SeqCst);
    };
    let (downloaded, _) = tokio::join!(download, competing_check);
    assert_eq!(downloaded.unwrap().status, "ready");
    assert_eq!(updater.check(app.handle()).await.status, "ready");
    assert_eq!(
        updater
            .download(app.handle(), "1.1.0")
            .await
            .unwrap()
            .status,
        "ready"
    );
    assert_eq!(
        *server.requests.lock().unwrap(),
        vec!["GET /stable.json HTTP/1.1", "GET /package HTTP/1.1"]
    );
}

#[tokio::test]
async fn consent_for_an_obsolete_candidate_cannot_download_another_version() {
    let app = app();
    let (first, _executable, mut updater) = download_fixture();
    assert!(updater.download(app.handle(), "1.1.0").await.is_err());
    assert!(first.requests.lock().unwrap().is_empty());
    assert_eq!(
        updater
            .check(app.handle())
            .await
            .available_version
            .as_deref(),
        Some("1.1.0")
    );
    let (newer, _newer_executable, _) = download_fixture_with("1.2.0", SIGNATURE.trim());
    updater.config.endpoint = Some(newer.url.clone());
    assert_eq!(
        updater
            .check(app.handle())
            .await
            .available_version
            .as_deref(),
        Some("1.2.0")
    );
    assert!(updater
        .download(app.handle(), "1.1.0")
        .await
        .unwrap_err()
        .contains("重新检查"));
    assert_eq!(updater.status().status, "available");
    assert_eq!(updater.status().available_version.as_deref(), Some("1.2.0"));
    assert_eq!(
        *first.requests.lock().unwrap(),
        vec!["GET /stable.json HTTP/1.1"]
    );
    assert_eq!(
        *newer.requests.lock().unwrap(),
        vec!["GET /stable.json HTTP/1.1"]
    );
}
