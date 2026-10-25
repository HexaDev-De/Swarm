export function ultracodeStageName(stage: string | null | undefined): string {
  switch (stage) {
    case 'recon': return 'Recon';
    case 'workers': return 'Workers';
    case 'qa_loop': return 'QA Loop';
    case 'critic_review': return 'Critic Review';
    default: return stage ? stage.charAt(0).toUpperCase() + stage.slice(1) : 'Recon';
  }
}
