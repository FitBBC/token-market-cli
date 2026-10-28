# Token Market Agent CLI

[English](README.md) | [简体中文](README.zh-CN.md)

Configure Token Market in popular AI agent tools with one command. The same setup flow is available as a terminal CLI and as a guided desktop app.

## Installation

### Terminal users

Prerequisites:

- Install a supported AI agent tool before running setup. The CLI only configures tools it can detect on your computer.
- To build the CLI from source, install a stable Rust toolchain with Cargo.
- The `tokensmarket` executable must be on your `PATH`.

The repository does not currently publish a standalone CLI package. Install the executable from source:

```bash
cargo install --path rust/tokensmarket-cli
```

Cargo normally installs it under `~/.cargo/bin`; make sure that directory is on your `PATH`. Alternatively, build without installing and invoke the binary by path:

```bash
cargo build --release --manifest-path rust/tokensmarket-cli/Cargo.toml
rust/tokensmarket-cli/target/release/tokensmarket --version
```

### Desktop users

Download the guided desktop app from GitHub Releases:

- **macOS**: download the `.dmg`, open it, and drag **Token Market Setup** into **Applications**.
- **Windows**: download the `x64-setup.exe` installer and run it.

The app is not yet signed and notarized with an Apple Developer ID. If macOS blocks the first launch, run:

```bash
xattr -cr "/Applications/Token Market Setup.app"
```

Then open the app normally.

## Quick start

Run setup without arguments to choose an installed tool interactively:

```bash
tokensmarket setup
```

Or configure a specific tool directly:

```bash
tokensmarket setup claude-code
```

After setup, check the local installation and gateway reachability:

```bash
tokensmarket doctor
tokensmarket test claude-code
```

## Supported tools

| Tool | Tool ID | Configuration location |
| --- | --- | --- |
| Claude Code | `claude-code` | `~/.claude/settings.json` |
| Codex CLI | `codex` | `~/.codex/config.toml` |
| WorkBuddy | `workbuddy` | `~/.workbuddy/models.json` |
| DeepSeek Harness | `deepseek-harness` | `~/.dsh/profiles/<active-profile>/cordis.patch.yml` |
| OpenClaw | `openclaw` | `~/.openclaw/openclaw.json` |
| OpenCode | `opencode` | `~/.config/opencode/opencode.json` |

The CLI detects each tool's actual configuration path. `CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `DSH_HOME`, `DSH_ACTIVE_PROFILE`, `WORKBUDDY_CONFIG_PATH`, `OPENCLAW_CONFIG_PATH`, and `OPENCODE_CONFIG` can override the defaults above.

## Commands

### `tokensmarket setup [tool]`

Authorize Token Market, create a CLI API key, write the selected tool's configuration, and run verification requests.

Options:

- `--model <preset>`: choose a model preset. Defaults to `recommended`.

Examples:

```bash
tokensmarket setup codex
tokensmarket setup codex --model fast
```

### `tokensmarket doctor [tool]`

Show the configured gateway URL, each tool's setup state, configuration path, and model API reachability. Passing a tool ID limits the report to that tool.

```bash
tokensmarket doctor openclaw
```

### `tokensmarket test <tool>`

Run configured request tests. By default, this runs all tests.

Options:

- `--models`: model list request
- `--chat`: non-streaming chat request
- `--stream`: streaming chat request
- `--tool-call`: tool-call request

```bash
tokensmarket test codex --models --chat
```

### `tokensmarket keys`

List API keys managed by this CLI. The command shows key names, key IDs, target tools, and creation times; it never prints the API key secret.

```bash
tokensmarket keys
```

## Models

Setup uses a preset rather than asking users to choose a raw model name.

| Preset | Model |
| --- | --- |
| `recommended` | `glm-5.3` |
| `powerful` | `glm-5.3` |
| `cheap` | `glm-5.3` |
| `fast` | `deepseek-v4.1-flash` |

The default preset is `recommended`.

## Security, backups, and revocation

- Authorization uses PKCE and a one-time local callback at `http://127.0.0.1`.
- The browser authorization page lets the logged-in user choose the workspace that will own the CLI API key.
- On Unix-like systems, adapter configuration and credential files are written with owner-only file permissions. On Windows, privacy relies on the user-profile and parent-directory ACLs.
- Before modifying an existing configuration, the CLI copies it to `~/.tokensmarket/backups/`.
- The local key registry is stored at `~/.tokensmarket/keys.json`; it records metadata only and does not replace the tool configuration.
- Re-running setup replaces the local managed record for that tool. Revoke the actual key from the Token Market dashboard if you no longer want it to remain active.

## Architecture

- `rust/tokensmarket-core`: shared setup domain. Contains tool adapters, PKCE authorization, configuration discovery and backups, the managed-key registry, model resolution, and gateway verification.
- `rust/tokensmarket-cli`: the `tokensmarket` executable. It is a thin command-line shell over the shared core.
- `src-tauri`: the native desktop app. It is also a thin shell and invokes the same Rust core directly.
- `src/tauri-ui`: the desktop app's HTML interface.

## Development

There is no root Cargo workspace, so Cargo commands must address the desired crate manifest directly.

```bash
cargo fmt --manifest-path rust/tokensmarket-core/Cargo.toml --all --check
cargo test --manifest-path rust/tokensmarket-core/Cargo.toml --all-targets
cargo test --manifest-path rust/tokensmarket-cli/Cargo.toml --all-targets
cargo check --manifest-path src-tauri/Cargo.toml
```

Build release binaries:

```bash
cargo build --release --manifest-path rust/tokensmarket-cli/Cargo.toml
cargo tauri build
```

`cargo tauri build` is only needed when producing the desktop app bundle locally; it is not part of normal Rust development checks.

## Platform contract

The authorization API defaults to `https://console-api.tokensapi.cn` (`TOKEN_MARKET_CONSOLE_API_BASE_URL`); gateway requests default to `https://api.tokensapi.cn` (`TOKEN_MARKET_GATEWAY_API_BASE_URL`).

The CLI expects these platform endpoints:

- `POST /api/cli/sessions`
- `POST /api/cli/exchange`
- OpenAI-compatible model/chat endpoints under `/v1`