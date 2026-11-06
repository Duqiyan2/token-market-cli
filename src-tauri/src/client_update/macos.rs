//! Apple Silicon app-bundle updates. The official installer is synchronous on
//! macOS; this owned handoff retains every legacy operation lock until it ends.
use super::{
    ClientUpdateCommit, ClientUpdateHandoff, ClientUpdateInstaller, ClientUpdateStartup,
    VerifiedClientUpdate,
};
use std::{
    fs,
    path::{Component, Path, PathBuf},
    process::Command,
    sync::Arc,
};
use tokenmarket_core::client_update::{ClientUpdateGuard, ClientUpdateJournal, ClientUpdatePhase};

type Restart = Arc<dyn Fn() + Send + Sync>;

pub(super) struct MacInstaller {
    home: PathBuf,
    executable: PathBuf,
    from_version: String,
    restart: Restart,
}

pub(super) fn installer(app: &tauri::AppHandle) -> Option<Arc<dyn ClientUpdateInstaller>> {
    if !cfg!(target_arch = "aarch64") {
        return None;
    }
    let executable = std::env::current_exe().ok()?;
    let home = tokenmarket_core::config::home_dir();
    if installed_bundle(&home, &executable).is_err() {
        return None;
    }
    let handle = app.clone();
    Some(Arc::new(MacInstaller {
        home,
        executable,
        from_version: env!("CARGO_PKG_VERSION").into(),
        restart: Arc::new(move || handle.restart()),
    }))
}

/// Production permits installed application directories only: never a mounted
/// DMG, translocated download, development binary or symlinked bundle.
fn installed_bundle(home: &Path, executable: &Path) -> Result<PathBuf, String> {
    let bundle = bundle_for_executable(executable)?;
    if bundle.parent() != Some(Path::new("/Applications"))
        && bundle.parent() != Some(home.join("Applications").as_path())
    {
        return Err(
            "请先将客户端安装到“应用程序”目录，再安装客户端更新；也可使用官方安装包恢复".into(),
        );
    }
    if fs::canonicalize(executable).map_err(|e| e.to_string())? != executable {
        return Err("当前客户端路径包含链接，请使用“应用程序”中的正式客户端更新".into());
    }
    Ok(bundle)
}

fn bundle_for_executable(executable: &Path) -> Result<PathBuf, String> {
    let macos = executable.parent().ok_or("客户端安装路径无效")?;
    let contents = macos.parent().ok_or("客户端安装路径无效")?;
    let bundle = contents.parent().ok_or("客户端安装路径无效")?;
    if !executable.is_absolute()
        || macos.file_name().and_then(|s| s.to_str()) != Some("MacOS")
        || contents.file_name().and_then(|s| s.to_str()) != Some("Contents")
        || bundle.extension().and_then(|s| s.to_str()) != Some("app")
    {
        return Err("仅支持已安装的 macOS Apple Silicon 应用，请使用官方安装包更新".into());
    }
    Ok(bundle.into())
}

impl MacInstaller {
    #[cfg(test)]
    pub(super) fn fixture(
        home: PathBuf,
        executable: PathBuf,
        from_version: String,
        restart: Restart,
    ) -> Self {
        // The fixture seam is native test-only and cannot target a user's app.
        assert!(executable.starts_with(std::env::temp_dir()));
        assert!(home.starts_with(std::env::temp_dir()));
        Self {
            home,
            executable,
            from_version,
            restart,
        }
    }
}

impl ClientUpdateInstaller for MacInstaller {
    fn prepare(
        &self,
        verified: VerifiedClientUpdate,
    ) -> Result<Box<dyn ClientUpdateHandoff>, String> {
        let guard = ClientUpdateGuard::acquire(
            &self.home,
            &tokenmarket_core::config::console_api_base_url(),
        )
        .map_err(|e| format!("{e:#}"))?;
        let bundle = bundle_for_executable(&self.executable)?;
        let (update, bytes) = verified.into_parts();
        if update.target != "darwin-aarch64" {
            return Err("此更新包不适用于 macOS Apple Silicon，请使用官方安装包更新".into());
        }
        preflight(
            &bundle,
            &self.executable,
            &self.from_version,
            &update.version,
            &bytes,
        )?;
        let mut journal = ClientUpdateJournal::begin(
            &self.home,
            &self.from_version,
            &update.version,
            &self.executable,
        )
        .map_err(|e| format!("{e:#}"))?;
        journal
            .record(ClientUpdatePhase::Guarded)
            .map_err(|e| format!("{e:#}"))?;
        Ok(Box::new(MacHandoff {
            _guard: guard,
            journal,
            update,
            bytes,
            restart: self.restart.clone(),
        }))
    }
}

struct MacHandoff {
    _guard: ClientUpdateGuard,
    journal: ClientUpdateJournal,
    update: tauri_plugin_updater::Update,
    bytes: Vec<u8>,
    restart: Restart,
}

impl Drop for MacHandoff {
    fn drop(&mut self) {
        if self.journal.phase() == ClientUpdatePhase::Guarded {
            let _ = self.journal.record(ClientUpdatePhase::Failed);
        }
    }
}

impl ClientUpdateHandoff for MacHandoff {
    fn commit(mut self: Box<Self>) -> Result<ClientUpdateCommit, String> {
        self.journal
            .record(ClientUpdatePhase::Committed)
            .map_err(|e| format!("{e:#}"))?;
        self.journal
            .record(ClientUpdatePhase::InstallerRunning)
            .map_err(|e| format!("{e:#}"))?;
        if let Err(error) = self.update.install(&self.bytes) {
            // The synchronous call has terminated; locks remain held while the
            // failure is durably recorded. No launch/exit inference is involved.
            self.journal
                .record(ClientUpdatePhase::Failed)
                .map_err(|e| format!("安装失败且无法保存恢复记录，请使用官方安装包恢复：{e:#}"))?;
            return Err(format!(
                "客户端安装失败，已保留验证包，可重试或使用官方安装包恢复：{error}"
            ));
        }
        if let Err(error) = self.journal.record(ClientUpdatePhase::AwaitingStartup) {
            // Replacement has completed. Never make this candidate retryable
            // after an uncertain journal write; startup will explain recovery.
            return Ok(ClientUpdateCommit::HandoffWithWarning(format!(
                "客户端已替换但无法保存启动确认，请使用官方安装包检查恢复：{error:#}"
            )));
        }
        (self.restart)();
        Ok(ClientUpdateCommit::Handoff) // Test restarters return; production app.restart exits the process.
    }
}

fn output(program: &str, args: &[&std::ffi::OsStr]) -> Result<String, String> {
    let result = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("无法检查客户端签名或安装包：{e}"))?;
    if !result.status.success() {
        return Err("客户端签名、身份或安装包检查失败，请使用发布方的官方安装包恢复".into());
    }
    Ok(String::from_utf8_lossy(&result.stdout).trim().to_string())
}

fn requirement(bundle: &Path) -> Result<String, String> {
    let text = output(
        "/usr/bin/codesign",
        &["-d".as_ref(), "-r-".as_ref(), bundle.as_os_str()],
    )?;
    let requirement = text
        .strip_prefix("designated => ")
        .ok_or("无法确认客户端签名身份，请使用官方安装包恢复")?;
    // The release build pins the shared CLI/desktop requirement to its signing
    // certificate. An ad-hoc hash or identifier-only requirement cannot preserve it.
    if !requirement.contains("certificate leaf = H\"") {
        return Err("当前客户端没有可延续的证书签名身份，请使用官方安装包更新".into());
    }
    Ok(requirement.into())
}

fn plist(bundle: &Path, key: &str) -> Result<String, String> {
    output(
        "/usr/libexec/PlistBuddy",
        &[
            "-c".as_ref(),
            format!("Print :{key}").as_ref(),
            bundle.join("Contents/Info.plist").as_os_str(),
        ],
    )
}

fn preflight(
    bundle: &Path,
    executable: &Path,
    from: &str,
    target: &str,
    bytes: &[u8],
) -> Result<(), String> {
    let staging = tempfile::tempdir().map_err(|e| format!("无法准备客户端更新：{e}"))?;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    let mut root = None;
    for entry in archive
        .entries()
        .map_err(|_| "客户端更新包不是有效应用归档，请重新下载或使用官方安装包")?
    {
        let mut entry =
            entry.map_err(|_| "客户端更新包不是有效应用归档，请重新下载或使用官方安装包")?;
        let path = entry
            .path()
            .map_err(|_| "客户端更新包路径无效")?
            .into_owned();
        if !path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
            || !(entry.header().entry_type().is_file() || entry.header().entry_type().is_dir())
        {
            return Err("客户端更新包包含不支持的路径或链接，请使用官方安装包更新".into());
        }
        let first = path
            .components()
            .next()
            .ok_or("客户端更新包为空")?
            .as_os_str()
            .to_owned();
        if root.as_ref().is_some_and(|value| value != &first) {
            return Err("客户端更新包必须只包含一个应用，请使用官方安装包更新".into());
        }
        root = Some(first);
        entry
            .unpack_in(staging.path())
            .map_err(|_| "无法准备客户端更新包，请检查磁盘空间后重试")?;
    }
    let incoming = staging.path().join(root.ok_or("客户端更新包为空")?);
    let identifier = plist(bundle, "CFBundleIdentifier")?;
    if !matches!(
        identifier.as_str(),
        "cn.tokenmarket.setup" | "cn.tokenmarket.setup.test"
    ) || plist(&incoming, "CFBundleIdentifier")? != identifier
        || plist(bundle, "CFBundleShortVersionString")? != from
        || plist(&incoming, "CFBundleShortVersionString")? != target
        || incoming.file_name() != bundle.file_name()
        || plist(&incoming, "CFBundleExecutable")?
            != executable.file_name().unwrap().to_string_lossy()
    {
        return Err("客户端更新包的版本或应用身份不匹配，请使用官方安装包恢复".into());
    }
    let current_requirement = requirement(bundle)?;
    if requirement(&incoming)? != current_requirement {
        return Err(
            "客户端更新包的签名身份已改变，已拒绝安装；请联系发布方或使用官方安装包恢复".into(),
        );
    }
    let explicit = format!("={current_requirement}");
    for app in [bundle, incoming.as_path()] {
        output(
            "/usr/bin/codesign",
            &[
                "--verify".as_ref(),
                "--deep".as_ref(),
                "--strict".as_ref(),
                "-R".as_ref(),
                explicit.as_ref(),
                app.as_os_str(),
            ],
        )?;
    }
    let incoming_exe = incoming
        .join("Contents/MacOS")
        .join(executable.file_name().unwrap());
    if output(
        "/usr/bin/lipo",
        &["-archs".as_ref(), incoming_exe.as_os_str()],
    )? != "arm64"
    {
        return Err("客户端更新包不是 Apple Silicon 应用，请使用官方安装包更新".into());
    }
    Ok(())
}

/// Executed before checking any distribution configuration. The arguments are
/// obtained from compiled version/current_exe in production, never from IPC.
pub(super) fn reconcile(
    home: &Path,
    running_version: &str,
    executable: &Path,
) -> Result<ClientUpdateStartup, String> {
    if ClientUpdateJournal::read(home)
        .map_err(|e| format!("{e:#}"))?
        .is_none()
    {
        return Ok(ClientUpdateStartup::Clear);
    }
    let _guard =
        ClientUpdateGuard::acquire(home, &tokenmarket_core::config::console_api_base_url())
            .map_err(|e| format!("{e:#}"))?;
    let Some(mut journal) = ClientUpdateJournal::read(home).map_err(|e| format!("{e:#}"))? else {
        return Ok(ClientUpdateStartup::Clear);
    };
    // macOS installation runs only synchronously inside the guard-owning
    // desktop. Owning the same full guard now proves that earlier in-process
    // installer is inactive, including after an abrupt process termination.
    // This proof does not apply to Windows' independent installer process.
    match journal.phase() {
        ClientUpdatePhase::Preparing
        | ClientUpdatePhase::Guarded
        | ClientUpdatePhase::Committed => {
            // InstallerRunning is flushed before calling install, so these
            // interrupted phases never entered application replacement.
            journal
                .record(ClientUpdatePhase::Failed)
                .map_err(|e| format!("{e:#}"))?;
            return Ok(ClientUpdateStartup::RetryableFailure(
                "上次客户端更新准备已中断，未开始安装，请重新检查更新或使用官方安装包恢复".into(),
            ));
        }
        ClientUpdatePhase::InstallerRunning => {
            journal
                .record(ClientUpdatePhase::AwaitingStartup)
                .map_err(|e| format!("{e:#}"))?;
        }
        _ => {}
    }
    match journal.phase() {
        ClientUpdatePhase::Succeeded => Ok(ClientUpdateStartup::Clear),
        ClientUpdatePhase::Failed => Ok(ClientUpdateStartup::RetryableFailure(
            "上次客户端安装未完成，请重新检查更新或使用官方安装包恢复".into(),
        )),
        ClientUpdatePhase::AwaitingStartup => {
            match journal.acknowledge_startup(running_version, executable) {
                Ok(()) => Ok(ClientUpdateStartup::Clear),
                Err(error) => Ok(ClientUpdateStartup::Pending(format!("{error:#}"))),
            }
        }
        _ => Ok(ClientUpdateStartup::Pending(
            "上次客户端更新交接中断，尚未确认完成，请使用官方安装包检查恢复".into(),
        )),
    }
}

pub(super) fn reconcile_startup() -> Result<ClientUpdateStartup, String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    reconcile(
        &tokenmarket_core::config::home_dir(),
        env!("CARGO_PKG_VERSION"),
        &executable,
    )
}
