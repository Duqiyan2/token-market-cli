//! One official Node release/asset owner; tools keep their own compatibility policy.
use crate::inventory::HostPlatform;
use anyhow::{bail, Result};
pub(super) const VERSION: &str = "24.22.1";
pub(super) struct Asset {
    pub target: &'static str,
    pub sha256: &'static str,
    pub filename: String,
    pub url: String,
}
pub(super) fn asset(platform: HostPlatform, architecture: &str) -> Result<Asset> {
    let (target, sha256) = match (platform, architecture) {
        (HostPlatform::MacOs, "aarch64") => (
            "darwin-arm64",
            "89c682593d76cd44233fa53b1563264e7de9cbb189cad169135c5b4fadfbc0a7",
        ),
        (HostPlatform::MacOs, "x86_64") => (
            "darwin-x64",
            "d132557e7ee2b85af08b0c8c34338e65c6430f2238d01f727728ef42dc83b035",
        ),
        (HostPlatform::Unix, "aarch64") => (
            "linux-arm64",
            "33c37be234ba8bee041961047b19160f4d7d0f164e8e246f78bdd10feff148b3",
        ),
        (HostPlatform::Unix, "x86_64") => (
            "linux-x64",
            "eb6728e77c576a35162c122c45ccadf224d89ff00abc8dae46cc2897c6002d7c",
        ),
        (HostPlatform::Windows, "aarch64") => (
            "win-arm64",
            "a0c807319fede24c4a0d8f84564c82a619bd313b16b10c007c383c2609e99171",
        ),
        (HostPlatform::Windows, "x86_64") => (
            "win-x64",
            "d2046d28c2c3dcd211ac5145c0bf30e2047ef9dfb306693c8c6e2eccd8b95fa5",
        ),
        _ => bail!("官方 Node.js 私有运行时不支持当前系统/架构 {architecture}"),
    };
    let filename = format!(
        "node-v{VERSION}-{target}.{}",
        if platform == HostPlatform::Windows {
            "zip"
        } else {
            "tar.gz"
        }
    );
    Ok(Asset {
        target,
        sha256,
        url: format!("https://nodejs.org/dist/v{VERSION}/{filename}"),
        filename,
    })
}
/// Callers supply catalogue-owned JS and prefix/archive/origin/checksum variables.
/// No package, profile or tool configuration belongs to runtime preparation.
pub(super) fn posix_prepare(compatibility: &str) -> String {
    r#"usable_node() {
 [ -n "$1" ] && [ -x "$1" ] && [ -x "$(dirname "$1")/npm" ] && "$1" -e '__COMPATIBILITY__'
}
node=$(command -v node || true)
if ! usable_node "$node"; then
 node="$prefix/runtime/bin/node"
 if ! usable_node "$node"; then
  echo 'Preparing private Node.js __VERSION__ runtime'
  mkdir -p "$(dirname "$archive")" "$prefix/runtime"
  curl --fail --location --silent --show-error --proto '=https' --proto-redir '=https' --connect-timeout 30 --max-time 300 --output "$archive" "$origin"
  if command -v sha256sum >/dev/null 2>&1; then actual=$(sha256sum "$archive"); else actual=$(shasum -a 256 "$archive"); fi
  actual=${actual%% *}
  [ "$actual" = "$checksum" ] || { echo 'Node.js SHA256 verification failed' >&2; exit 1; }
  tar -xzf "$archive" --strip-components=1 -C "$prefix/runtime"
  usable_node "$node" || { echo 'Private Node.js runtime was not installed' >&2; exit 1; }
 fi
fi
runtime_bin=$(dirname "$node")
PATH="$runtime_bin:$PATH"; export PATH
"#.replace("__COMPATIBILITY__",compatibility).replace("__VERSION__",VERSION)
}
