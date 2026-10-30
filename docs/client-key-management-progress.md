# Shared client Key refactor execution record

2026-10-30: ADR 0004 accepted by the user with explicit consolidated confirmation.
No commits, pushes, production changes or real credential deletion are authorized
as a separate delivery action by this execution record.

## Checkouts

- CLI/desktop: /Users/bingbing.chen/mying/token-market-cli,
  codex/reuse-desktop-tool-keys, HEAD 8a43c8d plus pre-existing conversation changes.
- Platform isolated worktree: /private/tmp/tokenmarket-key-refactor-platform,
  codex/shared-client-key-management, based on origin/bbc 9bd6a26c.
- Website isolated worktree: /private/tmp/tokenmarket-key-refactor-web,
  temp/shared-client-key-management, based on origin/bbc 1d695e9e.
- Original platform/website bbc checkouts are preserved, no application edits there.
- Gateway: no code changes required; preserve existing model_types feature.

## Implementation

- [x] Platform: transactional client-only scope/issuance persistence, authoritative
  hash/ownership/status validation, pending rotation, durable receipt confirmation,
  applied/finalization, exact confirmed-copy-loss predecessor retirement.
- [x] Platform: client-only workspace/balance facades and auth audience; restore
  shared workspace guard; remove old CLI setup/exchange/bulk-disable exclusively.
- [x] Shared Rust: credential issuer/recovery, safe persistent plans, client session
  refresh across CLI/desktop, Linux secret storage and passphrase fallback.
- [x] CLI: shared login/workspace/connect/reconnect/disconnect/status/test commands;
  remove old setup exchange/history/presets/automatic paid tests.
- [x] Desktop: shared kernel and issuer; remove Clear Connection; lost-copy-confirm
  recovery; preserve existing secure store, tool configuration and restoration.
- [x] Website: remove old CLI authorization/client API; remote-browser client login;
  preserve default website behavior and independent accepted enhancements.
- [x] Update knowledge base, domain documents and usage guides to final behavior.
- [x] Run focused then required automated checks; build and inspect test package.

## Design invariants

Source of truth: ADR 0004 and client-key-management-audit.md. Do not change original
platform Key crypto/verify/lifecycle, permission or billing semantics. Use exact
environment/account/workspace/device/tool scopes; treat disabled/expired and
uncertain failures differently from deletion/reset. A saved Key outlives catalogue
and configuration failures. No server plaintext recovery. Never silently interpret
a locked/corrupt store as an empty cache. Do not migrate old CLI history or
proactively remove actual local data.


## Delivery, 2026-10-30

Source changes and test packages are complete. See
[verification record](client-key-management-verification.md) for actual results,
baseline failures, artifacts and deployment order. The test App and DMG were
built and statically verified; neither installed/native/browser-return acceptance
nor a database migration, deployment or paid-model request is claimed.

Original platform/web worktrees remain clean. New APIs/coordination data are
isolated; existing public Key generation/reset/verification and gateway code are
unchanged. Historical database metadata is retained, not destructively migrated.


## BBC integration and push, 2026-10-30

Explicitly requested by the user. Both remote refs were verified after normal,
non-force pushes:

- Platform bbc: aad06b1c6bdf66de3e83b6863954a70886fbbafb;
  feature commit 0a3661cd. Current source: /Users/bingbing.chen/mying/llm-platform.
- Web bbc: a563e49bf12694bc80115f123abd460cf90a0266;
  feature commit 9161181a. Current source: /Users/bingbing.chen/mying/llm-platform-web.

No conflicts. Post-merge validation passed: platform core 32 + webapi 39 related
regressions; Web 2045 tests plus lint and KB checks. Both bbc working trees are
clean. Task-owned temporary worktrees and merged branches were removed; the
previous paths above describe the build-time snapshot. The shared dependency
directory was preserved; only its task-created symlink was removed.

The CLI/desktop repository remains uncommitted. SQL migration was not executed;
no manual deployment or paid-model request was performed by this delivery.
