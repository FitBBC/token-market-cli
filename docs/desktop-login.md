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
consumption, expiry or cancellation. Reusable Desktop Session persistence and
restoration are delivered separately; this change retains the established
credential only in Rust memory and exposes only account/workspace context to UI.

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
No Agent configuration or model generation is part of desktop login.

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
