import type {ProviderModel} from './types';
import {fallbackModel} from './model-catalog';

// Browser fixtures only; the desktop always reads its installed OpenCode catalog.
export const openCodePreviewModels: ProviderModel[] = [
  {id:'opencode/big-pickle',name:'Big Pickle',description:'opencode/big-pickle',contextWindow:200000},
  {id:'anthropic/claude-sonnet-4-6',name:'Claude Sonnet 4.6',description:'anthropic/claude-sonnet-4-6',contextWindow:200000},
  {id:'openai/gpt-5.4',name:'GPT-5.4',description:'openai/gpt-5.4',contextWindow:1050000},
  {id:'google/gemini-3-pro',name:'Gemini 3 Pro',description:'google/gemini-3-pro',contextWindow:1000000},
  {id:'moonshot/kimi-k2.5',name:'Kimi K2.5',description:'moonshot/kimi-k2.5',contextWindow:256000},
  {id:'zai/glm-5',name:'GLM-5',description:'zai/glm-5',contextWindow:200000},
  {id:'xai/grok-4',name:'Grok 4',description:'xai/grok-4',contextWindow:256000},
];
export const openCodePreviewKey = 'pandacode.preview.opencode-models';
export function previewOpenCodeSelection(): string[] {
  try {
    const stored = JSON.parse(localStorage.getItem(openCodePreviewKey) || '[]');
    return Array.isArray(stored) ? stored.filter((id): id is string => typeof id === 'string' && id.includes('/') && id.length <= 200) : [];
  } catch { return []; }
}
export type OpenCodeModels = {models: ProviderModel[]; selectedModelIds: string[]; error: string | null};
/** Picker choices: the saved Settings selection, or the whole catalog when none is saved. */
export function openCodeChoices({models, selectedModelIds, error}: OpenCodeModels): ProviderModel[] {
  if (!selectedModelIds.length) {
    if (error) throw new Error(error);
    return [fallbackModel('opencode'), ...models.filter(model => model.id !== 'default')];
  }
  return [fallbackModel('opencode'), ...selectedModelIds.map(id => models.find(model => model.id === id) ?? fallbackModel('opencode', id))];
}
export function previewOpenCodeChoices(): ProviderModel[] {
  return openCodeChoices({models: openCodePreviewModels, selectedModelIds: previewOpenCodeSelection(), error: null});
}
