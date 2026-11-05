//! Current-user NSIS only. This module never calls the updater's exiting install shortcut.
use super::{
    ClientUpdateCommit, ClientUpdateHandoff, ClientUpdateInstaller, ClientUpdateStartup,
    VerifiedClientUpdate,
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    os::windows::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt},
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
        process::CommandExt,
    },
    path::{Component, Path, PathBuf, Prefix},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::Arc,
    time::Duration,
};
use tauri::AppHandle;
use tokenmarket_core::client_update::{
    run_guardian, ClientUpdateJournal, ClientUpdatePhase, GuardianEnvironment, GuardianPlan,
    NativeProcessIdentity,
};
use windows_sys::Win32::{
    Foundation::{GetLastError, LocalFree, FILETIME, HANDLE, WAIT_OBJECT_0},
    Security::{
        Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW, SECURITY_ATTRIBUTES,
    },
    Storage::FileSystem::{
        CreateDirectoryW, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
    },
    System::Threading::{
        GetCurrentProcess, GetProcessTimes, OpenProcess, WaitForSingleObject, INFINITE,
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    },
};

const HELPER_ARGUMENT: &str = "--tokenmarket-update-guardian-v1";
const POLICY: &str = "nsis-currentUser-v1-bundler2.10.4";
const UNINSTALL_ROOT: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";

#[derive(Serialize, Deserialize)]
struct Bootstrap {
    plan: GuardianPlan,
    parent: NativeProcessIdentity,
}

struct PinnedPackage {
    file: File,
    _stage_directory: File,
    digest: String,
}
impl PinnedPackage {
    fn open(path: &Path) -> Result<Self> {
        reject_reparse_components(path)?;
        let directory = path.parent().context("更新包目录缺失")?;
        let stage_directory = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(directory)?;
        let mut file = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?;
        if !file.metadata()?.is_file()
            || file.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        {
            bail!("更新包必须是普通文件");
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        Ok(Self {
            file,
            _stage_directory: stage_directory,
            digest: format!("{:x}", Sha256::digest(&bytes)),
        })
    }
    fn bytes(&mut self) -> Result<Vec<u8>> {
        use std::io::{Seek, SeekFrom};
        self.file.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::new();
        self.file.read_to_end(&mut bytes)?;
        Ok(bytes)
    }
}

fn reject_reparse_components(path: &Path) -> Result<()> {
    if !path.is_absolute() {
        bail!("客户端更新路径必须是本机绝对路径");
    }
    for ancestor in path.ancestors() {
        if fs::symlink_metadata(ancestor)?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            bail!("客户端更新不支持链接或重解析目录，请使用官方安装包更新");
        }
    }
    Ok(())
}

fn nsis_tail(directory: &Path) -> Result<OsString> {
    let mut components = directory.components();
    if !matches!(components.next(), Some(Component::Prefix(p)) if matches!(p.kind(), Prefix::Disk(_)))
        || !matches!(components.next(), Some(Component::RootDir))
        || components.clone().next().is_none()
        || !components.all(|p| matches!(p, Component::Normal(_)))
        || directory
            .as_os_str()
            .encode_wide()
            .any(|c| matches!(c, 0 | 10 | 13 | 34))
    {
        bail!("安装目录不符合安全的 NSIS 更新路径要求，请使用官方安装包更新");
    }
    // NSIS /D MUST be last, absolute, and unquoted even when the directory has spaces.
    let mut tail = OsString::from("/D=");
    tail.push(directory);
    Ok(tail)
}

fn creation_time(handle: HANDLE) -> Result<u64> {
    let mut created: FILETIME = unsafe { std::mem::zeroed() };
    let mut exited = created;
    let mut kernel = created;
    let mut user = created;
    if unsafe { GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64)
}

fn current_identity() -> Result<NativeProcessIdentity> {
    Ok(NativeProcessIdentity {
        pid: std::process::id(),
        created: creation_time(unsafe { GetCurrentProcess() })?,
    })
}

fn parent_handle(expected: &NativeProcessIdentity) -> Result<OwnedHandle> {
    let handle = unsafe {
        OpenProcess(
            PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            expected.pid,
        )
    };
    if handle.is_null() {
        return Err(std::io::Error::last_os_error().into());
    }
    let owned = unsafe { OwnedHandle::from_raw_handle(handle) };
    if creation_time(handle)? != expected.created
        || unsafe { WaitForSingleObject(handle, 0) } != 0x102
    {
        bail!("无法确认发起更新的原客户端进程");
    }
    Ok(owned)
}

fn private_stage(home: &Path) -> Result<PathBuf> {
    // Owner-rights + SYSTEM only, protected DACL. Do not inherit a permissive
    // temp-directory DACL. No real login or credential file is opened.
    let root = home.join(".tokenmarket/client-update-stage");
    fs::create_dir_all(&root)?;
    reject_reparse_components(&root)?;
    let stage = root.join(format!("{:032x}", rand::random::<u128>()));
    let descriptor: Vec<u16> = "D:P(A;OICI;FA;;;OW)(A;OICI;FA;;;SY)\0"
        .encode_utf16()
        .collect();
    let mut security = std::ptr::null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            descriptor.as_ptr(),
            1,
            &mut security,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    let attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: security,
        bInheritHandle: 0,
    };
    let name: Vec<u16> = stage.as_os_str().encode_wide().chain(Some(0)).collect();
    let success = unsafe { CreateDirectoryW(name.as_ptr(), &attributes) };
    let error = if success == 0 {
        Some(unsafe { GetLastError() })
    } else {
        None
    };
    unsafe { LocalFree(security) };
    if let Some(error) = error {
        return Err(std::io::Error::from_raw_os_error(error as i32).into());
    }
    Ok(stage)
}
/// NSIS bundle stamp and our installation hook establish provenance together.
/// The registry is ownership evidence, never version-completion evidence.
fn installed_application(app: &AppHandle) -> Result<PathBuf> {
    use tauri::utils::config::{BundleType, NSISInstallerMode};
    use winreg::{
        enums::{
            HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
        },
        RegKey,
    };
    if tauri::utils::platform::bundle_type() != Some(BundleType::Nsis)
        || !app.config().bundle.windows.nsis.as_ref().is_some_and(|n| {
            n.install_mode == NSISInstallerMode::CurrentUser && n.template.is_none()
        })
    {
        bail!("当前安装方式不支持自动更新，请使用官方 NSIS 安装包手动更新；MSI 请使用 MSI 恢复");
    }
    let application = std::env::current_exe()?;
    reject_reparse_components(&application)?;
    let directory = application.parent().context("无法定位客户端安装目录")?;
    nsis_tail(directory)?;
    let product = app
        .config()
        .product_name
        .as_deref()
        .unwrap_or("Token Market");
    for hive in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        for view in [KEY_WOW64_64KEY, KEY_WOW64_32KEY] {
            let root = match RegKey::predef(hive)
                .open_subkey_with_flags(UNINSTALL_ROOT, KEY_READ | view)
            {
                Ok(root) => root,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            };
            for name in root.enum_keys() {
                let key = root.open_subkey_with_flags(name?, KEY_READ | view)?;
                if key.get_value::<String, _>("DisplayName").ok().as_deref() == Some(product) {
                    let is_msi = key.get_value::<u32, _>("WindowsInstaller").unwrap_or(0) == 1;
                    let uninstall = key
                        .get_value::<String, _>("UninstallString")
                        .unwrap_or_default()
                        .to_ascii_lowercase();
                    if hive == HKEY_LOCAL_MACHINE || is_msi || uninstall.contains("msiexec") {
                        bail!("检测到 MSI 或系统级安装，无法自动切换安装归属；请使用对应官方安装包更新");
                    }
                }
            }
        }
    }
    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(format!(r"{}\{}", UNINSTALL_ROOT, product))?;
    let policy: String = key.get_value("TokenMarketUpdatePolicy")?;
    let location: String = key.get_value("TokenMarketUpdateDirectory")?;
    let registered: String = key.get_value("InstallLocation")?;
    let uninstall: String = key.get_value("UninstallString")?;
    if policy != POLICY
        || Path::new(&location) != directory
        || registered != format!("\"{}\"", directory.display())
        || uninstall != format!("\"{}\"", directory.join("uninstall.exe").display())
        || !directory.join("uninstall.exe").is_file()
    {
        bail!("无法确认当前用户 NSIS 安装来源，请先使用官方安装包手动更新");
    }
    Ok(application)
}

pub struct WindowsInstaller {
    app: AppHandle,
    application: PathBuf,
}
/// Register only after Windows lifecycle CI has validated this revision. Physical
/// installed-client acceptance remains separate. Unsupported provenance is closed.
pub fn installer(app: &AppHandle) -> Option<Arc<dyn ClientUpdateInstaller>> {
    // Deliberately closed until the actual Windows subprocess suite passes.
    // The parent integration replaces this gate after recorded CI evidence.
    const WINDOWS_LIFETIME_VALIDATED: bool = false;
    if !WINDOWS_LIFETIME_VALIDATED {
        return None;
    }
    installed_application(app).ok().map(|application| {
        Arc::new(WindowsInstaller {
            app: app.clone(),
            application,
        }) as Arc<dyn ClientUpdateInstaller>
    })
}

impl ClientUpdateInstaller for WindowsInstaller {
    fn prepare(
        &self,
        verified: VerifiedClientUpdate,
    ) -> std::result::Result<Box<dyn ClientUpdateHandoff>, String> {
        self.prepare_native(verified).map_err(|e| e.to_string())
    }
}
impl WindowsInstaller {
    fn prepare_native(
        &self,
        verified: VerifiedClientUpdate,
    ) -> Result<Box<dyn ClientUpdateHandoff>> {
        // Recheck ownership on each explicit attempt, not only at startup.
        if installed_application(&self.app)? != self.application {
            bail!("客户端安装路径已改变，请重启后重试");
        }
        let (update, bytes) = verified.into_parts();
        if update.target != "windows-x86_64-nsis" {
            bail!("更新包安装方式不匹配，请使用官方 NSIS 安装包");
        }
        let home = tokenmarket_core::config::home_dir();
        let stage = private_stage(&home)?;
        let package = stage.join("verified-update.exe");
        let helper = stage.join("tokenmarket-update-guardian.exe");
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&package)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        let mut pinned = PinnedPackage::open(&package)?;
        // Official verification applies to the actual staged executable, not
        // just the earlier memory buffer. Keep this handle until helper READY.
        update.verify(&pinned.bytes()?)?;
        let mut copy = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&helper)?;
        std::io::copy(&mut File::open(&self.application)?, &mut copy)?;
        copy.sync_all()?;
        drop(copy);
        let parent = current_identity()?;
        let mut child = Command::new(&helper)
            .arg(HELPER_ARGUMENT)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .creation_flags(0x08000000)
            .spawn()?;
        let mut input = child.stdin.take().context("更新守护进程没有私有输入管道")?;
        let stdout = child
            .stdout
            .take()
            .context("更新守护进程没有私有输出管道")?;
        let bootstrap = Bootstrap {
            parent,
            plan: GuardianPlan {
                home,
                origin: tokenmarket_core::config::console_api_base_url(),
                from_version: update.current_version,
                target_version: update.version,
                application: self.application.clone(),
                package,
                package_sha256: pinned.digest.clone(),
                guardian: NativeProcessIdentity {
                    pid: child.id(),
                    created: 0,
                },
            },
        };
        serde_json::to_writer(&mut input, &bootstrap)?;
        input.write_all(b"\n")?;
        input.flush()?;
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            let result = reader.read_line(&mut line).map(|_| line);
            let _ = sender.send((result, reader));
        });
        let response = receiver.recv_timeout(Duration::from_secs(20));
        let (line, reader) = match response {
            Ok((line, reader)) => (line?, reader),
            Err(_) => {
                drop(input);
                let _ = child.wait();
                bail!("更新守护进程未就绪，尚未安装；请重试或手动更新");
            }
        };
        let response: serde_json::Value =
            serde_json::from_str(&line).context("更新守护进程无法安全交接")?;
        let nonce = response
            .get("nonce")
            .and_then(|v| v.as_str())
            .filter(|_| response.get("ready") == Some(&serde_json::Value::Bool(true)));
        let Some(nonce) = nonce else {
            drop(input);
            let _ = child.wait();
            bail!(
                "{}",
                response
                    .get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("更新守护进程未就绪，尚未安装")
            );
        };
        // Helper now owns its deny-write/delete package handle AND every legacy lock.
        drop(pinned);
        Ok(Box::new(WindowsHandoff {
            app: self.app.clone(),
            input: Some(input),
            reader,
            child,
            nonce: nonce.into(),
            committed: false,
        }))
    }
}

struct WindowsHandoff {
    app: AppHandle,
    input: Option<ChildStdin>,
    reader: BufReader<ChildStdout>,
    child: Child,
    nonce: String,
    committed: bool,
}
impl ClientUpdateHandoff for WindowsHandoff {
    fn commit(mut self: Box<Self>) -> std::result::Result<ClientUpdateCommit, String> {
        // Once any COMMIT bytes may have reached the guardian, an uncertain pipe
        // result is pending handoff, never retryable installation failure.
        self.committed = true;
        if let Some(mut input) = self.input.take() {
            let _ = writeln!(input, "{{\"commit\":\"{}\"}}", self.nonce);
            let _ = input.flush();
            let mut line = String::new();
            let _ = self.reader.read_line(&mut line);
        }
        self.app.exit(0);
        Ok(ClientUpdateCommit::Handoff)
    }
}
impl Drop for WindowsHandoff {
    fn drop(&mut self) {
        self.input.take();
        if !self.committed {
            let _ = self.child.wait();
        }
    }
}

struct NativeEnvironment {
    parent: OwnedHandle,
    application: PathBuf,
    package: PathBuf,
    _pinned: PinnedPackage,
}
impl GuardianEnvironment for NativeEnvironment {
    fn wait_for_parent(&mut self) -> Result<()> {
        if unsafe { WaitForSingleObject(self.parent.as_raw_handle(), INFINITE) } != WAIT_OBJECT_0 {
            bail!("原客户端进程等待失败，未启动安装器");
        }
        Ok(())
    }
    fn launch_installer(&mut self) -> Result<Child> {
        let directory = self.application.parent().context("无法定位安装目录")?;
        Ok(Command::new(&self.package)
            .args(["/P", "/UPDATE"])
            .raw_arg(nsis_tail(directory)?)
            .current_dir(directory)
            .spawn()?)
    }
    fn installer_identity(&self, child: &Child) -> Result<NativeProcessIdentity> {
        Ok(NativeProcessIdentity {
            pid: child.id(),
            created: creation_time(child.as_raw_handle())?,
        })
    }
    fn relaunch(&mut self) -> Result<()> {
        Command::new(&self.application)
            .current_dir(self.application.parent().context("无法定位安装目录")?)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(())
    }
}

/// Call before Tauri, single-instance, tool managers, session or credential setup.
/// All metadata arrives only through the child's inherited anonymous input pipe.
pub fn try_run_guardian() -> Option<i32> {
    if std::env::args_os().nth(1).as_deref() != Some(std::ffi::OsStr::new(HELPER_ARGUMENT)) {
        return None;
    }
    let result = (|| -> Result<()> {
        if std::env::args_os().count() != 2 {
            bail!("守护进程参数无效");
        }
        let mut reader = BufReader::new(std::io::stdin());
        let mut bootstrap = String::new();
        reader.by_ref().take(64 * 1024).read_line(&mut bootstrap)?;
        let mut bootstrap: Bootstrap = serde_json::from_str(&bootstrap)?;
        let current = std::env::current_exe()?;
        if current.file_name() != Some(std::ffi::OsStr::new("tokenmarket-update-guardian.exe"))
            || current.parent() != bootstrap.plan.package.parent()
            || current.parent() == bootstrap.plan.application.parent()
        {
            bail!("守护进程必须位于独立更新目录");
        }
        reject_reparse_components(&current)?;
        reject_reparse_components(&bootstrap.plan.application)?;
        nsis_tail(
            bootstrap
                .plan
                .application
                .parent()
                .context("安装目录缺失")?,
        )?;
        let parent = parent_handle(&bootstrap.parent)?;
        let pinned = PinnedPackage::open(&bootstrap.plan.package)?;
        if pinned.digest != bootstrap.plan.package_sha256 {
            bail!("更新包暂存字节已改变，未启动安装");
        }
        bootstrap.plan.guardian = current_identity()?;
        let mut environment = NativeEnvironment {
            parent,
            application: bootstrap.plan.application.clone(),
            package: bootstrap.plan.package.clone(),
            _pinned: pinned,
        };
        run_guardian(
            bootstrap.plan,
            &mut reader,
            &mut std::io::stdout(),
            &mut environment,
        )
    })();
    match result {
        Ok(()) => Some(0),
        Err(error) => {
            let _ = writeln!(
                std::io::stdout(),
                "{}",
                serde_json::json!({"error":error.to_string()})
            );
            Some(1)
        }
    }
}

pub fn reconcile_startup() -> std::result::Result<ClientUpdateStartup, String> {
    let result = (|| -> Result<ClientUpdateStartup> {
        let home = tokenmarket_core::config::home_dir();
        if let Some(mut journal) = ClientUpdateJournal::read(&home)? {
            if matches!(
                journal.phase(),
                ClientUpdatePhase::Failed | ClientUpdatePhase::Succeeded
            ) {
                return Ok(ClientUpdateStartup::Clear);
            }
            // This exact compiled process is the only authority on completion.
            if journal.phase() == ClientUpdatePhase::AwaitingStartup {
                journal
                    .acknowledge_startup(env!("CARGO_PKG_VERSION"), &std::env::current_exe()?)?;
            } else {
                return Ok(ClientUpdateStartup::Pending("上次客户端更新交接尚未确认结束，请检查安装进程或使用官方安装包恢复；不会自动重试".into()));
            }
        }
        Ok(ClientUpdateStartup::Clear)
    })();
    result.map_err(|e| e.to_string())
}
