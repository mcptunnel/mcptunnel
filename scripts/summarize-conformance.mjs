// Save check identities and real results without large payloads or credential material.
import fs from 'node:fs';
import path from 'node:path';
import {pathToFileURL} from 'node:url';
const root=path.resolve(process.argv[2]??'target/conformance-source');
const {parse}=await import(pathToFileURL(path.join(root,'node_modules/yaml/dist/index.js')));
const snapshots=[];
for(const revision of ['2026-07-28','2025-11-25']) {
  const directory=process.argv[3] ? path.resolve(process.argv[3],revision) : path.resolve('target/conformance-results',`verified-${revision}`);
  const required=parse(fs.readFileSync(path.join(root,'requirements',`${revision}.yaml`),'utf8')).server;
  const scenarios=fs.readdirSync(directory).sort().flatMap(name=>{
    const file=path.join(directory,name,'checks.json');if(!fs.existsSync(file))return [];
    const scenario=name.replace(/^server-/,'').replace(/-\d{4}-\d{2}-\d{2}T.*$/,'');
    const checks=JSON.parse(fs.readFileSync(file,'utf8')).map(({id,status,errorMessage})=>({id,status,...(errorMessage?{errorMessage}: {})}));
    return [{scenario,scored:required.includes(scenario),passed:checks.filter(c=>c.status==='SUCCESS').length,failed:checks.filter(c=>c.status==='FAILURE').length,warnings:checks.filter(c=>c.status==='WARNING').length,skipped:checks.filter(c=>c.status==='SKIPPED').length,info:checks.filter(c=>c.status==='INFO').length,checks}];
  });
  const counts=(scoredOnly)=>scenarios.filter(s=>!scoredOnly||s.scored).reduce((a,s)=>Object.fromEntries(Object.keys(a).map(key=>[key,a[key]+s[key]])),{passed:0,failed:0,warnings:0,skipped:0,info:0});
  snapshots.push({revision,total:counts(false),scored:counts(true),scenarios});
}
const result={retrieved:'2026-10-08',sdk:'rmcp 3.5.1',conformanceCommit:'c37eec888e1c6ff140af79987a40008548b7cc5f',...(process.argv[3]?{runDirectory:path.basename(path.resolve(process.argv[3]))}:{}),fixture:'Official reusable everything-server factory projected to stdio; noninteractive capability, logging-suppression and header diagnostics added. MRTR/Tasks diagnostics absent/unsupported.',snapshots};
fs.writeFileSync(process.argv[4]??'docs/CONFORMANCE_RESULTS.json',JSON.stringify(result,null,2)+'\n');
console.log(JSON.stringify(snapshots.map(({revision,total,scored})=>({revision,total,scored})),null,2));
