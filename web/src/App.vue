<template lang="pug">
.container
  header.header
    h1 zkpool-solana
    .wallet
      .connected(v-if="wallet.connected")
        span.name {{ wallet.walletName }}
        code.addr {{ shortAddr }}
        button(@click="wallet.disconnect()") Disconnect
      .disconnected(v-else)
        button(
          v-for="w in wallet.available"
          :key="w.name"
          :disabled="wallet.connecting"
          @click="wallet.connect(w)"
        ) Connect {{ w.name }}
        p.muted(v-if="wallet.available.length === 0") No Solana wallet detected. Install Phantom or Solflare.

  main
    p.connect-hint(v-if="!wallet.connected") Connect a wallet to continue.
    template(v-else)
      nav.tabs
        button(
          :class="{ active: tab === 'deposit' }"
          @click="tab = 'deposit'"
        ) Deposit
        button(
          :class="{ active: tab === 'withdraw' }"
          @click="tab = 'withdraw'"
        ) Withdraw
      DepositForm(v-if="tab === 'deposit'")
      WithdrawForm(v-else)

  p.error(v-if="wallet.error") {{ wallet.error }}
</template>

<script setup lang="ts">
import { computed, ref } from "vue";
import { useWalletStore } from "./stores/wallet";
import DepositForm from "./components/DepositForm.vue";
import WithdrawForm from "./components/WithdrawForm.vue";

const wallet = useWalletStore();
const tab = ref<"deposit" | "withdraw">("deposit");

const shortAddr = computed(() => {
  const a = wallet.addr;
  if (!a) return "";
  const s = a.toString();
  return `${s.slice(0, 4)}…${s.slice(-4)}`;
});
</script>

<style lang="scss" scoped>
body {
  font-family: system-ui, sans-serif;
  margin: 2rem;
}

.container {
  max-width: 800px;
  margin: 0 auto;
}

.header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 1rem;
  padding-bottom: 1rem;
  border-bottom: 1px solid #ddd;
}

.wallet {
  display: flex;
  gap: 0.5rem;
  align-items: center;

  .name {
    font-weight: 600;
    margin-right: 0.25rem;
  }

  .addr {
    font-family: ui-monospace, monospace;
    font-size: 0.85rem;
    background: #f4f4f4;
    padding: 0.15rem 0.4rem;
    border-radius: 4px;
    margin-right: 0.5rem;
  }

  button {
    padding: 0.4rem 0.8rem;
    border: 1px solid #333;
    background: #fff;
    border-radius: 4px;
    cursor: pointer;

    &:hover:not(:disabled) {
      background: #333;
      color: #fff;
    }

    &:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }
  }

  .muted {
    color: #888;
    font-size: 0.9rem;
  }
}

main {
  margin-top: 2rem;
}

.connect-hint {
  color: #666;
}

.tabs {
  display: flex;
  gap: 0;
  border-bottom: 1px solid #ddd;
  margin-bottom: 1.5rem;

  button {
    padding: 0.6rem 1.2rem;
    border: none;
    background: none;
    cursor: pointer;
    font-size: 1rem;
    color: #666;
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;

    &:hover {
      color: #333;
    }

    &.active {
      color: #000;
      font-weight: 600;
      border-bottom-color: #333;
    }
  }
}

.error {
  color: #c00;
  margin-top: 1rem;
}
</style>
