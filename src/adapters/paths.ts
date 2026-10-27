import { readdir } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';

export function homePath(relativePath: string): string {
  return path.join(os.homedir(), relativePath);
}

export function claudeSettingsPath(): string {
  return path.join(process.env.CLAUDE_CONFIG_DIR ?? path.join(os.homedir(), '.claude'), 'settings.json');
}

export function codexConfigPath(): string {
  return path.join(process.env.CODEX_HOME ?? path.join(os.homedir(), '.codex'), 'config.toml');
}

export function workBuddyConfigPath(): string {
  return process.env.WORKBUDDY_CONFIG_PATH ?? homePath('.workbuddy/models.json');
}

export function deepSeekHarnessHome(): string {
  return process.env.DSH_HOME ?? homePath('.dsh');
}

export async function deepSeekHarnessPatchPath(): Promise<string> {
  const activeProfile = process.env.DSH_ACTIVE_PROFILE ?? 'desktop';
  const profilePatch = path.join(deepSeekHarnessHome(), 'profiles', activeProfile, 'cordis.patch.yml');
  try {
    await readdir(path.dirname(profilePatch));
    return profilePatch;
  } catch {
    return path.join(deepSeekHarnessHome(), 'cordis.patch.yml');
  }
}

export function deepSeekHarnessCredentialsPath(): string {
  return path.join(deepSeekHarnessHome(), '.credentials.yaml');
}

export function openClawConfigPath(): string {
  return process.env.OPENCLAW_CONFIG_PATH ?? homePath('.openclaw/openclaw.json');
}

export function openCodeConfigPath(): string {
  return process.env.OPENCODE_CONFIG ?? homePath('.config/opencode/opencode.json');
}
