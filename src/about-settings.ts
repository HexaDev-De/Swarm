import { preview } from './bridge';
import { icon } from './dom';
import { appUpdates, describeUpdate } from './app-updates';

/** Public project links. Opening one is user-initiated; only the update check runs on its own. */
export const PROJECT_LINKS = {
  repository: 'https://github.com/HexaDev-De/Swarm',
  issues: 'https://github.com/HexaDev-De/Swarm/issues',
  releases: 'https://github.com/HexaDev-De/Swarm/releases',
  license: 'https://github.com/HexaDev-De/Swarm/blob/main/LICENSE',
} as const;

const link = (label: string, url: string) =>
  `<button type="button" class="secondary-button" data-action="open-url" data-url="${url}">${label}${icon('forward')}</button>`;

/** The About group: app version, license, updates from GitHub Releases, and links. */
export function aboutSettingsHtml(): string {
  return `<div class="settings-group settings-about" aria-labelledby="settings-about-heading">
    <div class="setting-row" style="align-items:center;gap:14px;">
      <img src="/icon.png" width="48" height="48" style="border-radius:12px;object-fit:cover;box-shadow:0 4px 16px rgba(0,0,0,0.25);" alt="Swarm" />
      <div><strong id="settings-about-heading">Swarm</strong><small class="settings-about-version">Version 0.1.0</small></div>
    </div>
    <div class="setting-row"><div><strong>License & Credits</strong><small>Swarm is a multi-agent AI coding workspace by HexaDev-De. Fork of PandaCode under the PandaCode License. No accounts, no telemetry.</small></div>${link('Swarm License', PROJECT_LINKS.license)}</div>
    <div class="setting-row settings-updates"><div><strong>Updates</strong><small class="settings-update-status" aria-live="polite">Checks releases automatically.</small></div><div class="settings-row-actions"><button type="button" class="secondary-button settings-update-action" hidden></button>${link('Releases', PROJECT_LINKS.releases)}</div></div>
    <div class="setting-row settings-updates-auto"><div><strong>Install updates automatically</strong><small>Downloads in the background and restarts when no agent is working.</small></div><input type="checkbox" class="settings-switch" data-update-automatic aria-label="Install updates automatically"></div>
    <div class="setting-row"><div><strong>Source code</strong><small>Report bugs and follow development on HexaDev-De/Swarm.</small></div><div class="settings-row-actions">${link('GitHub repository', PROJECT_LINKS.repository)}${link('Issues', PROJECT_LINKS.issues)}</div></div>
  </div>`;
}

/** Reads the bundle version from Tauri; the browser preview has no app bundle. */
async function appVersion(): Promise<string> {
  if (preview) return 'Preview';
  return (await import('@tauri-apps/api/app')).getVersion();
}

export function mountAboutSettings(host: HTMLElement): () => void {
  const version = host.querySelector<HTMLElement>('.settings-about-version');
  let disposed = false;
  const cleanupUpdates = mountUpdates(host);
  if (version) {
    appVersion()
      .then(value => { if (!disposed) version.textContent = `Version ${value}`; })
      .catch(() => { if (!disposed) version.textContent = 'Version unavailable'; });
  }
  return () => { disposed = true; cleanupUpdates(); };
}

/** The Updates rows follow the updater: status, Check now / Restart now, and the automatic switch. */
function mountUpdates(host: HTMLElement): () => void {
  const updates = appUpdates();
  const status = host.querySelector<HTMLElement>('.settings-update-status')!;
  const action = host.querySelector<HTMLButtonElement>('.settings-update-action')!;
  const auto = host.querySelector<HTMLInputElement>('[data-update-automatic]')!;
  if (!updates?.supported) {
    status.textContent = describeUpdate({ kind: 'off' }, false);
    host.querySelector<HTMLElement>('.settings-updates-auto')?.remove();
    return () => {};
  }
  const paint = () => {
    const state = updates.state;
    status.textContent = describeUpdate(state, updates.automatic);
    auto.checked = updates.automatic;
    const restartable = state.kind === 'ready' || state.kind === 'available';
    action.hidden = state.kind === 'downloading' || state.kind === 'installing';
    action.disabled = state.kind === 'checking';
    action.textContent = restartable ? (state.kind === 'ready' ? 'Restart now' : 'Update now') : 'Check for updates';
  };
  const click = () => { void (updates.state.kind === 'ready' || updates.state.kind === 'available' ? updates.restart() : updates.check()); };
  const change = () => updates.setAutomatic(auto.checked);
  action.addEventListener('click', click);
  auto.addEventListener('change', change);
  const unsubscribe = updates.subscribe(paint);
  paint();
  return () => { unsubscribe(); action.removeEventListener('click', click); auto.removeEventListener('change', change); };
}
