import { ToolConfig } from '../types.js';

export const DEFAULT_CONTEXT_WINDOW_TOKENS = 200_000;

export async function resolveContextWindowTokens(config: ToolConfig): Promise<number> {
  try {
    const response = await fetch(new URL('/v1/models', config.baseUrl), {
      headers: { Authorization: `Bearer ${config.apiKey}` },
    });
    if (!response.ok) return DEFAULT_CONTEXT_WINDOW_TOKENS;

    const payload = await response.json() as {
      data?: Array<{
        id?: string;
        context_window_tokens?: number | string | null;
      }>;
    };
    const model = payload.data?.find((item) => item?.id === config.model);
    const value = Number(model?.context_window_tokens);
    if (!Number.isFinite(value) || value <= 0) {
      console.warn(`Model context window metadata is unavailable for ${config.model}; assuming ${DEFAULT_CONTEXT_WINDOW_TOKENS} tokens.`);
      return DEFAULT_CONTEXT_WINDOW_TOKENS;
    }
    return value;
  } catch {
    console.warn(`Could not read model metadata for ${config.model}; assuming ${DEFAULT_CONTEXT_WINDOW_TOKENS} tokens.`);
    return DEFAULT_CONTEXT_WINDOW_TOKENS;
  }
}
