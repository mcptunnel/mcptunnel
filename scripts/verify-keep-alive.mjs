#!/usr/bin/env node
// One localhost, Basic-authenticated real-browser continuity check. No user profile/accounts.
import { spawn, execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { mkdtemp, writeFile, mkdir, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createServer } from 'node:http';
import { randomUUID } from 'node:crypto';
import assert from 'node:assert/strict';
const exec = promisify(execFile);
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const binary = resolve(process.argv[2] || join(root, 'target/debug/mcptunnel'));
const output = process.argv[3] && resolve(process.argv[3]);
if (output && (output.startsWith(join(root, '/')) || output.includes('/Downloads/'))) throw new Error('Save QA evidence outside the repository and Downloads');
const scratch = await mkdtemp(join(tmpdir(), 'mcptunnel-keep-alive-'));
const nonce = randomUUID();
const page = createServer((_, res) => { res.setHeader('Content-Type', 'text/html'); res.end(`<title>mcptunnel continuity</title><p>${nonce}</p><script>window.mcptunnelNonce=${JSON.stringify(nonce)}</script>`); });
const listen = server => new Promise((yes, no) => { server.once('error', no); server.listen(0, '127.0.0.1', yes); });
await listen(page);
const pageUrl = `http://127.0.0.1:${page.address().port}/`;
const portReservation = createServer(); await listen(portReservation); const port = portReservation.address().port;
await new Promise(yes => portReservation.close(yes));
const config = join(scratch, 'config.toml');
await writeFile(config, `[daemon]\nbind="127.0.0.1:${port}"\nauto_start=false\n[tunnel]\ndefault_mode="local"\n`);
const env = { ...process.env, MCPTUNNEL_CONFIG: config, MCPTUNNEL_STATE_DIR: join(scratch, 'state') };
const cli = async args => JSON.parse((await exec(binary, args, { env, cwd: root, timeout: 60000, maxBuffer: 4 * 1024 * 1024 })).stdout);
let daemon, mount, authorization;
let daemonLog = '';
let id = 0;
const result = { passed: false, package: '@playwright/mcp@0.0.84', protocol: '2026-07-28', transport: 'localhost HTTP', auth: 'Basic', browser: 'existing Chrome, headless isolated profile', accountBackedClientE2E: false };
async function rpc(name, args = {}) {
  const response = await fetch(mount.url, { method: 'POST', signal: AbortSignal.timeout(65000), headers: {
    Authorization: authorization, 'Content-Type': 'application/json', Accept: 'application/json, text/event-stream',
    'MCP-Protocol-Version': '2026-07-28', 'Mcp-Method': 'tools/call', 'Mcp-Name': name,
  }, body: JSON.stringify({ jsonrpc: '2.0', id: ++id, method: 'tools/call', params: { name, arguments: args, _meta: {
    'io.modelcontextprotocol/protocolVersion': '2026-07-28', 'io.modelcontextprotocol/clientInfo': { name: 'keep-alive-acceptance', version: '1' }, 'io.modelcontextprotocol/clientCapabilities': {},
  } } }) });
  assert.equal(response.status, 200);
  assert.equal(response.headers.has('Mcp-Session-Id'), false);
  const text = await response.text();
  const message = response.headers.get('content-type')?.includes('text/event-stream')
    ? text.split('\n').filter(s => s.startsWith('data:')).map(s => JSON.parse(s.slice(5))).find(s => s.id === id)
    : JSON.parse(text);
  assert.ok(message && !message.error, JSON.stringify(message?.error));
  assert.notEqual(message.result.isError, true, JSON.stringify(message.result));
  return message.result;
}
try {
  daemon = spawn(binary, ['daemon', 'start'], { env, cwd: root, stdio: ['ignore', 'ignore', 'pipe'] });
  daemon.stderr.on('data', d => { daemonLog = (daemonLog + d).slice(-8192); });
  let ready = false;
  const deadline = Date.now() + 10000;
  while (Date.now() < deadline) {
    try { await cli(['daemon', 'status', '--json']); ready = true; break; } catch { if (daemon.exitCode !== null) throw new Error('test daemon exited during startup'); await new Promise(yes => setTimeout(yes, 40)); }
  }
  assert.ok(ready, 'test daemon did not become ready');
  mount = (await cli(['--keep-alive', '--slug', 'browser', '--path', '/browser', '--auth', 'basic', '--allow-tool', 'browser_navigate', '--allow-tool', 'browser_evaluate', '--allow-tool', 'browser_close', '--json', '--', 'npx', '-y', '@playwright/mcp@0.0.84', '--browser', 'chrome', '--headless', '--isolated', '--output-dir', join(scratch, 'browser-output')])).mount;
  assert.equal(mount.upstream_lifetime, 'retained');
  authorization = `Basic ${Buffer.from(`${mount.basic_credentials.username}:${mount.basic_credentials.password}`).toString('base64')}`;
  const navigation = await rpc('browser_navigate', { url: pageUrl });
  result.navigationReturnedPageUrl = JSON.stringify(navigation).includes(pageUrl);
  const read = await rpc('browser_evaluate', { function: '() => ({url: location.href, nonce: window.mcptunnelNonce, title: document.title})' });
  assert.ok(JSON.stringify(read).includes(nonce), 'second HTTP call must see the same in-memory page nonce: ' + JSON.stringify(read));
  assert.ok(JSON.stringify(read).includes(pageUrl), 'second HTTP call must still be on the same page');
  result.evaluation = read;
  const inspected = await cli(['inspect', 'browser', '--json']);
  assert.equal(inspected.retained.contexts, 1);
  assert.equal(inspected.retained.states[0].status, 'ready');
  const doctor = await cli(['doctor', 'browser', '--json']);
  assert.ok(doctor.daemon_checks.some(c => c.check === 'retained_upstream'));
  await rpc('browser_close');
  const reset = await cli(['reset', 'browser', '--json']);
  assert.equal(reset.retained.contexts, 0);
  assert.equal(reset.retained.generation, 1);
  Object.assign(result, { passed: true, nonce, browserContinuity: 'browser_navigate -> separate browser_evaluate: same URL and window nonce', retainedContextsBeforeReset: inspected.retained.contexts, retainedContextsAfterReset: reset.retained.contexts, resetGeneration: reset.retained.generation, doctorAvoidedDuplicateProbe: true });
} catch (error) {
  result.error = error.message;
  process.exitCode = 1;
} finally {
  if (mount) {
    if (!result.passed) await rpc('browser_close').catch(() => {});
    await exec(binary, ['stop', 'browser'], { env, timeout: 10000 }).catch(() => {});
  }
  if (daemon) {
    await exec(binary, ['daemon', 'stop', '--force'], { env, timeout: 10000 }).catch(() => {});
    const exited = new Promise(yes => { if (daemon.exitCode !== null || daemon.signalCode !== null) yes(); else daemon.once('exit', yes); });
    const timed = async ms => Promise.race([exited.then(() => true), new Promise(yes => { const timer = setTimeout(() => yes(false), ms); timer.unref(); })]);
    if (!await timed(5000)) { daemon.kill('SIGTERM'); if (!await timed(3000)) { daemon.kill('SIGKILL'); await exited; } }
    result.ownedDaemonExited = daemon.exitCode !== null || daemon.signalCode !== null;
  }
  await new Promise(yes => page.close(yes));
  await rm(scratch, { recursive: true, force: true });
  result.disposableStateRemoved = true;
  if (!result.passed) result.daemonDiagnostic = daemonLog.replace(/Basic\s+[A-Za-z0-9+/=]+/g, 'Basic [redacted]');
  if (output) { await mkdir(dirname(output), { recursive: true }); await writeFile(output, JSON.stringify(result, null, 2) + '\n'); }
  console.log(JSON.stringify(result, null, 2));
}
