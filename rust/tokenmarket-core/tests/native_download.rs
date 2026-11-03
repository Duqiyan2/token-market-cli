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

#[test]
fn cancelling_a_native_download_stops_a_live_curl_without_executing_the_next_step() {
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
}
