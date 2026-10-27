import http from 'node:http';
import { AddressInfo } from 'node:net';

export interface CallbackOptions {
  timeoutMs?: number;
}

export interface CallbackResult {
  code: string;
  state: string | null;
  port: number;
}

const DEFAULT_TIMEOUT_MS = 5 * 60 * 1000;

export async function waitForCallback(options: CallbackOptions = {}): Promise<CallbackResult> {
  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const server = http.createServer((request, response) => {
    if (!request.url || !request.url.startsWith('/callback')) {
      response.writeHead(404).end('Not found');
      return;
    }

    const parsed = new URL(request.url, 'http://127.0.0.1');
    const code = parsed.searchParams.get('code');
    const error = parsed.searchParams.get('error');
    const errorDescription = parsed.searchParams.get('error_description');

    if (error) {
      response.writeHead(200, { 'Content-Type': 'text/plain; charset=utf-8' }).end(
        `Authorization failed: ${errorDescription ?? error}. You can close this tab.`,
      );
      server.close();
      resolve(new Error(`authorization failed: ${errorDescription ?? error}`));
      return;
    }

    if (!code) {
      response.writeHead(400).end('Missing authorization code');
      return;
    }

    response.writeHead(200, { 'Content-Type': 'text/plain; charset=utf-8' }).end(
      'Authorization complete. Return to your terminal.',
    );
    server.close();
    resolve(undefined, {
      code,
      state: parsed.searchParams.get('state'),
      port: (server.address() as AddressInfo).port,
    });
  });

  let resolve: (error?: Error, value?: CallbackResult) => void;
  const settled = new Promise<CallbackResult>((resolveFunction, rejectFunction) => {
    resolve = (error?: Error, value?: CallbackResult) => {
      if (error) rejectFunction(error);
      else if (value) resolveFunction(value);
    };
  });

  const timer = setTimeout(() => {
    server.close();
    resolve(new Error('authorization timed out'));
  }, timeoutMs);
  timer.unref?.();

  await new Promise<void>((resolveFunction, rejectFunction) => {
    server.once('error', rejectFunction);
    server.listen(0, '127.0.0.1', () => {
      (settled as any).port = (server.address() as AddressInfo).port;
      resolveFunction();
    });
  });

  try {
    return await settled;
  } finally {
    clearTimeout(timer);
    server.close();
  }
}
