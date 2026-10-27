import { getAdapter, toolId } from '../adapters/registry.js';
import { findManagedKey } from '../config/managed-keys.js';
import { verifyTokenMarket } from '../models/verification.js';
import { ToolConfig } from '../types.js';

export async function test(toolName: string, tests: ('models' | 'chat' | 'stream' | 'tool-call')[]): Promise<void> {
  const tool = toolId(toolName);
  const adapter = getAdapter(tool);
  const key = await findManagedKey(tool);
  if (!key) throw new Error(`No managed configuration found for ${adapter.name}. Run tokensmarket setup ${tool}.`);

  const config = await adapter.readConfig();
  if (!config) throw new Error(`Token Market configuration is missing in ${adapter.name}. Run tokensmarket setup ${tool}.`);
  const results = await verifyTokenMarket(config, adapter.protocol, tests);
  for (const result of results) console.log(`${result.passed ? '✓' : '✗'} ${result.name}${result.detail ? ` — ${result.detail}` : ''}`);
}
