'use strict';

// Rust targets and npm OS/CPU names must stay aligned with cli-npm.yml.
module.exports = {
  'darwin-arm64': { target: 'aarch64-apple-darwin', binary: 'tokenmarket' },
  'darwin-x64': { target: 'x86_64-apple-darwin', binary: 'tokenmarket' },
  'win32-x64': { target: 'x86_64-pc-windows-msvc', binary: 'tokenmarket.exe' },
  'linux-x64': { target: 'x86_64-unknown-linux-gnu', binary: 'tokenmarket' },
  'linux-arm64': { target: 'aarch64-unknown-linux-gnu', binary: 'tokenmarket' },
};
