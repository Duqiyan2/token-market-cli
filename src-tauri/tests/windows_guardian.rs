//! Real Windows guardian/parent/installer process lifetime. No installed app,
//! credentials, paid request or user files are used. Run with cargo test --test windows_guardian.
#![cfg(windows)]
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    os::windows::{
        fs::OpenOptionsExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use tokenmarket_core::client_update::{ClientUpdateJournal, ClientUpdatePhase};
use windows_sys::Win32::{
    Foundation::{FILETIME, WAIT_OBJECT_0},
    System::Threading::{
        GetCurrentProcess, GetProcessTimes, OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
    },
};

fn creation_time() -> u64 {
    let mut a: FILETIME = unsafe { std::mem::zeroed() };
    let mut b = a;
    let mut c = a;
    let mut d = a;
    assert_ne!(
        unsafe { GetProcessTimes(GetCurrentProcess(), &mut a, &mut b, &mut c, &mut d) },
        0
    );
    ((a.dwHighDateTime as u64) << 32) | a.dwLowDateTime as u64
}
fn until(reader: &mut impl BufRead, text: &str) -> String {
    loop {
        let mut line = String::new();
        assert_ne!(
            reader.read_line(&mut line).unwrap(),
            0,
            "guardian exited before {text}"
        );
        if line.contains(text) {
            return line;
        }
    }
}
fn wait_file(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !path.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(path.exists(), "missing {}", path.display());
}
#[test]
fn fixture_parent() {
    let Some(root) = std::env::var_os("WINDOWS_GUARDIAN_PARENT") else {
        return;
    };
    let root = PathBuf::from(root);
    let package = root.join("stage/installer.exe");
    let pinned = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&package)
        .unwrap();
    let digest = format!("{:x}", Sha256::digest(fs::read(&package).unwrap()));
    let mut guardian = Command::new(root.join("stage/tokenmarket-update-guardian.exe"))
        .arg("--tokenmarket-update-guardian-v1")
        .env("WINDOWS_INSTALLER_FIXTURE", &root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let mut input = guardian.stdin.take().unwrap();
    let mut reader = BufReader::new(guardian.stdout.take().unwrap());
    let bootstrap = serde_json::json!({"parent":{"pid":std::process::id(),"created":creation_time()},"plan":{
        "home":root,"origin":"https://fixture.invalid","from_version":"1.0.0","target_version":"99.0.0",
        "application":root.join("应用 Token Market/app.exe"),"package":package,"package_sha256":digest,
        "guardian":{"pid":guardian.id(),"created":0}}});
    writeln!(input, "{bootstrap}").unwrap();
    let ready = until(&mut reader, "ready");
    print!("{ready}");
    std::io::stdout().flush().unwrap();
    drop(pinned); // actual helper already owns its own immutable stage handle
    let mut command = String::new();
    std::io::stdin().read_line(&mut command).unwrap();
    if command.trim() == "cancel" {
        drop(input);
        let _ = guardian.wait();
        return;
    }
    input.write_all(command.as_bytes()).unwrap();
    input.flush().unwrap();
    let committed = until(&mut reader, "committed");
    print!("{committed}");
    std::io::stdout().flush().unwrap();
    command.clear();
    std::io::stdin().read_line(&mut command).unwrap();
    // Exit the exact parent process. Guardian holds OpenProcess+creation-time identity.
    // Preserve its output for the driver after this process exits.
    std::thread::spawn(move || {
        let mut rest = String::new();
        let _ = reader.read_to_string(&mut rest);
    });
}
#[test]
fn fixture_legacy_mutator() {
    let Some(root) = std::env::var_os("WINDOWS_GUARDIAN_PROBE") else {
        return;
    };
    let root = PathBuf::from(root);
    let mut locked = 0;
    let mut acquired = Vec::new();
    for path in [
        ".tokenmarket/desktop-connections/operation.lock",
        ".tokenmarket/install-locks/claude-code.lock",
        ".tokenmarket/install-locks/codex.lock",
        ".tokenmarket/install-locks/workbuddy.lock",
        ".tokenmarket/install-locks/deepseek-harness.lock",
        ".tokenmarket/install-locks/openclaw.lock",
        ".tokenmarket/install-locks/opencode.lock",
        ".tokenmarket/install-locks/grok-build.lock",
        ".tokenmarket/install-locks/hermes.lock",
        ".tokenmarket/install-locks/pi.lock",
        ".tokenmarket/install-locks/minimax-code.lock",
        ".tokenmarket/install-locks/oh-my-pi.lock",
        ".tokenmarket/install-locks/kimi-code.lock",
        ".tokenmarket/install-locks/dumate.lock",
        ".tokenmarket/client-login/65622f37e6d4c2b67cd1f675e3c3a89982a65a38d41a4157161a27c0297eeb80/operation.lock",
        ".tokenmarket/client-login/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/operation.lock",
    ] {
        let file = fs::OpenOptions::new().read(true).write(true).open(root.join(path)).unwrap();
        if file.try_lock().is_ok() { acquired.push(file); } else { locked += 1; }
    }
    println!(
        "{}",
        if locked == 16 {
            "BUSY"
        } else if locked == 0 {
            "MUTATED"
        } else {
            "PARTIAL"
        }
    );
}
fn probe(root: &Path, blocked: bool) {
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "fixture_legacy_mutator", "--nocapture"])
        .env("WINDOWS_GUARDIAN_PROBE", root)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stdout).contains(if blocked { "BUSY" } else { "MUTATED" })
    );
}
struct Fixture {
    root: tempfile::TempDir,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join(".tokenmarket/client-login/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")).unwrap();
        fs::create_dir(root.path().join("stage")).unwrap();
        fs::create_dir(root.path().join("应用 Token Market")).unwrap();
        let app = env!("CARGO_BIN_EXE_app");
        fs::copy(
            app,
            root.path().join("stage/tokenmarket-update-guardian.exe"),
        )
        .unwrap();

        // A native controllable installer keeps its own process alive until the
        // driver releases it. Only these fresh synthetic files are accessed.
        let source = r#"use std::{env,fs,thread,time::Duration};
            #[link(name = "kernel32")] unsafe extern "system" { fn GetCommandLineW() -> *const u16; }
            fn raw_command_line() -> String { unsafe { let ptr = GetCommandLineW(); let mut len = 0; while *ptr.add(len) != 0 {len += 1;} String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len)) } }
            fn main() {
            let root = std::path::PathBuf::from(env::var_os("WINDOWS_INSTALLER_FIXTURE").unwrap());
            if env::args().len() == 1 { fs::write(root.join("relaunched"), b"old version").unwrap(); return; }
            fs::write(root.join("arguments"), raw_command_line()).unwrap();
            fs::write(root.join("installer-started"), b"started").unwrap();
            while !root.join("finish-installer").exists() { thread::sleep(Duration::from_millis(10)); }
            if root.join("nonzero").exists() { std::process::exit(2); }
        }"#;
        fs::write(root.path().join("installer.rs"), source).unwrap();
        assert!(Command::new("rustc")
            .arg(root.path().join("installer.rs"))
            .arg("-o")
            .arg(root.path().join("stage/installer.exe"))
            .status()
            .unwrap()
            .success());
        fs::copy(
            root.path().join("stage/installer.exe"),
            root.path().join("应用 Token Market/app.exe"),
        )
        .unwrap();
        Self { root }
    }
    fn parent(
        &self,
    ) -> (
        Child,
        BufReader<std::process::ChildStdout>,
        String,
        OwnedHandle,
    ) {
        let mut parent = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "fixture_parent", "--nocapture"])
            .env("WINDOWS_GUARDIAN_PARENT", self.root.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut reader = BufReader::new(parent.stdout.take().unwrap());
        let ready: serde_json::Value = serde_json::from_str(&until(&mut reader, "ready")).unwrap();
        {
            let handle = unsafe {
                OpenProcess(
                    PROCESS_SYNCHRONIZE,
                    0,
                    ready["guardian"]["pid"].as_u64().unwrap() as u32,
                )
            };
            assert!(!handle.is_null());
            let handle = unsafe { OwnedHandle::from_raw_handle(handle) };
            (
                parent,
                reader,
                ready["nonce"].as_str().unwrap().into(),
                handle,
            )
        }
    }
}
#[test]
fn exact_parent_exit_keeps_legacy_exclusion_and_staged_bytes_pinned_through_native_installer_exit()
{
    let fixture = Fixture::new();
    let root = fixture.root.path();
    let (mut parent, mut reader, nonce, guardian) = fixture.parent();
    probe(root, true);
    assert!(OpenOptions::new()
        .write(true)
        .open(root.join("stage/installer.exe"))
        .is_err());
    writeln!(parent.stdin.as_mut().unwrap(), "{{\"commit\":\"{nonce}\"}}").unwrap();
    until(&mut reader, "committed");
    assert!(!root.join("installer-started").exists());
    probe(root, true);
    writeln!(parent.stdin.take().unwrap(), "exit").unwrap();
    assert!(parent.wait().unwrap().success());
    wait_file(&root.join("installer-started"));
    probe(root, true);
    assert!(OpenOptions::new()
        .write(true)
        .open(root.join("stage/installer.exe"))
        .is_err());
    let arguments = fs::read_to_string(root.join("arguments")).unwrap();
    assert!(arguments.contains(" /P /UPDATE /D="), "{arguments}");
    assert_eq!(
        arguments.split("/D=").last().unwrap(),
        root.join("应用 Token Market").display().to_string()
    );
    assert!(arguments.contains("应用 Token Market"));
    assert!(!arguments.contains("/R"));
    fs::write(root.join("finish-installer"), b"finish").unwrap();
    assert_eq!(
        unsafe { WaitForSingleObject(guardian.as_raw_handle(), 20_000) },
        WAIT_OBJECT_0
    );
    let deadline = Instant::now() + Duration::from_secs(20);
    while ClientUpdateJournal::read(root).unwrap().unwrap().phase()
        == ClientUpdatePhase::InstallerRunning
        && Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut journal = ClientUpdateJournal::read(root).unwrap().unwrap();
    assert_eq!(journal.phase(), ClientUpdatePhase::AwaitingStartup);
    assert!(journal
        .acknowledge_startup("1.0.0", &root.join("应用 Token Market/app.exe"))
        .is_err());
    probe(root, false);
}
#[test]
fn cancellation_before_commit_does_not_launch_and_permits_explicit_retry() {
    let fixture = Fixture::new();
    let root = fixture.root.path();
    let (mut parent, mut reader, _, _guardian) = fixture.parent();
    writeln!(parent.stdin.take().unwrap(), "cancel").unwrap();
    let mut rest = String::new();
    reader.read_to_string(&mut rest).unwrap();
    assert!(parent.wait().unwrap().success(), "{rest}");
    assert!(!root.join("installer-started").exists());
    probe(root, false);
    let (mut parent, mut reader, _, _guardian) = fixture.parent();
    writeln!(parent.stdin.take().unwrap(), "cancel").unwrap();
    let mut rest = String::new();
    reader.read_to_string(&mut rest).unwrap();
    assert!(parent.wait().unwrap().success(), "{rest}");
}

#[cfg(feature = "windows-update-fixtures")]
mod native_preparation {
    use super::*;
    use app_lib::client_update::windows::fixtures::NativeFixture;
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    const KEY: &str = include_str!("fixtures/client-update/public-key.txt");
    const SIGNATURE: &str = include_str!("fixtures/client-update/package.txt.sig");
    struct Server {
        url: String,
        stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
        worker: Option<std::thread::JoinHandle<()>>,
    }
    impl Server {
        fn new() -> Self {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let url = format!("http://{}/stable.json", listener.local_addr().unwrap());
            let manifest = serde_json::json!({"version":"1.1.0","platforms":{"windows-x86_64-nsis":{"url":url.replace("stable.json","package"),"signature":SIGNATURE.trim()}}}).to_string();
            let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let stopping = stop.clone();
            let worker = std::thread::spawn(move || {
                while !stopping.load(std::sync::atomic::Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            let mut request = [0u8; 4096];
                            let n = stream.read(&mut request).unwrap();
                            let bytes = if String::from_utf8_lossy(&request[..n])
                                .starts_with("GET /package ")
                            {
                                include_bytes!("fixtures/client-update/package.txt").as_slice()
                            } else {
                                manifest.as_bytes()
                            };
                            write!(stream,"HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",bytes.len()).unwrap();
                            stream.write_all(bytes).unwrap();
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(5))
                        }
                        Err(error) => panic!("{error}"),
                    }
                }
            });
            Self {
                url,
                stop,
                worker: Some(worker),
            }
        }
    }
    impl Drop for Server {
        fn drop(&mut self) {
            self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
            self.worker.take().unwrap().join().unwrap();
        }
    }
    struct Registry {
        base: String,
        user: String,
        machine: String,
    }
    impl Registry {
        fn new(directory: &Path) -> Self {
            let base = format!(
                r"Software\TokenMarketNativeFixture\{:032x}",
                rand::random::<u128>()
            );
            let user = format!(r"{base}\UserUninstall");
            let machine = format!(r"{base}\SystemUninstall");
            RegKey::predef(HKEY_CURRENT_USER)
                .create_subkey(&machine)
                .unwrap();
            let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
                .create_subkey(format!(r"{user}\Token Market"))
                .unwrap();
            key.set_value("DisplayName", &"Token Market").unwrap();
            key.set_value(
                "TokenMarketUpdatePolicy",
                &"nsis-currentUser-v1-bundler2.10.4",
            )
            .unwrap();
            key.set_value(
                "TokenMarketUpdateDirectory",
                &directory.display().to_string(),
            )
            .unwrap();
            key.set_value("InstallLocation", &format!("\"{}\"", directory.display()))
                .unwrap();
            key.set_value(
                "UninstallString",
                &format!("\"{}\"", directory.join("uninstall.exe").display()),
            )
            .unwrap();
            Self {
                base,
                user,
                machine,
            }
        }
        fn key(&self) -> RegKey {
            RegKey::predef(HKEY_CURRENT_USER)
                .open_subkey_with_flags(
                    format!(r"{}\Token Market", self.user),
                    winreg::enums::KEY_ALL_ACCESS,
                )
                .unwrap()
        }
    }
    impl Drop for Registry {
        fn drop(&mut self) {
            RegKey::predef(HKEY_CURRENT_USER)
                .delete_subkey_all(&self.base)
                .unwrap();
        }
    }
    fn app() -> tauri::App<tauri::test::MockRuntime> {
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.package_info_mut().version = "1.0.0".parse().unwrap();
        context.config_mut().plugins.0.insert("updater".into(),serde_json::json!({"pubkey":"","requireSignedVersion":true,"dangerousInsecureTransportProtocol":true}));
        tauri::test::mock_builder()
            .plugin(tauri_plugin_updater::Builder::new().build())
            .build(context)
            .unwrap()
    }
    struct Installed {
        home: tempfile::TempDir,
        application: PathBuf,
        registry: Registry,
        server: Server,
        app: tauri::App<tauri::test::MockRuntime>,
    }
    impl Installed {
        fn new() -> Self {
            let home = tempfile::tempdir().unwrap();
            let directory = home.path().join("应用 Token Market");
            fs::create_dir(&directory).unwrap();
            let application = directory.join("app.exe");
            fs::copy(env!("CARGO_BIN_EXE_app"), &application).unwrap();
            fs::write(
                directory.join("uninstall.exe"),
                b"synthetic uninstaller, never executed",
            )
            .unwrap();
            let registry = Registry::new(&directory);
            Self {
                home,
                application,
                registry,
                server: Server::new(),
                app: app(),
            }
        }
        fn client(&self, family: &str) -> NativeFixture<tauri::test::MockRuntime> {
            NativeFixture::new(
                self.app.handle(),
                self.home.path().to_owned(),
                self.application.clone(),
                self.registry.user.clone(),
                self.registry.machine.clone(),
                family,
                self.server.url.clone(),
                KEY.trim().into(),
            )
        }
    }
    #[tokio::test]
    async fn signed_native_prepare_pins_actual_private_stage_and_drop_cancels_without_installing() {
        let installed = Installed::new();
        let client = installed.client("nsis-currentUser");
        let handoff = client.prepare(false).await.unwrap();
        let packages = client.staged_packages();
        assert_eq!(packages.len(), 1);
        let package = &packages[0];
        assert_eq!(
            fs::read(package).unwrap(),
            include_bytes!("fixtures/client-update/package.txt")
        );
        assert!(OpenOptions::new().write(true).open(package).is_err());
        assert!(fs::remove_file(package).is_err());
        assert!(fs::rename(
            package.parent().unwrap(),
            package.parent().unwrap().with_extension("moved")
        )
        .is_err());
        // Query the real OS security descriptor at the filesystem boundary.
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::{
            Foundation::LocalFree,
            Security::{
                Authorization::{
                    ConvertSecurityDescriptorToStringSecurityDescriptorW, GetNamedSecurityInfoW,
                    SE_FILE_OBJECT,
                },
                DACL_SECURITY_INFORMATION,
            },
        };
        let path: Vec<u16> = package
            .parent()
            .unwrap()
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        let mut security = std::ptr::null_mut();
        assert_eq!(
            unsafe {
                GetNamedSecurityInfoW(
                    path.as_ptr(),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    &mut security,
                )
            },
            0
        );
        let mut text = std::ptr::null_mut();
        assert_ne!(
            unsafe {
                ConvertSecurityDescriptorToStringSecurityDescriptorW(
                    security,
                    1,
                    DACL_SECURITY_INFORMATION,
                    &mut text,
                    std::ptr::null_mut(),
                )
            },
            0
        );
        let sddl = unsafe {
            let mut len = 0;
            while *text.add(len) != 0 {
                len += 1;
            }
            String::from_utf16_lossy(std::slice::from_raw_parts(text, len))
        };
        unsafe {
            LocalFree(text.cast());
            LocalFree(security);
        }
        assert!(sddl.starts_with("D:P"), "{sddl}");
        let aces: Vec<_> = sddl.split('(').skip(1).collect();
        assert_eq!(aces.len(), 2, "{sddl}");
        for ace in aces {
            let principal = ace.trim_end_matches(')').rsplit(';').next().unwrap();
            assert!(["OW", "SY"].contains(&principal), "{sddl}");
            assert!(ace.contains(";FA;"), "{sddl}");
        }
        drop(handoff);
        assert!(OpenOptions::new().write(true).open(package).is_ok());
        assert_eq!(
            ClientUpdateJournal::read(installed.home.path())
                .unwrap()
                .unwrap()
                .phase(),
            ClientUpdatePhase::Failed
        );
        // A later explicit preparation gets a fresh guardian after safe cancel.
        let retry = installed.client("nsis-currentUser");
        let handoff = retry.prepare(false).await.unwrap();
        drop(handoff);
    }
    #[tokio::test]
    async fn altered_verified_bytes_are_rejected_by_official_signature_reverification_before_ready()
    {
        let installed = Installed::new();
        let client = installed.client("nsis-currentUser");
        let error = client
            .prepare(true)
            .await
            .err()
            .expect("corrupt staging must never prepare");
        assert!(error.starts_with("prepare: "), "{error}");
        assert!(error.to_ascii_lowercase().contains("signature"), "{error}");
        assert!(ClientUpdateJournal::read(installed.home.path())
            .unwrap()
            .is_none());
    }
    #[tokio::test]
    async fn raw_unmarked_msi_and_mismatched_installation_never_prepare_or_create_a_journal() {
        for mode in [
            "raw",
            "msi",
            "nsis-both",
            "unmarked",
            "mismatch",
            "owned-msi",
            "system",
        ] {
            let installed = Installed::new();
            match mode {
                "unmarked" => {
                    installed
                        .registry
                        .key()
                        .delete_value("TokenMarketUpdatePolicy")
                        .unwrap();
                }
                "mismatch" => {
                    installed
                        .registry
                        .key()
                        .set_value("TokenMarketUpdateDirectory", &"C:\\Other Installation")
                        .unwrap();
                }
                "owned-msi" => {
                    installed
                        .registry
                        .key()
                        .set_value("WindowsInstaller", &1u32)
                        .unwrap();
                }
                "system" => {
                    let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
                        .create_subkey(format!(r"{}\Competing", installed.registry.machine))
                        .unwrap();
                    key.set_value("DisplayName", &"Token Market").unwrap();
                    key.set_value("WindowsInstaller", &1u32).unwrap();
                }
                _ => {}
            }
            let family = if ["raw", "msi", "nsis-both"].contains(&mode) {
                mode
            } else {
                "nsis-currentUser"
            };
            let client = installed.client(family);
            let error = client.prepare(false).await.err().expect(mode);
            assert!(error.starts_with("prepare: "), "{mode}: {error}");
            assert!(
                ClientUpdateJournal::read(installed.home.path())
                    .unwrap()
                    .is_none(),
                "{mode}"
            );
            assert!(client.staged_packages().is_empty(), "{mode}");
        }
    }
}

#[test]
fn native_launch_failure_and_nonzero_exit_record_recovery_and_allow_later_explicit_retry() {
    for failure in ["invalid-image", "nonzero"] {
        let fixture = Fixture::new();
        let root = fixture.root.path();
        if failure == "invalid-image" {
            fs::write(
                root.join("stage/installer.exe"),
                b"not a Windows executable",
            )
            .unwrap();
        } else {
            fs::write(root.join("nonzero"), b"fixture").unwrap();
        }
        fs::write(root.join("finish-installer"), b"finish").unwrap();
        let (mut parent, mut reader, nonce, guardian) = fixture.parent();
        writeln!(parent.stdin.as_mut().unwrap(), "{{\"commit\":\"{nonce}\"}}").unwrap();
        until(&mut reader, "committed");
        writeln!(parent.stdin.take().unwrap(), "exit").unwrap();
        assert!(parent.wait().unwrap().success());
        assert_eq!(
            unsafe { WaitForSingleObject(guardian.as_raw_handle(), 20_000) },
            WAIT_OBJECT_0
        );
        wait_file(&root.join("relaunched"));
        let journal = ClientUpdateJournal::read(root).unwrap().unwrap();
        assert_eq!(journal.phase(), ClientUpdatePhase::Failed);
        assert!(journal
            .failure_reason()
            .unwrap()
            .contains(if failure == "nonzero" { "2" } else { "启动" }));
        probe(root, false);
        if failure == "invalid-image" {
            fs::copy(
                root.join("应用 Token Market/app.exe"),
                root.join("stage/installer.exe"),
            )
            .unwrap();
        } else {
            fs::remove_file(root.join("nonzero")).unwrap();
        }
        let (mut parent, mut reader, _, _) = fixture.parent();
        writeln!(parent.stdin.take().unwrap(), "cancel").unwrap();
        let mut rest = String::new();
        reader.read_to_string(&mut rest).unwrap();
        assert!(parent.wait().unwrap().success(), "{rest}");
    }
}
