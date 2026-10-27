import { mkdtempSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { afterAll, beforeAll, expect, test } from 'vitest';
import { listManagedKeys, saveManagedKey } from '../src/config/managed-keys.js';
import { keysFile } from '../src/config/paths.js';

let originalHome: string | undefined;

beforeAll(() => {
  originalHome = process.env.HOME;
  process.env.HOME = mkdtempSync(path.join(os.tmpdir(), 'tokensmarket-'));
});

afterAll(() => {
  if (originalHome) process.env.HOME = originalHome;
});

test('saves and lists managed keys without duplicating the key', async () => {
  await saveManagedKey({
    tool: 'claude-code',
    keyName: 'Token Market CLI · Claude Code',
    keyId: 'key_123',
    setupId: 'setup_123',
    configPath: '/tmp/config.json',
    backupPath: null,
    model: 'glm-5.2',
    createdAt: '2026-10-27T00:00:00.000Z',
    revocable: true,
  });

  const records = await listManagedKeys();
  expect(records).toHaveLength(1);
  expect(records[0]?.keyId).toBe('key_123');
  expect((await readFile(keysFile(), 'utf8')).includes('tm-secret')).toBe(false);
});
