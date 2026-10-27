import { AdapterContext, ToolConfig, ToolId } from '../types.js';
import { BaseAdapter } from './base.js';
import { readJsonFile, writeJsonFile } from '../utils/fs.js';

export class JsonAdapter extends BaseAdapter {
  constructor(
    id: ToolId,
    name: string,
    protocol: 'anthropic-compatible' | 'openai-compatible' | 'custom',
    executable: string,
    resolveConfigPath: () => string | Promise<string>,
    private readonly applyConfig: (current: any, config: ToolConfig, context: AdapterContext) => any,
    private readonly extractConfig: (value: any) => ToolConfig | null = () => null,
  ) {
    super(id, name, protocol, executable, resolveConfigPath, async (configPath, config, context) => {
      let current: any = {};
      try {
        current = await readJsonFile(configPath);
      } catch {
        current = {};
      }
      await writeJsonFile(configPath, this.applyConfig(current, config, context));
    });
  }

  override async readConfig(): Promise<ToolConfig | null> {
    try {
      return this.extractConfig(await readJsonFile<any>(await this.configPath()));
    } catch {
      return null;
    }
  }
}
