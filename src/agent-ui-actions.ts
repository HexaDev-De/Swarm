import { call, subscribe } from './bridge';

export interface AgentUiAction { requestId: string; sessionId: string; action: string; args: Record<string, unknown>; }
export async function connectAgentUiActions(activeTask: (sessionId: string) => boolean | string, perform: (request: AgentUiAction) => Promise<unknown>): Promise<() => void> {
  // Serialize visible actions: simultaneous agents cannot race the same browser.
  let queue = Promise.resolve();
  return subscribe('agent-ui-action', (request: AgentUiAction) => {
    queue = queue.catch(() => {}).then(async () => {
      try {
        if (!await call<boolean>('agent_ui_request_active', { requestId: request.requestId })) return;
        const check = activeTask(request.sessionId);
        const active = typeof check === 'boolean' ? check : check === request.sessionId;
        if (!active) throw new Error('Open this task before using its app or browser tools.');
        const result = await perform(request);
        await call('resolve_agent_ui_action', { requestId: request.requestId, result });
      } catch (error) {
        await call('resolve_agent_ui_action', { requestId: request.requestId, error: String(error) }).catch(() => {});
      }
    });
  });
}
