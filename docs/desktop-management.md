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
| Claude Code | `ANTHROPIC_MESSAGES` | `glm-5.3` (test build: `glm-5.2`) must have `TEXT_GENERATION` in `model_types`; the connection uses the Anthropic Messages protocol |
| Codex CLI | `OPENAI_RESPONSES` | `glm-5.3` (test build: `glm-5.2`) must have `TEXT_GENERATION` in `model_types`; provider uses Responses |
| WorkBuddy | `OPENAI_CHAT_COMPLETIONS` | All key-visible text-generation models; preserves existing default preference |
| DeepSeek Harness | `OPENAI_CHAT_COMPLETIONS` | All key-visible text-generation models; profile and credential file are restored together |
| OpenClaw | `OPENAI_CHAT_COMPLETIONS` | All key-visible text-generation models; existing agent selection/allowlists stay in place |
| OpenCode | `OPENAI_CHAT_COMPLETIONS` | All key-visible text-generation models; root model selections stay in place; JSONC comments/trailing commas retained |

Connect obtains a device/tool key and its catalogue, then atomically publishes
configuration and private ownership receipts. It stops if required models or
text-generation models are unavailable. It performs no generation request. Multi-model import
adds choices without ranking them or selecting a preferred model.

Retired model fields retain their original restoration baseline as history, while
current configuration checks use only fields still actively managed. Removed
fields are restored once, so subsequent unrelated edits remain user-owned. A model
that reappears can reuse its first baseline; if a released field was edited in the
meantime, replacement stops safely for explicit repair rather than overwriting it.

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

Deploy the platform and website contracts before shipping the matching desktop
build. The platform's
`docs/sql/temp/api_tokens_desktop_device_20261028.sql` adds `api_tokens.device_id`;
this migration is prepared **NOT EXECUTED** and requires operator execution in the
confirmed target environment. Gateway `/v1/models` must expose `model_types` containing `TEXT_GENERATION` for the
authenticated key. This identifies model type and does not itself prove a particular protocol call succeeds. Desktop Redis keys use the platform's current
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

## Test environment build and deployment checks

Build a separate test-named desktop bundle with `scripts/build-test-desktop.sh`.
It requires the Tauri CLI and uses the domestic test WebAPI and gateway as
compile-time defaults. Runtime `TOKEN_MARKET_CONSOLE_API_BASE_URL` and
`TOKEN_MARKET_GATEWAY_API_BASE_URL` override those defaults. The test bundle
shares the `tokenmarket://` protocol and Agent configuration with other builds;
use a separate OS user or test machine for isolation.

After deployment, run `python3 scripts/check-desktop-test-environment.py` to
check the public WebAPI status, the three desktop pages, and gateway reachability.
Add `--auth-create` to create one anonymous authorization request with a short
expiry and check its browser origin. The script does not inspect deployment
revision, authenticate a user, install the app, access a model, or test billing.
Verify those separately. Use `--help` for endpoint overrides.

To verify a test Key's actual gateway contract, save only the JSON response body
from `/v1/models` to a private temporary file (never include its authorization
header) and run `python3 scripts/check-model-catalogue.py <response.json>`. This
checks `model_types` and the presence of the required text-generation model for that Key; it does not verify a protocol call. Access remains workspace- and
Key-specific; no model call is made by the script.
