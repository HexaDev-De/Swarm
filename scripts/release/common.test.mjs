import test from 'node:test';
import assert from 'node:assert/strict';
import { version, required, targetConfig, targets, hostTarget, releaseDirectory, cargoLockVersion } from './common.mjs';

test('release versions cannot become paths, prereleases, or shell commands', () => {
  for (const input of ['../secret', '1.0.0;echo x', 'v1.0.0', '01.0.0', '1.0.0-beta', undefined]) assert.throws(() => version(input));
  assert.equal(version('0.3.0'), '0.3.0');
  assert.match(releaseDirectory('1.2.3'), /release[\\/]1\.2\.3$/);
});

test('only the three published targets are accepted', () => {
  assert.deepEqual(Object.keys(targets), ['darwin-aarch64', 'windows-x86_64', 'linux-x86_64']);
  for (const target of ['linux-aarch64', 'windows-arm64', '../mac', 'toString', '__proto__', null]) assert.throws(() => targetConfig(target));
});

test('release asset names are stable across versions', () => {
  const assets = Object.values(targets).map(config => config.asset);
  assert.equal(new Set(assets).size, 3);
  for (const asset of assets) assert.doesNotMatch(asset, /\d+\.\d+\.\d+/);
});

test('the host maps to its native target', () => {
  assert.equal(hostTarget('darwin', 'arm64'), 'darwin-aarch64');
  assert.equal(hostTarget('win32', 'x64'), 'windows-x86_64');
  assert.equal(hostTarget('linux', 'x64'), 'linux-x86_64');
  assert.throws(() => hostTarget('linux', 'arm64'), /No release target/);
});

test('missing variables fail without revealing other values', () => {
  assert.throws(() => required('MISSING', { SECRET: 'dontlogme' }), { message: 'Missing MISSING. Set it in your shell or CI environment.' });
});

test('the Cargo.lock version is set with either line ending', () => {
  const lock = ['[[package]]', 'name = "pancakes"', 'version = "1.0.0"', '', '[[package]]', 'name = "pandacode"', 'version = "0.3.0"', 'dependencies = [', ' "serde",', ']', ''];
  for (const eol of ['\n', '\r\n']) {
    assert.equal(cargoLockVersion(lock.join(eol), '0.3.1'), lock.join(eol).replace('version = "0.3.0"', 'version = "0.3.1"'));
  }
  assert.throws(() => cargoLockVersion('[[package]]\r\nname = "other"\r\nversion = "1.0.0"\r\n', '0.3.1'), /No pandacode package/);
  assert.throws(() => cargoLockVersion(lock.join('\n'), '0.3.1;x'));
});

test('the updater manifest points every signed platform at its release asset', async () => {
  const { updaterManifest } = await import('./common.mjs');
  const manifest = updaterManifest('0.3.1', ' Notes \n', { 'darwin-aarch64': 'sig-mac\n', 'linux-x86_64': 'sig-linux' }, '2026-10-18T00:00:00Z', 'aicodeking/pandacode');
  assert.deepEqual(manifest, {
    version: '0.3.1', notes: 'Notes', pub_date: '2026-10-18T00:00:00Z',
    platforms: {
      'darwin-aarch64': { signature: 'sig-mac', url: 'https://github.com/aicodeking/pandacode/releases/download/v0.3.1/Swarm-macOS-arm64.app.tar.gz' },
      'linux-x86_64': { signature: 'sig-linux', url: 'https://github.com/aicodeking/pandacode/releases/download/v0.3.1/Swarm-Linux-x86_64.AppImage' },
    },
  });
  assert.throws(() => updaterManifest('0.3.1', '', {}), /No signed updater files/);
  assert.throws(() => updaterManifest('0.3.1', '', { 'windows-x86_64': ' ' }), /Missing updater signature/);
  const fork = updaterManifest('0.3.1', '', { 'windows-x86_64': 'sig-win' }, '2026-10-18T00:00:00Z', 'someone/pandacode');
  assert.equal(fork.platforms['windows-x86_64'].url, 'https://github.com/someone/pandacode/releases/download/v0.3.1/Swarm-Windows-x64-setup.exe');
  assert.throws(() => updaterManifest('0.3.1', '', { 'windows-x86_64': 'sig' }, undefined, 'evil.example/x?y'), /Not a GitHub repository/);
});
