//! Bridge: `window`-injected wallet provider → `@solana/kit` TransactionSigner.
//!
//! `@solana/kit` expects a `TransactionSigner` with `signTransactions`.
//! Injected wallets (Phantom/Solflare) expose `signTransaction` — different
//! shape. This adapter builds a minimal signer.
//!
//! For v1, we only need the signer object to satisfy the type. Actual
//! signing happens out-of-band via `provider.signAndSendTransaction` on the
//! serialized wire transaction. The signer returned here is a "noop" — its
//! `signTransactions` throws if called.

import type { Address, TransactionSigner } from "@solana/kit";

export function makeNoopSigner(addr: Address): TransactionSigner {
  return {
    address: addr,
    async signTransactions() {
      throw new Error("noop signer: signing is delegated to the wallet via signAndSendTransaction");
    },
  } as unknown as TransactionSigner;
}
