# Contributing to zkpool-mesh

Thanks for your interest. This is a **demo / educational project**. Read this
document before opening a PR.

## Before you start

1. Read `docs/PROJECT_CONTEXT.md` — it describes the project, the stages, and
   the rules (including the ones for contributors).
2. Read `docs/ru/articles/` — engineering notes for every stage.
3. Read `docs/PROJECT_CONTEXT.md` §6 (Known pitfalls) — the fastest way to
   avoid re-learning our mistakes.

## Setup

```bash
git clone git@github.com:kwebhub/zkpool-mesh.git
cd zkpool-mesh
make up
make status
```

If `make status` shows all services healthy — you are ready.

## Workflow

- Every change goes to a **feature branch**, then a **pull request**.
- Merge to `main` only if all CI workflows are green.
- Small, focused changes. One logical change per commit.
- Commits follow [Conventional Commits](https://www.conventionalcommits.org/):
  - `feat: ...`, `fix: ...`, `docs: ...`, `chore: ...`, `refactor: ...`,
    `test: ...`, `ci: ...`.
  - Scopes: `onchain`, `backend`, `prover`, `merkle`, `web`, `circuits`,
    `infra`, `docs`, `ci`.

## Code style

- **Rust** — `rustfmt` default (4 spaces). `cargo clippy` clean.
- **TypeScript / Vue** — `vue-tsc` clean, `pnpm typecheck` passes.
- **Shell / Makefile** — POSIX-friendly, tabs in Makefile recipes.
- Follow existing patterns in the nearest file.

## What gets merged

**Welcome:**
- Bug fixes with a test.
- Tests (unit, integration, E2E).
- Documentation improvements.
- Small features that don't change the public input layout.
- CI improvements.

**Discuss first (open an issue):**
- New services or new components.
- Changes to circuit schemas.
- Changes to the public input layout (breaking change).
- New runtime dependencies.
- Large refactors.

**Not accepted:**
- Changes that break the demo flow.
- Removing tests without a replacement.
- Removing information from `docs/` without a replacement.
- License changes.

## Pitfalls

The top-7 of `docs/PROJECT_CONTEXT.md` §6:

1. `bash -c` does not read `.bashrc` — use `bash -ic` inside the container.
2. Volume created from root becomes root-owned on host —
   `sudo chown -R 1000:1000 <dir>`.
3. Port mappings require `--force-recreate` on the container.
4. `anchor init .` fails — use a temp directory with a valid Rust identifier.
5. `sunspot deploy` requires `GNARK_VERIFIER_BIN` — set in Dockerfile.
6. `solana/cli/config.yml` gets into git — ignore the whole `solana/` folder.
7. Stale ACIR copies in consumers — run `make sync-circuits-check` before a
   circuit-consuming stage.

Full list: `docs/PROJECT_CONTEXT.md` §6.

## Security reporting

Please **do not** open a public issue for security vulnerabilities. See
[`SECURITY.md`](SECURITY.md).

## Getting help

- `docs/PROJECT_CONTEXT.md` — project overview and rules.
- `docs/ru/articles/` — engineering notes for every stage.
- GitHub Issues — for bugs and features.

## License

By contributing, you agree that your contributions will be licensed under the
MIT License — see [`LICENSE`](LICENSE).
