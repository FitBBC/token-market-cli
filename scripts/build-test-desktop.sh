#!/usr/bin/env bash
# Low-level Tauri build. Use build-test-clients.sh to deliver a CLI/App pair
# with the stable shared keychain signing identity and verified DMG.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
config_file=$(mktemp "${TMPDIR:-/tmp}/token-market-test-config.XXXXXX.json")
trap 'rm -f "$config_file"' EXIT
cat > "$config_file" <<'JSON'
{"productName":"Token Market Test","identifier":"cn.tokenmarket.setup.test"}
JSON
export TOKEN_MARKET_BUILD_CONSOLE_API_BASE_URL='https://console-api-test.tokensapi.cn'
export TOKEN_MARKET_BUILD_GATEWAY_API_BASE_URL='https://api-test.tokensapi.cn'
export TOKEN_MARKET_BUILD_DESKTOP_DEFAULT_MODEL='glm-5.2'
printf 'Test build: WebAPI=%s Gateway=%s\n' "$TOKEN_MARKET_BUILD_CONSOLE_API_BASE_URL" "$TOKEN_MARKET_BUILD_GATEWAY_API_BASE_URL"
printf '%s\n' 'Runtime TOKEN_MARKET_*_BASE_URL variables override these defaults. The test app shares the tokenmarket:// scheme and local Agent configuration.'
cd "$root"
if [[ -n "${TAURI_CLI_JS:-}" ]]; then
  node "$TAURI_CLI_JS" build --config "$config_file" "$@"
else
  cargo tauri build --config "$config_file" "$@"
fi
