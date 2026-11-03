#!/usr/bin/env bash
# Low-level Tauri build. Use build-local-clients.sh to deliver a CLI/App pair
# with the stable shared keychain signing identity and verified DMG.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
config_file=$(mktemp "${TMPDIR:-/tmp}/token-market-local-config.XXXXXX.json")
trap 'rm -f "$config_file"' EXIT
cat > "$config_file" <<'JSON'
{"productName":"Token Market Local","identifier":"cn.tokenmarket.setup.test"}
JSON
cd "$root"
if [[ -n "${TAURI_CLI_JS:-}" ]]; then
  node "$TAURI_CLI_JS" build --config "$config_file" "$@"
else
  cargo tauri build --config "$config_file" "$@"
fi
