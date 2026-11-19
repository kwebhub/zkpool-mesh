#!/usr/bin/env bash
# scripts/boot.sh — one-time cache warm-up for zkpool-mesh.
#
# Versions are read from .env (empty => latest):
#   ANCHOR_VERSION, NODE_VERSION, PNPM_VERSION, GO_VERSION, CARGO_DENY_VERSION
#
# Cache layout (gitignored, NOT removed by `make wipe`):
#   cache/go/                Go toolchain
#   cache/solana-dev/bin/    Rust, Solana CLI, Anchor, avm
#   cache/solana-dev/node/   Node.js
#   cache/cargo-bin/         cargo-deny + cargo-audit
#   cache/sunspot/           reilabs/sunspot repo + binary
#   cache/nargo/             nargo binary
#   cache/pnpm/              pnpm binary
#   cache/images/*.tar       docker images

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CACHE="$ROOT/cache"
BUILDER_IMAGE="zkpool-boot-builder:latest"
ENV_FILE="$ROOT/.env"
ENV_EXAMPLE="$ROOT/.env.example"
BUILDER_USER="i"
BUILDER_HOME="/home/${BUILDER_USER}"

log() { printf '==> %s\n' "$*"; }
skip() { printf '    (cached) %s\n' "$*"; }
die() {
  printf 'ERROR: %s\n' "$*" >&2
  exit 1
}
need() { command -v "$1" >/dev/null 2>&1 || die "missing host dependency: $1"; }

need curl
need docker
need tar
need python3

load_env() {
  if [ ! -f "$ENV_FILE" ]; then
    [ -f "$ENV_EXAMPLE" ] || die "no .env and no .env.example"
    cp "$ENV_EXAMPLE" "$ENV_FILE"
    log "created .env from .env.example"
  fi
  set -a
  # shellcheck disable=SC1090
  . "$ENV_FILE"
  set +a
}

env_opt() { printf '%s' "${!1:-}"; }

mkdir -p "$CACHE"
load_env

ANCHOR_VERSION="$(env_opt ANCHOR_VERSION)"
NODE_VERSION="$(env_opt NODE_VERSION)"
PNPM_VERSION="$(env_opt PNPM_VERSION)"
GO_VERSION="$(env_opt GO_VERSION)"
CARGO_DENY_VERSION="$(env_opt CARGO_DENY_VERSION)"

log "versions from .env:"
printf '    ANCHOR_VERSION=%s\n' "${ANCHOR_VERSION:-<latest>}"
printf '    NODE_VERSION=%s\n' "${NODE_VERSION:-<latest>}"
printf '    PNPM_VERSION=%s\n' "${PNPM_VERSION:-<latest>}"
printf '    GO_VERSION=%s\n' "${GO_VERSION:-<latest>}"
printf '    CARGO_DENY_VERSION=%s\n' "${CARGO_DENY_VERSION:-<latest>}"

ensure_builder() {
  log "builder image ($BUILDER_IMAGE)"
  if docker image inspect "$BUILDER_IMAGE" >/dev/null 2>&1; then
    skip "builder image present"
    return
  fi
  local ctx
  ctx="$(mktemp -d)"
  cat >"$ctx/Dockerfile" <<EOF
FROM debian:13

ENV DEBIAN_FRONTEND=noninteractive
ENV RUSTUP_HOME=${BUILDER_HOME}/.rustup \\
    CARGO_HOME=${BUILDER_HOME}/.cargo \\
    PATH=${BUILDER_HOME}/.cargo/bin:${BUILDER_HOME}/.local/share/solana/install/active_release/bin:${BUILDER_HOME}/.avm/bin:\$PATH

RUN apt-get update && apt-get install -y --no-install-recommends \\
      build-essential \\
      curl \\
      git \\
      pkg-config \\
      libssl-dev \\
      libudev-dev \\
      libclang-dev \\
      protobuf-compiler \\
      ca-certificates \\
      python3 \\
      jq \\
    && rm -rf /var/lib/apt/lists/*

RUN if ! id -u 1000 >/dev/null 2>&1; then \\
      useradd -m -u 1000 -s /bin/bash ${BUILDER_USER}; \\
    fi

USER ${BUILDER_USER}
WORKDIR ${BUILDER_HOME}
CMD ["/bin/bash"]
EOF
  docker build -t "$BUILDER_IMAGE" "$ctx"
  rm -rf "$ctx"
}

boot_go() {
  log "go toolchain"
  local go_dir="$CACHE/go"
  if [ -x "$go_dir/bin/go" ]; then
    skip "go: $("$go_dir/bin/go" version)"
    return
  fi
  mkdir -p "$go_dir"
  local version="$GO_VERSION"
  if [ -z "$version" ]; then
    version="$(curl -fsSL --retry 5 --retry-all-errors https://go.dev/dl/?mode=json |
      python3 -c 'import json,sys; print(json.load(sys.stdin)[0]["version"])')"
  fi
  [ -n "$version" ] || die "could not determine Go version"
  local url="https://go.dev/dl/${version}.linux-amd64.tar.gz"
  local tarball="$CACHE/${version}.linux-amd64.tar.gz"
  log "  downloading $version"
  curl -fL --retry 5 --retry-all-errors --retry-delay 5 -o "$tarball" "$url"
  tar -xzf "$tarball" -C "$go_dir" --strip-components=1
  rm -f "$tarball"
  [ -x "$go_dir/bin/go" ] || die "go extraction failed"
  log "  go binary: $go_dir/bin/go"
}

boot_solana_dev() {
  log "solana dev toolchain (Rust, Solana CLI, Anchor, Node, cargo tools)"
  local bin_dir="$CACHE/solana-dev/bin"
  local node_dir="$CACHE/solana-dev/node/bin"
  local cargo_dir="$CACHE/cargo-bin"
  if [ -e "$bin_dir/anchor" ] && [ -x "$bin_dir/solana" ] && [ -x "$node_dir/node" ] &&
    [ -x "$cargo_dir/cargo-deny" ] && [ -x "$cargo_dir/cargo-audit" ]; then
    skip "solana-dev"
    return
  fi
  mkdir -p "$bin_dir" "$node_dir" "$cargo_dir"
  ensure_builder

  docker run --rm \
    --user "$BUILDER_USER" \
    -v "$CACHE:/cache:rw" \
    -e "HOME=${BUILDER_HOME}" \
    -e "ANCHOR_VERSION=${ANCHOR_VERSION}" \
    -e "NODE_VERSION=${NODE_VERSION}" \
    -e "CARGO_DENY_VERSION=${CARGO_DENY_VERSION}" \
    "$BUILDER_IMAGE" \
    bash -lc '
      set -e
      export PATH="$HOME/.local/share/solana/install/active_release/bin:$HOME/.cargo/bin:$HOME/.avm/bin:$PATH"

      # Rust
      curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
      . "$HOME/.cargo/env"
      rustup default stable

      # Solana CLI / Agave (standalone installer, no Surfpool)
      sh -c "$(curl -sSfL https://release.anza.xyz/stable/install)"
      export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"

      # Anchor via avm
      cargo install --git https://github.com/coral-xyz/anchor avm --locked --force
      export PATH="$HOME/.avm/bin:$PATH"
      if [ -n "${ANCHOR_VERSION:-}" ]; then
        avm install "$ANCHOR_VERSION" && avm use "$ANCHOR_VERSION"
      else
        avm install latest && avm use latest
      fi

      # Node via nvm
      curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.7/install.sh | bash
      export NVM_DIR="$HOME/.nvm"
      . "$NVM_DIR/nvm.sh"
      if [ -n "${NODE_VERSION:-}" ]; then
        nvm install "$NODE_VERSION" && nvm alias default "$NODE_VERSION"
      else
        nvm install --lts && nvm alias default "lts/*"
      fi

      # cargo tools
      if [ -n "${CARGO_DENY_VERSION:-}" ]; then
        cargo install cargo-deny --locked --version "$CARGO_DENY_VERSION"
      else
        cargo install cargo-deny --locked
      fi
      cargo install cargo-audit --locked

      # Copy to cache
      mkdir -p /cache/solana-dev/bin /cache/solana-dev/node/bin /cache/cargo-bin
      cp -a "$HOME/.local/share/solana/install/active_release/bin/." /cache/solana-dev/bin/
      cp -a "$HOME/.cargo/bin/." /cache/solana-dev/bin/ 2>/dev/null || true
      cp -a "$HOME/.avm/bin/." /cache/solana-dev/bin/ 2>/dev/null || true
      cp -a "$HOME/.avm/." /cache/solana-dev/avm/ 2>/dev/null || true
      cp -a "$HOME/.rustup/." /cache/solana-dev/rustup/ 2>/dev/null || true
      if [ -L /cache/solana-dev/bin/anchor ]; then
        target="$(readlink -f /cache/solana-dev/bin/anchor)"
        if [ -x "$target" ] && [ "$target" != "/cache/solana-dev/bin/anchor" ]; then
          rm -f /cache/solana-dev/bin/anchor
          cp -a "$target" /cache/solana-dev/bin/anchor
        fi
      fi
      for f in "$HOME/.cargo/bin/cargo-deny" "$HOME/.cargo/bin/cargo-audit"; do
        [ -f "$f" ] && cp "$f" /cache/cargo-bin/
      done
      node_bin="$(command -v node)"
      if [ -n "$node_bin" ]; then
        node_root="$(dirname "$(dirname "$node_bin")")"
        mkdir -p /cache/solana-dev/node
        cp -a "$node_root/bin" /cache/solana-dev/node/
        cp -a "$node_root/lib" /cache/solana-dev/node/ 2>/dev/null || true
      fi
    '
  [ -e "$bin_dir/anchor" ] || die "anchor binary missing in cache"
  [ -x "$bin_dir/solana" ] || die "solana binary missing in cache"
  [ -x "$node_dir/node" ] || die "node binary missing in cache"
  [ -x "$cargo_dir/cargo-deny" ] || die "cargo-deny missing in cache"
  [ -x "$cargo_dir/cargo-audit" ] || die "cargo-audit missing in cache"
  log "  solana-dev: $bin_dir"
}

boot_sunspot() {
  log "sunspot (source + binary)"
  local repo="$CACHE/sunspot"
  if [ -x "$repo/go/sunspot" ]; then
    skip "sunspot binary present"
    return
  fi
  [ -x "$CACHE/go/bin/go" ] || die "cache/go missing — run boot_go first"
  ensure_builder
  mkdir -p "$repo"
  if [ ! -d "$repo/.git" ]; then
    log "  cloning reilabs/sunspot"
    git clone --depth 1 https://github.com/reilabs/sunspot.git "$repo"
  else
    skip "sunspot repo present"
  fi
  log "  building sunspot"
  docker run --rm \
    --user "$BUILDER_USER" \
    -e "HOME=${BUILDER_HOME}" \
    -v "$CACHE/go:/opt/go:ro" \
    -v "$repo:/src" \
    -w /src/go \
    "$BUILDER_IMAGE" \
    bash -lc 'export PATH=/opt/go/bin:$PATH && cd cmd/sunspot && go build -buildvcs=false -o /src/go/sunspot .'
  [ -x "$repo/go/sunspot" ] || die "sunspot build failed"
  rm -rf "$repo/.git" "$repo/.github"
  log "  sunspot binary: $repo/go/sunspot"
}

boot_nargo() {
  log "nargo"
  local dir="$CACHE/nargo"
  if [ -x "$dir/nargo" ]; then
    skip "nargo: $("$dir/nargo" --version 2>/dev/null || echo present)"
    return
  fi
  mkdir -p "$dir"
  ensure_builder
  docker run --rm \
    --user "$BUILDER_USER" \
    -e "HOME=${BUILDER_HOME}" \
    -v "$dir:/out" \
    "$BUILDER_IMAGE" \
    bash -lc '
      set -e
      curl -L https://raw.githubusercontent.com/noir-lang/noirup/refs/heads/main/install | bash
      export PATH="$HOME/.nargo/bin:$PATH"
      noirup
      cp "$HOME/.nargo/bin/nargo" /out/nargo
      chmod +x /out/nargo
    '
  [ -x "$dir/nargo" ] || die "nargo build failed"
  log "  nargo binary: $dir/nargo"
}

boot_pnpm() {
  log "pnpm"
  local dir="$CACHE/pnpm"
  if [ -x "$dir/bin/pnpm" ]; then
    skip "pnpm: $("$dir/bin/pnpm" --version 2>/dev/null || echo present)"
    return
  fi
  mkdir -p "$dir"
  ensure_builder
  local pnpm_ver="${PNPM_VERSION:-latest}"
  docker run --rm \
    --user "$BUILDER_USER" \
    -e "HOME=${BUILDER_HOME}" \
    -e "PNPM_VERSION=${pnpm_ver}" \
    -v "$dir:/out" \
    "$BUILDER_IMAGE" \
    bash -lc '
      set -e
      curl -fsSL https://get.pnpm.io/install.sh | env PNPM_VERSION="$PNPM_VERSION" sh -
      export PNPM_HOME="$HOME/.local/share/pnpm"
      export PATH="$PNPM_HOME/bin:$PATH"
      cp -a "$PNPM_HOME/." /out/
    '
  [ -x "$dir/bin/pnpm" ] || die "pnpm install failed"
  log "  pnpm home: $dir"
}

boot_images() {
  log "docker images"
  local dir="$CACHE/images"
  mkdir -p "$dir"
  local images=(
    "debian:13"
    "postgres:16-alpine"
    "redis:7-alpine"
    "prom/prometheus:v2.55.0"
    "grafana/grafana:11.3.0"
    "nginx:alpine"
  )
  for img in "${images[@]}"; do
    local fname
    fname="$(printf '%s' "$img" | tr '/:' '__').tar"
    if [ -f "$dir/$fname" ]; then
      skip "$img"
      continue
    fi
    log "  pulling $img"
    docker pull "$img"
    docker save -o "$dir/$fname" "$img"
  done
  log "  loading images into local docker"
  for img in "${images[@]}"; do
    local fname
    fname="$(printf '%s' "$img" | tr '/:' '__').tar"
    docker load -i "$dir/$fname" >/dev/null
  done
}

boot_wallet() {
  log "wallet"
  local key="$ROOT/solana/id.json"
  local rpc="${SOLANA_RPC_URL:-http://localhost:8899}"
  local log_file="$ROOT/.secrets/keygen.log"
  mkdir -p "$(dirname "$key")" "$(dirname "$log_file")"
  [ -x "$CACHE/solana-dev/bin/solana-keygen" ] || die "solana-keygen missing — run boot_solana_dev first"
  if [ ! -f "$key" ]; then
    log "  creating wallet → .secrets/keygen.log"
    "$CACHE/solana-dev/bin/solana-keygen" new -o "$key" --no-bip39-passphrase --force >"$log_file" 2>&1
  else
    skip "wallet present"
  fi
  local pub balance
  pub="$("$CACHE/solana-dev/bin/solana-keygen" pubkey "$key")"
  balance="$("$CACHE/solana-dev/bin/solana" balance "$pub" --url "$rpc" 2>/dev/null | awk '{print $1}' || echo 0)"
  log "  wallet: $pub, balance: ${balance} SOL"
  log "  keygen.log: $log_file"
}

main() {
  log "boot root: $ROOT"
  log "cache dir: $CACHE"
  mkdir -p "$CACHE"

  boot_go
  boot_solana_dev
  boot_sunspot
  boot_nargo
  boot_pnpm
  boot_images
  boot_wallet

  log "boot complete"
  du -sh "$CACHE"/* 2>/dev/null || true
}

main "$@"
