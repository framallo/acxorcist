#!/usr/bin/env bash
#
# acxorcist installer.
#
#   curl -fsSL https://raw.githubusercontent.com/framallo/acxorcist/main/install.sh | bash
#
# Downloads acx_convert.sh from GitHub and installs it as the `acxorcist`
# command into a directory on your PATH.

set -euo pipefail

REPO="framallo/acxorcist"
BRANCH="main"
SRC_URL="https://raw.githubusercontent.com/${REPO}/${BRANCH}/acx_convert.sh"
CMD_NAME="acxorcist"

info()  { printf '\033[1;34m==>\033[0m %s\n' "$1"; }
warn()  { printf '\033[1;33mwarning:\033[0m %s\n' "$1" >&2; }
die()   { printf '\033[1;31merror:\033[0m %s\n' "$1" >&2; exit 1; }

# --- Pick a downloader ---------------------------------------------------
if command -v curl >/dev/null 2>&1; then
  fetch() { curl -fsSL "$1"; }
elif command -v wget >/dev/null 2>&1; then
  fetch() { wget -qO- "$1"; }
else
  die "need curl or wget to download the script."
fi

# --- Warn if ffmpeg is missing (required at runtime, not install time) ---
if ! command -v ffmpeg >/dev/null 2>&1 || ! command -v ffprobe >/dev/null 2>&1; then
  warn "ffmpeg/ffprobe not found. acxorcist needs them to run."
  warn "  macOS:         brew install ffmpeg"
  warn "  Debian/Ubuntu: sudo apt install ffmpeg"
fi

# --- Choose an install directory on PATH ---------------------------------
case ":$PATH:" in
  *":/usr/local/bin:"*) [ -w /usr/local/bin ] && INSTALL_DIR="/usr/local/bin" ;;
esac
if [ -z "${INSTALL_DIR:-}" ]; then
  INSTALL_DIR="$HOME/.local/bin"
  mkdir -p "$INSTALL_DIR"
fi

DEST="${INSTALL_DIR}/${CMD_NAME}"

info "Downloading ${CMD_NAME} from ${REPO}@${BRANCH}..."
tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT
fetch "$SRC_URL" > "$tmp"
[ -s "$tmp" ] || die "downloaded file is empty."

install -m 0755 "$tmp" "$DEST"
info "Installed to ${DEST}"

# --- PATH hint -----------------------------------------------------------
case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *) warn "${INSTALL_DIR} is not on your PATH. Add this to your shell profile:"
     warn "  export PATH=\"${INSTALL_DIR}:\$PATH\"" ;;
esac

info "Done. Run '${CMD_NAME}' in a folder of MP3s to audit, then '${CMD_NAME} convert'."
