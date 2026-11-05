//! Public guardian protocol with real parent, installer and legacy mutator processes.
//! All state is synthetic and lives below a temporary home.
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};
use tokenmarket_core::client_update::{
    run_guardian, GuardianEnvironment, GuardianPlan, NativeProcessIdentity,
};

struct FixtureEnvironment {
    root: PathBuf,
}
impl GuardianEnvironment for FixtureEnvironment {
    fn wait_for_parent(&mut self) -> anyhow::Result<()> {
        let mut rest = String::new();
        std::io::stdin().read_to_string(&mut rest)?;
        Ok(()) // This private pipe's writer belongs only to the fixture parent.
    }
    fn launch_installer(&mut self) -> anyhow::Result<Child> {
        if self.root.join("fail-launch").exists() {
            anyhow::bail!("fixture launch failure");
        }
        Ok(Command::new(std::env::current_exe()?)
            .args(["--exact", "fixture_installer", "--nocapture"])
            .env("GUARDIAN_FIXTURE_ROOT", &self.root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .spawn()?)
    }
    fn installer_identity(&self, child: &Child) -> anyhow::Result<NativeProcessIdentity> {
        Ok(NativeProcessIdentity {
            pid: child.id(),
            created: 1,
        })
    }
    fn relaunch(&mut self) -> anyhow::Result<()> {
        fs::write(self.root.join("relaunched"), b"old version")?;
        Ok(())
    }
}

#[test]
fn fixture_guardian() {
    let Some(root) = std::env::var_os("GUARDIAN_FIXTURE_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let plan = GuardianPlan {
        home: root.clone(),
        origin: "https://fixture.invalid".into(),
        from_version: "1.0.0".into(),
        target_version: "1.1.0".into(),
        application: root.join("app.exe"),
        package: root.join("update.exe"),
        package_sha256: "fixture digest".into(),
        guardian: NativeProcessIdentity {
            pid: std::process::id(),
            created: 1,
        },
    };
    let result = run_guardian(
        plan,
        &mut BufReader::with_capacity(1, std::io::stdin()),
        &mut std::io::stdout(),
        &mut FixtureEnvironment { root },
    );
    if let Err(error) = result {
        println!("GUARDIAN_ERROR: {error}");
    }
}

#[test]
fn fixture_installer() {
    let Some(root) = std::env::var_os("GUARDIAN_FIXTURE_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    fs::write(root.join("installer-started"), b"started").unwrap();
    while !root.join("finish-installer").exists() {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    if root.join("nonzero").exists() {
        std::process::exit(2);
    }
}

#[test]
fn fixture_parent() {
    let Some(root) = std::env::var_os("GUARDIAN_PARENT_ROOT") else {
        return;
    };
    let mut guardian = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "fixture_guardian", "--nocapture"])
        .env("GUARDIAN_FIXTURE_ROOT", root)
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .spawn()
        .unwrap();
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
    guardian
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    guardian.stdin.as_mut().unwrap().flush().unwrap();
    input.clear();
    std::io::stdin().read_line(&mut input).unwrap();
    // Parent exits without waiting/killing its guardian; only the exact pipe writer closes.
}

fn line_until(reader: &mut impl BufRead, needle: &str) -> String {
    loop {
        let mut line = String::new();
        assert_ne!(reader.read_line(&mut line).unwrap(), 0, "missing {needle}");
        if line.contains(needle) {
            return line;
        }
    }
}
fn probe(home: &Path, busy: bool) {
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "fixture_legacy_mutator", "--nocapture"])
        .env("GUARDIAN_PROBE_ROOT", home)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stdout).contains(if busy { "BUSY" } else { "MUTATED" }),
        "{output:?}"
    );
}
#[test]
fn fixture_legacy_mutator() {
    let Some(root) = std::env::var_os("GUARDIAN_PROBE_ROOT") else {
        return;
    };
    let path = PathBuf::from(root).join(".tokenmarket/desktop-connections/operation.lock");
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .unwrap();
    if file.try_lock().is_ok() {
        println!("MUTATED");
    } else {
        println!("BUSY");
    }
}

#[test]
fn guardian_protects_legacy_work_through_parent_exit_and_installer_termination() {
    let home = tempfile::tempdir().unwrap();
    let mut parent = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "fixture_parent", "--nocapture"])
        .env("GUARDIAN_PARENT_ROOT", home.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut reader = BufReader::new(parent.stdout.take().unwrap());
    let ready = line_until(&mut reader, "ready");
    let ready: serde_json::Value = serde_json::from_str(&ready).unwrap();
    probe(home.path(), true);
    writeln!(
        parent.stdin.as_mut().unwrap(),
        "{{\"commit\":\"{}\"}}",
        ready["nonce"].as_str().unwrap()
    )
    .unwrap();
    line_until(&mut reader, "committed");
    assert!(
        !home.path().join("installer-started").exists(),
        "must wait for parent"
    );
    probe(home.path(), true);
    writeln!(parent.stdin.take().unwrap(), "exit").unwrap();
    assert!(parent.wait().unwrap().success());
    for _ in 0..500 {
        if home.path().join("installer-started").exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(home.path().join("installer-started").exists());
    probe(home.path(), true);
    fs::write(home.path().join("finish-installer"), b"finish").unwrap();
    let mut rest = String::new();
    reader.read_to_string(&mut rest).unwrap();
    assert!(home.path().join("relaunched").exists(), "{rest}");
    probe(home.path(), false);
    let mut journal = tokenmarket_core::client_update::ClientUpdateJournal::read(home.path())
        .unwrap()
        .unwrap();
    assert_eq!(
        journal.phase(),
        tokenmarket_core::client_update::ClientUpdatePhase::AwaitingStartup
    );
    assert!(journal
        .acknowledge_startup("1.0.0", &home.path().join("app.exe"))
        .is_err());
    journal
        .acknowledge_startup("1.1.0", &home.path().join("app.exe"))
        .unwrap();
}

fn start_guardian(root: &Path) -> (Child, BufReader<std::process::ChildStdout>) {
    let mut guardian = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "fixture_guardian", "--nocapture"])
        .env("GUARDIAN_FIXTURE_ROOT", root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let reader = BufReader::new(guardian.stdout.take().unwrap());
    (guardian, reader)
}
fn finish(mut guardian: Child, mut reader: BufReader<std::process::ChildStdout>) -> String {
    drop(guardian.stdin.take());
    let mut rest = String::new();
    reader.read_to_string(&mut rest).unwrap();
    assert!(guardian.wait().unwrap().success(), "{rest}");
    rest
}
#[test]
fn cancelling_prepared_handoff_does_not_install_and_allows_explicit_retry() {
    let root = tempfile::tempdir().unwrap();
    let (guardian, mut reader) = start_guardian(root.path());
    line_until(&mut reader, "ready");
    probe(root.path(), true);
    finish(guardian, reader);
    assert!(!root.path().join("installer-started").exists());
    probe(root.path(), false);
    let (guardian, mut reader) = start_guardian(root.path());
    line_until(&mut reader, "ready");
    finish(guardian, reader);
}
#[test]
fn launch_failure_and_nonzero_installer_are_honest_failures_and_retry_is_explicit() {
    use tokenmarket_core::client_update::{ClientUpdateJournal, ClientUpdatePhase};
    for failure in ["fail-launch", "nonzero"] {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join(failure), b"fixture").unwrap();
        fs::write(root.path().join("finish-installer"), b"finish").unwrap();
        let (mut guardian, mut reader) = start_guardian(root.path());
        let ready: serde_json::Value =
            serde_json::from_str(&line_until(&mut reader, "ready")).unwrap();
        writeln!(
            guardian.stdin.as_mut().unwrap(),
            "{{\"commit\":\"{}\"}}",
            ready["nonce"].as_str().unwrap()
        )
        .unwrap();
        line_until(&mut reader, "committed");
        let output = finish(guardian, reader);
        assert!(output.contains("GUARDIAN_ERROR"), "{output}");
        assert!(
            root.path().join("relaunched").exists(),
            "known failure must reopen the existing client for recovery"
        );
        assert_eq!(
            ClientUpdateJournal::read(root.path())
                .unwrap()
                .unwrap()
                .phase(),
            ClientUpdatePhase::Failed
        );
        let journal = ClientUpdateJournal::read(root.path()).unwrap().unwrap();
        let failure_message = journal
            .failure_reason()
            .expect("known failure must retain recovery explanation");
        assert!(
            failure_message.contains(if failure == "nonzero" { "2" } else { "启动" }),
            "{failure_message}"
        );
        probe(root.path(), false);
        fs::remove_file(root.path().join(failure)).unwrap();
        let (guardian, mut reader) = start_guardian(root.path());
        line_until(&mut reader, "ready");
        finish(guardian, reader);
    }
}
