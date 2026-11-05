//! Official graphical installers hand off to the user's native installation UI.
use super::{InstallPlan, InstallStage, InstallStep};
use crate::{
    inventory::{HostPlatform, InstallationEnvironment},
    ToolId,
};
use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use std::{
    io::{Read, Seek, SeekFrom},
    path::Path,
};

pub(super) fn is_gui(tool: ToolId) -> bool {
    matches!(
        tool,
        ToolId::WorkBuddy
            | ToolId::DeepSeekHarness
            | ToolId::DuMate
            | ToolId::DuMateEnterprise
            | ToolId::QwenWork
    )
}
pub(super) fn instructions(tool: ToolId, platform: HostPlatform) -> &'static str {
    if matches!(tool, ToolId::DuMate | ToolId::DuMateEnterprise) && platform == HostPlatform::MacOs
    {
        "请将已打开文件夹中的 DuMate.app 拖入 Applications。安装完成后请先登录要接入的个人版或企业版；接入前完全退出 DuMate"
    } else if tool == ToolId::DeepSeekHarness && platform == HostPlatform::MacOs {
        "请将已打开文件夹中的 DeepSeek Harness.app 移入 Applications。dsh 命令注册可在工具内另行操作；取消请关闭原生安装窗口"
    } else if platform == HostPlatform::MacOs {
        "请在原生安装器中将应用拖入 Applications。取消安装请在原生安装器中操作；关闭此窗口会继续等待实际安装"
    } else {
        "请完成原生安装器中的安装向导。取消安装请在原生安装器中操作；关闭此窗口会继续等待实际安装"
    }
}
pub(super) fn matches_version(tool: ToolId, actual: &str, target: &str) -> bool {
    actual.split_whitespace().any(|version| {
        version == target
            || (tool == ToolId::WorkBuddy && version.starts_with(&format!("{target}.")))
    })
}
pub(super) fn plan(
    tool: ToolId,
    environment: &InstallationEnvironment,
    architecture: &str,
) -> Result<InstallPlan> {
    if tool == ToolId::QwenWork {
        bail!("请通过千问办公官网安装，登录后完全退出再接入；更新版本后需要重新接入")
    }
    if matches!(tool, ToolId::DuMate | ToolId::DuMateEnterprise) {
        let mut plan = super::dumate::plan(environment, architecture)?;
        plan.tool = tool;
        return Ok(plan);
    }
    if tool == ToolId::DeepSeekHarness {
        return harness_plan(environment, architecture);
    }
    let platform = match (environment.platform, architecture) {
        (HostPlatform::MacOs, "aarch64") => "darwin-arm64",
        (HostPlatform::MacOs, "x86_64") => "darwin-x64",
        (HostPlatform::Windows, "x86_64") => "win32-x64-user",
        (HostPlatform::Unix, _) => bail!("WorkBuddy 在此 Linux 环境没有已验证的通用安装包；统信 UOS / 银河麒麟请按官方应用商店说明安装"),
        _ => bail!("WorkBuddy 官方安装方案不支持当前系统或架构 {architecture}"),
    };
    let version = "5.9.3";
    let extension = if environment.platform == HostPlatform::Windows {
        "exe"
    } else {
        "dmg"
    };
    let filename = format!("WorkBuddy-{platform}-5.9.3.42398134-d4692421.{extension}");
    let package = environment
        .home
        .join(".tokenmarket/installers/workbuddy")
        .join(&filename);
    let handoff = if environment.platform == HostPlatform::MacOs {
        InstallStep {
            stage: InstallStage::WaitingForUser,
            program: "/usr/bin/open".into(),
            args: vec![package.to_string_lossy().into_owned()],
            environment: vec![],
            download: None,
        }
    } else {
        InstallStep {
            stage: InstallStage::WaitingForUser,
            program: "powershell.exe".into(),
            args: vec![
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-Command".into(),
                format!(
                    "$ErrorActionPreference = 'Stop'; Start-Process -FilePath '{}'",
                    package.to_string_lossy().replace('\'', "''")
                ),
            ],
            environment: vec![],
            download: None,
        }
    };
    Ok(InstallPlan {
        tool,
        version: version.into(),
        command: "workbuddy".into(),
        steps: vec![InstallStep {
            stage: InstallStage::Downloading,
            program: String::new(),
            args: vec![],
            environment: vec![],
            download: Some((format!("https://acc-1258344699.cos.ap-guangzhou.myqcloud.com/workbuddy/saas/{platform}/{filename}"), package)),
        }, handoff],
    })
}
pub(super) fn harness_application_version(application: &Path) -> Result<String> {
    let resources = if application.is_dir() {
        application.join("Contents/Resources")
    } else {
        application
            .parent()
            .context("应用路径无效")?
            .join("resources")
    };
    let package = if resources.join("app.asar").exists() {
        read_asar_package(&resources.join("app.asar"))?
    } else {
        read_package_json(&resources.join("app/package.json"))?
    };
    package["version"]
        .as_str()
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .context("无法读取 Harness 完整版本元数据")
}

fn harness_plan(environment: &InstallationEnvironment, architecture: &str) -> Result<InstallPlan> {
    if environment.platform == HostPlatform::Unix
        || (environment.platform == HostPlatform::MacOs && architecture == "x86_64")
    {
        return super::harness_cli::plan(environment, architecture);
    }
    let (platform, extension, checksum) = match (environment.platform, architecture) {
        (HostPlatform::MacOs, "aarch64") => ("mac-arm64", "zip", "SPXR+TMC5+7v9R544aKgA2CkDrLr2XyNL/35/NuxGBbP5nrSTEGMurNRAi+R1RSlcWZm3KNdySAt0RS8Qizv9g=="),
        (HostPlatform::Windows, "x86_64") => ("win-x64", "exe", "gu2dH+aTr678c0P8DGAH/h4B1GaWfQIjpsPrLjl/ukLrQIh6DLVq/MhX4FZvJ8o1GCMocOhix6XbLa6KmtjXXg=="),
        _ => bail!("DeepSeek Harness 当前已验证的官方桌面包仅支持 macOS Apple Silicon 和 Windows x64；其他平台请查看官方 CLI / 安装说明"),
    };
    // Harness alone has an explicitly accepted fixed RC exception; never use a rolling alias.
    let version = "0.2.1-rc.5";
    let filename = format!("deepseek-harness-{version}-{platform}.{extension}");
    let directory = environment
        .home
        .join(".tokenmarket/installers/deepseek-harness")
        .join(version);
    let package = directory.join(&filename);
    let digest = STANDARD.decode(checksum)?;
    if digest.len() != 64 {
        bail!("官方 Harness SHA512 长度无效")
    }
    let hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let mut steps = vec![InstallStep {
        stage: InstallStage::Downloading,
        program: String::new(),
        args: vec![],
        environment: vec![],
        download: Some((
            format!("https://download.deepseek.com/dsh-desk/bin/{platform}/{filename}"),
            package.clone(),
        )),
    }];
    if environment.platform == HostPlatform::MacOs {
        let unpacked = directory.join("unpacked");
        steps.push(InstallStep {
            stage: InstallStage::Installing,
            program: "/bin/sh".into(),
            args: vec!["-c".into(), "set -eu; actual=$(/usr/bin/shasum -a 512 \"$1\"); actual=${actual%% *}; if [ \"$actual\" != \"$2\" ]; then echo 'Harness SHA512 verification failed' >&2; exit 1; fi; /bin/mkdir -p \"$3\"; /usr/bin/ditto -x -k \"$1\" \"$3\"".into(), "tokenmarket-harness-verify".into(), package.to_string_lossy().into_owned(), hex, unpacked.to_string_lossy().into_owned()],
            environment: vec![], download: None,
        });
        steps.push(InstallStep {
            stage: InstallStage::WaitingForUser,
            program: "/usr/bin/open".into(),
            args: vec![unpacked.to_string_lossy().into_owned()],
            environment: vec![],
            download: None,
        });
    } else {
        let quoted_package = package.to_string_lossy().replace('\'', "''");
        steps.push(InstallStep {
            stage: InstallStage::Installing,
            program: "powershell.exe".into(),
            args: vec!["-NoProfile".into(), "-NonInteractive".into(), "-Command".into(), format!("$ErrorActionPreference = 'Stop'; if ((Get-FileHash -LiteralPath '{quoted_package}' -Algorithm SHA512).Hash -ne '{hex}') {{ throw 'Harness SHA512 verification failed' }}")],
            environment: vec![], download: None,
        });
        steps.push(InstallStep {
            stage: InstallStage::WaitingForUser,
            program: "powershell.exe".into(),
            args: vec![
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-Command".into(),
                format!(
                    "$ErrorActionPreference = 'Stop'; Start-Process -FilePath '{quoted_package}'"
                ),
            ],
            environment: vec![],
            download: None,
        });
    }
    Ok(InstallPlan {
        tool: ToolId::DeepSeekHarness,
        version: version.into(),
        command: "dsh".into(),
        steps,
    })
}

fn read_asar_package(path: &Path) -> Result<serde_json::Value> {
    // ASAR stores two Chromium pickles, followed by file data. Read only the
    // bounded root package.json metadata; do not extract or execute app code.
    let mut archive = std::fs::File::open(path)?;
    let length = archive.metadata()?.len();
    let mut prefix = [0; 8];
    archive.read_exact(&mut prefix)?;
    let header_size = u32::from_le_bytes(prefix[4..8].try_into()?) as u64;
    if u32::from_le_bytes(prefix[0..4].try_into()?) != 4
        || !(8..=64 * 1024 * 1024).contains(&header_size)
        || header_size + 8 > length
    {
        bail!("应用 ASAR 版本索引无效")
    }
    let mut header = vec![0; header_size as usize];
    archive.read_exact(&mut header)?;
    let json_size = u32::from_le_bytes(header[4..8].try_into()?) as usize;
    let json = header
        .get(8..8usize.checked_add(json_size).context("ASAR 索引长度无效")?)
        .context("ASAR 索引长度无效")?;
    let index: serde_json::Value = serde_json::from_slice(json)?;
    let entry = &index["files"]["package.json"];
    if entry["unpacked"] == true {
        return read_package_json(&path.with_extension("asar.unpacked").join("package.json"));
    }
    let size = entry["size"]
        .as_u64()
        .filter(|size| *size <= 1024 * 1024)
        .context("应用版本文件大小无效")?;
    let offset: u64 = entry["offset"]
        .as_str()
        .context("应用版本文件位置无效")?
        .parse()?;
    let start = 8u64
        .checked_add(header_size)
        .and_then(|value| value.checked_add(offset))
        .context("应用版本文件位置溢出")?;
    if start.checked_add(size).is_none_or(|end| end > length) {
        bail!("应用版本文件超出 ASAR 边界")
    }
    archive.seek(SeekFrom::Start(start))?;
    let mut package = vec![0; size as usize];
    archive.read_exact(&mut package)?;
    Ok(serde_json::from_slice(&package)?)
}

fn read_package_json(path: &Path) -> Result<serde_json::Value> {
    let file = std::fs::File::open(path)?;
    let mut bytes = vec![];
    file.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 1024 * 1024 {
        bail!("应用版本文件过大")
    }
    Ok(serde_json::from_slice(&bytes)?)
}
