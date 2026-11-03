#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
if [[ "$(uname -s)" != Darwin || "$(uname -m)" != arm64 ]]; then
  printf '%s\n' 'This paired local build currently supports macOS arm64.' >&2
  exit 1
fi
output=${1:-"$root/dist/local-signed-clients-$(date +%Y%m%d-%H%M%S)"}
local_sign() {
  if [[ -n "${TOKEN_MARKET_LOCAL_SIGNING_DIR:-}" ]]; then
    python3 "$root/scripts/macos-local-signing.py" --identity-dir "$TOKEN_MARKET_LOCAL_SIGNING_DIR" "$@"
  else
    python3 "$root/scripts/macos-local-signing.py" "$@"
  fi
}
local_sign init
cargo build --release --locked --manifest-path "$root/rust/tokenmarket-cli/Cargo.toml"
bash "$root/scripts/build-local-desktop.sh" --bundles app
local_sign smoke
local_sign package \
  --app "$root/src-tauri/target/release/bundle/macos/Token Market Local.app" \
  --cli "$root/rust/tokenmarket-cli/target/release/tokenmarket" --output "$output"
