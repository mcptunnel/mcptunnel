// Project the official fixture's reusable MCP factory onto stdio, without its HTTP server.
// Usage: node scripts/prepare-conformance.mjs target/conformance-source
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
const root = path.resolve(process.argv[2] ?? 'target/conformance-source');
const dir = path.join(root, 'examples/servers/typescript');
const source = fs.readFileSync(path.join(dir, 'everything-server.ts'), 'utf8');
const marker = '// ===== EXPRESS APP =====';
if (!source.includes(marker)) throw new Error('Pinned fixture layout changed; review before adapting');
const projection = source.slice(0, source.indexOf(marker)) + `
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { CallToolRequestSchema, McpError } from '@modelcontextprotocol/sdk/types.js';
console.log = console.error;
const fixture = createMcpServer();
const call = fixture.server._requestHandlers.get('tools/call');
const list = fixture.server._requestHandlers.get('tools/list');
// The official HTTP-only diagnostics are absent from its reusable legacy factory.
// Add equivalent noninteractive diagnostics to exercise rejection and header rules.
fixture.server.setRequestHandler(ListToolsRequestSchema, async (request, extra) => {
  const result = await list(request, extra);
  result.tools.push(
    {name:'test_missing_capability',description:'Reject an undeclared upstream client capability',inputSchema:{type:'object',properties:{}}},
    {name:'test_logging_tool',description:'Emit an upstream log for suppression verification',inputSchema:{type:'object',properties:{}}},
    {name:'test_header_echo',description:'Echo a parameter annotated with x-mcp-header',inputSchema:{type:'object',properties:{text:{type:'string','x-mcp-header':'Text'}},required:['text']}}
  );
  return result;
});
fixture.server.setRequestHandler(CallToolRequestSchema, async (request, extra) => {
  if (request.params.name === 'test_missing_capability') {
    if (!fixture.server.getClientCapabilities()?.sampling) throw new McpError(-32021, 'sampling capability not declared', {requiredCapabilities:{sampling:{}}});
    throw new McpError(-32601, 'diagnostic does not perform sampling');
  }
  if (request.params.name === 'test_header_echo') return {content:[{type:'text',text:request.params.arguments?.text ?? ''}]};
  if (request.params.name === 'test_logging_tool') {
    await fixture.server.sendLoggingMessage({level:'info',data:'fixture log'});
    return {content:[{type:'text',text:'logging diagnostic complete'}]};
  }
  return call(request, extra);
});
await fixture.connect(new StdioServerTransport());
`;
const entry = path.join(dir, 'mcptunnel-stdio-fixture.ts');
fs.writeFileSync(entry, projection);
const { build } = await import(pathToFileURL(path.join(root, 'node_modules/esbuild/lib/main.js')));
const output = path.join(dir, 'mcptunnel-stdio-fixture.mjs');
// Resolve the existing dependency graph once, not at every isolated process start.
// Keep the official checks and their fixed response deadlines unchanged.
await build({entryPoints:[entry],outfile:output,bundle:true,platform:'node',format:'esm',banner:{js:"import { createRequire as fixtureCreateRequire } from 'node:module'; const require = fixtureCreateRequire(import.meta.url);"},logLevel:'warning'});
console.log(output);
