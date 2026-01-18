/**
 * Zelana SDK Type Definitions
 * 
 * These types mirror the Rust API types in core/src/api/types.rs
 */

// ============================================================================
// Core Types
// ============================================================================

/** 32-byte array represented as Uint8Array */
export type Bytes32 = Uint8Array;

/** Account state on L2 */
export interface AccountState {
  accountId: string;
  balance: bigint;
  nonce: bigint;
}

// ============================================================================
// Transaction Types
// ============================================================================

/** Transaction type enumeration */
export type TxType = 'deposit' | 'transfer' | 'shielded' | 'withdrawal';

/** Transaction status */
export type TxStatus = 'pending' | 'included' | 'executed' | 'settled' | 'failed';

/** Batch status */
export type BatchStatus = 'building' | 'proving' | 'pending_settlement' | 'settled' | 'failed';

/** Transaction summary */
export interface TxSummary {
  txHash: string;
  txType: TxType;
  batchId?: bigint;
  status: TxStatus;
  receivedAt: bigint;
  executedAt?: bigint;
  amount?: bigint;
  from?: string;
  to?: string;
}

/** Batch summary */
export interface BatchSummary {
  batchId: bigint;
  txCount: number;
  stateRoot: string;
  shieldedRoot: string;
  l1TxSig?: string;
  status: BatchStatus;
  createdAt: bigint;
  settledAt?: bigint;
}

// ============================================================================
// Request/Response Types
// ============================================================================

/** Transfer request */
export interface TransferRequest {
  from: Bytes32;
  to: Bytes32;
  amount: bigint;
  nonce: bigint;
  chainId: bigint;
  signature: Uint8Array;
  signerPubkey: Bytes32;
}

/** Transfer response */
export interface TransferResponse {
  txHash: string;
  accepted: boolean;
  message: string;
}

/** Withdrawal request */
export interface WithdrawRequest {
  from: Bytes32;
  toL1Address: Bytes32;
  amount: bigint;
  nonce: bigint;
  signature: Uint8Array;
  signerPubkey: Bytes32;
}

/** Withdrawal response */
export interface WithdrawResponse {
  txHash: string;
  accepted: boolean;
  estimatedCompletion?: string;
  message: string;
}

/** Withdrawal status */
export interface WithdrawalStatus {
  txHash: string;
  state: string;
  amount: bigint;
  toL1Address: string;
  l1TxSig?: string;
}

/** Shielded transaction request */
export interface ShieldedRequest {
  proof: Uint8Array;
  nullifier: Bytes32;
  commitment: Bytes32;
  ciphertext: Uint8Array;
  ephemeralKey: Bytes32;
}

/** Shielded transaction response */
export interface ShieldedResponse {
  txHash: string;
  accepted: boolean;
  position?: number;
  message: string;
}

/** State roots response */
export interface StateRoots {
  batchId: bigint;
  stateRoot: string;
  shieldedRoot: string;
  commitmentCount: bigint;
}

/** Batch status response */
export interface BatchStatusInfo {
  currentBatchId: bigint;
  currentBatchTxs: number;
  provingCount: number;
  pendingSettlement: number;
}

/** Health response */
export interface HealthInfo {
  healthy: boolean;
  version: string;
  uptimeSecs: bigint;
}

/** Global statistics */
export interface GlobalStats {
  totalBatches: bigint;
  totalTransactions: bigint;
  totalDeposited: bigint;
  totalWithdrawn: bigint;
  currentBatchId: bigint;
  activeAccounts: bigint;
  shieldedCommitments: bigint;
  uptimeSecs: bigint;
}

/** Fast withdrawal quote */
export interface FastWithdrawQuote {
  available: boolean;
  amount: bigint;
  fee: bigint;
  amountReceived: bigint;
  feeBps: number;
  lpAddress?: string;
}

/** Committee member info */
export interface CommitteeMemberInfo {
  id: number;
  publicKey: string;
  endpoint?: string;
}

/** Committee info */
export interface CommitteeInfo {
  enabled: boolean;
  threshold: number;
  totalMembers: number;
  epoch: bigint;
  members: CommitteeMemberInfo[];
  pendingCount: number;
}

/** Merkle path response */
export interface MerklePath {
  position: number;
  path: string[];
  root: string;
}

/** Scanned note */
export interface ScannedNote {
  position: number;
  commitment: string;
  value: bigint;
  memo?: string;
}

// ============================================================================
// Pagination
// ============================================================================

/** Pagination parameters */
export interface PaginationParams {
  offset?: number;
  limit?: number;
}

/** Paginated response */
export interface PaginatedResponse<T> {
  items: T[];
  total: number;
  offset: number;
  limit: number;
}

// ============================================================================
// Error Types
// ============================================================================

/** API error response */
export interface ApiError {
  error: string;
  code: string;
}

/** SDK error class */
export class ZelanaError extends Error {
  constructor(
    message: string,
    public code: string,
    public cause?: unknown
  ) {
    super(message);
    this.name = 'ZelanaError';
  }
}

// ============================================================================
// Dev Mode Types (Testing Only)
// ============================================================================

/** Dev deposit request - simulates L1 deposit without real indexer */
export interface DevDepositRequest {
  /** Recipient account (hex-encoded 32-byte public key) */
  to: string;
  /** Amount in lamports */
  amount: bigint;
}

/** Dev deposit response */
export interface DevDepositResponse {
  /** Transaction hash */
  txHash: string;
  /** Whether the deposit was accepted */
  accepted: boolean;
  /** New balance after deposit */
  newBalance: bigint;
  /** Status message */
  message: string;
}

/** Dev seal request - forces current batch to seal */
export interface DevSealRequest {
  /** Whether to wait for proof generation (default: false) */
  waitForProof?: boolean;
}

/** Dev seal response */
export interface DevSealResponse {
  /** Sealed batch ID */
  batchId: bigint;
  /** Number of transactions in the sealed batch */
  txCount: number;
  /** Status message */
  message: string;
}
