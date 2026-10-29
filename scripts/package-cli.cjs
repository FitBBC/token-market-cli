'use strict';

const fs = require('node:fs');
const path = require('node:path');
const platforms = require('../npm/platforms.cjs');
const root = path.resolve(__dirname, '..');
const pkg = require('../package.json');

function packagePlatform(key, binary, output = path.join(root, 'dist', 'npm')) {
  const entry = platforms[key];
  if (!entry) throw new Error(`Unknown platform: ${key}`);
  const [os, cpu] = key.split('-');
  const name = `tokenmarket-${key}`;
  if (pkg.optionalDependencies[name] !== pkg.version) {
    throw new Error(`Pin ${name} to ${pkg.version} in optionalDependencies before packaging.`);
  }
  const cargo = fs.readFileSync(path.join(root, 'rust/tokensmarket-cli/Cargo.toml'), 'utf8');
  if (!cargo.includes(`version = "${pkg.version}"`)) {
    throw new Error('The npm and Rust CLI versions must match.');
  }
  const dir = path.join(output, name);
  fs.mkdirSync(path.join(dir, 'bin'), { recursive: true });
  const destination = path.join(dir, 'bin', entry.binary);
  fs.copyFileSync(binary, destination);
  fs.chmodSync(destination, 0o755);
  fs.writeFileSync(path.join(dir, 'package.json'), JSON.stringify({
    name,
    version: pkg.version,
    description: `Token Market native CLI for ${key}`,
    license: pkg.license,
    repository: pkg.repository,
    os: [os],
    cpu: [cpu],
    ...(os === 'linux' ? { libc: ['glibc'] } : {}),
    files: ['bin/'],
    publishConfig: pkg.publishConfig,
  }, null, 2) + '\n');
  fs.writeFileSync(path.join(dir, 'README.md'),
    `# ${name}\n\nPlatform binary for tokenmarket@${pkg.version}. Install the tokenmarket package instead.\n`);
  return dir;
}

if (require.main === module) {
  try {
    const [key, binary, output] = process.argv.slice(2);
    if (!key || !binary) throw new Error('Usage: npm run package:platform -- <os-arch> <binary> [output-directory]');
    console.log(packagePlatform(key, binary, output));
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}

module.exports = { packagePlatform };
