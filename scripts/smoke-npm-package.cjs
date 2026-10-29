'use strict';

// Serve the actual release tarballs through a temporary local npm registry.
// This exercises npx's optional-dependency selection without publishing a package.
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const http = require('node:http');
const { createHash } = require('node:crypto');
const { spawn } = require('node:child_process');
const assert = require('node:assert/strict');
const pkg = require('../package.json');
const root = path.resolve(__dirname, '..');
const platforms = require('../npm/platforms.cjs');

async function main() {
  const platformTarball = process.argv[2];
  const npmCli = process.env.npm_execpath;
  if (!platformTarball || !npmCli) throw new Error('Usage: npm run test:package -- <platform.tgz>');
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
    const packed = await run(['pack', root, '--json', '--pack-destination', temporary,
      '--ignore-scripts', '--cache', path.join(temporary, 'pack-cache')]);
    assert.equal(packed.status, 0, packed.stderr);
    const launcher = fs.readFileSync(path.join(temporary, JSON.parse(packed.stdout)[0].filename));
    const platformName = `tokenmarket-${key}`;
    const nativePath = fs.statSync(platformTarball).isDirectory()
      ? path.join(platformTarball, `${platformName}-${pkg.version}.tgz`) : platformTarball;
    const native = fs.readFileSync(path.resolve(nativePath));
    let origin;
    const downloaded = new Set();
    registry = http.createServer((request, response) => {
      const name = decodeURIComponent(new URL(request.url, 'http://localhost').pathname.slice(1));
      const bytes = name === 'launcher.tgz' ? launcher : name === 'native.tgz' ? native : null;
      if (bytes) {
        downloaded.add(name);
        response.writeHead(200, { 'Content-Type': 'application/octet-stream' });
        response.end(bytes);
      } else if (name === pkg.name || name === platformName) {
        const data = name === pkg.name ? launcher : native;
        const manifest = name === pkg.name ? pkg : {
          name: platformName, version: pkg.version,
          os: [process.platform], cpu: [process.arch],
        };
        response.setHeader('Content-Type', 'application/json');
        response.end(JSON.stringify({
          name, 'dist-tags': { latest: pkg.version }, versions: {
            [pkg.version]: { ...manifest, dist: {
              tarball: `${origin}/${name === pkg.name ? 'launcher' : 'native'}.tgz`,
              integrity: `sha512-${createHash('sha512').update(data).digest('base64')}`,
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
    const exec = args => run(['exec', '--yes', '--include=optional', '--registry', origin,
      '--package', `${pkg.name}@${pkg.version}`, '--', 'tokenmarket', ...args]);
    const version = await exec(['--version']);
    assert.equal(version.status, 0, version.stderr);
    assert.equal(version.stdout.trim(), `tokenmarket ${pkg.version}`);
    assert.deepEqual([...downloaded].sort(), ['launcher.tgz', 'native.tgz']);
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
