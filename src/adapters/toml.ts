import { AdapterContext, ToolConfig, ToolId } from '../types.js';
import { BaseAdapter } from './base.js';
import { readFile, writeFile } from 'node:fs/promises';
import { ensureParentDirectory } from '../utils/fs.js';
import { parse, stringify } from 'smol-toml';

export class TomlAdapter extends BaseAdapter {
  constructor(
    id: ToolId,
    name: string,
    protocol: 'anthropic-compatible' | 'openai-compatible' | 'custom',
    executable: string,
    resolveConfigPath: () => string | Promise<string>,
    private readonly applyConfig: (current: any, config: ToolConfig, context: AdapterContext) => any,
    private readonly extractConfig: (value: any) => ToolConfig | null,
  ) {
    super(id, name, protocol, executable, resolveConfigPath, async (configPath, config, context) => {
      let current: any = {};
      try {
        current = parse(await readFile(configPath, 'utf8')) as any;
      } catch {
        current = {};
      }
      await ensureParentDirectory(configPath);
      await writeFile(configPath, `${stringify(this.applyConfig(current, config, context))}\n`, { mode: 0o600 });
    });
  }

  override async readConfig(): Promise<ToolConfig | null> {
    try {
      const value = parse(await readFile(await this.configPath(), 'utf8')) as any;
      return this.extractConfig(value);
    } catch {
      return null;
    }
  }
}
