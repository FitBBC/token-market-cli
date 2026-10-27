import { promises as fs } from 'node:fs';
import { ManagedKeyRecord, ToolId } from '../types.js';
import { keysFile } from './paths.js';
import { ensureParentDirectory } from '../utils/fs.js';

export async function listManagedKeys(): Promise<ManagedKeyRecord[]> {
  try {
    const data = JSON.parse(await fs.readFile(keysFile(), 'utf8')) as ManagedKeyRecord[];
    return Array.isArray(data) ? data : [];
  } catch {
    return [];
  }
}

export async function saveManagedKey(record: ManagedKeyRecord): Promise<void> {
  const current = await listManagedKeys();
  const next = [...current.filter((item) => item.tool !== record.tool), record];
  await ensureParentDirectory(keysFile());
  await fs.writeFile(keysFile(), `${JSON.stringify(next, null, 2)}\n`, { mode: 0o600 });
}

export async function findManagedKey(tool: ToolId): Promise<ManagedKeyRecord | null> {
  return (await listManagedKeys()).find((record) => record.tool === tool) ?? null;
}
