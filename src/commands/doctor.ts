import { adapters, getAdapter } from '../adapters/registry.js';
import { findManagedKey } from '../config/managed-keys.js';
import { gatewayApiBaseUrl } from '../config/endpoints.js';

export async function doctor(toolName?: string): Promise<void> {
  const selected = toolName ? getAdapter(toolName as any).id : undefined;
  console.log(`Token Market gateway base URL: ${gatewayApiBaseUrl()}`);

  for (const adapter of adapters) {
    if (selected && adapter.id !== selected) continue;

    const key = await findManagedKey(adapter.id);
    const config = await adapter.readConfig();
    console.log(`\n${adapter.name}`);
    console.log(`- Local setup: ${key ? 'registered' : 'not registered'}`);
    console.log(`- Key id: ${key?.keyId ?? '-'}`);
    console.log(`- Config file: ${key?.configPath ?? (await adapter.configPath())}`);
    console.log(`- Config status: ${config ? 'valid Token Market entry' : 'missing Token Market entry'}`);
    if (config) {
      const network = await checkModels(config.baseUrl, config.apiKey, adapter.protocol);
      console.log(`- Model API: ${network.passed ? 'reachable' : `failed${network.detail ? ` (${network.detail})` : ''}`}`);
    }
  }
}

async function checkModels(baseUrl: string, apiKey: string, protocol: 'anthropic-compatible' | 'openai-compatible' | 'custom') {
  try {
    const response = await fetch(new URL('/v1/models', baseUrl), { headers: protocol === 'anthropic-compatible' ? { 'x-api-key': apiKey, 'anthropic-version': '2023-06-01' } : { Authorization: `Bearer ${apiKey}` } });
    return { passed: response.ok, detail: response.ok ? undefined : `HTTP ${response.status}` };
  } catch (error) {
    return { passed: false, detail: error instanceof Error ? error.message : 'network error' };
  }
}
