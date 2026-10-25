import './settings.css';
import { mountProviderReadiness } from './provider-readiness';
import { escape, icon } from './dom';
import { providerLogo } from './provider-logos';
import { uiSizeControls } from './ui-size';
import type { Provider, ProviderId } from './types';
import { mountOverlaySettings } from './overlay-settings';
import { mountSupportDetails } from './support-settings';
import { aboutSettingsHtml, mountAboutSettings } from './about-settings';
import { mountResourceUsage, resourceUsageHtml } from './resource-usage-settings';
import { colorsSummary } from './theme-settings';
import { mountAgentTeams, type AgentTeamsOptions } from './agent-teams';
import { mountCloudSettings, type CloudSettingsHost } from './cloud-settings';

/** Panels that live elsewhere and are embedded here: each mounts into a host and returns its cleanup. */
type Embed = (host: HTMLElement) => Promise<() => void>;

export type SettingsSection = 'general' | 'appearance' | 'agents' | 'providers' | 'teams' | 'tools' | 'cloud' | 'github' | 'about';

interface SettingsOptions {
  providers: Provider[];
  defaultProvider: ProviderId;
  compact: boolean;
  wallpapers: boolean;
  theme: 'light' | 'dark';
  preview: boolean;
  section?: SettingsSection;
  discover(): Promise<Provider[]>;
  /** Mounts the optional GitHub row; returns its cleanup. */
  github?(host: HTMLElement): () => void;
  /** Named agent teams and the classic Team setup switch. */
  teams?: Omit<AgentTeamsOptions, 'providers'>;
  /** Cloud sandbox connections (Daytona, Cloudflare, AWS). */
  cloud?: CloudSettingsHost;
  embed: { colors: Embed; wallpapers: Embed; providerConnections: Embed; openCodeModels: Embed };
  /** Leaves Settings for the workspace. */
  close(): void;
}
export interface SettingsView { show(section: SettingsSection): void; dispose(): void }

/** The groups in the left column, in order. Each names what it holds, not a vague umbrella. */
export const SECTIONS: { id: SettingsSection; label: string; icon: string; description: string }[] = [
  { id: 'general', label: 'General', icon: 'settings', description: 'Text size, conversation density and the quick launcher.' },
  { id: 'appearance', label: 'Appearance', icon: 'sun', description: 'Theme, accent colors and wallpapers.' },
  { id: 'agents', label: 'Agents', icon: 'bot', description: 'Your default agent, installed agents and their models.' },
  { id: 'providers', label: 'Providers', icon: 'server', description: 'Connections for Claude Code, GLM, MiMo, OpenCode and Antigravity.' },
  { id: 'teams', label: 'Agent teams', icon: 'grid', description: 'Named teams of models, and the classic team setup.' },
  { id: 'tools', label: 'Tools & add-ons', icon: 'widget', description: 'App tools, MCP servers, skills and Jev.' },
  { id: 'cloud', label: 'Cloud sandboxes', icon: 'globe', description: 'Daytona, Cloudflare and AWS machines for your agents.' },
  { id: 'github', label: 'GitHub', icon: 'github', description: 'Issues, pull requests and repositories through the GitHub CLI.' },
  { id: 'about', label: 'About & updates', icon: 'info', description: 'Version, updates, resource usage, license and support.' },
];

const switchRow = (id: string, title: string, detail: string, checked: boolean) =>
  `<div class="setting-row"><div><strong>${title}</strong><small>${detail}</small></div><input type="checkbox" class="settings-switch" id="${id}" aria-label="${title}" ${checked ? 'checked' : ''}></div>`;

function sectionHtml(id: SettingsSection, options: SettingsOptions): string {
  const defaultName = options.providers.find(provider => provider.id === options.defaultProvider)?.name ?? options.defaultProvider;
  switch (id) {
    case 'general': return `<div class="settings-group">${uiSizeControls()}${switchRow('compact-mode', 'Compact conversation', 'Less space between messages.', options.compact)}</div>
      <h3 class="settings-heading">Quick launcher</h3><div id="launcher-settings"></div>`;
    case 'appearance': return `<div class="settings-group">
        <div class="setting-row"><div><strong>Theme</strong><small>${options.theme === 'dark' ? 'Dark' : 'Light'}. Imported VS Code themes keep their own.</small></div><button type="button" class="secondary-button" data-action="toggle-theme">${icon(options.theme === 'dark' ? 'sun' : 'moon')}Use ${options.theme === 'dark' ? 'light' : 'dark'}</button></div>
        <div class="setting-row"><div><strong>Daily wallpapers</strong><small>A new scene for each part of the day.</small></div><input type="checkbox" class="settings-switch" id="daily-wallpapers" aria-label="Daily wallpapers" ${options.wallpapers ? 'checked' : ''}></div>
      </div>
      <h3 class="settings-heading">Colors <small class="settings-colors-summary">${escape(colorsSummary())}</small></h3><div class="settings-embed" data-embed="colors"></div>
      <h3 class="settings-heading">Wallpapers</h3><div class="settings-embed" data-embed="wallpapers"></div>`;
    case 'agents': return `<div class="settings-group">
        <div class="setting-row settings-default-row"><div><strong>Default agent</strong><small>Used for new tasks.</small></div><button type="button" id="default-provider" class="settings-select" data-action="default-provider" aria-label="Default agent for new tasks" aria-haspopup="dialog" aria-expanded="false">${providerLogo(options.defaultProvider)}<span>${escape(defaultName)}</span>${icon('down')}</button></div>
        <div class="setting-row"><div><strong>Agent updates</strong><small>Keep installed agents up to date.</small></div><button type="button" class="secondary-button" data-action="agent-updates" aria-haspopup="dialog">Check updates</button></div>
      </div>
      <div class="settings-heading settings-agents-heading"><h3>Installed agents</h3><button type="button" class="settings-refresh">${icon('refresh')}Refresh agents</button></div>
      <div class="settings-provider-status" role="status" hidden></div>
      <div class="provider-diagnostics settings-group"></div>
      ${options.preview ? '<p class="settings-preview-note">Browser preview · sample agent availability.</p>' : ''}
      <h3 class="settings-heading">OpenCode models</h3><div class="settings-embed" data-embed="openCodeModels"></div>`;
    case 'providers': return '<div class="settings-embed" data-embed="providerConnections"></div>';
    case 'teams': return '<div id="agent-teams-settings"></div>';
    case 'tools': return `<div class="settings-group">
        <div class="setting-row"><div><strong>Tools & integrations</strong><small>App tools, MCP servers, and skills.</small></div><button type="button" class="secondary-button" data-action="agent-integrations">Manage</button></div>
        <div class="setting-row"><div><strong>Jev · Optional add-on</strong><small>Browser tests and decision assistance. Your agents work without it.</small></div><button type="button" class="secondary-button" data-action="jev-settings">Set up Jev</button></div>
      </div>`;
    case 'cloud': return '<div id="cloud-settings"></div>';
    case 'github': return '<div id="github-settings"></div>';
    case 'about': return `${aboutSettingsHtml()}${resourceUsageHtml()}
      <div class="settings-group settings-support"><div class="setting-row"><div><strong>Help & support</strong><small>Open an issue on GitHub.</small></div><button type="button" class="secondary-button" data-action="support">Open issues${icon('forward')}</button></div><details class="support-details"><summary>App details for support</summary><p>Only the app version, platform, and architecture. Nothing is sent automatically.</p><textarea aria-label="App details for support" rows="3" readonly spellcheck="false"></textarea><div class="support-details-actions"><button type="button" class="secondary-button support-copy" disabled>${icon('copy')}Copy app details</button><button type="button" class="secondary-button support-retry" hidden>Try again</button></div><p role="status" aria-live="polite"></p></details></div>`;
  }
}

/** Settings as a full page: groups on the left, the chosen group's settings on the right. */
export function mountSettings(host: HTMLElement, options: SettingsOptions): SettingsView {
  host.classList.add('settings-page-root');
  host.innerHTML = `<nav class="settings-nav" aria-label="Settings sections">
      <div class="settings-nav-head" data-tauri-drag-region><button type="button" class="settings-back" data-settings-close aria-label="Back to the workspace" title="Back (Esc)">${icon('back')}<span>Back</span></button></div>
      <h1 class="settings-nav-title" data-tauri-drag-region>Settings</h1>
      <div class="settings-nav-list" role="tablist" aria-orientation="vertical" aria-label="Settings">${SECTIONS.map(section => `<button type="button" role="tab" id="settings-tab-${section.id}" data-section="${section.id}" aria-controls="settings-panel-${section.id}" aria-selected="false" tabindex="-1">${icon(section.icon)}<span>${section.label}</span></button>`).join('')}</div>
    </nav>
    <div class="settings-content">${SECTIONS.map(section => `<section class="settings-panel" id="settings-panel-${section.id}" role="tabpanel" aria-labelledby="settings-tab-${section.id}" hidden><header class="settings-panel-head" data-tauri-drag-region><h2>${section.label}</h2><p>${section.description}</p></header><div class="settings-panel-body">${sectionHtml(section.id, options)}</div></section>`).join('')}</div>`;
  const tabs = [...host.querySelectorAll<HTMLButtonElement>('[role="tab"]')];
  const panels = new Map(SECTIONS.map(section => [section.id, host.querySelector<HTMLElement>(`#settings-panel-${section.id}`)!]));
  const cleanups: (() => void)[] = [];
  const mounted = new Set<SettingsSection>();
  let disposed = false;

  const list = host.querySelector<HTMLElement>('.provider-diagnostics')!;
  const refreshButton = host.querySelector<HTMLButtonElement>('.settings-refresh')!;
  const status = host.querySelector<HTMLElement>('.settings-provider-status')!;
  let pending = false, discovered = false;
  const readinessCleanup: (() => void)[] = [];
  const live = () => !disposed && host.isConnected && !host.hidden;

  function updateProviders(providers: Provider[]): void {
    const rows = new Map(Array.from(list.querySelectorAll<HTMLDetailsElement>('details')).map(row => [row.dataset.provider!, row]));
    for (const [index, provider] of providers.entries()) {
      let row = rows.get(provider.id);
      if (!row) {
        row = document.createElement('details');
        row.className = 'provider-diagnostic'; row.dataset.provider = provider.id;
        row.innerHTML = `<summary aria-label="${escape(provider.name)} details"><span class="provider-avatar ${provider.id}">${providerLogo(provider.id)}</span><strong></strong><span class="provider-availability"><span class="status-dot"></span><span class="settings-installed"></span></span>${icon('chevron')}</summary><div class="settings-provider-details"><span>Version</span><span class="settings-provider-version"></span><span>Location</span><code></code></div>`;
      }
      if (provider.id === 'antigravity' && !row.querySelector('.provider-readiness')) {
        const check = document.createElement('div');
        row.append(check); readinessCleanup.push(mountProviderReadiness(check));
      }
      row.querySelector('strong')!.textContent = provider.name;
      row.querySelector('summary')!.setAttribute('aria-label', `${provider.name} details, ${provider.installed ? 'Installed' : 'Not installed'}`);
      row.querySelector('.provider-availability')!.classList.toggle('available', provider.installed);
      row.querySelector('.settings-installed')!.textContent = provider.installed ? (provider.id === 'antigravity' ? 'ACP installed' : 'Installed') : 'Not installed';
      row.querySelector('.settings-provider-version')!.textContent = provider.version || 'Not reported';
      row.querySelector('code')!.textContent = provider.path || 'Install the agent, then refresh.';
      if (list.children[index] !== row) list.insertBefore(row, list.children[index] ?? null);
      rows.delete(provider.id);
    }
    rows.forEach(row => row.remove());
  }
  async function refresh(): Promise<void> {
    if (!live() || pending) return;
    pending = true; refreshButton.disabled = true;
    status.hidden = false; status.textContent = 'Checking agents…';
    try {
      const providers = await options.discover();
      if (!live()) return;
      updateProviders(providers); discovered = true;
      status.textContent = ''; status.hidden = true;
    } catch {
      if (!live()) return;
      status.textContent = 'Couldn’t check agents. Try refreshing again.';
    } finally { pending = false; if (live()) refreshButton.disabled = false; }
  }

  /** A group's live parts mount the first time it opens, and stay mounted while Settings is open. */
  function mountSection(id: SettingsSection): void {
    if (mounted.has(id)) return;
    mounted.add(id);
    const panel = panels.get(id)!;
    if (id === 'general') cleanups.push(mountOverlaySettings(panel.querySelector<HTMLElement>('#launcher-settings')!));
    if (id === 'github' && options.github) cleanups.push(options.github(panel.querySelector<HTMLElement>('#github-settings')!));
    if (id === 'teams' && options.teams) cleanups.push(mountAgentTeams(panel.querySelector<HTMLElement>('#agent-teams-settings')!, { ...options.teams, providers: options.providers }));
    if (id === 'cloud' && options.cloud) cleanups.push(mountCloudSettings(panel.querySelector<HTMLElement>('#cloud-settings')!, options.cloud));
    if (id === 'about') {
      cleanups.push(mountSupportDetails(panel.querySelector<HTMLDetailsElement>('.support-details')!));
      cleanups.push(mountAboutSettings(panel.querySelector<HTMLElement>('.settings-about')!));
      cleanups.push(mountResourceUsage(panel.querySelector<HTMLElement>('.settings-resources')!));
    }
    if (id === 'agents' && !discovered) void refresh();
    for (const slot of panel.querySelectorAll<HTMLElement>('[data-embed]')) {
      const embed = options.embed[slot.dataset.embed as keyof SettingsOptions['embed']];
      slot.innerHTML = '<p class="dialog-loading">Loading…</p>';
      embed(slot).then(cleanup => { if (disposed) cleanup(); else cleanups.push(cleanup); })
        .catch(() => { if (!disposed) slot.innerHTML = '<p class="settings-embed-error" role="alert">This section couldn’t load. Close Settings and try again.</p>'; });
    }
  }

  function show(id: SettingsSection, focus = false): void {
    if (disposed) return;
    tabs.forEach(tab => { const on = tab.dataset.section === id; tab.setAttribute('aria-selected', String(on)); tab.tabIndex = on ? 0 : -1; });
    panels.forEach((panel, key) => { panel.hidden = key !== id; });
    mountSection(id);
    host.querySelector<HTMLElement>('.settings-content')!.scrollTop = 0;
    if (focus) tabs.find(tab => tab.dataset.section === id)?.focus({ preventScroll: true });
  }

  const click = (event: MouseEvent) => {
    const target = event.target instanceof Element ? event.target.closest<HTMLElement>('[data-section], [data-settings-close]') : null;
    if (!target || !host.contains(target)) return;
    if ('settingsClose' in target.dataset) { options.close(); return; }
    show(target.dataset.section as SettingsSection);
  };
  /** The group list moves like a native sidebar: arrow keys, Home and End. */
  const keydown = (event: KeyboardEvent) => {
    const index = tabs.indexOf(event.target as HTMLButtonElement);
    if (index < 0 || !['ArrowUp', 'ArrowDown', 'Home', 'End'].includes(event.key)) return;
    event.preventDefault();
    const next = event.key === 'Home' ? 0 : event.key === 'End' ? tabs.length - 1 : (index + (event.key === 'ArrowDown' ? 1 : -1) + tabs.length) % tabs.length;
    show(tabs[next].dataset.section as SettingsSection, true);
  };
  updateProviders(options.providers);
  host.addEventListener('click', click);
  host.addEventListener('keydown', keydown);
  refreshButton.addEventListener('click', refresh);
  show(options.section ?? 'general', true);
  return {
    show: id => show(id, true),
    dispose: () => {
      if (disposed) return;
      disposed = true; readinessCleanup.forEach(cleanup => cleanup());
      cleanups.forEach(cleanup => cleanup());
      host.removeEventListener('click', click); host.removeEventListener('keydown', keydown);
      refreshButton.removeEventListener('click', refresh);
      host.replaceChildren(); host.classList.remove('settings-page-root');
    },
  };
}
