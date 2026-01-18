/**
 * Example: Withdraw to L1
 * 
 * This example demonstrates how to:
 * 1. Connect with a keypair
 * 2. Check balance
 * 3. Submit a withdrawal to L1 (Solana)
 * 4. Check withdrawal status
 * 5. Optionally use fast withdrawal
 * 
 * Run with: bun run examples/withdraw-to-l1.ts
 */

import { ZelanaClient, Keypair, PublicKey } from '../src';

const SEQUENCER_URL = process.env.SEQUENCER_URL || 'http://localhost:3000';

async function main() {
  console.log('=== Zelana SDK: Withdrawal Example ===\n');

  // 1. Load keypair (in production, load from secure storage)
  // For this example, we'll generate one
  const keypair = Keypair.generate();
  console.log(`L2 Account: ${keypair.publicKeyBase58}`);

  // Destination L1 address (could be the same or different)
  const l1Destination = keypair.publicKeyBase58; // Same key for simplicity
  console.log(`L1 Destination: ${l1Destination}\n`);

  // 2. Create client
  const client = new ZelanaClient({
    baseUrl: SEQUENCER_URL,
    keypair,
  });

  // 3. Check health
  const healthy = await client.isHealthy();
  if (!healthy) {
    console.error('Sequencer is not healthy!');
    process.exit(1);
  }

  // 4. Check balance
  console.log('Checking balance...');
  try {
    const account = await client.getAccount();
    console.log(`Balance: ${account.balance} lamports`);
    console.log(`Nonce:   ${account.nonce}\n`);

    // 5. Check fast withdrawal availability
    const withdrawAmount = 10_000_000n; // 0.01 SOL
    console.log('Checking fast withdrawal quote...');
    const quote = await client.getFastWithdrawQuote(withdrawAmount);
    
    if (quote.available) {
      console.log('Fast withdrawal available!');
      console.log(`  Amount:   ${quote.amount} lamports`);
      console.log(`  Fee:      ${quote.fee} lamports (${quote.feeBps} bps)`);
      console.log(`  Receive:  ${quote.amountReceived} lamports`);
      console.log(`  LP:       ${quote.lpAddress}\n`);
    } else {
      console.log('Fast withdrawal not available (using standard withdrawal)\n');
    }

    // 6. Submit withdrawal
    if (account.balance >= withdrawAmount) {
      console.log(`Submitting withdrawal of ${withdrawAmount} lamports...`);
      
      const result = await client.withdraw(
        new PublicKey(l1Destination).toBytes(),
        withdrawAmount
      );

      if (result.accepted) {
        console.log(`Withdrawal accepted! TX Hash: ${result.txHash}`);
        console.log(`Estimated completion: ${result.estimatedCompletion || 'unknown'}\n`);

        // 7. Poll withdrawal status
        console.log('Checking withdrawal status...');
        const status = await client.getWithdrawalStatus(result.txHash);
        console.log(`State: ${status.state}`);
        console.log(`Amount: ${status.amount} lamports`);
        console.log(`To L1: ${status.toL1Address}`);
        if (status.l1TxSig) {
          console.log(`L1 TX: ${status.l1TxSig}`);
        }
      } else {
        console.log(`Withdrawal rejected: ${result.message}`);
      }
    } else {
      console.log('Insufficient balance for withdrawal.');
      console.log(`Need ${withdrawAmount} lamports, have ${account.balance}`);
    }
  } catch (error) {
    console.log('Account not found or has no balance.');
    console.log('\nTo test withdrawals:');
    console.log('1. First deposit funds via the L1 bridge');
    console.log(`2. Use destination: ${keypair.publicKeyHex}`);
    console.log('3. Wait for deposit to be processed');
    console.log('4. Run this example again');
  }

  console.log('\n=== Example Complete ===');
}

main().catch(console.error);
