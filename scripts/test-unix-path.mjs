// Test the real archive installer and native shell startup in isolated homes.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

const quote = value => "'" + value.replaceAll("'", "'\\''") + "'";
const quoteFish = value => "'" + value.replaceAll('\\', '\\\\').replaceAll("'", "\\'") + "'";
const original = '# Existing user configuration without a final newline';

export function testUnixPath({ root, run, version }) {
  const available = name => spawnSync('sh', ['-c', 'command -v "$1"', 'sh', name], { encoding: 'utf8' });
  const shells = ['bash', 'zsh', 'fish'].filter(name => available(name).status === 0);
  assert(shells.includes('bash'), 'Bash is required for installer verification');
  if (process.platform === 'darwin') assert(shells.includes('zsh'), 'macOS must verify its default zsh');
  if (process.env.MCPTUNNEL_TEST_FISH === '1') assert(shells.includes('fish'), 'Fish verification was requested');
  const verified = [];

  function fixture(name, shell, extra = {}) {
    const home = path.join(root, name, "home with 'quotes' $literal `ticks` \\backslash");
    const bin = path.join(home, 'bin');
    fs.mkdirSync(bin, { recursive: true });
    const env = { HOME: home, SHELL: available(shell).stdout.trim(), XDG_CONFIG_HOME: path.join(home, 'custom config'), ...extra };
    const install = path.join(home, "install with 'quotes' $literal `ticks` \\backslash [glob]*");
    return { home, bin, env, install };
  }

  function profilesFor(f) {
    const shell = path.basename(f.env.SHELL);
    if (shell === 'fish') return [path.join(f.env.XDG_CONFIG_HOME, 'fish/conf.d/mcptunnel.fish')];
    if (shell === 'zsh') return ['.zprofile', '.zshrc'].map(name => path.join(f.env.ZDOTDIR || f.home, name));
    const login = ['.bash_profile', '.bash_login', '.profile'].find(name => fs.existsSync(path.join(f.home, name))) || '.profile';
    return [path.join(f.home, login), path.join(f.home, '.bashrc')];
  }

  function installTwice(f) {
    const profiles = profilesFor(f);
    for (const profile of profiles) {
      fs.mkdirSync(path.dirname(profile), { recursive: true });
      fs.writeFileSync(profile, original);
    }
    for (let i = 0; i < 2; i++) {
      // Already being on this process's PATH must not skip persistent setup.
      const result = run(f.install, { ...f.env, PATH: f.install + ':' + process.env.PATH });
      assert.equal(result.status, 0, result.stdout + result.stderr);
      assert.match(result.stdout, /PATH configured/);
      assert.match(result.stdout, /Open a new terminal/);
      for (const profile of profiles) {
        const content = fs.readFileSync(profile, 'utf8');
        assert(content.startsWith(original + '\n'), 'Existing profile content changed');
        assert.equal(content.split('# mcptunnel PATH').length - 1, 1, 'Reinstall duplicated the profile entry');
      }
    }
    return profiles;
  }

  function checkShell(f, flags, install = f.install) {
    const fish = path.basename(f.env.SHELL) === 'fish';
    const prefix = path.join(f.home, 'previous path entry');
    fs.mkdirSync(prefix, { recursive: true });
    const envFile = path.join(f.home, '.local/share/mcptunnel', fish ? 'env.fish' : 'env');
    // Real startup must find the binary with no pre-existing installation on PATH.
    // Source again twice to ensure activation never repeats the directory or
    // drops/reorders any pre-existing PATH entries.
    const command = fish
      ? `mcptunnel --version; or exit 31; set -l before $PATH; source ${quoteFish(envFile)}; source ${quoteFish(envFile)}; test (count $before) = (count $PATH); or exit 32; printf '__MCPTUNNEL_PATH__%s\\n' (string join : $PATH)`
      : `mcptunnel --version || exit 31; before=$PATH; . ${quote(envFile)}; . ${quote(envFile)}; [ "$PATH" = "$before" ] || exit 32; printf '__MCPTUNNEL_PATH__%s\\n' "$PATH"`;
    const result = spawnSync(f.env.SHELL, [...flags, '-c', command], {
      encoding: 'utf8', env: { ...process.env, ...f.env, PATH: prefix + ':' + process.env.PATH, ENV: '', BASH_ENV: '', ZDOTDIR: f.env.ZDOTDIR || '' },
    });
    assert.equal(result.status, 0, result.stdout + result.stderr);
    assert(result.stdout.includes('mcptunnel ' + version), 'New shell did not run the installed binary');
    const entries = result.stdout.match(/__MCPTUNNEL_PATH__(.*)/)[1].split(':');
    assert.equal(entries.filter(entry => entry === install).length, 1, 'Installed directory missing or duplicated');
    assert(entries.includes(prefix), 'Existing PATH entry was lost');
    const inherited = (prefix + ':' + process.env.PATH).split(':');
    // Login shells may run the OS path helper; exact preservation is checked
    // separately by sourcing our env from a shell without startup files.
    if (!flags.includes('-l')) {
      assert.deepEqual(entries.filter(entry => entry !== install), inherited, 'Existing PATH was changed');
    }
  }

  for (const shell of shells) {
    const f = fixture(shell, shell);
    installTwice(f);
    if (shell === 'fish') {
      checkShell(f, []);
      checkShell(f, ['-i']);
    } else {
      checkShell(f, ['-i']);
      checkShell(f, ['-l']);
      checkShell(f, ['-l', '-i']);
    }
    const profiles = profilesFor(f).map(profile => [profile, fs.readFileSync(profile, 'utf8')]);
    const relocated = path.join(f.home, 'relocated installation');
    const moved = run(relocated, f.env);
    assert.equal(moved.status, 0, moved.stdout + moved.stderr);
    for (const [profile, before] of profiles) assert.equal(fs.readFileSync(profile, 'utf8'), before, 'Relocation changed profile entries');
    checkShell(f, ['-i'], relocated);
    verified.push(shell + ': fresh login/interactive startup, preserved PATH, quoting, reinstall and relocation');
  }

  // Do not shadow bash's existing login profile selection.
  for (const name of ['.bash_profile', '.bash_login']) {
    const f = fixture(name, 'bash');
    fs.writeFileSync(path.join(f.home, name), original);
    fs.writeFileSync(path.join(f.home, '.profile'), '# Leave this fallback unchanged\n');
    const fallback = fs.readFileSync(path.join(f.home, '.profile'), 'utf8');
    installTwice(f);
    checkShell(f, ['-l', '-i']);
    assert.equal(fs.readFileSync(path.join(f.home, '.profile'), 'utf8'), fallback);
    if (name === '.bash_login') assert(!fs.existsSync(path.join(f.home, '.bash_profile')));
  }
  verified.push('bash: existing login profile precedence');

  if (shells.includes('zsh')) {
    for (const exported of [true, false]) {
      const f = fixture('zdotdir-' + exported, 'zsh');
      const custom = path.join(f.home, 'custom zsh config');
      if (exported) f.env.ZDOTDIR = custom;
      else fs.writeFileSync(path.join(f.home, '.zshenv'), 'ZDOTDIR=' + quote(custom) + '\n');
      const profiles = ['.zprofile', '.zshrc'].map(name => path.join(custom, name));
      for (let i = 0; i < 2; i++) {
        const result = run(f.install, f.env);
        assert.equal(result.status, 0, result.stdout + result.stderr);
      }
      for (const profile of profiles) assert.equal(fs.readFileSync(profile, 'utf8').split('# mcptunnel PATH').length - 1, 1);
      assert(!fs.existsSync(path.join(f.home, '.zprofile')));
      assert(!fs.existsSync(path.join(f.home, '.zshrc')));
      checkShell(f, ['-l', '-i']);
      checkShell(f, ['-i']);
    }
    verified.push('zsh: exported and .zshenv-defined ZDOTDIR');
  }

  const noPath = fixture('opt out', 'bash');
  const result = run(noPath.install, { ...noPath.env, MCPTUNNEL_NO_PATH: '1' });
  assert.equal(result.status, 0, result.stdout + result.stderr);
  assert.match(result.stdout, /PATH setup skipped/);
  assert(!fs.existsSync(path.join(noPath.home, '.profile')));
  assert(!fs.existsSync(path.join(noPath.home, '.bashrc')));
  assert(!fs.existsSync(path.join(noPath.home, '.local')));

  const posix = fixture('POSIX profile', 'sh');
  const installedPosix = run(posix.install, { ...posix.env, SHELL: '' });
  assert.equal(installedPosix.status, 0, installedPosix.stdout + installedPosix.stderr);
  const defaultProfile = process.platform === 'darwin' ? '.zprofile' : '.profile';
  const sourced = spawnSync('sh', ['-c', `. ${quote(path.join(posix.home, defaultProfile))}; mcptunnel --version`], { encoding: 'utf8', env: { ...process.env, ...posix.env } });
  assert.equal(sourced.status, 0, sourced.stdout + sourced.stderr);
  assert.equal(sourced.stdout.trim(), 'mcptunnel ' + version);
  verified.push('POSIX activation and missing SHELL');

  // Read-only dotfiles must report failure, never claim that PATH was configured.
  const readOnly = fixture('read only', 'bash');
  const profile = path.join(readOnly.home, '.profile');
  fs.writeFileSync(profile, original, { mode: 0o444 });
  const refused = run(readOnly.install, readOnly.env);
  assert.notEqual(refused.status, 0, 'Read-only profile was silently accepted');
  assert(!refused.stdout.includes('PATH configured'), 'Installer claimed failed profile setup succeeded');
  assert.equal(fs.readFileSync(profile, 'utf8'), original);
  fs.chmodSync(profile, 0o644);

  const invalid = fixture('invalid directory', 'bash');
  for (const suffix of [':separator', '\nnewline', '\rcarriage']) {
    const rejected = run(invalid.install + suffix, invalid.env);
    assert.notEqual(rejected.status, 0);
    assert.match(rejected.stderr, /colons or line breaks/);
    assert(!fs.existsSync(invalid.install + suffix));
    assert(!fs.existsSync(path.join(invalid.home, '.local')));
  }
  verified.push('opt-out, read-only profile failure and invalid PATH entries');
  console.log('Unix PATH installer PASS: ' + verified.join('; '));
}
