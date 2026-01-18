/**
 * Integration Test: SDK against Live Sequencer
 * 
 * This script tests the SDK against a running Zelana sequencer.
 * 
 * Prerequisites:
 * 1. Start the sequencer: cargo run -p zelana-core
 * 2. Run this test: npx ts-node tests/integration.test.ts
 * 
 * The test will:
 * - Check health endpoint
 * - Query status endpoints
 * - Create keypairs and sign transactions
 * - Submit a transfer (will fail if account has no balance, which is expected)
 */

import { ZelanaClient, Keypair, ApiClient, bytesToHex } from '../src';

const SEQUENCER_URL = process.env.SEQUENCER_URL || 'http://localhost:3000';

async function runIntegrationTests() {
  console.log('=== Zelana SDK Integration Tests ===\n');
  console.log(`Sequencer URL: ${SEQUENCER_URL}\n`);

  let passed = 0;
  let failed = 0;

  async function test(name: string, fn: () => Promise<void>) {
    try {
      await fn();
      console.log(`✓ ${name}`);
      passed++;
    } catch (error) {
      console.log(`✗ ${name}`);
      console.log(`  Error: ${error instanceof Error ? error.message : error}`);
      failed++;
    }
  }

  // Create clients
  const api = new ApiClient({ baseUrl: SEQUENCER_URL });
  const keypair = Keypair.generate();
  const client = new ZelanaClient({ baseUrl: SEQUENCER_URL, keypair });

  console.log(`Test keypair: ${keypair.publicKeyBase58}\n`);

  // =========================================================================
  // Health & Status Tests
  // =========================================================================
  console.log('--- Health & Status ---');

  await test('Health endpoint returns healthy', async () => {
    const health = await api.health();
    if (!health.healthy) throw new Error('Sequencer not healthy');
    if (!health.version) throw new Error('Missing version');
  });

  await test('State roots endpoint works', async () => {
    const roots = await api.getStateRoots();
    if (typeof roots.batchId !== 'bigint') throw new Error('Invalid batch ID');
    if (!roots.stateRoot) throw new Error('Missing state root');
    if (!roots.shieldedRoot) throw new Error('Missing shielded root');
  });

  await test('Batch status endpoint works', async () => {
    const status = await api.getBatchStatus();
    if (typeof status.currentBatchId !== 'bigint') throw new Error('Invalid batch ID');
  });

  await test('Stats endpoint works', async () => {
    const stats = await api.getStats();
    if (typeof stats.totalBatches !== 'bigint') throw new Error('Invalid total batches');
    if (typeof stats.totalTransactions !== 'bigint') throw new Error('Invalid total txs');
  });

  await test('isHealthy convenience method works', async () => {
    const healthy = await client.isHealthy();
    if (typeof healthy !== 'boolean') throw new Error('Invalid return type');
  });

  console.log();

  // =========================================================================
  // Keypair & Signing Tests
  // =========================================================================
  console.log('--- Keypair & Signing ---');

  await test('Generate and sign transfer', async () => {
    const sender = Keypair.generate();
    const recipient = Keypair.generate();
    
    const request = sender.signTransfer(
      recipient.publicKey,
      BigInt(1000000),
      BigInt(0),
      BigInt(1)
    );

    if (request.signature.length !== 64) throw new Error('Invalid signature length');
    if (!bytesToHex(request.from).length) throw new Error('Missing from');
    if (!bytesToHex(request.to).length) throw new Error('Missing to');
  });

  await test('Generate and sign withdrawal', async () => {
    const sender = Keypair.generate();
    const l1Addr = Keypair.generate().publicKey;
    
    const request = sender.signWithdrawal(
      l1Addr,
      BigInt(5000000),
      BigInt(0)
    );

    if (request.signature.length !== 64) throw new Error('Invalid signature length');
    if (request.amount !== BigInt(5000000)) throw new Error('Wrong amount');
  });

  await test('Signature verification works', async () => {
    const kp = Keypair.generate();
    const msg = new Uint8Array([1, 2, 3, 4, 5]);
    const sig = kp.sign(msg);
    
    if (!Keypair.verify(sig, msg, kp.publicKey)) {
      throw new Error('Valid signature rejected');
    }
    
    // Tamper and verify it fails
    sig[0] ^= 0xff;
    if (Keypair.verify(sig, msg, kp.publicKey)) {
      throw new Error('Invalid signature accepted');
    }
  });

  console.log();

  // =========================================================================
  // Account & Transaction Tests
  // =========================================================================
  console.log('--- Account & Transactions ---');

  await test('Account query for unfunded account returns error', async () => {
    try {
      await client.getAccount();
      throw new Error('Expected error for unfunded account');
    } catch (error) {
      // Expected - account doesn't exist
      if (error instanceof Error && error.message.includes('Expected error')) {
        throw error;
      }
      // Good - got expected error
    }
  });

  await test('Transfer to unfunded account fails gracefully', async () => {
    const recipient = Keypair.generate();
    try {
      await client.transfer(recipient.publicKey, BigInt(1000));
      throw new Error('Expected error for unfunded sender');
    } catch (error) {
      // Expected - sender has no balance
      if (error instanceof Error && error.message.includes('Expected error')) {
        throw error;
      }
      // Good - got expected error
    }
  });

  await test('List batches returns array', async () => {
    const result = await client.listBatches({ limit: 10 });
    if (!Array.isArray(result.batches)) throw new Error('Expected array');
    if (typeof result.total !== 'number') throw new Error('Expected total count');
  });

  await test('List transactions returns array', async () => {
    const result = await client.listTransactions({ limit: 10 });
    if (!Array.isArray(result.transactions)) throw new Error('Expected array');
    if (typeof result.total !== 'number') throw new Error('Expected total count');
  });

  console.log();

  // =========================================================================
  // Summary
  // =========================================================================
  console.log('=== Summary ===');
  console.log(`Passed: ${passed}`);
  console.log(`Failed: ${failed}`);
  console.log();

  if (failed > 0) {
    console.log('Some tests failed!');
    process.exit(1);
  } else {
    console.log('All tests passed!');
  }
}

// Run tests
runIntegrationTests().catch((error) => {
  console.error('Integration test error:', error);
  process.exit(1);
});
