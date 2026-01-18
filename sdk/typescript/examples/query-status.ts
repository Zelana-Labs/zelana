/**
 * Example: Query Sequencer Status
 * 
 * This example demonstrates how to:
 * 1. Connect to the sequencer (no keypair needed)
 * 2. Query health and status
 * 3. Get global statistics
 * 4. Browse batches and transactions
 * 
 * Run with: bun run examples/query-status.ts
 */

import { ApiClient } from '../src';

const SEQUENCER_URL = process.env.SEQUENCER_URL || 'http://localhost:3000';

async function main() {
  console.log('=== Zelana SDK: Query Status Example ===\n');

  // Create low-level API client (no keypair needed for queries)
  const api = new ApiClient({
    baseUrl: SEQUENCER_URL,
  });

  // 1. Health check
  console.log('--- Health Check ---');
  try {
    const health = await api.health();
    console.log(`Healthy:  ${health.healthy}`);
    console.log(`Version:  ${health.version}`);
    console.log(`Uptime:   ${health.uptimeSecs} seconds\n`);
  } catch (error) {
    console.error('Failed to connect to sequencer:', error);
    process.exit(1);
  }

  // 2. State roots
  console.log('--- State Roots ---');
  const roots = await api.getStateRoots();
  console.log(`Batch ID:         ${roots.batchId}`);
  console.log(`State Root:       ${roots.stateRoot.slice(0, 20)}...`);
  console.log(`Shielded Root:    ${roots.shieldedRoot.slice(0, 20)}...`);
  console.log(`Commitment Count: ${roots.commitmentCount}\n`);

  // 3. Batch status
  console.log('--- Batch Status ---');
  const batchStatus = await api.getBatchStatus();
  console.log(`Current Batch:      ${batchStatus.currentBatchId}`);
  console.log(`TXs in Batch:       ${batchStatus.currentBatchTxs}`);
  console.log(`Proving:            ${batchStatus.provingCount}`);
  console.log(`Pending Settlement: ${batchStatus.pendingSettlement}\n`);

  // 4. Global statistics
  console.log('--- Global Statistics ---');
  const stats = await api.getStats();
  console.log(`Total Batches:      ${stats.totalBatches}`);
  console.log(`Total Transactions: ${stats.totalTransactions}`);
  console.log(`Total Deposited:    ${stats.totalDeposited} lamports`);
  console.log(`Total Withdrawn:    ${stats.totalWithdrawn} lamports`);
  console.log(`Active Accounts:    ${stats.activeAccounts}`);
  console.log(`Shielded Notes:     ${stats.shieldedCommitments}\n`);

  // 5. Recent batches
  console.log('--- Recent Batches ---');
  const { batches } = await api.listBatches({ limit: 5 });
  if (batches.length === 0) {
    console.log('No batches yet.\n');
  } else {
    for (const batch of batches) {
      console.log(`  Batch ${batch.batchId}: ${batch.txCount} txs, status=${batch.status}`);
    }
    console.log();
  }

  // 6. Recent transactions
  console.log('--- Recent Transactions ---');
  const { transactions } = await api.listTransactions({ limit: 10 });
  if (transactions.length === 0) {
    console.log('No transactions yet.\n');
  } else {
    for (const tx of transactions) {
      console.log(`  ${tx.txHash.slice(0, 16)}... type=${tx.txType} status=${tx.status}`);
    }
    console.log();
  }

  // 7. Committee info (threshold encryption)
  console.log('--- Committee Info ---');
  try {
    const committee = await api.getCommittee();
    console.log(`Enabled:   ${committee.enabled}`);
    console.log(`Threshold: ${committee.threshold}/${committee.totalMembers}`);
    console.log(`Epoch:     ${committee.epoch}`);
    console.log(`Pending:   ${committee.pendingCount} encrypted txs\n`);
  } catch {
    console.log('Committee not configured.\n');
  }

  console.log('=== Example Complete ===');
}

main().catch(console.error);
