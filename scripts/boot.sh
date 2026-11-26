#!/usr/bin/env bash
# scripts/boot.sh — one-time cache warm-up for zkpool-mesh.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE}")/.." && pwd)"
CACHE="$ROOT/cache"
BUILDER_IMAGE="zkpool-boot-builder:latest"
ENV_FILE="$ROOT/.env"
ENV_EXAMPLE="$ROOT/.env.example"
BUILDER_USER="i"
BUILDER_HOME="/home/${BUILDER_USER}"

source "$ROOT/scripts/boot/common.sh"
source "$ROOT/scripts/boot/toolchain.sh"
source "$ROOT/scripts/boot/deps.sh"
source "$ROOT/scripts/boot/env.sh"

need curl
need docker
need tar
need python3

mkdir -p "$CACHE"
load_env

ANCHOR_VERSION="$(env_opt ANCHOR_VERSION)"
NODE_VERSION="$(env_opt NODE_VERSION)"
PNPM_VERSION="$(env_opt PNPM_VERSION)"
GO_VERSION="$(env_opt GO_VERSION)"
CARGO_DENY_VERSION="$(env_opt CARGO_DENY_VERSION)"
SOLANA_PLATFORM_TOOLS_VERSION="$(env_opt SOLANA_PLATFORM_TOOLS_VERSION)"
SOLANA_PLATFORM_TOOLS_VERSION="${SOLANA_PLATFORM_TOOLS_VERSION:-v1.56}"

log "versions from .env:"
printf '    ANCHOR_VERSION=%s\n' "${ANCHOR_VERSION:-<latest>}"
printf '    NODE_VERSION=%s\n' "${NODE_VERSION:-<latest>}"
printf '    PNPM_VERSION=%s\n' "${PNPM_VERSION:-<latest>}"
printf '    GO_VERSION=%s\n' "${GO_VERSION:-<latest>}"
printf '    CARGO_DENY_VERSION=%s\n' "${CARGO_DENY_VERSION:-<latest>}"
printf '    SOLANA_PLATFORM_TOOLS_VERSION=%s\n' "$SOLANA_PLATFORM_TOOLS_VERSION"

main() {
  log "boot root: $ROOT"
  log "cache dir: $CACHE"

  boot_go
  boot_solana_dev
  boot_sunspot
  boot_cargo_cache
  boot_nargo
  boot_pnpm
  boot_pnpm_store
  boot_images
  boot_wallet

  log "removing builder image"
  docker rmi -f "$BUILDER_IMAGE" >/dev/null 2>&1 || true

  log "boot complete"
  du -sh "$CACHE"/* 2>/dev/null || true
}

main "$@"
