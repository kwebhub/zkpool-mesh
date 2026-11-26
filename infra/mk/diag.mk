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
