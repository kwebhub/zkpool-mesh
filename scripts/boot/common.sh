log() { printf '==> %s\n' "$*"; }
skip() { printf '    (cached) %s\n' "$*"; }
die() {
  printf 'ERROR: %s\n' "$*" >&2
  exit 1
}
need() { command -v "$1" >/dev/null 2>&1 || die "missing host dependency: $1"; }

env_opt() { printf '%s' "${!1:-}"; }

load_env() {
  if [ ! -f "$ENV_FILE" ]; then
    [ -f "$ENV_EXAMPLE" ] || die "no .env and no .env.example"
    cp "$ENV_EXAMPLE" "$ENV_FILE"
    log "created .env from .env.example"
  fi
  set -a
  . "$ENV_FILE"
  set +a
}

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
    PATH=${BUILDER_HOME}/.cargo/bin:${BUILDER_HOME}/.local/share/solana/install/active_release/bin:\$PATH

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
