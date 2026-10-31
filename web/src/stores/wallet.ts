//! Pinia store for wallet state.

import { defineStore } from "pinia";
import { ref, computed, shallowRef } from "vue";
import { address, type Address } from "@solana/kit";
import { detectWallets } from "../wallet/detect";
import type { DetectedWallet, WalletProvider } from "../wallet/types";

/**
 * Extract a base58 public key from a wallet provider after `connect()`.
 *
 * Wallets differ:
 *  - Phantom: `connect()` resolves to `{ publicKey }`, and `provider.publicKey`
 *    is also set.
 *  - Solflare: `connect()` resolves to `true`, and the public key is on
 *    `provider.publicKey` afterwards.
 *  - Backpack: behaves like Phantom (untested).
 *
 * The safest path is: after `connect()` resolves, read `provider.publicKey`.
 */
function readPublicKey(provider: WalletProvider): string {
  const pk = provider.publicKey;
  if (!pk) {
    throw new Error("wallet did not return a public key after connect()");
  }
  if (typeof pk === "string") return pk;
  if (typeof pk.toBase58 === "function") return pk.toBase58();
  throw new Error("wallet returned an unexpected publicKey shape");
}

export const useWalletStore = defineStore("wallet", () => {
  const available = ref<DetectedWallet[]>(detectWallets());
  const provider = shallowRef<WalletProvider | null>(null);
  const walletName = ref<string | null>(null);
  const addr = shallowRef<Address | null>(null);
  const connecting = ref(false);
  const error = ref<string | null>(null);

  const connected = computed(() => addr.value !== null);

  function refreshAvailable() {
    available.value = detectWallets();
  }

  async function connect(wallet: DetectedWallet) {
    error.value = null;
    connecting.value = true;
    try {
      await wallet.provider.connect();
      const pk = readPublicKey(wallet.provider);
      provider.value = wallet.provider;
      walletName.value = wallet.name;
      addr.value = address(pk);
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      connecting.value = false;
    }
  }

  async function disconnect() {
    error.value = null;
    try {
      await provider.value?.disconnect();
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      provider.value = null;
      walletName.value = null;
      addr.value = null;
    }
  }

  /** Auto-connect silently if user has already trusted this dapp. */
  async function tryEagerConnect() {
    refreshAvailable();
    for (const w of available.value) {
      try {
        await w.provider.connect({ onlyIfTrusted: true });
        const pk = readPublicKey(w.provider);
        provider.value = w.provider;
        walletName.value = w.name;
        addr.value = address(pk);
        return;
      } catch {
        // Not trusted, or not available. Continue.
      }
    }
  }

  return {
    available,
    provider,
    walletName,
    addr,
    connecting,
    error,
    connected,
    refreshAvailable,
    connect,
    disconnect,
    tryEagerConnect,
  };
});
