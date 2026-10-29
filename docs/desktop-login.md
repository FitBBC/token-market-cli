# Desktop account login

The desktop application starts with account login. Terminal CLI setup continues
using its existing loopback authorization, tool/workspace consent and key flow.

Desktop commands call the public `tokensmarket_core::desktop::DesktopLogin`
application entry. `begin` creates S256 PKCE and a random state; `complete`
validates the fixed `tokenmarket://auth/callback` scheme, request/state and expiry
before exchanging a one-time code. `cancel` prevents a late exchange from becoming
a local login. The callback contains no Agent key or reusable session credential.
The server resolves the user and default workspace.

An unfinished request is saved in the application's local data directory so a
browser callback can launch a closed application. On Unix the directory is 0700
and the pending request file is 0600. The pending PKCE verifier is deleted on
consumption, expiry or cancellation. Reusable Desktop Sessions are saved through
`NativeDesktopCredentials`: macOS Keychain and Windows Credential Store using
`keyring`'s explicit `apple-native` / `windows-native` backends. There is no
plaintext or unsupported-platform fallback. Credentials are scoped to a SHA-256
identifier of the configured console API origin. Only Rust reads the credential;
status responses expose account/workspace/expiry, never the session token.

The desktop host wraps the native store in `CachedDesktopCredentials`. Each app
process reads private storage lazily once and keeps the result only in Rust
memory. Repeated unchanged writes are skipped; changed sessions are persisted
before the cache is updated. Logout always attempts native deletion and clears
the cache only after success. Failed reads/writes/deletions are not cached as
success and the next read consults native storage again. A new app process starts
with an empty cache and reads the platform store again. Server renewal still runs
on focus and at the existing interval; cached credentials do not bypass server
validation. No credential or display-cache file is introduced.

`DesktopLogin::with_credentials` supplies private storage to the same public
application entry. `restore` renews the saved valid session at
`POST /api/desktop/auth/renew` before returning authenticated context. Startup,
application focus, and a five-minute interval refresh the session; local expiry
also immediately hides authenticated status. Renewal preserves optional server
`web_origin` metadata. Invalid, expired, revoked, malformed, or unrenewable saved
sessions return to browser login and are removed from private storage. A private
storage failure fails closed and reports an error rather than falling back to a
file.

Renewal extends a still-valid dedicated desktop grant by 30 days. It keeps the
same token and does not touch console tokens, browser sessions, or Agent keys.
Redis compares the current grant atomically before extension, so revocation while
account/default-workspace lookup is running cannot recreate a deleted grant.
Expired grants require a new browser authorization.

`logout_desktop_session` removes the local desktop credential and attempts only
that desktop grant's logout route. If remote revocation fails, the UI explicitly
reports that it is unconfirmed while local logout remains complete. A private
storage deletion failure is reported as a failed logout. Switching account uses
this logout followed by browser login. Agent configs, legacy key records, backups,
and any recorded connection ownership are never read or rewritten by lifecycle
operations. Legacy records without ownership remain unknown; current login must
not be used to infer their owning account or Bound Workspace.

The registered protocol uses the Tauri deep-link plugin, `get_current` for cold
launch and `on_open_url` for a running process. The single-instance plugin forwards
Windows secondary launches and focuses the existing window. Protocol callbacks
and credentials must not be printed. See the authoritative platform contract in
`llm-platform/docs/page-schema.md`, “Desktop account authorization”.

## Verification boundaries

Public application tests use temporary directories and a fake HTTP platform.
They prove request serialization, cold request recovery, user/default workspace,
invalid/mismatched/repeated callbacks, denial, expiry, rejected codes, cancellation
and late exchange rejection. Existing terminal CLI contract/loopback tests remain.
Lifecycle tests supply fake private storage and HTTP, recover across independent
application instances, check renewal, expiry/revocation, local logout on server
failure, a second browser account login, and byte-for-byte preservation of retained
Agent configuration and connection metadata. They do not exercise the real OS
credential stores. No model generation is part of desktop login.

Native acceptance remains required on installed bundles. Record these separately
from the simulated tests and Rust compilation:

| Platform | Installed protocol | Cold callback | Running-app callback | Browser success stays on platform | Manual Open Application |
| --- | --- | --- | --- | --- | --- |
| macOS | Pending | Pending | Pending | Pending | Pending |
| Windows | Pending | Pending | Pending | Pending | Pending |

On macOS test an installed `.app` in `/Applications`; dynamic registration is not
supported. On Windows test the packaged installer registration and secondary
process forwarding. Use a real pending request with a fake/local platform for
smoke tests until target deployment is authorized. The browser success page uses
a child browsing context to request the protocol and retains a manual open link.

## Workspace/account controls

The signed-in header uses `desktop_workspace_context` and `switch_desktop_workspace`.
They call the public core `DesktopWorkspaceController` boundary, which serializes
shared preference saves and discards stale balance completions. Scoped HTTP uses
only Rust-held desktop Bearer plus `X-Workspace-Id`. Balance amount remains a
decimal string in display units, with authoritative server currency. The account
ID may be a parent of the selected L2; Current Workspace stays the requested ID.
No Agent configuration or saved Tool Connection is rewritten on switch.

`open_desktop_account_page({action})` accepts only profile/tokens/billing/workspaces
or recharge and opens the authenticated session's server-configured `web_origin`.
An explicit operator `TOKEN_MARKET_CONSOLE_WEB_BASE_URL` overrides this origin;
missing origin fails closed and asks the user to sign in again. Browser URLs
carry only workspace ID and allowed action; browser login and target permissions
are rechecked by `/desktop/console` or `/desktop/recharge` before opening existing
console/Billing UI. Returning focus refreshes account/balance without assuming
payment success. The website route needs deployment alongside these clients.

2026-10-28 verification: core fake-HTTP/temp-file tests cover two spaces, shared
preference save, decimal precision, L2 parent account and stale balance; browser
fixture inspected avatar/nested menu, switching and balance labels. This is
simulated behavior, not macOS/Windows opener, callback, real account/balance,
recharge or target deployment proof. No payment/model request was submitted.
## Native credential persistence acceptance

The macOS native target compiles the Keychain adapter. The following runtime
checks remain separate from simulated behavior and compilation:

| Platform | Save/read/delete in OS private store | App restart | Computer restart | Locked/denied store | Offline logout and account change |
| --- | --- | --- | --- | --- | --- |
| macOS | Isolated fixture passed (2026-10-28) | Pending | Pending | Pending | Pending |
| Windows | See hosted native-validation opt-in smoke for the checked revision | Pending | Pending | Pending | Pending |

Ordinary automated suites keep `tests/native_credentials.rs` ignored and do not
write OS credentials. To opt in on macOS or Windows, run:

```sh
cargo test --manifest-path rust/tokensmarket-core/Cargo.toml --test native_credentials --locked -- --ignored --nocapture
```

This smoke uses the production private-store adapter with a random `.invalid`
platform namespace and a fake value, reads it from a separate process, then deletes
only its matching fixture. It performs no network/account operations and changes
no OS-store settings. Denied or unavailable storage fails the test; it does not
unlock the store or fall back to plaintext. Hosted Windows native-validation CI
explicitly opts into this test. Separate-process storage is only part of native
acceptance: use an installed signed bundle and a fake/local authorized platform
for app/computer restart, locked-store and login/logout checks. Do not substitute
this smoke or a successful compile for those remaining checks.
The keyring backend choice follows [keyring 3.6.3 documentation](https://docs.rs/keyring/3.6.3/keyring/).

## Signing and cross-platform storage behavior

| Platform | Persistent login credential | In-process optimization |
| --- | --- | --- |
| macOS | Keychain (`apple-native`) | Shared Rust credential cache |
| Windows | Credential Manager (`windows-native`) | Shared Rust credential cache |

App display state remains in memory and is reloaded from the authenticated
server. Test and production credentials remain separated by the configured
console API identity. The cache never shares values between store instances.

The cache reduces repeated OS access within one run; it does not remove OS
permission prompts at startup or after replacing an app. macOS ad hoc builds
have version-specific code identity. Distribution must use a stable bundle ID
and Developer ID signing to support consistent keychain access across updates.
For the Tauri test-build script, supply `APPLE_SIGNING_IDENTITY` when a valid
signing certificate is available; no signing identity is created or installed by
the script. Windows persistence and upgrade behavior still require validation
on an installed Windows build; macOS simulation cannot establish that behavior.
