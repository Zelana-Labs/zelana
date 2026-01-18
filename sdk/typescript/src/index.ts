/**
 * Zelana TypeScript SDK
 * 
 * Official SDK for interacting with Zelana L2, a privacy-focused Solana rollup.
 * 
 * @example
 * ```typescript
 * import { ZelanaClient, Keypair } from '@zelana/sdk';
 * 
 * // Generate a new keypair
 * const keypair = Keypair.generate();
 * console.log('Public Key:', keypair.publicKeyBase58);
 * 
 * // Create client
 * const client = new ZelanaClient({
 *   baseUrl: 'http://localhost:3000',
 *   keypair,
 * });
 * 
 * // Check health
 * const healthy = await client.isHealthy();
 * console.log('Sequencer healthy:', healthy);
 * 
 * // Get account balance
 * const balance = await client.getBalance();
 * console.log('Balance:', balance);
 * 
 * // Transfer funds
 * const result = await client.transfer(recipientPubkey, 1_000_000n);
 * console.log('TX Hash:', result.txHash);
 * 
 * // Wait for confirmation
 * await client.waitForTransaction(result.txHash);
 * console.log('Transfer confirmed!');
 * ```
 * 
 * @packageDocumentation
 */

// Main client
export { ZelanaClient } from './zelana';
export type { ZelanaClientConfig } from './zelana';

// Keypair & PublicKey
export { Keypair, PublicKey } from './keypair';

// Low-level API client
export { ApiClient } from './client';
export type { ApiClientConfig } from './client';

// Types
export type {
  // Core types
  Bytes32,
  AccountState,
  
  // Transaction types
  TxType,
  TxStatus,
  TxSummary,
  BatchStatus,
  BatchSummary,
  
  // Request/Response types
  TransferRequest,
  TransferResponse,
  WithdrawRequest,
  WithdrawResponse,
  WithdrawalStatus,
  ShieldedRequest,
  ShieldedResponse,
  
  // Status types
  StateRoots,
  BatchStatusInfo,
  HealthInfo,
  GlobalStats,
  
  // Shielded types
  MerklePath,
  ScannedNote,
  
  // Fast withdrawal
  FastWithdrawQuote,
  
  // Committee
  CommitteeMemberInfo,
  CommitteeInfo,
  
  // Pagination
  PaginationParams,
  PaginatedResponse,
  
  // Dev mode types
  DevDepositRequest,
  DevDepositResponse,
  DevSealRequest,
  DevSealResponse,
  
  // Errors
  ApiError,
} from './types';

export { ZelanaError } from './types';

// Shielded (Privacy) Transactions
export {
  shielded,
  ShieldedTransactionBuilder,
  generateShieldedKeys,
  shieldedKeysFromSpendingKey,
  createNote,
  noteWithRandomness,
  computeCommitment,
  computeNullifier,
  tryDecryptNote,
} from './shielded';

export type {
  Note,
  EncryptedNote,
  ShieldedKeys,
  ShieldedInput,
  ShieldedOutput,
  PreparedShieldedTx,
  ShieldedTransaction,
  ShieldedWitness,
  ScanResult,
  MerklePath as ShieldedMerklePath,
} from './shielded';

// Utilities
export {
  bytesToHex,
  hexToBytes,
  bytesToBase58,
  base58ToBytes,
  u64ToLeBytes,
  leBytesToU64,
  concatBytes,
  bytesEqual,
  randomBytes,
} from './utils';
