# ADR 0002 — CLI Workspace Selection and OpenClaw Scope

Date: 2026-10-27

## Status
Accepted

The desktop application's workspace-selection decision is superseded by
[ADR 0003](0003-desktop-session-and-workspace-binding.md). The terminal CLI flow
and the OpenClaw/qclaw scope decisions are not changed by that desktop decision.

## Context
ADR 0001 chose to bind the CLI API key to the user's default personal workspace to simplify the first release. The platform already models workspaces as an authorization and billing boundary, and users may have access to more than one workspace. A key silently created in an inferred workspace could therefore charge the wrong account or expose the wrong model policy.

The original investigation also mentioned a possible `.qclaw` configuration path. OpenClaw and qclaw are separate products. Supporting qclaw would expand the product scope and require independently verified configuration behavior.

## Decision
- The CLI authorization page will let the logged-in user choose the workspace for the CLI API key.
- The CLI setup session will record the selected workspace.
- The API key will be created for exactly that workspace.
- The first release supports only the domestic environment.
- OpenClaw is supported.
- qclaw is not supported and will not be detected, configured, or documented by this CLI.
- OpenClaw configuration must use OpenClaw's own configuration discovery mechanism, with `OPENCLAW_CONFIG_PATH` and the official `~/.openclaw/openclaw.json` default path as the supported paths.

## Consequences
- Users can avoid creating a CLI key in the wrong workspace.
- Setup session persistence must include `workspace_id`.
- The authorization confirmation page must show the selected workspace before consent.
- API key metadata remains workspace-bound and auditable.
- No engineering time will be spent validating qclaw.
- Any qclaw installation encountered on a user's machine is ignored.
