//! Confirmed Windows restart using exact owner/path/PID/birth identities.
//! No task queries, process-memory reads, credential discovery or name-based kills.
use super::{windows_contract as contract, Driver, UNKNOWN};
use anyhow::{bail, Context, Result};
use std::{
    mem::{size_of, zeroed},
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    ptr::{null, null_mut},
    sync::Arc,
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, GetLastError, ERROR_MORE_DATA, ERROR_NO_MORE_FILES, FILETIME, HANDLE,
        INVALID_HANDLE_VALUE, WAIT_OBJECT_0, WAIT_TIMEOUT,
    },
    Security::{EqualSid, GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER},
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        },
        RestartManager::*,
        Threading::{
            GetCurrentProcess, GetProcessTimes, OpenProcess, OpenProcessToken,
            QueryFullProcessImageNameW, WaitForSingleObject, CREATE_NO_WINDOW,
            PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
        },
    },
};
struct Handle(HANDLE);
// A process/token/snapshot handle can be used from a worker thread; it is never duplicated
// or closed while a worker owns it. WindowsDriver retains captured handles across awaits.
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
struct Process {
    handle: Handle,
    pid: u32,
    parent_pid: u32,
    created: FILETIME,
    image: PathBuf,
}
fn time(value: FILETIME) -> u64 {
    (u64::from(value.dwHighDateTime) << 32) | u64::from(value.dwLowDateTime)
}
fn equal_path(left: &Path, right: &Path) -> bool {
    contract::same_path(&left.to_string_lossy(), &right.to_string_lossy())
}
fn owner(handle: HANDLE) -> Result<Vec<usize>> {
    let mut token = null_mut();
    if unsafe { OpenProcessToken(handle, TOKEN_QUERY, &mut token) } == 0 {
        bail!(UNKNOWN)
    }
    let token = Handle(token);
    let mut size = 0;
    unsafe {
        GetTokenInformation(token.0, TokenUser, null_mut(), 0, &mut size);
    }
    if size == 0 || size > 65536 {
        bail!(UNKNOWN)
    }
    // usize alignment is required for TOKEN_USER's SID pointer.
    let mut bytes = vec![0usize; (size as usize).div_ceil(size_of::<usize>())];
    if unsafe {
        GetTokenInformation(
            token.0,
            TokenUser,
            bytes.as_mut_ptr().cast(),
            size,
            &mut size,
        )
    } == 0
    {
        bail!(UNKNOWN)
    }
    Ok(bytes)
}
impl Process {
    fn open(pid: u32) -> Result<Self> {
        let flags = PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE;
        let raw = unsafe { OpenProcess(flags, 0, pid) };
        if raw.is_null() {
            bail!(UNKNOWN)
        }
        let handle = Handle(raw);
        let this = owner(unsafe { GetCurrentProcess() })?;
        let other = owner(handle.0)?;
        let a = unsafe { &*(this.as_ptr().cast::<TOKEN_USER>()) };
        let b = unsafe { &*(other.as_ptr().cast::<TOKEN_USER>()) };
        if unsafe { EqualSid(a.User.Sid, b.User.Sid) } == 0 {
            bail!(UNKNOWN)
        }
        let mut name = vec![0u16; 32768];
        let mut size = name.len() as u32;
        if unsafe { QueryFullProcessImageNameW(handle.0, 0, name.as_mut_ptr(), &mut size) } == 0 {
            bail!(UNKNOWN)
        }
        let image = PathBuf::from(String::from_utf16(&name[..size as usize])?).canonicalize()?;
        let mut created = FILETIME::default();
        let mut exited = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        if unsafe { GetProcessTimes(handle.0, &mut created, &mut exited, &mut kernel, &mut user) }
            == 0
        {
            bail!(UNKNOWN)
        }
        Ok(Self {
            handle,
            pid,
            parent_pid: 0,
            created,
            image,
        })
    }
    fn stopped(&self) -> Result<bool> {
        match unsafe { WaitForSingleObject(self.handle.0, 0) } {
            WAIT_OBJECT_0 => Ok(true),
            WAIT_TIMEOUT => Ok(false),
            _ => bail!(UNKNOWN),
        }
    }
    fn unique(&self) -> RM_UNIQUE_PROCESS {
        RM_UNIQUE_PROCESS {
            dwProcessId: self.pid,
            ProcessStartTime: self.created,
        }
    }
}
fn snapshot() -> Result<Vec<(u32, u32, String)>> {
    let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if raw == INVALID_HANDLE_VALUE {
        bail!(UNKNOWN)
    }
    let handle = Handle(raw);
    let mut entry: PROCESSENTRY32W = unsafe { zeroed() };
    entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
    let mut ok = unsafe { Process32FirstW(handle.0, &mut entry) };
    let mut entries = Vec::new();
    while ok != 0 {
        let len = entry
            .szExeFile
            .iter()
            .position(|c| *c == 0)
            .context(UNKNOWN)?;
        entries.push((
            entry.th32ProcessID,
            entry.th32ParentProcessID,
            String::from_utf16(&entry.szExeFile[..len])?,
        ));
        ok = unsafe { Process32NextW(handle.0, &mut entry) };
    }
    if unsafe { GetLastError() } != ERROR_NO_MORE_FILES {
        bail!(UNKNOWN)
    }
    Ok(entries)
}

fn capture(app: &Path) -> Result<Vec<Arc<Process>>> {
    let root = app.parent().context(UNKNOWN)?.join("resources");
    let mut found = Vec::new();
    for (pid, parent_pid, name) in snapshot()? {
        let mut process = match Process::open(pid) {
            Ok(process) => process,
            Err(_) => {
                if name.eq_ignore_ascii_case("DuMate.exe")
                    || name.to_ascii_lowercase().starts_with("dumate-")
                {
                    // A known child can exit between the snapshot and handle
                    // capture while shutdown/startup is in progress. A vanished
                    // PID is not an identity failure; a still-present PID is.
                    if snapshot()?.iter().any(|(current, _, _)| *current == pid) {
                        bail!(UNKNOWN);
                    }
                }
                continue;
            }
        };
        if (equal_path(&process.image, app)
            || contract::within(&process.image.to_string_lossy(), &root.to_string_lossy()))
            && !process.stopped()?
        {
            process.parent_pid = parent_pid;
            found.push(Arc::new(process));
        }
    }
    if found.len() > 128 {
        bail!("搭子附属进程过多，请手动重启");
    }
    Ok(found)
}
pub(super) struct WindowsDriver {
    app: PathBuf,
    closing: Vec<Arc<Process>>,
    expect_backend: bool,
}
impl WindowsDriver {
    pub(super) fn new(app: PathBuf) -> Result<Self> {
        contract::validate_app(&app)?;
        Ok(Self {
            app,
            closing: Vec::new(),
            expect_backend: false,
        })
    }
}
impl Driver for WindowsDriver {
    async fn running(&mut self) -> Result<bool> {
        let app = self.app.clone();
        self.closing = tokio::task::spawn_blocking(move || capture(&app)).await??;
        self.expect_backend = self
            .closing
            .iter()
            .any(|process| is_backend(&process.image));
        Ok(!self.closing.is_empty())
    }
    async fn quit(&mut self) -> Result<()> {
        if self
            .closing
            .iter()
            .map(|process| process.stopped())
            .collect::<Result<Vec<_>>>()?
            .iter()
            .all(|v| *v)
        {
            return Ok(());
        }
        let identities = self
            .closing
            .iter()
            .map(|process| {
                (
                    process.pid,
                    process.parent_pid,
                    equal_path(&process.image, &self.app),
                )
            })
            .collect::<Vec<_>>();
        let hosts = contract::shutdown_hosts(&identities)
            .into_iter()
            .map(|index| self.closing[index].clone())
            .collect::<Vec<_>>();
        if hosts.is_empty() && identities.iter().any(|(_, _, is_app)| *is_app) {
            bail!(UNKNOWN);
        }
        shutdown_group(hosts).await?;
        // The host cannot respawn a service after this point. Capture again to
        // include children created while the host was shutting down.
        let app = self.app.clone();
        let remaining = tokio::task::spawn_blocking(move || capture(&app)).await??;
        if remaining.iter().any(|process| {
            equal_path(&process.image, &self.app)
                && !self
                    .closing
                    .iter()
                    .any(|old| old.pid == process.pid && time(old.created) == time(process.created))
        }) {
            bail!("搭子实例已变化，未结束新启动的实例，请确认后重试");
        }
        self.closing.extend(remaining.iter().cloned());
        shutdown_group(remaining).await
    }
    async fn wait_stopped(&mut self) -> Result<()> {
        for _ in 0..75 {
            let app = self.app.clone();
            if self
                .closing
                .iter()
                .map(|process| process.stopped())
                .collect::<Result<Vec<_>>>()?
                .iter()
                .all(|v| *v)
                && tokio::task::spawn_blocking(move || capture(&app))
                    .await??
                    .is_empty()
            {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        bail!("搭子后台尚未完全退出，未重复打开。请手动退出后重新打开")
    }
    async fn open(&mut self) -> Result<()> {
        contract::validate_app(&self.app)?;
        let app = self.app.clone();
        if !tokio::task::spawn_blocking(move || capture(&app))
            .await??
            .is_empty()
        {
            bail!("搭子仍在运行，未重复打开");
        }
        let mut child = Command::new(&self.app)
            .current_dir(self.app.parent().context(UNKNOWN)?)
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("无法重新打开搭子，请手动打开")?;
        let application = self.app.clone();
        let launched_pid = child.id();
        super::windows_startup::confirm(self.expect_backend, || {
            let status = child.try_wait();
            let app = application.clone();
            async move {
                let launch_error = status?
                    .filter(|status| !status.success())
                    .map(|status| status.to_string());
                let scan = app.clone();
                let processes = tokio::task::spawn_blocking(move || capture(&scan)).await??;
                Ok(super::windows_startup::Snapshot {
                    host: processes.iter().any(|process| {
                        process.pid == launched_pid && equal_path(&process.image, &app)
                    }),
                    backend: processes.iter().any(|process| is_backend(&process.image)),
                    launch_error,
                })
            }
        })
        .await
    }
}

fn is_backend(image: &Path) -> bool {
    image.file_name().is_some_and(|name| {
        name.to_string_lossy()
            .eq_ignore_ascii_case("dumate-main-server.exe")
    })
}

async fn shutdown_group(processes: Vec<Arc<Process>>) -> Result<()> {
    let active = processes
        .into_iter()
        .filter_map(|process| match process.stopped() {
            Ok(true) => None,
            Ok(false) => Some(Ok(process)),
            Err(error) => Some(Err(error)),
        })
        .collect::<Result<Vec<_>>>()?;
    if active.is_empty() {
        return Ok(());
    }
    let session = Arc::new(RestartSession::new(&active)?);
    let worker = session.clone();
    let shutdown = tokio::task::spawn_blocking(move || {
        worker.shutdown()?;
        // RmShutdown's return is not sufficient evidence for starting the next
        // group. Confirm the captured native handles have actually exited.
        for process in active {
            if unsafe { WaitForSingleObject(process.handle.0, 15_000) } != WAIT_OBJECT_0 {
                bail!("搭子进程尚未完全退出，未继续重启，请手动退出后重新打开");
            }
        }
        Ok(())
    });
    super::windows_shutdown::finish(async { shutdown.await? }, || unsafe {
        RmCancelCurrentTask(session.0);
    })
    .await
}

struct RestartSession(u32);
impl RestartSession {
    fn new(processes: &[Arc<Process>]) -> Result<Self> {
        let mut handle = 0;
        let mut key = [0u16; 33];
        if unsafe { RmStartSession(&mut handle, 0, key.as_mut_ptr()) } != 0 {
            bail!("无法准备搭子重启，请手动重启")
        }
        let session = Self(handle);
        let identities: Vec<_> = processes.iter().map(|process| process.unique()).collect();
        if identities.is_empty()
            || identities.len() > 128
            || unsafe {
                RmRegisterResources(
                    handle,
                    0,
                    null(),
                    identities.len() as u32,
                    identities.as_ptr(),
                    0,
                    null(),
                )
            } != 0
        {
            bail!(UNKNOWN)
        }
        let mut needed = 0;
        let mut count = 0;
        let mut reasons = 0;
        let status =
            unsafe { RmGetList(handle, &mut needed, &mut count, null_mut(), &mut reasons) };
        if status != 0 && status != ERROR_MORE_DATA || needed > 128 || reasons != 0 {
            bail!(UNKNOWN)
        }
        let mut affected = vec![RM_PROCESS_INFO::default(); needed as usize];
        count = needed;
        if unsafe {
            RmGetList(
                handle,
                &mut needed,
                &mut count,
                affected.as_mut_ptr(),
                &mut reasons,
            )
        } != 0
            || reasons != 0
        {
            bail!(UNKNOWN)
        }
        if count as usize > affected.len()
            || affected[..count as usize].iter().any(|p| {
                !identities.iter().any(|id| {
                    id.dwProcessId == p.Process.dwProcessId
                        && time(id.ProcessStartTime) == time(p.Process.ProcessStartTime)
                })
            })
        {
            bail!(UNKNOWN)
        }
        Ok(session)
    }
    fn shutdown(&self) -> Result<()> {
        // Explicit confirmation authorizes forced shutdown of this exact PID/birth set.
        let status = unsafe { RmShutdown(self.0, RmForceShutdown as u32, None) };
        if status != 0 {
            bail!("搭子未能完全退出，请手动退出后重新打开（Windows 错误码 {status}）")
        }
        Ok(())
    }
}
impl Drop for RestartSession {
    fn drop(&mut self) {
        unsafe {
            RmEndSession(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, os::windows::ffi::OsStrExt};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    fn wide(value: &std::ffi::OsStr) -> Vec<u16> {
        value.encode_wide().chain(Some(0)).collect()
    }
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    #[test]
    fn native_process_identity_and_restart_manager_registration_are_read_only() {
        let process = Process::open(std::process::id()).unwrap();
        assert!(equal_path(
            &process.image,
            &std::env::current_exe().unwrap().canonicalize().unwrap()
        ));
        assert!(!process.stopped().unwrap());
        // Register this test process, inspect membership, and close the session. Never shut it down.
        let _session = RestartSession::new(&[Arc::new(process)]).unwrap();
    }
    #[test]
    #[ignore = "Windows desktop opt-in; gracefully closes only its own temporary window fixture"]
    fn restart_manager_closes_only_own_window_fixture() {
        let dir = tempfile::tempdir().unwrap();
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "dumate::restart::windows::tests::window_fixture_child",
            ])
            .env("TOKENMARKET_RESTART_WINDOW_FIXTURE", dir.path())
            .creation_flags(CREATE_NO_WINDOW)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut child = Child(child);
        for _ in 0..100 {
            if dir.path().join("ready").exists() {
                break;
            }
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "fixture exited before creating its window"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(dir.path().join("ready").exists());
        let process = Process::open(child.0.id()).unwrap();
        let session = RestartSession::new(&[Arc::new(process)]).unwrap();
        session.shutdown().unwrap();
        for _ in 0..100 {
            if child.0.try_wait().unwrap().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(child.0.try_wait().unwrap().is_some());
        assert!(
            dir.path().join("session-ended").exists(),
            "normal Restart Manager session-end callback must run"
        );
    }
    #[test]
    #[ignore = "child of restart_manager_closes_only_own_window_fixture"]
    fn window_fixture_child() {
        use windows_sys::Win32::{
            Foundation::{HWND, LPARAM, LRESULT, WPARAM},
            UI::WindowsAndMessaging::*,
        };
        static MARKER: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
        unsafe extern "system" fn window_proc(
            window: HWND,
            message: u32,
            w: WPARAM,
            l: LPARAM,
        ) -> LRESULT {
            if message == WM_QUERYENDSESSION {
                return 1;
            }
            if message == WM_ENDSESSION && w != 0 {
                if let Some(path) = MARKER.get() {
                    let _ = fs::write(path, b"normal session end");
                }
                unsafe {
                    PostQuitMessage(0);
                }
                return 0;
            }
            unsafe { DefWindowProcW(window, message, w, l) }
        }
        let Some(dir) = std::env::var_os("TOKENMARKET_RESTART_WINDOW_FIXTURE").map(PathBuf::from)
        else {
            return;
        };
        let _ = MARKER.set(dir.join("session-ended"));
        let class = wide(std::ffi::OsStr::new("TokenMarketRestartFixture"));
        let instance = unsafe { GetModuleHandleW(null()) };
        let mut definition: WNDCLASSW = unsafe { zeroed() };
        definition.lpfnWndProc = Some(window_proc);
        definition.hInstance = instance;
        definition.lpszClassName = class.as_ptr();
        assert_ne!(unsafe { RegisterClassW(&definition) }, 0);
        let window = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW,
                class.as_ptr(),
                class.as_ptr(),
                WS_OVERLAPPEDWINDOW,
                0,
                0,
                200,
                100,
                null_mut(),
                null_mut(),
                instance,
                null(),
            )
        };
        assert!(!window.is_null());
        fs::write(dir.join("ready"), b"ready").unwrap();
        let mut message: MSG = unsafe { zeroed() };
        while unsafe { GetMessageW(&mut message, null_mut(), 0, 0) } > 0 {
            unsafe {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        unsafe {
            DestroyWindow(window);
        }
    }
}
