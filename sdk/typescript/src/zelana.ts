/**
 * Zelana SDK - High-level Client
 * 
 * Combines keypair management and API client for a convenient developer experience.
 * This is the main entry point for interacting with Zelana L2.
 */

import { Keypair, PublicKey } from './keypair';
import { ApiClient, ApiClientConfig } from './client';
import { ZelanaError } from './types';
import { bytesToHex } from './utils';
import type {
  AccountState,
  TransferResponse,
  WithdrawResponse,
  HealthInfo,
  StateRoots,
  BatchStatusInfo,
  GlobalStats,
  TxSummary,
  BatchSummary,
  PaginationParams,
  DevDepositResponse,
  DevSealResponse,
} from './types';

/**
 * Configuration for ZelanaClient
 */
export interface ZelanaClientConfig extends ApiClientConfig {
  /** Default chain ID for transactions (default: 1) */
  chainId?: bigint;
}

/**
 * High-level Zelana L2 client
 * 
 * Provides a convenient interface for common operations:
 * - Account management (balance, nonce)
 * - Transfers (sign + submit in one call)
 * - Withdrawals (sign + submit in one call)
 * - Transaction queries
 * 
 * @example
 * ```typescript
 * import { ZelanaClient, Keypair } from '@zelana/sdk';
 * 
 * const keypair = Keypair.generate();
 * const client = new ZelanaClient({
 *   baseUrl: 'http://localhost:3000',
 *   keypair,
 * });
 * 
 * // Check balance
 * const account = await client.getAccount();
 * console.log(`Balance: ${account.balance}`);
 * 
 * // Send transfer
 * const result = await client.transfer(recipientPubkey, 1_000_000n);
 * console.log(`TX Hash: ${result.txHash}`);
 * ```
 */
export class ZelanaClient {
  private readonly api: ApiClient;
  private readonly keypair: Keypair | null;
  private readonly chainId: bigint;

  constructor(config: ZelanaClientConfig & { keypair?: Keypair }) {
    this.api = new ApiClient(config);
    this.keypair = config.keypair ?? null;
    this.chainId = config.chainId ?? BigInt(1);
  }

  /**
   * Get the underlying API client for low-level operations
   */
  get apiClient(): ApiClient {
    return this.api;
  }

  /**
   * Get the public key of the configured keypair
   */
  get publicKey(): Uint8Array | null {
    return this.keypair?.publicKey ?? null;
  }

  /**
   * Get the public key as hex string
   */
  get publicKeyHex(): string | null {
    return this.keypair?.publicKeyHex ?? null;
  }

  // ==========================================================================
  // Health & Status
  // ==========================================================================

  /**
   * Check if the sequencer is healthy
   */
  async isHealthy(): Promise<boolean> {
    try {
      const health = await this.api.health();
      return health.healthy;
    } catch {
      return false;
    }
  }

  /**
   * Get health info
   */
  async health(): Promise<HealthInfo> {
    return this.api.health();
  }

  /**
   * Get current state roots
   */
  async getStateRoots(): Promise<StateRoots> {
    return this.api.getStateRoots();
  }

  /**
   * Get current batch status
   */
  async getBatchStatus(): Promise<BatchStatusInfo> {
    return this.api.getBatchStatus();
  }

  /**
   * Get global statistics
   */
  async getStats(): Promise<GlobalStats> {
    return this.api.getStats();
  }

  // ==========================================================================
  // Account Operations
  // ==========================================================================

  /**
   * Get account state for the configured keypair
   */
  async getAccount(): Promise<AccountState> {
    if (!this.keypair) {
      throw new ZelanaError('No keypair configured', 'NO_KEYPAIR');
    }
    return this.api.getAccountByPubkey(this.keypair.publicKey);
  }

  /**
   * Get account state by public key
   */
  async getAccountFor(pubkey: Uint8Array | string): Promise<AccountState> {
    if (typeof pubkey === 'string') {
      return this.api.getAccount(pubkey);
    }
    return this.api.getAccountByPubkey(pubkey);
  }

  /**
   * Get current balance for the configured keypair
   */
  async getBalance(): Promise<bigint> {
    const account = await this.getAccount();
    return account.balance;
  }

  /**
   * Get current nonce for the configured keypair
   */
  async getNonce(): Promise<bigint> {
    const account = await this.getAccount();
    return account.nonce;
  }

  // ==========================================================================
  // Transfer Operations
  // ==========================================================================

  /**
   * Transfer funds to another account
   * 
   * Signs and submits a transfer transaction in one call.
   * Automatically fetches the current nonce if not provided.
   * 
   * @param to - Recipient public key (bytes or hex string)
   * @param amount - Amount to transfer in lamports
   * @param nonce - Optional nonce (auto-fetched if not provided)
   * @returns Transfer response with transaction hash
   */
  async transfer(
    to: Uint8Array | string,
    amount: bigint,
    nonce?: bigint
  ): Promise<TransferResponse> {
    if (!this.keypair) {
      throw new ZelanaError('No keypair configured', 'NO_KEYPAIR');
    }

    // Resolve recipient pubkey
    const toPubkey = typeof to === 'string' 
      ? new PublicKey(to).toBytes() 
      : to;

    // Fetch nonce if not provided
    const txNonce = nonce ?? (await this.getNonce());

    // Sign transfer
    const request = this.keypair.signTransfer(toPubkey, amount, txNonce, this.chainId);

    // Submit
    return this.api.submitTransfer(request);
  }

  /**
   * Transfer all funds to another account (minus a small reserve)
   * 
   * @param to - Recipient public key
   * @param reserve - Amount to keep (default: 0)
   */
  async transferAll(
    to: Uint8Array | string,
    reserve: bigint = BigInt(0)
  ): Promise<TransferResponse> {
    const account = await this.getAccount();
    const amount = account.balance - reserve;
    
    if (amount <= 0n) {
      throw new ZelanaError(
        `Insufficient balance: ${account.balance} (reserve: ${reserve})`,
        'INSUFFICIENT_BALANCE'
      );
    }

    return this.transfer(to, amount, account.nonce);
  }

  // ==========================================================================
  // Withdrawal Operations
  // ==========================================================================

  /**
   * Withdraw funds to L1 (Solana)
   * 
   * Signs and submits a withdrawal request in one call.
   * Automatically fetches the current nonce if not provided.
   * 
   * @param toL1Address - Destination Solana address (bytes or base58)
   * @param amount - Amount to withdraw in lamports
   * @param nonce - Optional nonce (auto-fetched if not provided)
   * @returns Withdrawal response with transaction hash
   */
  async withdraw(
    toL1Address: Uint8Array | string,
    amount: bigint,
    nonce?: bigint
  ): Promise<WithdrawResponse> {
    if (!this.keypair) {
      throw new ZelanaError('No keypair configured', 'NO_KEYPAIR');
    }

    // Resolve L1 address
    const l1Pubkey = typeof toL1Address === 'string'
      ? new PublicKey(toL1Address).toBytes()
      : toL1Address;

    // Fetch nonce if not provided
    const txNonce = nonce ?? (await this.getNonce());

    // Sign withdrawal
    const request = this.keypair.signWithdrawal(l1Pubkey, amount, txNonce);

    // Submit
    return this.api.submitWithdrawal(request);
  }

  /**
   * Get withdrawal status
   */
  async getWithdrawalStatus(txHash: string) {
    return this.api.getWithdrawalStatus(txHash);
  }

  /**
   * Get fast withdrawal quote
   */
  async getFastWithdrawQuote(amount: bigint) {
    return this.api.getFastWithdrawQuote(amount);
  }

  // ==========================================================================
  // Transaction Queries
  // ==========================================================================

  /**
   * Get transaction by hash
   */
  async getTransaction(txHash: string): Promise<TxSummary | null> {
    return this.api.getTransaction(txHash);
  }

  /**
   * List transactions with optional filters
   */
  async listTransactions(
    params: PaginationParams & {
      batchId?: bigint;
      txType?: string;
      status?: string;
    } = {}
  ): Promise<{ transactions: TxSummary[]; total: number }> {
    return this.api.listTransactions(params);
  }

  /**
   * Get batch by ID
   */
  async getBatch(batchId: bigint): Promise<BatchSummary | null> {
    return this.api.getBatch(batchId);
  }

  /**
   * List batches
   */
  async listBatches(
    params: PaginationParams = {}
  ): Promise<{ batches: BatchSummary[]; total: number }> {
    return this.api.listBatches(params);
  }

  // ==========================================================================
  // Convenience Methods
  // ==========================================================================

  /**
   * Wait for a transaction to reach a specific status
   * 
   * @param txHash - Transaction hash to wait for
   * @param targetStatus - Status to wait for (default: 'executed')
   * @param timeoutMs - Maximum wait time (default: 60000)
   * @param pollIntervalMs - Poll interval (default: 1000)
   */
  async waitForTransaction(
    txHash: string,
    targetStatus: 'included' | 'executed' | 'settled' = 'executed',
    timeoutMs: number = 60000,
    pollIntervalMs: number = 1000
  ): Promise<TxSummary> {
    const statusOrder = ['pending', 'included', 'executed', 'settled'];
    const targetIndex = statusOrder.indexOf(targetStatus);

    const startTime = Date.now();
    while (Date.now() - startTime < timeoutMs) {
      const tx = await this.getTransaction(txHash);
      
      if (!tx) {
        throw new ZelanaError(`Transaction not found: ${txHash}`, 'TX_NOT_FOUND');
      }

      if (tx.status === 'failed') {
        throw new ZelanaError(`Transaction failed: ${txHash}`, 'TX_FAILED');
      }

      const currentIndex = statusOrder.indexOf(tx.status);
      if (currentIndex >= targetIndex) {
        return tx;
      }

      await sleep(pollIntervalMs);
    }

    throw new ZelanaError(
      `Timeout waiting for transaction ${txHash} to reach ${targetStatus}`,
      'TIMEOUT'
    );
  }

  /**
   * Wait for a batch to be settled on L1
   */
  async waitForBatch(
    batchId: bigint,
    timeoutMs: number = 120000,
    pollIntervalMs: number = 2000
  ): Promise<BatchSummary> {
    const startTime = Date.now();
    while (Date.now() - startTime < timeoutMs) {
      const batch = await this.getBatch(batchId);
      
      if (!batch) {
        throw new ZelanaError(`Batch not found: ${batchId}`, 'BATCH_NOT_FOUND');
      }

      if (batch.status === 'failed') {
        throw new ZelanaError(`Batch failed: ${batchId}`, 'BATCH_FAILED');
      }

      if (batch.status === 'settled') {
        return batch;
      }

      await sleep(pollIntervalMs);
    }

    throw new ZelanaError(
      `Timeout waiting for batch ${batchId} to settle`,
      'TIMEOUT'
    );
  }

  // ==========================================================================
  // Dev Mode Methods (Testing Only)
  // ==========================================================================

  /**
   * Simulate a deposit from L1 (DEV MODE ONLY)
   * 
   * This method is only available when the sequencer is running with DEV_MODE=true.
   * It bypasses the L1 indexer and directly credits funds to the configured keypair's account.
   * 
   * @param amount - Amount to deposit in lamports
   * @returns Deposit response with new balance
   * @throws ZelanaError if dev mode is not enabled (404) or no keypair configured
   */
  async devDeposit(amount: bigint): Promise<DevDepositResponse> {
    if (!this.keypair) {
      throw new ZelanaError('No keypair configured', 'NO_KEYPAIR');
    }
    return this.api.devDeposit(this.keypair.publicKeyHex, amount);
  }

  /**
   * Simulate a deposit to a specific account (DEV MODE ONLY)
   * 
   * This method is only available when the sequencer is running with DEV_MODE=true.
   * It bypasses the L1 indexer and directly credits funds to the specified account.
   * 
   * @param to - Recipient public key (bytes or hex string)
   * @param amount - Amount to deposit in lamports
   * @returns Deposit response with new balance
   * @throws ZelanaError if dev mode is not enabled (404)
   */
  async devDepositTo(to: Uint8Array | string, amount: bigint): Promise<DevDepositResponse> {
    const toHex = typeof to === 'string' ? to : bytesToHex(to);
    return this.api.devDeposit(toHex, amount);
  }

  /**
   * Force seal the current batch (DEV MODE ONLY)
   * 
   * This method is only available when the sequencer is running with DEV_MODE=true.
   * It forces the current batch to seal immediately, bypassing the normal seal triggers.
   * 
   * @param waitForProof - Whether to wait for proof generation (default: false)
   * @returns Seal response with batch ID and transaction count
   * @throws ZelanaError if dev mode is not enabled (404)
   */
  async devSeal(waitForProof: boolean = false): Promise<DevSealResponse> {
    return this.api.devSeal(waitForProof);
  }
}

// Helper
function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
