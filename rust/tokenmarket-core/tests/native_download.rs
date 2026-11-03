use std::{
    net::TcpListener,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};
use tokenmarket_core::installation::{
    InstallExecutor, InstallStage, InstallStep, NativeInstallExecutor,
};

#[cfg(unix)]
struct Peer(std::process::Child);
#[cfg(unix)]
impl Drop for Peer {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn cancelling_a_native_download_stops_a_live_curl_without_executing_the_next_step() {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        const ISOLATED_CHILD: &str = "TOKENMARKET_NATIVE_CANCEL_ISOLATED_CHILD";
        if std::env::var_os(ISOLATED_CHILD).is_none() {
            // Keep a cancellation regression confined to a test-owned group,
            // even if a broken kill implementation signals the helper itself.
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "cancelling_a_native_download_stops_a_live_curl_without_executing_the_next_step",
                    "--nocapture",
                ])
                .env(ISOLATED_CHILD, "1")
                .process_group(0)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "cancellation killed or failed its parent helper: {}\n{}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
    }
    #[cfg(unix)]
    let mut peer = Peer(
        std::process::Command::new("/bin/sleep")
            .arg("60")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    );
    // A real local socket accepts curl's connection but deliberately withholds
    // its TLS handshake. No Agent, network origin or replacement curl is used.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let root = tempfile::tempdir().unwrap();
    let step = InstallStep {
        stage: InstallStage::Downloading,
        download: Some((
            format!("https://{}/fixture", listener.local_addr().unwrap()),
            root.path().join("official-package.tar.gz"),
        )),
        program: root.path().join("must-not-run").display().to_string(),
        args: vec![],
        environment: vec![],
    };
    let cancelled = Arc::new(AtomicBool::new(false));
    let worker_cancelled = cancelled.clone();
    let (sender, receiver) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        sender
            .send(NativeInstallExecutor.execute(&step, &worker_cancelled, &mut |_| {}))
            .unwrap();
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    let socket = loop {
        match listener.accept() {
            Ok((socket, _)) => break Some(socket),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    break None;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("local download fixture failed: {error}"),
        }
    };
    let started_cancel = Instant::now();
    cancelled.store(true, Ordering::SeqCst);
    let result = receiver.recv_timeout(Duration::from_secs(10)).unwrap();
    worker.join().unwrap();
    assert!(
        socket.is_some(),
        "native curl never reached the local fixture"
    );
    assert!(started_cancel.elapsed() < Duration::from_secs(10));
    assert!(result.unwrap_err().to_string().contains("安装已取消"));
    #[cfg(unix)]
    assert!(
        peer.0.try_wait().unwrap().is_none(),
        "cancelling an installer must not signal an unrelated peer process"
    );
}
