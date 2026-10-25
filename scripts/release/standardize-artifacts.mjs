// Standardizes build artifacts into stable release names, generates sha256 checksums,
// and preserves Tauri update signatures across macOS, Linux, and Windows.
import { readdir, readFile, writeFile, mkdir, copyFile, chmod } from 'node:fs/promises';
import { resolve, join } from 'node:path';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { repackAppImage } from './appimage.mjs';
import { run } from './common.mjs';

const root = fileURLToPath(new URL('../../', import.meta.url));

async function findFiles(dir, filter) {
  const results = [];
  try {
    const entries = await readdir(dir, { withFileTypes: true });
    for (const entry of entries) {
      const fullPath = join(dir, entry.name);
      if (entry.isDirectory()) {
        results.push(...await findFiles(fullPath, filter));
      } else if (filter(entry.name)) {
        results.push(fullPath);
      }
    }
  } catch {}
  return results;
}

async function sha256File(filePath) {
  const content = await readFile(filePath);
  return createHash('sha256').update(content).digest('hex');
}

async function signingEnvironment(env) {
  const key = env.TAURI_SIGNING_PRIVATE_KEY?.trim();
  if (!key) return env;
  try {
    return { ...env, TAURI_SIGNING_PRIVATE_KEY: (await readFile(key, 'utf8')).trim() };
  } catch {
    return env;
  }
}

async function main() {
  const platform = process.argv[2] || process.platform;
  const pkg = JSON.parse(await readFile(resolve(root, 'package.json'), 'utf8'));
  const ver = pkg.version;
  const outDir = resolve(root, 'dist-artifacts');
  await mkdir(outDir, { recursive: true });

  const targetDir = resolve(root, 'src-tauri/target');

  if (platform === 'ubuntu-22.04' || platform === 'linux') {
    const appImages = await findFiles(targetDir, name => name.endsWith('.AppImage') && !name.endsWith('.AppImage.sig'));
    if (appImages.length > 0) {
      const src = appImages[0];
      try {
        console.log(`Repacking Linux AppImage to drop bundled Wayland libraries: ${src}`);
        await repackAppImage(src, { log: message => console.log(message) });
      } catch (err) {
        console.warn(`repackAppImage warning: ${err.message}`);
      }

      if (process.env.TAURI_SIGNING_PRIVATE_KEY) {
        try {
          const tauriCli = resolve(root, 'node_modules/@tauri-apps/cli/tauri.js');
          const signEnv = await signingEnvironment(process.env);
          await run(process.execPath, [tauriCli, 'signer', 'sign', src], { env: signEnv });
          console.log(`Resigned repacked AppImage: ${src}`);
        } catch (e) {
          console.warn(`Could not resign AppImage: ${e.message}`);
        }
      }

      const dest = resolve(outDir, 'Swarm-Linux-x86_64.AppImage');
      const versioned = resolve(outDir, `Swarm_${ver}-Linux-amd64.AppImage`);
      await copyFile(src, dest);
      await copyFile(src, versioned);
      await chmod(dest, 0o755);
      await chmod(versioned, 0o755);
      const digest = await sha256File(dest);
      await writeFile(`${dest}.sha256`, `${digest}  Swarm-Linux-x86_64.AppImage\n`);

      const sigSrc = `${src}.sig`;
      let sigContent = '';
      try {
        sigContent = await readFile(sigSrc, 'utf8');
      } catch {}
      if (!sigContent.trim()) {
        sigContent = 'untrusted comment: unsigned release artifact\n';
      }
      await writeFile(resolve(outDir, 'Swarm-Linux-x86_64.AppImage.sig'), sigContent);
      console.log(`Standardized Linux artifacts in ${outDir}`);
    }
  } else if (platform === 'windows-latest' || platform === 'win32') {
    const exes = await findFiles(targetDir, name => name.endsWith('.exe') && name.includes('setup') && !name.endsWith('.sig'));
    if (exes.length > 0) {
      const src = exes[0];
      const dest = resolve(outDir, 'Swarm-Windows-x64-setup.exe');
      await copyFile(src, dest);
      const digest = await sha256File(dest);
      await writeFile(`${dest}.sha256`, `${digest}  Swarm-Windows-x64-setup.exe\n`);

      const sigSrc = `${src}.sig`;
      let sigContent = '';
      try {
        sigContent = await readFile(sigSrc, 'utf8');
      } catch {}
      if (!sigContent.trim()) {
        sigContent = 'untrusted comment: unsigned release artifact\n';
      }
      await writeFile(resolve(outDir, 'Swarm-Windows-x64-setup.exe.sig'), sigContent);
      console.log(`Standardized Windows artifacts in ${outDir}`);
    }
  } else if (platform === 'macos-latest' || platform === 'darwin') {
    const dmgs = await findFiles(targetDir, name => name.endsWith('.dmg'));
    if (dmgs.length > 0) {
      const src = dmgs[0];
      const dest = resolve(outDir, 'Swarm-macOS-arm64.dmg');
      await copyFile(src, dest);
      const digest = await sha256File(dest);
      await writeFile(`${dest}.sha256`, `${digest}  Swarm-macOS-arm64.dmg\n`);
      console.log(`Standardized macOS DMG in ${outDir}`);
    }
    const tars = await findFiles(targetDir, name => name.endsWith('.app.tar.gz'));
    if (tars.length > 0) {
      const src = tars[0];
      const dest = resolve(outDir, 'Swarm-macOS-arm64.app.tar.gz');
      await copyFile(src, dest);
      const sigSrc = `${src}.sig`;
      let sigContent = '';
      try {
        sigContent = await readFile(sigSrc, 'utf8');
      } catch {}
      if (!sigContent.trim()) {
        sigContent = 'untrusted comment: unsigned release artifact\n';
      }
      await writeFile(resolve(outDir, 'Swarm-macOS-arm64.app.tar.gz.sig'), sigContent);
      console.log(`Standardized macOS update archive and signature in ${outDir}`);
    }
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch(err => {
    console.error(err);
    process.exit(1);
  });
}
