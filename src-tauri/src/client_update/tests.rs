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

#[tokio::test]
async fn later_suppresses_startup_reminders_until_a_new_native_process() {
    let app = app();
    let server = ManifestServer::start(serde_json::json!({"version":"1.1.0","platforms":{
        "darwin-aarch64":{"url":"https://updates.example.invalid/macos.tar.gz","signature":SIGNATURE.trim()}
    }}));
    let preferences = tempfile::tempdir().unwrap();
    let executable = tempfile::tempdir().unwrap();
    let build = || {
        let mut updater = ClientUpdater::with_reminder_path(
            "1.0.0".into(),
            preferences.path().join("updates.json"),
        );
        updater.config.endpoint = Some(server.url.clone());
        updater.config.public_key = Some(PUBLIC_KEY.trim().into());
        updater.config.target = Some("darwin-aarch64".into());
        updater.config.allow_loopback_http = true;
        updater.config.executable_path = Some(
            executable
                .path()
                .join("Token Market.app/Contents/MacOS/Token Market"),
        );
        updater
    };
    let updater = build();
    assert!(updater.startup_check(app.handle()).await.remind);
    assert!(!updater.later().remind);
    assert!(
        !updater.status().remind,
        "a renderer status read must retain native Later"
    );
    assert_eq!(
        updater
            .check(app.handle())
            .await
            .available_version
            .as_deref(),
        Some("1.1.0")
    );
    assert!(
        !updater.status().remind,
        "manual discovery keeps process deferral"
    );
    assert!(!updater.startup_check(app.handle()).await.remind);
    assert!(build().startup_check(app.handle()).await.remind);
    assert_eq!(
        server.requests.lock().unwrap().len(),
        3,
        "no repeated startup or periodic checks in one process"
    );
}

#[tokio::test]
async fn ignored_exact_version_persists_but_manual_and_next_version_remain_available() {
    let app = app();
    let preferences = tempfile::tempdir().unwrap();
    let executable = tempfile::tempdir().unwrap();
    let server = |version| {
        ManifestServer::start(serde_json::json!({"version":version,"platforms":{
            "darwin-aarch64":{"url":"https://updates.example.invalid/macos.tar.gz","signature":SIGNATURE.trim()}
        }}))
    };
    let build = |server: &ManifestServer| {
        let mut updater = ClientUpdater::with_reminder_path(
            "1.0.0".into(),
            preferences.path().join("updates.json"),
        );
        updater.config.endpoint = Some(server.url.clone());
        updater.config.public_key = Some(PUBLIC_KEY.trim().into());
        updater.config.target = Some("darwin-aarch64".into());
        updater.config.allow_loopback_http = true;
        updater.config.executable_path = Some(
            executable
                .path()
                .join("Token Market.app/Contents/MacOS/Token Market"),
        );
        updater
    };
    let first = server("1.1.0");
    let updater = build(&first);
    assert!(updater.startup_check(app.handle()).await.remind);
    assert!(!updater.ignore_version("1.1.0").unwrap().remind);
    let restarted = build(&first);
    let ignored = restarted.startup_check(app.handle()).await;
    assert_eq!(ignored.status, "available");
    assert!(!ignored.remind);
    let manual = restarted.check(app.handle()).await;
    assert_eq!(manual.available_version.as_deref(), Some("1.1.0"));
    assert!(!manual.automatic);
    assert!(
        !build(&first).startup_check(app.handle()).await.remind,
        "manual checking preserves exact-version ignore"
    );
    let next = server("1.2.0");
    assert!(build(&next).startup_check(app.handle()).await.remind);
    assert!(
        restarted.ignore_version("9.9.9").is_err(),
        "stale renderer must not ignore an unseen version"
    );
}

#[tokio::test]
async fn downloading_a_startup_release_clears_automatic_reminder_metadata() {
    let app = app();
    let (_server, _executable, updater) = download_fixture();
    let offered = updater.startup_check(app.handle()).await;
    assert!(offered.automatic && offered.remind);
    let downloaded = updater.download(app.handle(), "1.1.0").await.unwrap();
    assert_eq!(downloaded.status, "ready");
    assert!(!downloaded.automatic && !downloaded.remind);
    let checked = updater.startup_check(app.handle()).await;
    assert_eq!(checked.status, "ready");
    assert!(!checked.automatic && !checked.remind);
}

/// This separate release acceptance seam requires the built verifier command.
/// Run the exact documented command after `cargo build --example verify-update-artifacts`.
#[tokio::test]
#[ignore = "requires built release verifier; run the documented release-distribution check"]
async fn release_distribution_client_downloads_and_verifies_both_cli_published_targets() {
    use std::{
        io::BufRead,
        process::{Command, Stdio},
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let mut server = Command::new("python3")
        .arg(root.join("tests/helpers/update-distribution-fixture.py"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let stdout = server.stdout.take().unwrap();
    let mut reader = std::io::BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    assert!(
        !line.is_empty(),
        "fixture publisher must start before client checks"
    );
    let service: serde_json::Value = serde_json::from_str(&line).unwrap();
    let app = app();
    for target in ["darwin-aarch64", "windows-x86_64-nsis"] {
        let executable = tempfile::tempdir().unwrap();
        let mut updater = ClientUpdater::new("1.0.0".into());
        updater.config.endpoint = Some(service["endpoint"].as_str().unwrap().into());
        updater.config.public_key = Some(PUBLIC_KEY.trim().into());
        updater.config.target = Some(target.into());
        updater.config.allow_loopback_http = true;
        updater.config.executable_path = Some(
            executable
                .path()
                .join("Token Market.app/Contents/MacOS/Token Market"),
        );
        let checked = updater.check(app.handle()).await;
        assert_eq!(checked.status, "available", "{target}: {checked:?}");
        let downloaded = updater.download(app.handle(), "1.1.0").await.unwrap();
        assert_eq!(downloaded.status, "ready", "{target}: {downloaded:?}");
        assert_eq!(
            std::fs::read_dir(executable.path()).unwrap().count(),
            0,
            "release verification must never install"
        );
    }
    server.stdin.take().unwrap().write_all(b"stop\n").unwrap();
    assert!(server.wait().unwrap().success());
}

struct FixtureInstaller {
    home: std::path::PathBuf,
    fail_launch: AtomicBool,
    launched: Arc<std::sync::atomic::AtomicUsize>,
}
struct FixtureHandoff {
    _guard: tokenmarket_core::client_update::ClientUpdateGuard,
    fail_launch: bool,
    launched: Arc<std::sync::atomic::AtomicUsize>,
}
impl ClientUpdateHandoff for FixtureHandoff {
    fn commit(self: Box<Self>) -> Result<(), String> {
        if self.fail_launch {
            return Err("安装器启动失败，请重试".into());
        }
        self.launched.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}
impl ClientUpdateInstaller for FixtureInstaller {
    fn prepare(
        &self,
        update: VerifiedClientUpdate,
    ) -> Result<Box<dyn ClientUpdateHandoff>, String> {
        let guard = tokenmarket_core::client_update::ClientUpdateGuard::acquire(
            &self.home,
            "https://fixture.invalid",
        )
        .map_err(|error| error.to_string())?;
        assert_eq!(update.update.version, "1.1.0");
        assert_eq!(
            update.bytes,
            include_bytes!("../../tests/fixtures/client-update/package.txt")
        );
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(
                self.home
                    .join(".tokenmarket/desktop-connections/operation.lock"),
            )
            .unwrap();
        assert!(
            lock.try_lock().is_err(),
            "handoff must still exclude mutations"
        );
        Ok(Box::new(FixtureHandoff {
            _guard: guard,
            fail_launch: self.fail_launch.load(Ordering::SeqCst),
            launched: self.launched.clone(),
        }))
    }
}

#[tokio::test]
async fn guarded_install_refuses_busy_work_retains_verified_retry_and_reports_handoff_not_completion(
) {
    let app = app();
    let (_server, _executable, mut updater) = download_fixture();
    let home = tempfile::tempdir().unwrap();
    let installer = Arc::new(FixtureInstaller {
        home: home.path().into(),
        fail_launch: AtomicBool::new(true),
        launched: Default::default(),
    });
    assert!(!updater.status().install_available);
    assert!(updater
        .install(app.handle(), "1.1.0")
        .await
        .unwrap_err()
        .contains("暂不可用"));
    updater = updater.with_installer(installer.clone());
    assert!(
        updater.install(app.handle(), "1.1.0").await.is_err(),
        "unverified bytes are never handed off"
    );
    updater.check(app.handle()).await;
    assert_eq!(
        updater
            .download(app.handle(), "1.1.0")
            .await
            .unwrap()
            .status,
        "ready"
    );
    let directory = home.path().join(".tokenmarket/desktop-connections");
    std::fs::create_dir_all(&directory).unwrap();
    let active = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join("operation.lock"))
        .unwrap();
    active.try_lock().unwrap();
    let busy = updater.install(app.handle(), "1.1.0").await.unwrap();
    assert_eq!(busy.status, "ready");
    assert!(busy.error.unwrap().contains("连接"));
    assert_eq!(installer.launched.load(Ordering::SeqCst), 0);
    drop(active);
    let failed = updater.install(app.handle(), "1.1.0").await.unwrap();
    assert_eq!(failed.status, "ready");
    assert!(failed.error.unwrap().contains("启动失败"));
    installer.fail_launch.store(false, Ordering::SeqCst);
    let result = updater.install(app.handle(), "1.1.0").await.unwrap();
    assert_eq!(result.status, "handoff");
    assert_eq!(installer.launched.load(Ordering::SeqCst), 1);
    assert!(result.error.is_none());
    assert_eq!(updater.check(app.handle()).await.status, "handoff");
    assert_eq!(
        updater
            .download(app.handle(), "1.1.0")
            .await
            .unwrap()
            .status,
        "handoff",
        "repeated download cannot erase a committed handoff"
    );
}
