import { test } from 'node:test';
import assert from 'node:assert/strict';
import extension from '../src-tauri/src/providers/pi-extension.mjs';

async function permission(mode, toolName, approved = false) {
  process.env.PANDACODE_PI_BRIDGE = JSON.stringify({ mode });
  let hook;
  let asked = false;
  await extension({ on: (name, handler) => { if (name === 'tool_call') hook = handler; } });
  const result = await hook({ toolName, input: { command: 'echo hello' } }, { ui: { confirm: async () => { asked = true; return approved; } } });
  return { result, asked };
}

test('Pi standard mode asks before writes and commands; declines block tools', async () => {
  for (const name of ['write', 'edit', 'bash', 'custom']) {
    assert.equal((await permission('ask', name)).result.block, true);
    assert.equal((await permission('ask', name, true)).asked, true);
    assert.equal((await permission('ask', name, true)).result, undefined);
  }
  assert.deepEqual(await permission('ask', 'read'), { result: undefined, asked: false });
});

test('Pi plan denies side effects, auto allows file edits, full allows commands', async () => {
  for (const name of ['write', 'edit', 'bash', 'custom']) {
    assert.deepEqual(await permission('plan', name), { result: { block: true, reason: 'This tool is unavailable in Plan mode.' }, asked: false });
  }
  for (const name of ['read', 'grep', 'find', 'ls']) assert.equal((await permission('plan', name)).result, undefined);
  assert.equal((await permission('auto', 'edit')).asked, false);
  assert.equal((await permission('auto', 'bash')).result.block, true);
  assert.equal((await permission('full', 'bash')).asked, false);
});

test('Pi blocks tools when permission UI fails', async () => {
  process.env.PANDACODE_PI_BRIDGE = JSON.stringify({ mode: 'ask' });
  let hook;
  await extension({ on: (name, handler) => { if (name === 'tool_call') hook = handler; } });
  const result = await hook({ toolName: 'bash', input: {} }, { ui: { confirm: () => { throw new Error('Disconnected'); } } });
  assert.equal(result.block, true);
});

test('Pi app tools use authenticated MCP and report tool failures', async () => {
  const { createServer } = await import('node:http');
  const requests = [];
  const server = createServer(async (req, res) => {
    let body = '';
    for await (const chunk of req) body += chunk;
    const request = JSON.parse(body);
    requests.push({ request, auth: req.headers.authorization });
    const result = request.method === 'tools/list'
      ? { tools: [{ name: 'pandacode_test', description: 'Test tool.', inputSchema: { type: 'object', properties: {} } }] }
      : { content: [{ type: 'text', text: 'Host failure' }], isError: true };
    res.setHeader('Content-Type', 'application/json');
    res.end(JSON.stringify({ jsonrpc: '2.0', id: 1, result }));
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  try {
    process.env.PANDACODE_PI_BRIDGE = JSON.stringify({ mode: 'ask', url: `http://127.0.0.1:${server.address().port}/mcp`, authorization: 'Bearer test' });
    let tool;
    await extension({ registerTool: spec => { tool = spec; }, on: () => {} });
    assert.equal(tool.name, 'pandacode_test');
    await assert.rejects(tool.execute('id', {}), /Host failure/);
    assert.deepEqual(requests.map(item => item.request.method), ['tools/list', 'tools/call']);
    assert.ok(requests.every(item => item.auth === 'Bearer test'));
  } finally {
    server.closeAllConnections();
    await new Promise(resolve => server.close(resolve));
  }
});

test('Pi removes the bridge token from the environment before any request', async () => {
  process.env.PANDACODE_PI_BRIDGE = JSON.stringify({ mode: 'ask', url: 'http://127.0.0.1:9/mcp', authorization: 'Bearer secret' });
  let seen;
  const fetch = globalThis.fetch;
  globalThis.fetch = async () => { seen = process.env.PANDACODE_PI_BRIDGE; throw new Error('offline'); };
  try {
    await assert.rejects(extension({ registerTool: () => {}, on: () => {} }), /offline/);
  } finally {
    globalThis.fetch = fetch;
  }
  assert.equal(seen, undefined);
  assert.equal(process.env.PANDACODE_PI_BRIDGE, undefined);
});

test('Pi plan mode blocks app tools except read-only ones; other modes keep app tools', async () => {
  const fetch = globalThis.fetch;
  globalThis.fetch = async () => ({ ok: true, json: async () => ({ result: { tools: ['pandacode_dispatch_workers', 'pandacode_vm_exec', 'pandacode_list_tasks'].map(name => ({ name, inputSchema: {} })) } }) });
  try {
    for (const mode of ['plan', 'ask', 'auto', 'full']) {
      process.env.PANDACODE_PI_BRIDGE = JSON.stringify({ mode, url: 'http://127.0.0.1:9/mcp', authorization: 'Bearer test' });
      let hook;
      await extension({ registerTool: () => {}, on: (name, handler) => { if (name === 'tool_call') hook = handler; } });
      const ui = { confirm: async () => { throw new Error('app tools must not prompt here'); } };
      for (const toolName of ['pandacode_dispatch_workers', 'pandacode_vm_exec']) {
        const result = await hook({ toolName, input: {} }, { ui });
        assert.equal(result?.block === true, mode === 'plan', `${mode} ${toolName}`);
      }
      assert.equal(await hook({ toolName: 'pandacode_list_tasks', input: {} }, { ui }), undefined);
    }
    // An allowlisted name that PandaCode did not register (a user MCP tool) is not trusted.
    process.env.PANDACODE_PI_BRIDGE = JSON.stringify({ mode: 'plan' });
    let hook;
    await extension({ on: (name, handler) => { if (name === 'tool_call') hook = handler; } });
    assert.equal((await hook({ toolName: 'pandacode_list_tasks', input: {} }, {})).block, true);
  } finally {
    globalThis.fetch = fetch;
  }
});
