'use strict';

const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { createHash } = require('node:crypto');
const pkg = require('../package.json');
const platforms = require('../npm/platforms.cjs');
const publisher = path.resolve(__dirname, '../scripts/publish-npm.cjs');
const names = [pkg.name];

function fixture(run) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'tokenmarket-publish-'));
  const log = path.join(dir, 'calls.jsonl');
  const fakeNpm = path.join(dir, 'npm.cjs');
  const packedFiles = Object.entries(platforms).flatMap(([key, entry]) => [
    { path: `npm/native/${key}/bin/${entry.binary}` }, { path: `npm/native/${key}/manifest.json` },
  ]);
  fs.writeFileSync(fakeNpm, `
    const fs = require('node:fs');
    const args = process.argv.slice(2);
    fs.appendFileSync(process.env.TOKEN_MARKET_PUBLISH_LOG, JSON.stringify(args)+'\\n');
    if (args[0] === 'pack') {
      console.log(JSON.stringify([{name:${JSON.stringify(pkg.name)},version:${JSON.stringify(pkg.version)},
        files: ${JSON.stringify(packedFiles)}.filter(entry=>entry.path!==process.env.TOKEN_MARKET_PUBLISH_MISSING)}]));
    } else if (args[0] === 'view') {
      const existing = JSON.parse(process.env.TOKEN_MARKET_PUBLISH_EXISTING || '{}');
      if (existing[args[1]]) console.log(JSON.stringify(existing[args[1]]));
      else { console.log(JSON.stringify({error:{code:'E404'}})); process.exitCode=1; }
    } else if (process.env.TOKEN_MARKET_PUBLISH_FAIL && args.includes(process.env.TOKEN_MARKET_PUBLISH_FAIL)) {
      process.exitCode = 1;
    }
  `);
  for (const name of names) fs.writeFileSync(path.join(dir, `${name}-${pkg.version}.tgz`), name);
  const execute = (options = {}) => {
    const result = spawnSync(process.execPath, [publisher, dir, ...(options.dryRun ? ['--dry-run'] : [])], {
      encoding: 'utf8', env: {
        ...process.env, npm_execpath: fakeNpm, RELEASE_TAG: options.tag ?? `cli-v${pkg.version}`,
        TOKEN_MARKET_PUBLISH_LOG: log,
        TOKEN_MARKET_PUBLISH_EXISTING: JSON.stringify(options.existing ?? {}),
        TOKEN_MARKET_PUBLISH_FAIL: options.fail ?? '',
        TOKEN_MARKET_PUBLISH_MISSING: options.missing ?? '',
      },
    });
    const calls = fs.existsSync(log) ? fs.readFileSync(log, 'utf8').trim().split('\n').map(JSON.parse) : [];
    return { ...result, calls, writes: calls.filter(args => args[0] === 'publish' && !args.includes('--dry-run')) };
  };
  try { run({ dir, execute }); } finally { fs.rmSync(dir, { recursive: true, force: true }); }
}

test('an incomplete release or mismatched tag causes no registry writes', () => {
  fixture(({ dir, execute }) => {
    const mismatch = execute({ tag: `v${pkg.version}` });
    assert.equal(mismatch.status, 1);
    assert.equal(mismatch.calls.length, 0);
    fs.unlinkSync(path.join(dir, `${pkg.name}-${pkg.version}.tgz`));
    const missing = execute();
    assert.equal(missing.status, 1);
    assert.equal(missing.calls.length, 0);
  });
});

test('all tarballs must pass the publication preflight before the first write', () => {
  fixture(({ dir, execute }) => {
    const result = execute({ fail: path.join(dir, `${pkg.name}-${pkg.version}.tgz`) });
    assert.equal(result.status, 1);
    assert.equal(result.writes.length, 0);
  });
});

test('dry runs never publish and successful releases publish exactly one complete package', () => {
  fixture(({ execute }) => {
    const dryRun = execute({ dryRun: true });
    assert.equal(dryRun.status, 0, dryRun.stderr);
    assert.equal(dryRun.writes.length, 0);
    const result = execute();
    assert.equal(result.status, 0, result.stderr);
    assert.deepEqual(result.writes.map(args => path.basename(args[1])), names.map(name => `${name}-${pkg.version}.tgz`));
  });
});

test('a release missing a bundled platform is rejected before publication', () => {
  fixture(({ execute }) => {
    const result = execute({ missing: 'npm/native/win32-x64/bin/tokenmarket.exe' });
    assert.equal(result.status, 1);
    assert.equal(result.writes.length, 0);
    assert.match(result.stderr, /Incomplete release tarball/);
  });
});

test('release retries skip identical artifacts but reject an existing conflicting version', () => {
  fixture(({ execute }) => {
    const name = names[0];
    const integrity = `sha512-${createHash('sha512').update(name).digest('base64')}`;
    const result = execute({ existing: { [`${name}@${pkg.version}`]: integrity } });
    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.writes.length, names.length - 1);
    assert.ok(result.writes.every(args => !args[1].endsWith(`${name}-${pkg.version}.tgz`)));
  });
  fixture(({ execute }) => {
    const result = execute({ existing: { [`${names[0]}@${pkg.version}`]: 'sha512-different' } });
    assert.equal(result.status, 1);
    assert.equal(result.writes.length, 0);
    assert.match(result.stderr, /different contents/);
  });
});
