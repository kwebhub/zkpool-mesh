//! Deposit composable: generate note(s) → preview root → build + send tx.
//!
//! Stage 15: two flows.
//!   - `deposit(amount)`      — legacy single-commitment `deposit` (v0.1.0).
//!   - `depositSplit(amounts)` — new 3-way `deposit_split`.
//!
//! Both use the wallet's `signAndSendTransaction`:
//! 1. Build kit `Instruction` via Codama.
//! 2. Build kit `TransactionMessage`.
//! 3. Convert to wire format (base64).
//! 4. `provider.signAndSendTransaction(base64)` → signature.
//! 5. Poll RPC until confirmed.

import { ref, shallowRef } from "vue";
import {
  createSolanaRpc,
  createTransactionMessage,
  setTransactionMessageFeePayer,
  setTransactionMessageLifetimeUsingBlockhash,
  appendTransactionMessageInstruction,
  compileTransaction,
  getBase64EncodedWireTransaction,
} from "@solana/kit";
import { getDepositInstructionAsync, getDepositSplitInstructionAsync } from "../client";
import { getCommitments, postRootPreview } from "../api/client";
import { generateNote, generateSplitNotes, type DepositNote } from "../deposit/generateNote";
import { useWalletStore } from "../stores/wallet";
import { makeNoopSigner } from "../wallet/kitSigner";
import { SPLIT_COUNT } from "../constants";

const RPC_URL = "https://api.devnet.solana.com";

export interface DepositResult {
  signature: string;
  /** Single note for `deposit`, array for `depositSplit`. */
  notes: DepositNote[];
  /** Final new root after all insertions. */
  newRoot: string;
  /** Only present for split deposits. */
  intermediateRoots?: string[];
}

export function useDeposit() {
  const wallet = useWalletStore();
  const loading = ref(false);
  const error = ref<string | null>(null);
  const result = shallowRef<DepositResult | null>(null);

  /**
   * Legacy: single-commitment deposit.
   *
   * @param amountLamports deposit amount in lamports
   */
  async function deposit(amountLamports: bigint): Promise<DepositResult | null> {
    loading.value = true;
    error.value = null;
    try {
      const { walletAddr, provider } = requireWallet(wallet);

      // 1. Generate note + commitment.
      const note = await generateNote(amountLamports);

      // 2. Fetch current commitments + preview new root.
      const { commitments } = await getCommitments();
      const commitmentHexes = commitments.map((c) => c.commitment);
      commitmentHexes.push(note.commitment);
      const { root: newRoot } = await postRootPreview(commitmentHexes);

      // 3. Build the deposit instruction.
      const ix = await getDepositInstructionAsync({
        depositor: makeNoopSigner(walletAddr),
        commitment: hexToBytes(note.commitment),
        newRoot: hexToBytes(newRoot),
        amount: amountLamports,
      });

      // 4-7. Send.
      const signature = await sendInstruction(walletAddr, provider, ix);
      const out: DepositResult = { signature, notes: [note], newRoot };
      result.value = out;
      return out;
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
      return null;
    } finally {
      loading.value = false;
    }
  }

  /**
   * Split deposit: `SPLIT_COUNT` commitments in one transaction.
   *
   * @param amountsLamports exactly `SPLIT_COUNT` per-note amounts
   */
  async function depositSplit(amountsLamports: bigint[]): Promise<DepositResult | null> {
    loading.value = true;
    error.value = null;
    try {
      if (amountsLamports.length !== SPLIT_COUNT) {
        throw new Error(`expected ${SPLIT_COUNT} amounts, got ${amountsLamports.length}`);
      }
      const { walletAddr, provider } = requireWallet(wallet);

      // 1. Generate SPLIT_COUNT notes (browser-side noir_js).
      const notes = await generateSplitNotes(amountsLamports);

      // 2. Fetch current commitments.
      const { commitments } = await getCommitments();
      const existingHexes = commitments.map((c) => c.commitment);

      // 3. Preview each intermediate root by appending commitments one at a
      //    time. The last preview gives the final root.
      const newRoots: string[] = [];
      const cumulative = [...existingHexes];
      for (const n of notes) {
        cumulative.push(n.commitment);
        const { root } = await postRootPreview(cumulative);
        newRoots.push(root);
      }
      const finalRoot = newRoots[newRoots.length - 1];

      // 4. Compute total_amount (BigInt).
      const total = amountsLamports.reduce((a, b) => a + b, 0n);

      // 5. Build the deposit_split instruction.
      const ix = await getDepositSplitInstructionAsync({
        depositor: makeNoopSigner(walletAddr),
        commitments: notes.map((n) => hexToBytes(n.commitment)),
        newRoots: newRoots.map((r) => hexToBytes(r)),
        amounts: amountsLamports,
        totalAmount: total,
      });

      // 6-7. Send.
      const signature = await sendInstruction(walletAddr, provider, ix);

      const out: DepositResult = {
        signature,
        notes,
        newRoot: finalRoot,
        intermediateRoots: newRoots,
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

  return { loading, error, result, deposit, depositSplit };
}

interface WalletContext {
  walletAddr: NonNullable<ReturnType<typeof useWalletStore>["addr"]>;
  provider: NonNullable<ReturnType<typeof useWalletStore>["provider"]>;
}

function requireWallet(wallet: ReturnType<typeof useWalletStore>): {
  walletAddr: WalletContext["walletAddr"];
  provider: WalletContext["provider"] & {
    signAndSendTransaction: NonNullable<WalletContext["provider"]["signAndSendTransaction"]>;
  };
} {
  const walletAddr = wallet.addr;
  const provider = wallet.provider;
  if (!walletAddr || !provider) {
    throw new Error("wallet not connected");
  }
  if (!provider.signAndSendTransaction) {
    throw new Error("wallet does not support signAndSendTransaction");
  }
  return { walletAddr, provider: provider as never };
}

async function sendInstruction(
  walletAddr: WalletContext["walletAddr"],
  provider: WalletContext["provider"],
  ix: Parameters<typeof appendTransactionMessageInstruction>[0],
): Promise<string> {
  const signAndSend = provider.signAndSendTransaction;
  if (!signAndSend) {
    throw new Error("wallet does not support signAndSendTransaction");
  }

  const rpc = createSolanaRpc(RPC_URL);
  const { value: latestBlockhash } = await rpc.getLatestBlockhash().send();

  const message = appendTransactionMessageInstruction(
    ix,
    setTransactionMessageLifetimeUsingBlockhash(
      latestBlockhash,
      setTransactionMessageFeePayer(walletAddr, createTransactionMessage({ version: 0 })),
    ),
  );

  const compiled = compileTransaction(message);
  const wireBase64 = getBase64EncodedWireTransaction(compiled);

  const { signature } = await signAndSend.call(provider, {
    serialize: () => Uint8Array.from(atob(wireBase64), (c) => c.charCodeAt(0)),
    message: { version: 0 },
  } as never);
  void confirmSignature(rpc, signature);
  return signature;
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
        console.error("deposit failed:", status.err);
        return;
      }
    } catch {
      // Ignore transient errors; keep polling.
    }
  }
}

function hexToBytes(hex: string): Uint8Array {
  if (hex.length % 2 !== 0) throw new Error(`odd hex length: ${hex.length}`);
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) {
    out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}
