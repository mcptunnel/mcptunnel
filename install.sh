#!/bin/sh
# Download verified native binaries. Installs in the user's directory without sudo.
set -eu
version=${MCPTUNNEL_VERSION:-latest}
install_dir=${MCPTUNNEL_INSTALL_DIR:-"$HOME/.local/bin"}
assets_dir=${MCPTUNNEL_ASSET_DIR:-}
repository=https://github.com/mcptunnel/mcptunnel
case "$(uname -s)" in
  Linux) os=linux; suffix=unknown-linux-gnu ;;
  Darwin) os=darwin; suffix=apple-darwin ;;
  *) echo 'Use install.ps1 on Windows.' >&2; exit 1 ;;
esac
case "$(uname -m)" in
  x86_64|amd64) arch=x86_64; cloud_arch=amd64 ;;
  arm64|aarch64) arch=aarch64; cloud_arch=arm64 ;;
  *) echo 'Supported architectures: x64 and ARM64.' >&2; exit 1 ;;
esac
for command in curl tar; do command -v "$command" >/dev/null || { echo "Missing $command" >&2; exit 1; }; done
if command -v sha256sum >/dev/null; then hash_command=sha256sum
elif command -v shasum >/dev/null; then hash_command='shasum -a 256'
else echo 'Missing sha256sum or shasum.' >&2; exit 1; fi
download() { curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 "$1" -o "$2"; }
if [ "$version" = latest ]; then
  [ -z "$assets_dir" ] || { echo 'Offline assets require MCPTUNNEL_VERSION=vX.Y.Z.' >&2; exit 1; }
  version=$(curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 https://api.github.com/repos/mcptunnel/mcptunnel/releases/latest | sed -n 's/.*"tag_name": *"\(v[0-9][0-9.]*\)".*/\1/p')
fi
printf '%s\n' "$version" | grep -Eq '^v[0-9]+\.[0-9]+\.[0-9]+$' || { echo 'Version must be vX.Y.Z.' >&2; exit 1; }
temp_dir=$(mktemp -d "${TMPDIR:-/tmp}/mcptunnel-install.XXXXXXXX")
trap 'rm -rf "$temp_dir"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
name=mcptunnel-$version-$arch-$suffix
archive=$name.tar.gz
for asset in "$archive" SHA256SUMS; do
  if [ -n "$assets_dir" ]; then cp "$assets_dir/$asset" "$temp_dir/$asset"
  else download "$repository/releases/download/$version/$asset" "$temp_dir/$asset"; fi
done
expected=$(awk -v asset="$archive" '$2 == asset && length($1) == 64 {print $1}' "$temp_dir/SHA256SUMS")
actual=$($hash_command "$temp_dir/$archive" | awk '{print $1}')
[ "$expected" = "$actual" ] || { echo 'Archive checksum mismatch; nothing installed.' >&2; exit 1; }
tar -xzf "$temp_dir/$archive" -C "$temp_dir"
binary=$temp_dir/$name/mcptunnel
[ -f "$binary" ] || { echo 'Release archive is missing mcptunnel.' >&2; exit 1; }
chmod +x "$binary"
[ "$("$binary" --version)" = "mcptunnel ${version#v}" ] || { echo 'Release binary version mismatch.' >&2; exit 1; }
cloudflared=
if [ "${MCPTUNNEL_SKIP_CLOUDFLARED:-0}" != 1 ] && ! command -v cloudflared >/dev/null && [ ! -x "$install_dir/cloudflared" ]; then
  case "$os-$cloud_arch" in
    linux-amd64) cloud_digest=d33ff2d14475178d2012c2c56beba87389ac5ded27649519f198a7d3134a99db ;;
    linux-arm64) cloud_digest=e6422b9d4f72d3194bc5a38676f13667c06666523217b842a877d72a80b5ac08 ;;
    darwin-amd64) cloud_digest=903845b81828c8cb3c5d13d816a2de71c06a3da5785469df8eb0e1b736d92f9f ;;
    darwin-arm64) cloud_digest=a2f79ff7b9420aa537d74af239f376da170bbabeb529aec416002adac6a72e70 ;;
  esac
  cloud_asset=cloudflared-$os-$cloud_arch
  [ "$os" != darwin ] || cloud_asset=$cloud_asset.tgz
  download "https://github.com/cloudflare/cloudflared/releases/download/2026.10.0/$cloud_asset" "$temp_dir/$cloud_asset"
  [ "$($hash_command "$temp_dir/$cloud_asset" | awk '{print $1}')" = "$cloud_digest" ] || { echo 'Cloudflared checksum mismatch; nothing installed.' >&2; exit 1; }
  cloudflared=$temp_dir/cloudflared
  if [ "$os" = darwin ]; then tar -xzf "$temp_dir/$cloud_asset" -C "$temp_dir"
  else mv "$temp_dir/$cloud_asset" "$cloudflared"; fi
  chmod +x "$cloudflared"
fi
mkdir -p "$install_dir"
cp "$binary" "$install_dir/mcptunnel"
chmod +x "$install_dir/mcptunnel"
if [ -n "$cloudflared" ]; then cp "$cloudflared" "$install_dir/cloudflared"; chmod +x "$install_dir/cloudflared"; fi
printf 'Installed mcptunnel %s in %s\n' "$version" "$install_dir"
case ":$PATH:" in *":$install_dir:"*) ;; *) printf 'Add to your shell profile: export PATH="%s:$PATH"\n' "$install_dir" ;; esac
echo 'Next: https://github.com/mcptunnel/mcptunnel#quick-start'
