// Check durable result arithmetic and local documentation links without client credentials.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
execFileSync(process.execPath, ['scripts/compatibility-matrix.mjs', '--check'], { stdio: 'inherit' });
for (const evidenceFile of fs.readdirSync('docs').filter(f => /^CONFORMANCE.*_RESULTS\.json$/.test(f))) {
  const report = JSON.parse(fs.readFileSync(path.join('docs', evidenceFile), 'utf8'));
  assert.equal(report.conformanceCommit, 'c37eec888e1c6ff140af79987a40008548b7cc5f');
  assert.deepEqual(report.snapshots.map(s => s.revision), ['2026-07-28', '2025-11-25']);
  for (const snapshot of report.snapshots) {
    for (const scenario of snapshot.scenarios) {
      assert.equal(scenario.passed, scenario.checks.filter(c => c.status === 'SUCCESS').length);
      assert.equal(scenario.failed, scenario.checks.filter(c => c.status === 'FAILURE').length);
      for (const [key, status] of [['warnings', 'WARNING'], ['skipped', 'SKIPPED'], ['info', 'INFO']]) assert.equal(scenario[key], scenario.checks.filter(c => c.status === status).length);
      for (const check of scenario.checks) assert(check.id && ['SUCCESS', 'FAILURE', 'WARNING', 'SKIPPED', 'INFO'].includes(check.status));
    }
    for (const [name, scenarios] of [['total', snapshot.scenarios], ['scored', snapshot.scenarios.filter(s => s.scored)]]) {
      for (const key of ['passed', 'failed', 'warnings', 'skipped', 'info']) assert.equal(snapshot[name][key], scenarios.reduce((sum, s) => sum + s[key], 0));
    }
  }
}
const files = ['README.md', ...fs.readdirSync('docs').filter(f => f.endsWith('.md')).map(f => path.join('docs', f))];
let links = 0;
for (const file of files) {
  const markdown = fs.readFileSync(file, 'utf8');
  for (const [, target] of markdown.matchAll(/\[[^\]]*\]\(([^)\s]+)\)/g)) {
    if (target.startsWith('#')) continue;
    if (/^https?:/.test(target)) { new URL(target); continue; }
    const local = decodeURIComponent(target.split('#')[0]);
    assert(fs.existsSync(path.resolve(path.dirname(file), local)), `${file}: broken local link ${target}`);
    links++;
  }
}
console.log(`Conformance arithmetic and ${links} local links in ${files.length} documents verified`);
