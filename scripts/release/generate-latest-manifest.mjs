// Generates latest.json auto-updater manifest from collected release artifacts.
import { readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../', import.meta.url));

export function sanitizeSignature(rawSig) {
  if (!rawSig) return '';
  const lines = rawSig.trim().split(/\r?\n/);
  const candidate = lines.length >= 2 ? lines[1].trim() : lines[0].trim();
  const isBase64 = /^[A-Za-z0-9+/=]+$/.test(candidate) && candidate.length > 20;
  return isBase64 ? candidate : '';
}

export async function readSignature(filePath) {
  try {
    const content = await readFile(filePath, 'utf8');
    return sanitizeSignature(content);
  } catch {
    return '';
  }
}

export async function generateLatestManifest(assetsDir, releaseVersion, repository = 'HexaDev-De/Swarm') {
  const darwinSig = await readSignature(resolve(assetsDir, 'Swarm-macOS-arm64.app.tar.gz.sig'));
  const windowsSig = await readSignature(resolve(assetsDir, 'Swarm-Windows-x64-setup.exe.sig'));
  const linuxSig = await readSignature(resolve(assetsDir, 'Swarm-Linux-x86_64.AppImage.sig'));

  const pubDate = new Date().toISOString();
  const v = releaseVersion.replace(/^v/, '');

  const platforms = {};
  const addPlatform = (target, sig, url) => {
    const entry = { url };
    if (sig) {
      entry.signature = sig;
    }
    platforms[target] = entry;
  };

  addPlatform('darwin-aarch64', darwinSig, `https://github.com/${repository}/releases/download/v${v}/Swarm-macOS-arm64.app.tar.gz`);
  addPlatform('windows-x86_64', windowsSig, `https://github.com/${repository}/releases/download/v${v}/Swarm-Windows-x64-setup.exe`);
  addPlatform('linux-x86_64', linuxSig, `https://github.com/${repository}/releases/download/v${v}/Swarm-Linux-x86_64.AppImage`);

  return {
    version: v,
    notes: `Swarm v${v}: Multi-Agent AI Coding Workspace`,
    pub_date: pubDate,
    platforms
  };
}

async function main() {
  const assetsDir = process.argv[2] ? resolve(process.argv[2]) : resolve(root, 'dist-artifacts');
  const pkg = JSON.parse(await readFile(resolve(root, 'package.json'), 'utf8'));
  const version = process.argv[3] || pkg.version;
  const repository = process.env.GITHUB_REPOSITORY || 'HexaDev-De/Swarm';

  const manifest = await generateLatestManifest(assetsDir, version, repository);
  const outPath = resolve(assetsDir, 'latest.json');
  await writeFile(outPath, `${JSON.stringify(manifest, null, 2)}\n`);
  console.log(`Generated ${outPath}:\n`, JSON.stringify(manifest, null, 2));
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch(err => {
    console.error(err);
    process.exit(1);
  });
}
