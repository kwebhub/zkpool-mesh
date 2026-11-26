airdrop:
	@docker network inspect $(COMPOSE_NETWORK) >/dev/null 2>&1 || { echo "ERROR: $(COMPOSE_NETWORK) not running — run make up first"; exit 1; }
	@echo "==> [chain 0/5] airdrop to wallet (only if balance < 2 SOL)"
	@$(mesh_run) 'solana config set --url $(LOCAL_RPC) >/dev/null; \
	  BAL=$$(solana balance --url $(LOCAL_RPC) 2>/dev/null | awk "{print \$$1}"); \
	  BAL=$${BAL:-0}; \
	  if awk "BEGIN{exit !($$BAL < 2)}"; then \
	    echo "   balance $$BAL SOL < 2, requesting airdrop"; \
	    solana airdrop 100 --url $(LOCAL_RPC); \
	  else \
	    echo "   balance $$BAL SOL >= 2, skipping airdrop"; \
	  fi'

chain: airdrop
	@echo "==> [chain 1/5] verify wallet + config"
	@$(mesh_run) 'test -f /home/i/.config/solana/id.json || { echo "ERROR: solana/id.json missing"; exit 1; }; solana config set --url $(LOCAL_RPC); solana-keygen pubkey /home/i/.config/solana/id.json'
	@echo "==> [chain 2/5] verifier: nargo compile → sunspot compile → setup → deploy"
	@$(mesh_run) 'set -e; export PATH=/home/i/sunspot/go:$$PATH; \
	  cd /home/i/circuits/withdrawal && nargo compile && \
	  sunspot compile target/withdrawal.json && \
	  sunspot setup target/withdrawal.ccs && \
	  cd target && rm -f withdrawal-keypair.json && sunspot deploy withdrawal.vk && \
	  solana program deploy withdrawal.so --program-id withdrawal-keypair.json --use-rpc --max-sign-attempts 1000 --commitment confirmed --url $(LOCAL_RPC) > /home/i/.secrets/deploy-verifier.log 2>&1; \
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
	@$(mesh_run) 'set -eo pipefail; cd /home/i/onchain && rm -f target/deploy/zk_pool-keypair.json && \
	  PT_VER=$$(grep -E "^SOLANA_PLATFORM_TOOLS_VERSION=" /home/i/.env | cut -d= -f2); \
	  PT_VER=$${PT_VER:-$$(ls -1 /home/i/.cache/solana | head -1)}; \
	  anchor build --no-idl --tools-version $$PT_VER 2>&1 | tail -3; \
	  NEW_ID=$$(solana-keygen pubkey target/deploy/zk_pool-keypair.json); \
	  echo "   new zk_pool PROGRAM_ID=$$NEW_ID"; \
	  sed "s|declare_id!(\"[^\"]*\")|declare_id!(\"$$NEW_ID\")|" /home/i/onchain/programs/zk_pool/src/lib.rs > /tmp/lib.rs.new && cat /tmp/lib.rs.new > /home/i/onchain/programs/zk_pool/src/lib.rs && rm -f /tmp/lib.rs.new; \
	  sed "s|^zk_pool = .*|zk_pool = \"$$NEW_ID\"|" /home/i/onchain/Anchor.toml > /tmp/Anchor.toml.new && cat /tmp/Anchor.toml.new > /home/i/onchain/Anchor.toml && rm -f /tmp/Anchor.toml.new; \
	  sed "s|^PROGRAM_ID=.*|PROGRAM_ID=$$NEW_ID|" /home/i/.env > /tmp/.env.new && cat /tmp/.env.new > /home/i/.env && rm -f /tmp/.env.new; \
	  anchor build --no-idl --tools-version $$PT_VER 2>&1 | tail -3; \
	  anchor idl build -o target/idl/zk_pool.json && \
	  solana program deploy target/deploy/zk_pool.so --program-id target/deploy/zk_pool-keypair.json --use-rpc --max-sign-attempts 1000 --url $(LOCAL_RPC) > /home/i/.secrets/deploy-zkpool.log 2>&1; \
	  grep -A2 "12-word seed phrase" /home/i/.secrets/deploy-zkpool.log | tail -1 | sed "s/^/zkpool_buffer_seed: /" >> /home/i/.secrets/keys.md; \
	  grep -E "Program Id:|Signature:" /home/i/.secrets/deploy-zkpool.log'
	@$(mesh_run) 'set -e; NEW_PROGRAM=$$(solana-keygen pubkey /home/i/onchain/target/deploy/zk_pool-keypair.json); \
	  echo "   new PROGRAM_ID=$$NEW_PROGRAM"; \
	  sed "s|^PROGRAM_ID=.*|PROGRAM_ID=$$NEW_PROGRAM|; s|^POOL_ADDRESS=.*|POOL_ADDRESS=|" /home/i/.env > /tmp/.env.new && cat /tmp/.env.new > /home/i/.env && rm -f /tmp/.env.new; \
	  grep -E "^(PROGRAM_ID|POOL_ADDRESS)=" /home/i/.env'
	@echo "==> [chain 5/5] pool-init"
	@$(mesh_run) 'set -eo pipefail; cd /home/i && cargo build --release --manifest-path scripts/pool-init/Cargo.toml 2>&1 | tail -3 && \
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
