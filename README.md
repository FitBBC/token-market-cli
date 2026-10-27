# Token Market Agent CLI

Configure Token Market in popular agent tools with one command.

## Commands

```bash
npx tokensmarket setup
npx tokensmarket setup claude-code
npx tokensmarket doctor
npx tokensmarket test
npx tokensmarket keys
```

## Supported tools

- Claude Code
- Codex CLI
- WorkBuddy
- DeepSeek Harness
- OpenClaw
- OpenCode

## Platform contract

The authorization API defaults to `https://test.tokensapi.cn/console` (`TOKEN_MARKET_CONSOLE_API_BASE_URL`); gateway requests default to `https://api-test.tokensapi.cn` (`TOKEN_MARKET_GATEWAY_API_BASE_URL`).

The CLI expects these platform endpoints:

- `POST /api/cli/sessions`
- `POST /api/cli/exchange`
- OpenAI-compatible model/chat endpoints under `/v1`

Authorization uses PKCE and a one-time local callback at `http://127.0.0.1`.

## Development

```bash
npm install
npm run typecheck
npm test
npm run build
npm run dev -- setup claude-code
```
