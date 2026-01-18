/**
 * Tests for Zelana SDK Shielded Transactions
 */

import { describe, it, expect } from 'bun:test';
import {
  generateShieldedKeys,
  shieldedKeysFromSpendingKey,
  createNote,
  noteWithRandomness,
  computeCommitment,
  computeNullifier,
  ShieldedTransactionBuilder,
} from '../src/shielded';
import { bytesToHex, randomBytes } from '../src/utils';

describe('Shielded Keys', () => {
  it('should generate random shielded keys', () => {
    const keys1 = generateShieldedKeys();
    const keys2 = generateShieldedKeys();

    expect(keys1.spendingKey.length).toBe(32);
    expect(keys1.viewingKey.length).toBe(32);
    expect(keys1.publicKey.length).toBe(32);

    // Should be different
    expect(bytesToHex(keys1.spendingKey)).not.toBe(bytesToHex(keys2.spendingKey));
  });

  it('should derive same keys from same spending key', () => {
    const spendingKey = randomBytes(32);
    
    const keys1 = shieldedKeysFromSpendingKey(spendingKey);
    const keys2 = shieldedKeysFromSpendingKey(spendingKey);

    expect(bytesToHex(keys1.viewingKey)).toBe(bytesToHex(keys2.viewingKey));
    expect(bytesToHex(keys1.publicKey)).toBe(bytesToHex(keys2.publicKey));
  });

  it('should derive different public key from different spending keys', () => {
    const keys1 = generateShieldedKeys();
    const keys2 = generateShieldedKeys();

    expect(bytesToHex(keys1.publicKey)).not.toBe(bytesToHex(keys2.publicKey));
  });
});

describe('Notes', () => {
  it('should create note with random blinding', () => {
    const ownerPk = randomBytes(32);
    const note = createNote(1000n, ownerPk);

    expect(note.value).toBe(1000n);
    expect(note.randomness.length).toBe(32);
    expect(note.ownerPk).toEqual(ownerPk);
    expect(note.position).toBeUndefined();
  });

  it('should create note with explicit randomness', () => {
    const ownerPk = randomBytes(32);
    const randomness = randomBytes(32);
    const note = noteWithRandomness(1000n, ownerPk, randomness, 42n);

    expect(note.value).toBe(1000n);
    expect(note.randomness).toEqual(randomness);
    expect(note.position).toBe(42n);
  });
});

describe('Commitments', () => {
  it('should compute deterministic commitment', () => {
    const ownerPk = new Uint8Array(32).fill(1);
    const randomness = new Uint8Array(32).fill(2);
    
    const note = noteWithRandomness(1000n, ownerPk, randomness);
    
    const c1 = computeCommitment(note);
    const c2 = computeCommitment(note);

    expect(bytesToHex(c1)).toBe(bytesToHex(c2));
  });

  it('should compute different commitment for different values', () => {
    const ownerPk = new Uint8Array(32).fill(1);
    const randomness = new Uint8Array(32).fill(2);
    
    const note1 = noteWithRandomness(1000n, ownerPk, randomness);
    const note2 = noteWithRandomness(2000n, ownerPk, randomness);

    const c1 = computeCommitment(note1);
    const c2 = computeCommitment(note2);

    expect(bytesToHex(c1)).not.toBe(bytesToHex(c2));
  });

  it('should compute different commitment for different randomness', () => {
    const ownerPk = new Uint8Array(32).fill(1);
    
    const note1 = noteWithRandomness(1000n, ownerPk, new Uint8Array(32).fill(1));
    const note2 = noteWithRandomness(1000n, ownerPk, new Uint8Array(32).fill(2));

    const c1 = computeCommitment(note1);
    const c2 = computeCommitment(note2);

    expect(bytesToHex(c1)).not.toBe(bytesToHex(c2));
  });
});

describe('Nullifiers', () => {
  it('should require position to compute nullifier', () => {
    const keys = generateShieldedKeys();
    const note = createNote(1000n, keys.publicKey);
    
    // No position -> null
    const nullifier = computeNullifier(note, keys.spendingKey);
    expect(nullifier).toBeNull();
  });

  it('should compute nullifier with position', () => {
    const keys = generateShieldedKeys();
    const note = createNote(1000n, keys.publicKey);
    note.position = 42n;
    
    const nullifier = computeNullifier(note, keys.spendingKey);
    expect(nullifier).not.toBeNull();
    expect(nullifier!.length).toBe(32);
  });

  it('should compute different nullifier for different positions', () => {
    const keys = generateShieldedKeys();
    const randomness = randomBytes(32);
    
    const note1 = noteWithRandomness(1000n, keys.publicKey, randomness, 1n);
    const note2 = noteWithRandomness(1000n, keys.publicKey, randomness, 2n);

    const n1 = computeNullifier(note1, keys.spendingKey);
    const n2 = computeNullifier(note2, keys.spendingKey);

    expect(bytesToHex(n1!)).not.toBe(bytesToHex(n2!));
  });

  it('should compute same nullifier for same note', () => {
    const keys = generateShieldedKeys();
    const randomness = randomBytes(32);
    const note = noteWithRandomness(1000n, keys.publicKey, randomness, 42n);

    const n1 = computeNullifier(note, keys.spendingKey);
    const n2 = computeNullifier(note, keys.spendingKey);

    expect(bytesToHex(n1!)).toBe(bytesToHex(n2!));
  });
});

describe('ShieldedTransactionBuilder', () => {
  it('should validate no inputs', () => {
    const builder = new ShieldedTransactionBuilder();
    
    const result = builder.validate();
    expect(result.valid).toBe(false);
    expect(result.error).toBe('No inputs');
  });

  it('should validate no outputs', () => {
    const keys = generateShieldedKeys();
    const note = createNote(1000n, keys.publicKey);
    note.position = 1n;

    const builder = new ShieldedTransactionBuilder();
    builder.addInput({
      note,
      merklePath: { siblings: [], indices: [] },
      spendingKey: keys.spendingKey,
    });

    const result = builder.validate();
    expect(result.valid).toBe(false);
    expect(result.error).toBe('No outputs');
  });

  it('should validate merkle root not set', () => {
    const keys = generateShieldedKeys();
    const recipient = generateShieldedKeys();
    const note = createNote(1000n, keys.publicKey);
    note.position = 1n;

    const builder = new ShieldedTransactionBuilder();
    builder.addInput({
      note,
      merklePath: { siblings: [], indices: [] },
      spendingKey: keys.spendingKey,
    });
    builder.addOutput({
      recipientPk: recipient.publicKey,
      value: 1000n,
    });

    const result = builder.validate();
    expect(result.valid).toBe(false);
    expect(result.error).toBe('Merkle root not set');
  });

  it('should validate balance mismatch', () => {
    const keys = generateShieldedKeys();
    const recipient = generateShieldedKeys();
    const note = createNote(1000n, keys.publicKey);
    note.position = 1n;

    const builder = new ShieldedTransactionBuilder();
    builder.addInput({
      note,
      merklePath: { siblings: [], indices: [] },
      spendingKey: keys.spendingKey,
    });
    builder.addOutput({
      recipientPk: recipient.publicKey,
      value: 500n, // Mismatch!
    });
    builder.setMerkleRoot(randomBytes(32));

    const result = builder.validate();
    expect(result.valid).toBe(false);
    expect(result.error).toContain('Balance mismatch');
  });

  it('should validate balanced transaction', () => {
    const keys = generateShieldedKeys();
    const recipient = generateShieldedKeys();
    const note = createNote(1000n, keys.publicKey);
    note.position = 1n;

    const builder = new ShieldedTransactionBuilder();
    builder.addInput({
      note,
      merklePath: { siblings: [], indices: [] },
      spendingKey: keys.spendingKey,
    });
    builder.addOutput({
      recipientPk: recipient.publicKey,
      value: 1000n,
    });
    builder.setMerkleRoot(randomBytes(32));

    const result = builder.validate();
    expect(result.valid).toBe(true);
  });

  it('should prepare transaction', () => {
    const keys = generateShieldedKeys();
    const recipient = generateShieldedKeys();
    const note = createNote(1000n, keys.publicKey);
    note.position = 1n;

    const builder = new ShieldedTransactionBuilder();
    builder.addInput({
      note,
      merklePath: { siblings: [], indices: [] },
      spendingKey: keys.spendingKey,
    });
    builder.addOutput({
      recipientPk: recipient.publicKey,
      value: 700n,
    });
    builder.addOutput({
      recipientPk: keys.publicKey, // Change back to self
      value: 300n,
    });
    builder.setMerkleRoot(randomBytes(32));

    const prepared = builder.prepare();

    expect(prepared.nullifiers.length).toBe(1);
    expect(prepared.commitments.length).toBe(2);
    expect(prepared.encryptedOutputs.length).toBe(2);
    expect(prepared.witness.inputs.length).toBe(1);
    expect(prepared.witness.outputs.length).toBe(2);
  });

  it('should clear builder for reuse', () => {
    const keys = generateShieldedKeys();
    const note = createNote(1000n, keys.publicKey);
    note.position = 1n;

    const builder = new ShieldedTransactionBuilder();
    builder.addInput({
      note,
      merklePath: { siblings: [], indices: [] },
      spendingKey: keys.spendingKey,
    });

    builder.clear();

    const result = builder.validate();
    expect(result.error).toBe('No inputs');
  });
});
