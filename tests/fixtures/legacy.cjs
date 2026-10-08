// Deterministic legacy stdio fixture. No npm dependencies or network access.
const readline = require('node:readline');
const fs = require('node:fs');
let initialized = false;
let count = 0;
const timers = new Map();
const record = (kind) => { if (process.env.FIXTURE_EVENTS) fs.appendFileSync(process.env.FIXTURE_EVENTS, `${kind} ${process.pid}\n`); };
const reply = (id, result) => process.stdout.write(JSON.stringify({jsonrpc:'2.0', id, result}) + '\n');
const error = (id, code, message, data) => process.stdout.write(JSON.stringify({jsonrpc:'2.0', id, error:{code,message,...(data ? {data} : {})}}) + '\n');
const tools = ['counter','crash','echo','error','slow','progress','hang'].map(name => ({name, description:`Fixture ${name}`, inputSchema:{type:'object',properties:{text:{type:'string','x-mcp-header':'Text'}},additionalProperties:true}}));
record('start');
readline.createInterface({input:process.stdin}).on('line', line => {
  const req = JSON.parse(line);
  if (req.method === 'server/discover') return error(req.id,-32601,'legacy fixture');
  if (req.method === 'initialize') {
    initialized = true;
    return reply(req.id,{protocolVersion:process.env.FIXTURE_PROTOCOL || '2025-11-25',capabilities:{tools:{},resources:{},prompts:{}},serverInfo:{name:'legacy-fixture',version:'1.0.0'}});
  }
  if (req.method === 'notifications/cancelled') {
    record('cancelled'); clearTimeout(timers.get(req.params.requestId)); timers.delete(req.params.requestId); return;
  }
  if (req.id === undefined) return;
  if (!initialized) return error(req.id,-32000,'initialize required');
  const p = req.params || {};
  switch (req.method) {
    case 'tools/list':
      if (process.env.FIXTURE_PAGED) return reply(req.id,{tools:p.cursor ? tools.slice(1) : tools.slice(0,1), ...(!p.cursor || process.env.FIXTURE_REPEAT_CURSOR ? {nextCursor:'page2'} : {})});
      return reply(req.id,{tools, _meta:{fixture:'catalog'}, ...(p.cursor ? {nextCursor:'next'} : {})});
    case 'tools/call':
      if (['progress','slow'].includes(p.name) && p._meta?.progressToken !== undefined) process.stdout.write(JSON.stringify({jsonrpc:'2.0',method:'notifications/progress',params:{progressToken:p._meta.progressToken,progress:1,total:2}})+'\n');
      if (p.name === 'progress') { timers.set(req.id,setTimeout(()=>reply(req.id,{content:[{type:'text',text:'progress done'}]}),80)); return; }
      if (p.name === 'crash') return process.exit(17);
      if (p.name === 'hang') { record('call'); return; }
      if (p.name === 'error') return error(req.id,-32602,'fixture error',{detail:'preserved'});
      if (p.name === 'slow') { record('call'); timers.set(req.id,setTimeout(()=>reply(req.id,{content:[{type:'text',text:'done'}]}),30000)); return; }
      if (!['counter','echo'].includes(p.name)) return error(req.id,-32601,'unknown tool');
      const value = p.name === 'counter' ? ++count : (p.arguments?.text || 'echo');
      return reply(req.id,{content:[{type:'text',text:String(value)}],structuredContent:{value},isError:false,_meta:{fixture:'output',traceparent:p._meta?.traceparent}});
    case 'resources/list': return reply(req.id,{resources:[{uri:'test://resource',name:'Fixture resource',description:'preserved',mimeType:'text/plain'}]});
    case 'resources/templates/list': return reply(req.id,{resourceTemplates:[{uriTemplate:'test://{name}',name:'Fixture template'}]});
    case 'resources/read': return reply(req.id,{contents:[{uri:p.uri,mimeType:'text/plain',text:'fixture resource'}]});
    case 'prompts/list': return reply(req.id,{prompts:[{name:'greet',description:'preserved',arguments:[{name:'name',required:false}]}]});
    case 'prompts/get': return reply(req.id,{description:'preserved',messages:[{role:'user',content:{type:'text',text:`Hello ${p.arguments?.name || 'world'}`}}]});
    default: return error(req.id,-32601,'method not found');
  }
}).on('close', () => process.exit(0));
