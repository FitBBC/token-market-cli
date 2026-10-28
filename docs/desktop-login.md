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

`DesktopLogin::with_credentials` supplies that private storage to the same public
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

## Native credential persistence acceptance

The macOS native target compiles the Keychain adapter. The following runtime
checks remain separate from simulated behavior and compilation:

| Platform | Save/read/delete in OS private store | App restart | Computer restart | Locked/denied store | Offline logout and account change |
| --- | --- | --- | --- | --- | --- |
| macOS | Pending | Pending | Pending | Pending | Pending |
| Windows | Pending | Pending | Pending | Pending | Pending |

No native keychain/credential entry was created or deleted during automated
validation. Use an installed signed bundle and a fake/local authorized platform
for these checks; do not substitute a successful compile for persistence proof.
The keyring backend choice follows [keyring 3.6.3 documentation](https://docs.rs/keyring/3.6.3/keyring/).
