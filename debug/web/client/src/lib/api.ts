/**
 * API Client for the debug dashboard
 */

const API_BASE = "/api";

export interface PaginatedResponse<T> {
  items: T[];
  total: number;
  offset: number;
  limit: number;
}

export interface Stats {
  accounts: number;
  transactions: number;
  batches: number;
  blocks: number;
  nullifiers: number;
  commitments: number;
  encrypted_notes: number;
  withdrawals: number;
  deposits: number;
  latest_state_root: string;
  latest_batch_id: number;
}

export interface HealthStatus {
  dbReader: boolean;
  sequencer: boolean;
  solanaRpc: boolean;
  sequencerUrl: string;
  solanaRpcUrl: string;
}

export interface Account {
  id: string;
  balance: number;
  nonce: number;
}

export interface Transaction {
  tx_hash: string;
  tx_type: "deposit" | "transfer" | "shielded" | "withdrawal";
  batch_id?: number;
  status: "pending" | "included" | "executed" | "settled" | "failed";
  received_at: number;
  executed_at?: number;
  amount?: number;
  from?: string;
  to?: string;
}

export interface Batch {
  batch_id: number;
  tx_count: number;
  state_root: string;
  shielded_root: string;
  l1_tx_sig?: string;
  status: "building" | "proving" | "pending_settlement" | "settled" | "failed";
  created_at: number;
  settled_at?: number;
}

export interface Block {
  batch_id: number;
  prev_root: string;
  new_root: string;
  tx_count: number;
  open_at: number;
  flags: number;
}

export interface Nullifier {
  nullifier: string;
}

export interface Commitment {
  position: number;
  commitment: string;
}

export interface EncryptedNote {
  commitment: string;
  ciphertext_len: number;
  ephemeral_pk: string;
}

export interface TreeMeta {
  next_position: number;
  frontier: Array<{ level: number; hash: string }>;
}

export interface Deposit {
  l1_seq: number;
  slot: number;
}

export interface Withdrawal {
  tx_hash: string;
  data_len: number;
}

export interface IndexerMeta {
  last_processed_slot?: number;
}

async function fetchApi<T>(endpoint: string): Promise<T> {
  const response = await fetch(`${API_BASE}${endpoint}`);
  if (!response.ok) {
    throw new Error(`API error: ${response.status}`);
  }
  return response.json();
}

export const api = {
  getHealth: () => fetchApi<HealthStatus>("/health"),
  getStats: () => fetchApi<Stats>("/stats"),

  // Accounts
  getAccounts: (offset = 0, limit = 50) =>
    fetchApi<PaginatedResponse<Account>>(
      `/accounts?offset=${offset}&limit=${limit}`
    ),
  getAccount: (id: string) => fetchApi<Account>(`/accounts/${id}`),

  // Transactions
  getTransactions: (
    offset = 0,
    limit = 50,
    filters?: {
      batch_id?: number;
      tx_type?: string;
      status?: string;
    }
  ) => {
    const params = new URLSearchParams({
      offset: String(offset),
      limit: String(limit),
    });
    if (filters?.batch_id) params.set("batch_id", String(filters.batch_id));
    if (filters?.tx_type) params.set("tx_type", filters.tx_type);
    if (filters?.status) params.set("status", filters.status);
    return fetchApi<PaginatedResponse<Transaction>>(
      `/transactions?${params.toString()}`
    );
  },
  getTransaction: (hash: string) => fetchApi<Transaction>(`/transactions/${hash}`),

  // Batches
  getBatches: (offset = 0, limit = 50) =>
    fetchApi<PaginatedResponse<Batch>>(`/batches?offset=${offset}&limit=${limit}`),
  getBatch: (id: number) => fetchApi<Batch>(`/batches/${id}`),

  // Blocks
  getBlocks: (offset = 0, limit = 50) =>
    fetchApi<PaginatedResponse<Block>>(`/blocks?offset=${offset}&limit=${limit}`),

  // Shielded
  getNullifiers: (offset = 0, limit = 50) =>
    fetchApi<PaginatedResponse<Nullifier>>(
      `/shielded/nullifiers?offset=${offset}&limit=${limit}`
    ),
  getCommitments: (offset = 0, limit = 50) =>
    fetchApi<PaginatedResponse<Commitment>>(
      `/shielded/commitments?offset=${offset}&limit=${limit}`
    ),
  getEncryptedNotes: (offset = 0, limit = 50) =>
    fetchApi<PaginatedResponse<EncryptedNote>>(
      `/shielded/notes?offset=${offset}&limit=${limit}`
    ),
  getTreeMeta: () => fetchApi<TreeMeta>("/shielded/tree"),

  // Bridge
  getDeposits: (offset = 0, limit = 50) =>
    fetchApi<PaginatedResponse<Deposit>>(
      `/bridge/deposits?offset=${offset}&limit=${limit}`
    ),
  getWithdrawals: (offset = 0, limit = 50) =>
    fetchApi<PaginatedResponse<Withdrawal>>(
      `/bridge/withdrawals?offset=${offset}&limit=${limit}`
    ),

  // Indexer
  getIndexerMeta: () => fetchApi<IndexerMeta>("/indexer"),
};
