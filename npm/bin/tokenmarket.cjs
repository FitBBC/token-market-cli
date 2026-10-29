#!/usr/bin/env node
'use strict';

const { spawnSync } = require('node:child_process');
const platforms = require('../platforms.cjs');
const { version } = require('../../package.json');

function resolveBinary(platform = process.platform, arch = process.arch, resolve = require.resolve) {
  const key = `${platform}-${arch}`;
  const entry = platforms[key];
  if (!entry) {
    throw new Error(`Unsupported platform: ${key}. Supported: ${Object.keys(platforms).join(', ')}.`);
  }
  if (platform === 'linux' && !process.report.getReport().header.glibcVersionRuntime) {
    throw new Error('The prebuilt Linux CLI requires glibc. On Alpine/musl, build rust/tokensmarket-cli from source.');
  }
  const name = `tokenmarket-${key}`;
  let manifest;
  try {
    manifest = resolve(`${name}/package.json`);
  } catch (error) {
    if (error.code !== 'MODULE_NOT_FOUND') throw error;
    throw new Error(
      `Missing ${name}@${version}. Install with optional dependencies enabled: ` +
      '`npm install -g --include=optional tokenmarket`. For npx, retry with a fresh npm cache and --include=optional.',
    );
  }
  const installed = require(manifest);
  if (installed.version !== version) {
    throw new Error(`Expected ${name}@${version}, found ${installed.version}. Reinstall tokenmarket.`);
  }
  return require('node:path').join(require('node:path').dirname(manifest), 'bin', entry.binary);
}

function main() {
  try {
    const result = spawnSync(resolveBinary(), process.argv.slice(2), { stdio: 'inherit' });
    if (result.error) throw result.error;
    if (result.signal) {
      // Preserve interrupt/termination semantics for callers and shell scripts.
      process.kill(process.pid, result.signal);
      return;
    }
    process.exitCode = result.status ?? 1;
  } catch (error) {
    console.error(`tokenmarket: ${error.message}`);
    process.exitCode = 1;
  }
}

if (require.main === module) main();
module.exports = { resolveBinary };
