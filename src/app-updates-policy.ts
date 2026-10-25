/* The updater's decisions, kept free of app imports so they can be tested on their own:
 * how often to check, when a downloaded update may restart PandaCode, and how Settings
 * describes each state. */
export type UpdateState =
  | { kind: 'off' }
  | { kind: 'idle' }
  | { kind: 'checking' }
  | { kind: 'current'; checkedAt: number }
  | { kind: 'available'; version: string }
  | { kind: 'downloading'; version: string; percent: number | null }
  | { kind: 'ready'; version: string }
  | { kind: 'installing'; version: string }
  | { kind: 'error'; message: string };

export const CHECK_EVERY = 6 * 60 * 60 * 1000;
/** A downloaded update installs after this long without input, when no agent is working. */
export const IDLE_BEFORE_RESTART = 2 * 60 * 1000;

/** Whether a downloaded update may install now: nothing running, and the user stepped away. */
export function readyToRestart(busy: boolean, lastInput: number, now = Date.now()): boolean {
  return !busy && now - lastInput >= IDLE_BEFORE_RESTART;
}

/** One line for Settings → About. */
export function describeUpdate(state: UpdateState, automatic: boolean, now = Date.now()): string {
  switch (state.kind) {
    case 'off': return 'Updates are installed by the desktop app.';
    case 'checking': return 'Checking GitHub Releases…';
    case 'current': { const minutes = Math.round((now - state.checkedAt) / 60_000); return `Up to date · checked ${minutes < 1 ? 'just now' : minutes < 60 ? `${minutes}m ago` : `${Math.round(minutes / 60)}h ago`}`; }
    case 'available': return `Swarm ${state.version} is available.`;
    case 'downloading': return `Downloading ${state.version}${state.percent === null ? '…' : ` · ${state.percent}%`}`;
    case 'ready': return `${state.version} is ready. ${automatic ? 'Swarm restarts when your agents are done and you step away.' : 'Restart to finish.'}`;
    case 'installing': return `Installing ${state.version}…`;
    case 'error': return `Couldn’t check for updates: ${state.message}`;
    default: return automatic ? 'Checks GitHub Releases automatically.' : 'Automatic updates are off.';
  }
}
