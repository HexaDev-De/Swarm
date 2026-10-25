import type { ProviderId } from './types';

export interface WorkerProfile {
  id: string;
  name: string;
  provider: ProviderId;
  model: string;
  reasoningEffort?: string | null;
}
export interface TeamConfig {
  /** The task's own model leads and picks each worker's model (from `workers`, a roster of
   * installed models, or as the prompt asks). Configured teams name fixed worker profiles. */
  auto?: boolean;
  /** Named agent teams (Settings → Agent teams): the lead may only orchestrate `workers`. */
  rosterOnly?: boolean;
  /** The agent team's name, shown in the model picker and composer. */
  name?: string;
  workers: WorkerProfile[];
  /** Zero means unlimited; positive values preserve a user-selected limit. */
  maxParallel: number;
  maxRounds: number;
  maxAssignments: number;
  workspace: 'shared';
  summaryLimit: number;
}
export type ModelSelection = Pick<WorkerProfile, 'provider' | 'model' | 'reasoningEffort'>;
export interface TeamSetup { architect: ModelSelection; config: TeamConfig }
export interface TeamProfile { id: string; name: string; setup: TeamSetup; updatedAt: number }
export interface TeamLibrary {
  enabled: boolean;
  lastSetup: TeamSetup | null;
  activeProfileId: string | null;
  profiles: TeamProfile[];
}
export interface WorkerCheckpoint {
  summary: string;
  nextSteps: string;
  blocker: string | null;
  updatedAt: number;
}
export interface TeamMessage {
  id: number;
  sender: string;
  recipient: string;
  message: string;
  createdAt: number;
}
export interface TeamMessagePage {
  messages: TeamMessage[];
  nextCursor: number;
  hasMore: boolean;
}
export interface TeamWorker extends WorkerProfile {
  sessionId: string;
  profileId: string;
  status: 'queued' | 'running' | 'waiting' | 'completed' | 'failed' | 'stopped';
  assignment: string;
  summary: string;
  error?: string | null;
  dependsOn: string[];
  files: string[];
  /** Advisory no-edit assignment; legacy workers reserve their declared edit scope. */
  readOnly?: boolean;
  attempt: number;
  abandoned?: boolean;
  checkpoint?: WorkerCheckpoint | null;
  createdAt: number;
  updatedAt: number;
}
export interface TeamSnapshot {
  sessionId: string;
  config: TeamConfig;
  /** Selected during an architect turn; becomes effective at the next turn. */
  pendingSetup?: TeamSetup | null;
  status: 'ready' | 'running' | 'waiting' | 'paused' | 'completed' | 'failed' | 'interrupted';
  round: number;
  revision: number;
  coordinationRevision?: number;
  reviewedCoordinationRevision?: number;
  workers: TeamWorker[];
  error?: string | null;
  summary?: string | null;
  updatedAt: number;
}
export const teamActive = (team?: TeamSnapshot | null): boolean => team?.status === 'running' || team?.status === 'waiting';

export type TeamEvent = TeamSnapshot | { sessionId: string; removed: true };
