#!/usr/bin/env bash
# Pin exact tool versions from built images into .env and .env.example.
# Idempotent: overwrites the corresponding *_VERSION= lines in place.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

BASE_IMAGE="${BASE_IMAGE:-zkpool-base:latest}"
NODE_BASE_IMAGE="${NODE_BASE_IMAGE:-zkpool-node-base:latest}"

get_base() {
  docker run --rm --entrypoint bash "$BASE_IMAGE" -c "$1"
}
get_node() {
  docker run --rm --entrypoint bash "$NODE_BASE_IMAGE" -c "$1"
}

ANCHOR_VERSION=$(get_base 'anchor --version' | awk '{print $2}')
SOLANA_VERSION=$(get_base 'solana --version' | awk '{print $2}')
NARGO_VERSION=$(get_base 'nargo --version' | sed -n 's/^nargo version = \(.*\)$/\1/p' | head -1)
RUSTC_VERSION=$(get_base 'rustc --version' | awk '{print $2}')
GO_VERSION=$(get_base '/opt/go/bin/go version' | awk '{print $3}' | sed 's/^go//')
CARGO_DENY_VERSION=$(get_base 'cargo-deny --version' | awk '{print $2}')
NODE_VERSION=$(get_node 'node --version' | sed 's/^v//')
PNPM_VERSION=$(get_node 'pnpm --version')

declare -A V=(
  [ANCHOR_VERSION]="$ANCHOR_VERSION"
  [SOLANA_VERSION]="$SOLANA_VERSION"
  [NARGO_VERSION]="$NARGO_VERSION"
  [RUSTC_VERSION]="$RUSTC_VERSION"
  [GO_VERSION]="$GO_VERSION"
  [CARGO_DENY_VERSION]="$CARGO_DENY_VERSION"
  [NODE_VERSION]="$NODE_VERSION"
  [PNPM_VERSION]="$PNPM_VERSION"
)

pin_file() {
  local f="$1"
  [ -f "$f" ] || return 0
  local tmp
  tmp="$(mktemp)"
  cp "$f" "$tmp"
  for k in "${!V[@]}"; do
    if grep -qE "^${k}=" "$tmp"; then
      sed -i "s|^${k}=.*|${k}=${V[$k]}|" "$tmp"
    else
      printf '%s=%s\n' "$k" "${V[$k]}" >>"$tmp"
    fi
  done
  # keep sorted version block: optional, leave as-is otherwise
  mv "$tmp" "$f"
}

pin_file .env

echo "==> [pin-versions] pinned:"
for k in ANCHOR_VERSION SOLANA_VERSION NARGO_VERSION RUSTC_VERSION GO_VERSION CARGO_DENY_VERSION NODE_VERSION PNPM_VERSION; do
  echo "    $k=${V[$k]}"
done
