#!/usr/bin/env bash
# one-click download, build, and launch for aetheria.
# usage: curl -fsSL https://raw.githubusercontent.com/witorsell/aetheria-new/main/install.sh | bash
set -euo pipefail

REPO_URL="git@github.com:witorsell/aetheria-new.git"
INSTALL_DIR="${AETHERIA_INSTALL_DIR:-aetheria}"

log() { printf '\n\033[1;35m==>\033[0m %s\n' "$1"; }

# --- system build deps (best-effort, Debian/Ubuntu only, skipped elsewhere) ---
if command -v apt-get >/dev/null 2>&1 && [ "$(id -u)" -ne 0 ]; then
  if ! command -v pkg-config >/dev/null 2>&1 || ! dpkg -s libssl-dev >/dev/null 2>&1; then
    log "installing build dependencies (pkg-config, libssl-dev)"
    sudo apt-get update -qq && sudo apt-get install -y -qq build-essential pkg-config libssl-dev
  fi
fi

# --- rust toolchain ---
if ! command -v cargo >/dev/null 2>&1; then
  log "installing Rust via rustup"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  source "$HOME/.cargo/env"
fi

# --- trunk (frontend wasm bundler) ---
if ! command -v trunk >/dev/null 2>&1; then
  log "installing trunk"
  cargo install trunk
fi

# --- fetch the repo ---
if [ -f "Cargo.toml" ] && [ -d "crates/server" ] && [ -d "crates/frontend" ]; then
  log "already inside an aetheria checkout, pulling latest and building in place"
  git pull --ff-only
elif [ -d "$INSTALL_DIR" ]; then
  log "found existing checkout at ./$INSTALL_DIR, pulling latest"
  git -C "$INSTALL_DIR" pull --ff-only
  cd "$INSTALL_DIR"
else
  log "cloning aetheria into ./$INSTALL_DIR"
  git clone "$REPO_URL" "$INSTALL_DIR"
  cd "$INSTALL_DIR"
fi

# --- configure ---
if [ ! -f .env ]; then
  log "generating .env with a fresh session secret and encryption key"
  cp .env.example .env
  SESSION_SECRET=$(openssl rand -base64 64 | tr -d '\n')
  ENCRYPTION_KEY=$(openssl rand -hex 16) # 16 bytes -> 32 hex chars, the key must be exactly 32 bytes
  # portable in-place sed for both GNU and BSD/macOS sed
  sed -i.bak "s#^AETHERIA_SESSION_SECRET=.*#AETHERIA_SESSION_SECRET=${SESSION_SECRET}#" .env
  sed -i.bak "s#^AETHERIA_ENCRYPTION_KEY=.*#AETHERIA_ENCRYPTION_KEY=${ENCRYPTION_KEY}#" .env
  rm -f .env.bak
else
  log ".env already exists, leaving it alone"
fi

# --- build ---
log "building frontend (trunk, release)"
(cd crates/frontend && trunk build --release --cargo-profile wasm-release)

log "building server (cargo, release)"
cargo build --release -p server

# --- run ---
log "starting aetheria on the port set by AETHERIA_BIND in .env (default 127.0.0.1:4310)"
if command -v pm2 >/dev/null 2>&1; then
  pm2 start ./target/release/server --name aetheria-rs --cwd "$(pwd)"
  pm2 save
  log "running under pm2 as 'aetheria-rs'. 'pm2 logs aetheria-rs' to follow it."
else
  log "pm2 not found, starting in the foreground. install pm2 (npm i -g pm2) to run it as a background service instead."
  exec ./target/release/server
fi
