// Generates latest.json auto-updater manifest from collected release artifacts.
import { readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../', import.meta.url));

export async function readSignature(filePath) {
  try {
    const content = await readFile(filePath, 'utf8');
    return content.trim();
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

  return {
    version: v,
    notes: `Swarm v${v}: Multi-Agent AI Coding Workspace`,
    pub_date: pubDate,
    platforms: {
      'darwin-aarch64': {
        signature: darwinSig,
        url: `https://github.com/${repository}/releases/download/v${v}/Swarm-macOS-arm64.app.tar.gz`
      },
      'windows-x86_64': {
        signature: windowsSig,
        url: `https://github.com/${repository}/releases/download/v${v}/Swarm-Windows-x64-setup.exe`
      },
      'linux-x86_64': {
        signature: linuxSig,
        url: `https://github.com/${repository}/releases/download/v${v}/Swarm-Linux-x86_64.AppImage`
      }
    }
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
