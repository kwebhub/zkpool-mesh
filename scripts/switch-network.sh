#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="${1:-}"

case "$MODE" in
localnet | local)
  NET="localnet"
  RPC="http://solana-test-validator:8899"
  PUBLIC_RPC="http://localhost:8899"
  ;;
devnet)
  NET="devnet"
  RPC="https://solana.com"
  PUBLIC_RPC="https://solana.com"
  ;;
*)
  echo "Usage: $0 <localnet|devnet>" >&2
  exit 1
  ;;
esac

update_env() {
  local f="$1"
  [ -f "$f" ] || return 0
  sed -i -E \
    -e "s|^SOLANA_NETWORK=.*|SOLANA_NETWORK=${NET}|" \
    -e "s|^SOLANA_RPC_URL=.*|SOLANA_RPC_URL=${RPC}|" \
    -e "s|^DEFAULT_RPC=.*|DEFAULT_RPC=${RPC}|" \
    -e "s|^RPC_URL=.*|RPC_URL=${RPC}|" \
    "$f"
  echo "  updated $f ($NET -> $RPC)"
}

update_env "$ROOT/.env"
update_env "$ROOT/.env.example"

if [ -f "$ROOT/onchain/Anchor.toml" ]; then
  sed -i -E "s|^cluster = \".*\"|cluster = \"${NET}\"|" "$ROOT/onchain/Anchor.toml"
  echo "  updated $ROOT/onchain/Anchor.toml (cluster = \"$NET\")"
fi

if [ -f "$ROOT/web/.env" ]; then
  sed -i -E "s|^VITE_SOLANA_RPC_URL=.*|VITE_SOLANA_RPC_URL=${PUBLIC_RPC}|" "$ROOT/web/.env"
  echo "  updated $ROOT/web/.env (VITE_SOLANA_RPC_URL = $PUBLIC_RPC)"
fi

echo "==> switched network to $NET"
