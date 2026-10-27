import http from 'node:http';
import { AddressInfo } from 'node:net';

export interface CallbackOptions {
  timeoutMs?: number;
}

export interface CallbackServer {
  port: number;
  waitForResult(): Promise<CallbackResult>;
  close(): void;
}

export interface CallbackResult {
  code: string;
  state: string | null;
  port: number;
}

const DEFAULT_TIMEOUT_MS = 5 * 60 * 1000;

/** Starts the loopback callback server and resolves the port immediately. */
export function startCallbackServer(options: CallbackOptions = {}): Promise<CallbackServer> {
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
      writeCallbackPage(response, false, `授权失败：${errorDescription ?? error}`);
      const callbackPort = (server.address() as AddressInfo).port;
      server.close();
      resolve(new Error(`authorization failed: ${errorDescription ?? error}`));
      return;
    }

    if (!code) {
      response.writeHead(400).end('Missing authorization code');
      return;
    }

    writeCallbackPage(response, true, '授权完成。');
    const callbackPort = (server.address() as AddressInfo).port;
    server.close();
    resolve(undefined, {
      code,
      state: parsed.searchParams.get('state'),
      port: callbackPort,
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

  return new Promise((resolveFunction, rejectFunction) => {
    server.once('error', rejectFunction);
    server.listen(0, '127.0.0.1', () => {
      resolveFunction({
        port: (server.address() as AddressInfo).port,
        waitForResult: () => settled,
        close: () => {
          clearTimeout(timer);
          server.close();
        },
      });
    });
  });
}

function writeCallbackPage(response: http.ServerResponse, success: boolean, message: string): void {
  const statusText = success ? 'Token Market CLI' : 'Token Market CLI — 授权失败';
  const statusClass = success ? 'is-success' : 'is-failed';
  const script = `
    (function closeTab() {
      function requestClose() {
        try { window.close(); } catch (_) {}
        try { window.open('', '_self').close(); } catch (_) {}
      }
      try {
        if (window.opener) window.opener.postMessage({ type: 'token-market-cli-callback', success: ${success} }, '*');
      } catch (_) {}
      requestClose();
      setTimeout(requestClose, 200);
      setTimeout(requestClose, 800);
    })();
  `.trim();

  response.writeHead(200, {
    'Content-Type': 'text/html; charset=utf-8',
    'Content-Security-Policy': "default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'",
  }).end(`<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>${escapeHtml(statusText)}</title>
  <style>
    :root { color-scheme: light; }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      min-height: 100vh;
      display: grid;
      place-items: center;
      padding: 24px;
      font: 14px/1.5 -apple-system, BlinkMacSystemFont, "Segoe UI", "PingFang SC", "Microsoft YaHei", sans-serif;
      color: #0f172a;
      background:
        radial-gradient(circle at 12% 8%, rgba(59,130,246,.16), transparent 34%),
        radial-gradient(circle at 88% 12%, rgba(16,185,129,.14), transparent 32%),
        #f4f6fb;
    }
    .card {
      width: min(420px, 100%);
      overflow: hidden;
      border: 1px solid #dbe1ea;
      border-radius: 18px;
      background: rgba(255,255,255,.92);
      box-shadow: 0 24px 80px rgba(15,23,42,.12);
      backdrop-filter: blur(18px);
    }
    .header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 18px 22px;
      border-bottom: 1px solid #e8edf4;
      background: linear-gradient(90deg, rgba(239,246,255,.95), rgba(255,255,255,0));
    }
    .brand { display: flex; align-items: center; gap: 12px; }
    .brand h1 { margin: 0; font-size: 17px; font-weight: 650; letter-spacing: -.01em; }
    .brand p { margin: 3px 0 0; color: #64748b; font-size: 13px; }
    .badge {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      width: 38px;
      height: 38px;
      border-radius: 12px;
    }
    .badge.is-success { color: #059669; background: #ecfdf5; border: 1px solid #a7f3d0; }
    .badge.is-failed { color: #dc2626; background: #fef2f2; border: 1px solid #fecaca; }
    .body { padding: 22px; }
    .status {
      display: flex;
      align-items: center;
      gap: 10px;
      padding: 14px 16px;
      border-radius: 12px;
      background: #f8fafc;
      border: 1px solid #e2e8f0;
      color: #334155;
      font-weight: 500;
    }
    .status strong { color: #0f172a; }
    .hint {
      margin: 16px 0 0;
      color: #64748b;
      font-size: 12px;
      line-height: 1.7;
    }
    .steps { margin: 14px 0 0; padding: 0; list-style: none; color: #64748b; font-size: 12px; }
    .steps li { display: flex; align-items: center; gap: 8px; margin-top: 8px; }
    .steps span {
      width: 16px;
      height: 16px;
      border-radius: 50%;
      background: #2563eb;
      color: white;
      display: inline-flex;
      align-items: center;
      justify-content: center;
      font-size: 10px;
    }
    @media (prefers-color-scheme: light) {
      .card { border-color: #dbe1ea; }
    }
  </style>
</head>
<body>
  <main class="card">
    <header class="header">
      <div class="brand">
        <span class="badge ${statusClass}" aria-hidden="true">${success ? '✓' : '!'}</span>
        <div>
          <h1>${escapeHtml(statusText)}</h1>
          <p>本机授权回调</p>
        </div>
      </div>
    </header>
    <section class="body">
      <div class="status">${success ? '<strong>授权完成。</strong>' : `<strong>${escapeHtml(message)}</strong>`}</div>
      ${success ? `<p class="hint">CLI 正在继续完成后续步骤：</p><ul class="steps"><li><span>1</span>创建 API Key</li><li><span>2</span>写入工具配置</li><li><span>3</span>发送测试请求</li></ul>` : ''}
      <p class="hint">如果页面没有自动关闭，你可以手动关闭这个标签页。</p>
    </section>
  </main>
  <script>${script}</script>
</body>
</html>
`);
}

function escapeHtml(value: string): string {
  return value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');
}

/** Compatibility helper for existing call sites that need a promise directly. */
export async function waitForCallback(options: CallbackOptions = {}): Promise<CallbackResult> {
  const server = await startCallbackServer(options);
  try {
    return await server.waitForResult();
  } finally {
    server.close();
  }
}
