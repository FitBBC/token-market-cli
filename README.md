# Token Market Agent CLI

Configure Token Market in popular agent tools with one command.

## Commands

For terminal users:

```bash
tokensmarket setup
tokensmarket setup claude-code
tokensmarket doctor
tokensmarket test claude-code
tokensmarket keys
```

For non-technical users, launch the guided macOS app:

```bash
open "Token Market Setup.app"
```

## Supported tools

- Claude Code
- Codex CLI
- WorkBuddy
- DeepSeek Harness
- OpenClaw
- OpenCode

## Platform contract

The authorization API defaults to `https://console-api.tokensapi.cn` (`TOKEN_MARKET_CONSOLE_API_BASE_URL`); gateway requests default to `https://api.tokensapi.cn` (`TOKEN_MARKET_GATEWAY_API_BASE_URL`).

The CLI expects these platform endpoints:

- `POST /api/cli/sessions`
- `POST /api/cli/exchange`
- OpenAI-compatible model/chat endpoints under `/v1`

Authorization uses PKCE and a one-time local callback at `http://127.0.0.1`.

## Rust architecture

- `rust/tokensmarket-core`: shared setup domain, adapters, authorization, verification, and key registry.
- `rust/tokensmarket-cli`: the `tokensmarket` executable.
- `src-tauri`: the native desktop app, which calls the same Rust core directly.

## Development

```bash
cargo fmt --all
cargo test --manifest-path rust/tokensmarket-core/Cargo.toml
cargo test --manifest-path rust/tokensmarket-cli/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
cargo build --release --manifest-path rust/tokensmarket-cli/Cargo.toml
cargo tauri build
```
