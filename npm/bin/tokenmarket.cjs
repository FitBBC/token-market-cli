#!/usr/bin/env node
'use strict';

const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');
const platforms = require('../platforms.cjs');
const { version } = require('../../package.json');

function resolveBinary(platform = process.platform, arch = process.arch, root = path.resolve(__dirname, '../..')) {
  const key = `${platform}-${arch}`;
  const entry = platforms[key];
  if (!entry) {
    throw new Error(`Unsupported platform: ${key}. Supported: ${Object.keys(platforms).join(', ')}.`);
  }
  if (platform === 'linux' && !process.report.getReport().header.glibcVersionRuntime) {
    throw new Error('The prebuilt Linux CLI requires glibc. On Alpine/musl, build rust/tokenmarket-cli from source.');
  }
  const directory = path.join(root, 'npm', 'native', key);
  const manifest = path.join(directory, 'manifest.json');
  const binary = path.join(directory, 'bin', entry.binary);
  if (!fs.existsSync(manifest) || !fs.existsSync(binary)) {
    throw new Error(`Missing bundled CLI for ${key}. Reinstall tokenmarket@${version}.`);
  }
  const installed = require(manifest);
  if (installed.version !== version) {
    throw new Error(`Expected native CLI ${version}, found ${installed.version}. Reinstall tokenmarket.`);
  }
  return binary;
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
