//! Withdraw composable: parse note → build witness → get proof → send tx.
//!
//! Pipeline:
//!   1. Parse the saved note (JSON).
//!   2. Assemble the witness (hashes, Merkle proof, Groth16 proof from backend).
//!   3. Build the on-chain `withdraw` instruction via Codama.
//!   4. Sign + send via wallet `signAndSendTransaction`.
//!   5. Poll for confirmation.
//!
//! Stage 15: proof is 388 B (was 324); instruction carries `total_amount`.
//!
//! Phantom compatibility: `provider.signAndSendTransaction` expects an
//! object with `serialize()` and `message.version`, not a base64 string.
//! Same workaround as in `useDeposit.ts`.

import { ref, shallowRef } from "vue";
import {
  createSolanaRpc,
  createTransactionMessage,
  setTransactionMessageFeePayer,
  setTransactionMessageLifetimeUsingBlockhash,
  appendTransactionMessageInstruction,
  compileTransaction,
  getBase64EncodedWireTransaction,
  address,
} from "@solana/kit";
import { getWithdrawInstructionAsync } from "../client";
import { useWalletStore } from "../stores/wallet";
import { makeNoopSigner } from "../wallet/kitSigner";
import { parseNote } from "../withdraw/parseNote";
import { buildWitness, type WithdrawWitness } from "../withdraw/buildWitness";
import { hexToBytes } from "../withdraw/reduce";

const RPC_URL = "https://api.devnet.solana.com";

/** Groth16 proof length after Stage 15.4 (was 324). */
const PROOF_LEN = 388;

export interface WithdrawResult {
  signature: string;
  recipient: string;
  amount: string;
  witness: WithdrawWitness;
}

export function useWithdraw() {
  const wallet = useWalletStore();
  const loading = ref(false);
  const error = ref<string | null>(null);
  const result = shallowRef<WithdrawResult | null>(null);

  async function withdraw(
    noteJson: string,
    recipientBase58: string,
  ): Promise<WithdrawResult | null> {
    loading.value = true;
    error.value = null;
    try {
      const walletAddr = wallet.addr;
      const provider = wallet.provider;
      if (!walletAddr || !provider) {
        throw new Error("wallet not connected");
      }
      if (!provider.signAndSendTransaction) {
        throw new Error("wallet does not support signAndSendTransaction");
      }

      // 1. Parse note.
      const note = parseNote(noteJson);

      // 2. Assemble witness + get Groth16 proof from backend.
      const witness = await buildWitness(note, recipientBase58);

      // 3. Decode the base64 Groth16 proof → 388 bytes.
      const proofBytes = base64ToBytes(witness.proofBase64);
      if (proofBytes.length !== PROOF_LEN) {
        throw new Error(`unexpected proof length: ${proofBytes.length}, expected ${PROOF_LEN}`);
      }

      // 4. Build the on-chain withdraw instruction.
      const recipientAddr = address(recipientBase58);
      const ix = await getWithdrawInstructionAsync({
        payer: makeNoopSigner(walletAddr),
        to: recipientAddr,
        proof: proofBytes,
        nullifierHash: hexToBytes(witness.nullifierHash),
        root: hexToBytes(witness.root),
        recipient: recipientAddr,
        amount: BigInt(witness.amount),
        recipientBinding: hexToBytes(witness.recipientBinding),
        totalAmount: BigInt(witness.totalAmount),
      });

      // 5. Fetch blockhash and build the transaction message.
      const rpc = createSolanaRpc(RPC_URL);
      const { value: latestBlockhash } = await rpc.getLatestBlockhash().send();

      const message = appendTransactionMessageInstruction(
        ix,
        setTransactionMessageLifetimeUsingBlockhash(
          latestBlockhash,
          setTransactionMessageFeePayer(walletAddr, createTransactionMessage({ version: 0 })),
        ),
      );

      // 6. Compile to wire format (base64).
      const compiled = compileTransaction(message);
      const wireBase64 = getBase64EncodedWireTransaction(compiled);

      // 7. Sign + send via wallet.
      //    Phantom expects an object exposing `serialize()` and
      //    `message.version`, not a base64 string. Same workaround as
      //    `useDeposit.ts`.
      const signAndSend = provider.signAndSendTransaction;
      const { signature } = await signAndSend.call(provider, {
        serialize: () => Uint8Array.from(atob(wireBase64), (c) => c.charCodeAt(0)),
        message: { version: 0 },
      } as never);

      // 8. Poll for confirmation (best-effort).
      void confirmSignature(rpc, signature);

      const out: WithdrawResult = {
        signature,
        recipient: recipientBase58,
        amount: witness.amount,
        witness,
      };
      result.value = out;
      return out;
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
      return null;
    } finally {
      loading.value = false;
    }
  }

  return { loading, error, result, withdraw };
}

async function confirmSignature(
  rpc: ReturnType<typeof createSolanaRpc>,
  signature: string,
): Promise<void> {
  for (let i = 0; i < 30; i++) {
    await new Promise((r) => setTimeout(r, 1000));
    try {
      const { value } = await rpc.getSignatureStatuses([signature as never]).send();
      const status = value[0];
      if (
        status?.confirmationStatus === "confirmed" ||
        status?.confirmationStatus === "finalized"
      ) {
        return;
      }
      if (status?.err) {
        console.error("withdraw failed:", status.err);
        return;
      }
    } catch {
      // Ignore transient errors; keep polling.
    }
  }
}

function base64ToBytes(b64: string): Uint8Array<ArrayBuffer> {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) {
    out[i] = bin.charCodeAt(i);
  }
  return out;
}
