'use strict';

const fs = require('node:fs');
const path = require('node:path');
const { createHash } = require('node:crypto');
const platforms = require('../npm/platforms.cjs');
const pkg = require('../package.json');

function checkBinaries(directory = path.resolve(__dirname, '../npm/native')) {
  for (const [key, entry] of Object.entries(platforms)) {
    const manifest = JSON.parse(fs.readFileSync(path.join(directory, key, 'manifest.json'), 'utf8'));
    const binary = path.join(directory, key, 'bin', entry.binary);
    if (manifest.version !== pkg.version || manifest.target !== entry.target) {
      throw new Error(`Wrong version or Rust target for ${key}`);
    }
    const digest = createHash('sha256').update(fs.readFileSync(binary)).digest('hex');
    if (digest !== manifest.sha256) throw new Error(`Binary checksum mismatch for ${key}`);
    // GitHub artifact downloads restore files as 0644. npm must pack executables.
    fs.chmodSync(binary, 0o755);
  }
}

if (require.main === module) {
  try {
    checkBinaries();
    console.log(`All ${Object.keys(platforms).length} native binaries verified for ${pkg.version}.`);
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
module.exports = { checkBinaries };
