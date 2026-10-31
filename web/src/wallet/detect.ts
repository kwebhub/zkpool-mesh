//! Detect injected wallet providers in `window`.

import type { DetectedWallet, WalletProvider } from "./types";

interface Candidate {
  name: string;
  provider: WalletProvider | undefined;
}

export function detectWallets(): DetectedWallet[] {
  if (typeof window === "undefined") return [];

  const candidates: Candidate[] = [
    { name: "Phantom", provider: window.phantom?.solana ?? window.solana },
    { name: "Solflare", provider: window.solflare },
    { name: "Backpack", provider: window.backpack },
  ];

  const seen = new Set<WalletProvider>();
  const detected: DetectedWallet[] = [];

  for (const c of candidates) {
    if (!c.provider) continue;
    if (seen.has(c.provider)) continue;
    seen.add(c.provider);
    detected.push({
      name: c.provider.name ?? c.name,
      provider: c.provider,
      address: null,
    });
  }

  return detected;
}
