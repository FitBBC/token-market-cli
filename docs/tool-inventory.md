# Desktop tool inventory

`ToolInventory::current().list()` is a read-only, fresh filesystem read. Installation
discovery never needs tool credentials or executes a tool. Connection summaries decrypt app-owned receipts through the shared connection
manager, whose encryption key is cached after successful OS credential-store
access. They also read existing receipt-owned configuration fields to check that an earlier
Test Passed still applies; owned drift or missing files shows Configuration Changed
/ Unavailable and clears the current pass from the summary. Credentials are never displayed. Inventory does not obtain a
Key, write configuration or make network/model requests.

`require_installed(tool)` is the connection precondition; callers run it immediately
before beginning an operation. It does not authorize the connection or replace
its session/workspace checks. Connect and Reconnect repeat this gate. Open Tool
uses the same fresh installation snapshot to create its command plan, so newly
discovered Windows PATH entries and macOS nvm paths also reach the launched tool.

The public fixture boundary is `ToolInventory::new(InstallationEnvironment)`.
Tests supply a temporary home, command directories, application directories,
platform and PATHEXT extensions, then observe the inventory and allowed/refused
connection action. No environment variable or real configuration is mutated.
Configuration locations continue to use the existing adapters and their explicit
`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `WORKBUDDY_CONFIG_PATH`, `DSH_HOME`,
`DSH_ACTIVE_PROFILE`, `OPENCLAW_CONFIG_PATH` and `OPENCODE_CONFIG` overrides.
Windows home discovery uses `USERPROFILE`.

## Installation evidence

- Commands: `claude`, `codex`, `workbuddy`, `dsh`, `openclaw`, `opencode`.
  Unix discovery requires an executable file, including a valid symlink target.
  Search includes PATH and common user/package-manager binary directories.
- macOS desktop applications: WorkBuddy, DeepSeek Harness, OpenClaw and OpenCode
  under `/Applications` and `~/Applications`. A readable XML/binary Info.plist
  must declare a CFBundleExecutable that exists and has executable permission
  inside Contents/MacOS. A bundle directory alone is insufficient. Claude.app
  and Codex.app are different products and do not prove their CLI is installed.
- Windows: case-insensitive executable/launcher lookup honors PATHEXT for EXE,
  CMD, BAT and COM. Discovery also reads current user/machine registry PATH so
  later installs can be re-detected without retaining the process's old PATH.
  Desktop executable discovery checks LOCALAPPDATA, LOCALAPPDATA/Programs,
  ProgramFiles and ProgramFiles(x86), plus supported products' uninstall registry
  InstallLocation values in both registry views. Registry metadata without an
  actual executable is insufficient.
- A configuration directory such as `.dsh`, credentials or an old key record is
  never sufficient installation evidence. No detection probe executes a tool.

The six rows display native SVG tool marks, names and installation status.
Missing tools offer the fixed official download page and a disabled Connect.
The UI refreshes after login, explicit re-detection, foreground/visibility return
and every 15 seconds while visible and authenticated. Logout clears the region
and late results cannot restore a previous account's inventory. Old CLI key-history files are not read or projected. Unknown tool configuration
is not assigned to the current account/workspace by inference.

`download_tool` opens only the fixed URL selected by the Rust catalogue. The UI
never supplies an arbitrary download URL. Verified primary pages, 2026-10-28:

| Tool | Official installation/download page |
| --- | --- |
| Claude Code | [Claude Code setup](https://code.claude.com/docs/en/setup) |
| Codex CLI | [Codex CLI](https://developers.openai.com/codex/cli/) |
| WorkBuddy | [WorkBuddy](https://www.workbuddy.ai/) |
| DeepSeek Harness | [DeepSeek downloads](https://www.deepseek.com/download/) |
| OpenClaw | [OpenClaw install](https://docs.openclaw.ai/install) |
| OpenCode | [OpenCode install](https://opencode.ai/docs/) |

DeepSeek's [desktop README](https://github.com/deepseek-ai/deepseek-harness/blob/master/apps/desktop/README.md)
confirms `dsh` / `dsh.cmd`, optional command registration and uninstall preservation
of `.dsh`. WorkBuddy's [vendor download archive](https://www.codebuddy.cn/docs/workbuddy/Download-History)
links the vendor's current site and macOS/Windows installers. These are verified
landing-page entries, not a claim that downloading/installing succeeded.

## Integration and verification boundaries

`window.TokenMarketTools.setAuthenticated(boolean)` controls `#tools-region`.
Connect, Reconnect, Disconnect and charge-confirmed Test call the public core
application manager through native commands. Account/workspace changes leave
retained connections intact. See [desktop management](desktop-management.md) for
model policies, exact-key cleanup, configuration recovery and test proof limits.

Automated temporary-filesystem fixtures cover missing tools, residual configuration,
all six command names, installation/removal re-detection, macOS bundle evidence,
Windows PATHEXT/case handling and desktop binaries. Public command-plan fixtures
cover newly discovered `.cmd` launchers with spaces/ampersands in their directory,
and macOS nvm commands with shell/AppleScript quoting. No native tools are executed.

The macOS native target compiles locally. Full Windows compilation and the opted-in
private-store smoke are reported by hosted native-validation CI for its checked
revision; a successful job is CI proof for that revision. These checks do not
establish installed-bundle protocol handoff, registry/installer integration,
application/computer restart, actual tool launch or target deployment. Those
remain separate [native acceptance](desktop-login.md#verification-boundaries).
