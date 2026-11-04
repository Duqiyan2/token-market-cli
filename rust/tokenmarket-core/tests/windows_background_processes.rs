#![cfg(windows)]

use std::{
    os::windows::process::CommandExt, path::Path, process::Command, sync::atomic::AtomicBool,
};
use tokenmarket_core::installation::{
    InstallExecutor, InstallStage, InstallStep, NativeInstallExecutor,
};

#[link(name = "kernel32")]
extern "system" {
    fn GetConsoleWindow() -> *mut std::ffi::c_void;
}

#[test]
fn version_and_shell_probes_do_not_allocate_a_console() {
    const FIXTURE: &str = "TOKENMARKET_TEST_CONSOLE_FIXTURE";
    if let Some(fixture) = std::env::var_os(FIXTURE) {
        // A detached test process has the same lack of a console as the desktop
        // app. A normal console test runner would mask the original regression.
        assert!(unsafe { GetConsoleWindow() }.is_null());
        let fixture = Path::new(&fixture);
        let control = Command::new(fixture).output().unwrap();
        assert!(control.status.success());
        assert_ne!(
            String::from_utf8(control.stdout).unwrap().trim(),
            "1.2.3 console=0"
        );
        let executor = NativeInstallExecutor;
        for command in [fixture.to_path_buf(), fixture.with_extension("cmd")] {
            let version = executor.version(&command).unwrap();
            assert_eq!(version, "1.2.3 console=0", "probe: {}", command.display());
        }
        let script = format!("& '{}'", fixture.to_string_lossy().replace('\'', "''"));
        let step = InstallStep {
            stage: InstallStage::Detecting,
            program: "powershell.exe".into(),
            args: vec![
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-Command".into(),
                script,
            ],
            environment: vec![],
            download: None,
        };
        let mut lines = vec![];
        executor
            .execute(&step, &AtomicBool::new(false), &mut |line| lines.push(line))
            .unwrap();
        assert_eq!(lines, ["1.2.3 console=0"]);
        return;
    }

    let temp = tempfile::Builder::new()
        .prefix("console probe with spaces ")
        .tempdir()
        .unwrap();
    let source = temp.path().join("probe.rs");
    let fixture = temp.path().join("probe.exe");
    std::fs::write(
        &source,
        r#"
        #[link(name = "kernel32")]
        extern "system" { fn GetConsoleWindow() -> *mut std::ffi::c_void; }
        fn main() { println!("1.2.3 console={}", unsafe { GetConsoleWindow() } as usize); }
    "#,
    )
    .unwrap();
    let compiler = Command::new("rustc")
        .arg(&source)
        .args(["--crate-name", "console_probe", "-o"])
        .arg(&fixture)
        .output()
        .unwrap();
    assert!(
        compiler.status.success(),
        "{}",
        String::from_utf8_lossy(&compiler.stderr)
    );
    std::fs::write(fixture.with_extension("cmd"), "@\"%~dp0probe.exe\" %*\r\n").unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "version_and_shell_probes_do_not_allocate_a_console",
            "--nocapture",
        ])
        .env(FIXTURE, &fixture)
        .creation_flags(0x0000_0008) // DETACHED_PROCESS: reproduce a GUI parent.
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
