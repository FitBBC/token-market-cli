import { promises as fs } from 'node:fs';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import path from 'node:path';
import os from 'node:os';

const execFileAsync = promisify(execFile);

export async function ensureParentDirectory(file: string): Promise<void> {
  await fs.mkdir(path.dirname(file), { recursive: true });
}

export async function fileExists(file: string): Promise<boolean> {
  try {
    await fs.access(file);
    return true;
  } catch {
    return false;
  }
}

export async function commandExists(command: string): Promise<boolean> {
  try {
    const finder = process.platform === 'win32' ? 'where' : 'which';
    await execFileAsync(finder, [command]);
    return true;
  } catch {
    return false;
  }
}

export function home(relativePath: string): string {
  return path.join(os.homedir(), relativePath);
}

export async function createBackup(configPath: string): Promise<string | null> {
  const exists = await fileExists(configPath);
  if (!exists) return null;

  const extension = path.extname(configPath) || '.json';
  const backupPath = path.join(
    os.homedir(),
    '.tokensmarket',
    'backups',
    path.basename(configPath, extension),
    `${timestamp()}${extension}`,
  );
  await ensureParentDirectory(backupPath);
  await fs.copyFile(configPath, backupPath);
  return backupPath;
}

export function timestamp(): string {
  return new Date().toISOString().replaceAll(/[:.]/g, '-');
}

export async function readJsonFile<T>(file: string): Promise<T> {
  return JSON.parse(await fs.readFile(file, 'utf8')) as T;
}

export async function writeJsonFile(file: string, value: unknown): Promise<void> {
  await ensureParentDirectory(file);
  await fs.writeFile(file, `${JSON.stringify(value, null, 2)}\n`, { mode: 0o600 });
}
