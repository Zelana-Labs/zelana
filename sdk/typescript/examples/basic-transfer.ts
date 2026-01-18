/**
 * Example: Basic Transfer
 * 
 * This example demonstrates how to:
 * 1. Generate a keypair
 * 2. Connect to the Zelana L2 sequencer
 * 3. Check account balance
 * 4. Send a transfer
 * 5. Wait for confirmation
 * 
 * Run with: bun run examples/basic-transfer.ts
 */

import { ZelanaClient, Keypair, PublicKey } from '../src';

const SEQUENCER_URL = process.env.SEQUENCER_URL || 'http://localhost:3000';

async function main() {
  console.log('=== Zelana SDK: Basic Transfer Example ===\n');

  // 1. Generate keypairs (in production, load from secure storage)
  console.log('Generating keypairs...');
  const sender = Keypair.generate();
  const recipient = Keypair.generate();

  console.log(`Sender:    ${sender.publicKeyBase58}`);
  console.log(`Recipient: ${recipient.publicKeyBase58}\n`);

  // 2. Create client
  const client = new ZelanaClient({
    baseUrl: SEQUENCER_URL,
    keypair: sender,
  });

  // 3. Check sequencer health
  console.log('Checking sequencer health...');
  const healthy = await client.isHealthy();
  if (!healthy) {
    console.error('Sequencer is not healthy!');
    process.exit(1);
  }
  console.log('Sequencer is healthy!\n');

  // 4. Get initial account state
  console.log('Fetching account state...');
  try {
    const account = await client.getAccount();
    console.log(`Balance: ${account.balance} lamports`);
    console.log(`Nonce:   ${account.nonce}\n`);

    // 5. Send transfer (if we have funds)
    if (account.balance >= 1000n) {
      const amount = 1000n; // 1000 lamports
      console.log(`Sending ${amount} lamports to ${recipient.publicKeyBase58}...`);
      
      const result = await client.transfer(recipient.publicKey, amount);
      
      if (result.accepted) {
        console.log(`Transfer accepted! TX Hash: ${result.txHash}`);
        
        // 6. Wait for execution
        console.log('\nWaiting for transaction to execute...');
        const tx = await client.waitForTransaction(result.txHash, 'executed', 30000);
        console.log(`Transaction executed! Status: ${tx.status}`);
        
        // 7. Check updated balance
        const updatedAccount = await client.getAccount();
        console.log(`\nUpdated balance: ${updatedAccount.balance} lamports`);
      } else {
        console.log(`Transfer rejected: ${result.message}`);
      }
    } else {
      console.log('Insufficient balance for transfer.');
      console.log('(This account needs to receive a deposit first)');
    }
  } catch (error) {
    console.log('Account not found (needs deposit first)');
    console.log('\nTo fund this account:');
    console.log(`1. Deposit SOL to the L1 bridge with destination: ${sender.publicKeyHex}`);
    console.log('2. Wait for the deposit to be processed');
    console.log('3. Run this example again');
  }

  console.log('\n=== Example Complete ===');
}

main().catch(console.error);
