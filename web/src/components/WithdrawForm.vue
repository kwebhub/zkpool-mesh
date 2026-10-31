<template lang="pug">
.withdraw-form
  h2 Withdraw SOL

  .field
    label(for="note") Deposit note (JSON)
    textarea#note(
      v-model="noteJson"
      rows="6"
      placeholder='{"nullifier":"...","secret":"...",...}'
      :disabled="withdraw.loading.value"
    )

  .field
    label(for="recipient") Recipient address (base58)
    input#recipient(
      v-model="recipient"
      type="text"
      placeholder="5iM6nzaCqegVG3j4CSf19zmU3tmcs9KP51djaBXnAKGc"
      :disabled="withdraw.loading.value"
    )

  button.primary(
    :disabled="!canWithdraw"
    @click="onWithdraw"
  )
    | {{ withdraw.loading.value ? "Withdrawing…" : "Withdraw" }}

  p.error(v-if="withdraw.error.value") {{ withdraw.error.value }}

  .result(v-if="withdraw.result.value")
    h3 ✅ Withdrawal submitted
    dl
      dt Signature
      dd
        a(
          :href="explorerUrl"
          target="_blank"
          rel="noopener"
        ) {{ shortSig }}
      dt Recipient
      dd.mono {{ withdraw.result.value.recipient }}
      dt Amount (lamports)
      dd.mono {{ withdraw.result.value.amount }}
      dt Root
      dd.mono {{ withdraw.result.value.witness.root }}
      dt Nullifier hash
      dd.mono {{ withdraw.result.value.witness.nullifierHash }}
</template>

<script setup lang="ts">
import { computed, ref } from "vue";
import { useWithdraw } from "../composables/useWithdraw";
import { useWalletStore } from "../stores/wallet";

const wallet = useWalletStore();
const withdraw = useWithdraw();

const noteJson = ref("");
const recipient = ref("");

const canWithdraw = computed(() => {
  if (!wallet.connected) return false;
  if (withdraw.loading.value) return false;
  if (noteJson.value.trim().length === 0) return false;
  if (recipient.value.trim().length === 0) return false;
  return true;
});

const explorerUrl = computed(() => {
  const sig = withdraw.result.value?.signature;
  if (!sig) return "#";
  return `https://explorer.solana.com/tx/${sig}?cluster=devnet`;
});

const shortSig = computed(() => {
  const sig = withdraw.result.value?.signature;
  if (!sig) return "";
  return `${sig.slice(0, 8)}…${sig.slice(-8)}`;
});

async function onWithdraw() {
  await withdraw.withdraw(noteJson.value.trim(), recipient.value.trim());
}
</script>

<style lang="scss" scoped>
.withdraw-form {
  max-width: 600px;

  h2 {
    margin-top: 0;
  }

  .field {
    margin-bottom: 1rem;

    label {
      display: block;
      font-weight: 600;
      margin-bottom: 0.25rem;
    }

    input,
    textarea {
      width: 100%;
      padding: 0.5rem;
      border: 1px solid #ccc;
      border-radius: 4px;
      font-size: 0.9rem;
      font-family: ui-monospace, monospace;
      box-sizing: border-box;

      &:disabled {
        opacity: 0.6;
      }
    }

    textarea {
      resize: vertical;
    }
  }

  button.primary {
    padding: 0.6rem 1.2rem;
    border: 1px solid #333;
    background: #333;
    color: #fff;
    border-radius: 4px;
    font-size: 1rem;
    cursor: pointer;

    &:hover:not(:disabled) {
      background: #000;
    }

    &:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }
  }

  .error {
    color: #c00;
    margin-top: 0.75rem;
    padding: 0.5rem;
    background: #fee;
    border-radius: 4px;
  }

  .result {
    margin-top: 1.5rem;
    padding: 1rem;
    background: #f0fff0;
    border-radius: 6px;
    border: 1px solid #a0d0a0;

    h3 {
      margin-top: 0;
      color: #060;
    }

    dl {
      margin: 0.5rem 0;

      dt {
        font-weight: 600;
        font-size: 0.85rem;
        color: #555;
        margin-top: 0.5rem;
      }

      dd {
        margin: 0;
        word-break: break-all;

        &.mono {
          font-family: ui-monospace, monospace;
          font-size: 0.8rem;
          background: #e8f8e8;
          padding: 0.25rem 0.4rem;
          border-radius: 3px;
        }
      }
    }

    a {
      color: #06c;
    }
  }
}
</style>
