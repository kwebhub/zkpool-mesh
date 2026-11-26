boot_cargo_cache() {
  log "cargo registry + git cache"
  local reg_dir="$CACHE/cargo-registry"
  local git_dir="$CACHE/cargo-git"
  mkdir -p "$reg_dir" "$git_dir"
  if [ -n "$(ls -A "$reg_dir" 2>/dev/null)" ]; then
    skip "cargo-registry present"
    return
  fi
  [ -d "$CACHE/solana-dev/rustup" ] || die "rustup missing — run boot_solana_dev first"
  ensure_builder

  docker run --rm \
    --user "$BUILDER_USER" \
    -e "HOME=${BUILDER_HOME}" \
    -e "CARGO_HOME=${BUILDER_HOME}/.cargo" \
    -v "$ROOT:/repo:ro" \
    -v "$CACHE:/cache:rw" \
    -v "$reg_dir:${BUILDER_HOME}/.cargo/registry:rw" \
    -v "$git_dir:${BUILDER_HOME}/.cargo/git:rw" \
    -v "$CACHE/solana-dev/rustup:${BUILDER_HOME}/.rustup:ro" \
    -v "$CACHE/solana-dev/bin:${BUILDER_HOME}/.local/bin:ro" \
    -v "$CACHE/solana-dev/solana-install:${BUILDER_HOME}/.local/share/solana/install:ro" \
    -v "$CACHE/solana-dev/platform-tools:${BUILDER_HOME}/.cache/solana:ro" \
    -v "$CACHE/cargo-bin:/opt/cargo-bin:ro" \
    "$BUILDER_IMAGE" \
    bash -lc '
      set -e
      export PATH="/opt/cargo-bin:$HOME/.local/share/solana/install/active_release/bin:$HOME/.local/bin:$PATH"

      manifests=(
        /cache/sunspot/gnark-solana/Cargo.toml
        /cache/sunspot/testgen/Cargo.toml
        /repo/onchain/Cargo.toml
        /repo/onchain/tests/mock-verifier/Cargo.toml
        /repo/scripts/common/Cargo.toml
        /repo/scripts/pool-init/Cargo.toml
        /repo/scripts/validate-spec/Cargo.toml
        /repo/scripts/sync-circuits/Cargo.toml
        /repo/scripts/e2e-deposit/Cargo.toml
        /repo/scripts/e2e-deposit-split/Cargo.toml
        /repo/scripts/e2e-withdraw/Cargo.toml
        /repo/services/backend/Cargo.toml
        /repo/services/prover/Cargo.toml
        /repo/tests/Cargo.toml
      )

      for m in "${manifests[@]}"; do
        [ -f "$m" ] || continue
        echo "  fetching $m"
        cargo fetch --locked --manifest-path "$m"
      done

      if [ -f /repo/onchain/Cargo.toml ]; then
        echo "  fetching anchor idl build crates"
        cp -a /repo/onchain /tmp/onchain-probe
        ( cd /tmp/onchain-probe && anchor idl build -o /tmp/idl.json >/dev/null 2>&1 || true )
        rm -rf /tmp/onchain-probe /tmp/idl.json
      fi
    '
  [ -n "$(ls -A "$reg_dir" 2>/dev/null)" ] || die "cargo-registry cache empty"
  log "  cargo-registry: $reg_dir"
}

boot_pnpm_store() {
  log "pnpm store cache"
  local store_dir="$CACHE/pnpm-store"
  mkdir -p "$store_dir"
  if [ -n "$(ls -A "$store_dir" 2>/dev/null)" ]; then
    skip "pnpm-store present"
    return
  fi
  [ -x "$CACHE/solana-dev/node/bin/node" ] || die "node missing — run boot_solana_dev first"
  [ -x "$CACHE/pnpm/bin/pnpm" ] || die "pnpm missing — run boot_pnpm first"
  ensure_builder

  docker run --rm \
    --user "$BUILDER_USER" \
    -e "HOME=${BUILDER_HOME}" \
    -e "PNPM_HOME=${BUILDER_HOME}/.local/share/pnpm" \
    -v "$ROOT:/repo:ro" \
    -v "$CACHE/solana-dev/node:${BUILDER_HOME}/.local/node:ro" \
    -v "$CACHE/pnpm:${BUILDER_HOME}/.local/share/pnpm:ro" \
    -v "$store_dir:/store:rw" \
    "$BUILDER_IMAGE" \
    bash -lc '
      set -e
      export PATH="$HOME/.local/share/pnpm/bin:$HOME/.local/node/bin:$PATH"
      for dir in /repo/services/merkle /repo/web; do
        [ -f "$dir/pnpm-lock.yaml" ] || continue
        echo "  fetching pnpm store for $dir"
        mkdir -p /tmp/pnpm-probe
        cp "$dir/package.json" "$dir/pnpm-lock.yaml" /tmp/pnpm-probe/
        ( cd /tmp/pnpm-probe && pnpm fetch --frozen-lockfile --store-dir /store --ignore-scripts )
        rm -rf /tmp/pnpm-probe
      done
    '
  [ -n "$(ls -A "$store_dir" 2>/dev/null)" ] || die "pnpm-store cache empty"
  log "  pnpm-store: $store_dir"
}
