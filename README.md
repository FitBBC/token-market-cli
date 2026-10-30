# Token Market

[English](README.md) | [简体中文](README.zh-CN.md)

Connect local AI agent tools to Token Market from the desktop app or terminal. Both share your sign-in, current workspace and tool connections, so you can switch between them.

**Version 1.1.0** · [Desktop download](https://github.com/FitBBC/token-market-cli/releases/tag/v1.1.0) · [npm package](https://www.npmjs.com/package/tokenmarket)

## Install

Have a Token Market account and install the AI agent tool you want to connect first.

### Desktop

Download an installer from [Releases](https://github.com/FitBBC/token-market-cli/releases/tag/v1.1.0):

| System | Installation |
| --- | --- |
| macOS, Apple Silicon | Open the `.dmg` and drag **Token Market** into **Applications**. |
| Windows x64 | Run the `x64-setup.exe` installer. |

### CLI

Requires **Node.js 20+**. Supports macOS arm64/x64, Windows x64 and Linux x64/arm64 with glibc 2.35+.

```bash
npm install -g tokenmarket@1.1.0
tokenmarket --version
```

Or run without a global installation:

```bash
npx --yes tokenmarket@1.1.0 login
```

The npm package includes all five native binaries; the launcher selects the one for your system.

## Connect your first tool

### In the desktop app

1. Open Token Market and click **使用浏览器登录** (Sign in with browser).
2. Finish signing in, then click **Open Application** to return to the app.
3. Confirm the current workspace. New connections use this workspace for billing.
4. Find your tool and click **接入** (Connect). Click **重新检测** (Detect again) if you just installed it.
5. Reopen the tool and select the Token Market model as prompted.

### In the terminal

```bash
tokenmarket login
tokenmarket workspace
tokenmarket connect claude-code
tokenmarket status
```

To switch workspaces, use an ID shown by `workspace`:

```bash
tokenmarket workspace <workspace-id>
```

CLI and desktop share sign-in on the same machine and environment. Signing in through either lets you manage connections from both. Switching the current workspace affects new connections; existing tools keep their bound billing workspace.

## Supported tools

Choose a tool in the app, or use its ID in CLI commands:

| Tool | CLI ID |
| --- | --- |
| Claude Code | `claude-code` |
| Codex CLI | `codex` |
| WorkBuddy | `workbuddy` |
| DeepSeek Harness | `deepseek-harness` |
| OpenClaw | `openclaw` |
| OpenCode | `opencode` |

Claude Code and Codex use `glm-5.3` in production (`glm-5.2` in test builds). Other tools receive the compatible text-generation model catalogue.

## Manage connections

| Action | Desktop | CLI |
| --- | --- | --- |
| View ownership, models and status | Open tool details | `tokenmarket status [tool]` |
| Update a connection or change its billing workspace | Switch workspace, then **重新接入** | `tokenmarket reconnect <tool>` |
| Restore managed configuration | **取消接入** | `tokenmarket disconnect <tool>` |
| Test a model request | **测试**, choose a model and confirm | `tokenmarket test <tool> [--model <id>]` |
| Sign out of shared management | Account menu → **退出登录** | `tokenmarket logout` |

Connecting and reconnecting configure tools without sending model requests. **Testing sends one potentially billable model request.** For tools with several saved models, specify one with `--model <id>`.

Reconnecting asks for confirmation when the account or billing workspace changes. Disconnecting restores the fields managed by the connection and asks how to handle later edits. Signing out preserves tool configurations and saved credentials.

### SSH and noninteractive use

For a machine without a browser:

```bash
tokenmarket login --no-browser
```

Open the displayed URL on another device. The terminal waits for confirmation; the browser does not receive tool Keys or long-lived client credentials.

Noninteractive commands require explicit choices when applicable:

| Situation | Option |
| --- | --- |
| Confirm a reconnect changes account or workspace | `--confirm-rebind` |
| Recover after every local credential copy is lost | `--confirm-recovery` |
| Keep later edits during disconnect | `--conflicts keep-current` |
| Restore original values during disconnect | `--conflicts restore-original` |

Use `tokenmarket <command> --help` for command options.

## Credentials and recovery

CLI and desktop reuse credentials for the same environment, account, workspace, device and tool. Saved Keys, connection records and restoration backups are encrypted. macOS and Windows use the system credential store; Linux prefers Secret Service and falls back to a passphrase-encrypted vault when unavailable. A locked or unreadable store returns an error.

Reconnect validates the saved credential. Deleted or reset credentials can be replaced; disabled or expired credentials require action in the platform. Balance, network and permission errors do not create replacement Keys. If every local copy is lost, recovery asks you to confirm the affected connection and retires its old credential after the replacement configuration succeeds.

## Troubleshooting

| Symptom | What to do |
| --- | --- |
| Installed tool is missing | Click **重新检测**, or make sure the tool is available in your terminal and reopen the terminal. |
| Tool still uses old settings | Fully quit and reopen it. Restart the OpenClaw Gateway/session; check OpenCode project settings for overrides. |
| App reports changed configuration or disables testing | Follow the prompts to repair or reconnect, then test again. |
| npm reports a missing bundled binary | Reinstall with `npm install -g tokenmarket@1.1.0`. |
| Need to restore settings | Use **取消接入** or `disconnect`. Encrypted restoration records are under `~/.tokenmarket/desktop-connections/`. |

### macOS blocks the first launch

The release is not signed and notarized with an Apple Developer ID. After verifying that the installer came from this repository's Releases, run:

```bash
xattr -cr "/Applications/Token Market.app"
```

Then reopen the app. For internal builds and shared keychain signing, see the [macOS test signing guide](docs/macos-local-test-signing.md).

## Development and release

- [CLI packaging and npm release](docs/cli-npm-release.md)
- [Desktop management](docs/desktop-management.md) and [browser sign-in](docs/desktop-login.md)
- [Tool inventory and configuration](docs/tool-inventory.md)
- [Shared credential lifecycle](docs/adr/0004-shared-client-key-lifecycle.md)

CLI and desktop use the same `rust/tokenmarket-core` and version number. Release tags `cli-v1.1.0` and `v1.1.0` point to the same commit and trigger their respective builds.
