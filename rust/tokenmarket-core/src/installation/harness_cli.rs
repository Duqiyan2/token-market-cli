//! Persistent official Harness CLI distribution for environments without a desktop package.
use super::{InstallPlan, InstallStage, InstallStep};
use crate::{
    inventory::{HostPlatform, InstallationEnvironment},
    ToolId,
};
use anyhow::{bail, Result};

pub(super) fn plan(
    environment: &InstallationEnvironment,
    architecture: &str,
) -> Result<InstallPlan> {
    if !matches!(
        (environment.platform, architecture),
        (HostPlatform::Unix, "x86_64" | "aarch64") | (HostPlatform::MacOs, "x86_64")
    ) {
        bail!("该系统或架构没有已验证的 Harness CLI 安装流程，请查看官方安装说明")
    }
    let asset = super::node::asset(environment.platform, architecture)?;
    let prefix = environment.home.join(".tokenmarket/tools/deepseek-harness");
    let archive = environment
        .home
        .join(".tokenmarket/installers/deepseek-harness")
        .join(&asset.filename);
    let script = r#"set -eu
prefix=$1
archive=$2
origin=$3
checksum=$4
__NODE_PREPARE__
mkdir -p "$prefix/package" "$prefix/bin"
"$runtime_bin/npm" install --global --prefix "$prefix/package" --no-audit --no-fund '@deepseek-ai/dsh@0.2.1-rc.5'
printf '%s\n' "$node" > "$prefix/node-path"
cat > "$prefix/bin/dsh" <<'TOKENMARKET_DSH'
#!/bin/sh
set -eu
prefix=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
node=$(cat "$prefix/node-path")
PATH="$(dirname "$node"):$PATH"
export PATH
if [ "$#" -eq 0 ]; then set -- "${DSH_ACTIVE_PROFILE:-web}"; fi
exec "$node" "$prefix/package/lib/node_modules/@deepseek-ai/dsh/lib/bin.js" "$@"
TOKENMARKET_DSH
chmod 755 "$prefix/bin/dsh"
echo 'Harness CLI installed; connect separately before starting a profile'
"#.replace("__NODE_PREPARE__",&super::node::posix_prepare("const [major,minor,patch]=process.versions.node.split(\".\").map(Number); process.exit((major===24 && (minor>22 || (minor===22 && patch>=1))) || (major>=26 && major%2===0) ? 0 : 1)"));
    Ok(InstallPlan {
        method: super::InstallMethod::Npm,
        tool: ToolId::DeepSeekHarness,
        version: "0.2.1-rc.5".into(),
        command: "dsh".into(),
        steps: vec![InstallStep {
            stage: InstallStage::Dependencies,
            program: "/bin/sh".into(),
            args: vec![
                "-c".into(),
                script.into(),
                "tokenmarket-harness-cli-install".into(),
                prefix.to_string_lossy().into_owned(),
                archive.to_string_lossy().into_owned(),
                asset.url,
                asset.sha256.into(),
            ],
            environment: vec![],
            download: None,
        }],
    })
}
