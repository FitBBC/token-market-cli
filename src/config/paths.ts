import os from 'node:os';
import path from 'node:path';

export function tokenMarketHome(): string {
  return path.join(os.homedir(), '.tokensmarket');
}

export function keysFile(): string {
  return path.join(tokenMarketHome(), 'keys.json');
}

export function backupsRoot(): string {
  return path.join(tokenMarketHome(), 'backups');
}
