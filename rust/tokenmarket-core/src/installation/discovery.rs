//! Passive installation identity. Never launches a tool or reads its credentials.
use super::{recipes, upgrades};
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
    let hint = path
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    if ["/.nvm/", "/.volta/", "/volta/", "fnm_multishells", "/mise/"]
        .iter()
        .any(|s| hint.contains(s))
    {
        return "npm";
    }
    if hint.contains("/.bun/") {
        return "Bun";
    }
    if hint.contains("/pnpm/") {
        return "pnpm";
    }
    if hint.contains("/scoop/") {
        return "Scoop";
    }
    if hint.contains("/site-packages/") || hint.contains("/library/python") {
        return "pip";
    }
    let native = is_native(&resolved);
    if native {
        "原生程序"
    } else if path.starts_with(environment.home.join(".tokenmarket/tools")) {
        "客户端管理"
    } else {
        "命令行程序"
    }
}

pub(super) fn is_native(path: &Path) -> bool {
    let mut magic = [0u8; 4];
    let native = std::fs::File::open(path)
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
    native
}
