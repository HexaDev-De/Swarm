/**
 * Crisp theme-aware Lucide/Linear-style SVG icons for PandaCode.
 * All icons use viewBox="0 0 24 24" with stroke="currentColor".
 */

export const iconPaths: Record<string, string> = {
  // Core Actions & UI
  file: '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/>',
  folder: '<path d="M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.93a2 2 0 0 1-1.66-.9l-.82-1.2A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z"/>',
  search: '<circle cx="11" cy="11" r="8"/><path d="m21 21-4.3-4.3"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  close: '<path d="M18 6 6 18M6 6l12 12"/>',
  check: '<path d="m5 13 4 4L19 7"/>',
  arrow: '<path d="M5 12h14M12 5l7 7-7 7"/>',
  down: '<path d="m6 9 6 6 6-6"/>',
  up: '<path d="m18 15-6-6-6 6"/>',
  chevron: '<path d="m9 18 6-6-6-6"/>',
  chevronLeft: '<path d="m15 18-6-6 6-6"/>',
  chevronRight: '<path d="m9 18 6-6-6-6"/>',
  settings: '<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/>',
  brain: '<path d="M12 5a3 3 0 1 0-5.997.125 4 4 0 0 0-2.526 5.77 4 4 0 0 0 .556 6.588A4 4 0 1 0 12 18Z"/><path d="M12 5a3 3 0 1 1 5.997.125 4 4 0 0 1 2.526 5.77 4 4 0 0 1-.556 6.588A4 4 0 1 1 12 18Z"/><path d="M12 5v13"/>',

  // Replacement Icons for Unicode Emojis
  zap: '<polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"/>', // ⚡
  swords: '<path d="m14.5 17.5 3 3 3-3-3-3m-3 3-9-9m3-3 3 3-3 3-3-3m3 3-9 9m1.5-1.5L2 22l4.5-1.5m10-10L22 2l-4.5 1.5"/>', // ⚔️
  rewind: '<polygon points="11 19 2 12 11 5 11 19"/><polygon points="22 19 13 12 22 5 22 19"/>', // ⏪
  hourglass: '<path d="M5 22h14M5 2h14m-2 2v5l-5 5 5 5v5M7 4v5l5 5-5 5v5"/>', // ⏳
  clock: '<circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/>',
  refresh: '<path d="M20 7v5h-5M4 17v-5h5M5 8a8 8 0 0 1 13-3l2 3M4 16l2 3a8 8 0 0 0 13-3"/>', // 🔄
  edit: '<path d="m14 5 5 5M4 20l5-1L21 7l-5-5L4 14Z"/>', // ✏️
  pencil: '<path d="m14 5 5 5M4 20l5-1L21 7l-5-5L4 14Z"/>',
  mic: '<path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2M12 19v3M8 22h8"/>', // 🎙️
  message: '<path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/>', // 💬
  chat: '<path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/>',
  target: '<circle cx="12" cy="12" r="10"/><circle cx="12" cy="12" r="6"/><circle cx="12" cy="12" r="2"/>', // 🎯
  trophy: '<path d="M6 9H4.5a2.5 2.5 0 0 1 0-5H6M18 9h1.5a2.5 2.5 0 0 0 0-5H18M4 22h16M10 14.66V17c0 .55-.45 1-1 1H8v4h8v-4h-1c-.55 0-1-.45-1-1v-2.34M18 2H6v7a6 6 0 0 0 12 0V2Z"/>', // 🏆
  spark: '<path d="m12 3 2.5 6.5L21 12l-6.5 2.5L12 21l-2.5-6.5L3 12l6.5-2.5Z"/>', // ✨
  alert: '<path d="m12 3 10 18H2Zm0 6v5m0 3v1"/>', // ⚠️
  trash: '<path d="M4 7h16M9 7V4h6v3m-8 0 1 13h8l1-13"/>', // 🗑️
  pin: '<path d="m16 3 5 5-4 2-3 5-2-2-6 6m3-9L7 8l5-3Z"/>', // 📍
  blueprint: '<path d="M3 21h18M3 7v14M21 7v14M6 21V10a2 2 0 0 1 2-2h8a2 2 0 0 1 2 2v11M10 12h4M10 16h4"/>', // 📐
  ruler: '<path d="m21.3 8.7-5-5a3.1 3.1 0 0 0-4.3 0L2.7 13a3.1 3.1 0 0 0 0 4.3l5 5a3.1 3.1 0 0 0 4.3 0l9.3-9.3a3.1 3.1 0 0 0 0-4.3ZM7.5 18l-1.5-1.5M10.5 15l-1.5-1.5M13.5 12l-1.5-1.5M16.5 9l-1.5-1.5"/>',
  lightbulb: '<path d="M9 18h6M10 22h4M12 2a7 7 0 0 0-4.95 11.95c.78.78 1.45 1.8 1.65 2.9h6.6c.2-1.1.87-2.12 1.65-2.9A7 7 0 0 0 12 2Z"/>', // 💡
  palette: '<circle cx="13.5" cy="6.5" r=".5" fill="currentColor"/><circle cx="17.5" cy="10.5" r=".5" fill="currentColor"/><circle cx="8.5" cy="7.5" r=".5" fill="currentColor"/><circle cx="6.5" cy="12.5" r=".5" fill="currentColor"/><path d="M12 2C6.5 2 2 6.5 2 12s4.5 10 10 10c.93 0 1.5-.7 1.5-1.5 0-.39-.15-.74-.39-1-.23-.27-.38-.62-.38-1 0-.83.67-1.5 1.5-1.5H16c3.31 0 6-2.69 6-6 0-4.97-4.48-9-10-9Z"/>', // 🎨
  code: '<polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/>', // 💻
  play: '<polygon points="5 3 19 12 5 21 5 3"/>', // ▶️
  eye: '<path d="2 12s3-7 10-7 10 7 10 7-3 7-10 7-10-7-10-7Z"/><circle cx="12" cy="12" r="3"/>', // 👀
  diff: '<path d="M12 3v18M3 12h18"/><circle cx="12" cy="12" r="10"/>',
  split: '<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M12 3v18"/>',
  gitCommit: '<circle cx="12" cy="12" r="4"/><line x1="1.05" y1="12" x2="7" y2="12"/><line x1="17.01" y1="12" x2="22.96" y2="12"/>',
  gitBranch: '<line x1="6" y1="3" x2="6" y2="15"/><circle cx="18" cy="6" r="3"/><circle cx="6" cy="18" r="3"/><path d="M18 9a9 9 0 0 1-9 9"/>',

  // Navigation & Framework Elements
  moon: '<path d="M20.5 13A8.5 8.5 0 0 1 11 3.5 8.5 8.5 0 1 0 20.5 13Z"/>',
  sun: '<circle cx="12" cy="12" r="4"/><path d="M12 2v2m0 16v2M2 12h2m16 0h2M5 5l1.5 1.5m11 11L19 19M5 19l1.5-1.5m11-11L19 5"/>',
  bot: '<rect x="4" y="8" width="16" height="12" rx="4"/><path d="M12 4v4M9 13v1m6-1v1M2 13v3m20-3v3"/>',
  panel: '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M15 4v16"/>',
  sidebar: '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M9 4v16"/>',
  attach: '<path d="m8 13 7-7a3 3 0 0 1 4 4L9 20a5 5 0 0 1-7-7L13 2m-5 11 7-7"/>',
  pause: '<path d="M9 5v14M15 5v14"/>',
  stop: '<rect x="6" y="6" width="12" height="12" rx="2"/>',
  copy: '<rect x="8" y="8" width="12" height="13" rx="2"/><path d="M16 8V3H3v13h5"/>',
  info: '<circle cx="12" cy="12" r="9"/><path d="M12 11v6m0-10v.01"/>',
  help: '<circle cx="12" cy="12" r="9"/><path d="M9.5 8.5a2.5 2.5 0 0 1 5 .5c0 2-2.5 2-2.5 4m0 3v.01"/>',
  globe: '<circle cx="12" cy="12" r="9"/><ellipse cx="12" cy="12" rx="4" ry="9"/><path d="M3 12h18"/>',
  back: '<path d="m11 5-7 7 7 7M4 12h16"/>',
  forward: '<path d="m13 5 7 7-7 7M20 12H4"/>',
  camera: '<path d="m8 5 2-2h4l2 2h4v15H4V5Z"/><circle cx="12" cy="12" r="4"/>',
  select: '<path d="m5 3 14 10-7 1-3 7Z"/><path d="m14 15 4 5"/>',
  server: '<rect x="3" y="4" width="18" height="7" rx="2"/><rect x="3" y="13" width="18" height="7" rx="2"/><path d="M7 7.5h.01M7 16.5h.01M11 7.5h6M11 16.5h6"/>',
  issue: '<circle cx="12" cy="12" r="9"/><circle cx="12" cy="12" r="1.6" fill="currentColor" stroke="none"/>',
  monitor: '<rect x="3" y="4" width="18" height="12.5" rx="2.2"/><path d="M8.5 20.5h7M12 16.5v4"/>',
  phone: '<rect x="6.5" y="2.5" width="11" height="19" rx="2.8"/><path d="M10.5 5.3h3"/>',
  window: '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M3 9h18M7 6.5h.01M10 6.5h.01M13 6.5h.01"/>',
  lock: '<rect x="5" y="10.5" width="14" height="10" rx="2.2"/><path d="M8 10.5V7.5a4 4 0 0 1 8 0v3"/>',
  merge: '<circle cx="18" cy="18" r="2.5"/><circle cx="6" cy="6" r="2.5"/><path d="M6 21V8.5M6 8.5a9.5 9.5 0 0 0 9.5 9.5"/>',
  pull: '<circle cx="6" cy="5" r="2"/><circle cx="6" cy="19" r="2"/><circle cx="18" cy="19" r="2"/><path d="M6 7v10M18 17v-6a3 3 0 0 0-3-3h-4m3-3-3 3 3 3"/>',
  upload: '<path d="M12 16V5m-5 5 5-5 5 5M4 19h16"/>',
  download: '<path d="M12 4v11m-5-5 5 5 5-5M4 19h16"/>',
  sketch: '<path d="M4 20h16"/><path d="m5 16 9.5-9.5a2.1 2.1 0 0 1 3 3L8 19l-4 1Z"/>',
  square: '<rect x="4" y="4" width="16" height="16" rx="2"/>',
  circle: '<circle cx="12" cy="12" r="8"/>',
  line: '<path d="m5 19 14-14"/>',
  minimize: '<path d="m4 14 6 6m0-6H4m6 0v6m10-10-6-6m0 6h6m-6 0V4"/>',
  expand: '<path d="m15 3 6 6m0-6h-6m6 0v6M9 21l-6-6m0 6h6m-6 0v-6"/>',
  maximize: '<path d="M8 3H5a2 2 0 0 0-2 2v3m18 0V5a2 2 0 0 0-2-2h-3m0 18h3a2 2 0 0 0 2-2v-3M3 16v3a2 2 0 0 0 2 2h3"/>',
  layout: '<rect width="18" height="18" x="3" y="3" rx="2"/><path d="M3 9h18M9 21V9"/>',
  'arrow-right': '<path d="M5 12h14m-6-6 6 6-6 6"/>',
  text: '<path d="M5 6h14M12 6v14m-4 0h8"/>',
  undo: '<path d="M9 14 4 9l5-5"/><path d="M4 9h10a6 6 0 0 1 0 12h-3"/>',
  redo: '<path d="m15 14 5-5-5-5"/><path d="M20 9H10a6 6 0 0 0 0 12h3"/>',
  move: '<path d="M12 3v18M3 12h18M8 7l4-4 4 4M8 17l4 4 4-4M7 8l-4 4 4 4m10-8 4 4-4 4"/>',
  grid: '<path d="M3 9h18M3 15h18M9 3v18M15 3v18"/>',
  widget: '<rect x="3" y="5" width="18" height="6" rx="1.5"/><rect x="3" y="14" width="10" height="5" rx="1.5"/><path d="M16 16.5h5"/>',
  command: '<path d="M8 8h8v8H8Zm0 0H5a3 3 0 1 1 3-3Zm8 0V5a3 3 0 1 1 3 3Zm0 8h3a3 3 0 1 1-3 3Zm-8 0v3a3 3 0 1 1-3-3Z"/>',
  leaf: '<path d="M19 3C5 3 2 9 5 15s14 4 14-12ZM5 20l9-11"/>',
  bamboo: '<path d="M9 3v18m-2-6h4M7 8h4m5 5v8m-2-4h4"/><path d="M10 9c1-4 4-6 9-6-1 4-4 6-9 6ZM8 15c-4 0-6-2-6-6 4 0 6 2 6 6Z" fill="currentColor" stroke="none"/>',
  github: '<path d="M9 19c-4.3 1.4-4.3-2.5-6-3m12 5v-3.5c0-1 .1-1.4-.5-2 2.8-.3 5.5-1.4 5.5-6a4.6 4.6 0 0 0-1.3-3.2 4.2 4.2 0 0 0-.1-3.2s-1.1-.3-3.5 1.3a12.3 12.3 0 0 0-6.2 0C6.5 2.8 5.4 3.1 5.4 3.1a4.2 4.2 0 0 0-.1 3.2A4.6 4.6 0 0 0 4 9.5c0 4.6 2.7 5.7 5.5 6-.6.6-.6 1.2-.5 2V21"/>',
  external: '<path d="M14 4h6v6M20 4l-9 9M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5"/>',
  reset: '<path d="M4 4v6h6M4.8 9a8 8 0 1 1-.3 6"/>',
};

export function renderSvgIcon(name: string, cls = ''): string {
  const content = iconPaths[name] ?? iconPaths.file;
  return `<svg class="icon ${cls}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${content}</svg>`;
}
