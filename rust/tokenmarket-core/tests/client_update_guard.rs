//! Separate-process legacy lock fixtures: no credentials or real tool files are opened.
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};
use tokenmarket_core::client_update::ClientUpdateGuard;

const ORIGIN: &str = "https://console.example.invalid";

struct Owner(Child, BufReader<std::process::ChildStdout>);
impl Owner {
    fn lock(path: &Path) -> Self {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "legacy_lock_process", "--nocapture"])
            .env("CLIENT_UPDATE_TEST_LOCK", path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let mut reader = BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        loop {
            assert_ne!(
                reader.read_line(&mut line).unwrap(),
                0,
                "owner exited before acquiring lock"
            );
            if line.contains("LOCKED") {
                break;
            }
            line.clear();
        }
        Self(child, reader)
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        self.0.stdin.take().unwrap().write_all(b"finish\n").unwrap();
        let mut output = String::new();
        std::io::Read::read_to_string(&mut self.1, &mut output).unwrap();
        assert!(self.0.wait().unwrap().success(), "{output}");
    }
}

#[test]
fn legacy_lock_process() {
    let Some(path) = std::env::var_os("CLIENT_UPDATE_TEST_LOCK") else {
        return;
    };
    let path = PathBuf::from(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .unwrap();
    file.try_lock().unwrap();
    println!("LOCKED");
    std::io::stdout().flush().unwrap();
    let mut finish = String::new();
    std::io::stdin().read_line(&mut finish).unwrap();
    // Completed work proves the guard did not cancel the lock owner.
    fs::write(path.with_extension("completed"), b"completed").unwrap();
}

#[test]
fn active_cli_connection_blocks_installation_without_cancellation_and_allows_retry() {
    let home = tempfile::tempdir().unwrap();
    let lock = home
        .path()
        .join(".tokenmarket/desktop-connections/operation.lock");
    let owner = Owner::lock(&lock);
    let error = ClientUpdateGuard::acquire(home.path(), ORIGIN)
        .err()
        .expect("must refuse active CLI work");
    assert!(error.to_string().contains("连接"), "{error}");
    assert!(!lock.with_extension("completed").exists());
    drop(owner);
    assert_eq!(
        fs::read(lock.with_extension("completed")).unwrap(),
        b"completed"
    );
    let _guard = ClientUpdateGuard::acquire(home.path(), ORIGIN).unwrap();
}

#[test]
fn every_legacy_tool_and_login_scope_blocks_installation_and_partial_acquisition_is_released() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join(".tokenmarket");
    let existing_scope = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    fs::create_dir_all(root.join("client-login").join(existing_scope)).unwrap();
    let mut locks: Vec<(PathBuf, &str)> = [
        "claude-code",
        "codex",
        "workbuddy",
        "deepseek-harness",
        "openclaw",
        "opencode",
        "grok-build",
        "hermes",
        "pi",
        "minimax-code",
        "oh-my-pi",
        "kimi-code",
        "dumate",
    ]
    .into_iter()
    .map(|tool| {
        (
            root.join("install-locks").join(format!("{tool}.lock")),
            "工具",
        )
    })
    .collect();
    locks.push((root.join("client-login/5efb3e5d6dc5c96cfb2cf99a80e022fb81c545900f54a5daf9f034a3b0a68ce1/operation.lock"), "登录"));
    locks.push((
        root.join("client-login")
            .join(existing_scope)
            .join("operation.lock"),
        "登录",
    ));
    for (path, reason) in locks {
        let owner = Owner::lock(&path);
        let error = ClientUpdateGuard::acquire(home.path(), ORIGIN)
            .err()
            .expect("legacy operation must block update");
        assert!(error.to_string().contains(reason), "{path:?}: {error}");
        // The earlier connection lock must be free while the conflicting owner still runs.
        let connection_owner = Owner::lock(&root.join("desktop-connections/operation.lock"));
        drop(connection_owner);
        drop(owner);
        let guard = ClientUpdateGuard::acquire(home.path(), ORIGIN).unwrap();
        drop(guard);
    }
    assert!(
        !root.join("install-locks/dumate-enterprise.lock").exists(),
        "enterprise shares DuMate's legacy lock"
    );
}

#[test]
fn legacy_mutator_process() {
    let Some(path) = std::env::var_os("CLIENT_UPDATE_TEST_PROBE") else {
        return;
    };
    let path = PathBuf::from(path);
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    if file.try_lock().is_ok() {
        fs::write(path.with_extension("mutated"), b"mutation").unwrap();
        println!("MUTATED");
    } else {
        println!("BUSY");
    }
}

fn probe(path: &Path, blocked: bool) {
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "legacy_mutator_process", "--nocapture"])
        .env("CLIENT_UPDATE_TEST_PROBE", path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains(if blocked { "BUSY" } else { "MUTATED" })
    );
    assert_eq!(path.with_extension("mutated").exists(), !blocked);
}

#[test]
fn guarded_decision_and_handoff_exclude_new_legacy_mutations_in_separate_processes() {
    let home = tempfile::tempdir().unwrap();
    let guard = ClientUpdateGuard::acquire(home.path(), ORIGIN).unwrap();
    let root = home.path().join(".tokenmarket");
    let mut locks = vec![root.join("desktop-connections/operation.lock"), root.join("client-login/5efb3e5d6dc5c96cfb2cf99a80e022fb81c545900f54a5daf9f034a3b0a68ce1/operation.lock")];
    locks.extend(
        fs::read_dir(root.join("install-locks"))
            .unwrap()
            .map(|entry| entry.unwrap().path()),
    );
    for lock in &locks {
        probe(lock, true);
    }
    // Move ownership across the native handoff seam; do not unlock/reacquire.
    let installer = std::thread::spawn(move || {
        let _guard = guard;
        for lock in &locks {
            probe(lock, true);
        }
        locks
    });
    let locks = installer.join().unwrap();
    for lock in &locks {
        probe(lock, false);
    }
}

#[test]
fn journal_retains_interrupted_handoff_and_only_matching_actual_startup_establishes_success() {
    use tokenmarket_core::client_update::{ClientUpdateJournal, ClientUpdatePhase};
    let home = tempfile::tempdir().unwrap();
    let application = home.path().join("Token Market/Token Market.exe");
    let journal = ClientUpdateJournal::begin(home.path(), "1.0.0", "1.1.0", &application).unwrap();
    assert_eq!(journal.phase(), ClientUpdatePhase::Preparing);
    let nonce = journal.nonce().to_owned();
    let mut journal = journal;
    journal.record(ClientUpdatePhase::Guarded).unwrap();
    journal.record(ClientUpdatePhase::Committed).unwrap();
    journal.record(ClientUpdatePhase::InstallerRunning).unwrap();
    drop(journal);
    let mut journal = ClientUpdateJournal::read(home.path()).unwrap().unwrap();
    assert_eq!(journal.nonce(), nonce);
    assert_eq!(journal.phase(), ClientUpdatePhase::InstallerRunning);
    assert!(
        ClientUpdateJournal::begin(home.path(), "1.0.0", "1.1.0", &application).is_err(),
        "do not silently overwrite interrupted work"
    );
    assert!(
        journal.acknowledge_startup("1.1.0", &application).is_err(),
        "launch alone does not prove termination"
    );
    journal.record(ClientUpdatePhase::AwaitingStartup).unwrap();
    assert!(journal.acknowledge_startup("1.0.0", &application).is_err());
    assert!(journal
        .acknowledge_startup("1.1.0", &home.path().join("wrong.exe"))
        .is_err());
    journal.acknowledge_startup("1.1.0", &application).unwrap();
    assert_eq!(
        ClientUpdateJournal::read(home.path())
            .unwrap()
            .unwrap()
            .phase(),
        ClientUpdatePhase::Succeeded
    );
}
