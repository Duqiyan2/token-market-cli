//! Tool lifecycle policy, aligned with CC Switch e217bfc. Commands are generated
//! from tool identity and platform; UI hints and execution share this plan.
use super::{InstallMethod, InstallPlan, InstallStep};
use crate::{
    inventory::{HostPlatform, InstallationEnvironment},
    ToolId,
};
use anyhow::{bail, Result};
use std::path::Path;

pub(super) fn is_desktop(tool: ToolId) -> bool {
    matches!(
        tool,
        ToolId::WorkBuddy | ToolId::DuMate | ToolId::DuMateEnterprise | ToolId::QwenWork
    )
}
pub(super) fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}
pub(super) fn ps(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
pub(super) fn extra_args(tool: ToolId) -> Vec<String> {
    match tool {
        ToolId::DeepSeekHarness => vec!["--engine-strict".into()],
        ToolId::ClaudeCode | ToolId::OpenCode | ToolId::GrokBuild | ToolId::MiniMaxCode => vec![
            "--ignore-scripts=false".into(),
            "--include=optional".into(),
            format!(
                "--allow-scripts={}{}",
                super::upgrades::package(tool).unwrap(),
                if tool == ToolId::MiniMaxCode {
                    ",better-sqlite3"
                } else {
                    ""
                }
            ),
        ],
        _ => vec![],
    }
}
pub(super) fn step(
    env: &InstallationEnvironment,
    program: &Path,
    args: Vec<String>,
) -> InstallStep {
    super::upgrades::step(
        env,
        program,
        args,
        program.parent().map(|p| vec![p.into()]).unwrap_or_default(),
    )
}
pub(super) fn shell_step(env: &InstallationEnvironment, command: String) -> InstallStep {
    let windows = env.platform == HostPlatform::Windows;
    step(
        env,
        Path::new(if windows { "powershell.exe" } else { "/bin/sh" }),
        if windows {
            vec![
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-ExecutionPolicy".into(),
                "Bypass".into(),
                "-Command".into(),
                command,
            ]
        } else {
            vec!["-c".into(), command]
        },
    )
}
pub(super) fn installer(tool: ToolId, windows: bool) -> Option<(&'static str, &'static str)> {
    Some(match tool {
        ToolId::ClaudeCode => (
            if windows {
                "https://claude.ai/install.ps1"
            } else {
                "https://claude.ai/install.sh"
            },
            "bash",
        ),
        ToolId::Codex => (
            if windows {
                "https://chatgpt.com/codex/install.ps1"
            } else {
                "https://chatgpt.com/codex/install.sh"
            },
            "sh",
        ),
        ToolId::OpenCode => ("https://opencode.ai/install", "bash"),
        ToolId::GrokBuild => (
            if windows {
                "https://x.ai/cli/install.ps1"
            } else {
                "https://x.ai/cli/install.sh"
            },
            "bash",
        ),
        ToolId::MiniMaxCode => (
            if windows {
                "https://filecdn.minimax.chat/public/install.ps1"
            } else {
                "https://filecdn.minimax.chat/public/install.sh"
            },
            "bash",
        ),
        ToolId::Hermes => (
            if windows {
                "https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.ps1"
            } else {
                "https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.sh"
            },
            "bash",
        ),
        ToolId::KimiCode => (
            if windows {
                "https://code.kimi.com/kimi-code/install.ps1"
            } else {
                "https://code.kimi.com/kimi-code/install.sh"
            },
            "bash",
        ),
        ToolId::OhMyPi => (
            if windows {
                "https://omp.sh/install.ps1"
            } else {
                "https://omp.sh/install"
            },
            "sh",
        ),
        _ => return None,
    })
}
pub(super) fn installer_command(tool: ToolId, env: &InstallationEnvironment) -> Result<String> {
    let windows = env.platform == HostPlatform::Windows;
    let (url, shell) =
        installer(tool, windows).ok_or_else(|| anyhow::anyhow!("没有官方安装脚本"))?;
    Ok(if windows {
        format!("$ErrorActionPreference='Stop'; $f=Join-Path ([IO.Path]::GetTempPath()) ([IO.Path]::GetRandomFileName()+'.ps1'); try {{ Invoke-WebRequest -UseBasicParsing {} -OutFile $f; & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $f; if ($LASTEXITCODE -ne 0) {{ throw '官方安装器失败' }} }} finally {{ Remove-Item -LiteralPath $f -ErrorAction SilentlyContinue }}", ps(url))
    } else {
        format!("(tmp=$(mktemp) || exit; trap 'rm -f \"$tmp\"' EXIT; curl --fail --location --proto '=https' --proto-redir '=https' {} -o \"$tmp\" && {shell} \"$tmp\")", quote(url))
    })
}
pub(super) fn npm_command(tool: ToolId, env: &InstallationEnvironment) -> Result<String> {
    let package =
        super::upgrades::package(tool).ok_or_else(|| anyhow::anyhow!("该工具没有 npm 包"))?;
    let npm = env.command_path("npm").unwrap_or_else(|| {
        if env.platform == HostPlatform::Windows {
            "npm.cmd".into()
        } else {
            "npm".into()
        }
    });
    let mut args = vec![
        "install".into(),
        "--global".into(),
        format!("{package}@latest"),
    ];
    args.extend(extra_args(tool));
    Ok(if env.platform == HostPlatform::Windows {
        format!("& {} {}; if ($LASTEXITCODE -ne 0) {{ throw 'npm 安装失败，请检查 Node.js/npm 和输出' }}", ps(&npm.to_string_lossy()), args.iter().map(|a| ps(a)).collect::<Vec<_>>().join(" "))
    } else {
        format!(
            "{} {}",
            quote(&npm.to_string_lossy()),
            args.iter().map(|a| quote(a)).collect::<Vec<_>>().join(" ")
        )
    })
}
pub(super) fn install_plan(
    tool: ToolId,
    env: &InstallationEnvironment,
    arch: &str,
) -> Result<InstallPlan> {
    if !matches!(arch, "aarch64" | "x86_64") {
        bail!("官方安装方案不支持当前架构 {arch}")
    }
    let windows = env.platform == HostPlatform::Windows;
    let script_first = matches!(tool, ToolId::GrokBuild | ToolId::MiniMaxCode)
        || (!windows && matches!(tool, ToolId::ClaudeCode | ToolId::OpenCode));
    let script_only = matches!(tool, ToolId::Hermes | ToolId::KimiCode | ToolId::OhMyPi);
    let (method, command) = if script_first {
        let installer = installer_command(tool, env)?;
        let npm = npm_command(tool, env)?;
        (
            InstallMethod::ScriptWithNpmFallback,
            if windows {
                format!("try {{ {installer} }} catch {{ Write-Output '官方安装失败，尝试 npm'; {npm} }}")
            } else {
                format!("{installer} || {{ {npm}; }}")
            },
        )
    } else if script_only {
        (InstallMethod::Script, installer_command(tool, env)?)
    } else {
        (InstallMethod::Npm, npm_command(tool, env)?)
    };
    Ok(InstallPlan {
        tool,
        command: tool.command().into(),
        version: "latest".into(),
        method,
        steps: vec![shell_step(env, command)],
    })
}
fn update_args(tool: ToolId, windows: bool) -> Option<Vec<&'static str>> {
    match tool {
        ToolId::ClaudeCode | ToolId::Hermes | ToolId::OhMyPi => Some(vec!["update"]),
        ToolId::OpenClaw => Some(vec!["update", "--yes"]),
        ToolId::OpenCode if !windows => Some(vec!["upgrade"]),
        ToolId::KimiCode => Some(vec!["upgrade", "--yes"]),
        _ => None,
    }
}

pub(super) fn upgrade(
    tool: ToolId,
    latest: &str,
    env: &InstallationEnvironment,
    current_version: Option<&str>,
) -> Result<super::upgrades::Route> {
    use super::uninstall::{known_native, verified_npm_prefix};
    use super::upgrades::{Route, UpgradePlan};
    let inventory = crate::inventory::ToolInventory::new(env.clone());
    if is_desktop(tool)
        || (inventory.application_path(tool).is_some()
            && env.command_path(tool.command()).is_none())
    {
        return Ok(Route::Manual("请通过应用内更新或原安装渠道更新".into()));
    }
    let probe = env
        .command_path(tool.command())
        .ok_or_else(|| anyhow::anyhow!("未检测到当前安装"))?;
    let real = probe.canonicalize().unwrap_or_else(|_| probe.clone());
    let path = probe.to_string_lossy().replace('\\', "/");
    let target = shell_path(&real).replace('\\', "/");
    let windows = env.platform == HostPlatform::Windows;
    let anchored = env.path_default_command(tool.command()).is_some()
        || inventory.installation_locations(tool).len() == 1;
    let make = |mut steps: Vec<InstallStep>| {
        if target.contains("/node_modules/")
            && !target.contains("/Cellar/")
            && matches!(tool, ToolId::ClaudeCode | ToolId::OpenCode)
        {
            for step in &mut steps {
                step.environment.extend([
                    ("npm_config_ignore_scripts".into(), "false".into()),
                    ("npm_config_include".into(), "optional".into()),
                    (
                        "npm_config_allow_scripts".into(),
                        super::upgrades::package(tool).unwrap().into(),
                    ),
                ]);
            }
        }
        Route::Automatic(UpgradePlan {
            plan: InstallPlan {
                tool,
                command: tool.command().into(),
                version: latest.into(),
                method: InstallMethod::Script,
                steps,
            },
            probe: probe.clone(),
            managed: super::recipes::owned_command_paths(tool, env).contains(&probe),
            anchored: true,
        })
    };
    let make_fallback = |steps| {
        let mut route = make(steps);
        if let Route::Automatic(plan) = &mut route {
            plan.anchored = false;
        }
        route
    };
    if !anchored {
        let install = install_plan(tool, env, "x86_64")?;
        if let Some(args) = update_args(tool, windows) {
            let primary = command_text(
                env,
                Path::new(tool.command()),
                &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            );
            let fallback = install.steps[0].args.last().unwrap();
            let command = if windows {
                format!("try {{ {primary}; if ($LASTEXITCODE -ne 0) {{ throw '自更新失败' }} }} catch {{ {fallback} }}")
            } else {
                format!("{primary} || {fallback}")
            };
            return Ok(make_fallback(vec![shell_step(env, command)]));
        }
        return Ok(make_fallback(install.steps));
    }
    let self_update =
        |args: &[&str]| step(env, &probe, args.iter().map(|a| a.to_string()).collect());
    if tool == ToolId::Hermes {
        return Ok(make(vec![self_update(&["update"])]));
    }
    if tool == ToolId::ClaudeCode
        && (target.contains("/.local/share/claude/") || target.contains("/claude/versions/"))
    {
        return Ok(make(vec![self_update(&["update"])]));
    }
    let with_root = |key: &str, root: &str, command: String| -> String {
        if windows {
            format!("$env:{key}={}; {command}", ps(root))
        } else {
            format!("export {key}={}; {command}", quote(root))
        }
    };
    if tool == ToolId::Codex {
        if let Some((home, _)) = target.split_once("/packages/standalone/") {
            let command = with_root(
                "CODEX_HOME",
                home,
                with_root(
                    "CODEX_INSTALL_DIR",
                    &probe.parent().unwrap().to_string_lossy(),
                    installer_command(tool, env)?,
                ),
            );
            return Ok(make(vec![shell_step(
                env,
                with_root("CODEX_NON_INTERACTIVE", "1", command),
            )]));
        }
    }
    if tool == ToolId::MiniMaxCode {
        let root = env.home.join(".minimax-code");
        let receipt = [probe.as_path(), real.as_path()]
            .into_iter()
            .filter_map(|entry| {
                if windows {
                    entry.parent()
                } else {
                    entry.parent().and_then(Path::parent)
                }
            })
            .find(|r| {
                std::fs::read(r.join("install.json"))
                    .ok()
                    .and_then(|b| {
                        serde_json::from_slice::<serde_json::Value>(
                            b.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&b),
                        )
                        .ok()
                    })
                    .is_some_and(|v| {
                        v["product"] == "minimax-code"
                            && v["updateOwner"] == "npm-prefix"
                            && v["prefix"]
                                .as_str()
                                .and_then(|s| Path::new(s).canonicalize().ok())
                                == r.canonicalize().ok()
                    })
            });
        if target.starts_with(&root.to_string_lossy().to_string())
            || path.starts_with(&root.to_string_lossy().to_string())
            || receipt.is_some()
        {
            let root = receipt.unwrap_or(&root);
            return Ok(make(vec![shell_step(
                env,
                with_root(
                    "MCODE_INSTALL_DIR",
                    &root.to_string_lossy(),
                    installer_command(tool, env)?,
                ),
            )]));
        }
    }
    if tool == ToolId::GrokBuild && known_native(tool, &real, env) {
        let mut command = command_text(env, &probe, &["update".into()]);
        command = if windows {
            format!("$env:npm_config_ignore_scripts='false'; $env:npm_config_include='optional'; $env:npm_config_allow_scripts='@xai-official/grok'; try {{ {command}; if ($LASTEXITCODE -ne 0) {{ throw '更新失败' }} }} catch {{ {} }}",installer_command(tool,env)?)
        } else {
            format!("npm_config_ignore_scripts=false npm_config_include=optional npm_config_allow_scripts=@xai-official/grok {command} || {}",installer_command(tool,env)?)
        };
        return Ok(make(vec![shell_step(env, command)]));
    }
    // Homebrew must be handled before looking inside node_modules.
    for root in real.ancestors() {
        if let Some(parent) = root.parent() {
            if let Some(kind) = parent
                .file_name()
                .and_then(|p| p.to_str())
                .filter(|s| matches!(*s, "Cellar" | "Caskroom"))
            {
                let program = parent.parent().unwrap().join("bin/brew");
                if !program.is_file() {
                    return Ok(Route::Manual("未找到原 Homebrew，请通过原渠道更新".into()));
                }
                let mut args = vec!["upgrade".into()];
                if kind == "Caskroom" {
                    args.push("--cask".into());
                }
                args.push(root.file_name().unwrap().to_string_lossy().into());
                return Ok(make(vec![step(env, &program, args)]));
            }
        }
    }
    let sibling = |name: &str| {
        let parent = probe.parent().unwrap();
        let candidates = if windows {
            vec![
                parent.join(format!("{name}.cmd")),
                parent.join(format!("{name}.exe")),
            ]
        } else {
            vec![parent.join(name)]
        };
        candidates.into_iter().find(|p| p.is_file())
    };
    let package = super::upgrades::package(tool);
    let mut manager = if let Some(package) = package {
        if path.contains("/.volta/") || path.to_ascii_lowercase().contains("/volta/") {
            sibling("volta")
                .map(|p| step(env, &p, vec!["install".into(), format!("{package}@latest")]))
        } else if path.contains("/.bun/") {
            sibling("bun").map(|p| {
                step(
                    env,
                    &p,
                    vec!["add".into(), "--global".into(), format!("{package}@latest")],
                )
            })
        } else if windows && path.to_ascii_lowercase().contains("/pnpm/") {
            sibling("pnpm").map(|p| {
                step(
                    env,
                    &p,
                    vec!["add".into(), "--global".into(), format!("{package}@latest")],
                )
            })
        } else if let Some(prefix) = verified_npm_prefix(tool, &probe, env) {
            sibling("npm").or_else(|| env.command_path("npm")).map(|p| {
                let mut args = vec![
                    "install".into(),
                    "--global".into(),
                    "--prefix".into(),
                    prefix.to_string_lossy().into(),
                    format!("{package}@latest"),
                ];
                args.extend(extra_args(tool));
                step(env, &p, args)
            })
        } else {
            None
        }
    } else {
        None
    };
    if tool == ToolId::KimiCode
        && current_version.is_some_and(|v| {
            super::updates::compare_versions(v, "0.43.0") == Some(std::cmp::Ordering::Less)
        })
    {
        if let Some(manager) = manager.take() {
            return Ok(make(vec![manager]));
        }
        if !target.contains("/node_modules/")
            && real
                .parent()
                .is_some_and(|p| p.file_name().is_some_and(|n| n == "bin"))
        {
            let root = real.parent().and_then(Path::parent).unwrap();
            return Ok(make(vec![shell_step(
                env,
                with_root(
                    "KIMI_INSTALL_DIR",
                    &root.to_string_lossy(),
                    installer_command(tool, env)?,
                ),
            )]));
        }
    }
    let args = update_args(tool, windows);
    if let Some(args) = args {
        if tool == ToolId::OhMyPi && target.starts_with("/nix/store/") {
            return Ok(Route::Manual("请通过 Nix 更新此安装".into()));
        }
        let primary = command_text(
            env,
            &probe,
            &args.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
        );
        if let Some(manager) = manager {
            let fallback = command_text(env, Path::new(&manager.program), &manager.args);
            let command = if windows {
                format!("try {{ {primary}; if ($LASTEXITCODE -ne 0) {{ throw '自更新失败' }} }} catch {{ {fallback}; if ($LASTEXITCODE -ne 0) {{ throw '原包管理器更新失败' }} }}")
            } else {
                format!("{primary} || {fallback}")
            };
            let mut s = shell_step(env, command);
            s.environment.extend(manager.environment);
            return Ok(make(vec![s]));
        }
        return Ok(make(vec![self_update(&args)]));
    }
    if let Some(manager) = manager {
        return Ok(make(vec![manager]));
    }
    if super::discovery::is_native(&real) {
        return Ok(Route::Manual(
            "此程序不属于 npm 或已识别的安装器，请按原安装方式更新".into(),
        ));
    }
    Ok(make_fallback(install_plan(tool, env, "x86_64")?.steps))
}
fn shell_path(path: &Path) -> String {
    let raw = path.to_string_lossy();
    if let Some(unc) = raw.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else {
        raw.strip_prefix(r"\\?\").unwrap_or(&raw).to_owned()
    }
}
pub(super) fn command_text(
    env: &InstallationEnvironment,
    program: &Path,
    args: &[String],
) -> String {
    let q = if env.platform == HostPlatform::Windows {
        ps
    } else {
        quote
    };
    format!(
        "{}{} {}",
        if env.platform == HostPlatform::Windows {
            "& "
        } else {
            ""
        },
        q(&program.to_string_lossy()),
        args.iter().map(|a| q(a)).collect::<Vec<_>>().join(" ")
    )
}
