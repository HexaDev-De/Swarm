import type { Mode, ProviderId } from './types';

export interface ApprovalMode { id: Mode; label: string; description: string; icon: string; }

/** Labels describe the native policy for each harness, not a simulated approval layer. */
export function approvalModes(provider: ProviderId): ApprovalMode[] {
  const standard: ApprovalMode = { id: 'ask', label: 'Standard', icon: 'check', description: provider === 'codex'
    ? 'Work in this project. Ask you when an action needs extra permission.'
    : provider === 'grok'
    ? 'Grok asks before edits and commands, whatever its own config says.'
    : provider === 'pi'
    ? 'Pi has no approval system; Swarm asks before edits, commands and other non-read tools.'
    : provider === 'deepseek'
    ? 'DeepSeek edits and runs commands inside this project freely; it asks you before going outside it.'
    : 'Use the harness’s normal rules. Ask you when approval is needed.' };
  const full: ApprovalMode = { id: 'full', label: 'Full access', icon: 'unlock', description: provider === 'antigravity'
    ? 'Automatically approve permission requests. Server restrictions still apply.'
    : provider === 'codex'
    ? 'Skip approval prompts and sandbox limits. Allow file and network access.'
    : provider === 'grok'
    ? 'Always approve: Grok runs every tool without asking (its --always-approve).'
    : provider === 'pi'
    ? 'Pi runs every tool without asking.'
    : provider === 'deepseek'
    ? 'No sandbox: DeepSeek runs every tool without asking.'
    : 'Run tools without approval prompts, including outside this project.' };
  return [standard, provider === 'deepseek'
    ? { id: 'auto', label: 'Accept edits', icon: 'edit', description: 'Like Standard, and edits outside this project are approved too. Other actions still ask.' }
    : ['claude', 'glm', 'mimo', 'opencode', 'grok', 'pi'].includes(provider)
    ? { id: 'auto', label: 'Accept edits', icon: 'edit', description: 'Allow file edits automatically. Other actions may still ask you.' }
    : provider === 'kimi'
    ? { id: 'auto', label: 'Ask when needed', icon: 'spark', description: 'Allow routine tools automatically. Ask you about sensitive actions.' }
    : provider === 'codex' ? { id: 'auto', label: 'Approve for me', icon: 'spark', description: 'Let Codex review approval requests. Keep the workspace sandbox.' }
    : { id: 'auto', label: 'Accept edits', icon: 'edit', description: 'Automatically approve edit requests offered by the server.' },
  provider === 'codex'
    ? { id: 'plan', label: 'Read only', icon: 'plan', description: 'Inspect the project. Block writes and permission escalation.' }
    : provider === 'grok'
    ? { id: 'plan', label: 'Plan mode', icon: 'plan', description: 'Read-only: Grok investigates and proposes a plan; edits and commands are declined.' }
    : provider === 'pi'
    ? { id: 'plan', label: 'Plan mode', icon: 'plan', description: 'Read-only: Pi may use read-only tools; everything else is declined.' }
    : provider === 'deepseek'
    ? { id: 'plan', label: 'Plan mode', icon: 'plan', description: 'Read-only sandbox: DeepSeek cannot write files and proposes a plan; requests to escalate are declined.' }
    : { id: 'plan', label: 'Plan mode', icon: 'plan', description: 'Explore and plan changes before implementation.' }, full];
}

export function approvalMode(provider: ProviderId, mode: Mode): ApprovalMode {
  const modes = approvalModes(provider);
  return modes.find(item => item.id === mode) ?? modes[0];
}
