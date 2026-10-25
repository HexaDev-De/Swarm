/* Automatic updates from GitHub Releases. The app checks the latest release's
 * latest.json (tauri.conf.json `plugins.updater`) shortly after launch and every six
 * hours, downloads a newer version in the background, and installs it with a restart
 * once no agent is working and the window has been idle for a while, so a restart never
 * cuts a run short. Every download is verified against the public key built into the
 * app before it installs. The request carries no personal data, and turning automatic
 * updates off in Settings → About stops it. The browser preview never checks. */
import { preview } from './bridge';
import { CHECK_EVERY, readyToRestart, type UpdateState } from './app-updates-policy';
export { describeUpdate, type UpdateState } from './app-updates-policy';

export interface UpdatesHost {
  /** True while any agent is working; installing waits for it to finish. */
  busy(): boolean;
  notify(text: string): void;
}
export interface AppUpdates {
  readonly state: UpdateState;
  readonly automatic: boolean;
  readonly supported: boolean;
  subscribe(listener: () => void): () => void;
  setAutomatic(on: boolean): void;
  check(): Promise<void>;
  /** Installs a downloaded update and restarts; refuses while an agent is working. */
  restart(): Promise<void>;
}

const KEY = 'swarm.autoUpdate';
const LEGACY_KEY = 'pandacode.autoUpdate';
const FIRST_CHECK = 30_000;

type Pending = { version: string; download(onEvent: (event: { event: string; data?: { contentLength?: number; chunkLength?: number } }) => void): Promise<void>; install(): Promise<void> };

let instance: AppUpdates | undefined;
export const appUpdates = (): AppUpdates | undefined => instance;

export function startAppUpdates(host: UpdatesHost): AppUpdates {
  const listeners = new Set<() => void>();
  let state: UpdateState = preview ? { kind: 'off' } : { kind: 'idle' };
  let automatic = (() => {
    try {
      const saved = localStorage.getItem(KEY) ?? localStorage.getItem(LEGACY_KEY);
      return saved !== 'off';
    } catch {
      return true;
    }
  })();
  let pending: Pending | null = null, lastCheck = 0, lastInput = Date.now(), checking: Promise<void> | null = null;
  const set = (next: UpdateState) => { state = next; listeners.forEach(listener => listener()); };
  const message = (cause: unknown) => cause instanceof Error ? cause.message : String(cause);

  async function download(update: Pending): Promise<void> {
    let total = 0, received = 0;
    set({ kind: 'downloading', version: update.version, percent: null });
    await update.download(event => {
      if (event.event === 'Started') total = event.data?.contentLength ?? 0;
      if (event.event === 'Progress') { received += event.data?.chunkLength ?? 0; if (total) set({ kind: 'downloading', version: update.version, percent: Math.min(100, Math.round(received / total * 100)) }); }
    });
    set({ kind: 'ready', version: update.version });
    host.notify(`Swarm will restart to update...`);
  }

  async function run(): Promise<void> {
    if (preview || state.kind === 'downloading' || state.kind === 'ready' || state.kind === 'installing') return;
    lastCheck = Date.now();
    set({ kind: 'checking' });
    try {
      const { check } = await import('@tauri-apps/plugin-updater');
      const update = await check() as Pending | null;
      if (!update) { set({ kind: 'current', checkedAt: Date.now() }); return; }
      pending = update;
      host.notify(`Swarm ${update.version} is available.`);
      if (automatic) await download(update); else set({ kind: 'available', version: update.version });
    } catch (cause) {
      // Offline, rate-limited or a release without a manifest: say so in About, never interrupt.
      set({ kind: 'error', message: message(cause) });
    }
  }
  const check = () => (checking ??= run().finally(() => { checking = null; }));

  async function restart(): Promise<void> {
    if (!pending) return;
    if (host.busy()) { host.notify('Swarm will restart to update when your agents finish.'); return; }
    if (state.kind === 'available') { try { await download(pending); } catch (cause) { set({ kind: 'error', message: message(cause) }); return; } }
    const version = pending.version;
    set({ kind: 'installing', version });
    try {
      await pending.install();
      await (await import('@tauri-apps/plugin-process')).relaunch();
    } catch (cause) { set({ kind: 'error', message: message(cause) }); }
  }

  if (!preview) {
    const input = () => { lastInput = Date.now(); };
    for (const name of ['pointerdown', 'keydown', 'wheel']) addEventListener(name, input, { passive: true, capture: true });
    window.setTimeout(() => { if (automatic) void check(); }, FIRST_CHECK);
    window.setInterval(() => {
      if (automatic && Date.now() - lastCheck >= CHECK_EVERY) void check();
      if (automatic && state.kind === 'ready' && readyToRestart(host.busy(), lastInput)) void restart();
    }, 30_000);
  }

  instance = {
    get state() { return state; },
    get automatic() { return automatic; },
    supported: !preview,
    subscribe: listener => { listeners.add(listener); return () => listeners.delete(listener); },
    setAutomatic(on) {
      automatic = on;
      try { localStorage.setItem(KEY, on ? 'on' : 'off'); } catch { /* A per-device preference only. */ }
      if (on && state.kind !== 'ready') void check();
      listeners.forEach(listener => listener());
    },
    check,
    restart,
  };
  return instance;
}
