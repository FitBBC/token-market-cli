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

For non-technical users, download the guided desktop app from GitHub Releases:

- macOS: download the `.dmg`, open it, and drag **Token Market Setup** into **Applications**.
- Windows: download the `x64-setup.exe` installer and run it.

The app is not yet signed and notarized with an Apple Developer ID. If macOS blocks the first launch, run:

```bash
xattr -cr "/Applications/Token Market Setup.app"
```

Then open the app normally.

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
