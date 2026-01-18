/**
 * Zelana API Client
 * 
 * Low-level HTTP client for the Zelana L2 sequencer API.
 * Handles request/response serialization and error handling.
 */

import { bytesToHex, hexToBytes } from './utils';
import { ZelanaError } from './types';
import type {
  Bytes32,
  AccountState,
  TransferRequest,
  TransferResponse,
  WithdrawRequest,
  WithdrawResponse,
  WithdrawalStatus,
  ShieldedRequest,
  ShieldedResponse,
  StateRoots,
  BatchStatusInfo,
  HealthInfo,
  GlobalStats,
  FastWithdrawQuote,
  CommitteeInfo,
  MerklePath,
  ScannedNote,
  TxSummary,
  BatchSummary,
  PaginationParams,
  ApiError,
  DevDepositResponse,
  DevSealResponse,
} from './types';

/**
 * API client configuration
 */
export interface ApiClientConfig {
  /** Base URL of the sequencer API (e.g., "http://localhost:3000") */
  baseUrl: string;
  /** Request timeout in milliseconds (default: 30000) */
  timeout?: number;
  /** Custom fetch implementation (for Node.js compatibility) */
  fetch?: typeof fetch;
}

/**
 * Low-level API client for Zelana L2 sequencer
 */
export class ApiClient {
  private readonly baseUrl: string;
  private readonly timeout: number;
  private readonly fetch: typeof fetch;

  constructor(config: ApiClientConfig) {
    this.baseUrl = config.baseUrl.replace(/\/$/, ''); // Remove trailing slash
    this.timeout = config.timeout ?? 30000;
    this.fetch = config.fetch ?? globalThis.fetch;
  }

  // ==========================================================================
  // Private Helpers
  // ==========================================================================

  private async request<T>(
    method: 'GET' | 'POST',
    path: string,
    body?: unknown
  ): Promise<T> {
    const url = `${this.baseUrl}${path}`;
    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), this.timeout);

    try {
      const response = await this.fetch(url, {
        method,
        headers: {
          'Content-Type': 'application/json',
        },
        body: body ? JSON.stringify(body, bigIntReplacer) : undefined,
        signal: controller.signal,
      });

      clearTimeout(timeoutId);

      const text = await response.text();
      let data: unknown;
      
      try {
        data = JSON.parse(text, bigIntReviver);
      } catch {
        throw new ZelanaError(`Invalid JSON response: ${text}`, 'PARSE_ERROR');
      }

      if (!response.ok) {
        const err = data as ApiError;
        throw new ZelanaError(
          err.error || `HTTP ${response.status}`,
          err.code || 'HTTP_ERROR'
        );
      }

      return data as T;
    } catch (error) {
      clearTimeout(timeoutId);
      if (error instanceof ZelanaError) {
        throw error;
      }
      if (error instanceof Error && error.name === 'AbortError') {
        throw new ZelanaError('Request timeout', 'TIMEOUT');
      }
      throw new ZelanaError(
        error instanceof Error ? error.message : 'Unknown error',
        'NETWORK_ERROR',
        error
      );
    }
  }

  private async get<T>(path: string): Promise<T> {
    return this.request<T>('GET', path);
  }

  private async post<T>(path: string, body: unknown): Promise<T> {
    return this.request<T>('POST', path, body);
  }

  // ==========================================================================
  // Health & Status
  // ==========================================================================

  /**
   * Check if the sequencer is healthy
   */
  async health(): Promise<HealthInfo> {
    const resp = await this.get<{
      healthy: boolean;
      version: string;
      uptime_secs: number;
    }>('/health');
    return {
      healthy: resp.healthy,
      version: resp.version,
      uptimeSecs: BigInt(resp.uptime_secs),
    };
  }

  /**
   * Get current state roots
   */
  async getStateRoots(): Promise<StateRoots> {
    const resp = await this.get<{
      batch_id: number;
      state_root: string;
      shielded_root: string;
      commitment_count: number;
    }>('/status/roots');
    return {
      batchId: BigInt(resp.batch_id),
      stateRoot: resp.state_root,
      shieldedRoot: resp.shielded_root,
      commitmentCount: BigInt(resp.commitment_count),
    };
  }

  /**
   * Get current batch status
   */
  async getBatchStatus(): Promise<BatchStatusInfo> {
    const resp = await this.get<{
      current_batch_id: number;
      current_batch_txs: number;
      proving_count: number;
      pending_settlement: number;
    }>('/status/batch');
    return {
      currentBatchId: BigInt(resp.current_batch_id),
      currentBatchTxs: resp.current_batch_txs,
      provingCount: resp.proving_count,
      pendingSettlement: resp.pending_settlement,
    };
  }

  /**
   * Get global statistics
   */
  async getStats(): Promise<GlobalStats> {
    const resp = await this.get<{
      total_batches: number;
      total_transactions: number;
      total_deposited: number;
      total_withdrawn: number;
      current_batch_id: number;
      active_accounts: number;
      shielded_commitments: number;
      uptime_secs: number;
    }>('/status/stats');
    return {
      totalBatches: BigInt(resp.total_batches),
      totalTransactions: BigInt(resp.total_transactions),
      totalDeposited: BigInt(resp.total_deposited),
      totalWithdrawn: BigInt(resp.total_withdrawn),
      currentBatchId: BigInt(resp.current_batch_id),
      activeAccounts: BigInt(resp.active_accounts),
      shieldedCommitments: BigInt(resp.shielded_commitments),
      uptimeSecs: BigInt(resp.uptime_secs),
    };
  }

  // ==========================================================================
  // Account Operations
  // ==========================================================================

  /**
   * Get account state by ID (hex-encoded public key)
   */
  async getAccount(accountId: string): Promise<AccountState> {
    const resp = await this.post<{
      account_id: string;
      balance: number;
      nonce: number;
    }>('/account', { account_id: accountId });
    return {
      accountId: resp.account_id,
      balance: BigInt(resp.balance),
      nonce: BigInt(resp.nonce),
    };
  }

  /**
   * Get account state by public key bytes
   */
  async getAccountByPubkey(pubkey: Bytes32): Promise<AccountState> {
    return this.getAccount(bytesToHex(pubkey));
  }

  // ==========================================================================
  // Transfer Operations
  // ==========================================================================

  /**
   * Submit a signed transfer transaction
   */
  async submitTransfer(request: TransferRequest): Promise<TransferResponse> {
    const resp = await this.post<{
      tx_hash: string;
      accepted: boolean;
      message: string;
    }>('/transfer', {
      from: Array.from(request.from),
      to: Array.from(request.to),
      amount: Number(request.amount),
      nonce: Number(request.nonce),
      chain_id: Number(request.chainId),
      signature: Array.from(request.signature),
      signer_pubkey: Array.from(request.signerPubkey),
    });
    return {
      txHash: resp.tx_hash,
      accepted: resp.accepted,
      message: resp.message,
    };
  }

  // ==========================================================================
  // Withdrawal Operations
  // ==========================================================================

  /**
   * Submit a signed withdrawal request
   */
  async submitWithdrawal(request: WithdrawRequest): Promise<WithdrawResponse> {
    const resp = await this.post<{
      tx_hash: string;
      accepted: boolean;
      estimated_completion?: string;
      message: string;
    }>('/withdraw', {
      from: Array.from(request.from),
      to_l1_address: Array.from(request.toL1Address),
      amount: Number(request.amount),
      nonce: Number(request.nonce),
      signature: Array.from(request.signature),
      signer_pubkey: Array.from(request.signerPubkey),
    });
    return {
      txHash: resp.tx_hash,
      accepted: resp.accepted,
      estimatedCompletion: resp.estimated_completion,
      message: resp.message,
    };
  }

  /**
   * Get withdrawal status by transaction hash
   */
  async getWithdrawalStatus(txHash: string): Promise<WithdrawalStatus> {
    const resp = await this.post<{
      tx_hash: string;
      state: string;
      amount: number;
      to_l1_address: string;
      l1_tx_sig?: string;
    }>('/withdraw/status', { tx_hash: txHash });
    return {
      txHash: resp.tx_hash,
      state: resp.state,
      amount: BigInt(resp.amount),
      toL1Address: resp.to_l1_address,
      l1TxSig: resp.l1_tx_sig,
    };
  }

  /**
   * Get fast withdrawal quote
   */
  async getFastWithdrawQuote(amount: bigint): Promise<FastWithdrawQuote> {
    const resp = await this.post<{
      available: boolean;
      amount: number;
      fee: number;
      amount_received: number;
      fee_bps: number;
      lp_address?: string;
    }>('/withdraw/fast/quote', { amount: Number(amount) });
    return {
      available: resp.available,
      amount: BigInt(resp.amount),
      fee: BigInt(resp.fee),
      amountReceived: BigInt(resp.amount_received),
      feeBps: resp.fee_bps,
      lpAddress: resp.lp_address,
    };
  }

  // ==========================================================================
  // Shielded Operations
  // ==========================================================================

  /**
   * Submit a shielded transaction
   */
  async submitShielded(request: ShieldedRequest): Promise<ShieldedResponse> {
    const resp = await this.post<{
      tx_hash: string;
      accepted: boolean;
      position?: number;
      message: string;
    }>('/shielded/submit', {
      proof: Array.from(request.proof),
      nullifier: Array.from(request.nullifier),
      commitment: Array.from(request.commitment),
      ciphertext: Array.from(request.ciphertext),
      ephemeral_key: Array.from(request.ephemeralKey),
    });
    return {
      txHash: resp.tx_hash,
      accepted: resp.accepted,
      position: resp.position,
      message: resp.message,
    };
  }

  /**
   * Get merkle path for a commitment
   */
  async getMerklePath(position: number): Promise<MerklePath> {
    const resp = await this.post<{
      position: number;
      path: string[];
      root: string;
    }>('/shielded/merkle_path', { position });
    return {
      position: resp.position,
      path: resp.path,
      root: resp.root,
    };
  }

  /**
   * Scan for notes owned by a viewing key
   */
  async scanNotes(
    decryptionKey: Bytes32,
    ownerPk: Bytes32,
    fromPosition?: number,
    limit?: number
  ): Promise<{ notes: ScannedNote[]; scannedTo: number }> {
    const resp = await this.post<{
      notes: Array<{
        position: number;
        commitment: string;
        value: number;
        memo?: string;
      }>;
      scanned_to: number;
    }>('/shielded/scan', {
      decryption_key: Array.from(decryptionKey),
      owner_pk: Array.from(ownerPk),
      from_position: fromPosition,
      limit,
    });
    return {
      notes: resp.notes.map((n) => ({
        position: n.position,
        commitment: n.commitment,
        value: BigInt(n.value),
        memo: n.memo,
      })),
      scannedTo: resp.scanned_to,
    };
  }

  // ==========================================================================
  // Batch & Transaction Queries
  // ==========================================================================

  /**
   * Get batch by ID
   */
  async getBatch(batchId: bigint): Promise<BatchSummary | null> {
    const resp = await this.post<{
      batch?: {
        batch_id: number;
        tx_count: number;
        state_root: string;
        shielded_root: string;
        l1_tx_sig?: string;
        status: string;
        created_at: number;
        settled_at?: number;
      };
    }>('/batch', { batch_id: Number(batchId) });

    if (!resp.batch) return null;
    return parseBatchSummary(resp.batch);
  }

  /**
   * List batches with pagination
   */
  async listBatches(
    params: PaginationParams = {}
  ): Promise<{ batches: BatchSummary[]; total: number }> {
    const resp = await this.post<{
      batches: Array<{
        batch_id: number;
        tx_count: number;
        state_root: string;
        shielded_root: string;
        l1_tx_sig?: string;
        status: string;
        created_at: number;
        settled_at?: number;
      }>;
      total: number;
      offset: number;
      limit: number;
    }>('/batches', {
      offset: params.offset ?? 0,
      limit: params.limit ?? 20,
    });
    return {
      batches: resp.batches.map(parseBatchSummary),
      total: resp.total,
    };
  }

  /**
   * Get transaction by hash
   */
  async getTransaction(txHash: string): Promise<TxSummary | null> {
    const resp = await this.post<{
      tx?: {
        tx_hash: string;
        tx_type: string;
        batch_id?: number;
        status: string;
        received_at: number;
        executed_at?: number;
        amount?: number;
        from?: string;
        to?: string;
      };
    }>('/tx', { tx_hash: txHash });

    if (!resp.tx) return null;
    return parseTxSummary(resp.tx);
  }

  /**
   * List transactions with pagination and filters
   */
  async listTransactions(
    params: PaginationParams & {
      batchId?: bigint;
      txType?: string;
      status?: string;
    } = {}
  ): Promise<{ transactions: TxSummary[]; total: number }> {
    const resp = await this.post<{
      transactions: Array<{
        tx_hash: string;
        tx_type: string;
        batch_id?: number;
        status: string;
        received_at: number;
        executed_at?: number;
        amount?: number;
        from?: string;
        to?: string;
      }>;
      total: number;
      offset: number;
      limit: number;
    }>('/txs', {
      offset: params.offset ?? 0,
      limit: params.limit ?? 20,
      batch_id: params.batchId ? Number(params.batchId) : undefined,
      tx_type: params.txType,
      status: params.status,
    });
    return {
      transactions: resp.transactions.map(parseTxSummary),
      total: resp.total,
    };
  }

  // ==========================================================================
  // Committee (Threshold Encryption)
  // ==========================================================================

  /**
   * Get threshold encryption committee info
   */
  async getCommittee(): Promise<CommitteeInfo> {
    const resp = await this.get<{
      enabled: boolean;
      threshold: number;
      total_members: number;
      epoch: number;
      members: Array<{
        id: number;
        public_key: string;
        endpoint?: string;
      }>;
      pending_count: number;
    }>('/encrypted/committee');
    return {
      enabled: resp.enabled,
      threshold: resp.threshold,
      totalMembers: resp.total_members,
      epoch: BigInt(resp.epoch),
      members: resp.members.map((m) => ({
        id: m.id,
        publicKey: m.public_key,
        endpoint: m.endpoint,
      })),
      pendingCount: resp.pending_count,
    };
  }

  // ==========================================================================
  // Dev Mode Endpoints (Testing Only)
  // ==========================================================================

  /**
   * Simulate a deposit from L1 (DEV MODE ONLY)
   * 
   * This endpoint is only available when the sequencer is running with DEV_MODE=true.
   * It bypasses the L1 indexer and directly credits funds to an account.
   * 
   * @param to - Recipient account (hex-encoded 32-byte public key)
   * @param amount - Amount to deposit in lamports
   * @returns Deposit response with new balance
   * @throws ZelanaError if dev mode is not enabled (404)
   */
  async devDeposit(to: string, amount: bigint): Promise<DevDepositResponse> {
    const resp = await this.post<{
      tx_hash: string;
      accepted: boolean;
      new_balance: number;
      message: string;
    }>('/dev/deposit', {
      to,
      amount: Number(amount),
    });
    return {
      txHash: resp.tx_hash,
      accepted: resp.accepted,
      newBalance: BigInt(resp.new_balance),
      message: resp.message,
    };
  }

  /**
   * Force seal the current batch (DEV MODE ONLY)
   * 
   * This endpoint is only available when the sequencer is running with DEV_MODE=true.
   * It forces the current batch to seal immediately, bypassing the normal seal triggers.
   * 
   * @param waitForProof - Whether to wait for proof generation (default: false)
   * @returns Seal response with batch ID and transaction count
   * @throws ZelanaError if dev mode is not enabled (404)
   */
  async devSeal(waitForProof: boolean = false): Promise<DevSealResponse> {
    const resp = await this.post<{
      batch_id: number;
      tx_count: number;
      message: string;
    }>('/dev/seal', {
      wait_for_proof: waitForProof,
    });
    return {
      batchId: BigInt(resp.batch_id),
      txCount: resp.tx_count,
      message: resp.message,
    };
  }
}

// ==========================================================================
// Helper Functions
// ==========================================================================

function parseBatchSummary(b: {
  batch_id: number;
  tx_count: number;
  state_root: string;
  shielded_root: string;
  l1_tx_sig?: string;
  status: string;
  created_at: number;
  settled_at?: number;
}): BatchSummary {
  return {
    batchId: BigInt(b.batch_id),
    txCount: b.tx_count,
    stateRoot: b.state_root,
    shieldedRoot: b.shielded_root,
    l1TxSig: b.l1_tx_sig,
    status: b.status as BatchSummary['status'],
    createdAt: BigInt(b.created_at),
    settledAt: b.settled_at ? BigInt(b.settled_at) : undefined,
  };
}

function parseTxSummary(t: {
  tx_hash: string;
  tx_type: string;
  batch_id?: number;
  status: string;
  received_at: number;
  executed_at?: number;
  amount?: number;
  from?: string;
  to?: string;
}): TxSummary {
  return {
    txHash: t.tx_hash,
    txType: t.tx_type as TxSummary['txType'],
    batchId: t.batch_id ? BigInt(t.batch_id) : undefined,
    status: t.status as TxSummary['status'],
    receivedAt: BigInt(t.received_at),
    executedAt: t.executed_at ? BigInt(t.executed_at) : undefined,
    amount: t.amount ? BigInt(t.amount) : undefined,
    from: t.from,
    to: t.to,
  };
}

// BigInt JSON serialization helpers
function bigIntReplacer(_key: string, value: unknown): unknown {
  if (typeof value === 'bigint') {
    return Number(value);
  }
  return value;
}

function bigIntReviver(_key: string, value: unknown): unknown {
  // We handle bigint conversion manually in the response parsers
  return value;
}
