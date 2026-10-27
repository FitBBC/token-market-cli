import { ExchangeResult, SetupSession, ToolId } from '../types.js';
import { TokenMarketClient } from './api-client.js';
import { startCallbackServer, CallbackResult } from './callback-server.js';
import { createPkce } from './pkce.js';
import { open } from '../utils/open-browser.js';

export interface AuthorizationResult extends ExchangeResult {
  setupId: string;
}

export type SetupClient = Pick<TokenMarketClient, 'createSetupSession' | 'exchange'>;

export async function authorizeAndExchange(
  tool: ToolId,
  client: SetupClient = new TokenMarketClient(),
): Promise<AuthorizationResult> {
  const pkce = createPkce();
  const server = await startCallbackServer();
  try {
    const session = await client.createSetupSession({
      tool,
      redirectPort: server.port,
      codeChallenge: pkce.codeChallenge,
      codeChallengeMethod: pkce.codeChallengeMethod,
    });

    await open(session.authorizeUrl);
    console.log(`Open this URL if your browser did not start:\n${session.authorizeUrl}`);
    const callback = await server.waitForResult();
    const exchange = await client.exchange({
      setupId: session.setupId,
      code: callback.code,
      codeVerifier: pkce.codeVerifier,
    });

    return { ...exchange, setupId: session.setupId };
  } finally {
    server.close();
  }
}
