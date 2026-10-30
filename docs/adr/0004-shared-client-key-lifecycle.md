---
status: accepted
---

# Shared client Key lifecycle with a compatible platform boundary

Date: 2026-10-30

Token Market's CLI and desktop are being refactored into two management interfaces
for the same local tool connections. The platform, website and gateway behavior
from before legacy CLI integration is the compatibility baseline. Client-specific
authorization, credential validation and issuance may be added in an isolated
integration layer; they must not redefine existing platform Key or billing rules.

The user confirmed Q1–Q12 and the consolidated design on 2026-10-30. This ADR is
accepted for implementation, not proof of implementation or deployment. It
supersedes the applicable Key-lifecycle and CLI/desktop-separation portions of
ADRs 0001–0003; unrelated tool support and configuration-preservation decisions
remain valid.

## Confirmed decisions

- Q1: preserve original website interfaces, Key generation/hash/verification,
  permissions, billing and gateway semantics. Isolated client endpoints and
  necessary ownership metadata are permitted. Existing integration changes must
  be audited by behavior rather than rolled back by whole commit or date.
- Q2: the new CLI and desktop share one saved credential, active connection and
  configuration-restoration history for the same environment, account, workspace,
  device and Tool. Different tools, devices, accounts, workspaces and environments
  are independent. One active local configuration per Tool remains the current
  supported installation model.
- Q3: Connect/Reconnect consult local credentials and validate a saved credential
  with the platform. A valid credential is reused. An explicitly deleted Key or
  a fingerprint mismatch after reset permits replacement. Disabled or expired
  Keys remain saved and require user action in the platform. Network failures,
  server errors, login expiry, balance, quota and model-permission errors never
  imply permission to issue a replacement.
- Q4: both interfaces share the Client Session and Current Workspace on a device.
  A login through either interface can be reused by the other. Logout ends that
  shared management login but preserves saved tool credentials and configurations.
  Workspace selection does not rewrite an existing Tool Connection.
- Q5: no legacy CLI compatibility is required: the user confirms the legacy flow
  has no users. Delete its per-setup authorization, exchange and bulk-disable flow
  rather than retaining a compatibility branch. This does not authorize deleting
  platform Key lifecycle code or unrelated platform/website functionality.
- Q6: client-specific issuance records, request deduplication and a durable-save
  confirmation are required. Recover a successful creation whose response was
  lost through that issuance protocol, not by retrieving historical plaintext.
  Recovery is confined to the exact unfinished issuance and must not modify other
  devices, tools or already-confirmed credentials as a side effect of a retry.
- Q7: if the saved credential is missing, first try to recover the exact credential
  from the tool's existing configuration and verify its ownership and fingerprint.
  If every local copy is truly lost, explicit user confirmation permits replacement;
  after the new connection succeeds, retire only the exact prior client-issued Key
  for that account/workspace/device/tool. A locked or inaccessible credential store
  is an error, not evidence of loss. Other scopes and console-issued Keys are untouched.
- Q8: retain Linux CLI support, including remote SSH environments. Provide safe
  credential persistence and remote browser authorization; do not silently drop
  Linux support or persist management credentials in plaintext.
- Q9: the CLI and desktop use the same connection, model-configuration, restoration
  and explicit-test rules. Remove legacy CLI model presets. Claude Code and Codex
  use the established required model policy; other supported tools receive the
  compatible catalogue. Connect configures only; paid model tests require a separate
  user action.
- Q10: retain independently accepted model_types metadata and recharge workspace-name
  display. Restore/isolate client-induced changes to shared permissions and auth
  without rolling back unrelated features by date. No new gateway Key changes
  are required by this refactor.
- Q11: Linux prefers an available system secret store. If none is usable, allow
  a local credential vault encrypted with a user-supplied passphrase. Require
  explicit unlocking; noninteractive operations without an available unlock source
  fail rather than silently storing plaintext. Remote login displays a browser
  authorization link that can be completed on another device; only the initiating
  CLI can collect the resulting Client Session, and the browser does not display
  long-lived session credentials or Agent API Keys.
- Q12: do not implement legacy CLI keys.json/setup-history migration or retain
  compatibility branches for those records. Do not proactively delete real local
  tool configuration or credentials. Existing desktop credentials, connection
  records and restoration history with known ownership remain reusable.
- Ordinary Disconnect restores the client's managed configuration under the
  existing conflict-preservation rules, retaining its saved credential. Remove
  the separate Clear Connection action and manual create/reuse choices from the
  new normal user flow.
- The platform cannot return a previously issued Key plaintext. Existing Key
  creation/reset, soft-delete and audit semantics are retained. Local reuse needs
  the client's securely saved copy; a fingerprint is not a replacement secret.

## Proposed issuance and connection protocol

Use a client-specific binding/issuance record in the platform, separate from the
original API Token lifecycle and status fields. It associates the registered
client, environment, account, workspace, device, Tool, Key ID, request ID and an
issuance generation. A new server-generated secret is returned only in the
creation or rotation response; server storage and ordinary reads never retain
or return historical plaintext.

1. Authenticate the shared Client Session and validate the requested scope with
   the original platform permission rules. The browser login is separate from
   tool-Key issuance and never delivers a model Key.
2. If a local Key exists, validate its ID, fingerprint, ownership, status and expiry
   through the isolated client API. Deleted/reset credentials may be replaced;
   disabled/expired credentials and uncertain failures stop without issuing a Key.
3. When issuance is needed, serialize operations for the exact scope on the server
   as well as in the local shared store. Create one tracked pending Key record.
4. Persist the returned Key and issuance generation durably in the encrypted
   local store, then confirm receipt. Receipt confirmation is idempotent. Fetching
   the model catalogue and applying tool configuration happen after this save;
   their failure never causes another issuance on retry.
5. If creation succeeded but its response was lost, and no local copy was saved,
   resume the exact pending issuance. Rotate only that unconfirmed Key's secret
   using the platform's existing Key-reset capability, retaining its Key record
   rather than creating another row for every transport retry. Each response is
   a fresh secret, never a plaintext replay. A generation check rejects late
   responses/confirmations; an externally disabled or expired pending Key must not
   be revived. An explicitly deleted pending Key may be replaced under Q3.
6. Once receipt is confirmed, normal retries only validate/reuse the saved Key.
   They cannot rotate it through pending-issuance recovery. Confirmed-copy loss
   follows the separate, user-confirmed Q7 replacement operation.
7. Apply configuration through the shared restoration/conflict-preservation engine.
   For Q7 replacement, keep the old Key until the new local configuration succeeds,
   then retire its exact recorded predecessor through the client coordinator.
   Record any interrupted finalization so the next explicit operation can resume
   it without another issuance or a misleading claim that cleanup succeeded.

If the old device identity is also lost, the client cannot infer a predecessor
from the account and Tool alone. Treat it as a new device; do not bulk-retire the
old device's credentials. Neither a store permission error nor malformed local
data may be interpreted as an empty credential cache.

The existing platform create/reset/disable methods retain their original
semantics. The new client coordinator owns request/generation checks, binding
transactions and the receipt/configuration distinction. It must not turn public
createToken into an implicit reuse endpoint or return old plaintext.

## Local interfaces and compatibility disposition

- The CLI and desktop share a cross-process operation lock, device identity,
  encrypted credential/connection store and restoration receipts. Account logout
  or a workspace change invalidates stale confirmation contexts in both interfaces.
- Keep Current Workspace distinct from Bound Workspace. Reuse the existing platform
  workspace preference operation; do not create a new webpage preference rule.
  An explicit workspace target for one CLI operation is checked and displayed;
  it must not silently rewrite an existing tool's billing workspace.
- CLI management commands cover login/logout, workspace selection, connect/reconnect,
  disconnect, status and explicit testing. Existing setup may be a convenience
  alias to the new Connect behavior, never an entry to legacy authorization.
- Retain existing desktop credentials and restoration data. Unknown local data is
  not assigned to the currently logged-in account by inference.
- Remove old CLI authorization/exchange endpoints, the dedicated old consent page,
  bulk-disable capability, model presets and obsolete tests/docs. Distinguish
  exclusive legacy dependencies from shared crypto, permission, billing and
  registered-client capabilities before deleting code. Historical database data
  is not remotely deleted merely because source code is removed.
- Restore the pre-CLI shared API-token workspace guard. Accept client sessions only
  in their dedicated API audience; client-specific facades call the existing
  permission, workspace, billing and model-policy services without widening them.
- Keep safe client authorization/handoff pages isolated from ordinary webpage login
  and console initialization defaults. Retain Q10's independent display/metadata
  features and the unchanged ordinary Key management page.

## Design interview status

Q1–Q12 and the consolidated protocol, compatibility disposition and acceptance
boundary are confirmed. Implementation is authorized; individual source, native
and delivery checks below must still be completed and reported separately.

## Acceptance boundary

- Platform compatibility: original console Key crypto/lifecycle/policy, workspace
  access rules, billing behavior and ordinary webpage login/Key management retain
  baseline semantics. Existing independent Q10 features remain present. Gateway
  authentication has no refactor changes.
- Client lifecycle: same-scope repeated operations, CLI↔desktop, restart and A→B→A
  reuse; different scopes remain independent; cancellation/logout do not revoke.
- Error distinctions: delete/reset allow replacement; disable/expiry/permission,
  balance, quota, network and session failures never masquerade as missing Keys.
- Issuance consistency: concurrent callers, lost creation response, lost confirmation
  response, late generations, local-save failure, catalogue/configuration failure
  and interrupted predecessor retirement cannot produce unsafe reuse or silent
  repeated issuance. Verify tracked pending-row reuse independently of normal
  connect success.
- Local preservation: conflict handling, stale confirmations, shared session changes,
  secure-store lock/failure, and confirmed-copy recovery preserve unrelated files
  and credentials. Verify supported storage behavior on macOS, Windows and Linux.
- Delivery proof: automated unit/contract/UI checks are distinct from native store,
  installed test package, browser authorization-return and paid-model testing.
  No production deployment, destructive data migration or paid request is implied
  by passing source tests.

## Evidence and trade-off

[The integration audit](../client-key-management-audit.md) identifies pre-CLI
source baselines and client-induced changes. A shared local cache alone reduces
creation but cannot guarantee deduplication after a lost one-time response. Adding
client-specific orchestration is preferable to changing the shared platform's
create/reset semantics or returning old plaintext credentials. The remaining
implementation must preserve the confirmed failure-handling and compatibility
boundaries rather than reopening them implicitly in individual adapters.
