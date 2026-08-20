#!/bin/sh
# One-line installer for the acxorcist binary.
#
#   curl -fsSL https://raw.githubusercontent.com/framallo/acxorcist/main/install.sh | sh
#
# Downloads a prebuilt, self-contained binary from the latest GitHub Release for
# your OS/arch — no ffmpeg, no Rust toolchain required. Override the install dir
# with ACXORCIST_INSTALL_DIR or the version with ACXORCIST_VERSION (e.g. v0.2.0).
set -eu

REPO="framallo/acxorcist"
BIN="acxorcist"
VERSION="${ACXORCIST_VERSION:-latest}"

err() { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }
info() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }

os="$(uname -s)"; arch="$(uname -m)"
case "$os" in
  Darwin) os_part="apple-darwin" ;;
  Linux)  os_part="unknown-linux-gnu" ;;
  *) err "unsupported OS '$os'. Try: cargo install --git https://github.com/$REPO" ;;
esac
case "$arch" in
  x86_64|amd64)  arch_part="x86_64" ;;
  arm64|aarch64) arch_part="aarch64" ;;
  *) err "unsupported arch '$arch'. Try: cargo install --git https://github.com/$REPO" ;;
esac

target="${arch_part}-${os_part}"
asset="${BIN}-${target}.tar.gz"
if [ "$VERSION" = "latest" ]; then
  url="https://github.com/$REPO/releases/latest/download/$asset"
else
  url="https://github.com/$REPO/releases/download/$VERSION/$asset"
fi

if [ -n "${ACXORCIST_INSTALL_DIR:-}" ]; then
  dir="$ACXORCIST_INSTALL_DIR"
elif [ -w "/usr/local/bin" ] 2>/dev/null; then
  dir="/usr/local/bin"
else
  dir="$HOME/.local/bin"
fi
mkdir -p "$dir"

tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
info "Downloading $asset ($VERSION)…"
if command -v curl >/dev/null 2>&1; then
  curl -fsSL "$url" -o "$tmp/$asset" || err "download failed: $url
  No prebuilt binary for $target? Try: cargo install --git https://github.com/$REPO"
elif command -v wget >/dev/null 2>&1; then
  wget -qO "$tmp/$asset" "$url" || err "download failed: $url"
else
  err "need curl or wget"
fi

tar -xzf "$tmp/$asset" -C "$tmp" || err "extract failed"
install -m 0755 "$tmp/$BIN" "$dir/$BIN" 2>/dev/null || { cp "$tmp/$BIN" "$dir/$BIN"; chmod 0755 "$dir/$BIN"; }
info "Installed $BIN -> $dir/$BIN"
"$dir/$BIN" --help >/dev/null 2>&1 && info "ready" || true

case ":$PATH:" in
  *":$dir:"*) : ;;
  *) printf '\033[1;33mnote:\033[0m %s is not on your PATH. Add:\n  export PATH="%s:$PATH"\n' "$dir" "$dir" ;;
esac
