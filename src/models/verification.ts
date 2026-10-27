import { ToolConfig, VerificationResult } from '../types.js';

export type VerificationTest = 'models' | 'chat' | 'stream' | 'tool-call';

export async function verifyTokenMarket(config: ToolConfig, protocol: 'anthropic-compatible' | 'openai-compatible' | 'custom', tests: VerificationTest[]): Promise<VerificationResult[]> {
  const results: VerificationResult[] = [];

  if (tests.includes('models')) results.push(await checkModels(config, protocol));
  if (tests.includes('chat')) results.push(await checkChat(config, protocol, false));
  if (tests.includes('stream')) results.push(await checkChat(config, protocol, true));
  if (tests.includes('tool-call')) results.push(await checkToolCall(config, protocol));

  return results;
}

async function checkModels(config: ToolConfig, protocol: 'anthropic-compatible' | 'openai-compatible' | 'custom'): Promise<VerificationResult> {
  const response = await fetch(new URL('/v1/models', config.baseUrl), { headers: authHeaders(config, protocol) });
  return result('Model list', response.ok, response.ok ? undefined : `HTTP ${response.status}`);
}

async function checkChat(config: ToolConfig, protocol: 'anthropic-compatible' | 'openai-compatible' | 'custom', stream: boolean): Promise<VerificationResult> {
  const body =
    protocol === 'anthropic-compatible'
      ? {
          model: config.model,
          max_tokens: 8,
          stream,
          messages: [{ role: 'user', content: 'Reply with OK only.' }],
        }
      : {
          model: config.model,
          max_tokens: 8,
          messages: [{ role: 'user', content: 'Reply with OK only.' }],
          stream,
        };

  const response = await fetch(chatUrl(config, protocol), {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', ...authHeaders(config, protocol) },
    body: JSON.stringify(body),
  });

  if (!response.ok) return result(stream ? 'Streaming chat' : 'Chat request', false, `HTTP ${response.status}`);
  if (!stream) return result(stream ? 'Streaming chat' : 'Chat request', true);

  const contentType = response.headers.get('content-type') ?? '';
  if (!contentType.includes('text/event-stream')) {
    return result('Streaming chat', false, `expected text/event-stream but received ${contentType || 'unknown content-type'}`);
  }

  const text = await response.text();
  const passed = /^data:/m.test(text) && !text.includes('"error"');
  return result('Streaming chat', passed, passed ? undefined : 'response was not a valid SSE stream');
}

async function checkToolCall(config: ToolConfig, protocol: 'anthropic-compatible' | 'openai-compatible' | 'custom'): Promise<VerificationResult> {
  const body =
    protocol === 'anthropic-compatible'
      ? {
          model: config.model,
          max_tokens: 128,
          messages: [{ role: 'user', content: 'What is the weather in Shanghai?' }],
          tools: [{ name: 'get_weather', description: 'Get current weather', input_schema: { type: 'object', properties: { city: { type: 'string' } }, required: ['city'] } }],
        }
      : {
          model: config.model,
          messages: [{ role: 'user', content: 'What is the weather in Shanghai?' }],
          tools: [{ type: 'function', function: { name: 'get_weather', description: 'Get current weather', parameters: { type: 'object', properties: { city: { type: 'string' } }, required: ['city'] } } }],
          max_tokens: 128,
        };

  const response = await fetch(chatUrl(config, protocol), {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', ...authHeaders(config, protocol) },
    body: JSON.stringify(body),
  });
  return result('Tool call', response.ok, response.ok ? undefined : `HTTP ${response.status}`);
}

function chatUrl(config: ToolConfig, protocol: 'anthropic-compatible' | 'openai-compatible' | 'custom'): URL {
  return new URL(protocol === 'anthropic-compatible' ? '/v1/messages' : '/v1/chat/completions', config.baseUrl);
}

function authHeaders(config: ToolConfig, protocol: 'anthropic-compatible' | 'openai-compatible' | 'custom'): Record<string, string> {
  return protocol === 'anthropic-compatible'
    ? { 'x-api-key': config.apiKey, 'anthropic-version': '2023-06-01' }
    : { Authorization: `Bearer ${config.apiKey}` };
}

function result(name: string, passed: boolean, detail?: string): VerificationResult {
  return { name, passed, detail };
}
