#!/usr/bin/env bash
# scripts/bootstrap.sh — one-time cache warm-up for zkpool-mesh.
#
# Downloads and caches everything later stages need, so that
# `make chain` and `make stack` never have to reach GitHub again.
#
# Cache layout (gitignored, NOT removed by `make wipe`):
#   cache/go/                Go toolchain (latest stable from go.dev)
#   cache/anchor/anchor      anchor-cli 1.1.2 binary
#   cache/sunspot/           reilabs/sunspot repo + built `go/sunspot` binary
#   cache/images/*.tar       docker images for compose
#   cache/nvm/               nvm install.sh
#   cache/node/              node 24 tarball
#   cache/pnpm/              pnpm install.sh
#   cache/nargo/             nargo install.sh
#   cache/solana-install/    solana-install script
#
# Usage: ./scripts/bootstrap.sh
#        make bootstrap
#
# Requirements on host: bash, curl, docker, tar, python3 (for JSON parsing).

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CACHE="$ROOT/cache"
BUILDER_IMAGE="zkpool-bootstrap-builder:latest"
ANCHOR_VERSION="1.1.2"

log() { printf '==> %s\n' "$*"; }
skip() { printf '    (cached) %s\n' "$*"; }
die() {
  printf 'ERROR: %s\n' "$*" >&2
  exit 1
}

need() {
  command -v "$1" >/dev/null 2>&1 || die "missing host dependency: $1"
}

need curl
need docker
need tar
need python3

mkdir -p "$CACHE"

# ---------------------------------------------------------------------------
# 1. Go toolchain (latest stable from go.dev)
# ---------------------------------------------------------------------------
bootstrap_go() {
  log "go toolchain"
  local go_dir="$CACHE/go"
  if [ -x "$go_dir/bin/go" ]; then
    skip "go: $("$go_dir/bin/go" version)"
    return
  fi

  mkdir -p "$go_dir"
  local json latest url
  json="$(curl -fsSL --retry 5 --retry-all-errors https://go.dev/dl/?mode=json)"
  latest="$(printf '%s' "$json" | python3 -c 'import json,sys; print(json.load(sys.stdin)[0]["version"])')"
  [ -n "$latest" ] || die "could not determine latest Go version"
  url="https://go.dev/dl/${latest}.linux-amd64.tar.gz"
  log "  downloading $latest"
  curl -fL --retry 5 --retry-all-errors --retry-delay 5 \
    -o "$CACHE/${latest}.linux-amd64.tar.gz" "$url"
  tar -xzf "$CACHE/${latest}.linux-amd64.tar.gz" -C "$go_dir" --strip-components=1
  rm -f "$CACHE/${latest}.linux-amd64.tar.gz"
  "$go_dir/bin/go" version
}

# ---------------------------------------------------------------------------
# 2. Temporary builder image (ubuntu + go + build-essential + git)
# ---------------------------------------------------------------------------
ensure_builder_image() {
  log "builder image ($BUILDER_IMAGE)"
  if docker image inspect "$BUILDER_IMAGE" >/dev/null 2>&1; then
    skip "builder image present"
    return
  fi

  local ctx
  ctx="$(mktemp -d)"
  cat >"$ctx/Dockerfile" <<'EOF'
FROM ubuntu:24.04
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
      ca-certificates curl git build-essential pkg-config libssl-dev && \
    rm -rf /var/lib/apt/lists/*
ENV PATH=/opt/go/bin:/usr/local/go/bin:$PATH
EOF

  docker build -t "$BUILDER_IMAGE" "$ctx"
  rm -rf "$ctx"
}

# ---------------------------------------------------------------------------
# 3. anchor-cli binary
# ---------------------------------------------------------------------------
bootstrap_anchor() {
  log "anchor-cli $ANCHOR_VERSION"
  local dir="$CACHE/anchor"
  if [ -x "$dir/anchor" ]; then
    skip "anchor: $("$dir/anchor" --version 2>/dev/null || echo present)"
    return
  fi

  mkdir -p "$dir"
  log "  building anchor-cli from tag v$ANCHOR_VERSION (in builder)"
  docker run --rm \
    -v "$CACHE/go:/opt/go:ro" \
    -v "$dir:/out" \
    "$BUILDER_IMAGE" \
    bash -lc "
      set -e
      export PATH=/opt/go/bin:\$HOME/.cargo/bin:\$PATH
      curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
      . \"\$HOME/.cargo/env\"
      cargo install --git https://github.com/solana-foundation/anchor \
        --tag v${ANCHOR_VERSION} anchor-cli --locked --force
      cp \"\$HOME/.cargo/bin/anchor\" /out/anchor
      chmod +x /out/anchor
    "
  [ -x "$dir/anchor" ] || die "anchor build failed"
  log "  anchor binary: $dir/anchor"
}

# ---------------------------------------------------------------------------
# 4. Clone + build sunspot
# ---------------------------------------------------------------------------
bootstrap_sunspot() {
  log "sunspot (source + binary)"
  local repo="$CACHE/sunspot"
  if [ -x "$repo/go/sunspot" ]; then
    skip "sunspot binary present"
    return
  fi

  mkdir -p "$repo"
  if [ ! -d "$repo/.git" ]; then
    log "  cloning reilabs/sunspot"
    git clone --depth 1 https://github.com/reilabs/sunspot.git "$repo"
  else
    skip "sunspot repo present"
  fi

  log "  building sunspot with cached Go toolchain"
  docker run --rm \
    -v "$CACHE/go:/opt/go:ro" \
    -v "$repo:/src" \
    -w /src/go \
    "$BUILDER_IMAGE" \
    bash -lc 'export PATH=/opt/go/bin:$PATH && go version && \
      cd cmd/sunspot && go build -buildvcs=false -o /src/go/sunspot .'
  [ -x "$repo/go/sunspot" ] || die "sunspot build failed"
  log "  sunspot binary: $repo/go/sunspot"
}

# ---------------------------------------------------------------------------
# 5. Pre-pull + save docker images
# ---------------------------------------------------------------------------
bootstrap_images() {
  log "docker images"
  local dir="$CACHE/images"
  mkdir -p "$dir"
  local images=(
    "ubuntu:24.04"
    "postgres:16-alpine"
    "redis:7-alpine"
    "prom/prometheus:v2.55.0"
    "grafana/grafana:11.3.0"
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
}

# ---------------------------------------------------------------------------
# 6. Install scripts (nvm, node, pnpm, nargo, solana-install)
# ---------------------------------------------------------------------------
bootstrap_installers() {
  log "install scripts"

  local nvm_install="$CACHE/nvm/install.sh"
  mkdir -p "$CACHE/nvm"
  if [ ! -s "$nvm_install" ]; then
    log "  nvm install.sh"
    curl -fsSL --retry 5 --retry-all-errors \
      -o "$nvm_install" \
      https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.7/install.sh
  else
    skip "nvm install.sh"
  fi

  local node_tar="$CACHE/node/node-v24.10.0-linux-x64.tar.xz"
  mkdir -p "$CACHE/node"
  if [ ! -s "$node_tar" ]; then
    log "  node 24.10.0"
    curl -fL --retry 5 --retry-all-errors \
      -o "$node_tar" \
      https://nodejs.org/dist/v24.10.0/node-v24.10.0-linux-x64.tar.xz
  else
    skip "node tarball"
  fi

  local pnpm_install="$CACHE/pnpm/install.sh"
  mkdir -p "$CACHE/pnpm"
  if [ ! -s "$pnpm_install" ]; then
    log "  pnpm install.sh"
    curl -fsSL --retry 5 --retry-all-errors \
      -o "$pnpm_install" \
      https://get.pnpm.io/install.sh
  else
    skip "pnpm install.sh"
  fi

  local nargo_install="$CACHE/nargo/install.sh"
  mkdir -p "$CACHE/nargo"
  if [ ! -s "$nargo_install" ]; then
    log "  nargo install.sh"
    curl -fsSL --retry 5 --retry-all-errors \
      -o "$nargo_install" \
      https://raw.githubusercontent.com/noir-lang/noirup/refs/heads/main/install
  else
    skip "nargo install.sh"
  fi

  local solana_install="$CACHE/solana-install/install.sh"
  mkdir -p "$CACHE/solana-install"
  if [ ! -s "$solana_install" ]; then
    log "  solana install.sh"
    curl -fsSL --retry 5 --retry-all-errors \
      -o "$solana_install" \
      https://solana-install.solana.workers.dev
  else
    skip "solana install.sh"
  fi
}

# ---------------------------------------------------------------------------
# main
# ---------------------------------------------------------------------------
main() {
  log "bootstrap root: $ROOT"
  log "cache dir:      $CACHE"
  mkdir -p "$CACHE"

  bootstrap_go
  ensure_builder_image
  bootstrap_anchor
  bootstrap_sunspot
  bootstrap_images
  bootstrap_installers

  log "bootstrap complete"
  log "cache size:"
  du -sh "$CACHE"/* 2>/dev/null || true
}

main "$@"
