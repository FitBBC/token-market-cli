# Desktop login and tool connections have separate lifecycles

Date: 2026-10-28

## Status

Accepted. The design interview is complete; this document records intended
behavior and acceptance criteria, not a completed implementation.

## Decision

The desktop application authorizes the user once, enters the platform's default
workspace without a workspace selector in the browser, and reuses a Desktop
Session for subsequent operations. The browser authorization success page stays
on the platform domain and offers to open the application through its registered
application protocol. A one-time Authorization Code is exchanged for the desktop
login; it is not reused as a long-lived credential.

Tool Connection happens separately, when the user clicks Connect on an installed
Agent. The application then requests a key using the Desktop Session, the Current
Workspace, and the target Tool. Each connected Tool displays its Bound Workspace.
This replaces applying ADR 0002's workspace-selecting consent flow to desktop
login: users manage several tools after login instead of authorizing each setup
in a browser.

The desktop application shows the Current Workspace's balance, opens that
workspace's recharge page in a browser, and provides an avatar menu consistent
with the authenticated website, including workspace switching. Switching the
Current Workspace refreshes the balance and changes the workspace used for
subsequent connections. Existing connections retain their Bound Workspace and
key; testing an existing connection uses that connection's credential.

Desktop workspace switching reuses the platform's existing workspace-switch
operation and updates the shared browser/server Default Workspace preference.
This preference does not determine where already issued model keys are billed;
their Bound Workspace remains unchanged. A separate desktop-only workspace
preference service is unnecessary.

Each device and tool has an independent Agent API Key. Establishing or replacing
a connection on one computer must not revoke another computer's key. The Desktop
Session persists across application and computer restarts while valid, supports
renewal, and requires browser login again if it is revoked or cannot be renewed.
Logging out ends the Desktop Session while preserving existing tool connections;
connections retained across an account change display their owning account and
workspace.

## Confirmed product constraints

- The login presentation and browser-to-application handoff follow the supplied
  DeepSeek screenshots; the connected-tool list and action presentation follow
  the supplied Teamo screenshots.
- Each tool offers connection, disconnection, and testing actions appropriate to
  its state. Tools not installed locally show a download action and cannot be
  connected.
- Claude Code and Codex use `glm-5.3` (test build: `glm-5.2`) under the requested product policy and show
  that model on their cards.
- Other tools obtain their model catalogue from `/v1/models` using the
  connection's actual key and configure every returned model whose `model_types`
  contains `TEXT_GENERATION`. Model type identifies eligibility for configuration;
  actual protocol behavior is verified separately through an explicit test.
- Multi-model configuration includes the complete compatible catalogue without
  a preferred model or a GLM-first policy. The desktop connection flow does not
  introduce a default-model selection step. If no compatible models are returned,
  connection stops with an explicit explanation. Claude Code and Codex stop if
  their required `glm-5.3` model is unavailable rather than silently using another.
- Multi-model tools load the catalogue at connection time and subsequently update
  it through Reconnect using the currently logged-in account and Current
  Workspace. Catalogue refresh is part of Reconnect rather than an independent
  action against the old key. Existing local configurations are preserved when
  obtaining the replacement key or catalogue fails.
- Connect establishes configuration separately from a real model test. The UI
  distinguishes configured-but-untested, test passed, and test failed. The user
  explicitly starts tests with notice that a small usage charge may result;
  tests use the protocol actually configured for that tool.
- A single-model connection tests `glm-5.3`. For a multi-model connection the
  user selects one of its configured models for that test; the application does
  not automatically test every model or write that test selection into the
  Agent's configuration. Tests use the existing connection's key and Bound
  Workspace even after the desktop account or Current Workspace changes.
- Existing configurations are backed up and only the required fields are
  changed. Subsequent manual changes are preserved; conflicts require the user's
  choice rather than replacement of an entire file with an old backup.
- Disconnect restores the local configuration taken over by that connection and
  preserves later manual changes under the conflict policy above. After local
  restoration succeeds, it may attempt to disable that exact server key once;
  failure does not block local completion and creates no pending-revocation state
  or retry. It must not claim that the server key was disabled without success.
- Reconnect uses the currently logged-in account and Current Workspace. If they
  differ from the connection's owner or Bound Workspace, the UI explicitly
  presents the account/workspace change before the action. Merely selecting
  another account or workspace leaves existing local configurations intact.
- After an account change, an old connection can still be disconnected locally
  and tested with its actual configured key. Its catalogue is not refreshed
  through the old account; the user instead explicitly reconnects using the
  current account and workspace.
- Replacement keeps the previous key and configuration usable until the new
  configuration is successfully written. Failure restores the previous
  configuration and cleans up any newly issued key that was not put into use;
  successful replacement disables the previous key. Real model testing remains
  a separate user action.
- During cross-account Reconnect, if the current account lacks permission to
  disable the old key, the application may still complete the new connection.
  It attempts to disable the old key once and explicitly reports that the old
  key was not disabled if that attempt fails. The user need not sign back into
  the old account to complete the new connection. This exception does not
  change the normal requirement to disable the old key when authorized.
- This release manages one active configuration for each tool on a device;
  parallel management of multiple installations or profiles is outside its scope.
- The current release scope remains Claude Code, Codex CLI, WorkBuddy, DeepSeek
  Harness, OpenClaw, and OpenCode on macOS and Windows. The screenshots do not add
  Claude Desktop, Codex Desktop, or Kimi Code to the supported product set.

## Consequences

Desktop login and each tool's model credential need distinct ownership and
lifecycle records. The Current Workspace cannot substitute for the Bound
Workspace when displaying or operating on an existing connection. This avoids
silently changing where an already configured tool's usage is charged.

The existing terminal CLI flow remains unchanged in this release.

## Acceptance checks

- A browser login returns to the application while its success page remains on
  the platform domain; a valid Desktop Session survives an application restart.
- Switching workspace updates the shared platform preference and displayed
  balance while existing Agent configurations retain their Bound Workspace.
- A tool card reports actual installation and connection state, owning account,
  Bound Workspace, and configured models; an uninstalled tool offers download
  and cannot be connected.
- Each supported tool can be connected on macOS and Windows without manual key
  entry, and another computer's connection remains usable.
- Multi-model configuration includes all key-visible models advertising
  `OPENAI_CHAT_COMPLETIONS` without a preferred model or default-selection step.
- Cancelling a connection preserves unrelated configuration changes and
  completes locally even if the one server-disable attempt fails.
- Reconnecting visibly applies the current account/workspace, loads a fresh
  catalogue, and preserves the previous connection if replacement configuration
  cannot be written. Normal replacement disables only that connection's old key.
- Cross-account replacement without permission to disable the old key reports
  the retained key while leaving the newly established connection usable.
- A user-triggered test exercises the configured protocol and chosen model,
  reports the result independently of configuration status, and leaves the
  configured model selection unchanged.
- Logging out preserves Agent configurations; a subsequently logged-in account
  can cancel or test those connections and explicitly reconnect under its own
  workspace. The terminal CLI retains its existing workflow.
