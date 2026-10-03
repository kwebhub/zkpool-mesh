# Security Policy

## Supported versions

| Version | Supported |
|---------|-----------|
| `main`  | yes       |
| < 0.1.0 | no        |

Only the `main` branch receives security fixes.

## Important context

This is a **demo / educational project**, not production software. Before
reporting an issue, read:

- [`docs/PROJECT_CONTEXT.md`](docs/PROJECT_CONTEXT.md) — project scope and rules.
- `docs/ru/articles/` — engineering notes per stage (once published).

Many "vulnerabilities" are **known limitations** documented in the engineering
notes. If your finding is already documented, please open a regular discussion
or issue, not a security report.

## How to report

Use [GitHub Security Advisories](https://github.com/kwebhub/zkpool-mesh/security/advisories/new)
for private reports. Do **not** open a public issue for a security vulnerability.

Please include:

- A description of the issue.
- Steps to reproduce (minimal).
- Affected components (on-chain program, backend, prover, merkle, frontend,
  circuits, infra).
- Impact assessment (what an attacker can achieve).
- Any suggested fix or mitigation.

## Response timeline

Best-effort, solo-maintained:

| Stage | Target |
|---|---|
| Acknowledgment | 3 business days |
| Initial assessment | 7 business days |
| Fix or mitigation | 14 business days |
| Public disclosure | after fix, or 90 days after report |

## Scope

**In scope:**
- On-chain program (`onchain/`).
- Backend service (`services/backend/`).
- Prover service (`services/prover/`).
- Merkle service (`services/merkle/`).
- Frontend (`web/`).
- Circuits (`circuits/`).
- Scripts (`scripts/`).
- Infrastructure configs (`infra/`).

**Out of scope:**
- Solana L1, Anchor, Noir, Sunspot, gnark-solana — report upstream.
- Third-party Rust / npm dependencies — report upstream.
- Theoretical attacks without a proof-of-concept.
- Best-practice suggestions.
- DoS at unrealistic scale.
- Typos, missing documentation, style issues.

**Minimum bar:** we accept reports with a working reproduction. Theoretical
issues are welcome as discussions, not security advisories.

## What we do

- Run automated audits (`cargo-audit`, `cargo-deny`, `pnpm audit`) in CI.
- Pin dependency versions for reproducibility.
- Publish known limitations in `docs/ru/articles/` and `docs/PROJECT_CONTEXT.md` §6.
- Credit reporters in release notes (unless anonymity is requested).

## Disclosure policy

**We will not:**
- Pursue legal action against researchers acting in good faith.
- Demand silence beyond the coordinated disclosure window.
- Ignore a report because it is "out of scope" — we will respond with an
  assessment.

**We will:**
- Acknowledge the report within the stated timeline.
- Be honest if we cannot fix the issue.
- Give credit for the finding.

## Bug bounty

There is no bug bounty. Reports are appreciated and credited, but not paid.
