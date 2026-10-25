/* Settings → Agents → Cloud sandboxes: connect Daytona, Cloudflare and AWS so agents
 * can create and use cloud machines, and Vercel, Railway and Google Cloud for deploys. Keys go straight to the OS keychain natively;
 * this view never reads them back. */
import './cloud-settings.css';
import { escape } from './dom';

export type CloudConnection = { provider: 'daytona' | 'cloudflare' | 'aws' | 'vercel' | 'railway' | 'gcloud'; connected: boolean; detail: string | null; sandboxes?: boolean };
export type CloudSettingsHost = {
  call<T>(command: string, args?: Record<string, unknown>): Promise<T>;
  listen(name: 'cloud-setup', callback: (event: { provider: string; label: string }) => void): Promise<() => void>;
};

type Field = { name: string; label: string; placeholder: string; secret?: boolean; optional?: boolean };
const PROVIDERS: Record<CloudConnection['provider'], { name: string; summary: string; key?: Field; fields: Field[]; note: string; connect: string }> = {
  daytona: {
    name: 'Daytona', summary: 'Linux desktops agents can see and control.',
    key: { name: 'key', label: 'API key', placeholder: 'dtn_…', secret: true },
    fields: [{ name: 'organization', label: 'Organization ID', placeholder: 'Optional', optional: true }, { name: 'apiUrl', label: 'API URL', placeholder: 'https://app.daytona.io/api', optional: true }],
    note: 'Create a key at app.daytona.io → Keys. Sandboxes stop after 15 idle minutes.', connect: 'Connect',
  },
  cloudflare: {
    name: 'Cloudflare', summary: 'Deploys to Workers and Pages, and container sandboxes.',
    key: { name: 'key', label: 'API token', placeholder: 'Workers Scripts, Pages and Containers: Edit', secret: true },
    fields: [{ name: 'account', label: 'Account ID', placeholder: '32 hexadecimal characters' }],
    note: 'Agents deploy with Wrangler using this token. Container sandboxes are a separate one-time setup that deploys a small Swarm Worker; it needs Docker running and a Workers Paid plan.', connect: 'Connect',
  },
  aws: {
    name: 'AWS', summary: 'EC2 Linux and Windows desktops, and AgentCore.',
    fields: [{ name: 'region', label: 'Region', placeholder: 'us-east-1' }, { name: 'profile', label: 'CLI profile', placeholder: 'Optional', optional: true }],
    note: 'Uses your AWS CLI sign-in (run `aws login` first). Desktops have no open ports; Swarm reaches them through Systems Manager, which needs the Session Manager plugin for screens.', connect: 'Connect',
  },
  vercel: {
    name: 'Vercel', summary: 'Deploys by agents, preview and production.',
    key: { name: 'key', label: 'Access token', placeholder: 'vercel.com/account/tokens', secret: true },
    fields: [{ name: 'team', label: 'Team slug', placeholder: 'Optional', optional: true }],
    note: 'Agents run the Vercel CLI with this token; it never enters their prompts.', connect: 'Connect',
  },
  railway: {
    name: 'Railway', summary: 'Deploys by agents: servers, containers and databases.',
    key: { name: 'key', label: 'API token', placeholder: 'railway.com/account/tokens', secret: true },
    fields: [{ name: 'workspace', label: 'Workspace ID', placeholder: 'Optional; needed for a workspace token', optional: true }],
    note: 'Use an account token, or a workspace token with its workspace ID. Agents run the Railway CLI with it (fetched on first use if it is not installed); it never enters their prompts.', connect: 'Connect',
  },
  gcloud: {
    name: 'Google Cloud', summary: 'Deploys by agents: Cloud Run and Cloud Storage.',
    fields: [{ name: 'project', label: 'Project ID', placeholder: 'my-app-123456' }, { name: 'region', label: 'Region', placeholder: 'Optional, such as us-central1', optional: true }],
    note: 'Uses your installed gcloud CLI and its sign-in (run `gcloud auth login` first); Swarm downloads nothing and stores only the project and region.', connect: 'Connect',
  },
};

export function mountCloudSettings(host: HTMLElement, options: CloudSettingsHost): () => void {
  host.innerHTML = `<div class="settings-group cloud-settings" aria-label="Cloud sandboxes">${(Object.keys(PROVIDERS) as CloudConnection['provider'][]).map(id => {
    const provider = PROVIDERS[id];
    const fields = [...(provider.key ? [provider.key] : []), ...provider.fields];
    return `<div class="cloud-provider" data-provider="${id}">
      <div class="setting-row"><div><strong>${escape(provider.name)}</strong><small class="cloud-detail">${escape(provider.summary)}</small></div><div class="cloud-buttons">${id === 'cloudflare' ? '<button type="button" class="secondary-button" data-cloud="sandboxes" hidden>Set up sandboxes</button>' : ''}<button type="button" class="secondary-button" data-cloud="toggle">Connect</button></div></div>
      <p class="cloud-progress cloud-row-status" role="status" hidden></p>
      <form class="cloud-form" hidden novalidate aria-label="${escape(`Connect ${provider.name}`)}">
        ${fields.map(field => `<label><span>${escape(field.label)}</span><input name="${field.name}" type="${field.secret ? 'password' : 'text'}" autocomplete="off" spellcheck="false" placeholder="${escape(field.placeholder)}" aria-label="${escape(`${provider.name} ${field.label}`)}" ${field.optional ? '' : 'required'}></label>`).join('')}
        <p class="cloud-note">${escape(provider.note)}</p>
        <p class="cloud-progress" role="status" hidden></p>
        <p class="cloud-error" role="alert" hidden></p>
        <div class="cloud-actions"><button type="button" class="secondary-button" data-cloud="cancel">Cancel</button><button type="submit" class="primary-button">${escape(provider.connect)}</button></div>
      </form>
    </div>`;
  }).join('')}</div>`;
  let connections: CloudConnection[] = [], disposed = false, busy = '';
  let stopListening: (() => void) | undefined;
  const section = (id: string) => host.querySelector<HTMLElement>(`.cloud-provider[data-provider="${id}"]`)!;

  function render(): void {
    for (const id of Object.keys(PROVIDERS) as CloudConnection['provider'][]) {
      const connection = connections.find(c => c.provider === id), root = section(id);
      const toggle = root.querySelector<HTMLButtonElement>('[data-cloud="toggle"]')!;
      const form = root.querySelector<HTMLFormElement>('form')!;
      const sandboxes = id === 'cloudflare' && connection?.connected ? (connection.sandboxes ? ' · sandboxes ready' : ' · sandboxes not set up') : '';
      root.querySelector('.cloud-detail')!.textContent = connection?.connected ? `Connected${connection.detail ? ` · ${connection.detail}` : ''}${sandboxes}` : PROVIDERS[id].summary;
      const setup = root.querySelector<HTMLButtonElement>('[data-cloud="sandboxes"]');
      if (setup) { setup.hidden = !connection?.connected || Boolean(connection.sandboxes); setup.disabled = Boolean(busy); }
      toggle.textContent = connection?.connected ? 'Disconnect' : form.hidden ? 'Connect' : 'Close';
      toggle.disabled = Boolean(busy);
      toggle.setAttribute('aria-label', `${toggle.textContent} ${PROVIDERS[id].name}`);
      form.querySelector<HTMLButtonElement>('[type="submit"]')!.disabled = Boolean(busy);
    }
  }
  async function load(): Promise<void> {
    try { connections = await options.call<CloudConnection[]>('cloud_connections'); } catch { connections = []; }
    if (!disposed) render();
  }
  function say(root: HTMLElement, kind: 'progress' | 'error', text: string): void {
    const line = root.querySelector<HTMLElement>(`.cloud-${kind}`)!;
    line.textContent = text; line.hidden = !text;
  }
  async function connect(id: CloudConnection['provider'], form: HTMLFormElement): Promise<void> {
    const root = section(id), data = new FormData(form);
    const value = (name: string) => String(data.get(name) ?? '').trim();
    const missing = [...(PROVIDERS[id].key ? [PROVIDERS[id].key!] : []), ...PROVIDERS[id].fields].find(field => !field.optional && !value(field.name));
    if (missing) { say(root, 'error', `Enter the ${missing.label.toLowerCase()}.`); return; }
    busy = id; say(root, 'error', ''); say(root, 'progress', id === 'cloudflare' ? 'Preparing the deploy…' : 'Checking…'); render();
    try {
      const fields = Object.fromEntries(PROVIDERS[id].fields.map(field => [field.name, value(field.name)]));
      connections = await options.call<CloudConnection[]>('cloud_connect', { provider: id, key: value('key'), options: fields });
      if (disposed) return;
      form.reset(); form.hidden = true;
      say(root, 'progress', '');
    } catch (error) {
      if (!disposed) { say(root, 'progress', ''); say(root, 'error', error instanceof Error ? error.message : String(error)); }
    } finally { busy = ''; if (!disposed) render(); }
  }
  async function setupSandboxes(root: HTMLElement): Promise<void> {
    const line = root.querySelector<HTMLElement>('.cloud-row-status')!;
    busy = 'cloudflare'; line.hidden = false; line.textContent = 'Preparing the sandbox Worker…'; render();
    try { connections = await options.call<CloudConnection[]>('cloud_setup_sandboxes'); line.textContent = 'Sandboxes are ready.'; }
    catch (error) { line.textContent = error instanceof Error ? error.message : String(error); line.classList.add('cloud-error'); }
    finally { busy = ''; if (!disposed) render(); }
  }
  async function disconnect(id: CloudConnection['provider']): Promise<void> {
    busy = id; render();
    try { connections = await options.call<CloudConnection[]>('cloud_disconnect', { provider: id }); }
    catch (error) { say(section(id), 'error', error instanceof Error ? error.message : String(error)); }
    finally { busy = ''; if (!disposed) render(); }
  }

  host.addEventListener('click', event => {
    const target = event.target instanceof Element ? event.target.closest<HTMLButtonElement>('[data-cloud]') : null;
    if (!target || target.disabled) return;
    const root = target.closest<HTMLElement>('.cloud-provider')!, id = root.dataset.provider as CloudConnection['provider'];
    const form = root.querySelector<HTMLFormElement>('form')!;
    if (target.dataset.cloud === 'cancel') { form.hidden = true; say(root, 'error', ''); render(); return; }
    if (target.dataset.cloud === 'sandboxes') { void setupSandboxes(root); return; }
    if (connections.find(c => c.provider === id)?.connected) { void disconnect(id); return; }
    form.hidden = !form.hidden;
    render();
    if (!form.hidden) form.querySelector<HTMLInputElement>('input')!.focus();
  });
  host.addEventListener('submit', event => {
    event.preventDefault();
    const form = event.target as HTMLFormElement, id = form.closest<HTMLElement>('.cloud-provider')!.dataset.provider as CloudConnection['provider'];
    void connect(id, form);
  });
  host.addEventListener('keydown', event => {
    const form = (event.target as Element).closest?.('form.cloud-form') as HTMLFormElement | null;
    // Escape closes the form, not the Settings dialog around it.
    if (form && event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); form.hidden = true; render(); }
  });
  void options.listen('cloud-setup', event => {
    if (event.provider !== busy) return;
    const line = section(busy).querySelector<HTMLElement>('.cloud-row-status')!;
    line.hidden = false; line.classList.remove('cloud-error'); line.textContent = event.label;
  }).then(stop => { if (disposed) stop(); else stopListening = stop; });
  void load();
  return () => { disposed = true; stopListening?.(); host.replaceChildren(); };
}
