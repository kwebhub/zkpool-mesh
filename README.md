# zkpool-mesh

> **Demo / educational project.** Not production software. Solana Devnet only.
> ZK pool for SOL transfers with microservices, K3s, and DevSecOps — a reference
> implementation for [zkpool-tutorial](https://github.com/kwebhub/zkpool-tutorial).

A private pool for SOL transfers on Solana using ZK proofs (Groth16 on BN254).
A user deposits SOL into a shared vault, receives a deposit note, and later
withdraws SOL to any address without revealing the link between deposit and
withdrawal.

## What it does

1. **Deposit.** SOL goes into a shared vault. A commitment is added to a Merkle
   tree. The user keeps a note (secrets).
2. **Withdraw.** The user proves in zero knowledge that they know the secrets of
   *some* deposit, and receives SOL at any address. The verifier program checks
   the proof on-chain.
3. **No link.** The verifier never learns *which* deposit was used. The only
   public information is the nullifier hash, which marks the deposit as spent.

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│ FRONTEND (Vue 3 + Vite + Pinia + TS + Pug + SCSS) — :8080        │
│   zkpool-web (nginx:alpine)                                      │
└──────────────────────────────────────────────────────────────────┘
              ↓ HTTP                    ↑ HTTP
┌──────────────────────────────────────────────────────────────────┐
│ BACKEND (Rust + axum) — :4001                                    │
│   zkpool-mesh                                                    │
│   /api/health  /api/commitments  /api/root  /api/proof           │
│   /api/withdraw  /metrics                                        │
└──────────────────────────────────────────────────────────────────┘
       ↓                    ↓                    ↓
┌──────────────┐   ┌──────────────┐   ┌──────────────────────┐
│ POSTGRES 16  │   │ REDIS 7      │   │ MERKLE (Node.js)     │
│  :5432       │   │  :6379       │   │  :4003               │
│  zkpool-     │   │  zkpool-     │   │  zkpool-merkle       │
│  postgres    │   │  redis       │   └──────────────────────┘
└──────────────┘   └──────────────┘

┌──────────────────────────────────────────────────────────────────┐
│ PROVER (Rust + Sunspot) — :4002                                  │
│   zkpool-mesh                                                    │
└──────────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────────┐
│ MONITORING                                                       │
│ • Prometheus :9090   zkpool-prometheus                           │
│ • Grafana    :3000   zkpool-grafana (admin/admin)                │
└──────────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────────┐
│ SOLANA (local validator)                                         │
│ • zkpool-validator — :8899 / :9900 (solana-test-validator)       │
│ • Verifier program — checks Groth16 proofs                       │
│ • zk_pool program  — deposit / withdraw                          │
│ • Pool PDA + Vault PDA                                           │
└──────────────────────────────────────────────────────────────────┘
```

**Five principles:**
1. Single source of truth (`spec.json`).
2. Contract checks at every boundary.
3. No magic numbers.
4. LiteSVM E2E test before devnet.
5. Checkpoints with SHA-256.

## Build architecture (boot → cache → images)

**Four images, four containers:**

| Image | Dockerfile | Container | Services |
|---|---|---|---|
| `zkpool-base` | `Dockerfile.base` | `zkpool-mesh` | backend, anchor, circuits, scripts |
| `zkpool-node-base` | `Dockerfile.node-base` | `zkpool-merkle` | merkle |
| `zkpool-frontend` | `Dockerfile.frontend` | `zkpool-web` | frontend |
| `zkpool-test` | `Dockerfile.test` | `zkpool-validator` | solana-test-validator |

**Cache flow:**

1. boot_pnpm:
- cache/pnpm/
- Dockerfile.node-base
- image: zkpool-node-base
- container: zkpool-merkle

2. boot_nginx:
- cache/images/nginx_alpine.tar
- Dockerfile.frontend (use zkpool-node-base)
- image: zkpool-frontend
- container: zkpool-web

3. boot_solana_dev:
- cache/solana-dev/node/
- cache/solana-dev/bin/
- cache/cargo-bin/
- cache/anchor/anchor
  3.1. boot_go - cache/go/
  3.2. boot_sunspot - cache/sunspot/
  3.3. boot_nargo - cache/nargo/
- Dockerfile.test
- image: zkpool-test
- container: zkpool-validator
- Dockerfile.base
- image: zkpool-base (for backend, anchor, circuits, scripts)
- container: zkpool-mesh

4. boot_images - cache/images/*.tar - infra (compose)

| Образ | Dockerfile | Контейнер | Службы |
| --- | --- | --- | --- |
| zkpool-base | Dockerfile.base | zkpool-mesh | backend, anchor, circuits, scripts |
| zkpool-node-base | Dockerfile.node-base | zkpool-merkle | merkle |
| zkpool-frontend | Dockerfile.frontend | zkpool-web | frontend |
| zkpool-test | Dockerfile.test | zkpool-validator | solana-test-validator |

**`scripts/boot.sh`** — one-time cache warm-up:
- Reads versions from `.env`: `ANCHOR_VERSION`, `NODE_VERSION`, `PNPM_VERSION`, `GO_VERSION`, `CARGO_DENY_VERSION` (empty → latest).
- Functions: `boot_go`, `boot_solana_dev`, `boot_sunspot`, `boot_nargo`, `boot_pnpm`, `boot_images`, `boot_wallet`.
- Builder: `debian:13` + `python3`, `jq`, `libclang-dev`, `protobuf-compiler`, user `i` (UID 1000), `HOME=/home/i`.
- `boot_wallet`: creates wallet, log to `.secrets/keygen.log` (no stdout leak), balance printed, airdrop **not** performed.
- Network is used only inside `boot.sh`. Base images and per-part images build offline from `cache/`.

## Quick start

> **Note:** `VERIFIER_PROGRAM_ID` in `.env` must match `onchain/programs/zk_pool/src/constants.rs`.
> `make chain` updates both automatically on every run.

### Prerequisites

- Docker
- Git
- ~10 GB free disk space for `cache/` and Docker images

### First-time setup

```bash
# 1. Clone
git clone git@github.com:kwebhub/zkpool-mesh.git
cd zkpool-mesh

# 2. Warm cache + create wallet
make boot

# 3. Build images
make build-images

# 4. Start infra (postgres, redis, validator)
make up

# 5. Apply DB migrations
make migrations

# 6. Deploy on-chain: verifier + zk_pool + pool-init
make chain

# 7. Build + start services (backend, prover, merkle)
make stack

# 8. E2E: deposit → withdraw
make e2e
```

### Daily use

```bash
make up       # start infra + services
make status   # containers + health + Postgres/Redis counts
make logs     # tail merkle/prover/backend/frontend logs
make down     # stop all
make wipe     # remove containers, volumes, images (cache/ preserved)
```

### Full cycle

```bash
make redeploy   # wipe → boot → chain → infra → stack → e2e
```

### E2E variants

```bash
make e2e         # deposit (1 note) → withdraw
make e2e-split   # split deposit (3 notes) → withdraw each
```

Logs are saved to `/tmp/e2e-deposit.log`, `/tmp/e2e-withdraw.log`, `/tmp/e2e-split.log`.

## Repository layout

```
zkpool-mesh/
├── circuits/         Noir circuits (poseidon, hash2, hashes, withdrawal)
├── onchain/          Anchor program `zk_pool` (Rust 1.89.0)
├── tests/            LiteSVM integration tests (Rust 1.98.1, standalone)
├── services/
│   ├── backend/      Rust + axum (indexer, DB, cache, HTTP API)
│   ├── merkle/       Node.js + Fastify (Poseidon2 via noir_js)
│   └── prover/       Rust + axum (nargo + sunspot pipeline)
├── web/              Vue 3 + TS + Pug + SCSS
├── scripts/          boot.sh, Rust CLIs (validate-spec, sync-circuits, e2e-*, pool-init)
├── infra/            compose.yml, Dockerfiles, Prometheus, Grafana
├── cache/            gitignored, filled by boot.sh (not removed by make wipe)
├── solana/           gitignored, wallet + CLI config
├── .secrets/         gitignored, deploy logs + keygen.log
├── .checkpoints/     gitignored, SHA-256 manifests per stage
└── .github/          Workflows, issue templates, dependabot
```

## Make targets

| Target | Description |
|---|---|
| `boot` | Warm `cache/` + create wallet |
| `build-images` | Build base, node-base, test, frontend images |
| `up` / `down` | Start / stop compose |
| `migrations` | Apply SQL migrations |
| `infra` | `build-images` + `up` + `migrations` |
| `chain` | Deploy verifier + zk_pool + pool-init |
| `chain-close` | Close old programs from `cache/old-programs.txt` |
| `sync-ids` | Verify `PROGRAM_ID` / `VERIFIER_PROGRAM_ID` match `.env` |
| `validate-spec` | Validate `circuits/withdrawal/spec.json` |
| `sync` / `sync-check` | Copy ACIR to merkle/web / check only |
| `check` | Verify `.ccs`/`.pk` exist (from `make chain`) |
| `deps` | `pnpm install` in merkle and frontend |
| `mesh` | `cargo build` backend/prover + `pnpm build` web |
| `e2e-build` | Build e2e binaries |
| `services` | Restart merkle/backend/prover + health checks |
| `stack` | `validate-spec` + `sync` + `check` + `deps` + `mesh` + `e2e-build` + `services` |
| `e2e` / `e2e-split` | Deposit → withdraw (1 note / 3 notes) |
| `redeploy` | Full cycle |
| `wipe` | Remove containers/volumes/images (`cache/` preserved) |
| `status` / `logs` / `verify` | Diagnostics |

## Documentation

- `README.md` (this file).
- `CONTRIBUTING.md`, `SECURITY.md`, `CHANGELOG.md`, `LICENSE`.

## License

MIT — see [`LICENSE`](LICENSE).
