env-check:
	@test -f .env || { echo "==> [infra] .env not found, copying from .env.example"; cp .env.example .env; }

load-images:
	@echo "==> [infra] loading cached images from cache/images/"
	@for f in cache/images/*.tar; do \
	  [ -f "$$f" ] || continue; \
	  docker load -i "$$f" >/dev/null; \
	done

pin-versions:
	@./scripts/pin-versions.sh

build-images: env-check load-images
	@echo "==> [infra] building $(BASE_IMAGE)"
	@docker build \
	  --build-arg SOLANA_PLATFORM_TOOLS_VERSION=$$(grep -E '^SOLANA_PLATFORM_TOOLS_VERSION=' .env | cut -d= -f2) \
	  -f infra/docker/Dockerfile.base -t $(BASE_IMAGE) .
	@echo "==> [infra] building $(NODE_BASE_IMAGE)"
	@docker build -f infra/docker/Dockerfile.node-base -t $(NODE_BASE_IMAGE) .
	@echo "==> [infra] building $(TEST_IMAGE)"
	@docker build -f infra/docker/Dockerfile.test -t $(TEST_IMAGE) .
	@echo "==> [infra] building $(FRONTEND_IMAGE)"
	@docker build -f infra/docker/Dockerfile.frontend -t $(FRONTEND_IMAGE) .
	@$(MAKE) pin-versions

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
