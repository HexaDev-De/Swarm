import { readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { root, version, json, cargoLockVersion } from './common.mjs';

const next = version(process.argv[2]);
for (const path of ['package.json', 'package-lock.json', 'src-tauri/tauri.conf.json']) {
  const absolute = resolve(root, path);
  const value = await json(absolute);
  value.version = next;
  if (value.packages?.['']) value.packages[''].version = next;
  await writeFile(absolute, `${JSON.stringify(value, null, 2)}\n`);
}
const cargoPath = resolve(root, 'src-tauri/Cargo.toml');
const cargo = await readFile(cargoPath, 'utf8');
await writeFile(cargoPath, cargo.replace(/(\[package\][\s\S]*?\nversion = )"[^"]+"/, `$1"${next}"`));
const lockPath = resolve(root, 'src-tauri/Cargo.lock');
const lock = await readFile(lockPath, 'utf8');
await writeFile(lockPath, cargoLockVersion(lock, next));
// The website changelog (/changelog) dates each release from this map.
const datesPath = resolve(root, 'website/src/data/release-dates.json');
const dates = await json(datesPath);
const today = new Date().toISOString().slice(0, 10);
await writeFile(datesPath, `${JSON.stringify({ ...dates, [next]: dates[next] ?? today }, null, 2)}\n`);
console.log(`Release version set to ${next}. Commit these files, then tag v${next}.`);
