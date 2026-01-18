/**
 * Example: Shielded (Private) Transaction
 * 
 * This example demonstrates how to:
 * 1. Generate shielded keys
 * 2. Create notes
 * 3. Build a shielded transaction
 * 4. Prepare for ZK proving
 * 
 * Run with: bun run examples/shielded-tx.ts
 * 
 * Note: This example shows the client-side preparation.
 * Actual ZK proof generation requires a prover (WASM or native).
 */

import {
  generateShieldedKeys,
  createNote,
  computeCommitment,
  computeNullifier,
  ShieldedTransactionBuilder,
  bytesToHex,
  randomBytes,
} from '../src';

async function main() {
  console.log('=== Zelana SDK: Shielded Transaction Example ===\n');

  // 1. Generate shielded keys for sender and recipient
  console.log('Generating shielded keys...');
  const sender = generateShieldedKeys();
  const recipient = generateShieldedKeys();

  console.log(`Sender public key:    ${bytesToHex(sender.publicKey).slice(0, 32)}...`);
  console.log(`Recipient public key: ${bytesToHex(recipient.publicKey).slice(0, 32)}...`);
  console.log();

  // 2. Create a note (simulating a note the sender owns)
  console.log('Creating input note...');
  const inputNote = createNote(1_000_000n, sender.publicKey);
  inputNote.position = 42n; // Simulate it's already in the tree at position 42

  const inputCommitment = computeCommitment(inputNote);
  console.log(`Input note value:      ${inputNote.value} lamports`);
  console.log(`Input note commitment: ${bytesToHex(inputCommitment).slice(0, 32)}...`);
  console.log(`Input note position:   ${inputNote.position}`);
  console.log();

  // 3. Compute nullifier (to mark this note as spent)
  const nullifier = computeNullifier(inputNote, sender.spendingKey);
  console.log(`Nullifier: ${bytesToHex(nullifier!).slice(0, 32)}...`);
  console.log();

  // 4. Build shielded transaction
  console.log('Building shielded transaction...');
  const builder = new ShieldedTransactionBuilder();

  // Add input (note we're spending)
  builder.addInput({
    note: inputNote,
    merklePath: {
      siblings: [randomBytes(32), randomBytes(32), randomBytes(32)], // Mock path
      indices: [false, true, false],
    },
    spendingKey: sender.spendingKey,
  });

  // Add outputs
  // - 700,000 to recipient
  // - 300,000 back to sender (change)
  builder.addOutput({
    recipientPk: recipient.publicKey,
    value: 700_000n,
    memo: new TextEncoder().encode('Payment for services'),
  });

  builder.addOutput({
    recipientPk: sender.publicKey,
    value: 300_000n,
  });

  // Set the merkle root we're referencing
  builder.setMerkleRoot(randomBytes(32)); // Would come from sequencer

  // 5. Validate
  const validation = builder.validate();
  console.log(`Validation: ${validation.valid ? 'PASSED' : 'FAILED'}`);
  if (!validation.valid) {
    console.log(`  Error: ${validation.error}`);
    process.exit(1);
  }
  console.log();

  // 6. Prepare for proving
  console.log('Preparing transaction for proving...');
  const prepared = builder.prepare();

  console.log(`Nullifiers:         ${prepared.nullifiers.length}`);
  console.log(`Commitments:        ${prepared.commitments.length}`);
  console.log(`Encrypted outputs:  ${prepared.encryptedOutputs.length}`);
  console.log();

  // Show outputs
  console.log('--- Output Commitments ---');
  for (let i = 0; i < prepared.commitments.length; i++) {
    console.log(`  Output ${i}: ${bytesToHex(prepared.commitments[i]).slice(0, 32)}...`);
  }
  console.log();

  console.log('--- Encrypted Notes ---');
  for (let i = 0; i < prepared.encryptedOutputs.length; i++) {
    const enc = prepared.encryptedOutputs[i];
    console.log(`  Output ${i}:`);
    console.log(`    Ephemeral PK: ${bytesToHex(enc.ephemeralPk).slice(0, 32)}...`);
    console.log(`    Ciphertext:   ${enc.ciphertext.length} bytes`);
  }
  console.log();

  // 7. Show witness structure (for ZK prover)
  console.log('--- Witness Data (for ZK Prover) ---');
  console.log(`  Input notes:  ${prepared.witness.inputs.length}`);
  console.log(`  Output notes: ${prepared.witness.outputs.length}`);
  console.log();

  console.log('At this point, the prepared transaction would be sent to a');
  console.log('ZK prover to generate a Groth16 proof. The proof, along with');
  console.log('the nullifiers, commitments, and encrypted outputs, would then');
  console.log('be submitted to the sequencer via the /shielded/submit endpoint.');

  console.log('\n=== Example Complete ===');
}

main().catch(console.error);
