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
  -v $(CURDIR)/.secrets:/home/i/.secrets

mesh_run := docker run --rm --network $(COMPOSE_NETWORK) -e GNARK_VERIFIER_BIN=/home/i/sunspot/gnark-solana/crates/verifier-bin $(CHAIN_VOLUMES) -w /home/i $(BASE_IMAGE) bash -c

.PHONY: help boot build-images up down migrations infra wipe chain chain-close \
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
	@echo "    make services      restart merkle/backend + health checks"
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

# ============================================================
# Zone 1 — boot (host, cache/)
# ============================================================
boot:
	@./scripts/boot.sh

# ============================================================
# Zone 2 — infra (images + compose + migrations)
# ============================================================
env-check:
	@test -f .env || { echo "==> [infra] .env not found, copying from .env.example"; cp .env.example .env; }

build-images: env-check
	@echo "==> [infra] building $(BASE_IMAGE)"
	@docker build -f infra/docker/Dockerfile.base -t $(BASE_IMAGE) .
	@echo "==> [infra] building $(NODE_BASE_IMAGE)"
	@docker build -f infra/docker/Dockerfile.node-base -t $(NODE_BASE_IMAGE) .
	@echo "==> [infra] building $(TEST_IMAGE)"
	@docker build -f infra/docker/Dockerfile.test -t $(TEST_IMAGE) .
	@echo "==> [infra] building $(FRONTEND_IMAGE)"
	@docker build -f infra/docker/Dockerfile.frontend -t $(FRONTEND_IMAGE) .

load-images:
	@echo "==> [infra] loading cached images from cache/images/"
	@for f in cache/images/*.tar; do \
	  [ -f "$$f" ] || continue; \
	  docker load -i "$$f" >/dev/null; \
	done

up: load-images
	@echo "==> [infra] docker compose up"
	@$(COMPOSE) up -d
	@sleep 3

migrations:
	@echo "==> [infra] applying migrations"
	@docker exec -i $(PG) psql -U zkpool -d zkpool < services/backend/migrations/001_init.sql

down:
	@$(COMPOSE) down

infra: build-images up migrations

wipe:
	@echo "==> [infra] removing containers, volumes, images, build cache (cache/ preserved)"
	@-$(COMPOSE) down -v --remove-orphans 2>/dev/null || true
	@-docker rmi -f $(BASE_IMAGE) $(NODE_BASE_IMAGE) $(TEST_IMAGE) $(FRONTEND_IMAGE) 2>/dev/null || true
	@-docker rmi -f debian:13 postgres:16-alpine redis:7-alpine prom/prometheus:v2.55.0 grafana/grafana:11.3.0 nginx:alpine 2>/dev/null || true
	@docker builder prune -af 2>/dev/null || true
	@docker image prune -af 2>/dev/null || true
	@docker volume prune -af 2>/dev/null || true

# ============================================================
# Zone 3 — on-chain (zkpool-base: anchor + circuits)
# ============================================================
chain:
	@echo "==> [chain 1/5] verify wallet + config"
	@$(mesh_run) 'test -f /home/i/.config/solana/id.json || { echo "ERROR: solana/id.json missing"; exit 1; }; solana config set --url $(LOCAL_RPC); solana-keygen pubkey /home/i/.config/solana/id.json'
	@echo "==> [chain 2/5] verifier: nargo compile → sunspot compile → setup → deploy"
	@$(mesh_run) 'set -e; export PATH=/home/i/sunspot/go:$$PATH; \
	  cd /home/i/circuits/withdrawal && nargo compile && \
	  sunspot compile target/withdrawal.json && \
	  sunspot setup target/withdrawal.ccs && \
	  cd target && rm -f withdrawal-keypair.json && sunspot deploy withdrawal.vk && \
	  solana program deploy withdrawal.so --program-id withdrawal-keypair.json --max-sign-attempts 1000 --url $(LOCAL_RPC) > /home/i/.secrets/deploy-verifier.log 2>&1; \
	  grep -A2 "12-word seed phrase" /home/i/.secrets/deploy-verifier.log | tail -1 | sed "s/^/verifier_buffer_seed: /" >> /home/i/.secrets/keys.md; \
	  grep -E "Program Id:|Signature:" /home/i/.secrets/deploy-verifier.log'
	@echo "==> [chain 3/5] update constants.rs with new VERIFIER_PROGRAM_ID"
	@$(mesh_run) 'set -e; \
	  VK_KEYPAIR=/home/i/circuits/withdrawal/target/withdrawal-keypair.json; \
	  NEW_VERIFIER=$$(solana-keygen pubkey $$VK_KEYPAIR); \
	  echo "   new VERIFIER_PROGRAM_ID=$$NEW_VERIFIER"; \
	  sed "s|pub const VERIFIER_PROGRAM_ID: Pubkey = pubkey!(\"[^\"]*\");|pub const VERIFIER_PROGRAM_ID: Pubkey = pubkey!(\"$$NEW_VERIFIER\");|" /home/i/onchain/programs/zk_pool/src/constants.rs > /tmp/constants.rs.new && cat /tmp/constants.rs.new > /home/i/onchain/programs/zk_pool/src/constants.rs && rm -f /tmp/constants.rs.new; \
	  sed "s|^VERIFIER_PROGRAM_ID=.*|VERIFIER_PROGRAM_ID=$$NEW_VERIFIER|" /home/i/.env > /tmp/.env.new && cat /tmp/.env.new > /home/i/.env && rm -f /tmp/.env.new; \
	  grep -E "^VERIFIER_PROGRAM_ID=" /home/i/.env'
	@echo "==> [chain 4/5] zk_pool: sync id → build → deploy"
	@$(mesh_run) 'set -e; cd /home/i/onchain && rm -f target/deploy/zk_pool-keypair.json && \
	  anchor build --no-idl -- --arch v0 2>&1 | tail -3; \
	  NEW_ID=$$(solana-keygen pubkey target/deploy/zk_pool-keypair.json); \
	  echo "   new zk_pool PROGRAM_ID=$$NEW_ID"; \
	  sed "s|declare_id!(\"[^\"]*\")|declare_id!(\"$$NEW_ID\")|" /home/i/onchain/programs/zk_pool/src/lib.rs > /tmp/lib.rs.new && cat /tmp/lib.rs.new > /home/i/onchain/programs/zk_pool/src/lib.rs && rm -f /tmp/lib.rs.new; \
	  sed "s|^zk_pool = .*|zk_pool = \"$$NEW_ID\"|" /home/i/onchain/Anchor.toml > /tmp/Anchor.toml.new && cat /tmp/Anchor.toml.new > /home/i/onchain/Anchor.toml && rm -f /tmp/Anchor.toml.new; \
	  sed "s|^PROGRAM_ID=.*|PROGRAM_ID=$$NEW_ID|" /home/i/.env > /tmp/.env.new && cat /tmp/.env.new > /home/i/.env && rm -f /tmp/.env.new; \
	  anchor build --no-idl -- --arch v0 2>&1 | tail -3; \
	  anchor idl build -o target/idl/zk_pool.json && \
	  solana program deploy target/deploy/zk_pool.so --program-id target/deploy/zk_pool-keypair.json --max-sign-attempts 1000 --url $(LOCAL_RPC) > /home/i/.secrets/deploy-zkpool.log 2>&1; \
	  grep -A2 "12-word seed phrase" /home/i/.secrets/deploy-zkpool.log | tail -1 | sed "s/^/zkpool_buffer_seed: /" >> /home/i/.secrets/keys.md; \
	  grep -E "Program Id:|Signature:" /home/i/.secrets/deploy-zkpool.log'
	@$(mesh_run) 'set -e; NEW_PROGRAM=$$(solana-keygen pubkey /home/i/onchain/target/deploy/zk_pool-keypair.json); \
	  echo "   new PROGRAM_ID=$$NEW_PROGRAM"; \
	  sed "s|^PROGRAM_ID=.*|PROGRAM_ID=$$NEW_PROGRAM|; s|^POOL_ADDRESS=.*|POOL_ADDRESS=|" /home/i/.env > /tmp/.env.new && cat /tmp/.env.new > /home/i/.env && rm -f /tmp/.env.new; \
	  grep -E "^(PROGRAM_ID|POOL_ADDRESS)=" /home/i/.env'
	@echo "==> [chain 5/5] pool-init"
	@$(mesh_run) 'set -e; cd /home/i && cargo build --release --manifest-path scripts/pool-init/Cargo.toml 2>&1 | tail -3 && \
	  ./scripts/pool-init/target/release/pool-init --rpc $(LOCAL_RPC) --keypair /home/i/.config/solana/id.json 2>&1 | tee /home/i/cache/pool-init.log'
	@NEW_POOL=$$(grep -E "^Pool PDA:" cache/pool-init.log | awk '{print $$3}'); \
	  [ -n "$$NEW_POOL" ] || { echo "ERROR: Pool PDA not found"; exit 1; }; \
	  echo "   new POOL_ADDRESS=$$NEW_POOL"; \
	  docker run --rm $(CHAIN_VOLUMES) -w /home/i $(BASE_IMAGE) bash -c "sed 's|^POOL_ADDRESS=.*|POOL_ADDRESS=$$NEW_POOL|' /home/i/.env > /tmp/.env.new && cat /tmp/.env.new > /home/i/.env && rm -f /tmp/.env.new && grep -E '^POOL_ADDRESS=' /home/i/.env"
	@$(MAKE) sync-ids
	@echo "==> [chain] complete"

chain-close:
	@echo "==> [chain] closing old programs from cache/old-programs.txt"
	@mkdir -p cache && touch cache/old-programs.txt
	@if [ ! -s cache/old-programs.txt ]; then echo "   (none)"; exit 0; fi
	@while read -r kind path; do \
	  [ -z "$$kind" ] && continue; \
	  echo "   closing $$kind $$path"; \
	  $(mesh_run) "solana program close --program-id $$path --url $(LOCAL_RPC) --bypass-warning" || echo "   WARN: close failed for $$path"; \
	done < cache/old-programs.txt
	@: > cache/old-programs.txt

# ============================================================
# Zone 4 — stack
# ============================================================

validate-spec:
	@echo "==> [stack] validate spec.json"
	@$(mesh_run) 'cd /home/i && cargo run --manifest-path scripts/validate-spec/Cargo.toml --release -- circuits/withdrawal/spec.json'

sync:
	@echo "==> [stack] sync ACIR"
	@$(mesh_run) 'cd /home/i && cargo run --manifest-path scripts/sync-circuits/Cargo.toml --release -- apply'

sync-check:
	@echo "==> [stack] sync-circuits check (CI mode, no writes)"
	@$(mesh_run) 'cd /home/i && cargo run --manifest-path scripts/sync-circuits/Cargo.toml --release -- check'

check:
	@echo "==> [stack] check .ccs/.pk from zone 3"
	@test -f circuits/withdrawal/target/withdrawal.ccs || { echo "ERROR: withdrawal.ccs missing — run make chain"; exit 1; }
	@test -f circuits/withdrawal/target/withdrawal.pk || { echo "ERROR: withdrawal.pk missing — run make chain"; exit 1; }
	@echo "   ok"

deps:
	@echo "==> [stack] pnpm install (merkle, frontend)"
	@docker exec zkpool-merkle bash -lc 'cd /home/i/services/merkle && pnpm install'
	@docker exec zkpool-web bash -lc 'cd /home/i/web && pnpm install'

mesh:
	@echo "==> [stack] build backend"
	@docker exec zkpool-mesh bash -lc 'cd /home/i/services/backend && cargo build --release'
	@echo "==> [stack] build prover"
	@docker exec zkpool-mesh bash -lc 'cd /home/i/services/prover && cargo build --release'
	@echo "==> [stack] build web"
	@docker exec zkpool-web bash -lc 'cd /home/i/web && pnpm install --frozen-lockfile && pnpm build'

services:
	@echo "==> [stack] restart merkle/backend/prover"
	@docker exec -d zkpool-merkle bash -lc 'cd /home/i/services/merkle && node src/server.js > /tmp/merkle.log 2>&1' || true
	@docker exec -d zkpool-mesh bash -lc 'cd /home/i/services/prover && ./target/release/zkpool-prover > /tmp/prover.log 2>&1' || true
	@docker exec -d zkpool-mesh bash -lc 'cd /home/i/services/backend && ./target/release/zkpool-backend > /tmp/backend.log 2>&1' || true
	@sleep 5
	@for entry in "4001 /api/health" "4002 /health" "4003 /health"; do \
	  p=$${entry%% *}; path=$${entry##* }; \
	  ok=0; for i in 1 2 3 4 5; do \
	    if curl -sf -m 3 "http://localhost:$$p$$path" >/dev/null 2>&1; then echo "   ok  port $$p$$path"; ok=1; break; fi; \
	    sleep 1; done; \
	  [ $$ok -eq 1 ] || echo "   FAIL port $$p$$path"; \
	done

stack: validate-spec sync check deps mesh e2e-build services

e2e:
	@echo "==> [stack] e2e deposit"
	@$(mesh_run) 'cd /home/i && ./scripts/e2e-deposit/target/release/e2e-deposit' 2>&1 | tee /tmp/e2e-deposit.log
	@echo "==> [stack] extracting note"
	@awk '/^=== NOTE/,/^}$$/' /tmp/e2e-deposit.log | sed '1d' > /tmp/note.json
	@python3 -c "import json; d=json.load(open('/tmp/note.json')); d['splits']=['00000000000000000000000000000000000000000000000000000000000f4240','0000000000000000000000000000000000000000000000000000000000000000','0000000000000000000000000000000000000000000000000000000000000000']; d['note_index']=0; json.dump(d,open('/tmp/note.json','w'),indent=2)"
	@cat /tmp/note.json
	@echo "==> [stack] waiting for indexer (poll /api/commitments, up to 60s)"
	@COMMIT=$$(python3 -c "import json; print(json.load(open('/tmp/note.json'))['commitment'])"); \
	  for i in $$(seq 1 30); do \
	    if curl -s http://localhost:4001/api/commitments | grep -q "$$COMMIT"; then echo "   ok  commitment indexed"; break; fi; \
	    sleep 2; \
	  done; \
	  curl -s http://localhost:4001/api/commitments | grep -q "$$COMMIT" || { echo "   FAIL commitment not indexed"; exit 1; }
	@docker cp /tmp/note.json zkpool-mesh:/tmp/note.json
	@echo "==> [stack] e2e withdraw"
	@docker exec zkpool-mesh bash -lc 'cd /home/i && ./scripts/e2e-withdraw/target/release/e2e-withdraw /tmp/note.json $(E2E_RECIPIENT)' 2>&1 | tee /tmp/e2e-withdraw.log

e2e-build:
	@echo "==> [stack] build e2e binaries"
	@$(mesh_run) 'cd /home/i && cargo build --release --manifest-path scripts/e2e-deposit/Cargo.toml && cargo build --release --manifest-path scripts/e2e-deposit-split/Cargo.toml && cargo build --release --manifest-path scripts/e2e-withdraw/Cargo.toml'

e2e-split:
	@echo "==> [stack] e2e deposit-split"
	@$(mesh_run) 'cd /home/i && ./scripts/e2e-deposit-split/target/release/e2e-deposit-split' 2>&1 | tee /tmp/e2e-split.log
	@echo "==> [stack] extracting 3 notes"
	@python3 -c "import re,json; \
	  data=open('/tmp/e2e-split.log').read(); \
	  parts=re.split(r'--- Note \d+ of \d+ ---', data); \
	  notes=[json.loads(p.strip()) for p in parts[1:] if p.strip().startswith('{')]; \
	  assert len(notes)>=3, f'expected 3 notes, got {len(notes)}'; \
	  json.dump(notes[:3], open('/tmp/notes.json','w'), indent=2)"
	@echo "==> [stack] waiting for indexer (3 commitments, up to 120s)"
	@COMMITS=$$(python3 -c "import json; print(' '.join(n['commitment'] for n in json.load(open('/tmp/notes.json'))))"); \
	  for i in $$(seq 1 60); do \
	    ok=1; for c in $$COMMITS; do \
	      curl -s http://localhost:4001/api/commitments | grep -q "$$c" || ok=0; done; \
	    [ $$ok -eq 1 ] && { echo "   ok  3 commitments indexed"; break; }; \
	    sleep 2; \
	  done; \
	  [ $$ok -eq 1 ] || { echo "   FAIL not all 3 commitments indexed"; exit 1; }
	@echo "==> [stack] splitting notes into /tmp/note{0,1,2}.json"
	@python3 -c "import json; \
	  notes=json.load(open('/tmp/notes.json')); \
	  [json.dump(n, open(f'/tmp/note{i}.json','w'), indent=2) for i,n in enumerate(notes)]"
	@for i in 0 1 2; do \
	  docker cp /tmp/note$$i.json zkpool-mesh:/tmp/note$$i.json; \
	done
	@echo "==> [stack] e2e withdraw x3"
	@for i in 0 1 2; do \
	  docker exec zkpool-mesh bash -lc "cd /home/i && ./scripts/e2e-withdraw/target/release/e2e-withdraw /tmp/note$$i.json $(E2E_RECIPIENT)" 2>&1 | tee /tmp/e2e-withdraw-$$i.log; \
	done

# ============================================================
# Full cycle
# ============================================================
redeploy: wipe boot chain infra stack e2e
	@echo "==> redeploy complete"

# ============================================================
# Diagnostics
# ============================================================
status:
	@echo "==> containers"
	@$(COMPOSE) ps
	@echo "==> HTTP health"
	@for entry in "4001 /api/health" "4002 /health" "4003 /health"; do \
	  p=$${entry%% *}; path=$${entry##* }; \
	  code=$$(curl -s -o /dev/null -w "%{http_code}" -m 3 "http://localhost:$$p$$path" 2>/dev/null); \
	  echo "   $$p$$path -> $$code"; \
	done
	@echo "==> sync (Postgres ↔ Redis)"
	@db=$$(docker exec $(PG) psql -U zkpool -d zkpool -t -c "SELECT COUNT(*) FROM commitments;" 2>/dev/null | tr -d ' '); \
	redis=$$(docker exec $(REDIS) redis-cli DBSIZE 2>/dev/null); \
	echo "   commitments: $$db (Postgres), $$redis keys (Redis)"

sync-ids:
	@echo "==> check PROGRAM_ID / VERIFIER_PROGRAM_ID consistency"
	@DECL=$$(grep -oP 'declare_id!\("\K[^"]+' onchain/programs/zk_pool/src/lib.rs); \
	  ENV_ID=$$(grep -E "^PROGRAM_ID=" .env | cut -d= -f2); \
	  if [ "$$DECL" != "$$ENV_ID" ]; then echo "   DRIFT PROGRAM_ID: lib.rs=$$DECL .env=$$ENV_ID"; exit 1; fi; \
	  echo "   ok PROGRAM_ID=$$DECL"
	@VER_CONST=$$(grep -oP 'VERIFIER_PROGRAM_ID: Pubkey = pubkey!\("\K[^"]+' onchain/programs/zk_pool/src/constants.rs); \
	  VER_ENV=$$(grep -E "^VERIFIER_PROGRAM_ID=" .env | cut -d= -f2); \
	  if [ "$$VER_CONST" != "$$VER_ENV" ]; then echo "   DRIFT VERIFIER_PROGRAM_ID: constants.rs=$$VER_CONST .env=$$VER_ENV"; exit 1; fi; \
	  echo "   ok VERIFIER_PROGRAM_ID=$$VER_CONST"

verify: sync-ids
	@echo "==> on-chain pool"
	@docker exec zkpool-mesh bash -lc 'solana account $$(grep -E "^POOL_ADDRESS=" /home/i/.env | cut -d= -f2) --url $(LOCAL_RPC) 2>&1 | head -8' || true
	@echo "==> postgres commitments"
	@docker exec $(PG) psql -U zkpool -d zkpool -c "SELECT leaf_index, encode(commitment,'hex') FROM commitments ORDER BY leaf_index;" 2>/dev/null || echo "   (db not ready)"
	@echo "==> postgres roots (last 3)"
	@docker exec $(PG) psql -U zkpool -d zkpool -c "SELECT leaf_index, encode(root,'hex') FROM roots ORDER BY id DESC LIMIT 3;" 2>/dev/null || true
	@echo "==> redis tree size"
	@POOL=$$(grep -E "^POOL_ADDRESS=" .env | cut -d= -f2); \
	  SZ=$$(docker exec $(REDIS) redis-cli GET "tree:$$POOL:size" 2>/dev/null); \
	  if [ -z "$$SZ" ] || [ "$$SZ" = "(nil)" ]; then echo "   дерево ещё не инициализировано"; else echo "   $$SZ"; fi
	@echo "==> redis tree root"
	@POOL=$$(grep -E "^POOL_ADDRESS=" .env | cut -d= -f2); \
	  RT=$$(docker exec $(REDIS) redis-cli GET "tree:$$POOL:root" 2>/dev/null); \
	  if [ -z "$$RT" ] || [ "$$RT" = "(nil)" ]; then echo "   дерево ещё не инициализировано"; else echo "   $$RT"; fi
	@echo "==> api /api/commitments (count)"
	@curl -s -m 3 http://localhost:4001/api/commitments | python3 -c "import json,sys; d=json.load(sys.stdin); print('count=', d['count'])" 2>/dev/null || echo "   (api not reachable)"
	@echo "==> api /api/root"
	@curl -s -m 3 http://localhost:4001/api/root 2>/dev/null || echo "   (api not reachable)"
	@echo "==> api /api/proof?leaf_index=last"
	@LAST=$$(docker exec $(PG) psql -U zkpool -d zkpool -t -c "SELECT COALESCE(MAX(leaf_index),0) FROM commitments;" 2>/dev/null | tr -d ' '); \
	  curl -s -m 5 "http://localhost:4001/api/proof?leaf_index=$$LAST" | head -c 200; echo

logs:
	@echo "==> merkle";  docker logs --tail 20 zkpool-merkle 2>/dev/null || true
	@echo "==> backend"; docker exec zkpool-mesh bash -lc 'tail -20 /tmp/backend.log 2>/dev/null' || true
	@echo "==> prover";  docker exec zkpool-mesh bash -lc 'tail -20 /tmp/prover.log 2>/dev/null' || true
