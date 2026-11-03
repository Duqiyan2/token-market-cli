//! Stable Hermes source bootstrap, without provider setup or config maintenance.
use super::{InstallPlan, InstallStage, InstallStep};
use crate::{
    inventory::{HostPlatform, InstallationEnvironment},
    ToolId,
};
use anyhow::{bail, Result};

pub const VERSION: &str = "0.22.5";
pub const COMMIT: &str = "bf0203a794a07aac3ff764972ab9087baa4e4f4e";

pub fn plan(environment: &InstallationEnvironment, architecture: &str) -> Result<InstallPlan> {
    if !matches!(architecture, "aarch64" | "x86_64") {
        bail!("当前架构没有已验证的 Hermes CLI 官方安装方案")
    }
    let root = environment.home.join(".tokenmarket/tools/hermes/checkout");
    let data = crate::config::hermes_home(&environment.home, environment.platform);
    let directory = environment.home.join(".tokenmarket/installers/hermes");
    let windows = environment.platform == HostPlatform::Windows;
    let installer = directory.join(if windows { "install.ps1" } else { "install.sh" });
    let download = InstallStep {
        stage: InstallStage::Downloading,
        program: String::new(),
        args: vec![],
        environment: vec![],
        download: Some((
            format!(
                "https://raw.githubusercontent.com/NousResearch/hermes-agent/{COMMIT}/scripts/{}",
                if windows { "install.ps1" } else { "install.sh" }
            ),
            installer.clone(),
        )),
    };
    let execution = if windows {
        let script = format!(
            r#"$ErrorActionPreference = 'Stop'
$installer = '{}'
if ((Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant() -ne '8d491a8b9b19934bb8d027b11962c394551798e969d244b68d4ab95295b970e2') {{ throw 'Hermes 安装脚本校验失败' }}
. $installer -Commit '{}' -InstallDir '{}' -HermesHome '{}' -NonInteractive -SkipBrowser -SkipComputerUse
Initialize-ResolvedPaths
$env:UV_NO_CONFIG = '1'
if ('__TOKENMARKET_PHASE__' -eq 'dependencies') {{ foreach ($part in @('prerequisites','repository','venv','python-deps')) {{ Invoke-StageByName $part }}; exit 0 }}
$bootPy = Get-BootstrapPython
$bootstrap = @'
{}
'@
$products = @'
{}
'@
Push-Location $InstallDir
try {{ & $bootPy -I -B -X utf8 -c $bootstrap $InstallDir $products; if ($LASTEXITCODE -ne 0) {{ throw 'Hermes 产品构建失败' }} }} finally {{ Pop-Location }}
Stage-Complete
"#,
            ps(&installer),
            COMMIT,
            ps(&root),
            ps(&data),
            BOOTSTRAP_PRODUCTS,
            BUILD_PRODUCTS
        );
        InstallStep {
            stage: InstallStage::Dependencies,
            program: "powershell.exe".into(),
            args: vec![
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-ExecutionPolicy".into(),
                "Bypass".into(),
                "-Command".into(),
                script,
            ],
            environment: installer_environment(),
            download: None,
        }
    } else {
        InstallStep {
            stage: InstallStage::Dependencies,
            program: "bash".into(),
            args: vec![
                "-c".into(),
                POSIX_INSTALL.into(),
                "tokenmarket-hermes-install".into(),
                installer.to_string_lossy().into(),
                root.to_string_lossy().into(),
                data.to_string_lossy().into(),
                COMMIT.into(),
                BOOTSTRAP_PRODUCTS.into(),
                BUILD_PRODUCTS.into(),
            ],
            environment: installer_environment(),
            download: None,
        }
    };
    let mut dependencies = execution.clone();
    let mut products = execution;
    products.stage = InstallStage::Installing;
    for argument in &mut dependencies.args {
        *argument = argument.replace("__TOKENMARKET_PHASE__", "dependencies");
    }
    for argument in &mut products.args {
        *argument = argument.replace("__TOKENMARKET_PHASE__", "products");
    }
    Ok(InstallPlan {
        tool: ToolId::Hermes,
        version: VERSION.into(),
        command: "hermes".into(),
        steps: vec![download, dependencies, products],
    })
}

fn installer_environment() -> Vec<(String, String)> {
    vec![
        (
            "HERMES_REPO_URL".into(),
            "https://github.com/NousResearch/hermes-agent.git".into(),
        ),
        ("HERMES_INSTALL_CHANNEL".into(), String::new()),
    ]
}
fn ps(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\'', "''")
}

const BOOTSTRAP_PRODUCTS: &str = r#"import sys, subprocess
from pathlib import Path
root = Path(sys.argv[1]).resolve()
sys.path.insert(0, str(root))
from pm.environments import project_python
subprocess.run([str(project_python(root)), '-I', '-B', '-X', 'utf8', '-c', sys.argv[2], str(root)], cwd=root, check=True)
"#;
const BUILD_PRODUCTS: &str = r#"import sys
from pathlib import Path
root = Path(sys.argv[1]).resolve()
sys.path.insert(0, str(root))
from pm.environments import activate_dependencies
activate_dependencies(root)
from hermes_cli.update_lock import UpdateLock
lock = UpdateLock(install_root=root)
if not lock.acquire():
    raise RuntimeError('Hermes source update is running; retry after it finishes')
try:
    from hermes_cli._subprocess_compat import expose_pm_git
    expose_pm_git(root)
    from hermes_cli.source_build import build_update_products
    build_update_products(root, desktop=False)
    from hermes_cli.source_stamp import write_source_stamp
    write_source_stamp(root)
    from hermes_cli.venv_sync import publish_launchers
    publish_launchers(root)
finally:
    lock.release()
"#;

const POSIX_INSTALL: &str = r#"set -eu
installer=$1
checkout=$2
data=$3
pin=$4
bootstrap=$5
products=$6
if command -v shasum >/dev/null 2>&1; then
  digest=$(shasum -a 256 "$installer" | cut -d ' ' -f 1)
elif command -v sha256sum >/dev/null 2>&1; then
  digest=$(sha256sum "$installer" | cut -d ' ' -f 1)
else
  echo '缺少系统 SHA256 校验工具，请准备后重试' >&2
  exit 1
fi
[ "$digest" = '99cb3691d55019c6973a7acb775a915648666cc24e483ca827a1cdbb3531a674' ] || { echo 'Hermes 安装脚本校验失败' >&2; exit 1; }
source "$installer" --commit "$pin" --dir "$checkout" --hermes-home "$data" --non-interactive --skip-browser --skip-computer-use
check_platform
if [ '__TOKENMARKET_PHASE__' = dependencies ]; then
  for part in prerequisites repository venv python-deps; do run_stage "$part"; done
  exit 0
fi
boot_py=''
bootstrap_python
cd "$INSTALL_DIR"
"$boot_py" -I -B -X utf8 -c "$bootstrap" "$INSTALL_DIR" "$products"
stage_complete
"#;
