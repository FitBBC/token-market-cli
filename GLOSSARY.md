# Glossary

## Setup Session
A short-lived, single-use authorization object created by the CLI before the user opens the browser. It binds the target tool, local callback port, PKCE challenge, and later the issued code to one setup attempt.

## PKCE Code
The one-time authorization code exchanged for an API key after the user approves the setup request. It is bound to the Setup Session and verified with PKCE.

## CLI API Key
An API key created specifically by the CLI authorization flow. It belongs to the user's default personal workspace, is permanent until revoked, and is tagged with its source, target tool, and setup session.

## Tool Adapter
The CLI component responsible for detecting a target agent tool, locating its real configuration path, backing up the existing configuration, writing the Token Market provider entry, and verifying the result.

## Authorization Workspace
The workspace selected by the logged-in user on the CLI authorization page. The CLI setup flow creates the API key for this workspace and never infers it silently.

## Revoke
The user-facing action that disables a CLI API key while preserving its audit record. It maps to the platform's disable operation, not delete.

## Source
The origin recorded on an API key, such as `cli`. It distinguishes CLI-created keys from keys created in the console.

## Tool
The target agent product configured by the CLI, such as Claude Code, Codex CLI, WorkBuddy, DeepSeek Harness, OpenClaw, or OpenCode.

## Model Preset
A named model choice offered by the CLI, such as recommended or fast. The first release uses `glm-5.3` and `deepseek-v4.1-flash`.

## Adapter Directory Detection
The process of resolving a target tool's actual configuration location from its own environment variables or known default paths before writing any file.
