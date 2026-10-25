import test from 'node:test';
import assert from 'node:assert/strict';
import { historicalProvider } from '../src/provider-handoff.ts';

test('saved author metadata survives a page without its future handoff', () => {
  const response = { id: 4, sessionId: 'task', role: 'assistant', content: 'Earlier answer', createdAt: 1, authorProvider: 'claude' };
  assert.equal(historicalProvider(response, [response], 'codex'), 'claude');
  assert.equal(historicalProvider({ ...response, authorProvider: 'javascript:alert(1)' }, [response], 'codex'), 'codex');
});

test('legacy pages infer only handoffs belonging to the same task', () => {
  const response = { id: 4, sessionId: 'task', role: 'assistant', content: 'Earlier answer', createdAt: 1 };
  const handoff = { id: 5, sessionId: 'other', role: 'handoff', content: JSON.stringify({fromProvider:'grok',toProvider:'codex',summary:'Switched harness'}), createdAt: 2 };
  assert.equal(historicalProvider(response, [response, handoff], 'codex'), 'codex');
  assert.equal(historicalProvider(response, [response, {...handoff,sessionId:'task'}], 'codex'), 'grok');
  assert.equal(historicalProvider({...response,id:-1}, [response, {...handoff,sessionId:'task'}], 'codex'), 'codex');
});
