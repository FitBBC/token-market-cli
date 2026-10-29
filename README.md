# Token Market

[English](README.md) | [简体中文](README.zh-CN.md)

Connect local AI agent tools to Token Market. Use the desktop app to manage connections, or configure a tool from the terminal.

Before you start, install the agent tool you want to connect and have a Token Market account ready.

## Desktop app

### Install

Download a desktop installer from [GitHub Releases](https://github.com/FitBBC/token-market-cli/releases):

- **macOS**: open the `.dmg` and drag **Token Market** into **Applications**.
- **Windows**: run the `x64-setup.exe` installer.

### Sign in and connect a tool

1. Open Token Market and click **使用浏览器登录** (Sign in with browser).
2. Sign in in your browser, then click **Open Application** to return to the app.
3. Confirm the current workspace. New connections will be billed to this workspace.
4. Find your installed tool and click **接入** (Connect). Click **重新检测** (Detect again) if you just installed it.
5. Reopen the agent tool and select the Token Market model as prompted.

Connecting only configures the tool; it does not send model requests. To check a connection, click **测试** (Test) in its details, select a model if several are available, and confirm the possible charge.

### Manage connections

- **重新接入** (Reconnect): update a connection using the current account and workspace. To change its billing workspace, switch the current workspace first, then reconnect.
- **取消接入** (Disconnect): restore settings managed by this connection and attempt to disable its Key. If settings conflict, follow the prompts to keep current values or restore the originals.
- **Check billing workspace**: use the workspace shown in the connection details. Switching the current workspace or signing out does not change existing connections.

If the app reports that an old Key was not disabled, check and disable it in the Token Market dashboard's Key management.

## Terminal CLI

### Install and run

Requires **Node.js 20 or newer**. Run directly without installing the desktop app:

```bash
npx --yes tokenmarket setup
```

Or install globally to use the `tokenmarket` command:

```bash
npm install -g tokenmarket
tokenmarket setup
```

The npm method requires a version published to npm. If the package or version cannot be found, use a desktop installer from [Releases](https://github.com/FitBBC/token-market-cli/releases).

The CLI supports macOS Apple Silicon / Intel, Windows x64, and Linux x64 / arm64 with glibc 2.35+ (such as Ubuntu 22.04+).

The examples below use the globally installed command. Without a global install, replace `tokenmarket` with `npx --yes tokenmarket`.

### Connect a tool

Run `tokenmarket setup`, choose a tool, sign in and select an authorization workspace in your browser, then return to the terminal and wait for setup to finish.

You can also specify a tool directly:

```bash
tokenmarket setup claude-code
tokenmarket setup codex
```

After setup, reopen the agent tool and select the Token Market model as prompted.

**CLI setup automatically sends verification requests, which may incur model usage charges.**

### Choose a model

Use `--model` to select a preset. The default is `recommended`:

```bash
tokenmarket setup codex --model fast
```

| Preset | Model |
| --- | --- |
| `recommended` (default) | `glm-5.3` |
| `powerful` | `glm-5.3` |
| `cheap` | `glm-5.3` |
| `fast` | `deepseek-v4.1-flash` |

### Check configuration and test a connection

Check configuration and model-list connectivity for all tools or one tool:

```bash
tokenmarket doctor
tokenmarket doctor codex
```

Send test requests for a configured tool:

```bash
tokenmarket test codex
tokenmarket test codex --models
tokenmarket test codex --chat --stream
```

| Test option | Check |
| --- | --- |
| `--models` | Model list |
| `--chat` | Non-streaming chat |
| `--stream` | Streaming chat |
| `--tool-call` | Tool calls |

Without test options, all tests run. Chat, streaming and tool-call tests send real model requests and may incur charges.

### View and disable Keys

```bash
tokenmarket keys
```

Lists the names, IDs, target tools and creation times of Keys managed by this CLI, without displaying secrets. Disable Keys in the Token Market dashboard; running setup again does not automatically revoke the old Key.

## Supported tools

Select a tool in the desktop app, or use its ID in terminal commands:

| Tool | Terminal tool ID |
| --- | --- |
| Claude Code | `claude-code` |
| Codex CLI | `codex` |
| WorkBuddy | `workbuddy` |
| DeepSeek Harness | `deepseek-harness` |
| OpenClaw | `openclaw` |
| OpenCode | `opencode` |

## Troubleshooting

### macOS blocks the first launch

The app is not yet signed and notarized with an Apple Developer ID. After confirming that the installer came from this repository's Releases, run:

```bash
xattr -cr "/Applications/Token Market.app"
```

Then open the app again.

### npm reports a missing bundled binary

The npm package bundles all five platform binaries. If an installation is incomplete, reinstall:

```bash
npm install -g tokenmarket
```

### An installed tool is not detected

Click **重新检测** (Detect again) in the desktop app. For the CLI, make sure the tool can be launched from your terminal. Reopen the terminal if needed, then run setup again.

### A connected tool still uses its old settings

Fully quit and reopen the tool. OpenClaw also needs a Gateway or session restart. OpenCode project settings may override its global configuration.

### The desktop app reports changed configuration or disables testing

Check whether the tool's settings were changed after connecting. Follow the prompts to repair the configuration or reconnect, then test again.

### Find configuration backups made by the CLI

The CLI backs up existing configuration to `~/.tokenmarket/backups/` before changing it. The setup output also shows the backup path.
