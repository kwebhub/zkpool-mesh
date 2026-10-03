SHELL := /bin/bash

COMPOSE   := docker compose -f infra/docker-compose.yml
SERVICE   := solana
CONTAINER := zkpool-mesh
PG        := zkpool-postgres
REDIS     := zkpool-redis

exec_c := $(COMPOSE) exec $(SERVICE) bash -ic
exec_d := $(COMPOSE) exec -d $(SERVICE) bash -ic

.PHONY: help up down reset status logs web build clean \
        merkle prover backend web-dev wait-healthy \
        exec-c validate-spec sync-circuits-check sync-circuits-apply

help:
	@echo "zkpool-mesh — make targets:"
	@echo "  make up                     bring up infra + services"
	@echo "  make down                   stop all"
	@echo "  make reset                  down + up + clear DB/Redis + services"
	@echo "  make status                 containers + processes + HTTP health"
	@echo "  make logs                   tail service logs"
	@echo "  make web                    start Vite dev server"
	@echo "  make build                  build backend + prover + web"
	@echo "  make clean                  remove build artifacts"
	@echo "  make exec-c CMD='...'       run a single command inside the container"
	@echo "  make validate-spec          run spec validator"
	@echo "  make sync-circuits-check    verify ACIRs in sync (CI mode)"
	@echo "  make sync-circuits-apply    sync ACIRs to consumers"

up:
	@echo "==> docker compose up -d"
	@$(COMPOSE) up -d
	@sleep 3
	@$(MAKE) merkle prover backend wait-healthy

down:
	@echo "==> stopping app processes"
	@-$(exec_c) 'pkill -f zkpool-backend  || true'
	@-$(exec_c) 'pkill -f zkpool-prover   || true'
	@-$(exec_c) 'pkill -f "node src/server.js" || true'
	@-$(exec_c) 'pkill -f vite || true'
	@echo "==> docker compose down"
	@$(COMPOSE) down 2>/dev/null || true

reset: down
	@echo "==> up infra"
	@$(COMPOSE) up -d
	@sleep 3
	@echo "==> clearing Postgres + Redis"
	@docker exec $(PG) psql -U zkpool -d zkpool -c \
	  "TRUNCATE commitments, roots, nullifiers RESTART IDENTITY CASCADE;" || true
	@docker exec $(REDIS) redis-cli FLUSHALL || true
	@$(MAKE) merkle prover backend wait-healthy

merkle:
	@echo "==> merkle service (port 4003)"
	@$(exec_d) 'pkill -f "node src/server.js" || true; cd /home/ubuntu/services/merkle && nohup node src/server.js > /tmp/merkle.log 2>&1 &'

prover:
	@echo "==> prover (port 4002)"
	@$(exec_d) 'pkill -f zkpool-prover || true; cd /home/ubuntu/services/prover && nohup ./target/release/zkpool-prover > /tmp/prover.log 2>&1 &'

backend:
	@echo "==> backend (port 4001)"
	@$(exec_d) 'pkill -f zkpool-backend || true; cd /home/ubuntu/services/backend && nohup ./target/release/zkpool-backend > /tmp/backend.log 2>&1 &'

web-dev:
	@echo "==> vite dev server (port 5173)"
	@$(exec_d) 'pkill -f vite || true; cd /home/ubuntu/web && nohup pnpm dev > /tmp/web.log 2>&1 &'

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

logs:
	@echo "==> /tmp/merkle.log"; $(exec_c) 'tail -20 /tmp/merkle.log 2>/dev/null || true'
	@echo "==> /tmp/prover.log"; $(exec_c) 'tail -20 /tmp/prover.log 2>/dev/null || true'
	@echo "==> /tmp/backend.log"; $(exec_c) 'tail -20 /tmp/backend.log 2>/dev/null || true'

build:
	@echo "==> backend"; $(exec_c) 'cd /home/ubuntu/services/backend && cargo build --release'
	@echo "==> prover";  $(exec_c) 'cd /home/ubuntu/services/prover  && cargo build --release'
	@echo "==> web";     $(exec_c) 'cd /home/ubuntu/web && pnpm install --frozen-lockfile && pnpm build'

clean:
	@echo "==> removing build artifacts"
	@$(exec_c) 'rm -rf /home/ubuntu/services/backend/target /home/ubuntu/services/prover/target /home/ubuntu/web/dist /home/ubuntu/web/node_modules'

exec-c:
	@$(COMPOSE) exec $(SERVICE) bash -ic '$(CMD)'

validate-spec:
	@$(exec_c) 'cd /home/ubuntu && cargo run --manifest-path scripts/validate-spec/Cargo.toml --release'

sync-circuits-check:
	@$(exec_c) 'cd /home/ubuntu && cargo run --manifest-path scripts/sync-circuits/Cargo.toml --release -- check'

sync-circuits-apply:
	@$(exec_c) 'cd /home/ubuntu && cargo run --manifest-path scripts/sync-circuits/Cargo.toml --release -- apply'
