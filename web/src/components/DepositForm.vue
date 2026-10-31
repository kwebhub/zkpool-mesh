<template lang="pug">
.deposit-form
  h2 Deposit SOL

  .mode
    label
      input(
        type="radio"
        value="single"
        v-model="mode"
        :disabled="deposit.loading.value"
      )
      |  Single note
    label
      input(
        type="radio"
        value="split"
        v-model="mode"
        :disabled="deposit.loading.value"
      )
      |  Split into {{ splitCount }} notes

  template(v-if="mode === 'single'")
    .field
      label(for="amount") Amount (SOL)
      input#amount(
        v-model="amountSol"
        type="text"
        placeholder="0.01"
        :disabled="deposit.loading.value"
      )
      p.hint Minimum: {{ minSol }} SOL

  template(v-else)
    .field(
      v-for="(amt, i) in splitAmountsSol"
      :key="i"
    )
      label(:for="'split-' + i") Split {{ i + 1 }} (SOL)
      input(
        :id="'split-' + i"
        v-model="splitAmountsSol[i]"
        type="text"
        :placeholder="splitPlaceholders[i]"
        :disabled="deposit.loading.value"
      )
    p.hint
      | Total: {{ totalSol }} SOL. Each split must be ≥ {{ minSol }} SOL.

  button.primary(
    :disabled="!canDeposit"
    @click="onDeposit"
  )
    | {{ deposit.loading.value ? "Depositing…" : "Deposit" }}

  p.error(v-if="deposit.error.value") {{ deposit.error.value }}

  .result(v-if="deposit.result.value")
    h3 ✅ Deposit submitted
    dl
      dt Signature
      dd
        a(
          :href="explorerUrl"
          target="_blank"
          rel="noopener"
        ) {{ shortSig }}
      dt Final root
      dd.mono {{ deposit.result.value.newRoot }}

    .note
      h4 ⚠️ Save {{ notePlural }}
      p
        | The following secrets are required to withdraw later.
        strong  Loss of any note means loss of those funds.

      .note-entry(
        v-for="(note, i) in deposit.result.value.notes"
        :key="i"
      )
        h5 Note {{ i + 1 }} of {{ deposit.result.value.notes.length }}
        dl
          dt nullifier
          dd.mono {{ note.nullifier }}
          dt secret
          dd.mono {{ note.secret }}
          dt note_secret
          dd.mono {{ note.noteSecret }}
          dt amount (hex)
          dd.mono {{ note.amount }}
          dt note_index
          dd.mono {{ note.noteIndex }}

      button.secondary(@click="copyNotes") Copy all notes as JSON
      p.copied(v-if="copied") Copied!
</template>

<script setup lang="ts">
import { computed, ref } from "vue";
import { useDeposit } from "../composables/useDeposit";
import { useWalletStore } from "../stores/wallet";
import { LAMPORTS_PER_SOL, MIN_DEPOSIT_AMOUNT, SPLIT_COUNT } from "../constants";

const wallet = useWalletStore();
const deposit = useDeposit();

type Mode = "single" | "split";
const mode = ref<Mode>("single");

const amountSol = ref("0.01");
const splitAmountsSol = ref<string[]>(["0.5", "0.3", "0.2"]);
const copied = ref(false);

const splitCount = SPLIT_COUNT;
const splitPlaceholders = ["0.5", "0.3", "0.2"];

const minSol = computed(() => (Number(MIN_DEPOSIT_AMOUNT) / Number(LAMPORTS_PER_SOL)).toString());

const totalSol = computed(() => {
  const total = splitAmountsSol.value.reduce((acc, s) => {
    const n = Number(s);
    return acc + (Number.isFinite(n) && n > 0 ? n : 0);
  }, 0);
  return total.toString();
});

const notePlural = computed(() => (mode.value === "split" ? `${splitCount} notes` : "this note"));

const canDeposit = computed(() => {
  if (!wallet.connected) return false;
  if (deposit.loading.value) return false;

  if (mode.value === "single") {
    const n = Number(amountSol.value);
    if (!Number.isFinite(n) || n <= 0) return false;
    const lamports = BigInt(Math.floor(n * Number(LAMPORTS_PER_SOL)));
    return lamports >= MIN_DEPOSIT_AMOUNT;
  }

  // Split: each amount must be >= MIN_DEPOSIT_AMOUNT.
  for (const s of splitAmountsSol.value) {
    const n = Number(s);
    if (!Number.isFinite(n) || n <= 0) return false;
    const lamports = BigInt(Math.floor(n * Number(LAMPORTS_PER_SOL)));
    if (lamports < MIN_DEPOSIT_AMOUNT) return false;
  }
  return true;
});

const explorerUrl = computed(() => {
  const sig = deposit.result.value?.signature;
  if (!sig) return "#";
  return `https://explorer.solana.com/tx/${sig}?cluster=devnet`;
});

const shortSig = computed(() => {
  const sig = deposit.result.value?.signature;
  if (!sig) return "";
  return `${sig.slice(0, 8)}…${sig.slice(-8)}`;
});

async function onDeposit() {
  copied.value = false;

  if (mode.value === "single") {
    const lamports = BigInt(Math.floor(Number(amountSol.value) * Number(LAMPORTS_PER_SOL)));
    await deposit.deposit(lamports);
    return;
  }

  const amounts = splitAmountsSol.value.map((s) =>
    BigInt(Math.floor(Number(s) * Number(LAMPORTS_PER_SOL))),
  );
  await deposit.depositSplit(amounts);
}

async function copyNotes() {
  const r = deposit.result.value;
  if (!r) return;

  const notesJson = r.notes.map((n) => ({
    nullifier: n.nullifier,
    secret: n.secret,
    note_secret: n.noteSecret,
    amount: n.amount,
    commitment: n.commitment,
    nullifier_hash: n.nullifierHash,
    splits: n.splits,
    note_index: n.noteIndex,
    tx_signature: r.signature,
    pool_pda: "B89Yhoecj9AKJEDXT49DfjbTJoqjovmcKgYmqdzQwBYf",
  }));

  const payload = notesJson.length === 1 ? notesJson[0] : notesJson;

  try {
    await navigator.clipboard.writeText(JSON.stringify(payload, null, 2));
    copied.value = true;
    setTimeout(() => (copied.value = false), 2000);
  } catch {
    // Clipboard not available; ignore.
  }
}
</script>

<style lang="scss" scoped>
.deposit-form {
  max-width: 600px;

  h2 {
    margin-top: 0;
  }

  .mode {
    display: flex;
    gap: 1.5rem;
    margin-bottom: 1rem;

    label {
      display: flex;
      gap: 0.4rem;
      align-items: center;
      cursor: pointer;
      font-weight: 500;
    }
  }

  .field {
    margin-bottom: 1rem;

    label {
      display: block;
      font-weight: 600;
      margin-bottom: 0.25rem;
    }

    input {
      width: 100%;
      padding: 0.5rem;
      border: 1px solid #ccc;
      border-radius: 4px;
      font-size: 1rem;
      font-family: ui-monospace, monospace;

      &:disabled {
        opacity: 0.6;
      }
    }

    .hint {
      color: #666;
      font-size: 0.85rem;
      margin: 0.25rem 0 0;
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

  button.secondary {
    margin-top: 0.5rem;
    padding: 0.4rem 0.8rem;
    border: 1px solid #666;
    background: #fff;
    border-radius: 4px;
    cursor: pointer;
  }

  .error {
    color: #c00;
    margin-top: 0.75rem;
  }

  .result {
    margin-top: 1.5rem;
    padding: 1rem;
    background: #f8f8f8;
    border-radius: 6px;
    border: 1px solid #ddd;

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
          background: #eee;
          padding: 0.25rem 0.4rem;
          border-radius: 3px;
        }
      }
    }

    a {
      color: #06c;
    }

    .note {
      margin-top: 1.5rem;
      padding: 1rem;
      background: #fff8e0;
      border: 1px solid #e0c060;
      border-radius: 4px;

      h4 {
        margin-top: 0;
      }

      strong {
        color: #c00;
      }

      .note-entry {
        margin-top: 1rem;
        padding: 0.75rem;
        background: #fffdf5;
        border: 1px solid #efe0b0;
        border-radius: 4px;

        h5 {
          margin: 0 0 0.5rem 0;
          color: #806000;
        }
      }
    }

    .copied {
      color: #060;
      font-weight: 600;
      margin: 0.25rem 0 0;
    }
  }
}
</style>
