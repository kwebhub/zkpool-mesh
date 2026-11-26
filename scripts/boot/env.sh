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
