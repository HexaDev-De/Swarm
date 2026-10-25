import { writeFile, chmod, rm, appendFile } from 'node:fs/promises';
import { randomBytes } from 'node:crypto';
import { resolve } from 'node:path';
import { execFileSync } from 'node:child_process';
import { required } from './common.mjs';

const dir = required('RUNNER_TEMP');
const keychain = resolve(dir, 'pandacode-release.keychain-db');
const certificate = resolve(dir, 'pandacode-signing.p12');
const apiKey = resolve(dir, 'pandacode-apple.p8');
try {
  if (process.argv[2] === 'cleanup') {
    try { execFileSync('security', ['delete-keychain', keychain], { stdio: 'pipe' }); } catch { /* May not have been created. */ }
    await rm(certificate, { force: true }); await rm(apiKey, { force: true });
  } else {
    const password = randomBytes(32).toString('base64url');
    await writeFile(certificate, Buffer.from(required('APPLE_CERTIFICATE'), 'base64'), { mode: 0o600 });
    await chmod(certificate, 0o600);
    execFileSync('security', ['create-keychain', '-p', password, keychain], { stdio: 'pipe' });
    execFileSync('security', ['set-keychain-settings', '-lut', '21600', keychain], { stdio: 'pipe' });
    execFileSync('security', ['unlock-keychain', '-p', password, keychain], { stdio: 'pipe' });
    execFileSync('security', ['import', certificate, '-k', keychain, '-P', required('APPLE_CERTIFICATE_PASSWORD'), '-T', '/usr/bin/codesign', '-T', '/usr/bin/security'], { stdio: 'pipe' });
    execFileSync('security', ['set-key-partition-list', '-S', 'apple-tool:,apple:,codesign:', '-s', '-k', password, keychain], { stdio: 'pipe' });
    execFileSync('security', ['list-keychains', '-d', 'user', '-s', keychain], { stdio: 'pipe' });
    await writeFile(apiKey, required('APPLE_API_PRIVATE_KEY'), { mode: 0o600 });
    await appendFile(required('GITHUB_ENV'), `APPLE_API_KEY_PATH=${apiKey}\n`);
    await rm(certificate, { force: true });
    console.log('Ephemeral Apple signing keychain configured.');
  }
} catch { console.error('Apple signing setup failed. Check protected CI secrets and Developer ID certificate. No credentials were printed.'); process.exitCode = 1; }
