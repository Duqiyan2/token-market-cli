#!/usr/bin/env bash
# Production identifiers, persistent signing identity, matching CLI/App and DMG.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
if [[ "$(uname -s)" != Darwin ]]; then
  printf '%s\n' 'This build requires macOS.' >&2
  exit 1
fi
output=${1:-"$root/dist/signed-clients-$(date +%Y%m%d-%H%M%S)"}
sign_clients() {
  local identity_dir=${TOKEN_MARKET_LOCAL_SIGNING_DIR:-"$HOME/Library/Application Support/Token Market/Local Signing"}
  python3 "$root/scripts/macos-local-signing.py" --identity-dir "$identity_dir" --profile production "$@"
}
# A missing identity must stop an upgrade build rather than create a new one.
# Initialize once explicitly; keep and back up that identity for future builds.
sign_clients smoke
sign_clients native-smoke
# Release orchestration provides an external Cargo target directory. Keep the
# historical standalone layout when this wrapper is called without that input.
cli_target=${CARGO_TARGET_DIR:-"$root/rust/tokenmarket-cli/target"}
app_target=${CARGO_TARGET_DIR:-"$root/src-tauri/target"}
cargo build --release --locked --manifest-path "$root/rust/tokenmarket-cli/Cargo.toml"
cd "$root"
if [[ -n "${TAURI_CLI_JS:-}" ]]; then
  node "$TAURI_CLI_JS" build --bundles app
else
  cargo tauri build --bundles app
fi
package_options=()
if [[ "${TOKEN_MARKET_INDEPENDENT_CLI_VERSION:-}" == 1 ]]; then
  package_options+=(--independent-cli-version)
fi
sign_clients package "${package_options[@]}" \
  --app "$app_target/release/bundle/macos/Token Market.app" \
  --cli "$cli_target/release/tokenmarket" --output "$output"
