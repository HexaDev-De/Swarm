import type { ProviderId } from './types';
import { escape } from './dom';

// Original provider assets; see docs/brand-assets.md for provenance.
const logos: Record<ProviderId, string> = {
  claude: new URL('./assets/providers/claude.svg', import.meta.url).href,
  codex: new URL('./assets/providers/openai.svg', import.meta.url).href,
  kimi: new URL('./assets/providers/kimi.png', import.meta.url).href,
  opencode: new URL('./assets/providers/opencode.png', import.meta.url).href,
  glm: new URL('./assets/providers/glm.svg', import.meta.url).href,
  antigravity: new URL('./assets/providers/antigravity.png', import.meta.url).href,
  grok: new URL('./assets/providers/grok.png', import.meta.url).href,
  mimo: new URL('./assets/providers/mimo.png', import.meta.url).href,
  pi: new URL('./assets/providers/pi.png', import.meta.url).href,
  deepseek: new URL('./assets/providers/deepseek.png', import.meta.url).href,
};

/** Decorative provider mark. The surrounding control supplies its accessible name. */
export function providerLogo(id: ProviderId, cls = ''): string {
  if (!Object.hasOwn(logos, id)) return '';
  return `<img class="provider-logo${cls ? ` ${escape(cls)}` : ''}" data-provider="${id}" src="${escape(logos[id])}" width="20" height="20" alt="" aria-hidden="true" draggable="false">`;
}
