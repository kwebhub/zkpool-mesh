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
│ FRONTEND (Vue 3 + Vite + Pinia + TS + Pug + SCSS) — 5173         │
└──────────────────────────────────────────────────────────────────┘
              ↓ HTTP                    ↑ HTTP
┌──────────────────────────────────────────────────────────────────┐
│ BACKEND (Rust + axum) — 4001                                     │
│   /api/health  /api/commitments  /api/root  /api/proof           │
│   /api/withdraw  /metrics                                        │
└──────────────────────────────────────────────────────────────────┘
       ↓                    ↓                    ↓
┌──────────────┐   ┌──────────────┐   ┌──────────────────────┐
│ POSTGRES 16  │   │ REDIS 7      │   │ MERKLE (Node.js)     │
│  :5432       │   │  :6379       │   │  :4003               │
└──────────────┘   └──────────────┘   └──────────────────────┘

┌──────────────────────────────────────────────────────────────────┐
│ PROVER (Rust + Sunspot) — 4002                                   │
└──────────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────────┐
│ MONITORING                                                       │
│ • Prometheus :9090                                               │
│ • Grafana    :3000  (admin/admin)                                │
└──────────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────────┐
│ SOLANA (devnet)                                                  │
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

## Quick start

```bash
git clone git@github.com:kwebhub/zkpool-mesh.git
cd zkpool-mesh
make up
make status
```

`make up` starts the Docker Compose stack and all services inside the container.
`make status` prints container state, running processes, and HTTP health.

Prerequisites: Docker, Git, ~5 GB free disk space for the container image.

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
├── scripts/          Rust CLIs (validate-spec, sync-circuits, …)
├── infra/            Docker Compose, Dockerfile, Prometheus, Grafana
└── .github/          Workflows, issue templates, dependabot
```

## Documentation

- `README.md` (this file).
- `CONTRIBUTING.md`, `SECURITY.md`, `CHANGELOG.md`, `LICENSE`.

## License

MIT — see [`LICENSE`](LICENSE).
