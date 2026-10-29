'use strict';

const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { createHash } = require('node:crypto');
const pkg = require('../package.json');
const platforms = require('../npm/platforms.cjs');

try {
  const [directory, flag] = process.argv.slice(2);
  if (!directory || (flag && flag !== '--dry-run') || !process.env.npm_execpath) {
    throw new Error('Usage: npm run publish:cli -- <tarball-directory> [--dry-run]');
  }
  if (process.env.RELEASE_TAG && process.env.RELEASE_TAG !== `cli-v${pkg.version}`) {
    throw new Error(`Release tag must equal cli-v${pkg.version}. Update npm and Rust CLI versions together.`);
  }
  // Validate the whole artifact set before any irreversible registry writes.
  const files = [...Object.keys(platforms).map(key => `tokenmarket-${key}`), pkg.name]
    .map(name => path.resolve(directory, `${name}-${pkg.version}.tgz`));
  for (const file of files) {
    if (!fs.statSync(file).isFile()) throw new Error(`Missing release tarball: ${file}`);
  }
  // Check every tarball before writing any package to the registry.
  const publish = (file, dryRun) => {
    const result = spawnSync(process.execPath, [process.env.npm_execpath, 'publish', file,
      '--access', 'public', ...(dryRun ? ['--dry-run'] : [])], { stdio: 'inherit' });
    if (result.error) throw result.error;
    if (result.status !== 0) throw new Error(`Publishing ${path.basename(file)} failed; launcher publication stopped.`);
  };
  for (const file of files) publish(file, true);
  if (!flag) {
    // A retry may follow a partially successful release. Only reuse an existing
    // immutable version when its integrity matches this exact release artifact.
    const pending = [];
    for (const file of files) {
      const name = path.basename(file, `-${pkg.version}.tgz`);
      const result = spawnSync(process.execPath, [process.env.npm_execpath,
        'view', `${name}@${pkg.version}`, 'dist.integrity', '--json',
        '--registry', 'https://registry.npmjs.org/'], { encoding: 'utf8' });
      if (result.error) throw result.error;
      let metadata;
      try { metadata = JSON.parse(result.stdout); } catch { /* fail closed below */ }
      if (result.status === 0) {
        const integrity = `sha512-${createHash('sha512').update(fs.readFileSync(file)).digest('base64')}`;
        if (metadata !== integrity) throw new Error(`${name}@${pkg.version} already exists with different contents. Use a new version.`);
        console.log(`Already published with matching integrity: ${name}@${pkg.version}`);
      } else if (metadata?.error?.code === 'E404') {
        pending.push(file);
      } else {
        throw new Error(`Cannot check ${name}@${pkg.version}: ${result.stderr}`);
      }
    }
    for (const file of pending) publish(file, false);
  }
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
