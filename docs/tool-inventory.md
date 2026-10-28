# Desktop tool inventory

`ToolInventory::current().list()` is a read-only, fresh filesystem read. It never
runs a tool, obtains an Agent API Key, reads tool credentials, or writes an Agent
configuration. `require_installed(tool)` is the connection precondition; callers
must run it immediately before beginning their connection operation. The native
`prepare_tool_connect` command exposes that same gate. It does not authorize the
later connection or replace the session/workspace checks required by Connect.

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
and late results cannot restore a previous account's inventory. Existing legacy
key records display their saved model with account and Bound Workspace marked
unknown; the current account/workspace is never used to fill these fields.

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
A successful native installation gate emits a cancelable
`tokenmarket-connect-request` event with `detail: {tool}`. The next connection
module handles it with `preventDefault()`; until then the action shows a
placeholder, and never invokes legacy `start_setup` or automatic model tests.
T5 must repeat the core gate at its actual mutation entrypoint.

Automated fixture checks cover missing tools, residual configuration, all six
command names, installation/removal re-detection, macOS bundle evidence, Windows
PATHEXT/case handling and Windows desktop binaries. They do not establish Windows
native launch, registry, installer, or browser behavior. On this macOS machine,
`cargo run --manifest-path rust/tokensmarket-core/Cargo.toml --example inspect_inventory`
read actual installed files and reported Claude Code, Codex CLI, WorkBuddy and
DeepSeek Harness installed; OpenClaw and OpenCode missing. No configuration was
changed and no model request was sent. macOS native Tauri compilation passed.
Windows core cross-compilation is blocked by the existing ring dependency's C
build: Windows SDK `assert.h` is unavailable on this macOS host. The isolated
Windows registry discovery functions type-check against winreg with the Windows
Rust target, but that is not full Windows application compilation or acceptance.
