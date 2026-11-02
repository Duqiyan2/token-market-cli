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
    let (target, checksum) = match (environment.platform, architecture) {
        (HostPlatform::Unix, "x86_64") => (
            "linux-x64",
            "eb6728e77c576a35162c122c45ccadf224d89ff00abc8dae46cc2897c6002d7c",
        ),
        (HostPlatform::Unix, "aarch64") => (
            "linux-arm64",
            "33c37be234ba8bee041961047b19160f4d7d0f164e8e246f78bdd10feff148b3",
        ),
        (HostPlatform::MacOs, "x86_64") => (
            "darwin-x64",
            "d132557e7ee2b85af08b0c8c34338e65c6430f2238d01f727728ef42dc83b035",
        ),
        _ => bail!("该系统或架构没有已验证的 Harness CLI 安装流程，请查看官方安装说明"),
    };
    let prefix = environment.home.join(".tokenmarket/tools/deepseek-harness");
    let archive = environment
        .home
        .join(".tokenmarket/installers/deepseek-harness")
        .join(format!("node-v24.22.1-{target}.tar.gz"));
    let script = r#"set -eu
prefix=$1
archive=$2
origin=$3
checksum=$4
usable_node() {
  [ -n "$1" ] && [ -x "$1" ] && [ -x "$(dirname "$1")/npm" ] && "$1" -e 'const [major,minor,patch]=process.versions.node.split(".").map(Number); process.exit((major===24 && (minor>22 || (minor===22 && patch>=1))) || (major>=26 && major%2===0) ? 0 : 1)'
}
node=$(command -v node || true)
if ! usable_node "$node"; then
  node="$prefix/runtime/bin/node"
  if ! usable_node "$node"; then
    echo 'Preparing private Node.js runtime'
    mkdir -p "$(dirname "$archive")" "$prefix/runtime"
    curl --fail --location --silent --show-error --proto '=https' --proto-redir '=https' --connect-timeout 30 --max-time 300 --output "$archive" "$origin"
    if command -v sha256sum >/dev/null 2>&1; then actual=$(sha256sum "$archive"); else actual=$(shasum -a 256 "$archive"); fi
    actual=${actual%% *}
    if [ "$actual" != "$checksum" ]; then echo 'Node.js SHA256 verification failed' >&2; exit 1; fi
    tar -xzf "$archive" --strip-components=1 -C "$prefix/runtime"
  fi
fi
runtime_bin=$(dirname "$node")
PATH="$runtime_bin:$PATH"
export PATH
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
"#;
    Ok(InstallPlan {
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
                format!("https://nodejs.org/dist/v24.22.1/node-v24.22.1-{target}.tar.gz"),
                checksum.into(),
            ],
            environment: vec![],
            download: None,
        }],
    })
}
