import './coordination-message.css';
import type { Message } from './types';
import { icon } from './dom';

/** Only app-origin rows use this renderer; user JSON stays ordinary conversation. */
export function renderCoordination(message: Message): string {
  const label = message.coordinationKind === 'worker_review' ? 'Reviewing worker results'
    : message.coordinationKind === 'worker_update' ? 'Checking worker updates'
    : message.coordinationKind === 'continue' ? 'Continuing the plan'
    : message.coordinationKind === 'worker_failed' ? 'Handling a failed worker' : 'Coordinating team';
  return `<div class="coordination-status" role="note" aria-label="Team status">${icon('worktree')}<strong>Team</strong><span>${label}</span></div>`;
}
