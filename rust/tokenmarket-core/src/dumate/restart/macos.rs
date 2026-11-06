//! PID, owner, executable and birth-time checks only. Never read task state or process secrets.
use anyhow::{bail, Result};
use std::{
    ffi::{c_int, c_void},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
#[repr(C)]
#[derive(Default)]
struct BsdInfo {
    flags: u32,
    status: u32,
    xstatus: u32,
    pid: u32,
    parent: u32,
    uid: u32,
    gid: u32,
    ruid: u32,
    rgid: u32,
    svuid: u32,
    svgid: u32,
    reserved: u32,
    comm: [u8; 16],
    name: [u8; 32],
    nfiles: u32,
    pgid: u32,
    jobc: u32,
    tdev: u32,
    tpgid: u32,
    nice: i32,
    start_sec: u64,
    start_usec: u64,
}
#[link(name = "proc")]
unsafe extern "C" {
    fn proc_listpids(kind: u32, info: u32, buffer: *mut c_void, size: c_int) -> c_int;
    fn proc_pidpath(pid: c_int, buffer: *mut c_void, size: u32) -> c_int;
    fn proc_pidinfo(pid: c_int, flavor: c_int, arg: u64, buffer: *mut c_void, size: c_int)
        -> c_int;
}
unsafe extern "C" {
    fn getuid() -> u32;
    fn kill(pid: c_int, signal: c_int) -> c_int;
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Process {
    pid: i32,
    uid: u32,
    birth: (u64, u64),
    path: PathBuf,
}
pub(super) fn validate_app(app: &Path) -> Result<()> {
    let info = plist::Value::from_file(app.join("Contents/Info.plist"))?;
    if info
        .as_dictionary()
        .and_then(|d| d.get("CFBundleIdentifier"))
        .and_then(plist::Value::as_string)
        != Some("com.baidu.qianfan.desktop")
    {
        bail!("无法确认百度搭子应用身份")
    }
    Ok(())
}
fn executable(pid: i32) -> Option<PathBuf> {
    let mut bytes = vec![0u8; 4096];
    let count = unsafe { proc_pidpath(pid, bytes.as_mut_ptr().cast(), bytes.len() as u32) };
    if count <= 0 {
        return None;
    }
    let end = bytes.iter().position(|b| *b == 0)?;
    Path::new(std::str::from_utf8(&bytes[..end]).ok()?)
        .canonicalize()
        .ok()
}
fn identity(pid: i32) -> Result<Option<Process>> {
    let mut info = BsdInfo::default();
    let count = unsafe {
        proc_pidinfo(
            pid,
            3,
            0,
            (&mut info as *mut BsdInfo).cast(),
            std::mem::size_of::<BsdInfo>() as c_int,
        )
    };
    if count == 0 {
        return Ok(None);
    }
    if count != std::mem::size_of::<BsdInfo>() as i32 || info.pid != pid as u32 {
        bail!(super::UNKNOWN);
    }
    if info.status == 5 {
        return Ok(None);
    } // A zombie no longer executes code.
    let Some(path) = executable(pid) else {
        return Ok(None);
    };
    Ok(Some(Process {
        pid,
        uid: info.uid,
        birth: (info.start_sec, info.start_usec),
        path,
    }))
}
pub(super) fn capture(app: &Path) -> Result<Vec<Process>> {
    validate_app(app)?;
    let uid = unsafe { getuid() };
    let needed = unsafe { proc_listpids(4, uid, std::ptr::null_mut(), 0) };
    if needed <= 0 || needed > 1024 * 1024 {
        bail!(super::UNKNOWN);
    }
    let mut pids = vec![0i32; needed as usize / 4 + 1024];
    let count =
        unsafe { proc_listpids(4, uid, pids.as_mut_ptr().cast(), (pids.len() * 4) as c_int) };
    if count <= 0 || count as usize >= pids.len() * 4 || count % 4 != 0 {
        bail!(super::UNKNOWN);
    }
    let scope = app.canonicalize()?.join("Contents");
    let mut found = Vec::new();
    for pid in pids
        .into_iter()
        .take(count as usize / 4)
        .filter(|pid| *pid > 1)
    {
        if !executable(pid).is_some_and(|p| p.starts_with(&scope)) {
            continue;
        }
        if let Some(process) = identity(pid)? {
            if process.uid != uid || !process.path.starts_with(&scope) {
                bail!(super::UNKNOWN);
            }
            found.push(process);
        }
    }
    if found.len() > 256 {
        bail!("搭子附属进程过多，请手动重启");
    }
    Ok(found)
}
fn signal(app: &Path, process: &Process, value: i32) -> Result<()> {
    if process.uid != unsafe { getuid() }
        || !process
            .path
            .starts_with(app.canonicalize()?.join("Contents"))
    {
        bail!(super::UNKNOWN);
    }
    match identity(process.pid)? {
        None => Ok(()),
        Some(current) if current == *process => {
            if unsafe { kill(process.pid, value) } != 0
                && std::io::Error::last_os_error().raw_os_error() != Some(3)
            {
                bail!("无法结束搭子进程，请手动重启");
            }
            Ok(())
        }
        Some(_) => bail!("搭子实例已改变，请重新点击重启"),
    }
}
pub(super) fn terminate(app: &Path, processes: &[Process]) -> Result<()> {
    validate_app(app)?;
    // Give normal OS termination a short opportunity to flush state, then end
    // only the original, explicitly confirmed instances that remain alive.
    for process in processes {
        signal(app, process, 15)?;
    }
    let deadline = Instant::now() + Duration::from_millis(800);
    while Instant::now() < deadline {
        if processes
            .iter()
            .map(|p| identity(p.pid))
            .collect::<Result<Vec<_>>>()?
            .iter()
            .all(Option::is_none)
        {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    for process in processes {
        signal(app, process, 9)?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    #[test]
    fn forced_stop_only_targets_its_own_bundle_fixture_and_checks_birth_time() {
        use std::os::unix::process::{CommandExt, ExitStatusExt};
        unsafe extern "C" {
            #[link_name = "signal"]
            fn c_signal(value: c_int, handler: usize) -> usize;
        }
        let root = tempfile::tempdir().unwrap();
        let app = root.path().join("DuMate.app");
        let bin = app.join("Contents/MacOS/fixture");
        std::fs::create_dir_all(bin.parent().unwrap()).unwrap();
        std::fs::write(app.join("Contents/Info.plist"),r#"<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>com.baidu.qianfan.desktop</string></dict></plist>"#).unwrap();
        let source = root.path().join("fixture.rs");
        std::fs::write(
            &source,
            "fn main(){std::thread::sleep(std::time::Duration::from_secs(30));}",
        )
        .unwrap();
        let compiled = std::process::Command::new("rustc")
            .arg(&source)
            .arg("-o")
            .arg(&bin)
            .output()
            .unwrap();
        assert!(compiled.status.success(), "fixture compilation failed");
        let mut command = std::process::Command::new(&bin);
        command.arg("30");
        unsafe {
            command.pre_exec(|| {
                c_signal(15, 1);
                Ok(())
            });
        } // This child alone ignores TERM.
        let mut owned = Child(command.spawn().unwrap());
        let mut unrelated = Child(
            std::process::Command::new("/bin/sleep")
                .arg("30")
                .spawn()
                .unwrap(),
        );
        let mut processes = Vec::new();
        for _ in 0..100 {
            processes = capture(&app).unwrap();
            if !processes.is_empty() {
                break;
            }
            assert!(owned.0.try_wait().unwrap().is_none());
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(processes.len(), 1);
        assert_eq!(processes[0].pid, owned.0.id() as i32);
        let mut replaced = processes[0].clone();
        replaced.birth.1 += 1;
        assert!(signal(&app, &replaced, 15).is_err());
        assert!(owned.0.try_wait().unwrap().is_none());
        terminate(&app, &processes).unwrap();
        assert_eq!(owned.0.wait().unwrap().signal(), Some(9));
        assert!(unrelated.0.try_wait().unwrap().is_none());
        assert!(capture(&app).unwrap().is_empty());
    }
}
