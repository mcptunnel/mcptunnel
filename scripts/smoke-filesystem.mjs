// Real official Filesystem MCP, real packaged CLI, real HTTP, isolated state only.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import net from 'node:net';
import { spawn, spawnSync } from 'node:child_process';
import { once } from 'node:events';
const binary = path.resolve(process.argv[2] ?? 'target/release/mcptunnel' + (process.platform === 'win32' ? '.exe' : ''));
const upstream = path.resolve('target/real-mcp/node_modules/@modelcontextprotocol/server-filesystem/dist/index.js');
const npx = process.argv.includes('--npx');
const quick = process.argv.includes('--quick');
if (!npx) assert(fs.existsSync(upstream), 'Install @modelcontextprotocol/server-filesystem@2026.8.31 under target/real-mcp first');
const temp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcptunnel-filesystem-'));
const directory = path.join(temp, 'shared files');
fs.mkdirSync(directory);
fs.writeFileSync(path.join(directory, 'hello.txt'), 'Hello from the official Filesystem MCP!');
const listener = net.createServer();
listener.listen(0, '127.0.0.1');
await once(listener, 'listening');
const port = listener.address().port;
await new Promise(resolve => listener.close(resolve));
const config = path.join(temp, 'config.toml');
const cloudflared = path.resolve('target/install-cloudflared-smoke/cloudflared' + (process.platform === 'win32' ? '.exe' : ''));
if (quick) assert(fs.existsSync(cloudflared), 'Install cloudflared under target/install-cloudflared-smoke first');
fs.writeFileSync(config, '[daemon]\nbind="127.0.0.1:' + port + '"\n[tunnel]\ndefault_mode="' + (quick ? 'quick' : 'local') + '"\n' + (quick ? 'cloudflared_path=' + JSON.stringify(cloudflared) + '\n' : ''));
const env = { ...process.env, MCPTUNNEL_CONFIG: config, MCPTUNNEL_STATE_DIR: path.join(temp, 'state') };
const cli = args => {
  const result = spawnSync(binary, args, { env, encoding: 'utf8', windowsHide: true, timeout: quick ? 60000 : 30000 });
  assert.equal(result.status, 0, 'CLI ' + args[0] + ' failed: ' + result.stderr);
  return result.stdout;
};
const daemon = spawn(binary, ['daemon', 'start'], { env, windowsHide: true, stdio: 'ignore' });
const exited = once(daemon, 'exit');
try {
  const deadline = Date.now() + 10000;
  while (spawnSync(binary, ['daemon', 'status', '--json'], { env, windowsHide: true, stdio: 'ignore' }).status !== 0) {
    assert(Date.now() < deadline, 'Daemon never became ready');
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  const command = !npx ? [process.execPath, upstream, directory] : ['npx', '-y', '@modelcontextprotocol/server-filesystem@2026.8.31', directory];
  const created = JSON.parse(cli([...(quick ? ['--public'] : []), '--path', '/filesystem', '--probe', 'eager', '--allow-tool', 'read_text_file', '--allow-tool', 'list_directory', '--json', '--', ...command]));
  const { username, password } = created.mount.basic_credentials;
  const url = created.mount.url;
  if (npx) assert.equal(created.mount.slug, 'filesystem');
  if (quick) assert.match(url, /^https:\/\/[a-z0-9-]+\.trycloudflare\.com\/filesystem$/);
  else { assert.equal(url, 'http://localhost:' + port + '/filesystem'); assert.equal(created.mount.public_url, null); }
  if (quick) {
    // A provider URL can precede DNS/edge readiness. Wait for a real health response.
    const deadline = Date.now() + 60000;
    let ready = false;
    while (Date.now() < deadline) {
      const response = await fetch(new URL('/healthz', url), { signal: AbortSignal.timeout(5000) }).catch(() => null);
      if (response?.status === 200) { ready = true; await response.body.cancel(); break; }
      await response?.body?.cancel();
      await new Promise(resolve => setTimeout(resolve, 1000));
    }
    assert(ready, 'Public Quick Tunnel did not become reachable within 60s');
  }
  const headers = { Authorization: 'Basic ' + Buffer.from(username + ':' + password).toString('base64'), Accept: 'application/json, text/event-stream', 'Content-Type': 'application/json', 'MCP-Protocol-Version': '2026-07-28' };
  async function rpc(method, args = {}) {
    const response = await fetch(url, { method: 'POST', headers: { ...headers, 'MCP-Method': method, ...(args.name ? { 'MCP-Name': args.name } : {}) }, signal: AbortSignal.timeout(20000), body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params: { ...args, _meta: { 'io.modelcontextprotocol/protocolVersion': '2026-07-28', 'io.modelcontextprotocol/clientInfo': { name: 'real-filesystem-smoke', version: '1' }, 'io.modelcontextprotocol/clientCapabilities': {} } } }) });
    assert.equal(response.status, 200, 'HTTP failed: ' + response.status);
    return response.json();
  }
  const tools = await rpc('tools/list');
  assert.deepEqual(tools.result.tools.map(t => t.name).sort(), ['list_directory', 'read_text_file']);
  const read = await rpc('tools/call', { name: 'read_text_file', arguments: { path: path.join(directory, 'hello.txt') } });
  assert(!read.error && !read.result.isError, JSON.stringify(read));
  assert(read.result.content.some(c => c.text?.includes('Hello from the official Filesystem MCP!')));
  const list = await rpc('tools/call', { name: 'list_directory', arguments: { path: directory } });
  assert(list.result.content.some(c => c.text?.includes('hello.txt')));
  const denied = await rpc('tools/call', { name: 'write_file', arguments: { path: path.join(directory, 'blocked.txt'), content: 'must not be written' } });
  assert.equal(denied.error?.code, -32010);
  assert(!fs.existsSync(path.join(directory, 'blocked.txt')));
  const unauthenticated = await fetch(url, { method: 'POST', headers: { Accept: headers.Accept, 'Content-Type': headers['Content-Type'] }, body: '{}' });
  assert.equal(unauthenticated.status, 401);
  cli(['stop', created.mount.slug]);
  console.log('Real Filesystem MCP 2026.8.31 PASS (' + (quick ? 'public Quick HTTPS' : 'local HTTP') + '): Basic auth, modern stateless discovery, read/list, denied write');
} finally {
  spawnSync(binary, ['daemon', 'stop', '--force'], { env, windowsHide: true, stdio: 'ignore', timeout: 10000 });
  const timer = setTimeout(() => daemon.kill(), 5000);
  await exited;
  clearTimeout(timer);
  fs.rmSync(temp, { recursive: true, force: true });
}
