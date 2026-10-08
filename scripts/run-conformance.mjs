// Explicitly scoped CI checks; --full runs both complete official requirement sets.
import { spawn, execFileSync } from 'node:child_process';
import { once } from 'node:events';
import fs from 'node:fs';
import path from 'node:path';
const source = path.resolve(process.argv[2] ?? 'target/conformance-source');
const full = process.argv.includes('--full');
const pin = 'c37eec888e1c6ff140af79987a40008548b7cc5f';
if (execFileSync('git',['rev-parse','HEAD'],{cwd:source,encoding:'utf8'}).trim() !== pin) throw new Error('Conformance source must match the documented pin');
const common = ['completion-complete','tools-list','tools-call-simple-text','tools-call-image','tools-call-audio','tools-call-embedded-resource','tools-call-mixed-content','tools-call-error','tools-call-with-progress','server-sse-multiple-streams','resources-list','resources-read-text','resources-read-binary','resources-templates-read','prompts-list','prompts-get-simple','prompts-get-with-args','prompts-get-embedded-resource','prompts-get-with-image','dns-rebinding-protection','json-schema-2020-12'];
// Unsupported callbacks/MRTR, logging, subscriptions and Tasks remain outside this subset.
// The full runs retain their failures; no expected-failure baseline converts them to PASS.
const sets = {
  '2026-07-28':['server-stateless',...common,'sep-2164-resource-not-found','caching','http-header-validation','http-custom-header-server-validation'],
  '2025-11-25':['server-initialize','ping',...common,'server-session-lifecycle']
};
const stamp = new Date().toISOString().replaceAll(':','-');
const output = path.resolve('target/conformance-results', `${full?'full':'applicable'}-${stamp}`);
fs.mkdirSync(output,{recursive:true});
const server = spawn(path.resolve(`target/debug/examples/conformance_gateway${process.platform==='win32'?'.exe':''}`),['node',path.join(source,'examples/servers/typescript/mcptunnel-stdio-fixture.mjs')],{windowsHide:true,stdio:['ignore','pipe','pipe']});
const exited = once(server,'exit');
let readyResolve, readyReject;
const ready = new Promise((resolve,reject)=>{readyResolve=resolve;readyReject=reject;});
server.stdout.on('data',data=>{if(data.toString().includes('READY '))readyResolve();});
server.stderr.on('data',data=>process.stderr.write(data));
server.on('error',readyReject);
server.on('exit',()=>readyReject(new Error('fixture server exited before readiness')));
const timer=setTimeout(()=>readyReject(new Error('fixture initialization exceeded 30s')),30000);
const run = args => new Promise(resolve=>{
  const child=spawn(process.execPath,[path.join(source,'dist/index.js'),'server','--url','http://127.0.0.1:43123/test/mcp',...args],{windowsHide:true,stdio:['ignore','pipe','pipe']});
  let log='';child.stdout.on('data',chunk=>{log+=chunk;});child.stderr.on('data',chunk=>{log+=chunk;});
  child.on('error',()=>resolve({code:1,log:'cannot start conformance CLI'}));
  child.on('exit',code=>resolve({code,log}));
});
let failures=0;
try {
  await ready;clearTimeout(timer);
  for(const [revision,scenarios] of Object.entries(sets)) {
    if(full) {
      const result=await run(['--requirements',revision,'--output-dir',path.join(output,revision)]);
      fs.writeFileSync(path.join(output,`${revision}.log`),result.log);
      process.stdout.write(result.log.slice(result.log.indexOf('=== SUMMARY ===')));
      if(result.code!==0)failures++;
    } else {
      for(const scenario of scenarios) {
        const result=await run(['--spec-version',revision,'--scenario',scenario,'--output-dir',path.join(output,revision)]);
        fs.writeFileSync(path.join(output,`${revision}-${scenario}.log`),result.log);
        if(result.code!==0){failures++;console.error(`${revision} ${scenario}: FAIL\n${result.log.slice(-3000)}`);}
      }
      console.log(`${revision}: ${scenarios.length} selected scenarios completed`);
    }
  }
} finally {
  clearTimeout(timer);
  await fetch('http://127.0.0.1:43123/__test/shutdown',{method:'POST'}).catch(()=>{});
  const stop=setTimeout(()=>server.kill(),5000);
  await exited;clearTimeout(stop);
}
console.log(`${full?'Full requirements':'Applicable subset'}: ${failures===0?'PASS':'FAIL'}, reports: ${output}`);
process.exitCode=failures===0?0:1;
