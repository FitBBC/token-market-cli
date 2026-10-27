export type ToolId =
  | 'claude-code'
  | 'codex'
  | 'workbuddy'
  | 'deepseek-harness'
  | 'openclaw'
  | 'opencode';

export interface SetupSession {
  setupId: string;
  authorizeUrl: string;
}

export interface ExchangeResult {
  apiKey: string;
  keyId: string;
  name: string;
  expiresAt: string | null;
}

export interface ToolConfig {
  baseUrl: string;
  apiKey: string;
  model: string;
}

export interface AdapterContext {
  setupId: string;
  keyId: string;
}

export interface AdapterWriteResult {
  configPath: string;
  backupPath: string | null;
  requiresRestart: boolean;
}

export interface Adapter {
  id: ToolId;
  name: string;
  protocol: 'anthropic-compatible' | 'openai-compatible' | 'custom';
  configPath(): string | Promise<string>;
  isInstalled(): Promise<boolean>;
  readConfig(): Promise<ToolConfig | null>;
  writeConfig(config: ToolConfig, context: AdapterContext): Promise<AdapterWriteResult>;
  verifyConfig(config: ToolConfig): Promise<void>;
}

export interface ManagedKeyRecord {
  tool: ToolId;
  keyName: string;
  keyId: string;
  setupId: string;
  configPath: string;
  backupPath: string | null;
  model: string;
  createdAt: string;
  revocable: true;
}

export interface VerificationResult {
  name: string;
  passed: boolean;
  detail?: string;
}
