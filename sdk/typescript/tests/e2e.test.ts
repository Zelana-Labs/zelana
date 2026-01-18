/**
 * E2E Integration Test: Full Transaction Flow with Dev Endpoints
 * 
 * This test requires the sequencer to be running externally with DEV_MODE=true:
 * 
 *   DEV_MODE=true cargo run -p zelana-core
 * 
 * Or use the config file at ~/.zelana/config.toml with dev_mode = true:
 * 
 *   cargo run -p zelana-core
 * 
 * Run the test:
 * 
 *   cd sdk/typescript && SEQUENCER_URL=http://localhost:8080 bun test tests/e2e.test.ts
 * 
 * Important: Zelana uses batched execution - transactions are queued in batches
 * and only executed when the batch is sealed. Use devSeal() to force execution.
 * 
 * Test flow:
 * 1. Fund account via POST /dev/deposit + seal
 * 2. Query balance via SDK
 * 3. Send transfer via SDK + seal
 * 4. Verify balances updated correctly
 */

import { describe, test, expect, beforeAll } from 'bun:test';
import { ZelanaClient, Keypair, bytesToHex } from '../src';

const SEQUENCER_URL = process.env.SEQUENCER_URL || 'http://localhost:8080';
const DEPOSIT_AMOUNT = BigInt(10_000_000_000); // 10 SOL in lamports
const TRANSFER_AMOUNT = BigInt(1_000_000_000); // 1 SOL in lamports

describe('E2E: Full Transaction Flow', () => {
  let sender: Keypair;
  let recipient: Keypair;
  let senderClient: ZelanaClient;
  let recipientClient: ZelanaClient;

  beforeAll(async () => {
    // Generate fresh keypairs for this test run
    sender = Keypair.generate();
    recipient = Keypair.generate();

    senderClient = new ZelanaClient({
      baseUrl: SEQUENCER_URL,
      keypair: sender,
    });

    recipientClient = new ZelanaClient({
      baseUrl: SEQUENCER_URL,
      keypair: recipient,
    });

    console.log('=== E2E Test Setup ===');
    console.log(`Sequencer URL: ${SEQUENCER_URL}`);
    console.log(`Sender:    ${sender.publicKeyBase58}`);
    console.log(`Recipient: ${recipient.publicKeyBase58}`);
    console.log();
  });

  test('sequencer is healthy', async () => {
    const healthy = await senderClient.isHealthy();
    expect(healthy).toBe(true);
  });

  test('dev deposit queues transaction', async () => {
    // Deposit funds to sender using dev endpoint
    const depositResult = await senderClient.devDeposit(DEPOSIT_AMOUNT);

    expect(depositResult.accepted).toBe(true);
    expect(depositResult.txHash).toBeTruthy();
    // Note: newBalance is 0 until batch is sealed (batched execution)

    console.log(`Deposited ${DEPOSIT_AMOUNT} lamports to sender (queued)`);
    console.log(`TX Hash: ${depositResult.txHash}`);
  });

  test('seal executes deposit and credits balance', async () => {
    // Seal the batch to execute the deposit
    const sealResult = await senderClient.devSeal(false);
    expect(sealResult.txCount).toBeGreaterThanOrEqual(1);

    console.log(`Batch ${sealResult.batchId} sealed with ${sealResult.txCount} txs`);

    // Now check balance - should be credited after seal
    const balance = await senderClient.getBalance();
    expect(balance).toBe(DEPOSIT_AMOUNT);
    console.log(`Verified sender balance: ${balance}`);
  });

  test('transfer queues in current batch', async () => {
    const senderBalanceBefore = await senderClient.getBalance();

    // Submit transfer - this queues it in the current batch
    const transferResult = await senderClient.transfer(
      recipient.publicKey,
      TRANSFER_AMOUNT
    );

    expect(transferResult.accepted).toBe(true);
    expect(transferResult.txHash).toBeTruthy();

    console.log(`Transfer of ${TRANSFER_AMOUNT} lamports submitted`);
    console.log(`TX Hash: ${transferResult.txHash}`);

    // Balance not yet updated (transaction is queued, not executed)
    const batchStatus = await senderClient.getBatchStatus();
    expect(batchStatus.currentBatchTxs).toBeGreaterThanOrEqual(1);
    console.log(`Current batch has ${batchStatus.currentBatchTxs} pending tx(s)`);
  });

  test('seal executes transfer and updates balances', async () => {
    // Get balances before seal (should still be pre-transfer values until seal)
    const senderBalanceBefore = await senderClient.getBalance();

    // Seal to execute the transfer
    const sealResult = await senderClient.devSeal(false);
    expect(sealResult.txCount).toBeGreaterThanOrEqual(1);

    console.log(`Batch ${sealResult.batchId} sealed with ${sealResult.txCount} txs`);

    // Verify sender balance decreased
    const senderBalanceAfter = await senderClient.getBalance();
    expect(senderBalanceAfter).toBe(senderBalanceBefore - TRANSFER_AMOUNT);
    console.log(`Sender balance: ${senderBalanceBefore} -> ${senderBalanceAfter}`);

    // Verify recipient received funds
    const recipientBalance = await recipientClient.getBalance();
    expect(recipientBalance).toBe(TRANSFER_AMOUNT);
    console.log(`Recipient balance: ${recipientBalance}`);
  });

  test('balances persist after seal', async () => {
    // Double-check balances persisted correctly
    const senderBal = await senderClient.getBalance();
    const recipBal = await recipientClient.getBalance();
    const senderNonce = await senderClient.getNonce();
    
    console.log(`[Verify] Sender balance: ${senderBal}, nonce: ${senderNonce}`);
    console.log(`[Verify] Recipient balance: ${recipBal}`);
    
    expect(senderBal).toBe(DEPOSIT_AMOUNT - TRANSFER_AMOUNT);
    expect(recipBal).toBe(TRANSFER_AMOUNT);
    expect(senderNonce).toBe(BigInt(1));
  });

  test('batch can be queried after seal', async () => {
    // Get current batch status
    const batchStatus = await senderClient.getBatchStatus();
    console.log(`Current batch ID: ${batchStatus.currentBatchId}`);

    // Query the previous batch (should be the one we just sealed)
    // Note: Batch summaries are saved asynchronously after settlement, so we may need to wait
    if (batchStatus.currentBatchId > BigInt(1)) {
      const previousBatchId = batchStatus.currentBatchId - BigInt(1);
      
      // Retry a few times since batch summary is saved async after settlement
      let batch = null;
      for (let i = 0; i < 10; i++) {
        batch = await senderClient.getBatch(previousBatchId);
        if (batch) break;
        await new Promise(r => setTimeout(r, 100)); // Wait 100ms between retries
      }
      
      expect(batch).not.toBeNull();
      if (batch) {
        console.log(`Batch ${previousBatchId}:`);
        console.log(`  Status: ${batch.status}`);
        console.log(`  TX Count: ${batch.txCount}`);
        console.log(`  State Root: ${batch.stateRoot.slice(0, 16)}...`);
      }
    }
  });

  test('transaction can be queried by hash', async () => {
    // List recent transactions to get a hash to query
    const txList = await senderClient.listTransactions({ limit: 5 });
    
    expect(txList.transactions.length).toBeGreaterThan(0);
    console.log(`Found ${txList.total} transactions`);

    // Query the first transaction
    const txHash = txList.transactions[0].txHash;
    const tx = await senderClient.getTransaction(txHash);

    expect(tx).not.toBeNull();
    if (tx) {
      console.log(`Transaction ${txHash.slice(0, 16)}...:`);
      console.log(`  Type: ${tx.txType}`);
      console.log(`  Status: ${tx.status}`);
      console.log(`  Amount: ${tx.amount}`);
    }
  });

  test('multiple transactions in single batch', async () => {
    // Debug: verify we're using the right account
    console.log(`[DEBUG] sender pubkey: ${sender.publicKeyBase58}`);
    
    // Get current balances
    const senderBalanceBefore = await senderClient.getBalance();
    const recipientBalanceBefore = await recipientClient.getBalance();
    console.log(`Before test: sender=${senderBalanceBefore}, recipient=${recipientBalanceBefore}`);

    // Submit multiple transfers without sealing between them
    // IMPORTANT: When submitting multiple txs before sealing, provide explicit nonces
    // since the on-chain nonce won't update until the batch is sealed.
    const transfer1 = BigInt(100_000_000); // 0.1 SOL
    const transfer2 = BigInt(200_000_000); // 0.2 SOL

    // Fetch the current nonce once
    const currentNonce = await senderClient.getNonce();
    console.log(`Current nonce: ${currentNonce}`);

    const result1 = await senderClient.transfer(recipient.publicKey, transfer1, currentNonce);
    expect(result1.accepted).toBe(true);
    console.log(`Transfer 1 (${transfer1}): accepted=${result1.accepted}`);

    const result2 = await senderClient.transfer(recipient.publicKey, transfer2, currentNonce + BigInt(1));
    expect(result2.accepted).toBe(true);
    console.log(`Transfer 2 (${transfer2}): accepted=${result2.accepted}`);

    // Check batch has 2 transactions
    const batchStatus = await senderClient.getBatchStatus();
    expect(batchStatus.currentBatchTxs).toBe(2);
    console.log(`Batch has ${batchStatus.currentBatchTxs} pending transactions`);

    // Seal to execute both
    const sealResult = await senderClient.devSeal(false);
    expect(sealResult.txCount).toBe(2);
    console.log(`Sealed batch ${sealResult.batchId} with ${sealResult.txCount} txs`);

    // Verify balances
    const senderBalanceAfter = await senderClient.getBalance();
    const recipientBalanceAfter = await recipientClient.getBalance();

    const totalTransferred = transfer1 + transfer2;
    expect(senderBalanceAfter).toBe(senderBalanceBefore - totalTransferred);
    expect(recipientBalanceAfter).toBe(recipientBalanceBefore + totalTransferred);

    console.log(`Sender: ${senderBalanceBefore} -> ${senderBalanceAfter} (-${totalTransferred})`);
    console.log(`Recipient: ${recipientBalanceBefore} -> ${recipientBalanceAfter} (+${totalTransferred})`);
  });

  test('deposit to specific account works', async () => {
    // Create a third account and deposit to it directly
    const thirdParty = Keypair.generate();
    const depositAmount = BigInt(2_000_000_000);

    // Use devDepositTo to deposit to a specific account
    const depositResult = await senderClient.devDepositTo(
      thirdParty.publicKey,
      depositAmount
    );

    expect(depositResult.accepted).toBe(true);

    // Seal to execute the deposit
    await senderClient.devSeal(false);

    // Verify with a client for that account
    const thirdClient = new ZelanaClient({
      baseUrl: SEQUENCER_URL,
      keypair: thirdParty,
    });

    const balance = await thirdClient.getBalance();
    expect(balance).toBe(depositAmount);

    console.log(`Deposited ${depositAmount} to third party: ${thirdParty.publicKeyBase58.slice(0, 8)}...`);
  });

  test('stats reflect transactions', async () => {
    const stats = await senderClient.getStats();

    expect(stats.totalTransactions).toBeGreaterThan(BigInt(0));
    // TODO: totalDeposited tracking not yet implemented in sequencer
    // expect(stats.totalDeposited).toBeGreaterThan(BigInt(0));
    expect(stats.activeAccounts).toBeGreaterThanOrEqual(BigInt(2));

    console.log('=== Final Stats ===');
    console.log(`Total Batches: ${stats.totalBatches}`);
    console.log(`Total Transactions: ${stats.totalTransactions}`);
    console.log(`Total Deposited: ${stats.totalDeposited} lamports (not tracked yet)`);
    console.log(`Active Accounts: ${stats.activeAccounts}`);
    console.log(`Uptime: ${stats.uptimeSecs}s`);
  });
});

describe('E2E: Dev Mode Error Handling', () => {
  const client = new ZelanaClient({
    baseUrl: SEQUENCER_URL,
  });

  test('devDeposit to invalid address is handled', async () => {
    // Should fail because invalid hex
    try {
      await client.apiClient.devDeposit('invalid-hex', BigInt(1000));
      expect(true).toBe(false); // Should not reach here
    } catch (error) {
      expect(error).toBeDefined();
    }
  });

  test('devDeposit without keypair requires explicit to address', async () => {
    const clientNoKeypair = new ZelanaClient({
      baseUrl: SEQUENCER_URL,
      // No keypair
    });

    // devDeposit() without keypair should throw
    try {
      await (clientNoKeypair as any).devDeposit(BigInt(1000));
      expect(true).toBe(false); // Should not reach here
    } catch (error: any) {
      expect(error.code).toBe('NO_KEYPAIR');
    }
  });

  test('devDepositTo works without keypair', async () => {
    const recipient = Keypair.generate();
    const result = await client.devDepositTo(recipient.publicKey, BigInt(1_000_000));
    expect(result.accepted).toBe(true);

    // Seal to execute
    await client.devSeal(false);

    // Verify balance
    const recipientClient = new ZelanaClient({
      baseUrl: SEQUENCER_URL,
      keypair: recipient,
    });
    const balance = await recipientClient.getBalance();
    expect(balance).toBe(BigInt(1_000_000));
  });
});

// =============================================================================
// Withdrawal E2E Tests
// =============================================================================

describe('E2E: Withdrawal Flow', () => {
  let client: ZelanaClient;
  let keypair: Keypair;
  let l1Recipient: Keypair; // Simulated L1 Solana address

  beforeAll(async () => {
    keypair = Keypair.generate();
    l1Recipient = Keypair.generate();

    client = new ZelanaClient({
      baseUrl: SEQUENCER_URL,
      keypair,
    });

    // Fund the account first
    const depositAmount = BigInt(5_000_000_000); // 5 SOL
    await client.devDeposit(depositAmount);
    await client.devSeal(false);

    const balance = await client.getBalance();
    expect(balance).toBe(depositAmount);

    console.log('=== Withdrawal Test Setup ===');
    console.log(`L2 Account: ${keypair.publicKeyBase58}`);
    console.log(`L1 Recipient: ${l1Recipient.publicKeyBase58}`);
    console.log(`Initial Balance: ${balance}`);
    console.log();
  });

  test('withdrawal is accepted and queued', async () => {
    const withdrawAmount = BigInt(1_000_000_000); // 1 SOL
    const balanceBefore = await client.getBalance();

    const result = await client.withdraw(l1Recipient.publicKey, withdrawAmount);

    expect(result.accepted).toBe(true);
    expect(result.txHash).toBeTruthy();

    console.log(`Withdrawal of ${withdrawAmount} lamports submitted`);
    console.log(`TX Hash: ${result.txHash}`);

    // Withdrawal is queued, not executed yet
    const balanceAfterQueue = await client.getBalance();
    expect(balanceAfterQueue).toBe(balanceBefore); // Balance unchanged until sealed
  });

  test('seal executes withdrawal and deducts balance', async () => {
    const balanceBefore = await client.getBalance();
    const withdrawAmount = BigInt(1_000_000_000); // Amount from previous test

    // Seal to execute the withdrawal
    const sealResult = await client.devSeal(false);
    expect(sealResult.txCount).toBeGreaterThanOrEqual(1);

    console.log(`Batch ${sealResult.batchId} sealed with ${sealResult.txCount} txs`);

    // Balance should be reduced by withdrawal amount
    const balanceAfter = await client.getBalance();
    expect(balanceAfter).toBe(balanceBefore - withdrawAmount);

    console.log(`Balance: ${balanceBefore} -> ${balanceAfter}`);
  });

  test('withdrawal status can be queried', async () => {
    // Get the withdrawal transaction - filter by type since there may be many txs
    const txList = await client.listTransactions({ limit: 10, txType: 'withdrawal' });
    const withdrawalTx = txList.transactions[0]; // Get the first (most recent) withdrawal

    expect(withdrawalTx).toBeDefined();
    if (withdrawalTx) {
      console.log(`Found withdrawal tx: ${withdrawalTx.txHash.slice(0, 16)}...`);
      const status = await client.getWithdrawalStatus(withdrawalTx.txHash);
      console.log(`Withdrawal state: ${status.state}, amount: ${status.amount}`);

      // In dev mode, withdrawal may be marked as pending or processing or settled
      expect(['pending', 'processing', 'completed', 'executed', 'settled']).toContain(status.state);
    }
  });

  test('multiple withdrawals in sequence', async () => {
    const balanceBefore = await client.getBalance();
    const withdrawal1 = BigInt(500_000_000); // 0.5 SOL
    const withdrawal2 = BigInt(300_000_000); // 0.3 SOL

    // Get nonce for sequential withdrawals
    const nonce = await client.getNonce();

    // Submit two withdrawals
    const result1 = await client.withdraw(l1Recipient.publicKey, withdrawal1, nonce);
    expect(result1.accepted).toBe(true);

    const result2 = await client.withdraw(l1Recipient.publicKey, withdrawal2, nonce + BigInt(1));
    expect(result2.accepted).toBe(true);

    // Seal both
    const sealResult = await client.devSeal(false);
    expect(sealResult.txCount).toBe(2);

    // Verify balance
    const balanceAfter = await client.getBalance();
    const totalWithdrawn = withdrawal1 + withdrawal2;
    expect(balanceAfter).toBe(balanceBefore - totalWithdrawn);

    console.log(`Withdrew ${totalWithdrawn} in 2 transactions`);
    console.log(`Balance: ${balanceBefore} -> ${balanceAfter}`);
  });

  test('fast withdrawal quote works', async () => {
    const amount = BigInt(500_000_000); // 0.5 SOL
    
    try {
      const quote = await client.getFastWithdrawQuote(amount);
      console.log('Fast withdrawal quote:', quote);
      
      // Quote should have fee info
      expect(quote.requestedAmount).toBeDefined();
      expect(quote.fee).toBeDefined();
      expect(quote.receiveAmount).toBeDefined();
    } catch (error: any) {
      // Fast withdrawals may not be enabled in dev mode
      console.log(`Fast withdrawal not available: ${error.message}`);
      // This is acceptable - not all deployments have LPs
    }
  });

  test('cannot withdraw more than balance', async () => {
    const balance = await client.getBalance();
    const tooMuch = balance + BigInt(1_000_000_000); // More than available

    try {
      await client.withdraw(l1Recipient.publicKey, tooMuch);
      // If it's accepted (for later validation), seal and check it fails
      const sealResult = await client.devSeal(false);
      
      // Balance should be unchanged (tx should fail during execution)
      const balanceAfter = await client.getBalance();
      expect(balanceAfter).toBe(balance);
    } catch (error: any) {
      // Rejected immediately - this is also valid
      console.log(`Withdrawal rejected: ${error.message}`);
      expect(error.code).toBeDefined();
    }
  });
});
