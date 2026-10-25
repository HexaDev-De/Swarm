import { readFile, readdir, stat } from 'node:fs/promises';
import { gzipSync } from 'node:zlib';
import { execFileSync } from 'node:child_process';
import path from 'node:path';

const root = path.resolve(import.meta.dirname, '..');
const assets = await readdir(path.join(root, 'dist/assets')).catch(() => []);
const sizes = await Promise.all(assets.map(async name => {
  const body = await readFile(path.join(root, 'dist/assets', name));
  return { file: name, bytes: body.length, gzipBytes: gzipSync(body).length };
}));
const binary = await stat(path.join(root, 'src-tauri/target/release/pandacode')).catch(() => null);
const result = { assets: sizes, nativeBinaryBytes: binary?.size ?? null, processes: [] };
if (process.platform === 'darwin' || process.platform === 'linux') {
  try {
  const rows = execFileSync('ps', ['-axo', 'pid=,ppid=,rss=,comm='], { encoding: 'utf8' }).trim().split('\n').map(line => {
    const match = line.trim().match(/^(\d+)\s+(\d+)\s+(\d+)\s+(.+)$/);
    return match ? { pid: Number(match[1]), parentPid: Number(match[2]), rssBytes: Number(match[3]) * 1024, command: match[4] } : null;
  }).filter(Boolean);
  const processIds = new Set(rows.filter(p => /(?:\/|^)pandacode$/i.test(p.command)).map(p => p.pid));
  let changed = true;
  while (changed) {
    changed = false;
    for (const row of rows) if (processIds.has(row.parentPid) && !processIds.has(row.pid)) { processIds.add(row.pid); changed = true; }
  }
  result.processes = rows.filter(row => processIds.has(row.pid));
  } catch {
    result.processSamplingError = 'Process sampling is unavailable in this environment.';
  }
}
result.note = 'RSS is a point-in-time resident-memory reading, not private footprint. macOS WebKit helpers may be parented by launchd and are not reliably attributable by process tree. Provider children are separate workloads. These figures are not a T3 Code comparison.';
console.log(JSON.stringify(result, null, 2));
