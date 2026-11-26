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
