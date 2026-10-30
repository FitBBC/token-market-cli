# ADR 0001 — CLI Authorization Domain

> Client login, CLI and Key lifecycle rules are superseded by accepted ADR 0004. Historical behavior below is retained for context.

Date: 2026-10-27

## Status
Accepted

2026-10-30: the user has confirmed that the legacy CLI has no users and needs no
compatibility. Its authorization/exchange and bulk-disable flow will be removed
under [ADR 0004](0004-shared-client-key-lifecycle.md), currently awaiting final
design confirmation. This ADR remains a historical record, not the new refactor
specification.

## Context
Token Market needs a one-command setup flow for popular agent tools. The existing platform already has API key lifecycle management, login sessions, one-time OAuth state, and gateway support for chat, responses, and Anthropic messages. It does not yet have a CLI-specific authorization domain, PKCE, a local callback flow, source metadata on keys, or a dedicated authorization confirmation page.

## Decision
We will implement a dedicated CLI authorization domain:

- The CLI will create a Setup Session before opening the browser.
- The browser will show a consent page and issue a one-time code.
- The code will be exchanged using PKCE.
- The exchange will create a CLI API Key.
- The key will be bound to the user's default personal workspace.
- The key will not be restricted to a fixed model list.
- The CLI will record `source`, `tool`, and `setup_id` on the key.
- Revoke will map to disable, preserving audit history.
- Setup sessions and authorization results will be persisted for audit.
- Re-running setup for the same user, workspace, and tool will replace the previous active CLI key by disabling it.
- The first release will support the domestic environment only.
- The default model will be `glm-5.3`.
- The fast model will be `deepseek-v4.1-flash`.

## Consequences
- CLI-created keys remain auditable and revocable.
- Users do not need to choose a workspace in the first release.
- Model access remains broad because keys are unrestricted by model.
- The platform needs new endpoints for setup session creation and exchange.
- The frontend needs a dedicated `/cli/authorize` page.
- The CLI must detect each tool's real configuration directory rather than assuming a fixed path.
- Adapter implementations must back up every file they modify, including credential files.
- Tests should adapt to model capability rather than always assuming tool-call support.
