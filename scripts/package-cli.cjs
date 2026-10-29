'use strict';

const fs = require('node:fs');
const path = require('node:path');
const { createHash } = require('node:crypto');
const platforms = require('../npm/platforms.cjs');
const root = path.resolve(__dirname, '..');
const pkg = require('../package.json');

function packagePlatform(key, binary, output = path.join(root, 'npm', 'native')) {
  const entry = platforms[key];
  if (!entry) throw new Error(`Unknown platform: ${key}`);
  const cargo = fs.readFileSync(path.join(root, 'rust/tokenmarket-cli/Cargo.toml'), 'utf8');
  if (!cargo.includes(`version = "${pkg.version}"`)) {
    throw new Error('The npm and Rust CLI versions must match.');
  }
  const dir = path.join(output, key);
  fs.mkdirSync(path.join(dir, 'bin'), { recursive: true });
  const destination = path.join(dir, 'bin', entry.binary);
  fs.copyFileSync(binary, destination);
  fs.chmodSync(destination, 0o755);
  fs.writeFileSync(path.join(dir, 'manifest.json'), JSON.stringify({
    version: pkg.version,
    target: entry.target,
    sha256: createHash('sha256').update(fs.readFileSync(destination)).digest('hex'),
  }, null, 2) + '\n');
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
