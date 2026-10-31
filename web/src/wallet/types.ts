//! Wallet-related TypeScript types.

import type { Address } from "@solana/kit";

/** Wallet provider injected into `window` by Phantom/Solflare/etc. */
export interface WalletProvider {
  /**
   * Public key (base58 string or object with `toBase58`). Present after
   * `connect()`. Phantom and Solflare use different shapes.
   */
  publicKey: { toBase58(): string } | string | null;
  /** True if the user has already approved this dapp. */
  isConnected: boolean;
  /**
   * Request connection. Phantom resolves to `{ publicKey }`; Solflare
   * resolves to `true` and stores the key on `provider.publicKey`.
   * Callers must read `provider.publicKey` after `connect()` resolves.
   */
  connect(options?: { onlyIfTrusted?: boolean }): Promise<unknown>;
  /** Disconnect. */
  disconnect(): Promise<void>;
  /** Sign one or more transactions (binary). */
  signTransaction?<T>(tx: T): Promise<T>;
  /** Sign and send one or more transactions. */
  signAndSendTransaction?<T>(tx: T): Promise<{ signature: string }>;
  /** Sign arbitrary message bytes. */
  signMessage?(message: Uint8Array, encoding?: string): Promise<{ signature: Uint8Array }>;
  /** Wallet name (for display). */
  name?: string;
  /** Event emitter — minimal. */
  on(event: string, handler: (...args: unknown[]) => void): void;
  off?(event: string, handler: (...args: unknown[]) => void): void;
}

declare global {
  interface Window {
    solana?: WalletProvider;
    phantom?: { solana?: WalletProvider };
    solflare?: WalletProvider;
    backpack?: WalletProvider;
  }
}

/** Detected wallet, ready for use. */
export interface DetectedWallet {
  name: string;
  provider: WalletProvider;
  /** Convert the provider's publicKey to a `@solana/kit` Address. */
  address: Address | null;
}
