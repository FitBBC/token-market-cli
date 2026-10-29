# Token Market Agent CLI

[English](README.md) | [简体中文](README.zh-CN.md)

Configure Token Market in popular AI agent tools from the terminal, or sign in once to the desktop app to manage six local tools. The terminal authorizes each setup; desktop login opens the platform Default Workspace, then Connect issues a separate device/tool key. See [desktop login](docs/desktop-login.md) and [desktop management](docs/desktop-management.md).

## Installation

### Terminal users

With Node.js 20 or newer, use the npm entry point without installing Rust or the desktop app:

```bash
npx --yes tokenmarket setup
npx --yes tokenmarket setup claude-code
npx --yes tokenmarket doctor
```

Alternatively, install globally to use `tokenmarket` directly:

```bash
npm install -g tokenmarket
tokenmarket setup
```

These commands require the current version to be published to npm first; having the launcher in this repository does not mean publication is complete. See [CLI npm releases](docs/cli-npm-release.md).

Prebuilt packages support macOS Apple Silicon / Intel, Windows x64, and Linux x64 / arm64 with glibc 2.35+ (for example, Ubuntu 22.04+). npm selects the platform binary automatically; keep optional dependencies enabled. Build from source for Alpine/musl or other architectures.

Prerequisites:

- Install a supported AI agent tool before running setup. The CLI only configures tools it can detect on your computer.
- To build the CLI from source, install a stable Rust toolchain with Cargo.
- The `tokenmarket` executable must be on your `PATH`.

To install the executable from source:

```bash
cargo install --path rust/tokensmarket-cli
```

Cargo normally installs it under `~/.cargo/bin`; make sure that directory is on your `PATH`. Alternatively, build without installing and invoke the binary by path:

```bash
cargo build --release --manifest-path rust/tokensmarket-cli/Cargo.toml
rust/tokensmarket-cli/target/release/tokenmarket --version
```

### Desktop users

Download the guided desktop app from GitHub Releases:

- **macOS**: download the `.dmg`, open it, and drag **Token Market** into **Applications**.
- **Windows**: download the `x64-setup.exe` installer and run it.

The app is not yet signed and notarized with an Apple Developer ID. If macOS blocks the first launch, run:

```bash
xattr -cr "/Applications/Token Market.app"
```

Then open the app normally.

## Desktop quick start

Sign in through the browser and return with **Open Application**. View balance or switch the Current Workspace, then **Connect** an installed tool. Its card retains its owning account and Bound Workspace. Connect does not run a paid model request; **Test** is a separate action with a charge confirmation.

## Terminal quick start

Run setup without arguments to choose an installed tool interactively:

```bash
tokenmarket setup
```

Or configure a specific tool directly:

```bash
tokenmarket setup claude-code
```

After setup, check the local installation and gateway reachability:

```bash
tokenmarket doctor
tokenmarket test claude-code
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

### `tokenmarket setup [tool]`

Authorize Token Market, create a CLI API key, write the selected tool's configuration, and run verification requests.

Options:

- `--model <preset>`: choose a model preset. Defaults to `recommended`.

Examples:

```bash
tokenmarket setup codex
tokenmarket setup codex --model fast
```

### `tokenmarket doctor [tool]`

Show the configured gateway URL, each tool's setup state, configuration path, and model API reachability. Passing a tool ID limits the report to that tool.

```bash
tokenmarket doctor openclaw
```

### `tokenmarket test <tool>`

Run configured request tests. By default, this runs all tests.

Options:

- `--models`: model list request
- `--chat`: non-streaming chat request
- `--stream`: streaming chat request
- `--tool-call`: tool-call request

```bash
tokenmarket test codex --models --chat
```

### `tokenmarket keys`

List API keys managed by this CLI. The command shows key names, key IDs, target tools, and creation times; it never prints the API key secret.

```bash
tokenmarket keys
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

- Terminal CLI authorization uses PKCE and a one-time local callback at `http://127.0.0.1`.
- The terminal CLI browser authorization page lets the logged-in user choose the workspace that will own the CLI API key.
- On Unix-like systems, adapter configuration and credential files are written with owner-only file permissions. On Windows, privacy relies on the user-profile and parent-directory ACLs.
- Before modifying an existing configuration, the CLI copies it to `~/.tokensmarket/backups/`.
- The local key registry is stored at `~/.tokensmarket/keys.json`; it records metadata only and does not replace the tool configuration.
- Re-running setup replaces the local managed record for that tool. Revoke the actual key from the Token Market dashboard if you no longer want it to remain active.

Desktop login instead uses PKCE and a one-time registered application-protocol callback, without choosing a workspace or issuing a tool key during login. Private desktop session storage, per-field configuration receipts, exact-key cleanup and testing limits are described in [desktop management](docs/desktop-management.md).

## Architecture

- `rust/tokensmarket-core`: shared setup domain. Contains tool adapters, PKCE authorization, configuration discovery and backups, the managed-key registry, model resolution, and gateway verification.
- `rust/tokensmarket-cli`: the `tokenmarket` executable. It is a thin command-line shell over the shared core.
- `npm`: the npx / npm launcher. Selects the platform's prebuilt Rust CLI and preserves arguments, terminal I/O and exit status.
- `src-tauri`: the native desktop app. It is also a thin shell and invokes the same Rust core directly.
- `src/tauri-ui`: the desktop app's HTML interface.

## Development

There is no root Cargo workspace, so Cargo commands must address the desired crate manifest directly.

The npm launcher tests use Node.js's built-in test runner and require no npm dependency installation:

```bash
npm test
```

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
