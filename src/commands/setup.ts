import { select } from '@clack/prompts';
import { authorizeAndExchange } from '../auth/setup.js';
import { getAdapter, toolId, toolIds } from '../adapters/registry.js';
import { TokenMarketClient } from '../auth/api-client.js';
import { gatewayApiBaseUrl } from '../config/endpoints.js';
import { ToolId } from '../types.js';
import { resolveModel } from '../models/model-presets.js';
import { resolveContextWindowTokens } from '../models/model-metadata.js';
import { saveManagedKey } from '../config/managed-keys.js';
import { verifyTokenMarket } from '../models/verification.js';

export async function setup(
  inputTool: string | undefined,
  modelPreset: string | undefined,
  client: Pick<TokenMarketClient, 'createSetupSession' | 'exchange'> = new TokenMarketClient(),
  adapterPicker: (tool: ToolId) => Awaited<ReturnType<typeof getAdapter>> = getAdapter,
  verifier: typeof verifyTokenMarket = verifyTokenMarket,
): Promise<void> {
  const tool = inputTool ? toolId(inputTool) : await chooseTool();
  const adapter = getAdapter(tool);
  const installed = await adapter.isInstalled();
  if (!installed) {
    throw new Error(`${adapter.name} is not installed or not available on PATH.`);
  }
  const apiKeyResult = await authorizeAndExchange(tool, client);
  const model = resolveModel(modelPreset);
  const contextWindowTokens = await resolveContextWindowTokens({
    baseUrl: gatewayApiBaseUrl(),
    apiKey: apiKeyResult.apiKey,
    model,
  });
  const config = {
    baseUrl: gatewayApiBaseUrl(),
    apiKey: apiKeyResult.apiKey,
    model,
    contextWindowTokens,
  };

  let writeResult;
  try {
    writeResult = await adapter.writeConfig(config, { setupId: apiKeyResult.setupId, keyId: apiKeyResult.keyId });
  } catch (error) {
    console.error(`\n⚠️ API key created, but writing the ${adapter.name} configuration failed.`);
    console.error(`Key name: ${apiKeyResult.name}`);
    console.error(`Key ID: ${apiKeyResult.keyId}`);
    console.error(`Config path: ${adapter.configPath()}`);
    console.error('The key remains active in Token Market. Fix the local error, then run tokensmarket doctor.');
    throw error;
  }

  await saveManagedKey({
    tool,
    keyName: apiKeyResult.name,
    keyId: apiKeyResult.keyId,
    setupId: apiKeyResult.setupId,
    configPath: writeResult.configPath,
    backupPath: writeResult.backupPath,
    model: config.model,
    createdAt: new Date().toISOString(),
    revocable: true,
  });

  console.log(`\n✅ Token Market configured for ${adapter.name}`);
  console.log(`API Key: ${apiKeyResult.name} (${apiKeyResult.keyId})`);
  console.log(`Model: ${config.model}`);
  console.log(`Config: ${writeResult.configPath}`);
  if (writeResult.backupPath) console.log(`Backup: ${writeResult.backupPath}`);

  const results = await verifier(config, adapter.protocol, config.model === 'glm-5.2' ? ['models', 'chat', 'stream', 'tool-call'] : ['models', 'chat', 'stream']);
  console.log('\nTest results:');
  for (const result of results) console.log(`- ${result.name}: ${result.passed ? 'passed' : `failed${result.detail ? ` (${result.detail})` : ''}`}`);

  console.log('\nNext: restart your agent tool if it is currently running, then select the Token Market model.');
}

async function chooseTool(): Promise<ToolId> {
  const selected = await select({ message: 'Which tool do you want to configure?', options: toolIds().map((id) => ({ value: id, label: getAdapter(id).name })) });
  return toolId(String(selected));
}
