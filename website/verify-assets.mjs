import { readdir } from 'node:fs/promises';

// A build must never publish desktop binaries, even if an old local copy remains.
async function verify(directory) {
  for (const entry of await readdir(directory, { withFileTypes: true }).catch(error => {
    if (error.code === 'ENOENT') return [];
    throw error;
  })) {
    const path = new URL(`${entry.name}${entry.isDirectory() ? '/' : ''}`, directory);
    if (/\.(?:zip|dmg|pkg|exe|msi|app|appimage|deb|rpm|tar|gz|sig|sha256)$/i.test(entry.name) || entry.name === 'downloads') {
      throw new Error(`Private desktop artifact found in static assets: ${path.pathname}. Remove it; builds are published on GitHub Releases, not with the website.`);
    }
    if (entry.isDirectory()) await verify(path);
  }
}
await verify(new URL('./public/', import.meta.url));
await verify(new URL('./dist/', import.meta.url));
