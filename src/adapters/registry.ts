import { Adapter, AdapterContext, ToolConfig, ToolId } from '../types.js';
import { JsonAdapter } from './json.js';
import { DeepSeekHarnessAdapter } from './deepseek-harness.js';
import { TomlAdapter } from './toml.js';
import {
  claudeSettingsPath,
  codexConfigPath,
  deepSeekHarnessPatchPath,
  openClawConfigPath,
  openCodeConfigPath,
  workBuddyConfigPath,
} from './paths.js';

const claudeCode = (current: any, config: ToolConfig, context: AdapterContext) => ({
  ...current,
  env: {
    ...current?.env,
    ANTHROPIC_BASE_URL: config.baseUrl,
    ANTHROPIC_API_KEY: config.apiKey,
  },
  tokensMarketSetup: { setupId: context.setupId, keyId: context.keyId, model: config.model },
});

const claudeExtract = (value: any): ToolConfig | null =>
  value?.env?.ANTHROPIC_BASE_URL && value?.env?.ANTHROPIC_API_KEY
    ? { baseUrl: value.env.ANTHROPIC_BASE_URL, apiKey: value.env.ANTHROPIC_API_KEY, model: value?.tokensMarketSetup?.model ?? 'glm-5.2' }
    : null;

const codex = (current: any, config: ToolConfig, context: AdapterContext) => ({
  ...current,
  model_provider: 'tokensmarket',
  model: config.model,
  model_providers: {
    ...current?.model_providers,
    tokensmarket: {
      name: 'Token Market',
      base_url: `${config.baseUrl}/v1`,
      experimental_bearer_token: config.apiKey,
      wire_api: 'responses',
    },
  },
  tokens_market_setup: { setup_id: context.setupId, key_id: context.keyId },
});

const codexExtract = (value: any): ToolConfig | null => {
  const provider = value?.model_providers?.tokensmarket;
  return provider?.base_url
    ? { baseUrl: provider.base_url, apiKey: value?.model_providers?.tokensmarket?.experimental_bearer_token ?? '', model: value?.model ?? 'glm-5.2' }
    : null;
};

const workBuddyEntryId = (model: string) => `tokensmarket-${model}`;

const workBuddy = (current: any, config: ToolConfig, context: AdapterContext) => {
  const existing = Array.isArray(current) ? current : [];
  const entry = {
    id: workBuddyEntryId(config.model),
    name: `Token Market ${config.model}`,
    model: config.model,
    vendor: 'Custom',
    url: `${config.baseUrl}/v1/chat/completions`,
    apiKey: config.apiKey,
    supportsToolCall: true,
    supportsImages: false,
    supportsReasoning: true,
    useCustomProtocol: true,
    tokensMarketSetup: { setupId: context.setupId, keyId: context.keyId },
  };
  return [...existing.filter((item) => !String(item?.id ?? '').startsWith('tokensmarket-')), entry];
};

const workBuddyExtract = (value: any): ToolConfig | null => {
  const entry = Array.isArray(value) ? value.find((item) => String(item?.id ?? '').startsWith('tokensmarket-')) : null;
  return entry?.url
    ? {
        baseUrl: new URL(entry.url).origin,
        apiKey: entry.apiKey,
        model: entry.model ?? String(entry.id ?? '').replace('tokensmarket-', ''),
      }
    : null;
};

const openClaw = (current: any, config: ToolConfig, context: AdapterContext) => ({
  ...current,
  models: {
    ...current?.models,
    mode: current?.models?.mode ?? 'merge',
    providers: {
      ...current?.models?.providers,
      tokensmarket: {
        baseUrl: `${config.baseUrl}/v1`,
        apiKey: config.apiKey,
        api: 'openai-completions',
        models: [
          { id: 'glm-5.2', name: 'GLM 5.2' },
          { id: 'deepseek-v4.1-flash', name: 'DeepSeek V4.1 Flash' },
        ],
      },
    },
  },
  tokensMarketSetup: { setupId: context.setupId, keyId: context.keyId },
});

const openClawExtract = (value: any): ToolConfig | null => {
  const provider = value?.models?.providers?.tokensmarket;
  return provider?.baseUrl && provider?.apiKey ? { baseUrl: provider.baseUrl, apiKey: provider.apiKey, model: 'glm-5.2' } : null;
};

const openCode = (current: any, config: ToolConfig, context: AdapterContext) => ({
  ...current,
  $schema: current?.$schema ?? 'https://opencode.ai/config.json',
  provider: {
    ...current?.provider,
    tokensmarket: {
      npm: '@ai-sdk/openai-compatible',
      name: 'Token Market',
      options: {
        baseURL: `${config.baseUrl}/v1`,
        apiKey: config.apiKey,
      },
      models: {
        'glm-5.2': { name: 'GLM 5.2' },
        'deepseek-v4.1-flash': { name: 'DeepSeek V4.1 Flash' },
      },
    },
  },
  tokensMarketSetup: { setupId: context.setupId, keyId: context.keyId },
});

const openCodeExtract = (value: any): ToolConfig | null => {
  const provider = value?.provider?.tokensmarket;
  return provider?.options?.baseURL && provider?.options?.apiKey
    ? { baseUrl: provider.options.baseURL, apiKey: provider.options.apiKey, model: 'glm-5.2' }
    : null;
};

export const adapters: Adapter[] = [
  new JsonAdapter('claude-code', 'Claude Code', 'anthropic-compatible', 'claude', claudeSettingsPath, claudeCode, claudeExtract),
  new TomlAdapter('codex', 'Codex CLI', 'openai-compatible', 'codex', codexConfigPath, codex, codexExtract),
  new JsonAdapter('workbuddy', 'WorkBuddy', 'openai-compatible', 'workbuddy', workBuddyConfigPath, workBuddy, workBuddyExtract),
  new DeepSeekHarnessAdapter(),
  new JsonAdapter('openclaw', 'OpenClaw', 'openai-compatible', 'openclaw', openClawConfigPath, openClaw, openClawExtract),
  new JsonAdapter('opencode', 'OpenCode', 'openai-compatible', 'opencode', openCodeConfigPath, openCode, openCodeExtract),
];

export function getAdapter(id: ToolId): Adapter {
  const adapter = adapters.find((candidate) => candidate.id === id);
  if (!adapter) throw new Error(`unsupported tool: ${id}`);
  return adapter;
}

export function toolId(value: string): ToolId {
  if (!adapters.some((adapter) => adapter.id === value)) throw new Error(`unsupported tool: ${value}`);
  return value as ToolId;
}

export function toolIds(): ToolId[] {
  return adapters.map((adapter) => adapter.id);
}
