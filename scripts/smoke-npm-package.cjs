'use strict';

// Serve the actual release tarball through a temporary local npm registry.
// This exercises a fresh npx installation without publishing a package.
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const http = require('node:http');
const { createHash } = require('node:crypto');
const { spawn } = require('node:child_process');
const assert = require('node:assert/strict');
const pkg = require('../package.json');
const platforms = require('../npm/platforms.cjs');

async function main() {
  const input = process.argv[2];
  const npmCli = process.env.npm_execpath;
  if (!input || !npmCli) throw new Error('Usage: npm run test:package -- <tokenmarket.tgz or directory>');
  const key = `${process.platform}-${process.arch}`;
  if (!platforms[key]) throw new Error(`Unsupported smoke-test host: ${key}`);
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), 'tokenmarket-npx-'));
  const cache = path.join(temporary, 'cache');
  const config = path.join(temporary, '.npmrc');
  fs.writeFileSync(config, '');
  const env = {
    ...process.env,
    NPM_CONFIG_CACHE: cache,
    NPM_CONFIG_USERCONFIG: config,
    NPM_CONFIG_AUDIT: 'false',
    NPM_CONFIG_FUND: 'false',
    NPM_CONFIG_UPDATE_NOTIFIER: 'false',
  };
  const run = (args, cwd = temporary) => new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [npmCli, ...args], { cwd, env });
    let stdout = '', stderr = '';
    const timeout = setTimeout(() => child.kill(), 120_000);
    child.stdout.on('data', data => { stdout += data; });
    child.stderr.on('data', data => { stderr += data; });
    child.on('error', reject);
    child.on('close', (status, signal) => {
      clearTimeout(timeout);
      if (signal) reject(new Error(`npm terminated: ${signal}\n${stderr}`));
      else resolve({ status, stdout, stderr });
    });
  });
  let registry;
  try {
    const tarball = fs.statSync(input).isDirectory() ? path.join(input, `${pkg.name}-${pkg.version}.tgz`) : input;
    const archive = fs.readFileSync(path.resolve(tarball));
    let origin;
    const downloaded = new Set();
    registry = http.createServer((request, response) => {
      const name = decodeURIComponent(new URL(request.url, 'http://localhost').pathname.slice(1));
      const bytes = name === 'package.tgz' ? archive : null;
      if (bytes) {
        downloaded.add(name);
        response.writeHead(200, { 'Content-Type': 'application/octet-stream' });
        response.end(bytes);
      } else if (name === pkg.name) {
        response.setHeader('Content-Type', 'application/json');
        response.end(JSON.stringify({
          name, 'dist-tags': { latest: pkg.version }, versions: {
            [pkg.version]: { ...pkg, dist: {
              tarball: `${origin}/package.tgz`,
              integrity: `sha512-${createHash('sha512').update(archive).digest('base64')}`,
            } },
          },
        }));
      } else {
        response.writeHead(404, { 'Content-Type': 'application/json' });
        response.end(JSON.stringify({ error: 'not_found' }));
      }
    });
    await new Promise((resolve, reject) => {
      registry.once('error', reject);
      registry.listen(0, '127.0.0.1', resolve);
    });
    origin = `http://127.0.0.1:${registry.address().port}`;
    const exec = args => run(['exec', '--yes', '--ignore-scripts', '--omit=optional', '--registry', origin,
      '--package', `${pkg.name}@${pkg.version}`, '--', 'tokenmarket', ...args]);
    const version = await exec(['--version']);
    assert.equal(version.status, 0, version.stderr);
    assert.equal(version.stdout.trim(), `tokenmarket ${pkg.version}`);
    assert.deepEqual([...downloaded], ['package.tgz']);
    const help = await exec(['--help']);
    assert.equal(help.status, 0, help.stderr);
    for (const command of ['setup', 'doctor', 'test', 'keys']) assert.match(help.stdout, new RegExp(command));
    const invalid = await exec(['not-a-command']);
    assert.equal(invalid.status, 2, invalid.stderr);
    console.log(`npx tarball smoke test passed (${key}): version, help, invalid-command exit status.`);
  } finally {
    if (registry) await new Promise(resolve => registry.close(resolve));
    fs.rmSync(temporary, { recursive: true, force: true });
  }
}

main().catch(error => { console.error(error); process.exitCode = 1; });
