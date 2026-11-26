SHELL := /bin/bash

COMPOSE := docker compose -f infra/compose.yml

PG    := zkpool-postgres
REDIS := zkpool-redis

E2E_RECIPIENT := $(shell grep -E '^E2E_RECIPIENT=' .env 2>/dev/null | cut -d= -f2)

BASE_IMAGE      := zkpool-base:latest
NODE_BASE_IMAGE := zkpool-node-base:latest
TEST_IMAGE      := zkpool-test:latest
FRONTEND_IMAGE  := zkpool-frontend:latest

LOCAL_RPC := http://solana-test-validator:8899
COMPOSE_NETWORK := infra_default

CHAIN_VOLUMES := \
  -v $(CURDIR)/.env:/home/i/.env \
  -v $(CURDIR)/solana:/home/i/.config/solana \
  -v $(CURDIR)/onchain:/home/i/onchain \
  -v $(CURDIR)/circuits:/home/i/circuits \
  -v $(CURDIR)/scripts:/home/i/scripts \
  -v $(CURDIR)/cache:/home/i/cache \
  -v $(CURDIR)/.secrets:/home/i/.secrets

mesh_run := docker run --rm --network $(COMPOSE_NETWORK) -e GNARK_VERIFIER_BIN=/home/i/sunspot/gnark-solana/crates/verifier-bin $(CHAIN_VOLUMES) -w /home/i $(BASE_IMAGE) bash -c

.PHONY: help boot build-images pin-versions up down migrations airdrop infra wipe chain chain-close \
        validate-spec sync sync-check check deps mesh services stack \
        e2e e2e-build e2e-split redeploy status logs verify sync-ids

help:
	@echo "zkpool-mesh — make targets:"
	@echo ""
	@echo "  zone 1:"
	@echo "    make boot          warm cache/ + create wallet"
	@echo ""
	@echo "  zone 2:"
	@echo "    make build-images  build zkpool-base, node-base, test, frontend"
	@echo "    make up            docker compose up"
	@echo "    make migrations    apply SQL migrations to postgres"
	@echo "    make down          docker compose down"
	@echo "    make infra         build-images + up + migrations"
	@echo ""
	@echo "  zone 3:"
	@echo "    make chain         deploy verifier + zk_pool + pool-init"
	@echo "    make chain-close   close old programs from cache/old-programs.txt"
	@echo "    make sync-ids      verify PROGRAM_ID / VERIFIER_PROGRAM_ID match .env"
	@echo ""
	@echo "  zone 4:"
	@echo "    make validate-spec validate circuits/withdrawal/spec.json"
	@echo "    make sync          copy ACIR files to merkle/web"
	@echo "    make sync-check    check ACIR sync (CI mode, no writes)"
	@echo "    make check         verify .ccs/.pk exist (from make chain)"
	@echo "    make deps          pnpm install in merkle and frontend"
	@echo "    make mesh          cargo build backend/prover + pnpm build web"
	@echo "    make e2e-build     build e2e binaries"
	@echo "    make services      restart merkle/backend/prover + health checks"
	@echo "    make stack         validate-spec + sync + check + deps + mesh + e2e-build + services"
	@echo "    make e2e           deposit → wait indexer → withdraw (1 note)"
	@echo "    make e2e-split     split deposit (3 notes) → withdraw each"
	@echo ""
	@echo "  full:"
	@echo "    make redeploy      wipe → boot → chain → infra → stack → e2e"
	@echo "    make wipe          remove containers, volumes, images (cache/ preserved)"
	@echo "    make status        containers + health + Postgres/Redis counts"
	@echo "    make logs          tail merkle/backend/frontend logs"
	@echo "    make verify        on-chain pool + Postgres commitments + roots + API"

boot:
	@./scripts/boot.sh

include infra/mk/infra.mk
include infra/mk/chain.mk
include infra/mk/stack.mk
include infra/mk/diag.mk

redeploy: wipe boot infra chain stack e2e
	@echo "==> redeploy complete"
