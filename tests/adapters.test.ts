import { mkdtempSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import YAML from 'js-yaml';
import { afterAll, beforeAll, expect, test } from 'vitest';
import { adapters, getAdapter } from '../src/adapters/registry.js';

let originalHome: string | undefined;

beforeAll(() => {
  originalHome = process.env.HOME;
  process.env.HOME = mkdtempSync(path.join(os.tmpdir(), 'tokensmarket-adapters-'));
  delete process.env.CLAUDE_CONFIG_DIR;
  delete process.env.CODEX_HOME;
  delete process.env.WORKBUDDY_CONFIG_PATH;
  delete process.env.DSH_HOME;
  delete process.env.OPENCLAW_CONFIG_PATH;
  delete process.env.OPENCODE_CONFIG;
});

afterAll(() => {
  if (originalHome) process.env.HOME = originalHome;
});

const config = { baseUrl: 'https://api.tokensapi.cn', apiKey: 'tm-test-key', model: 'glm-5.3' };
const context = { setupId: 'setup_123', keyId: 'key_123' };

test('writes Claude Code settings', async () => {
  const adapter = getAdapter('claude-code');
  const result = await adapter.writeConfig(config, context);
  const value = JSON.parse(await readFile(result.configPath, 'utf8'));
  expect(value.env.ANTHROPIC_BASE_URL).toBe('https://api.tokensapi.cn');
  expect(value.env.ANTHROPIC_AUTH_TOKEN).toBe('tm-test-key');
});

test('writes Codex TOML provider', async () => {
  const adapter = getAdapter('codex');
  const result = await adapter.writeConfig(config, context);
  const text = await readFile(result.configPath, 'utf8');
  expect(text).toContain('model_provider = "tokensmarket"');
  expect(text).toContain('[model_providers.tokensmarket]');
  expect(text).toContain('base_url = "https://api.tokensapi.cn/v1"');
  expect(text).toContain('wire_api = "responses"');
  expect(text).toContain('experimental_bearer_token = "tm-test-key"');
});

test('writes WorkBuddy custom model', async () => {
  const adapter = getAdapter('workbuddy');
  const result = await adapter.writeConfig(config, context);
  const value = JSON.parse(await readFile(result.configPath, 'utf8'));
  expect(value).toHaveLength(1);
  expect(value[0]).toMatchObject({
    id: 'tokensmarket-glm-5.3',
    model: 'glm-5.3',
    vendor: 'Custom',
    url: 'https://api.tokensapi.cn/v1/chat/completions',
    apiKey: 'tm-test-key',
    useCustomProtocol: true,
  });
});

test('writes DeepSeek Harness YAML patch and credentials', async () => {
  const adapter = getAdapter('deepseek-harness');
  const result = await adapter.writeConfig(config, context);
  const patch = YAML.load(await readFile(result.configPath, 'utf8')) as any[];
  const provider = patch[0].config.providers.tokensmarket;
  expect(provider.apiKeyEnv).toBe('TOKEN_MARKET_API_KEY');
  expect(provider.api).toBe('openai-completions');
  expect(provider.baseURL).toBe('https://api.tokensapi.cn/v1');
  const credentials = YAML.load(await readFile(path.join(process.env.HOME!, '.dsh/.credentials.yaml'), 'utf8')) as any;
  expect(credentials.TOKEN_MARKET_API_KEY).toBe('tm-test-key');
});

test('writes OpenClaw provider schema', async () => {
  const adapter = getAdapter('openclaw');
  const result = await adapter.writeConfig(config, context);
  const value = JSON.parse(await readFile(result.configPath, 'utf8'));
  const provider = value.models.providers.tokensmarket;
  expect(provider.baseUrl).toBe('https://api.tokensapi.cn/v1');
  expect(provider.api).toBe('openai-completions');
  expect(provider.models.map((model: any) => model.id)).toEqual(['glm-5.3', 'deepseek-v4.1-flash']);
});

test('writes OpenCode provider schema', async () => {
  const adapter = getAdapter('opencode');
  const result = await adapter.writeConfig(config, context);
  const value = JSON.parse(await readFile(result.configPath, 'utf8'));
  const provider = value.provider.tokensmarket;
  expect(provider.npm).toBe('@ai-sdk/openai-compatible');
  expect(provider.options.baseURL).toBe('https://api.tokensapi.cn/v1');
  expect(Object.keys(provider.models)).toEqual(['glm-5.3', 'deepseek-v4.1-flash']);
});

test('registers six supported adapters', () => {
  expect(adapters).toHaveLength(6);
});
