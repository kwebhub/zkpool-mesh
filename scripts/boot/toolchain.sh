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
    version="$(curl -fsSL --retry 5 --retry-all-errors https://go.dev |
      python3 -c 'import json,sys; print(json.load(sys.stdin)[0]["version"])')"
  fi
  [ -n "$version" ] || die "could not determine Go version"
  local url="https://go.dev{version}.linux-amd64.tar.gz"
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
  local pt_dir="$CACHE/solana-dev/platform-tools"
  local si_dir="$CACHE/solana-dev/solana-install"
  local ci_dir="$CACHE/solana-dev/solana-config-install"
  if [ -e "$bin_dir/anchor" ] && [ -x "$bin_dir/solana" ] && [ -x "$node_dir/node" ] &&
    [ -x "$cargo_dir/cargo-deny" ] && [ -x "$cargo_dir/cargo-audit" ] && [ -x "$cargo_dir/cargo-build-sbf" ] &&
    [ -d "$pt_dir" ] && [ -n "$(ls -A "$pt_dir" 2>/dev/null)" ] &&
    [ -d "$si_dir" ] && [ -f "$ci_dir/config.yml" ]; then
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
    -e "SOLANA_PLATFORM_TOOLS_VERSION=${SOLANA_PLATFORM_TOOLS_VERSION}" \
    "$BUILDER_IMAGE" \
    bash -lc '
      set -e
      export PATH="$HOME/.cargo/bin:$HOME/.local/share/solana/install/active_release/bin:$PATH"

      curl --proto "=https" --tlsv1.2 -sSf https://rustup.rs | sh -s -- -y --profile minimal
      . "$HOME/.cargo/env"
      rustup default stable

      sh -c "$(curl -sSfL https://anza.xyz)"
      export PATH="$HOME/.cargo/bin:$HOME/.local/share/solana/install/active_release/bin:$PATH"

      if [ -n "${ANCHOR_VERSION:-}" ]; then
        cargo install --git https://github.com anchor-cli --tag "v${ANCHOR_VERSION}" --locked --force
      else
        cargo install --git https://github.com anchor-cli --locked --force
      fi

      curl -o- https://githubusercontent.com | bash
      export NVM_DIR="$HOME/.nvm"
      . "$NVM_DIR/nvm.sh"
      if [ -n "${NODE_VERSION:-}" ]; then
        nvm install "$NODE_VERSION" && nvm alias default "$NODE_VERSION"
      else
        nvm install --lts && nvm alias default "lts/*"
      fi

      if [ -n "${CARGO_DENY_VERSION:-}" ]; then
        cargo install cargo-deny --locked --version "$CARGO_DENY_VERSION"
      else
        cargo install cargo-deny --locked
      fi
      cargo install cargo-audit --locked

      rm -f "$HOME/.local/share/solana/install/active_release/bin/cargo-build-sbf"
      cargo install cargo-build-sbf --locked

      set +e
      mkdir -p /tmp/pt-probe/src
      echo "fn main() {}" > /tmp/pt-probe/src/main.rs
      printf "[package]\nname = \"pt-probe\"\nversion = \"0.1.0\"\nedition = \"2021\"\n" > /tmp/pt-probe/Cargo.toml
      ( cd /tmp/pt-probe && "$HOME/.cargo/bin/cargo-build-sbf" --sbf-out-dir /tmp/pt-probe/out )
      pt_rc=$?
      set -e
      if [ $pt_rc -ne 0 ]; then
        echo "WARN: cargo-build-sbf probe exit=$pt_rc" >&2
      fi
      if [ -d "$HOME/.cache/solana" ]; then
        mkdir -p /cache/solana-dev/platform-tools
        cp -a "$HOME/.cache/solana/." /cache/solana-dev/platform-tools/
        echo "platform-tools cached: $(find /cache/solana-dev/platform-tools -maxdepth 2 -type d | wc -l) dirs"
      else
        echo "ERROR: $HOME/.cache/solana not created — platform-tools NOT cached" >&2
        exit 1
      fi

      mkdir -p /cache/solana-dev/bin /cache/solana-dev/node/bin /cache/cargo-bin
      mkdir -p /cache/solana-dev/solana-install /cache/solana-dev/solana-config-install
      cp -a "$HOME/.local/share/solana/install/." /cache/solana-dev/solana-install/
      cp -a "$HOME/.local/share/solana/install/active_release/bin/." /cache/solana-dev/bin/
      rm -f /cache/solana-dev/bin/cargo-build-sbf
      cp -a "$HOME/.config/solana/install/." /cache/solana-dev/solana-config-install/ 2>/dev/null || true
      cp -a "$HOME/.cargo/bin/." /cache/solana-dev/bin/ 2>/dev/null || true
      cp -a "$HOME/.rustup/." /cache/solana-dev/rustup/ 2>/dev/null || true
      for f in "$HOME/.cargo/bin/cargo-deny" "$HOME/.cargo/bin/cargo-audit" "$HOME/.cargo/bin/cargo-build-sbf"; do
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
  [ -x "$cargo_dir/cargo-build-sbf" ] || die "cargo-build-sbf missing in cache"
  [ -d "$pt_dir" ] && [ -n "$(ls -A "$pt_dir" 2>/dev/null)" ] || die "platform-tools missing in cache"
  [ -d "$si_dir" ] || die "solana-install missing in cache"
  [ -f "$ci_dir/config.yml" ] || die "solana-config-install/config.yml missing in cache"
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
    git clone --depth 1 https://github.com "$repo"
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
      curl -L https://githubusercontent.com | bash
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
      curl -fsSL https://pnpm.io | env PNPM_VERSION="$PNPM_VERSION" sh -
      export PNPM_HOME="$HOME/.local/share/pnpm"
      export PATH="$PNPM_HOME/bin:$PATH"
      cp -a "$PNPM_HOME/." /out/
    '
  [ -x "$dir/bin/pnpm" ] || die "pnpm install failed"
  log "  pnpm home: $dir"
}
