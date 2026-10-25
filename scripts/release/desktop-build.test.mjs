import test from 'node:test';
import assert from 'node:assert/strict';
import { buildPlan, builtArtifact, checksumLine, macosSigning } from './build.mjs';
import { appImageBundleOptions } from './common.mjs';

const env = { PANDACODE_LICENSE_PUBLIC_KEY: 'stale', PANDACODE_UPDATE_PUBLIC_KEY: 'stale', TAURI_SIGNING_PRIVATE_KEY: 'stale', APPLE_ID: 'someone', APPLE_SIGNING_IDENTITY: 'Developer ID Application: Someone', PATH: 'test-path' };
const config = plan => JSON.parse(plan.args.at(-1));

test('builds require the native host and never forward old license keys or Apple variables to Windows and Linux', () => {
  for (const [target, platform] of [['windows-x86_64', 'win32'], ['linux-x86_64', 'linux']]) {
    const plan = buildPlan(target, env, platform, 'x64');
    for (const name of ['APPLE_ID', 'APPLE_SIGNING_IDENTITY', 'PANDACODE_LICENSE_PUBLIC_KEY', 'PANDACODE_UPDATE_PUBLIC_KEY']) assert.equal(plan.env[name], undefined, name);
    assert.equal(plan.env.PATH, 'test-path');
    assert.equal(plan.signing.mode, 'unsigned');
    // The update signing key is the one secret a build needs: it signs the in-app update file.
    assert.equal(plan.env.TAURI_SIGNING_PRIVATE_KEY, 'stale');
    assert.equal(config(plan).bundle.createUpdaterArtifacts, true);
    assert.equal(config(buildPlan(target, { PATH: 'p' }, platform, 'x64')).bundle.createUpdaterArtifacts, false);
    assert.equal(config(plan).plugins, undefined);
    assert.throws(() => buildPlan(target, env, 'darwin', 'arm64'), /native/);
  }
  assert.throws(() => buildPlan('darwin-aarch64', env, 'darwin', 'x64'), /native/);
  assert.equal(env.APPLE_ID, 'someone', 'the caller environment is unchanged');
});

test('Windows installs per user, bootstraps WebView2 and links the C++ runtime statically', () => {
  const plan = buildPlan('windows-x86_64', { ...env, STATIC_VCRUNTIME: 'false' }, 'win32', 'x64');
  assert.equal(plan.env.STATIC_VCRUNTIME, 'true');
  assert.deepEqual(config(plan).bundle.windows.webviewInstallMode, { type: 'downloadBootstrapper', silent: true });
  assert.equal(config(plan).bundle.windows.nsis.installMode, 'currentUser');
  assert.equal(buildPlan('linux-x86_64', { STATIC_VCRUNTIME: 'false' }, 'linux', 'x64').env.STATIC_VCRUNTIME, 'false');
});

test('Linux bundles the external-link helper without changing Mac or Windows bundles', () => {
  const expected = { linux: { appimage: { files: { '/usr/bin/xdg-open': '/usr/bin/xdg-open' } } } };
  assert.deepEqual(appImageBundleOptions('linux-x86_64'), expected);
  assert.deepEqual(config(buildPlan('linux-x86_64', env, 'linux', 'x64')).bundle.linux, expected.linux);
  assert.deepEqual(appImageBundleOptions('darwin-aarch64'), {});
  assert.equal(config(buildPlan('windows-x86_64', env, 'win32', 'x64')).bundle.linux, undefined);
});

test('macOS falls back to an ad-hoc signature and drops partial Apple credentials', () => {
  const plan = buildPlan('darwin-aarch64', { APPLE_ID: 'someone', PATH: 'p' }, 'darwin', 'arm64');
  assert.equal(plan.signing.mode, 'adhoc');
  assert.equal(plan.env.APPLE_SIGNING_IDENTITY, '-');
  assert.equal(plan.env.APPLE_ID, undefined);
  assert.equal(config(plan).bundle.macOS.signingIdentity, '-');
  assert.ok(plan.args.includes('app,dmg'));
});

test('Developer ID signing requires a real identity and notarization credentials', () => {
  const identity = 'Developer ID Application: Example (TEAM123)';
  assert.throws(() => macosSigning({ APPLE_SIGNING_IDENTITY: 'Apple Development: Someone' }), /Developer ID/);
  assert.throws(() => macosSigning({ APPLE_SIGNING_IDENTITY: identity }), /notarized/);
  assert.deepEqual(macosSigning({ APPLE_SIGNING_IDENTITY: identity, APPLE_API_KEY: 'k', APPLE_API_ISSUER: 'i', APPLE_API_KEY_PATH: '/k.p8' }), { mode: 'developer-id', identity });
  const plan = buildPlan('darwin-aarch64', { APPLE_SIGNING_IDENTITY: identity, APPLE_ID: 'a', APPLE_PASSWORD: 'p', APPLE_TEAM_ID: 't' }, 'darwin', 'arm64');
  assert.equal(plan.signing.mode, 'developer-id');
  assert.equal(plan.env.APPLE_ID, 'a');
});

test('the build selects the exact versioned bundle Tauri produced', () => {
  for (const [target, file] of [['windows-x86_64', 'Swarm_1.2.3_x64-setup.exe'], ['linux-x86_64', 'Swarm_1.2.3_amd64.AppImage'], ['darwin-aarch64', 'Swarm_1.2.3_aarch64.dmg']]) {
    assert.equal(builtArtifact([file, file.replace('1.2.3', '1.2.2')], '1.2.3', target), file);
    assert.throws(() => builtArtifact([file], '1.2.4', target), /did not produce/);
  }
});

test('checksum sidecars use the sha256sum format', () => {
  assert.equal(checksumLine('a'.repeat(64), 'Swarm-Linux-x86_64.AppImage'), `${'a'.repeat(64)}  Swarm-Linux-x86_64.AppImage\n`);
  assert.throws(() => checksumLine('nothex', 'x'), /SHA-256/);
});
