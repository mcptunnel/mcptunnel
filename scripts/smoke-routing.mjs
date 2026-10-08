// Real daemon/CLI/listeners/stdio; deterministic provider without external accounts/network.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import net from 'node:net';
import http from 'node:http';
import {spawn, spawnSync} from 'node:child_process';
import {once} from 'node:events';

const suffix = process.platform === 'win32' ? '.exe' : '';
const binary = path.resolve(process.argv[2] ?? 'target/release/mcptunnel' + suffix);
const provider = path.resolve('target/debug/examples/fake_cloudflared' + suffix);
assert(fs.existsSync(provider), 'Build cargo build --locked --example fake_cloudflared first');
const temp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcptunnel-routing-'));
const events = path.join(temp, 'providers.jsonl');
const config = path.join(temp, 'config.toml');
const reserve = async () => {
  const server = net.createServer();
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  return server;
};
const reserved = await Promise.all([reserve(), reserve()]);
const [port, otherPort] = reserved.map(server => server.address().port);
await Promise.all(reserved.map(server => new Promise(resolve => server.close(resolve))));
// Legacy quick default must never make a newly created mount public without explicit selection.
fs.writeFileSync(config, '[daemon]\nbind="127.0.0.1:' + port + '"\n[tunnel]\ndefault_mode="quick"\ncloudflared_path=' + JSON.stringify(provider) + '\n');
const env = {...process.env, MCPTUNNEL_CONFIG: config, MCPTUNNEL_STATE_DIR: path.join(temp, 'state'), MCPTUNNEL_TEST_PROVIDER_EVENTS: events};
const command = [process.execPath, path.resolve('tests/fixtures/legacy.cjs')];
const processes = [];
// Use raw HTTP so Host-forgery checks actually send the requested authority.
const wire = (url, options = {}) => new Promise((resolve, reject) => {
  const request = http.request(url, {method:options.method, headers:options.headers, signal:options.signal ?? AbortSignal.timeout(10000)}, response => {
    const chunks = [];
    response.on('data', chunk => chunks.push(chunk));
    response.on('error', reject);
    response.on('end', () => resolve(new Response(Buffer.concat(chunks), {status:response.statusCode, headers:response.headers})));
  });
  request.on('error', reject);
  request.end(options.body);
});
const cliResult = args => spawnSync(binary, args, {env, encoding:'utf8', windowsHide:true, timeout:60000});
const cli = args => {
  const result = cliResult(args);
  assert.equal(result.status, 0, 'CLI failed: ' + result.stderr);
  return args.includes('--json') ? JSON.parse(result.stdout) : result.stdout;
};
const providers = () => fs.existsSync(events) ? fs.readFileSync(events, 'utf8').trim().split('\n').filter(Boolean).map(JSON.parse) : [];
const start = async () => {
  const daemon = spawn(binary, ['daemon', 'start'], {env, windowsHide:true, stdio:'ignore'});
  const exited = once(daemon, 'exit');
  processes.push({daemon, exited});
  const deadline = Date.now() + 10000;
  while (cliResult(['daemon','status','--json']).status !== 0) {
    assert(Date.now() < deadline, 'Daemon failed to start');
    await new Promise(resolve => setTimeout(resolve, 100));
  }
};
const stop = async () => {
  cliResult(['daemon','stop','--force']);
  const {daemon, exited} = processes.at(-1);
  const timer = setTimeout(() => daemon.kill(), 5000);
  await exited;
  clearTimeout(timer);
};
const create = options => cli([...options, '--json', '--', ...command]).mount;
const alive = pid => {try {process.kill(pid, 0); return true;} catch {return false;}};
const assertDead = async pids => {
  const deadline = Date.now() + 5000;
  while (pids.some(alive) && Date.now() < deadline) await new Promise(resolve => setTimeout(resolve, 50));
  assert(pids.every(pid => !alive(pid)), 'Provider was not reaped');
};
const rpc = async (url, name, headers = {}) => {
  const response = await wire(url, {method:'POST', headers:{'Content-Type':'application/json', Accept:'application/json, text/event-stream', 'MCP-Protocol-Version':'2026-07-28', 'MCP-Method':'tools/call', 'MCP-Name':'echo', 'MCP-Param-Text':name, ...headers}, signal:AbortSignal.timeout(10000), body:JSON.stringify({jsonrpc:'2.0',id:1,method:'tools/call',params:{name:'echo',arguments:{text:name},_meta:{'io.modelcontextprotocol/protocolVersion':'2026-07-28','io.modelcontextprotocol/clientInfo':{name:'routing-smoke',version:'1'},'io.modelcontextprotocol/clientCapabilities':{}}}})});
  assert.equal(response.status, 200, response.status === 200 ? '' : await response.text());
  const result = await response.json();
  assert(result.result.content.some(c => c.text === name), JSON.stringify(result));
};
try {
  await start();
  const local = create(['--path','/team/private', '--auth','public']);
  assert.equal(local.port, port);
  assert.equal(local.url, `http://localhost:${port}/team/private`);
  assert.equal(local.public_url, null);
  assert.equal(providers().length, 0, 'Local mount must not start cloudflared');
  const localOAuth = create(['--path','/team/oauth','--auth','oauth','--oidc-issuer','https://issuer.example']);
  const localMetadata = await wire(`http://localhost:${port}/.well-known/oauth-protected-resource/team/oauth`);
  assert.equal(localMetadata.status,200);
  assert.equal((await localMetadata.json()).resource,localOAuth.url);
  const inferred = create(['--auth','public']);
  const collision = create(['--auth','public']);
  assert.equal(inferred.path, '/legacy');
  assert.equal(collision.path, '/legacy-2');
  const duplicate = cliResult(['--path','/team/private','--probe','off','--',...command]);
  assert.equal(duplicate.status, 2);
  assert.match(duplicate.stderr, /already registered/);
  const alternate = create(['--port',String(otherPort),'--path','/team/private','--auth','public']);
  assert.equal(cli(['daemon','status','--json']).listeners.length, 2);
  await rpc(local.url, 'local');
  await rpc(alternate.url, 'other listener');
  const initialize = await wire(local.url, {method:'POST',headers:{'Content-Type':'application/json',Accept:'application/json, text/event-stream'},body:JSON.stringify({jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'routing-legacy',version:'1'}}})});
  assert.equal(initialize.status,200);
  const session = initialize.headers.get('mcp-session-id');
  assert(session);
  const crossPort = await wire(alternate.url,{method:'POST',headers:{'Content-Type':'application/json',Accept:'application/json, text/event-stream','MCP-Protocol-Version':'2025-11-25','MCP-Session-Id':session},body:JSON.stringify({jsonrpc:'2.0',id:2,method:'tools/call',params:{name:'counter',arguments:{}}})});
  assert.equal(crossPort.status,404,'Legacy session must remain bound to its mount across ports');
  assert.equal((await wire(`http://localhost:${otherPort}${inferred.path}`)).status,404);
  assert.equal((await wire(local.url,{headers:{Host:`localhost:${otherPort}`}})).status,403);
  assert.equal((await wire(local.url,{headers:{Origin:'https://evil.example'}})).status,403);
  const occupied = await reserve();
  try {
    const failed = cliResult(['--port',String(occupied.address().port),'--path','/occupied','--probe','off','--',...command]);
    assert.equal(failed.status,2);
    assert.match(failed.stderr,/choose another --port/);
    assert(!cli(['list','--json']).mounts.some(m => m.path === '/occupied'));
  } finally {await new Promise(resolve => occupied.close(resolve));}
  const broken = cliResult(['--port',String(otherPort),'--path','/broken','--','mcptunnel-nonexistent-upstream']);
  assert.notEqual(broken.status,0);
  assert(!cli(['list','--json']).mounts.some(m => m.path === '/broken'));
  const pub = create(['--public','--path','/team/public','--auth','public']);
  const pub2 = create(['--public','--path','/second','--auth','public']);
  const publicOAuth = create(['--public','--path','/team/secure','--auth','oauth','--oidc-issuer','https://issuer.example']);
  assert.equal(providers().length, 1, 'One provider per shared local port');
  assert.equal(new URL(pub.url).origin,new URL(pub2.url).origin);
  const publicOther = create(['--public','--port',String(otherPort),'--path','/team/public','--auth','public']);
  assert.equal(providers().length,2);
  assert.notEqual(new URL(pub.url).origin,new URL(publicOther.url).origin);
  const ingress = providers()[0];
  const host = new URL(ingress.origin).host;
  await rpc(ingress.ingress + '/team/public','public ingress',{Host:host});
  await rpc(pub.local_url,'local access to public mount');
  for (const hidden of ['/team/private','/legacy','/legacy-2',`/_mcptunnel/${local.slug}`,`/${local.slug}/mcp`,'/.well-known/oauth-protected-resource/team/private','/.well-known/oauth-protected-resource/team/oauth']) {
    for (const forged of [host,`localhost:${port}`,`127.0.0.1:${port}`]) {
      assert.equal((await wire(ingress.ingress + hidden,{headers:{Host:forged}})).status,404,hidden);
    }
  }
  assert.equal((await (await wire(ingress.ingress + '/healthz',{headers:{Host:host}})).json()).mounts,3);
  const publicMetadata = await wire(ingress.ingress + '/.well-known/oauth-protected-resource/team/secure',{headers:{Host:host}});
  assert.equal(publicMetadata.status,200);
  assert.equal((await publicMetadata.json()).resource,publicOAuth.url);
  const challenge = await wire(ingress.ingress + '/team/secure',{headers:{Host:host}});
  assert.equal(challenge.status,401);
  assert(challenge.headers.get('www-authenticate').includes(publicOAuth.url.replace('/team/secure','/.well-known/oauth-protected-resource/team/secure')));
  cli(['stop',pub.slug]);
  assert(alive(ingress.pid), 'Remaining public mount must retain its provider');
  cli(['stop',pub2.slug]);
  cli(['stop',publicOAuth.slug]);
  await assertDead([ingress.pid]);
  await rpc(local.url,'local survives last public removal');
  const saved = cli(['list','--json']).mounts.map(m => [m.slug,m.port,m.path,m.tunnel]).sort();
  await stop();
  await assertDead(providers().map(p => p.pid));
  await start();
  // Restoration is serialized with creation. A registration waits for restoration to finish.
  const restored = create(['--path','/restoration-ready','--auth','public']);
  cli(['stop',restored.slug]);
  assert.deepEqual(cli(['list','--json']).mounts.map(m => [m.slug,m.port,m.path,m.tunnel]).sort(),saved);
  await rpc(local.url,'persisted local');
  await rpc(alternate.url,'persisted second port');
  const newIngress = providers().at(-1);
  await rpc(newIngress.ingress + '/team/public','restored public',{Host:new URL(newIngress.origin).host});
  assert.equal((await wire(newIngress.ingress + '/team/private',{headers:{Host:new URL(newIngress.origin).host}})).status,404);
  for (const mount of cli(['list','--json']).mounts) cli(['stop',mount.slug]);
  assert.deepEqual(cli(['daemon','status','--json']).listeners,[]);
  await assertDead(providers().map(p => p.pid));
  await stop();
  fs.writeFileSync(config,'[daemon]\nbind="127.0.0.1:' + port + '"\n[tunnel]\ncloudflared_path=' + JSON.stringify(path.join(temp,'missing-cloudflared')) + '\n');
  await start();
  const failedPublic = cliResult(['--public','--path','/failed-public','--probe','off','--',...command]);
  assert.notEqual(failedPublic.status,0);
  assert.deepEqual(cli(['list','--json']).mounts,[],'Provider failure must roll back registration');
  assert.deepEqual(cli(['daemon','status','--json']).listeners,[],'Provider failure must close its unused listeners');
  const independentLocal = create(['--path','/still-local','--auth','public']);
  await rpc(independentLocal.url,'local works without cloudflared');
  cli(['stop',independentLocal.slug]);
  console.log('Routing PASS: optional CLI, names/collisions, exact paths, multiple/shared ports, conflict/probe rollback, local/public isolation, provider reuse/reaping, restart persistence');
} finally {
  if (processes.at(-1)?.daemon.exitCode === null) await stop();
  await assertDead(providers().map(p => p.pid));
  fs.rmSync(temp,{recursive:true,force:true});
}
