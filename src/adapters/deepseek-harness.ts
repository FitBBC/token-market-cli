import { AdapterContext, ToolConfig } from '../types.js';
import { BaseAdapter } from './base.js';
import { createBackup, ensureParentDirectory } from '../utils/fs.js';
import { readFile, writeFile } from 'node:fs/promises';
import { deepSeekHarnessCredentialsPath, deepSeekHarnessPatchPath } from './paths.js';
import { dump as yamlDump, load as yamlLoad } from 'js-yaml';

export class DeepSeekHarnessAdapter extends BaseAdapter {
  constructor() {
    super(
      'deepseek-harness',
      'DeepSeek Harness',
      'openai-compatible',
      'deepseek',
      deepSeekHarnessPatchPath,
      async (configPath, config, context) => {
        await writeCredential(config.apiKey);
        await writePatch(configPath, config, context);
      },
    );
  }

  override async writeConfig(config: ToolConfig, context: import('../types.js').AdapterContext): Promise<import('../types.js').AdapterWriteResult> {
    const credentialsBackup = await createBackup(deepSeekHarnessCredentialsPath());
    const result = await super.writeConfig(config, context);
    return { ...result, backupPath: result.backupPath ?? credentialsBackup};
  }

  override async readConfig(): Promise<ToolConfig | null> {
    try {
      const patch = (yamlLoad(await readFile(await this.configPath(), 'utf8')) as any[]) ?? [];
      const provider = patch.find((item) => item?.id === 'llm-pi-ai')?.config?.providers?.tokensmarket;
      if (!provider?.baseURL) return null;
      const credentials = (yamlLoad(await readFile(deepSeekHarnessCredentialsPath(), 'utf8')) as Record<string, string>) ?? {};
      return { baseUrl: provider.baseURL, apiKey: credentials.TOKEN_MARKET_API_KEY ?? '', model: provider.models?.[0]?.id ?? 'glm-5.2' };
    } catch {
      return null;
    }
  }
}

async function writeCredential(apiKey: string): Promise<void> {
  const file = deepSeekHarnessCredentialsPath();
  await ensureParentDirectory(file);
  await writeFile(file, yamlDump({ TOKEN_MARKET_API_KEY: apiKey }), { mode: 0o600 });
}

async function writePatch(configPath: string, config: ToolConfig, context: AdapterContext): Promise<void> {
  let current: any[] = [];
  try {
    current = (yamlLoad(await readFile(configPath, 'utf8')) as any[]) ?? [];
  } catch {
    current = [];
  }

  const next = current.filter((item) => item?.id !== 'llm-pi-ai');
  next.push({
    id: 'llm-pi-ai',
    config: {
      providers: {
        tokensmarket: {
          apiKeyEnv: 'TOKEN_MARKET_API_KEY',
          api: 'openai-completions',
          baseURL: `${config.baseUrl}/v1`,
          models: [{ id: 'glm-5.2' }, { id: 'deepseek-v4.1-flash' }],
        },
      },
    },
    tokensMarketSetup: { setupId: context.setupId, keyId: context.keyId },
  });

  await ensureParentDirectory(configPath);
  await writeFile(configPath, yamlDump(next), { mode: 0o600 });
}
