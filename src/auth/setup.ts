import { ExchangeResult, SetupSession, ToolId } from '../types.js';
import { TokenMarketClient } from './api-client.js';
import { waitForCallback, CallbackResult } from './callback-server.js';
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
  const serverPromise = waitForCallback();
  const port = await getCallbackPort(serverPromise);
  const session = await client.createSetupSession({
    tool,
    redirectPort: port,
    codeChallenge: pkce.codeChallenge,
    codeChallengeMethod: pkce.codeChallengeMethod,
  });

  await open(session.authorizeUrl);
  console.log(`Open this URL if your browser did not start:\n${session.authorizeUrl}`);
  const callback = await serverPromise;
  const exchange = await client.exchange({
    setupId: session.setupId,
    code: callback.code,
    codeVerifier: pkce.codeVerifier,
  });

  return { ...exchange, setupId: session.setupId };
}

function getCallbackPort(waitPromise: Promise<CallbackResult>): Promise<number> {
  return new Promise((resolveFunction, rejectFunction) => {
    const started = Date.now();
    const poll = () => {
      const timer = setTimeout(() => {
        const candidate = (waitPromise as any).port;
        if (candidate) resolveFunction(candidate);
        else if (Date.now() - started > 3000) rejectFunction(new Error('callback server did not start'));
        else poll();
      }, 20);
      timer.unref?.();
    };
    poll();
  });
}
