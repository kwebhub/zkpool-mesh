//! Typed HTTP client for the zkpool backend.
//!
//! All endpoints are mounted under `/api/*`, and Vite dev-server proxies
//! them to `http://localhost:4001`. In production, whatever reverse proxy
//! fronts the app must do the same.

import type {
  ApiError,
  CommitmentsResponse,
  HealthResponse,
  ProofResponse,
  RootResponse,
  WithdrawRequest,
  WithdrawResponse,
} from "./types";

const BASE = "";

/** Error thrown by `request()` on non-2xx or malformed JSON. */
export class ApiClientError extends Error {
  constructor(
    message: string,
    public readonly status: number,
    public readonly body: unknown,
  ) {
    super(message);
    this.name = "ApiClientError";
  }
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const resp = await fetch(`${BASE}${path}`, {
    headers: { "Content-Type": "application/json" },
    ...init,
  });

  const text = await resp.text();
  let body: unknown = null;
  if (text.length > 0) {
    try {
      body = JSON.parse(text);
    } catch {
      throw new ApiClientError(`invalid JSON from ${path}`, resp.status, text);
    }
  }

  if (!resp.ok) {
    const message =
      body && typeof body === "object" && "error" in body
        ? String((body as ApiError).error)
        : `HTTP ${resp.status}`;
    throw new ApiClientError(message, resp.status, body);
  }

  return body as T;
}

/** `GET /api/health`. */
export async function getHealth(): Promise<HealthResponse> {
  return request<HealthResponse>("/api/health");
}

/** `GET /api/commitments[?pool_address=...]`. */
export async function getCommitments(poolAddress?: string): Promise<CommitmentsResponse> {
  const q = poolAddress ? `?pool_address=${encodeURIComponent(poolAddress)}` : "";
  return request<CommitmentsResponse>(`/api/commitments${q}`);
}

/** `GET /api/root[?pool_address=...]`. */
export async function getRoot(poolAddress?: string): Promise<RootResponse> {
  const q = poolAddress ? `?pool_address=${encodeURIComponent(poolAddress)}` : "";
  return request<RootResponse>(`/api/root${q}`);
}

/** `GET /api/proof?leaf_index=N[&pool_address=...]`. */
export async function getProof(leafIndex: number, poolAddress?: string): Promise<ProofResponse> {
  const params = new URLSearchParams({ leaf_index: String(leafIndex) });
  if (poolAddress) params.set("pool_address", poolAddress);
  return request<ProofResponse>(`/api/proof?${params}`);
}

/** `POST /api/withdraw`. */
export async function postWithdraw(req: WithdrawRequest): Promise<WithdrawResponse> {
  return request<WithdrawResponse>("/api/withdraw", {
    method: "POST",
    body: JSON.stringify(req),
  });
}

/** `POST /api/root-preview`. */
export async function postRootPreview(commitments: string[]): Promise<{ root: string }> {
  return request<{ root: string }>("/api/root-preview", {
    method: "POST",
    body: JSON.stringify({ commitments }),
  });
}
