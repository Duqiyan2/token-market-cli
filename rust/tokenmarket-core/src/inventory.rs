//! Read-only discovery of installed tools. No executable is launched during discovery.
use crate::{
    adapters::{adapters, get_adapter},
    ToolId,
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HostPlatform {
    MacOs,
    Windows,
    Unix,
}

/// Platform boundary used by the desktop and by isolated filesystem fixtures.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallationEnvironment {
    pub platform: HostPlatform,
    pub home: PathBuf,
    pub command_directories: Vec<PathBuf>,
    pub application_directories: Vec<PathBuf>,
    pub executable_extensions: Vec<String>,
}

impl InstallationEnvironment {
    pub fn current() -> Self {
        let home = crate::config::home_dir();
        let platform = if cfg!(target_os = "macos") {
            HostPlatform::MacOs
        } else if cfg!(windows) {
            HostPlatform::Windows
        } else {
            HostPlatform::Unix
        };
        let mut command_directories: Vec<PathBuf> = std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default();
        command_directories.extend([
            home.join(".local/bin"),
            home.join(".bun/bin"),
            home.join(".npm-global/bin"),
            home.join(".cargo/bin"),
            home.join(".opencode/bin"),
        ]);
        command_directories.extend(crate::installation::recipes::managed_command_directories(
            &home,
        ));
        let mut application_directories = vec![];
        if platform == HostPlatform::Windows {
            if let Some(appdata) = std::env::var_os("APPDATA") {
                command_directories.push(PathBuf::from(appdata).join("npm"));
            }
            if let Some(local) = std::env::var_os("LOCALAPPDATA") {
                let local = PathBuf::from(local);
                command_directories.push(local.join("Microsoft/WinGet/Links"));
                application_directories.extend([local.join("Programs"), local]);
            }
            for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
                if let Some(path) = std::env::var_os(variable) {
                    application_directories.push(PathBuf::from(path));
                }
            }
            #[cfg(windows)]
            {
                application_directories.extend(registered_application_directories());
                // Newly installed launchers may update the user's registry PATH
                // after this desktop process was started.
                command_directories.extend(registered_command_directories());
            }
        } else {
            command_directories.extend(
                [
                    "/usr/local/bin",
                    "/opt/homebrew/bin",
                    "/usr/bin",
                    "/bin",
                    "/usr/sbin",
                    "/sbin",
                ]
                .map(PathBuf::from),
            );
            if let Ok(versions) = fs::read_dir(home.join(".nvm/versions/node")) {
                command_directories
                    .extend(versions.flatten().map(|version| version.path().join("bin")));
            }
            if platform == HostPlatform::MacOs {
                application_directories
                    .extend([PathBuf::from("/Applications"), home.join("Applications")]);
            }
        }
        Self {
            platform,
            home,
            command_directories,
            application_directories,
            executable_extensions: std::env::var("PATHEXT")
                .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
                .split(';')
                .map(str::to_owned)
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionProjection {
    pub state: &'static str,
    pub owner: Option<serde_json::Value>,
    pub bound_workspace: Option<serde_json::Value>,
    pub models: Vec<String>,
    pub key_id: Option<String>,
    pub protocol: Option<String>,
    pub test_result: Option<crate::connections::ConnectionTestResult>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallationLocation {
    pub path: String,
    pub resolved_path: Option<String>,
    pub method: &'static str,
    pub selected: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInventoryEntry {
    pub id: ToolId,
    pub name: &'static str,
    pub installed: bool,
    pub can_connect: bool,
    pub download_url: &'static str,
    pub config_path: String,
    pub config_directory: String,
    pub config_exists: bool,
    pub config_directory_exists: bool,
    pub installation_method: &'static str,
    pub installations: Vec<InstallationLocation>,
    pub connection: Option<ConnectionProjection>,
    pub installation: crate::installation::InstallationAvailability,
}

/// Private connection context: re-resolve the same installation's effective path
/// using current overrides before displaying a valid verdict or sending traffic.
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct ConfigurationLocation {
    environment: InstallationEnvironment,
    path: PathBuf,
    follows_process_home: bool,
    #[serde(default)]
    dumate_profile: Option<String>,
}
impl ConfigurationLocation {
    pub(crate) fn dumate_test_guard(&self) -> Result<crate::installation::InstallationGuard> {
        self.verify(ToolId::DuMateEnterprise)?;
        let mut environment = self.environment.clone();
        if self.follows_process_home {
            environment.home = crate::config::home_dir();
        }
        ToolInventory::new(environment).connection_guard(ToolId::DuMateEnterprise)
    }

    pub(crate) async fn authorize(&self, tool: ToolId) -> Result<()> {
        self.verify(tool)?;
        if tool == ToolId::QwenWork {
            let home = if self.follows_process_home {
                crate::config::home_dir()
            } else {
                self.environment.home.clone()
            };
            crate::qwenwork::require_ready(&home, self.environment.platform)?;
        }
        if tool == ToolId::DuMateEnterprise {
            let home = if self.follows_process_home {
                crate::config::home_dir()
            } else {
                self.environment.home.clone()
            };
            crate::dumate::authorize_enterprise(&home, self.environment.platform).await?;
            self.verify(tool)?;
        }
        Ok(())
    }
    pub(crate) fn verify(&self, tool: ToolId) -> Result<()> {
        let mut environment = self.environment.clone();
        if self.follows_process_home {
            environment.home = crate::config::home_dir();
        }
        if tool == ToolId::DuMateEnterprise
            && self.dumate_profile.as_deref()
                != Some(&crate::dumate::enterprise_fingerprint(
                    &environment.home,
                    environment.platform,
                )?)
        {
            bail!("DuMate 活动企业账号已改变，请取消原连接后重新接入")
        }
        let current = ToolInventory::new(environment).config_path(tool);
        if absolute_config_path(&current)? != self.path {
            bail!(
                "{} 活动配置目录/profile 已改变，请先取消原连接后重新检测接入",
                get_adapter(tool).name
            )
        }
        Ok(())
    }
}
fn absolute_config_path(path: &Path) -> Result<PathBuf> {
    Ok(if path.is_absolute() {
        path.into()
    } else {
        std::env::current_dir()?.join(path)
    })
}

pub struct ToolInventory {
    environment: InstallationEnvironment,
}
impl ToolInventory {
    pub fn new(environment: InstallationEnvironment) -> Self {
        Self { environment }
    }
    pub fn current() -> Self {
        Self::new(InstallationEnvironment::current())
    }
    pub(crate) fn configuration_location(&self, tool: ToolId) -> Result<ConfigurationLocation> {
        Ok(ConfigurationLocation {
            environment: self.environment.clone(),
            path: absolute_config_path(&self.config_path(tool))?,
            follows_process_home: self.environment.home == crate::config::home_dir(),
            dumate_profile: if tool == ToolId::DuMateEnterprise {
                Some(crate::dumate::enterprise_fingerprint(
                    &self.environment.home,
                    self.environment.platform,
                )?)
            } else {
                None
            },
        })
    }
    pub fn list(&self) -> Result<Vec<ToolInventoryEntry>> {
        let manager = crate::connections::ConnectionManager::new(
            self.environment
                .home
                .join(".tokenmarket/desktop-connections"),
            String::new(),
            String::new(),
        );
        self.list_with_connections(&manager)
    }
    pub fn list_with_connections(
        &self,
        manager: &crate::connections::ConnectionManager,
    ) -> Result<Vec<ToolInventoryEntry>> {
        let desktop = manager.summaries()?;
        Ok(adapters()
            .into_iter()
            .map(|adapter| {
                let installed = self.is_installed(adapter.id);
                let installations = self.installation_locations(adapter.id);
                let installation_method =
                    installations.first().map(|p| p.method).unwrap_or_else(|| {
                        crate::installation::discovery::planned_method(
                            adapter.id,
                            &self.environment,
                        )
                    });
                let config = absolute_config_path(&self.config_path(adapter.id))
                    .unwrap_or_else(|_| self.config_path(adapter.id));
                let directory = config.parent().unwrap_or(&config);
                ToolInventoryEntry {
                    id: adapter.id,
                    name: adapter.name,
                    installed,
                    installation: crate::installation::recipes::availability(
                        adapter.id,
                        &self.environment,
                        std::env::consts::ARCH,
                    ),
                    can_connect: installed,
                    download_url: official_download_url(adapter.id),
                    config_path: config.display().to_string(),
                    config_directory: directory.display().to_string(),
                    config_exists: config.is_file(),
                    config_directory_exists: directory.is_dir(),
                    installation_method,
                    installations,
                    connection: desktop.iter().find(|(tool, _)| *tool == adapter.id).map(
                        |(_, r)| ConnectionProjection {
                            state: r.state,
                            owner: Some(r.owner.clone()),
                            bound_workspace: Some(r.bound_workspace.clone()),
                            models: r.models.clone(),
                            key_id: Some(r.key_id.clone()),
                            protocol: Some(r.protocol.clone()),
                            test_result: r.test_result.clone(),
                        },
                    ),
                }
            })
            .collect())
    }
    /// List distinct artifacts in resolver order. Duplicate PATH entries and
    /// symlink aliases for the same artifact do not become extra installations.
    pub fn installation_locations(&self, tool: ToolId) -> Vec<InstallationLocation> {
        let commands = if matches!(
            tool,
            ToolId::DuMate | ToolId::DuMateEnterprise | ToolId::QwenWork
        ) {
            vec![]
        } else {
            self.environment.command_paths(tool.command())
        };
        let applications = self
            .environment
            .application_directories
            .iter()
            .filter_map(|directory| {
                let mut environment = self.environment.clone();
                environment.application_directories = vec![directory.clone()];
                Self::new(environment).application_path(tool)
            })
            .collect::<Vec<_>>();
        let prefer_application = matches!(
            tool,
            ToolId::WorkBuddy
                | ToolId::DeepSeekHarness
                | ToolId::DuMate
                | ToolId::DuMateEnterprise
                | ToolId::QwenWork
        );
        let candidates = if prefer_application {
            applications
                .into_iter()
                .map(|p| (p, true))
                .chain(commands.into_iter().map(|p| (p, false)))
                .collect::<Vec<_>>()
        } else {
            commands
                .into_iter()
                .map(|p| (p, false))
                .chain(applications.into_iter().map(|p| (p, true)))
                .collect::<Vec<_>>()
        };
        let mut seen = std::collections::BTreeSet::new();
        let mut result = vec![];
        for (path, application) in candidates {
            let path = absolute_config_path(&path).unwrap_or(path);
            let resolved = path.canonicalize().ok();
            let identity = resolved
                .as_ref()
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned();
            let identity = if self.environment.platform == HostPlatform::Windows {
                identity.to_lowercase()
            } else {
                identity
            };
            if !seen.insert(identity) {
                continue;
            }
            result.push(InstallationLocation {
                method: crate::installation::discovery::method(
                    tool,
                    &path,
                    &self.environment,
                    application,
                ),
                selected: result.is_empty(),
                resolved_path: resolved
                    .filter(|p| p != &path)
                    .map(|p| p.display().to_string()),
                path: path.display().to_string(),
            });
        }
        result
    }
    /// Desktop discovery and desktop writes share this path. Terminal CLI paths are unchanged.
    pub fn config_path(&self, tool: ToolId) -> PathBuf {
        if tool == ToolId::QwenWork {
            return crate::qwenwork::database_path(
                &self.environment.home,
                self.environment.platform,
            );
        }
        if matches!(tool, ToolId::DuMate | ToolId::DuMateEnterprise) {
            return if tool == ToolId::DuMateEnterprise {
                crate::dumate::enterprise_display_config(
                    &self.environment.home,
                    self.environment.platform,
                )
            } else {
                crate::dumate::display_config(&self.environment.home, self.environment.platform)
            };
        }
        if tool == ToolId::KimiCode {
            return std::env::var_os("KIMI_CODE_HOME")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| self.environment.home.join(".kimi-code"))
                .join("config.toml");
        }
        if tool == ToolId::Hermes {
            return crate::config::hermes_home(&self.environment.home, self.environment.platform)
                .join("config.yaml");
        }
        if tool == ToolId::OhMyPi {
            return crate::connections::oh_my_pi::config_path(&self.environment.home);
        }
        if tool == ToolId::DeepSeekHarness
            && std::env::var_os("DSH_ACTIVE_PROFILE").is_none()
            && self.application_path(tool).is_none()
            && self.environment.command_path("dsh")
                == Some(
                    self.environment
                        .home
                        .join(".tokenmarket/tools/deepseek-harness/bin/dsh"),
                )
        {
            // The npm CLI cannot boot Electron's reserved desktop profile. Its
            // home patch applies to web/headless and preserves retained profiles.
            return std::env::var_os("DSH_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| self.environment.home.join(".dsh"))
                .join("cordis.patch.yml");
        }
        if tool == ToolId::MiniMaxCode {
            return crate::adapters::minimax_data_dir(&self.environment.home).join("config.yaml");
        }
        if tool == ToolId::Pi {
            return std::env::var_os("PI_CODING_AGENT_DIR")
                .filter(|v| !v.to_string_lossy().trim().is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| self.environment.home.join(".pi/agent"))
                .join("models.json");
        }
        if tool == ToolId::GrokBuild {
            return std::env::var_os("GROK_HOME")
                .filter(|v| !v.to_string_lossy().trim().is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| self.environment.home.join(".grok"))
                .join("config.toml");
        }
        if tool == ToolId::WorkBuddy {
            let override_path = |name: &str| {
                std::env::var_os(name)
                    .filter(|v| !v.to_string_lossy().trim().is_empty())
                    .map(PathBuf::from)
            };
            return override_path("WORKBUDDY_CONFIG_PATH").unwrap_or_else(|| {
                override_path("WORKBUDDY_CONFIG_DIR")
                    .or_else(|| override_path("CODEBUDDY_CONFIG_DIR"))
                    .unwrap_or_else(|| self.environment.home.join(".workbuddy"))
                    .join("models.json")
            });
        }
        if tool == ToolId::OpenCode {
            if let Some(path) = std::env::var_os("OPENCODE_CONFIG")
                .filter(|v| !v.to_string_lossy().trim().is_empty())
            {
                return path.into();
            }
            let base = self.environment.home.join(".config/opencode");
            return if base.join("opencode.jsonc").exists() {
                base.join("opencode.jsonc")
            } else {
                base.join("opencode.json")
            };
        }
        get_adapter(tool).config_path()
    }
    /// Rechecks the filesystem immediately before a connection operation begins.
    pub fn require_installed(&self, tool: ToolId) -> Result<()> {
        if self
            .environment
            .command_path(tool.command())
            .is_some_and(|path| {
                crate::installation::attempt::incomplete(&self.environment, tool, &path)
            })
            && self.application_path(tool).is_none()
        {
            bail!("上次安装尚未完成，请重试 tokenmarket install {tool}；已准备的依赖保留")
        }
        if !self.is_installed(tool) {
            bail!(
                "{} 未安装，请先从官网下载并重新检测",
                get_adapter(tool).name
            );
        }
        Ok(())
    }
    pub fn connection_guard(&self, tool: ToolId) -> Result<crate::installation::InstallationGuard> {
        if tool == ToolId::QwenWork {
            crate::qwenwork::require_ready(&self.environment.home, self.environment.platform)?;
            crate::qwenwork::require_stopped(&self.config_path(tool))?;
        }
        if matches!(tool, ToolId::DuMate | ToolId::DuMateEnterprise) {
            let path = if tool == ToolId::DuMateEnterprise {
                crate::dumate::enterprise_config(&self.environment.home, self.environment.platform)?
            } else {
                crate::dumate::personal_config(&self.environment.home, self.environment.platform)?
            };
            crate::dumate::require_stopped(&path)?;
        }
        crate::installation::InstallationManager::new(
            self.environment.clone(),
            std::env::consts::ARCH.into(),
            std::sync::Arc::new(crate::installation::NativeInstallExecutor),
        )
        .connection_guard(tool)
    }
    pub fn is_installed(&self, tool: ToolId) -> bool {
        if matches!(
            tool,
            ToolId::DuMate | ToolId::DuMateEnterprise | ToolId::QwenWork
        ) {
            return self.application_path(tool).is_some();
        }
        self.environment
            .command_path(tool.command())
            .is_some_and(|path| {
                !crate::installation::attempt::incomplete(&self.environment, tool, &path)
            })
            || self.application_path(tool).is_some()
    }
    /// Verified application launch target; command launchers are handled separately.
    pub fn application_path(&self, tool: ToolId) -> Option<PathBuf> {
        let (folders, files): (&[&str], &[&str]) = match tool {
            ToolId::DuMate | ToolId::DuMateEnterprise => (&["DuMate", "dumate"], &["DuMate.exe"]),
            ToolId::WorkBuddy => (&["WorkBuddy"], &["WorkBuddy.exe"]),
            ToolId::QwenWork => (&["QwenWorkCN"], &["QwenWorkCN.exe"]),
            ToolId::DeepSeekHarness => (
                &["DeepSeek Harness", "deepseek-harness"],
                &["DeepSeek Harness.exe", "deepseek-harness.exe"],
            ),
            ToolId::OpenClaw => (
                &["OpenClaw", "OpenClaw Companion"],
                &["OpenClaw.exe", "OpenClawCompanion.exe"],
            ),
            ToolId::OpenCode => (&["OpenCode", "opencode"], &["OpenCode.exe", "opencode.exe"]),
            _ => return None,
        };
        for directory in &self.environment.application_directories {
            if self.environment.platform == HostPlatform::MacOs {
                for name in folders {
                    let bundle = directory.join(format!("{name}.app"));
                    let identified = !matches!(tool, ToolId::DuMate | ToolId::DuMateEnterprise)
                        || plist::Value::from_file(bundle.join("Contents/Info.plist"))
                            .ok()
                            .and_then(|v| {
                                v.as_dictionary()
                                    .and_then(|d| d.get("CFBundleIdentifier"))
                                    .and_then(plist::Value::as_string)
                                    .map(str::to_owned)
                            })
                            .as_deref()
                            == Some("com.baidu.qianfan.desktop");
                    let identified = identified
                        && (tool != ToolId::QwenWork
                            || plist::Value::from_file(bundle.join("Contents/Info.plist"))
                                .ok()
                                .and_then(|v| {
                                    v.as_dictionary()?
                                        .get("CFBundleIdentifier")?
                                        .as_string()
                                        .map(str::to_owned)
                                })
                                .as_deref()
                                == Some("cn.qwenwork.desktop.mac"));
                    if identified && macos_bundle_installed(&bundle) {
                        return Some(bundle);
                    }
                }
            } else if self.environment.platform == HostPlatform::Windows {
                for folder in std::iter::once(directory.clone())
                    .chain(folders.iter().map(|name| directory.join(name)))
                {
                    if let Ok(entries) = fs::read_dir(folder) {
                        for entry in entries.flatten() {
                            if files.iter().any(|name| {
                                entry
                                    .file_name()
                                    .to_string_lossy()
                                    .eq_ignore_ascii_case(name)
                            }) && is_executable(&entry.path(), HostPlatform::Windows)
                            {
                                return Some(entry.path());
                            }
                        }
                    }
                }
            }
        }
        None
    }
    pub fn harness_application_path(&self) -> Option<PathBuf> {
        self.application_path(ToolId::DeepSeekHarness)
    }
}

pub fn official_download_url(tool: ToolId) -> &'static str {
    match tool {
        ToolId::ClaudeCode => "https://code.claude.com/docs/en/setup",
        ToolId::Codex => "https://developers.openai.com/codex/cli/",
        ToolId::WorkBuddy => "https://www.workbuddy.ai/",
        ToolId::QwenWork => "https://qwenwork.cn/download",
        ToolId::DeepSeekHarness => "https://www.deepseek.com/download/",
        ToolId::OpenClaw => "https://docs.openclaw.ai/install",
        ToolId::OpenCode => "https://opencode.ai/docs/",
        ToolId::GrokBuild => "https://github.com/xai-org/grok-build",
        ToolId::Hermes => "https://hermes-agent.nousresearch.com/docs/getting-started/installation",
        ToolId::Pi => "https://pi.dev/",
        ToolId::MiniMaxCode => "https://github.com/MiniMax-AI/minimax-code/releases/tag/v0.7.2",
        ToolId::OhMyPi => "https://omp.sh/",
        ToolId::KimiCode => "https://github.com/MoonshotAI/kimi-code",
        ToolId::DuMate | ToolId::DuMateEnterprise => "https://www.dumate.cn/",
    }
}

impl InstallationEnvironment {
    pub(crate) fn has_command(&self, command: &str) -> bool {
        self.command_path(command).is_some()
    }
    pub(crate) fn command_path(&self, command: &str) -> Option<PathBuf> {
        self.command_paths(command).into_iter().next()
    }
    pub(crate) fn command_paths(&self, command: &str) -> Vec<PathBuf> {
        let mut result = vec![];
        let mut seen = std::collections::BTreeSet::new();
        for directory in &self.command_directories {
            let candidates = if self.platform != HostPlatform::Windows {
                vec![directory.join(command)]
            } else {
                let files = fs::read_dir(directory)
                    .map(|items| items.flatten().map(|e| e.path()).collect::<Vec<_>>())
                    .unwrap_or_default();
                self.executable_extensions
                    .iter()
                    .filter(|ext| {
                        [".exe", ".com", ".bat", ".cmd"]
                            .contains(&ext.to_ascii_lowercase().as_str())
                    })
                    .flat_map(|ext| {
                        files
                            .iter()
                            .filter(move |p| {
                                p.file_name().is_some_and(|n| {
                                    n.to_string_lossy()
                                        .eq_ignore_ascii_case(&format!("{command}{ext}"))
                                })
                            })
                            .cloned()
                    })
                    .collect()
            };
            for path in candidates {
                let identity = if self.platform == HostPlatform::Windows {
                    path.to_string_lossy().to_lowercase()
                } else {
                    path.to_string_lossy().into_owned()
                };
                if is_executable(&path, self.platform) && seen.insert(identity) {
                    result.push(path);
                }
            }
        }
        result
    }
}

fn is_executable(path: &Path, platform: HostPlatform) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    if platform == HostPlatform::Windows {
        return true;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        // Non-Unix hosts have no POSIX execute bit. The current native Windows
        // inventory already uses its Windows branch above; allow declared-platform
        // filesystem fixtures to model macOS command presence on that host too.
        true
    }
}

fn macos_bundle_installed(path: &Path) -> bool {
    let Ok(info) = plist::Value::from_file(path.join("Contents/Info.plist")) else {
        return false;
    };
    let Some(executable) = info
        .as_dictionary()
        .and_then(|d| d.get("CFBundleExecutable"))
        .and_then(plist::Value::as_string)
    else {
        return false;
    };
    // CFBundleExecutable is a filename, not an arbitrary relative/absolute path.
    if Path::new(executable).components().count() != 1 || executable == "." || executable == ".." {
        return false;
    }
    is_executable(
        &path.join("Contents/MacOS").join(executable),
        HostPlatform::MacOs,
    )
}

#[cfg(windows)]
fn registered_application_directories() -> Vec<PathBuf> {
    use winreg::{
        enums::{
            HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
        },
        RegKey,
    };
    let mut paths = vec![];
    for hive in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        for view in [KEY_WOW64_32KEY, KEY_WOW64_64KEY] {
            let Ok(uninstall) = RegKey::predef(hive).open_subkey_with_flags(
                r"Software\Microsoft\Windows\CurrentVersion\Uninstall",
                KEY_READ | view,
            ) else {
                continue;
            };
            for key in uninstall.enum_keys().flatten() {
                let Ok(app) = uninstall.open_subkey_with_flags(key, KEY_READ | view) else {
                    continue;
                };
                let Ok(name) = app.get_value::<String, _>("DisplayName") else {
                    continue;
                };
                if ![
                    "workbuddy",
                    "deepseek harness",
                    "opencode",
                    "openclaw",
                    "dumate",
                    "百度搭子",
                ]
                .iter()
                .any(|product| name.to_ascii_lowercase().contains(product))
                {
                    continue;
                }
                if let Ok(location) = app.get_value::<String, _>("InstallLocation") {
                    if !location.trim().is_empty() {
                        paths.push(PathBuf::from(location));
                    }
                }
            }
        }
    }
    paths
}

#[cfg(windows)]
fn registered_command_directories() -> Vec<PathBuf> {
    use winreg::{
        enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE},
        RegKey,
    };
    let mut paths = vec![];
    for (hive, key) in [
        (HKEY_CURRENT_USER, r"Environment"),
        (
            HKEY_LOCAL_MACHINE,
            r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
        ),
    ] {
        let Ok(environment) = RegKey::predef(hive).open_subkey(key) else {
            continue;
        };
        let Ok(path) = environment.get_value::<String, _>("Path") else {
            continue;
        };
        for value in path.split(';').filter(|value| !value.trim().is_empty()) {
            // Registry PATH may be REG_EXPAND_SZ. Only expand environment
            // variables, never invoke a shell or launcher during discovery.
            let mut expanded = String::new();
            let mut rest = value.trim().trim_matches('"');
            while let Some(start) = rest.find('%') {
                expanded.push_str(&rest[..start]);
                let tail = &rest[start + 1..];
                let Some(end) = tail.find('%') else {
                    expanded.push_str(&rest[start..]);
                    rest = "";
                    break;
                };
                let variable = &tail[..end];
                expanded
                    .push_str(&std::env::var(variable).unwrap_or_else(|_| format!("%{variable}%")));
                rest = &tail[end + 1..];
            }
            expanded.push_str(rest);
            paths.push(PathBuf::from(expanded));
        }
    }
    paths
}

/// An inspectable OS command plan. Creating it only discovers files; it never launches a process.
#[derive(Debug)]
pub struct ToolLaunchPlan {
    pub program: String,
    pub args: Vec<String>,
    pub environment: Vec<(String, String)>,
    pub wait_for_exit: bool,
}
impl ToolInventory {
    /// Foreground terminal/SSH launch. No shell lookup or persistent PATH changes
    /// are required for a private wrapper; execution remains an explicit action.
    pub fn terminal_launch_plan(&self, tool: ToolId) -> Result<ToolLaunchPlan> {
        self.require_installed(tool)?;
        if matches!(
            tool,
            ToolId::DuMate | ToolId::DuMateEnterprise | ToolId::QwenWork
        ) {
            return self.launch_plan(tool);
        }
        let command = tool.command();
        let target = self
            .environment
            .command_path(command)
            .context("未检测到命令行启动文件，请从系统应用入口打开此工具")?;
        if crate::installation::attempt::incomplete(&self.environment, tool, &target) {
            bail!("命令行安装尚未完成，请重试 tokenmarket install {tool}；桌面应用可单独打开")
        }
        let mut directories = vec![target.parent().context("工具启动目录无效")?.to_path_buf()];
        directories.extend(
            self.environment
                .command_directories
                .iter()
                .filter(|p| !directories.contains(p))
                .cloned()
                .collect::<Vec<_>>(),
        );
        let delimiter = if self.environment.platform == HostPlatform::Windows {
            ";"
        } else {
            ":"
        };
        let path = directories
            .iter()
            .map(|p| p.to_string_lossy())
            .collect::<Vec<_>>()
            .join(delimiter);
        let mut environment = vec![("PATH".into(), path)];
        if tool == ToolId::MiniMaxCode {
            environment.push((
                "MINIMAX_DATA_DIR".into(),
                self.config_path(tool)
                    .parent()
                    .context("MiniMax Code 数据目录无效")?
                    .to_string_lossy()
                    .into_owned(),
            ));
        }
        if tool == ToolId::KimiCode
            && self.environment.platform == HostPlatform::Windows
            && std::env::var_os("KIMI_SHELL_PATH").is_none_or(|v| v.is_empty())
        {
            if let Some(bash) = crate::installation::portable_git::bash_path(&self.environment) {
                environment.push((
                    "KIMI_SHELL_PATH".into(),
                    bash.to_string_lossy().into_owned(),
                ));
            }
        }
        if self.environment.platform == HostPlatform::Windows {
            // CMD/BAT need cmd.exe. Only the fixed catalogue command enters cmd
            // text; discovered paths (including &, spaces and Unicode) stay in PATH.
            return Ok(ToolLaunchPlan {
                program: "cmd.exe".into(),
                args: vec![
                    "/d".into(),
                    "/c".into(),
                    if tool == ToolId::OpenClaw {
                        "openclaw tui".into()
                    } else {
                        command.into()
                    },
                ],
                environment,
                wait_for_exit: true,
            });
        }
        Ok(ToolLaunchPlan {
            program: target.to_string_lossy().into_owned(),
            args: if tool == ToolId::OpenClaw {
                vec!["tui".into()]
            } else {
                vec![]
            },
            environment,
            wait_for_exit: true,
        })
    }
    /// Desktop discovery and foreground CLI launch share the executable/env plan.
    pub fn launch_plan(&self, tool: ToolId) -> Result<ToolLaunchPlan> {
        self.require_installed(tool)?;
        if let Some(path) = self.application_path(tool) {
            return match self.environment.platform {
                HostPlatform::MacOs => Ok(ToolLaunchPlan {
                    program: "open".into(),
                    args: vec!["-a".into(), path.to_string_lossy().into()],
                    environment: vec![],
                    wait_for_exit: true,
                }),
                HostPlatform::Windows => Ok(ToolLaunchPlan {
                    program: path.to_string_lossy().into(),
                    args: vec![],
                    environment: vec![],
                    wait_for_exit: false,
                }),
                HostPlatform::Unix => bail!("请从系统应用入口打开工具"),
            };
        }
        let foreground = self.terminal_launch_plan(tool)?;
        if self.environment.platform == HostPlatform::Windows {
            return Ok(ToolLaunchPlan {
                program: "cmd".into(),
                args: [
                    "/c",
                    "start",
                    "",
                    "cmd",
                    "/k",
                    foreground.args.last().unwrap(),
                ]
                .map(str::to_owned)
                .to_vec(),
                environment: foreground.environment,
                wait_for_exit: false,
            });
        }
        if self.environment.platform == HostPlatform::MacOs {
            fn quote(value: &str) -> String {
                format!("'{}'", value.replace('\'', "'\"'\"'"))
            }
            let variables = foreground
                .environment
                .iter()
                .map(|(k, v)| format!("{k}={}", quote(v)))
                .collect::<Vec<_>>()
                .join(" ");
            let args = foreground
                .args
                .iter()
                .map(|v| quote(v))
                .collect::<Vec<_>>()
                .join(" ");
            let terminal = format!("env {variables} {} {args}", quote(&foreground.program));
            let script = terminal.replace('\\', "\\\\").replace('\"', "\\\"");
            return Ok(ToolLaunchPlan {
                program: "osascript".into(),
                args: vec![
                    "-e".into(),
                    "tell application \"Terminal\" to activate".into(),
                    "-e".into(),
                    format!("tell application \"Terminal\" to do script \"{script}\""),
                ],
                environment: vec![],
                wait_for_exit: true,
            });
        }
        Ok(foreground)
    }
}
