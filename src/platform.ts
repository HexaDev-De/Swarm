/** System webviews expose the host platform without another native round trip. */
export const isMac = /Mac|iPhone|iPad|iPod/i.test(navigator.platform);
export const isWindows = /Win/i.test(navigator.platform);
export const commandKey = isMac ? '⌘' : 'Ctrl';

export function shortcut(key: string, shift = false): string {
  return `${commandKey}${shift ? isMac ? ' ⇧' : ' Shift' : ''} ${key}`;
}
