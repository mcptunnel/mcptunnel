// Exact combinations are deliberately more conservative than generic transport documentation.
import assert from 'node:assert/strict';
import fs from 'node:fs';
const clients = ['ChatGPT', 'OpenAI API', 'Codex', 'Claude remote', 'Claude Code', 'Claude Desktop remote', 'Cursor', 'VS Code / Copilot'];
const revisions = ['2026-07-28', '2025-11-25', '2025-06-18', '2025-03-26'];
const transports = ['Free Quick (gateway loopback fixture)', 'Paid managed', 'Independent local (gateway loopback fixture)'];
const authModes = ['Public', 'Basic', 'External OAuth'];
const operations = ['connect_discover_or_initialize', 'list_tools', 'call_tools', 'read_resources', 'get_prompts', 'reconnect', 'expired_or_invalid_token', 'invalid_protocol_version', 'long_running_invocation', 'concurrent_invocations'];
const core = operations.slice(0, 5);
const gateway = 'tests/gateway.rs';
const official = 'docs/CONFORMANCE_RESULTS.json';
function protocol(revision, transport, auth, operation) {
  if (transport === 'Paid managed') return ['UNSUPPORTED', 'src/tunnel/managed.rs: backend unavailable; no entitlement can provision a lease'];
  if (auth === 'Public' && operation === 'expired_or_invalid_token') return ['UNSUPPORTED', 'Public mode has no token validation'];
  const quick = transport.startsWith('Free Quick');
  if (auth === 'Public') {
    if (revision === '2026-07-28') {
      if (core.includes(operation)) return ['PASS', quick ? `${gateway}: modern_calls_legacy_without_session_and_preserves_results` : `${official}: stateless and tool/resource/prompt scenarios`];
      if (quick && operation === 'invalid_protocol_version') return ['PASS', `${gateway}: protocol_headers_auth_policy_and_http_errors`];
      if (quick && operation === 'long_running_invocation') return ['PASS', `${gateway}: timeout_cancels_a_real_nonresponding_child (60-second deadline, not unlimited execution)`];
      if (quick && operation === 'concurrent_invocations') return ['PASS', `${gateway}: concurrent_stateless_children_and_crash_recovery`];
    } else {
      if (quick && ['connect_discover_or_initialize', 'call_tools'].includes(operation)) return ['PASS', `${gateway}: legacy_versions_sessions_isolation_and_notifications`];
      if (!quick && revision === '2025-11-25' && [...core, 'reconnect'].includes(operation)) return ['PASS', `${official}: initialize, tool/resource/prompt and session-lifecycle scenarios`];
    }
  }
  if (auth === 'Basic' && quick && revision === '2026-07-28' && ['list_tools', 'expired_or_invalid_token'].includes(operation)) {
    return ['PASS', `${gateway}: media_types_payloads_basic_and_modern_notifications (invalid Basic credentials; Basic has no token expiry)`];
  }
  if (auth === 'External OAuth' && !quick && revision === '2025-11-25' && ['connect_discover_or_initialize', 'list_tools', 'expired_or_invalid_token'].includes(operation)) {
    return ['PASS', 'src/auth/oauth.rs: oauth_http_resource_metadata_scopes_expiry_and_session_subject (local signed JWT/key-cache fixture; no AS browser flow)'];
  }
  return ['NOT TESTED', 'Exact revision/transport/auth/operation combination has no executed positive fixture test'];
}
const header = ['client', 'protocol_revision', 'connection', 'authentication', 'operation', 'official_documented_status', 'automated_protocol_status', 'actual_client_e2e_status', 'documented_evidence', 'protocol_evidence', 'client_e2e_evidence'];
const rows = [];
for (const client of clients) for (const revision of revisions) for (const transport of transports) for (const auth of authModes) for (const operation of operations) {
  const [status, evidence] = protocol(revision, transport, auth, operation);
  const unsupported = transport === 'Paid managed' || (auth === 'Public' && operation === 'expired_or_invalid_token');
  rows.push([client, revision, transport, auth, operation, unsupported ? 'UNSUPPORTED' : 'NOT TESTED', status, unsupported ? 'UNSUPPORTED' : 'NOT TESTED', unsupported ? evidence : 'docs/RESEARCH.md; documented HTTP/auth capability does not specify this exact revision and complete combination', evidence, unsupported ? evidence : 'No client login/API credentials, exact client version or external provider E2E run available']);
}
assert.equal(rows.length, 2880);
assert.equal(new Set(rows.map(row => row.slice(0, 5).join('\0'))).size, rows.length);
for (const row of rows) for (const status of row.slice(5, 8)) assert(['PASS', 'FAIL', 'UNSUPPORTED', 'NOT TESTED'].includes(status));
const csv = row => row.map(value => `"${value.replaceAll('"', '""')}"`).join(',');
const result = [header, ...rows].map(csv).join('\n') + '\n';
const file = 'docs/CLIENT_COMPATIBILITY.csv';
if (process.argv.includes('--check')) assert.equal(fs.readFileSync(file, 'utf8').replaceAll('\r\n', '\n'), result, 'Regenerate the compatibility matrix after changing its evidence');
else fs.writeFileSync(file, result);
console.log(`${rows.length} unique combinations ${process.argv.includes('--check') ? 'verified' : 'written'}`);
