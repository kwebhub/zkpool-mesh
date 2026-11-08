SHELL := /bin/bash

COMPOSE   := docker compose -f infra/docker-compose.yml
SERVICE   := solana
CONTAINER := zkpool-mesh-app
PG        := zkpool-mesh-postgres
REDIS     := zkpool-mesh-redis

CHAIN_IMAGE := zkpool-chain-builder:latest
CHAIN_VOLUMES := \
  -v $(CURDIR)/.env:/home/ubuntu/.env \
  -v $(CURDIR)/solana:/home/ubuntu/.config/solana \
  -v $(CURDIR)/onchain:/home/ubuntu/onchain \
  -v $(CURDIR)/circuits:/home/ubuntu/circuits \
  -v $(CURDIR)/scripts:/home/ubuntu/scripts \
  -v $(CURDIR)/cache:/home/ubuntu/cache

exec_c := $(COMPOSE) exec $(SERVICE) bash -ic
exec_d := $(COMPOSE) exec -d $(SERVICE) bash -ic
chain  := docker run --rm -e GNARK_VERIFIER_BIN=/home/ubuntu/sunspot/gnark-solana/crates/verifier-bin $(CHAIN_VOLUMES) -w /home/ubuntu $(CHAIN_IMAGE) bash -c

.PHONY: help bootstrap host-bootstrap host-build host-up host-down host-migrations host-wipe \
        chain-image chain-all chain-close-old \
        stack-sync stack-check stack-deps stack-build stack-services stack-e2e \
        redeploy status logs e2e verify

help:
	@echo "zkpool-mesh — make targets:"
	@echo "  make boot        zone 1: warm cache/ (Go, anchor, sunspot, images, installers)"
	@echo "  make chain       zone 2: verifier + zk_pool + pool"
	@echo "  make infra       zone 3: build infra-solana image from cache/ + docker compose up + apply SQL migrations"
	@echo "  make down        zone 3: docker compose down"
	@echo "  make stack       zone 4: sync ACIR to merkle/web + verify .ccs/.pk exist (from zone 2) + pnpm install + cargo build + pnpm build + start merkle/prover/backend"
	@echo "  make e2e         zone 4: deposit → wait indexer → withdraw"
	@echo "  make redeploy    full cycle: wipe → boot → chain → infra → stack → e2e"
	@echo "  make wipe        remove containers, volumes, images (cache/ preserved)"
	@echo "  make status, logs, verify"

# ============================================================
# Zone 1 — bootstrap (host, cache/)
# ============================================================
boot:
	@./scripts/bootstrap.sh

# ============================================================
# Zone 2 — on-chain (temporary chain-builder)
# ============================================================
chain-image:
	@echo "==> [chain] building $(CHAIN_IMAGE) from cache/"
	@docker build -f infra/docker/Dockerfile.chain -t $(CHAIN_IMAGE) .

chain: chain-image
	@echo "==> [chain 1/5] verify wallet"
	@$(chain) 'test -f /home/ubuntu/.config/solana/id.json || { echo "ERROR: solana/id.json missing"; exit 1; }; solana-keygen pubkey /home/ubuntu/.config/solana/id.json'
	@echo "==> [chain 2/5] verifier: nargo compile → sunspot compile → setup → deploy"
	@$(chain) 'set -e; export PATH=/home/ubuntu/sunspot/go:$$PATH; \
	  cd /home/ubuntu/circuits/withdrawal && nargo compile && \
	  sunspot compile target/withdrawal.json && \
	  sunspot setup target/withdrawal.ccs && \
	  cd target && rm -f withdrawal-keypair.json && sunspot deploy withdrawal.vk && \
	  solana program deploy withdrawal.so --program-id withdrawal-keypair.json --url devnet'
	@echo "==> [chain 3/5] update constants.rs with new VERIFIER_PROGRAM_ID"
	@$(chain) 'set -e; \
	  VK_KEYPAIR=/home/ubuntu/circuits/withdrawal/target/withdrawal-keypair.json; \
	  NEW_VERIFIER=$$(solana-keygen pubkey $$VK_KEYPAIR); \
	  echo "   new VERIFIER_PROGRAM_ID=$$NEW_VERIFIER"; \
	  sed -i "s|pub const VERIFIER_PROGRAM_ID: Pubkey = pubkey!(\"[^\"]*\");|pub const VERIFIER_PROGRAM_ID: Pubkey = pubkey!(\"$$NEW_VERIFIER\");|" /home/ubuntu/onchain/programs/zk_pool/src/constants.rs; \
	  sed "s|^VERIFIER_PROGRAM_ID=.*|VERIFIER_PROGRAM_ID=$$NEW_VERIFIER|" /home/ubuntu/.env > /tmp/.env.new; cat /tmp/.env.new > /home/ubuntu/.env; rm -f /tmp/.env.new; \
	  grep -E "^VERIFIER_PROGRAM_ID=" /home/ubuntu/.env'
	@echo "==> [chain 4/5] zk_pool: anchor keys sync → build → deploy"
	@$(chain) 'set -e; cd /home/ubuntu/onchain && rm -f target/deploy/zk_pool-keypair.json && anchor keys sync && \
	  anchor build --no-idl -- --arch v0 && \
	  anchor idl build -o target/idl/zk_pool.json && \
	  solana program deploy target/deploy/zk_pool.so --program-id target/deploy/zk_pool-keypair.json --url devnet'
	@$(chain) 'set -e; NEW_PROGRAM=$$(solana-keygen pubkey /home/ubuntu/onchain/target/deploy/zk_pool-keypair.json); \
	  echo "   new PROGRAM_ID=$$NEW_PROGRAM"; \
	  sed "s|^PROGRAM_ID=.*|PROGRAM_ID=$$NEW_PROGRAM|; s|^POOL_ADDRESS=.*|POOL_ADDRESS=|" /home/ubuntu/.env > /tmp/.env.new; cat /tmp/.env.new > /home/ubuntu/.env; rm -f /tmp/.env.new; \
	  grep -E "^(PROGRAM_ID|POOL_ADDRESS)=" /home/ubuntu/.env'
	@echo "==> [chain 5/5] pool-init"
	@$(chain) 'set -e; cd /home/ubuntu && cargo build --release --manifest-path scripts/pool-init/Cargo.toml 2>&1 | tail -3 && \
	  ./scripts/pool-init/target/release/pool-init --rpc https://api.devnet.solana.com --keypair /home/ubuntu/.config/solana/id.json 2>&1 | tee /home/ubuntu/cache/pool-init.log'
	@NEW_POOL=$$(grep -E "^Pool PDA:" cache/pool-init.log | awk '{print $$3}'); \
	  [ -n "$$NEW_POOL" ] || { echo "ERROR: Pool PDA not found"; exit 1; }; \
	  echo "   new POOL_ADDRESS=$$NEW_POOL"; \
	  docker run --rm $(CHAIN_VOLUMES) -w /home/ubuntu $(CHAIN_IMAGE) bash -c "sed 's|^POOL_ADDRESS=.*|POOL_ADDRESS=$$NEW_POOL|' /home/ubuntu/.env > /tmp/.env.new && cat /tmp/.env.new > /home/ubuntu/.env && rm -f /tmp/.env.new && grep -E '^POOL_ADDRESS=' /home/ubuntu/.env"
	@echo "==> [chain] complete"

chain-close:
	@echo "==> [chain] closing old programs from cache/old-programs.txt"
	@mkdir -p cache && touch cache/old-programs.txt
	@if [ ! -s cache/old-programs.txt ]; then echo "   (none)"; exit 0; fi
	@while read -r kind path; do \
	  [ -z "$$kind" ] && continue; \
	  echo "   closing $$kind $$path"; \
	  docker run --rm $(CHAIN_VOLUMES) -w /home/ubuntu $(CHAIN_IMAGE) bash -c \
	    "solana program close --program-id $$path --url devnet --bypass-warning" || echo "   WARN: close failed for $$path"; \
	done < cache/old-programs.txt
	@: > cache/old-programs.txt

# ============================================================
# Zone 3 — infra (image + compose + migrations)
# ============================================================
build:
	@echo "==> [infra] building infra-solana from cache/"
	@$(COMPOSE) build

up:
	@echo "==> [infra] docker compose up"
	@$(COMPOSE) up -d
	@sleep 3

migrations:
	@echo "==> [infra] applying migrations"
	@docker exec -i $(PG) psql -U zkpool -d zkpool < services/backend/migrations/001_init.sql

down:
	@$(COMPOSE) down

infra: build up migrations

wipe:
	@echo "==> [infra] removing containers, volumes, images (cache/ preserved)"
	@-$(COMPOSE) down -v 2>/dev/null || true
	@-docker rmi -f infra-solana:latest $(CHAIN_IMAGE) 2>/dev/null || true
	@-docker rmi -f postgres:16-alpine redis:7-alpine prom/prometheus:v2.55.0 grafana/grafana:11.3.0 2>/dev/null || true
	@docker builder prune -af 2>/dev/null || true
	@docker image prune -f 2>/dev/null || true

# ============================================================
# Zone 4 — stack (inside zkpool-mesh-app)
# ============================================================
sync:
	@echo "==> [stack] sync ACIR"
	@$(exec_c) 'cd /home/ubuntu && cargo run --manifest-path scripts/sync-circuits/Cargo.toml --release -- apply'

check:
	@echo "==> [stack] check .ccs/.pk from zone 2"
	@$(exec_c) 'test -f /home/ubuntu/circuits/withdrawal/target/withdrawal.ccs || { echo "ERROR: withdrawal.ccs missing — run make chain-all"; exit 1; }; test -f /home/ubuntu/circuits/withdrawal/target/withdrawal.pk || { echo "ERROR: withdrawal.pk missing — run make chain-all"; exit 1; }; echo "   ok"'

deps:
	@echo "==> [stack] pnpm install"
	@$(exec_c) 'export NVM_DIR="$$HOME/.nvm" && . "$$NVM_DIR/nvm.sh" && export PNPM_HOME="/home/ubuntu/.local/share/pnpm" && export PATH="$$PNPM_HOME/bin:$$PATH" && cd /home/ubuntu/services/merkle && pnpm install'
	@$(exec_c) 'export NVM_DIR="$$HOME/.nvm" && . "$$NVM_DIR/nvm.sh" && export PNPM_HOME="/home/ubuntu/.local/share/pnpm" && export PATH="$$PNPM_HOME/bin:$$PATH" && cd /home/ubuntu/web && pnpm install'

build:
	@echo "==> [stack] build backend"
	@$(exec_c) 'cd /home/ubuntu/services/backend && cargo build --release'
	@echo "==> [stack] build prover"
	@$(exec_c) 'cd /home/ubuntu/services/prover && cargo build --release'
	@echo "==> [stack] build web"
	@$(exec_c) 'export NVM_DIR="$$HOME/.nvm" && . "$$NVM_DIR/nvm.sh" && export PNPM_HOME="/home/ubuntu/.local/share/pnpm" && export PATH="$$PNPM_HOME/bin:$$PATH" && cd /home/ubuntu/web && pnpm install --frozen-lockfile && pnpm build'

services:
	@echo "==> [stack] merkle (4003)"
	@$(exec_d) 'pkill -f "node src/server.js" || true; cd /home/ubuntu/services/merkle && export NVM_DIR="$$HOME/.nvm" && . "$$NVM_DIR/nvm.sh" && nohup node src/server.js > /tmp/merkle.log 2>&1 &'
	@sleep 3
	@echo "==> [stack] prover (4002)"
	@$(exec_d) 'pkill -f zkpool-prover || true; cd /home/ubuntu/services/prover && nohup ./target/release/zkpool-prover > /tmp/prover.log 2>&1 &'
	@echo "==> [stack] backend (4001)"
	@$(exec_d) 'pkill -f zkpool-backend || true; cd /home/ubuntu/services/backend && nohup ./target/release/zkpool-backend > /tmp/backend.log 2>&1 &'
	@sleep 5
	@for p in 4001 4002 4003; do \
	  ok=0; for i in 1 2 3 4 5; do \
	    if curl -sf -m 3 http://localhost:$$p/health >/dev/null 2>&1 || \
	       curl -sf -m 3 http://localhost:$$p/api/health >/dev/null 2>&1; then echo "   ok  port $$p"; ok=1; break; fi; \
	    sleep 1; done; \
	  [ $$ok -eq 1 ] || echo "   FAIL port $$p"; \
	done

stack: sync check deps build services

e2e:
	@echo "==> [stack] e2e deposit"
	@$(exec_c) 'cd /home/ubuntu && ./scripts/e2e-deposit/target/release/e2e-deposit' 2>&1 | tee /tmp/e2e-deposit.log
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
	@docker cp /tmp/note.json $(CONTAINER):/tmp/note.json
	@echo "==> [stack] e2e withdraw"
	@$(exec_c) 'cd /home/ubuntu && ./scripts/e2e-withdraw/target/release/e2e-withdraw /tmp/note.json HucM83aqVvwLepmKHpjtmtPY8HRcVNcfPK4Zmn1xPZwG' 2>&1 | tee /tmp/e2e-withdraw.log

# ============================================================
# Full cycle
# ============================================================
redeploy: wipe chain-close chain infra stack e2e
	@echo "==> redeploy complete"

# ============================================================
# Diagnostics
# ============================================================
status:
	@echo "==> containers"
	@$(COMPOSE) ps
	@echo "==> processes in $(CONTAINER)"
	@$(exec_c) 'pgrep -a zkpool-backend || true; pgrep -a zkpool-prover || true; pgrep -af "node src/server.js" || true'
	@echo "==> HTTP health"
	@for p in 4001 4002 4003; do \
	  code=$$(curl -s -o /dev/null -w "%{http_code}" -m 3 http://localhost:$$p/health 2>/dev/null); \
	  echo "   $$p -> $$code"; \
	done
	@echo "==> sync (Postgres ↔ Redis)"
	@db=$$(docker exec $(PG) psql -U zkpool -d zkpool -t -c "SELECT COUNT(*) FROM commitments;" 2>/dev/null | tr -d ' '); \
	redis=$$(docker exec $(REDIS) redis-cli DBSIZE 2>/dev/null); \
	echo "   commitments: $$db (Postgres), $$redis keys (Redis)"

verify:
	@echo "==> on-chain pool"
	@$(exec_c) 'solana account $$(grep -E "^POOL_ADDRESS=" /home/ubuntu/.env | cut -d= -f2) --url devnet 2>&1 | head -8' || true
	@echo "==> on-chain next_leaf_index (raw hex at 0x28)"
	@$(exec_c) 'solana account $$(grep -E "^POOL_ADDRESS=" /home/ubuntu/.env | cut -d= -f2) --url devnet 2>&1 | sed -n "/^0020:/p"' || true
	@echo "==> postgres commitments"
	@docker exec $(PG) psql -U zkpool -d zkpool -c "SELECT leaf_index, encode(commitment,'hex') FROM commitments ORDER BY leaf_index;" 2>/dev/null || echo "   (db not ready)"
	@echo "==> postgres roots (last 3)"
	@docker exec $(PG) psql -U zkpool -d zkpool -c "SELECT leaf_index, encode(root,'hex') FROM roots ORDER BY id DESC LIMIT 3;" 2>/dev/null || true
	@echo "==> redis tree size"
	@POOL=$$(grep -E "^POOL_ADDRESS=" .env | cut -d= -f2); \
	  docker exec $(REDIS) redis-cli GET "tree:$$POOL:size" 2>/dev/null || echo "   (redis not ready)"
	@echo "==> redis tree root"
	@POOL=$$(grep -E "^POOL_ADDRESS=" .env | cut -d= -f2); \
	  docker exec $(REDIS) redis-cli GET "tree:$$POOL:root" 2>/dev/null || true
	@echo "==> indexer last_signature"
	@docker exec $(REDIS) redis-cli GET "indexer:last_signature" 2>/dev/null || true
	@echo "==> indexer log (last 5 deposit/tree lines)"
	@$(exec_c) 'grep -E "deposit indexed|tree update failed|already in DB" /tmp/backend.log 2>/dev/null | tail -5' || true
	@echo "==> api /api/commitments (count)"
	@curl -s -m 3 http://localhost:4001/api/commitments | python3 -c "import json,sys; d=json.load(sys.stdin); print('count=', d['count'])" 2>/dev/null || echo "   (api not reachable)"
	@echo "==> api /api/root"
	@curl -s -m 3 http://localhost:4001/api/root 2>/dev/null || echo "   (api not reachable)"
	@echo "==> api /api/proof?leaf_index=last"
	@LAST=$$(docker exec $(PG) psql -U zkpool -d zkpool -t -c "SELECT COALESCE(MAX(leaf_index),0) FROM commitments;" 2>/dev/null | tr -d ' '); \
	  curl -s -m 5 "http://localhost:4001/api/proof?leaf_index=$$LAST" | head -c 200; echo

logs:
	@echo "==> /tmp/merkle.log"; $(exec_c) 'tail -20 /tmp/merkle.log 2>/dev/null || true'
	@echo "==> /tmp/prover.log"; $(exec_c) 'tail -20 /tmp/prover.log 2>/dev/null || true'
	@echo "==> /tmp/backend.log"; $(exec_c) 'tail -20 /tmp/backend.log 2>/dev/null || true'
