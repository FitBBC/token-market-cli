'use strict';

const { test, before, after } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { resolveBinary } = require('../npm/bin/tokenmarket.cjs');
const { packagePlatform } = require('../scripts/package-cli.cjs');
const platforms = require('../npm/platforms.cjs');
const pkg = require('../package.json');

let temporary;
let launcher;
before(() => {
  temporary = fs.mkdtempSync(path.join(os.tmpdir(), 'tokenmarket-launcher-'));
  fs.cpSync(path.join(__dirname, '../npm'), path.join(temporary, 'npm'), { recursive: true });
  fs.writeFileSync(path.join(temporary, 'package.json'), JSON.stringify(pkg));
  // A real native executable exercises argv, stdio, cwd, env and exit handling
  // on every OS without touching Agent configurations or authorization.
  packagePlatform(`${process.platform}-${process.arch}`, process.execPath, path.join(temporary, 'node_modules'));
  launcher = path.join(temporary, 'npm/bin/tokenmarket.cjs');
});
after(() => fs.rmSync(temporary, { recursive: true, force: true }));

test('all platform dependencies are pinned to the launcher version', () => {
  assert.deepEqual(pkg.optionalDependencies,
    Object.fromEntries(Object.keys(platforms).map(key => [`tokenmarket-${key}`, pkg.version])));
});

test('unsupported systems and missing optional packages give actionable errors', () => {
  assert.throws(() => resolveBinary('freebsd', 'x64'), /Unsupported platform/);
  assert.throws(() => resolveBinary('darwin', 'arm64', () => {
    throw Object.assign(new Error('missing'), { code: 'MODULE_NOT_FOUND' });
  }), /--include=optional/);
});

test('the launcher preserves arguments, working directory, environment and stdout/stderr', () => {
  const args = ['with spaces', '中文', '$(echo should-not-run)', '--flag=value'];
  const result = spawnSync(process.execPath, [launcher, '-e',
    'console.log(JSON.stringify({args:process.argv.slice(1),cwd:process.cwd(),env:process.env.TOKEN_MARKET_NPM_FIXTURE}));console.error("stderr inherited");',
    '--', ...args], {
    cwd: temporary,
    env: { ...process.env, TOKEN_MARKET_NPM_FIXTURE: 'inherited' },
    encoding: 'utf8',
  });
  assert.equal(result.status, 0, result.stderr);
  assert.deepEqual(JSON.parse(result.stdout), { args, cwd: fs.realpathSync(temporary), env: 'inherited' });
  assert.match(result.stderr, /stderr inherited/);
});

test('the launcher preserves a failing native exit status', () => {
  const result = spawnSync(process.execPath, [launcher, '-e', 'process.exit(17)']);
  assert.equal(result.status, 17);
});

test('interactive stdin reaches the native CLI', () => {
  const result = spawnSync(process.execPath, [launcher, '-e',
    'process.stdout.write(require("node:fs").readFileSync(0))'], {
    input: 'interactive choice\n', encoding: 'utf8',
  });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stdout, 'interactive choice\n');
});

test('the launcher preserves native termination signals', { skip: process.platform === 'win32' }, () => {
  const result = spawnSync(process.execPath, [launcher, '-e', 'process.kill(process.pid,"SIGTERM")']);
  assert.equal(result.signal, 'SIGTERM');
});

test('platform packaging sets npm OS/CPU restrictions and executable permissions', () => {
  for (const [key, entry] of Object.entries(platforms)) {
    const dir = packagePlatform(key, __filename, path.join(temporary, 'metadata'));
    const manifest = JSON.parse(fs.readFileSync(path.join(dir, 'package.json'), 'utf8'));
    assert.deepEqual(manifest.os, [key.split('-')[0]]);
    assert.deepEqual(manifest.cpu, [key.split('-')[1]]);
    assert.equal(manifest.version, pkg.version);
    assert.ok(fs.existsSync(path.join(dir, 'bin', entry.binary)));
    if (process.platform !== 'win32') assert.equal(fs.statSync(path.join(dir, 'bin', entry.binary)).mode & 0o777, 0o755);
  }
});

test('mismatched platform versions fail before executing the binary', () => {
  const manifest = path.join(temporary, 'node_modules', `tokenmarket-${process.platform}-${process.arch}`, 'package.json');
  const original = fs.readFileSync(manifest, 'utf8');
  try {
    fs.writeFileSync(manifest, JSON.stringify({ version: '0.0.0' }));
    const result = spawnSync(process.execPath, [launcher, '--version'], { encoding: 'utf8' });
    assert.equal(result.status, 1);
    assert.match(result.stderr, /found 0.0.0/);
  } finally {
    fs.writeFileSync(manifest, original);
  }
});
