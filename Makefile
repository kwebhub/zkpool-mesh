SHELL := /bin/bash

COMPOSE   := docker compose -f infra/docker-compose.yml
SERVICE   := solana
CONTAINER := zkpool-mesh-app
PG        := zkpool-mesh-postgres
REDIS     := zkpool-mesh-redis

exec_c := $(COMPOSE) exec $(SERVICE) bash -ic
exec_d := $(COMPOSE) exec -d $(SERVICE) bash -ic

.PHONY: help init wipe up down status logs web build clean e2e \
        merkle prover backend web-dev wait-healthy \
        exec-c validate-spec sync-circuits-check sync-circuits-apply

help:
	@echo "zkpool-mesh — make targets:"
	@echo "  make init					 first-time setup: build image, apply migrations, generate ACIR, build services, start"
	@echo "  make wipe                   remove everything (containers, volumes, images, caches)"
	@echo "  make up                     start infra + services (after make down)"
	@echo "  make down                   stop all"
	@echo "  make status                 containers + processes + HTTP health + sync"
	@echo "  make logs                   tail service logs"
	@echo "  make web                    start Vite dev server"
	@echo "  make build                  build backend + prover + web"
	@echo "  make clean                  remove build artifacts"
	@echo "  make e2e                    run E2E deposit → withdraw, logs to /tmp"
	@echo "  make exec-c CMD='...'       run a single command inside the container"
	@echo "  make validate-spec          run spec validator"
	@echo "  make sync-circuits-check    verify ACIRs in sync (CI mode)"
	@echo "  make sync-circuits-apply    sync ACIRs to consumers"

init:
	@echo "==> [1/9] checking prerequisites"
	@test -f .env || (echo "   ⚠️  .env missing. Run: cp .env.example .env && edit"; exit 1)
	@test -f solana/id.json || (echo "   ⚠️  solana/id.json missing. Create a wallet first."; exit 1)
	@echo "==> [2/9] building image"
	@$(COMPOSE) build
	@echo "==> [3/9] starting containers"
	@$(COMPOSE) up -d
	@sleep 3
	@echo "==> [4/9] applying migrations"
	@docker exec -i $(PG) psql -U zkpool -d zkpool < services/backend/migrations/001_init.sql
	@echo "==> [5/9] generating ACIR"
	@$(exec_c) 'cd /home/ubuntu/circuits/hash2 && nargo compile'
	@$(exec_c) 'cd /home/ubuntu/circuits/hashes && nargo compile'
	@$(exec_c) 'cd /home/ubuntu/circuits/withdrawal && nargo compile'
	@echo "==> [6/9] syncing ACIR"
	@$(MAKE) sync-circuits-apply
	@echo "==> [7/9] installing JS dependencies"
	@$(exec_c) 'export NVM_DIR="$$HOME/.nvm" && . "$$NVM_DIR/nvm.sh" && export PNPM_HOME="/home/ubuntu/.local/share/pnpm" && export PATH="$$PNPM_HOME/bin:$$PATH" && cd /home/ubuntu/services/merkle && pnpm install'
	@$(exec_c) 'export NVM_DIR="$$HOME/.nvm" && . "$$NVM_DIR/nvm.sh" && export PNPM_HOME="/home/ubuntu/.local/share/pnpm" && export PATH="$$PNPM_HOME/bin:$$PATH" && cd /home/ubuntu/web && pnpm install'
	@echo "==> [8/9] building services"
	@$(MAKE) build
	@echo "==> [9/9] starting services"
	@$(MAKE) merkle
	@sleep 3
	@$(MAKE) prover backend
	@sleep 5
	@$(MAKE) wait-healthy
	@echo "==> checking sync"
	@$(MAKE) status
	@echo "==> init complete"

wipe:
	@echo "==> clearing in-container caches"
	@-$(exec_c) 'rm -rf /home/ubuntu/services/backend/target /home/ubuntu/services/prover/target /home/ubuntu/services/merkle/node_modules /home/ubuntu/web/node_modules 2>/dev/null || true'
	@echo "==> removing containers, volumes, network"
	@$(COMPOSE) down -v
	@echo "==> removing images"
	@docker rmi -f infra-solana:latest 2>/dev/null || true
	@docker rmi -f postgres:16-alpine redis:7-alpine prom/prometheus:v2.55.0 grafana/grafana:11.3.0 2>/dev/null || true
	@echo "==> clearing host caches"
	@rm -rf ~/.cargo/registry/cache ~/.cargo/git/db ~/.local/share/pnpm/store 2>/dev/null || true
	@echo "==> pruning docker builder + dangling images"
	@docker builder prune -af 2>/dev/null || true
	@docker image prune -f 2>/dev/null || true
	@echo "==> containers"
	@docker ps -a
	@echo "==> volumes"
	@docker volume ls
	@echo "==> images"
	@docker images

up:
	@echo "==> starting infra"
	@$(COMPOSE) up -d
	@sleep 3
	@echo "==> starting services"
	@$(MAKE) merkle
	@sleep 3
	@$(MAKE) prover backend
	@sleep 5
	@$(MAKE) wait-healthy

down:
	@echo "==> stopping app processes"
	@-$(exec_c) 'pkill -f zkpool-backend  || true'
	@-$(exec_c) 'pkill -f zkpool-prover   || true'
	@-$(exec_c) 'pkill -f "node src/server.js" || true'
	@-$(exec_c) 'pkill -f vite || true'
	@echo "==> docker compose down"
	@$(COMPOSE) down 2>/dev/null || true

merkle:
	@echo "==> merkle service (port 4003)"
	@$(exec_d) 'pkill -f "node src/server.js" || true; cd /home/ubuntu/services/merkle && export NVM_DIR="$$HOME/.nvm" && . "$$NVM_DIR/nvm.sh" && nohup node src/server.js > /tmp/merkle.log 2>&1 &'

prover:
	@echo "==> prover (port 4002)"
	@$(exec_d) 'pkill -f zkpool-prover || true; cd /home/ubuntu/services/prover && nohup ./target/release/zkpool-prover > /tmp/prover.log 2>&1 &'

backend:
	@echo "==> backend (port 4001)"
	@$(exec_d) 'pkill -f zkpool-backend || true; cd /home/ubuntu/services/backend && nohup ./target/release/zkpool-backend > /tmp/backend.log 2>&1 &'

web-dev:
	@echo "==> vite dev server (port 5173)"
	@$(exec_d) 'pkill -f vite || true; cd /home/ubuntu/web && export NVM_DIR="$$HOME/.nvm" && . "$$NVM_DIR/nvm.sh" && nohup pnpm dev > /tmp/web.log 2>&1 &'

web: web-dev
	@sleep 3
	@curl -sf -m 5 http://localhost:5173/ >/dev/null && echo "   ok  http://localhost:5173" || echo "   FAIL vite not reachable"

wait-healthy:
	@sleep 2
	@for p in 4001 4002 4003; do \
	  for i in 1 2 3 4 5; do \
	    if curl -sf -m 3 http://localhost:$$p/health >/dev/null 2>&1 || \
	       curl -sf -m 3 http://localhost:$$p/api/health >/dev/null 2>&1; then \
	      echo "   ok  port $$p"; break; \
	    fi; \
	    sleep 1; \
	  done; \
	done

status:
	@echo "==> containers"
	@$(COMPOSE) ps
	@echo "==> processes inside $(CONTAINER)"
	@$(exec_c) 'pgrep -a zkpool-backend || true; pgrep -a zkpool-prover || true; pgrep -af "node src/server.js" || true; pgrep -af vite || true'
	@echo "==> HTTP health"
	@for p in 4001 4002 4003; do \
	  code=$$(curl -s -o /dev/null -w "%{http_code}" -m 3 http://localhost:$$p/health 2>/dev/null); \
	  echo "   $$p -> $$code"; \
	done
	@echo "==> sync check (Postgres ↔ Redis)"
	@db_count=$$(docker exec $(PG) psql -U zkpool -d zkpool -t -c "SELECT COUNT(*) FROM commitments;" 2>/dev/null | tr -d ' '); \
	redis_count=$$(docker exec $(REDIS) redis-cli DBSIZE 2>/dev/null); \
	echo "   commitments: $$db_count (Postgres), $$redis_count keys (Redis)"

logs:
	@echo "==> /tmp/merkle.log"; $(exec_c) 'tail -20 /tmp/merkle.log 2>/dev/null || true'
	@echo "==> /tmp/prover.log"; $(exec_c) 'tail -20 /tmp/prover.log 2>/dev/null || true'
	@echo "==> /tmp/backend.log"; $(exec_c) 'tail -20 /tmp/backend.log 2>/dev/null || true'

build:
	@echo "==> backend"; $(exec_c) 'cd /home/ubuntu/services/backend && cargo build --release'
	@echo "==> prover";  $(exec_c) 'cd /home/ubuntu/services/prover  && cargo build --release'
	@echo "==> web";     $(exec_c) 'cd /home/ubuntu/web && export NVM_DIR="$$HOME/.nvm" && . "$$NVM_DIR/nvm.sh" && export PNPM_HOME="/home/ubuntu/.local/share/pnpm" && export PATH="$$PNPM_HOME/bin:$$PATH" && pnpm install --frozen-lockfile && pnpm build'

clean:
	@echo "==> removing build artifacts"
	@$(exec_c) 'rm -rf /home/ubuntu/services/backend/target /home/ubuntu/services/prover/target /home/ubuntu/web/dist /home/ubuntu/web/node_modules'

e2e:
	@echo "==> e2e deposit"
	@$(exec_c) 'cd /home/ubuntu && ./scripts/e2e-deposit/target/release/e2e-deposit' 2>&1 | tee /tmp/e2e-deposit.log
	@echo "==> extracting note from deposit log"
	@awk '/^=== NOTE/,/^}$$/' /tmp/e2e-deposit.log | sed '1d' > /tmp/note.json
	@python3 -c "import json; d=json.load(open('/tmp/note.json')); d['splits']=['00000000000000000000000000000000000000000000000000000000000f4240','0000000000000000000000000000000000000000000000000000000000000000','0000000000000000000000000000000000000000000000000000000000000000']; d['note_index']=0; json.dump(d,open('/tmp/note.json','w'),indent=2)"
	@cat /tmp/note.json
	@echo "==> waiting for indexer"
	@sleep 20
	@echo "==> copying note into container"
	@docker cp /tmp/note.json $(CONTAINER):/tmp/note.json
	@echo "==> e2e withdraw"
	@$(exec_c) 'cd /home/ubuntu && ./scripts/e2e-withdraw/target/release/e2e-withdraw /tmp/note.json HucM83aqVvwLepmKHpjtmtPY8HRcVNcfPK4Zmn1xPZwG' 2>&1 | tee /tmp/e2e-withdraw.log
	@echo "==> logs saved to /tmp/e2e-deposit.log and /tmp/e2e-withdraw.log"

exec-c:
	@$(COMPOSE) exec $(SERVICE) bash -ic '$(CMD)'

validate-spec:
	@$(exec_c) 'cd /home/ubuntu && cargo run --manifest-path scripts/validate-spec/Cargo.toml --release'

sync-circuits-check:
	@$(exec_c) 'cd /home/ubuntu && cargo run --manifest-path scripts/sync-circuits/Cargo.toml --release -- check'

sync-circuits-apply:
	@$(exec_c) 'cd /home/ubuntu && cargo run --manifest-path scripts/sync-circuits/Cargo.toml --release -- apply'
