//! CC Switch-style uninstall: prove ownership, preview exact target, rederive
//! under the lifecycle lock, remove only program files, retain configuration.
use super::{
    discovery, installation_lock, policy, upgrades, InstallStage, InstallStatus,
    InstallationManager, Job,
};
use crate::{
    inventory::{HostPlatform, InstallationEnvironment, InstallationLocation, ToolInventory},
    ToolId,
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UninstallPlan {
    pub tool: ToolId,
    pub path: String,
    pub real_path: String,
    pub command: String,
    pub native: bool,
    pub files: Vec<PathBuf>,
    pub revision: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallCandidate {
    pub installation: InstallationLocation,
    pub plan: Option<UninstallPlan>,
    pub unavailable_reason: Option<String>,
    pub reinstalled_by_updater: bool,
    pub version: Option<String>,
    pub error: Option<String>,
    pub is_path_default: bool,
}

pub(super) fn known_native(tool: ToolId, real: &Path, env: &InstallationEnvironment) -> bool {
    if !discovery::is_native(real) || real.components().any(|c| c.as_os_str() == "node_modules") {
        return false;
    }
    let home = env.home.canonicalize().unwrap_or_else(|_| env.home.clone());
    let rel = real
        .strip_prefix(&home)
        .ok()
        .map(|p| p.to_string_lossy().replace('\\', "/"));
    let Some(rel) = rel else { return false };
    match tool {
        ToolId::ClaudeCode => {
            rel.strip_prefix(".local/share/claude/versions/")
                .is_some_and(|v| !v.is_empty() && !v.contains('/'))
                || matches!(rel.as_str(), ".local/bin/claude" | ".local/bin/claude.exe")
        }
        ToolId::Codex => rel
            .split_once("/packages/standalone/releases/")
            .and_then(|(_, s)| s.split_once('/'))
            .is_some_and(|(v, p)| {
                !v.is_empty() && matches!(p, "bin/codex" | "bin/codex.exe" | "codex" | "codex.exe")
            }),
        ToolId::OpenCode => matches!(
            rel.as_str(),
            ".opencode/bin/opencode" | ".opencode/bin/opencode.exe"
        ),
        ToolId::OhMyPi => matches!(rel.as_str(), ".local/bin/omp" | "AppData/Local/omp/omp.exe"),
        ToolId::GrokBuild => grok_binary(real, &home.join(".grok")),
        _ => false,
    }
}
fn version_name(v: &str) -> bool {
    v.starts_with(|c: char| c.is_ascii_digit())
        && v.chars()
            .all(|c| c.is_ascii_alphanumeric() || ".-+".contains(c))
}
fn grok_binary(path: &Path, root: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|p| p.to_str()) else {
        return false;
    };
    let Some(parent) = path.parent() else {
        return false;
    };
    let name = name.strip_suffix(".exe").unwrap_or(name);
    if parent == root.join("bin") {
        return name == "grok" || name.strip_prefix("grok-").is_some_and(version_name);
    }
    if parent != root.join("downloads") {
        return false;
    }
    let Some(rest) = name.strip_prefix("grok-") else {
        return false;
    };
    [
        "macos-aarch64",
        "macos-x86_64",
        "linux-aarch64",
        "linux-x86_64",
        "windows-aarch64",
        "windows-x86_64",
    ]
    .iter()
    .any(|p| {
        rest == *p
            || rest
                .strip_suffix(p)
                .and_then(|v| v.strip_suffix('-'))
                .is_some_and(version_name)
    })
}
pub(super) fn verified_npm_prefix(
    tool: ToolId,
    entry: &Path,
    env: &InstallationEnvironment,
) -> Option<PathBuf> {
    let package = upgrades::package(tool)?;
    let bin = entry.parent()?;
    let windows = env.platform == HostPlatform::Windows;
    let prefix = if windows {
        if bin.file_name()? == ".bin" {
            return None;
        }
        bin
    } else {
        if bin.file_name()? != "bin" {
            return None;
        }
        bin.parent()?
    };
    let root = prefix
        .join(if windows {
            "node_modules"
        } else {
            "lib/node_modules"
        })
        .join(package)
        .canonicalize()
        .ok()?;
    if !upgrades::package_identity(&root, package) {
        return None;
    }
    let real = entry.canonicalize().ok()?;
    let shim = windows
        && !discovery::is_native(entry)
        && std::fs::read_to_string(entry)
            .ok()?
            .replace('\\', "/")
            .contains(&format!("node_modules/{package}/"));
    if real.starts_with(&root) || shim {
        Some(prefix.into())
    } else {
        None
    }
}
pub(super) fn brew_owner(tool: ToolId, real: &Path) -> Option<(PathBuf, &'static str, String)> {
    for root in real.ancestors() {
        let parent = root.parent()?;
        let kind = parent.file_name()?.to_str()?;
        if !matches!(kind, "Cellar" | "Caskroom") {
            continue;
        }
        let token = root.file_name()?.to_str()?;
        let allowed = match tool {
            ToolId::ClaudeCode => {
                kind == "Caskroom" && matches!(token, "claude-code" | "claude-code@latest")
            }
            ToolId::Codex => kind == "Caskroom" && token == "codex",
            ToolId::OpenCode => kind == "Cellar" && token == "opencode",
            _ => false,
        };
        if !allowed {
            return None;
        }
        return Some((
            parent.parent()?.join("bin/brew"),
            if kind == "Caskroom" {
                "--cask"
            } else {
                "--formula"
            },
            token.into(),
        ));
    }
    None
}
fn command_for(env: &InstallationEnvironment, program: &Path, args: &[String]) -> String {
    let q = if env.platform == HostPlatform::Windows {
        policy::ps
    } else {
        policy::quote
    };
    format!(
        "{}{} {}",
        if env.platform == HostPlatform::Windows {
            "& "
        } else {
            ""
        },
        q(&program.to_string_lossy()),
        args.iter().map(|s| q(s)).collect::<Vec<_>>().join(" ")
    )
}
fn derive(
    tool: ToolId,
    location: &InstallationLocation,
    env: &InstallationEnvironment,
) -> Result<(UninstallPlan, Vec<super::InstallStep>)> {
    let entry = PathBuf::from(&location.path);
    let real = entry.canonicalize().context("安装路径已改变，请重新检测")?;
    if real.to_string_lossy().contains(".app/Contents/") || location.method == "桌面应用" {
        bail!("请通过系统应用管理或原安装器卸载；保留配置和用户数据")
    }
    let mut files = vec![];
    let mut steps = vec![];
    let mut display = vec![];
    let brew = real
        .components()
        .any(|p| matches!(p.as_os_str().to_str(), Some("Cellar" | "Caskroom")));
    let package_step = if brew {
        let (program, kind, owner) =
            brew_owner(tool, &real).context("无法确认 Homebrew 包归属，请通过原包管理器卸载")?;
        if !program.is_file() {
            bail!("未找到原 Homebrew 程序，请通过原包管理器卸载")
        }
        Some((program, vec!["uninstall".into(), kind.into(), owner]))
    } else if tool == ToolId::Hermes {
        Some((entry.clone(), vec!["uninstall".into(), "--yes".into()]))
    } else if !matches!(location.method, "Bun" | "pnpm" | "Yarn")
        && !entry.to_string_lossy().contains(".volta")
    {
        verified_npm_prefix(tool, &entry, env).map(|prefix| {
            let sibling = entry
                .parent()
                .unwrap()
                .join(if env.platform == HostPlatform::Windows {
                    "npm.cmd"
                } else {
                    "npm"
                });
            let npm = if sibling.is_file() {
                sibling
            } else {
                env.command_path("npm").unwrap_or_default()
            };
            (
                npm,
                vec![
                    "uninstall".into(),
                    "--global".into(),
                    "--prefix".into(),
                    prefix.to_string_lossy().into(),
                    upgrades::package(tool).unwrap().into(),
                ],
            )
        })
    } else {
        None
    };
    if let Some((program, args)) = package_step {
        if !program.is_file() {
            bail!("未找到原包管理器，请按原渠道卸载")
        }
        if tool == ToolId::OpenClaw {
            let args = vec![
                "uninstall".into(),
                "--service".into(),
                "--yes".into(),
                "--non-interactive".into(),
            ];
            display.push(command_for(env, &entry, &args));
            steps.push(policy::step(env, &entry, args));
        }
        display.push(command_for(env, &program, &args));
        steps.push(policy::step(env, &program, args));
    } else if known_native(tool, &real, env) {
        files.push(entry.clone());
        for candidate in env.command_paths(tool.command()) {
            if std::fs::symlink_metadata(&candidate).is_ok_and(|m| m.file_type().is_symlink())
                && candidate.canonicalize().ok().as_ref() == Some(&real)
                && !files.contains(&candidate)
            {
                files.push(candidate);
            }
        }
        if !files.contains(&real) {
            files.push(real.clone());
        }
        if tool == ToolId::GrokBuild {
            let root = env
                .home
                .join(".grok")
                .canonicalize()
                .unwrap_or_else(|_| env.home.join(".grok"));
            for kind in ["bin", "downloads"] {
                if let Ok(entries) = std::fs::read_dir(root.join(kind)) {
                    for item in entries.flatten() {
                        let path = item.path();
                        let meta = std::fs::symlink_metadata(&path)?;
                        let link = kind == "bin"
                            && matches!(
                                item.file_name().to_str(),
                                Some("grok" | "grok.exe" | "agent" | "agent.exe")
                            )
                            && meta.file_type().is_symlink()
                            && path.canonicalize().is_ok_and(|r| grok_binary(&r, &root));
                        if (link
                            || (meta.is_file()
                                && grok_binary(&path, &root)
                                && discovery::is_native(&path)))
                            && !files.contains(&path)
                        {
                            files.push(path);
                        }
                    }
                }
            }
            files.sort_by_key(|p| {
                !std::fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_symlink())
            });
        }
        let mut seen = std::collections::BTreeSet::new();
        files = files
            .into_iter()
            .map(|p| {
                p.parent()
                    .and_then(|parent| parent.canonicalize().ok())
                    .and_then(|parent| p.file_name().map(|name| parent.join(name)))
                    .unwrap_or(p)
            })
            .filter(|p| seen.insert(p.clone()))
            .collect();
        files.sort_by_key(|p| {
            !std::fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_symlink())
        });
        display.push(format!(
            "删除程序文件：{}",
            files
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join("、")
        ));
    } else {
        bail!("无法确认此安装的卸载归属，请使用原安装器或包管理器；配置和用户数据保留")
    }
    let mut hash = Sha256::new();
    for p in std::iter::once(&entry)
        .chain(std::iter::once(&real))
        .chain(files.iter())
    {
        hash.update(p.to_string_lossy().as_bytes());
        let meta = std::fs::symlink_metadata(p)?;
        hash.update(meta.len().to_le_bytes());
        hash.update(format!("{:?}", meta.modified()?).as_bytes());
        if meta.is_dir() {
            bail!("卸载目标包含目录，已拒绝删除")
        }
    }
    if let Some(prefix) = verified_npm_prefix(tool, &entry, env) {
        let manifest = prefix
            .join(if env.platform == HostPlatform::Windows {
                "node_modules"
            } else {
                "lib/node_modules"
            })
            .join(upgrades::package(tool).unwrap())
            .join("package.json");
        hash.update(std::fs::read(manifest)?);
    }
    let plan = UninstallPlan {
        tool,
        path: location.path.clone(),
        real_path: real.to_string_lossy().into(),
        command: display.join(" && "),
        native: steps.is_empty(),
        files,
        revision: format!("{:x}", hash.finalize()),
    };
    Ok((plan, steps))
}
impl InstallationManager {
    pub fn uninstall_candidates(&self, tool: ToolId) -> Vec<UninstallCandidate> {
        let default = self
            .environment
            .path_default_command(tool.command())
            .and_then(|p| p.canonicalize().ok());
        let grok_npm = tool == ToolId::GrokBuild
            && std::fs::read_to_string(self.environment.home.join(".grok/config.toml"))
                .ok()
                .and_then(|s| s.parse::<toml_edit::DocumentMut>().ok())
                .is_some_and(|d| d["cli"]["installer"].as_str() == Some("npm"));
        ToolInventory::new(self.environment.clone())
            .installation_locations(tool)
            .into_iter()
            .map(|installation| {
                let entry = Path::new(&installation.path);
                let is_path_default = default.is_some() && entry.canonicalize().ok() == default;
                let version = if installation.method == "桌面应用" {
                    self.executor.tool_application_version(tool, entry)
                } else {
                    self.executor.version(entry)
                };
                let (version, error) = match version {
                    Ok(version) => (Some(version), None),
                    Err(error) => (None, Some(error.to_string())),
                };
                let (plan, unavailable_reason) =
                    match derive(tool, &installation, &self.environment) {
                        Ok((plan, _)) => (Some(plan), None),
                        Err(e) => (None, Some(e.to_string())),
                    };
                let reinstalled_by_updater = grok_npm && plan.as_ref().is_some_and(|p| !p.native);
                UninstallCandidate {
                    installation,
                    plan,
                    unavailable_reason,
                    reinstalled_by_updater,
                    version,
                    error,
                    is_path_default,
                }
            })
            .collect()
    }
    pub fn start_uninstall(&self, tool: ToolId, confirmed: UninstallPlan) -> Result<InstallStatus> {
        if confirmed.tool != tool {
            bail!("卸载工具与确认目标不一致")
        }
        let guard = installation_lock(&self.environment, tool)?;
        let location = ToolInventory::new(self.environment.clone())
            .installation_locations(tool)
            .into_iter()
            .find(|p| p.path == confirmed.path)
            .context("安装已改变，请重新检测并确认卸载")?;
        let (plan, steps) = derive(tool, &location, &self.environment)?;
        if plan != confirmed {
            bail!("安装或卸载方案已改变，请重新确认")
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        let status = InstallStatus {
            tool,
            stage: InstallStage::Preparing,
            output: vec![plan.command.clone()],
            output_sequence: 1,
            detail: "正在卸载选定安装；配置和用户数据保留".into(),
            installed: true,
            version: None,
        };
        self.jobs.lock().unwrap().insert(
            tool.to_string(),
            Job {
                status: status.clone(),
                cancelled: cancelled.clone(),
            },
        );
        let manager = self.clone();
        std::thread::spawn(move || {
            let result = (|| -> Result<()> {
                let (current, _) = derive(tool, &location, &manager.environment)?;
                if current != plan {
                    bail!("安装在执行前已改变，请重新确认")
                }
                manager.update(tool, |state| state.stage = InstallStage::Installing);
                for step in steps {
                    if cancelled.load(Ordering::SeqCst) {
                        bail!("卸载已取消")
                    }
                    manager.executor.execute(&step, &cancelled, &mut |line| {
                        manager.update(tool, |s| {
                            s.output.push(line);
                            s.output_sequence += 1;
                        })
                    })?;
                }
                for file in &plan.files {
                    if cancelled.load(Ordering::SeqCst) {
                        bail!("卸载已取消，已删除的程序文件不会自动恢复")
                    }
                    let meta = std::fs::symlink_metadata(file)?;
                    if !meta.is_file() && !meta.file_type().is_symlink() {
                        bail!("卸载目标类型已改变，已停止")
                    }
                    std::fs::remove_file(file)
                        .with_context(|| format!("无法删除 {}", file.display()))?;
                }
                manager.update(tool, |state| state.stage = InstallStage::Detecting);
                match std::fs::symlink_metadata(&plan.path) {
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    _ => bail!("卸载命令已结束，但选定入口仍存在，请按原渠道处理"),
                }
                super::attempt::complete(&manager.environment, tool)?;
                Ok(())
            })();
            let installed = ToolInventory::new(manager.environment.clone()).is_installed(tool);
            drop(guard);
            manager
                .upgrade_plans
                .lock()
                .unwrap()
                .remove(&tool.to_string());
            manager.update(tool, |s| {
                s.installed = installed;
                s.stage = if result.is_ok() {
                    InstallStage::Succeeded
                } else if cancelled.load(Ordering::SeqCst) {
                    InstallStage::Cancelled
                } else {
                    InstallStage::Failed
                };
                s.detail = match result {
                    Ok(()) => if installed {
                        "选定安装已卸载，仍检测到其他安装；配置和用户数据保留"
                    } else {
                        "选定安装已卸载；配置和用户数据保留"
                    }
                    .into(),
                    Err(e) => e.to_string(),
                };
            });
        });
        Ok(status)
    }
}
