//! Passive installation identity. Never launches a tool or reads its credentials.
use super::{recipes, upgrades, InstallStage};
use crate::{
    inventory::{HostPlatform, InstallationEnvironment},
    ToolId,
};
use std::{io::Read, path::Path};

pub(crate) fn method(
    tool: ToolId,
    path: &Path,
    environment: &InstallationEnvironment,
    application: bool,
) -> &'static str {
    if application {
        return "桌面应用";
    }
    let resolved = path.canonicalize().unwrap_or_else(|_| path.into());
    if resolved
        .components()
        .any(|c| c.as_os_str() == "Cellar" || c.as_os_str() == "Caskroom")
    {
        return "Homebrew";
    }
    if resolved.to_string_lossy().contains(".app/Contents/") {
        return "应用内置";
    }
    if let Some(package) = upgrades::package(tool) {
        let packaged = resolved
            .ancestors()
            .take(10)
            .any(|root| upgrades::package_identity(root, package));
        let wrapper = if environment.platform == HostPlatform::Windows {
            path.parent().is_some_and(|p| {
                upgrades::package_identity(&p.join("node_modules").join(package), package)
            })
        } else {
            false
        };
        let managed = recipes::owned_command_paths(tool, environment).contains(&path.to_path_buf())
            && {
                let root = environment
                    .home
                    .join(".tokenmarket/tools")
                    .join(tool.to_string());
                ["package", "runtime", ""].iter().any(|prefix| {
                    upgrades::package_identity(
                        &root.join(prefix).join("lib/node_modules").join(package),
                        package,
                    ) || upgrades::package_identity(
                        &root.join(prefix).join("node_modules").join(package),
                        package,
                    )
                })
            };
        if packaged || wrapper || managed {
            let source = resolved.to_string_lossy();
            return if source.contains(".pnpm") {
                "pnpm"
            } else if source.contains(".yarn") {
                "Yarn"
            } else if source.contains(".bun") {
                "Bun"
            } else {
                "npm"
            };
        }
    }
    let mut magic = [0u8; 4];
    let native = std::fs::File::open(&resolved)
        .and_then(|mut f| f.read_exact(&mut magic))
        .is_ok()
        && (magic == [0x7f, b'E', b'L', b'F']
            || magic[..2] == [b'M', b'Z']
            || matches!(
                magic,
                [0xcf, 0xfa, 0xed, 0xfe]
                    | [0xce, 0xfa, 0xed, 0xfe]
                    | [0xfe, 0xed, 0xfa, 0xcf]
                    | [0xfe, 0xed, 0xfa, 0xce]
                    | [0xca, 0xfe, 0xba, 0xbe]
                    | [0xbe, 0xba, 0xfe, 0xca]
                    | [0xca, 0xfe, 0xba, 0xbf]
                    | [0xbf, 0xba, 0xfe, 0xca]
            ));
    if native {
        "原生程序"
    } else if path.starts_with(environment.home.join(".tokenmarket/tools")) {
        "客户端管理"
    } else {
        "命令行程序"
    }
}
pub(crate) fn planned_method(tool: ToolId, environment: &InstallationEnvironment) -> &'static str {
    let Ok(plan) = recipes::plan(tool, environment, std::env::consts::ARCH) else {
        return "当前平台暂无安装方案";
    };
    if plan
        .steps
        .iter()
        .any(|s| s.stage == InstallStage::WaitingForUser)
    {
        return "通过官方桌面安装包安装";
    }
    match tool {
        ToolId::ClaudeCode | ToolId::Codex => "通过官方安装器安装",
        ToolId::Hermes => "通过官方安装脚本安装",
        ToolId::OpenClaw => "通过官方脚本和 npm 安装",
        ToolId::GrokBuild | ToolId::DeepSeekHarness => "通过 npm 安装（含运行环境）",
        ToolId::MiniMaxCode => "通过 npm 安装官方发行包",
        ToolId::Pi | ToolId::OhMyPi | ToolId::KimiCode | ToolId::OpenCode => "下载官方独立安装包",
        _ => "通过官方安装包安装",
    }
}
