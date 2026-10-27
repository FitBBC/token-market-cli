import { SetupSession, ExchangeResult, ToolId } from '../types.js';
import { consoleApiBaseUrl } from '../config/endpoints.js';

export class TokenMarketApiError extends Error {
  constructor(
    message: string,
    readonly status: number | undefined,
    readonly code: string | undefined,
  ) {
    super(message);
    this.name = 'TokenMarketApiError';
  }
}

export class TokenMarketClient {
  constructor(private readonly baseUrl: string = consoleApiBaseUrl()) {}

  async createSetupSession(input: {
    tool: ToolId;
    redirectPort: number;
    codeChallenge: string;
    codeChallengeMethod: 'S256';
  }): Promise<SetupSession> {
    return this.request('/api/cli/sessions', input);
  }

  async exchange(input: {
    setupId: string;
    code: string;
    codeVerifier: string;
  }): Promise<ExchangeResult> {
    return this.request('/api/cli/exchange', input);
  }

  private async request<T>(path: string, body: unknown): Promise<T> {
    const response = await fetch(new URL(path, this.baseUrl), {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });

    const text = await response.text();
    const data = safelyParseJson(text);

    if (!response.ok) {
      throw new TokenMarketApiError(
        (data?.error_description ?? data?.message ?? data?.error) || `Token Market request failed (${response.status})`,
        response.status,
        data?.error ?? data?.code,
      );
    }

    return data as T;
  }
}

function safelyParseJson(text: string): any {
  try {
    return text ? JSON.parse(text) : undefined;
  } catch {
    return undefined;
  }
}

export function defaultBaseUrl(): string {
  return consoleApiBaseUrl();
}
