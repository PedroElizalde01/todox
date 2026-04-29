#!/usr/bin/env sh
# todox installer — Linux + macOS.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/PedroElizalde01/todox/main/install.sh | sh
#
# Env overrides:
#   TODOX_REF=<branch|tag|sha>   install a specific git ref (default: main)
#   TODOX_REPO=<owner/repo>      install from a fork
#   TODOX_FORCE=1                pass --force to cargo install

set -eu

REPO="${TODOX_REPO:-PedroElizalde01/todox}"
REF="${TODOX_REF:-main}"
GIT_URL="https://github.com/${REPO}.git"

log() { printf '\033[1;36m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m!!\033[0m %s\n' "$*" >&2; }
die() { printf '\033[1;31mxx\033[0m %s\n' "$*" >&2; exit 1; }

case "$(uname -s)" in
  Linux|Darwin) ;;
  *) die "unsupported OS: $(uname -s). Linux and macOS only." ;;
esac

if ! command -v cargo >/dev/null 2>&1; then
  if [ -f "$HOME/.cargo/env" ]; then
    # shellcheck disable=SC1091
    . "$HOME/.cargo/env"
  fi
fi

if ! command -v cargo >/dev/null 2>&1; then
  warn "cargo not found. Installing rustup (non-interactive, default toolchain)."
  if ! command -v curl >/dev/null 2>&1; then
    die "curl is required to install rustup."
  fi
  curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs | sh -s -- -y --default-toolchain stable --profile minimal
  # shellcheck disable=SC1091
  . "$HOME/.cargo/env"
fi

command -v cargo >/dev/null 2>&1 || die "cargo still not on PATH after rustup install."

FORCE_FLAG=""
if [ "${TODOX_FORCE:-0}" = "1" ]; then
  FORCE_FLAG="--force"
fi

log "installing todox@${REF} from ${GIT_URL}"
# `--locked` reuses the committed Cargo.lock for reproducible builds.
cargo install --git "$GIT_URL" --branch "$REF" --bin todo --locked $FORCE_FLAG || \
  cargo install --git "$GIT_URL" --rev    "$REF" --bin todo --locked $FORCE_FLAG || \
  cargo install --git "$GIT_URL" --tag    "$REF" --bin todo --locked $FORCE_FLAG

BIN="${CARGO_HOME:-$HOME/.cargo}/bin/todo"
if [ -x "$BIN" ]; then
  log "installed: $BIN"
else
  warn "installed via cargo, but '$BIN' not found. Check 'cargo install --list'."
fi

case ":$PATH:" in
  *":${CARGO_HOME:-$HOME/.cargo}/bin:"*) ;;
  *)
    warn "${CARGO_HOME:-\$HOME/.cargo}/bin is not on your PATH. Add this line to your shell rc:"
    printf '    export PATH="$HOME/.cargo/bin:$PATH"\n'
    ;;
esac

log "run 'todo --help' to get started."
