//! One-click external worker provisioning. Application bundles are read-only.
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[path = "qwenwork/contract.rs"]
mod contract;
pub use contract::ADAPTATION_REQUIRED;
use contract::{Contract, DISPATCH};

pub const MODEL_ENDPOINT: &str = "https://tokenmarket.local.invalid/v1/chat/completions";
pub const CREDENTIAL_PREFIX: &str = "tokenmarket-local-v2:";
pub const ROUTE_ENV: &str = "TOKENMARKET_QWENWORK_ROUTE";

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct HostRoute {
    pub fingerprint: String,
    pub default_model: String,
    pub port: u16,
    pub capability: String,
}
impl HostRoute {
    pub fn base_url(&self) -> String {
        format!(
            "http://127.0.0.1:{}/tokenmarket/{}",
            self.port, self.capability
        )
    }
    pub fn validate(&self) -> Result<()> {
        if self.port == 0
            || self.default_model.is_empty()
            || self.fingerprint.len() != 64
            || !self.fingerprint.bytes().all(|c| c.is_ascii_hexdigit())
            || !(40..=128).contains(&self.capability.len())
            || !self
                .capability
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c))
        {
            bail!("千问办公本地路由无效，请重新接入")
        }
        Ok(())
    }
}
const SDK_PATH: &str = "app.asar.unpacked/node_modules/@qoder-ai/qoder-agent-sdk";

#[derive(Clone)]
pub struct RuntimeInstallation {
    pub worker: PathBuf,
}

pub trait RuntimeInstaller: Send + Sync {
    fn prepare(
        &self,
        application: &Path,
        database: &Path,
        gateway: &str,
    ) -> Result<RuntimeInstallation>;
}
pub struct NativeRuntimeInstaller;

/// Structural revalidation only: never provisions assets, starts a process,
/// accesses a credential, or sends a request. Returns whether a refresh is needed.
pub(crate) fn assess_installed_runtime(application: &Path, database: &Path) -> Result<bool> {
    let resources = if application.is_dir() {
        application.join("Contents/Resources")
    } else {
        application
            .parent()
            .context("千问办公安装位置无效")?
            .join("resources")
    };
    require_worker_override(&resources)?;
    let source = fs::read_to_string(
        resources
            .join(SDK_PATH)
            .join("dist/_worker/qoder-worker-runtime.obf.mjs"),
    )?;
    let contract = Contract::recognize(&source)?;
    contract.probe_source(&source)?;
    crate::qwenwork::read_snapshot(database)?;
    let dotenv = crate::qwenwork::dotenv_path(database)?;
    let env = crate::qwenwork::parse_dotenv(Some(&fs::read(dotenv)?))?;
    let Some(worker) = env["QW_QODER_WORKER_RUNTIME_PATH"]
        .as_str()
        .filter(|path| is_managed_worker(path))
    else {
        return Ok(true);
    };
    let Some(root) = Path::new(worker).ancestors().nth(3) else {
        return Ok(true);
    };
    let saved = fs::read(root.join("compatibility.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
    Ok(saved
        .as_ref()
        .and_then(|value| value["worker_sha256"].as_str())
        != Some(fingerprint(&source).as_str()))
}

pub(crate) fn is_managed_worker(path: &str) -> bool {
    let path = path.replace('\\', "/");
    [
        "/.qwenworkcn/tokenmarket-direct-sdk/",
        "/.qwenworkcn/tokenmarket-local-sdk-v2/",
    ]
    .iter()
    .any(|prefix| path.contains(prefix))
}

pub(crate) fn is_managed_env_value(name: &str, value: &str) -> bool {
    match name {
        "QW_QODER_WORKER_RUNTIME_PATH" => is_managed_worker(value),
        ROUTE_ENV => {
            value.replace('\\', "/").contains("/.qwenworkcn/")
                && value.ends_with(".tokenmarket.json")
        }
        "QODERCN_SERVER_ENDPOINT" => value == "https://tokenmarket.local.invalid",
        "ENGINE_MAAS_MINT_URL" => url::Url::parse(value).is_ok_and(|url| {
            url.scheme() == "http"
                && url.host_str() == Some("127.0.0.1")
                && url.path().starts_with("/tokenmarket/")
                && url.path().ends_with("/api/desktop-sessions")
        }),
        _ => false,
    }
}

pub fn fingerprint(key: &str) -> String {
    format!("{:x}", Sha256::digest(key.as_bytes()))
}
pub fn placeholder(key: &str) -> String {
    format!("{CREDENTIAL_PREFIX}{}", fingerprint(key))
}

pub fn render_worker(source: &str, bridge: &Path, gateway: &str) -> Result<String> {
    let contract = Contract::recognize(source)?;
    let legacy = format!("{}/v1/chat/completions", gateway.trim_end_matches('/'));
    let quote = |s: &str| serde_json::to_string(s).expect("JSON string");
    let allowed = format!(
        "t.url==={}&&typeof t.api_key===\"string\"&&t.api_key.startsWith({})",
        quote(MODEL_ENDPOINT),
        quote(CREDENTIAL_PREFIX)
    );
    let assembly = format!("o=({allowed})?\"openai\":{}(t.provider,t.style??r?.custom_model_style),s=({allowed})?t.url:{}(t.provider,t.url);",contract.style,contract.url);
    // Host-job-token restoration needs official account/control APIs before any
    // inference dispatch. The managed blocking origin must apply to inference,
    // not authentication. Preserve SDK defaults/election and explicit user origins.
    let endpoint_override = contract.endpoint_override.replace(
        "if(n)return",
        "if(n&&!(globalThis.__tmQwenRoute&&n===\"https://tokenmarket.local.invalid\"&&(A===\"openapi\"||A===\"center\")))return",
    );
    // Legacy rows must fail before their credential can enter any cloud payload.
    let legacy_needle = &contract.legacy_needle;
    let reject_legacy = format!("if(t&&(t.url==={}||t.url==={}&&!({allowed})))throw new Error(\"Token Market configuration must be reconnected for local credential isolation\");{legacy_needle}", quote(&legacy),quote(MODEL_ENDPOINT));
    let bridge_js = include_str!("qwenwork/relay-worker.js")
        .replace(
            "__BRIDGE_BINARY__",
            &quote(bridge.to_str().context("本地代理路径无效")?),
        )
        .replace("__GATEWAY__", &quote(gateway))
        .replace(
            "__CONSOLE__",
            &quote(&crate::config::console_api_base_url()),
        )
        .replace("__BUILD_REQUEST__", &contract.builder)
        .replace("__OPEN_STREAM__", &contract.stream)
        .replace("__EMPTY_THINKING__", &contract.thinking);
    let bootstrap = include_str!("qwenwork/host-bootstrap.mjs")
        .replace(
            "__BRIDGE_BINARY__",
            &quote(bridge.to_str().context("本地代理路径无效")?),
        )
        .replace("__GATEWAY__", &quote(gateway))
        .replace(
            "__CONSOLE__",
            &quote(&crate::config::console_api_base_url()),
        );
    Ok(format!(
        "{bootstrap}\n{}",
        source
            .replace(&contract.endpoint_override, &endpoint_override)
            .replace(&contract.assembly, &assembly)
            .replace(legacy_needle, &reject_legacy)
            .replace(DISPATCH, &format!("{bridge_js}{DISPATCH}"))
    ))
}

pub fn render_probe_worker(source: &str, bridge: &Path, gateway: &str) -> Result<String> {
    Contract::recognize(source)?.probe_source(&render_worker(source, bridge, gateway)?)
}

/// SDK compatibility alone is insufficient if the app removes its worker override.
/// Read its packaged loader without modifying the archive or application signature.
fn packaged_main(resources: &Path) -> Result<Vec<u8>> {
    let mut archive = fs::File::open(resources.join("app.asar"))?;
    let mut first = [0u8; 8];
    archive.read_exact(&mut first)?;
    let header_size = u32::from_le_bytes(first[4..8].try_into().unwrap()) as usize;
    if !(8..=16 * 1024 * 1024).contains(&header_size) {
        bail!(ADAPTATION_REQUIRED)
    }
    let mut header = vec![0; header_size];
    archive.read_exact(&mut header)?;
    let json_size = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
    let index: serde_json::Value =
        serde_json::from_slice(header.get(8..8 + json_size).context(ADAPTATION_REQUIRED)?)?;
    let entry = &index["files"]["out"]["files"]["main"]["files"]["main.js"];
    let size = entry["size"].as_u64().context(ADAPTATION_REQUIRED)?;
    if size > 48 * 1024 * 1024 {
        bail!(ADAPTATION_REQUIRED)
    }
    let script = if entry["unpacked"] == true {
        fs::read(resources.join("app.asar.unpacked/out/main/main.js"))?
    } else {
        let offset = entry["offset"]
            .as_str()
            .context(ADAPTATION_REQUIRED)?
            .parse::<u64>()?;
        let start = 8u64 + header_size as u64;
        let end = start
            .checked_add(offset)
            .and_then(|v| v.checked_add(size))
            .context(ADAPTATION_REQUIRED)?;
        if end > archive.metadata()?.len() {
            bail!(ADAPTATION_REQUIRED)
        }
        archive.seek(SeekFrom::Start(start + offset))?;
        let mut bytes = vec![0; size as usize];
        archive.read_exact(&mut bytes)?;
        bytes
    };
    Ok(script)
}

fn require_worker_override(resources: &Path) -> Result<()> {
    let check = || -> Result<()> {
        let script = packaged_main(resources)?;
        for marker in [
            "QW_QODER_WORKER_RUNTIME_PATH",
            "ENGINE_MAAS_MINT_URL",
            "serverProvidedBaseUrl",
            "baseUrl:getEngineMaasApiBaseUrl()",
        ] {
            if !script.windows(marker.len()).any(|v| v == marker.as_bytes()) {
                bail!(ADAPTATION_REQUIRED)
            }
        }
        Ok(())
    };
    check().map_err(|error| error.context(ADAPTATION_REQUIRED))
}

struct ProbeDirectory(PathBuf);
impl Drop for ProbeDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn verify_contract(
    application: &Path,
    home: &Path,
    source: &str,
    gateway: &str,
    diagnostic: &mut crate::diagnostics::Operation,
) -> Result<()> {
    diagnostic.stage("probe_setup");
    let directory = home.join(format!(
        ".tokenmarket-probe-{}",
        crate::auth::create_pkce().code_verifier
    ));
    fs::create_dir(&directory)?;
    let directory = ProbeDirectory(directory);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&directory.0, fs::Permissions::from_mode(0o700))?;
    }
    let worker = directory.0.join("worker.mjs");
    fs::write(
        &worker,
        render_probe_worker(source, Path::new("tokenmarket-fixture-relay"), gateway)?,
    )?;
    let probe = directory.0.join("probe.mjs");
    fs::write(&probe, include_str!("qwenwork/compatibility-probe.mjs"))?;
    let program = if application.join("Contents/Info.plist").is_file() {
        let info = plist::Value::from_file(application.join("Contents/Info.plist"))?;
        let executable = info
            .as_dictionary()
            .and_then(|v| v.get("CFBundleExecutable"))
            .and_then(plist::Value::as_string)
            .context(ADAPTATION_REQUIRED)?;
        application.join("Contents/MacOS").join(executable)
    } else {
        application.to_path_buf()
    };
    let output = directory.0.join("result");
    let diagnostics = directory.0.join("diagnostics");
    diagnostic.stage("probe_spawn");
    let mut child = Command::new(program)
        .arg(&probe)
        .arg(&worker)
        .env("ELECTRON_RUN_AS_NODE", "1")
        .env_remove("NODE_OPTIONS")
        .env_remove("NODE_PATH")
        .env_remove(ROUTE_ENV)
        .stdin(Stdio::null())
        .stdout(fs::File::create(&output)?)
        .stderr(fs::File::create(&diagnostics)?)
        .spawn()
        .context("无法启动千问办公的接入自检，请检查安装是否完整后重试")?;
    diagnostic.stage("probe_wait");
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(status) = child.try_wait()? {
            if status.success()
                && fs::read_to_string(&output)?
                    .lines()
                    .any(|v| v == "TOKENMARKET_COMPAT_OK")
            {
                return Ok(());
            }
            diagnostic.stage(if status.success() {
                "probe_success_marker_missing"
            } else {
                "probe_exit_failed"
            });
            diagnostic.probe_exit(status.code(), &diagnostics);
            bail!(ADAPTATION_REQUIRED)
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            diagnostic.stage("probe_timeout");
            bail!(ADAPTATION_REQUIRED)
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn copy_assets(source: &Path, target: &Path) -> Result<()> {
    if fs::symlink_metadata(target).is_ok_and(|m| m.file_type().is_symlink()) {
        bail!("外置运行时目录是符号链接，请先确认实际路径")
    }
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        if entry.file_type()?.is_symlink() {
            bail!(ADAPTATION_REQUIRED)
        }
        let out = target.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_assets(&entry.path(), &out)?;
        } else {
            crate::connections::atomic_write(&out, &fs::read(entry.path())?)?;
            fs::set_permissions(&out, entry.metadata()?.permissions())?;
        }
    }
    Ok(())
}

impl RuntimeInstaller for NativeRuntimeInstaller {
    fn prepare(
        &self,
        application: &Path,
        database: &Path,
        gateway: &str,
    ) -> Result<RuntimeInstallation> {
        let home = crate::qwenwork::dotenv_path(database)?;
        let home = home.parent().context("设备目录无效")?;
        let log_directory = home
            .parent()
            .context("设备目录无效")?
            .join(".tokenmarket/desktop-connections/logs");
        let mut diagnostic = crate::diagnostics::Operation::new(
            log_directory,
            "qwenwork_runtime",
            crate::ToolId::QwenWork,
        );
        let result = self.prepare_runtime(application, database, gateway, &mut diagnostic);
        diagnostic.finish(&result);
        result
    }
}

impl NativeRuntimeInstaller {
    fn prepare_runtime(
        &self,
        application: &Path,
        database: &Path,
        gateway: &str,
        diagnostic: &mut crate::diagnostics::Operation,
    ) -> Result<RuntimeInstallation> {
        let resources = if application.join("Contents/Resources").is_dir() {
            application.join("Contents/Resources")
        } else {
            application
                .parent()
                .context("千问办公安装位置无效")?
                .join("resources")
        };
        let sdk = resources.join(SDK_PATH);
        diagnostic.stage("packaged_loader_contract");
        require_worker_override(&resources)?;
        let original = sdk.join("dist/_worker/qoder-worker-runtime.obf.mjs");
        diagnostic.stage("sdk_read");
        let source = fs::read_to_string(&original).map_err(|error| {
            let message = if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::InvalidData
            ) {
                ADAPTATION_REQUIRED
            } else {
                "无法读取千问办公 SDK，请检查文件访问权限后重试"
            };
            anyhow::Error::new(error).context(message)
        })?;
        let home = crate::qwenwork::dotenv_path(database)?
            .parent()
            .context("设备目录无效")?
            .to_path_buf();
        diagnostic.stage("bridge_read");
        let executable = std::env::current_exe().context("无法定位本地代理程序")?;
        let binary = fs::read(&executable).context("无法读取本地代理程序")?;
        let binary_id = format!("{:x}", Sha256::digest(&binary));
        let source_id = fingerprint(&source);
        let scope = fingerprint(&format!(
            "{}\n{}",
            gateway,
            crate::config::console_api_base_url()
        ));
        let root = home.join("tokenmarket-local-sdk-v2").join(format!(
            "{}-{}-{}",
            &source_id[..12],
            &binary_id[..12],
            &scope[..12]
        ));
        let bridge = root.join(if cfg!(windows) {
            "tokenmarket-bridge.exe"
        } else {
            "tokenmarket-bridge"
        });
        let worker = root.join("dist/_worker/qoder-worker-runtime.obf.mjs");
        // Complete compatibility checks before publishing any runtime file.
        diagnostic.stage("worker_contract");
        let patched = render_worker(&source, &bridge, gateway)?;
        verify_contract(application, &home, &source, gateway, diagnostic)?;
        diagnostic.stage("runtime_publish");
        fs::create_dir_all(worker.parent().unwrap())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
        }
        {
            crate::connections::atomic_write(&bridge, &binary)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&bridge, fs::Permissions::from_mode(0o700))?;
            }
        }
        for name in ["vendor", "proto", "builtin"] {
            let src = original.parent().unwrap().join(name);
            if src.is_dir() {
                copy_assets(&src, &worker.parent().unwrap().join(name))?;
            }
        }
        for (src, dest) in [
            (sdk.join("package.json"), root.join("package.json")),
            (
                original.parent().unwrap().join("runtime-info.json"),
                worker.parent().unwrap().join("runtime-info.json"),
            ),
            (
                sdk.join("dist/runtime-manifest.json"),
                root.join("dist/runtime-manifest.json"),
            ),
        ] {
            if src.exists() {
                crate::connections::atomic_write(&dest, &fs::read(src)?)?;
            }
        }
        crate::connections::atomic_write(&worker, patched.as_bytes())?;
        crate::connections::atomic_write(
            &root.join("compatibility.json"),
            &serde_json::to_vec_pretty(
                &serde_json::json!({"contract":"qoder-local-provider-v1","worker_sha256":source_id,"offline_probe_passed":true}),
            )?,
        )?;
        Ok(RuntimeInstallation { worker })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn installed_sdk_revalidation_distinguishes_refresh_from_unsupported_without_mutation() {
        let installed = Path::new("/Applications/QwenWorkCN.app").join(format!(
            "Contents/Resources/{SDK_PATH}/dist/_worker/qoder-worker-runtime.obf.mjs"
        ));
        if !installed.is_file() {
            return;
        }
        let source = fs::read_to_string(installed).unwrap();
        let root = tempfile::tempdir().unwrap();
        let app = root.path().join("QwenWorkCN.exe");
        let resources = root.path().join("resources");
        let worker = resources
            .join(SDK_PATH)
            .join("dist/_worker/qoder-worker-runtime.obf.mjs");
        fs::create_dir_all(worker.parent().unwrap()).unwrap();
        fs::write(&worker, &source).unwrap();
        let main = b"QW_QODER_WORKER_RUNTIME_PATH ENGINE_MAAS_MINT_URL serverProvidedBaseUrl baseUrl:getEngineMaasApiBaseUrl()";
        let index = serde_json::json!({"files":{"out":{"files":{"main":{"files":{"main.js":{"size":main.len(),"offset":"0"}}}}}}}).to_string();
        let mut archive = vec![0u8; 4];
        archive.extend_from_slice(&(8u32 + index.len() as u32).to_le_bytes());
        archive.extend_from_slice(&0u32.to_le_bytes());
        archive.extend_from_slice(&(index.len() as u32).to_le_bytes());
        archive.extend_from_slice(index.as_bytes());
        archive.extend_from_slice(main);
        fs::write(resources.join("app.asar"), archive).unwrap();
        let database = root
            .path()
            .join("Library/Application Support/QwenWorkCN/data/agents.db");
        fs::create_dir_all(database.parent().unwrap()).unwrap();
        rusqlite::Connection::open(&database)
            .unwrap()
            .execute_batch(include_str!("../tests/fixtures/qwenwork-schema.sql"))
            .unwrap();
        let managed = root
            .path()
            .join(".qwenworkcn/tokenmarket-local-sdk-v2/old/dist/_worker/worker.mjs");
        fs::create_dir_all(managed.parent().unwrap()).unwrap();
        let manifest = managed
            .ancestors()
            .nth(3)
            .unwrap()
            .join("compatibility.json");
        fs::write(
            &manifest,
            serde_json::json!({"worker_sha256":fingerprint(&source)}).to_string(),
        )
        .unwrap();
        let env = format!("QW_QODER_WORKER_RUNTIME_PATH={}\n", managed.display());
        let dotenv = root.path().join(".qwenworkcn/.env");
        fs::write(&dotenv, &env).unwrap();
        let before = fs::read(&database).unwrap();
        assert!(!assess_installed_runtime(&app, &database).unwrap());
        fs::write(
            &manifest,
            serde_json::json!({"worker_sha256":fingerprint("older SDK")}).to_string(),
        )
        .unwrap();
        assert!(assess_installed_runtime(&app, &database).unwrap());
        fs::write(&worker, "unsupported SDK contract").unwrap();
        assert!(assess_installed_runtime(&app, &database)
            .unwrap_err()
            .to_string()
            .contains("尚未适配"));
        assert_eq!(fs::read(&database).unwrap(), before);
        assert_eq!(fs::read_to_string(dotenv).unwrap(), env);
        assert!(!root.path().join(".tokenmarket").exists());
    }

    #[test]
    fn missing_packaged_loader_retains_stage_and_native_error_in_diagnostics() {
        let root = tempfile::tempdir().unwrap();
        let database = root
            .path()
            .join("Library/Application Support/QwenWorkCN/data/agents.db");
        let error = NativeRuntimeInstaller
            .prepare(
                &root.path().join("missing/QwenWorkCN.exe"),
                &database,
                "https://gateway.fixture",
            )
            .err()
            .expect("missing app archive must reject the runtime");
        assert_eq!(error.to_string(), ADAPTATION_REQUIRED);
        let log = fs::read_to_string(
            root.path()
                .join(".tokenmarket/desktop-connections/logs/connections.jsonl"),
        )
        .unwrap();
        let last: serde_json::Value = serde_json::from_str(log.lines().last().unwrap()).unwrap();
        assert_eq!(last["stage"], "packaged_loader_contract");
        assert_eq!(last["status"], "failed");
        assert_eq!(last["error"]["io_kind"], "NotFound");
        assert!(last["error"]["os_error"].is_number());
        assert!(!log.contains(&root.path().display().to_string()));
    }

    #[test]
    fn desktop_bootstrap_auxiliary_failure_preserves_primary_local_routing() {
        let app = Path::new("/Applications/QwenWorkCN.app");
        if !app.is_dir() {
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let bootstrap = root.path().join("bootstrap.mjs");
        fs::write(
            &bootstrap,
            include_str!("qwenwork/host-bootstrap.mjs")
                .replace("__BRIDGE_BINARY__", "\"fixture-native-bridge\"")
                .replace("__GATEWAY__", "\"https://gateway.fixture\"")
                .replace("__CONSOLE__", "\"https://console.fixture\""),
        )
        .unwrap();
        let output = Command::new(app.join("Contents/MacOS/QwenWorkCN"))
            .env("ELECTRON_RUN_AS_NODE", "1")
            .env_remove("NODE_OPTIONS")
            .env_remove("NODE_PATH")
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/helpers/qwenwork-host-bootstrap-probe.mjs"),
            )
            .arg(bootstrap)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("PASS:"));
    }
    #[test]
    fn reconnect_rebuilds_damaged_external_assets_from_the_current_installation() {
        let app = Path::new("/Applications/QwenWorkCN.app");
        if !app.is_dir() {
            return;
        }
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join(".qwenworkcn")).unwrap();
        let database = root
            .path()
            .join("Library/Application Support/QwenWorkCN/data/agents.db");
        let runtime = NativeRuntimeInstaller
            .prepare(app, &database, "https://gateway.fixture")
            .unwrap();
        let directory = runtime.worker.ancestors().nth(3).unwrap();
        let bridge = directory.join("tokenmarket-bridge");
        let info = runtime.worker.parent().unwrap().join("runtime-info.json");
        fs::write(&runtime.worker, "broken worker").unwrap();
        fs::write(&bridge, "broken relay").unwrap();
        fs::write(&info, "broken metadata").unwrap();
        let repaired = NativeRuntimeInstaller
            .prepare(app, &database, "https://gateway.fixture")
            .unwrap();
        assert_eq!(repaired.worker, runtime.worker);
        assert!(fs::read_to_string(repaired.worker)
            .unwrap()
            .contains("TOKENMARKET_LOCAL_RELAY_V2"));
        assert_eq!(
            fs::metadata(bridge).unwrap().len(),
            fs::metadata(std::env::current_exe().unwrap())
                .unwrap()
                .len()
        );
        assert_eq!(
            fs::read(info).unwrap(),
            fs::read(app.join(format!(
                "Contents/Resources/{SDK_PATH}/dist/_worker/runtime-info.json"
            )))
            .unwrap()
        );
    }
    #[test]
    fn installed_and_official_worker_builds_pass_the_same_offline_contract_probe() {
        let application = Path::new("/Applications/QwenWorkCN.app");
        if !application.is_dir() {
            return;
        }
        require_worker_override(&application.join("Contents/Resources")).unwrap();
        let mut inputs = vec![application.join(format!(
            "Contents/Resources/{SDK_PATH}/dist/_worker/qoder-worker-runtime.obf.mjs"
        ))];
        if let Some(root) = std::env::var_os("TOKEN_MARKET_QWENWORK_SDK_FIXTURES") {
            for entry in fs::read_dir(root).unwrap() {
                let file = entry.unwrap().path().join("qoder-worker-runtime.obf.mjs");
                if file.is_file() {
                    inputs.push(file);
                }
            }
        }
        if let Some(root) = std::env::var_os("TOKEN_MARKET_QWENWORK_WINDOWS_FIXTURE") {
            let resources = PathBuf::from(root).join("resources");
            require_worker_override(&resources).unwrap();
            inputs.push(resources.join(format!(
                "{SDK_PATH}/dist/_worker/qoder-worker-runtime.obf.mjs"
            )));
        }
        let root = tempfile::tempdir().unwrap();
        for path in inputs {
            let source = fs::read_to_string(&path).unwrap();
            let recognized = Contract::recognize(&source)
                .unwrap_or_else(|error| panic!("recognize {}: {error}", path.display()));
            recognized
                .probe_source(
                    &render_worker(
                        &source,
                        Path::new("fixture-bridge"),
                        "https://gateway.fixture",
                    )
                    .unwrap(),
                )
                .unwrap_or_else(|error| panic!("export {}: {error}", path.display()));
            let mut diagnostic = crate::diagnostics::Operation::new(
                root.path().join("logs"),
                "compatibility_test",
                crate::ToolId::QwenWork,
            );
            verify_contract(
                application,
                root.path(),
                &source,
                "https://gateway.fixture",
                &mut diagnostic,
            )
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            let unrelated_change =
                format!("/* SDK packaging metadata can change independently. */\n{source}");
            assert!(render_worker(
                &unrelated_change,
                Path::new("fixture-bridge"),
                "https://gateway.fixture"
            )
            .is_ok());
            let broken = source.replace(
                "parameters:{...r?.byok_parameters??{},api_key:t.api_key}",
                "parameters:{apiKey:t.api_key}",
            );
            assert!(render_worker(
                &broken,
                Path::new("fixture-bridge"),
                "https://gateway.fixture"
            )
            .unwrap_err()
            .to_string()
            .contains("联系管理员适配"));
            let broken_endpoint = source.replace(
                "reason:\"cn_server_endpoint_env\"",
                "reason:\"changed_endpoint_contract\"",
            );
            assert!(render_worker(
                &broken_endpoint,
                Path::new("fixture-bridge"),
                "https://gateway.fixture"
            )
            .is_err());
            println!("PASS compatible worker: {}", path.display());
        }
    }
}
