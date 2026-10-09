# Binary releases

End users install from [GitHub Releases](https://github.com/mcptunnel/mcptunnel/releases), not Cargo. The release archives contain mcptunnel, its README and MIT license. Cloudflared is downloaded separately from Cloudflare's official release by the installer.

## Actions budget

Both Actions workflows are currently disabled to stop quota consumption. Keep verification local until the user explicitly authorizes enabling or running Actions. A commit/push request alone does not authorize cloud verification or a release tag. Follow [the repository budget instructions](../AGENTS.md).

After authorization to enable automatic CI, source pushes to `main` and pull requests run one Linux job: fmt, strict Clippy, all-target tests, a debug build and routing smoke. Branch pushes do not duplicate PR checks; documentation-only changes do not start CI. Rust dependency/build caches and job time limits bound repeated work. Automatic CI does not produce release builds or uploaded binaries.

Use the manual CI input `full=true` only for explicitly requested full verification. It enables three OSes, release builds and official conformance. An authorized release tag passes that same full input through the reusable workflow and builds all five native archives. Manual candidate builds also require explicit authorization; a newer candidate on the same branch cancels the previous one.

## Targets

| Archive target | Native build / verification runner | Runtime |
|---|---|---|
| x86_64-pc-windows-msvc | Windows Server 2022 x64 | Windows x64; ARM64 emulation |
| x86_64-apple-darwin | macOS 15 Intel | macOS 15+ |
| aarch64-apple-darwin | macOS 14 Apple Silicon | macOS 14+ |
| x86_64-unknown-linux-gnu | Ubuntu 22.04 x64 | glibc 2.35+, OpenSSL 3 |
| aarch64-unknown-linux-gnu | Ubuntu 24.04 ARM64 | glibc 2.39+, OpenSSL 3 |

No Alpine/musl or native Windows ARM64 binary is advertised. Binaries are not code-signed or Apple-notarized; operating systems may require the user's normal downloaded-app approval.

## Publish

1. Bump Cargo.toml/Cargo.lock and write docs/releases/vX.Y.Z.md.
2. Verify locally: fmt, strict Clippy, all-target tests, release build, archive, installer, and real Filesystem MCP smoke.
3. Commit with a Conventional Commit and push the branch.
4. Create an annotated vX.Y.Z tag at that verified commit and push that tag.
5. The release workflow builds all five targets and tests the real archive installers, including corrupt-download rejection and paths with spaces. It also tests Basic auth, discovery, read/list and denied writes against the official Filesystem MCP. The reusable CI runs the full Rust checks and supported official conformance subset.
6. Only when all jobs pass does the workflow create a draft, upload all assets, then publish the complete release. Confirm the assets and install from the public URL before announcing it.

Never move an existing release tag to a new implementation. Correct failures in a new package version and tag. A failed publish can leave a draft; inspect and recover that draft rather than replacing a public release.

For a candidate build without publication, run the native-binary workflow manually on a branch. This builds and tests all five archives and retains them as Actions artifacts; publication runs only for version tags. Public installers require a public release repository. Do not change source-repository visibility without the owner's authorization.

## Installer controls

PowerShell: download install.ps1 and run with -Version vX.Y.Z, -InstallDir PATH, -NoPath, or -SkipCloudflared. Offline installation accepts -AssetsDirectory PATH and requires an explicit version.

Shell: `MCPTUNNEL_VERSION`, `MCPTUNNEL_INSTALL_DIR`, `MCPTUNNEL_SKIP_CLOUDFLARED=1`, `MCPTUNNEL_NO_PATH=1`, and `MCPTUNNEL_ASSET_DIR` control the same settings. By default, the installer writes idempotent activation scripts to `~/.local/share/mcptunnel/env` and `env.fish`, then adds a guarded source line to the selected shell's startup files. It preserves existing profile content and PATH entries. Reinstalling or changing the install directory updates the activation scripts without duplicating profile entries. `MCPTUNNEL_NO_PATH=1` skips all activation/profile writes.

- zsh: `.zprofile` and `.zshrc` under `ZDOTDIR` (or HOME); a `ZDOTDIR` set inside `.zshenv` is detected too.
- bash: `.bashrc` and the first existing login file among `.bash_profile`, `.bash_login`, `.profile`; when none exists, creates `.profile`.
- fish: `$XDG_CONFIG_HOME/fish/conf.d/mcptunnel.fish`, defaulting to `~/.config/fish/conf.d/mcptunnel.fish`.
- sh/dash/ash/ksh: `.profile`. Other shells report that automatic setup is unsupported; use `MCPTUNNEL_NO_PATH=1` for manual installation.

New terminals pick up PATH automatically. A piped installer cannot modify its parent shell, so it prints a shell-specific source command for optional immediate activation. Startup-file selection follows the [zsh manual](https://zsh.sourceforge.io/Doc/Release/Files.html), [bash manual](https://www.gnu.org/software/bash/manual/html_node/Bash-Startup-Files), and [fish configuration rules](https://fishshell.com/docs/current/language.html#configuration-files); the shared activation-file approach is also used by [rustup](https://github.com/rust-lang/rustup/blob/main/src/cli/self_update/shell.rs).

Offline assets must include the matching archive and SHA256SUMS. Both installers reject mismatches before writing executables or changing PATH. SHA256SUMS checks download integrity; it is not an independent publisher signature. The installer script itself must come from the trusted repository/release.

Cloudflared is pinned to 2026.10.0 with per-platform digests from its official GitHub release. An existing executable is preserved. Update this pin and verify all digests together when changing it.

## Evidence

Version **0.3.1** passed [Linux/macOS/Windows CI](https://github.com/mcptunnel/mcptunnel/actions/runs/37863297281): 66 tests per platform, fmt, strict Clippy, release builds, routing smoke and 50 supported conformance scenarios. Its [native candidate run](https://github.com/mcptunnel/mcptunnel/actions/runs/37863330366) passed all five target jobs and the actual official Filesystem MCP. Both macOS architectures verified zsh/bash startup, including custom ZDOTDIR; both Linux architectures verified bash/fish startup, including XDG_CONFIG_HOME. Real archive tests covered preserved profile content/PATH, repeated installation/activation, relocation, opt-out, read-only profiles, special characters and corruption before writes. All five downloaded archive digests match their sidecars. [Exact source commit, checksums and checks](INSTALLER_PATH_RESULTS.json) are retained. This is a verified candidate; no tag or release was published.

Version **0.3.0** passed [Linux/macOS/Windows CI](https://github.com/mcptunnel/mcptunnel/actions/runs/37859545144): 66 tests per platform, fmt, strict Clippy, release builds and shared/multiple-port routing checks, plus all 50 supported conformance scenarios. Its [native candidate run](https://github.com/mcptunnel/mcptunnel/actions/runs/37859545313) passed all five target jobs, including install/reinstall, paths with spaces, corrupt-download rejection and actual official Filesystem MCP via `npx`. Downloaded archive digests were verified locally; [exact checksums and source commit](LOCAL_ROUTING_RESULTS.json) are retained. Default localhost/8000 auto-start and real public Quick HTTPS also passed on Windows. These are candidate artifacts; anonymous publication awaits the owner's repository visibility/download destination decision.

The earlier 0.2.2 [three-platform CI run](https://github.com/mcptunnel/mcptunnel/actions/runs/37839162800) passed all Rust and supported conformance jobs.

The 0.2.3 [full CI run](https://github.com/mcptunnel/mcptunnel/actions/runs/37847936476) also passed on Linux/macOS/Windows: evidence checks, fmt, strict Clippy, 63 tests per platform and release builds, plus all 50 supported conformance scenarios.

The 0.2.3 [native candidate run](https://github.com/mcptunnel/mcptunnel/actions/runs/37847992850), built from source commit 94831e0, passed all five target jobs. Every archive passed install/reinstall, paths with spaces, corrupt-download rejection and the documented official Filesystem MCP launcher with Basic auth, modern discovery, read/list and write denial. All five downloaded archive digests were verified locally. [Exact candidate checksums and results](BINARY_RELEASE_RESULTS.json) are retained. These are candidate artifacts, not a published release; a rebuilt tagged release will have its own SHA256SUMS.

Public Quick HTTPS also passed the same operations on Windows with the actual Cloudflare provider. No actual AI-client E2E or full-conformance PASS is implied.

Primary sources: [GitHub runner architectures](https://docs.github.com/en/actions/reference/runners/github-hosted-runners), [GitHub CLI release creation](https://cli.github.com/manual/gh_release_create), [official Filesystem MCP](https://github.com/modelcontextprotocol/servers/tree/main/src/filesystem), [Cloudflared 2026.10.0](https://github.com/cloudflare/cloudflared/releases/tag/2026.10.0).
