// Package a native, already-built executable. No user-side compiler is needed.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { execFileSync } from 'node:child_process';

export const targets = [
  'x86_64-unknown-linux-gnu', 'aarch64-unknown-linux-gnu',
  'x86_64-apple-darwin', 'aarch64-apple-darwin', 'x86_64-pc-windows-msvc',
];
export const version = fs.readFileSync('Cargo.toml', 'utf8').match(/^version = "([^"]+)"/m)[1];
const target = process.argv[2];
if (!targets.includes(target)) throw new Error(`Unsupported release target: ${target}`);
const windows = target.includes('windows');
const binary = `mcptunnel${windows ? '.exe' : ''}`;
const source = process.argv[3] ? path.resolve(process.argv[3]) : path.resolve('target', target, 'release', binary);
const name = `mcptunnel-v${version}-${target}`;
const staging = fs.mkdtempSync(path.resolve('target', 'release-stage-'));
const output = path.resolve('target', 'dist');
fs.mkdirSync(output, { recursive: true });
try {
  const contents = path.join(staging, name);
  fs.mkdirSync(contents);
  for (const [from, to] of [[source, binary], ['LICENSE', 'LICENSE'], ['README.md', 'README.md']]) {
    fs.copyFileSync(from, path.join(contents, to));
  }
  if (!windows) fs.chmodSync(path.join(contents, binary), 0o755);
  const archive = path.join(output, `${name}.${windows ? 'zip' : 'tar.gz'}`);
  if (windows) {
    // Environment values avoid PowerShell -Command's re-parsing of paths with spaces.
    execFileSync('powershell.exe', ['-NoProfile', '-Command', 'Compress-Archive -LiteralPath $env:MCPTUNNEL_PACKAGE_SOURCE -DestinationPath $env:MCPTUNNEL_PACKAGE_DESTINATION -Force -ErrorAction Stop'], { stdio: 'inherit', windowsHide: true, env: { ...process.env, MCPTUNNEL_PACKAGE_SOURCE: contents, MCPTUNNEL_PACKAGE_DESTINATION: archive } });
  } else {
    execFileSync('tar', ['-czf', archive, '-C', staging, name], { stdio: 'inherit' });
  }
  const digest = crypto.createHash('sha256').update(fs.readFileSync(archive)).digest('hex');
  fs.writeFileSync(`${archive}.sha256`, `${digest}  ${path.basename(archive)}\n`);
  // Each artifact is independently usable for installation testing.
  fs.writeFileSync(path.join(output, 'SHA256SUMS'), `${digest}  ${path.basename(archive)}\n`);
  console.log(`Packaged ${path.basename(archive)} (${digest})`);
} finally {
  fs.rmSync(staging, { recursive: true, force: true });
}
