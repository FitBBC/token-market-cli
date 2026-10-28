# Desktop tool management

Implements [parent issue #1](https://github.com/FitBBC/token-market-cli/issues/1).
See [desktop login](desktop-login.md) for PKCE, registered-protocol return and
private-session storage; see [inventory](tool-inventory.md) for discovery paths.
The terminal CLI retains its separate per-setup loopback authorization flow.

Desktop login enters the platform Default Workspace. Current Workspace controls
balance, recharge and the next Connect/Reconnect. Existing cards retain their
actual owning account and Bound Workspace, which controls their model billing.
Changing workspace or logging out does not rewrite existing Agent configuration.

| Tool | Configured protocol | Model policy |
| --- | --- | --- |
| Claude Code | `ANTHROPIC_MESSAGES` | `glm-5.3` must advertise this endpoint; settings model and managed env overrides use GLM |
| Codex CLI | `OPENAI_RESPONSES` | `glm-5.3` must advertise this endpoint; provider uses Responses |
| WorkBuddy | `OPENAI_CHAT_COMPLETIONS` | All key-visible compatible models; preserves existing default preference |
| DeepSeek Harness | `OPENAI_CHAT_COMPLETIONS` | All key-visible compatible models; profile and credential file are restored together |
| OpenClaw | `OPENAI_CHAT_COMPLETIONS` | All key-visible compatible models; existing agent selection/allowlists stay in place |
| OpenCode | `OPENAI_CHAT_COMPLETIONS` | All key-visible compatible models; root model selections stay in place; JSONC comments/trailing commas retained |

Connect obtains a device/tool key and its catalogue, then atomically publishes
configuration and private ownership receipts. It stops if required models or
protocols are unavailable. It performs no generation request. Multi-model import
adds choices without ranking them or selecting a preferred model.

Reconnect captures the current account and workspace, obtains a new key/catalogue,
then replaces the configuration before disabling only the old connection's exact
key. Failure preserves the previous configuration/key and makes one cleanup
attempt for an unused new key. Cross-account old-key disable failure is reported
as **Old Key Not Disabled**, while the usable new connection remains active.

Disconnect restores only owned fields, preserving unknown settings and later
edits. Conflicts require a fresh explicit choice; an old whole-file backup never
overwrites current work. It then makes at most one best-effort exact-key disable
attempt; failure does not block local completion or create a retry queue. Existing
OpenCode `opencode.jsonc` remains the effective file, with no competing `.json`.
Its edit/restore path uses [jsonc-parser's concrete syntax tree](https://docs.rs/jsonc-parser/0.34.0/jsonc_parser/cst/index.html), restricted to JSON plus comments/trailing commas. It edits changed fields while retaining unknown content; comments inside a deleted owned container remain as standalone comments.

Test requires charge confirmation and, for multi-model tools, one configured
model. It checks current owned configuration against the receipts before any
billable request, rejecting drift, missing files and competing credentials without
sending generation traffic. Unrelated settings remain usable. Historical passes
show **Configuration Changed / Unavailable** after owned configuration drifts, with paid Test disabled until repaired.
The request uses the saved connection's actual key, Bound Workspace, gateway and
protocol even after desktop logout/account/workspace changes. A passed test proves
one complete non-streaming text response; it does not prove streaming, tool calls,
Agent runtime behavior, or the availability of every imported model.

Open Tool resolves a fresh installation snapshot. Windows CLI terminals inherit
fresh discovered PATH directories, including newly installed `.cmd` launchers;
macOS Terminal receives the resolved command and its fresh PATH (including nvm).
Only fixed tool commands/arguments are used. GUI tools use verified bundle/exe
paths. Launching does not restart an existing Agent: fully reopen the tool when
requested. OpenClaw requires Gateway/session restart, and existing allowlists may
still restrict its picker. OpenCode project/inline configuration can override its
global provider. These native tool behaviors require separate installed acceptance.

## Operator coordination and acceptance

Deploy the platform, website and gateway contracts together before shipping the
matching desktop build. The platform's
`docs/sql/temp/api_tokens_desktop_device_20261028.sql` adds `api_tokens.device_id`;
this migration is prepared **NOT EXECUTED** and requires operator execution in the
confirmed target environment. Gateway `/v1/models` must expose
`supported_endpoints`; the old target response lacks that field and therefore
cannot establish the required protocol compatibility. Do not infer readiness from
HTTP 200. Desktop Redis keys use the platform's current
`RedisConstants.REDIS_PRE_KEY` namespace. No unprefixed desktop grants were deployed
for this new feature, so no compatibility migration was added.

Source/fixture checks (run on both native CI hosts):

```sh
cargo fmt --manifest-path rust/tokensmarket-core/Cargo.toml --all --check
cargo fmt --manifest-path rust/tokensmarket-cli/Cargo.toml --all --check
cargo fmt --manifest-path src-tauri/Cargo.toml --all --check
cargo test --manifest-path rust/tokensmarket-core/Cargo.toml --all-targets --locked
cargo test --manifest-path rust/tokensmarket-cli/Cargo.toml --all-targets --locked
cargo test --manifest-path src-tauri/Cargo.toml --all-targets --locked
```

These commands use temporary configurations and fake HTTP. They do not launch
installed tools, call real models, run DDL or deploy services. macOS compilation
and command-plan fixtures do not substitute for Windows CI/installed acceptance.
Pending installed-bundle checks on **both macOS and Windows**: cold/warm deep-link
handoff, browser prompt/manual Open Application, private-store denial/recovery,
application/computer restart, newly installed tool redetection and actual terminal
or application launch for all six tools. Target environment authorization,
permissions, balance/recharge, catalogue metadata and real paid protocol tests
also remain pending until coordinated deployment and authorized acceptance.
