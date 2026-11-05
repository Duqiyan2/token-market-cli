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
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(PathBuf::from(root).join(".tokenmarket/desktop-connections/operation.lock"))
        .unwrap();
    println!(
        "{}",
        if file.try_lock().is_ok() {
            "MUTATED"
        } else {
            "BUSY"
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
