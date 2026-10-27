import { Adapter, AdapterContext, AdapterWriteResult, ToolConfig, ToolId } from '../types.js';
import { commandExists, createBackup } from '../utils/fs.js';

export abstract class BaseAdapter implements Adapter {
  constructor(
    readonly id: ToolId,
    readonly name: string,
    readonly protocol: 'anthropic-compatible' | 'openai-compatible' | 'custom',
    private readonly executable: string,
    private readonly resolveConfigPath: () => string | Promise<string>,
    private readonly write: (configPath: string, config: ToolConfig, context: AdapterContext) => Promise<void>,
  ) {}

  configPath(): string | Promise<string> {
    return this.resolveConfigPath();
  }

  async isInstalled(): Promise<boolean> {
    return commandExists(this.executable);
  }

  async writeConfig(config: ToolConfig, context: AdapterContext): Promise<AdapterWriteResult> {
    const configPath = await this.configPath();
    const backupPath = await createBackup(configPath);
    await this.write(configPath, config, context);
    return {
      configPath,
      backupPath,
      requiresRestart: this.id === 'codex' || this.id === 'deepseek-harness',
    };
  }

  async readConfig(): Promise<ToolConfig | null> {
    return null;
  }

  async verifyConfig(config: ToolConfig): Promise<void> {
    if (!config.baseUrl.startsWith('https://')) throw new Error('base URL must use HTTPS');
    if (!config.apiKey.startsWith('sk-')) throw new Error('invalid Token Market API key');
    if (!['glm-5.3', 'deepseek-v4.1-flash'].includes(config.model)) throw new Error('unsupported Token Market model');
  }
}
