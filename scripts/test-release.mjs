// Exercise the actual installer: spaces in paths, reinstall, and tamper rejection.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { testUnixPath } from './test-unix-path.mjs';
const target = process.argv[2];
assert(target, 'Supply the release target');
const version = fs.readFileSync('Cargo.toml', 'utf8').match(/^version = "([^"]+)"/m)[1];
const windows = process.platform === 'win32';
const archive = 'mcptunnel-v' + version + '-' + target + (windows ? '.zip' : '.tar.gz');
const temp = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'mcptunnel-installer-test-')));
const install = path.join(temp, 'install with spaces');
const assets = path.join(temp, 'assets with spaces');
const home = path.join(temp, 'isolated home');
fs.mkdirSync(home);
function run(directory, overrides = {}) {
  if (windows) return spawnSync('powershell.exe', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', path.resolve('install.ps1'), '-Version', 'v' + version, '-InstallDir', directory, '-AssetsDirectory', assets, '-NoPath', '-SkipCloudflared'], { encoding: 'utf8', windowsHide: true });
  return spawnSync('sh', [path.resolve('install.sh')], { encoding: 'utf8', env: { ...process.env, HOME: home, SHELL: '/bin/sh', ZDOTDIR: '', XDG_CONFIG_HOME: path.join(home, '.config'), ENV: '', BASH_ENV: '', MCPTUNNEL_NO_PATH: '0', ...overrides, MCPTUNNEL_VERSION: 'v' + version, MCPTUNNEL_INSTALL_DIR: directory, MCPTUNNEL_ASSET_DIR: assets, MCPTUNNEL_SKIP_CLOUDFLARED: '1' } });
}
try {
  fs.mkdirSync(assets);
  for (const name of [archive, 'SHA256SUMS']) fs.copyFileSync(path.join('target/dist', name), path.join(assets, name));
  for (let i = 0; i < 2; i++) {
    const result = run(install);
    assert.equal(result.status, 0, result.stdout + result.stderr);
    const binary = spawnSync(path.join(install, windows ? 'mcptunnel.exe' : 'mcptunnel'), ['--version'], { encoding: 'utf8', windowsHide: true });
    assert.equal(binary.status, 0, binary.stderr);
    assert.equal(binary.stdout.trim(), 'mcptunnel ' + version);
  }
  if (!windows) testUnixPath({ root: temp, run, version });
  fs.appendFileSync(path.join(assets, archive), 'tampered');
  const rejected = path.join(temp, 'must not exist');
  const corruptHome = path.join(temp, 'corrupt archive home');
  fs.mkdirSync(corruptHome);
  const result = run(rejected, { HOME: corruptHome, XDG_CONFIG_HOME: path.join(corruptHome, '.config') });
  assert.notEqual(result.status, 0, 'Installer accepted corrupt archive');
  assert.match(result.stdout + result.stderr, /checksum mismatch/i);
  assert(!fs.existsSync(rejected), 'Installer wrote files before checksum verification');
  assert.deepEqual(fs.readdirSync(corruptHome), [], 'Installer edited shell configuration before checksum verification');
  console.log('Release installer PASS: ' + target + '; install/reinstall, spaces, corruption rejected before writes');
} finally {
  fs.rmSync(temp, { recursive: true, force: true });
}
