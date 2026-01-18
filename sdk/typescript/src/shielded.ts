/**
 * Shielded Transaction Support
 * 
 * Provides privacy-preserving transactions using note-based commitments.
 * 
 * Note: This is a client-side helper. The actual ZK proof generation
 * must be done by a separate prover (either WASM or native).
 */

import { sha512 } from '@noble/hashes/sha512';
import { bytesToHex, hexToBytes, concatBytes, u64ToLeBytes, randomBytes } from './utils';
import type { Bytes32 } from './types';

// ============================================================================
// Types
// ============================================================================

/**
 * A shielded note representing privately held value
 */
export interface Note {
  /** The value held in this note */
  value: bigint;
  /** Random blinding factor (32 bytes) */
  randomness: Bytes32;
  /** Owner's shielded public key (32 bytes) */
  ownerPk: Bytes32;
  /** Position in the commitment tree (set after insertion) */
  position?: bigint;
}

/**
 * Encrypted note data (sent on-chain)
 */
export interface EncryptedNote {
  /** Ephemeral public key for ECDH (32 bytes) */
  ephemeralPk: Bytes32;
  /** Nonce for ChaCha20-Poly1305 (12 bytes) */
  nonce: Uint8Array;
  /** Encrypted note data with authentication tag */
  ciphertext: Uint8Array;
}

/**
 * Merkle path for proving note membership
 */
export interface MerklePath {
  /** Sibling hashes from leaf to root */
  siblings: Bytes32[];
  /** Bit indicating left (0) or right (1) for each level */
  indices: boolean[];
}

/**
 * Shielded spending key bundle
 */
export interface ShieldedKeys {
  /** Spending key - allows spending notes (keep secret!) */
  spendingKey: Bytes32;
  /** Viewing key - allows viewing notes (for auditors) */
  viewingKey: Bytes32;
  /** Public key / address (can be shared) */
  publicKey: Bytes32;
}

/**
 * Input for a shielded transaction (spending a note)
 */
export interface ShieldedInput {
  /** The note being spent */
  note: Note;
  /** Merkle path proving note is in the tree */
  merklePath: MerklePath;
  /** Spending key for this note */
  spendingKey: Bytes32;
}

/**
 * Output for a shielded transaction (creating a note)
 */
export interface ShieldedOutput {
  /** Recipient's shielded public key */
  recipientPk: Bytes32;
  /** Amount to send */
  value: bigint;
  /** Optional memo (max 512 bytes) */
  memo?: Uint8Array;
}

/**
 * Prepared shielded transaction (ready for proving)
 */
export interface PreparedShieldedTx {
  /** Nullifiers for spent notes */
  nullifiers: Bytes32[];
  /** Commitments for new notes */
  commitments: Bytes32[];
  /** Encrypted outputs for recipients */
  encryptedOutputs: EncryptedNote[];
  /** Merkle root being referenced */
  merkleRoot: Bytes32;
  /** Witness data for ZK proof (internal) */
  witness: ShieldedWitness;
}

/**
 * Witness data for ZK proof generation
 */
export interface ShieldedWitness {
  inputs: Array<{
    note: Note;
    merklePath: MerklePath;
    spendingKey: Bytes32;
    nullifier: Bytes32;
  }>;
  outputs: Array<{
    note: Note;
    commitment: Bytes32;
  }>;
}

/**
 * Shielded transaction ready for submission
 */
export interface ShieldedTransaction {
  /** ZK proof bytes (Groth16) */
  proof: Uint8Array;
  /** Nullifiers (spent note identifiers) */
  nullifiers: Bytes32[];
  /** Commitments (new note identifiers) */
  commitments: Bytes32[];
  /** Encrypted notes for recipients */
  encryptedOutputs: EncryptedNote[];
}

// ============================================================================
// Key Generation
// ============================================================================

/**
 * Generate a new shielded key bundle
 * 
 * Warning: In production, use a proper key derivation from a master seed.
 */
export function generateShieldedKeys(): ShieldedKeys {
  const spendingKey = randomBytes(32);
  const viewingKey = deriveViewingKey(spendingKey);
  const publicKey = derivePublicKey(spendingKey);
  
  return { spendingKey, viewingKey, publicKey };
}

/**
 * Restore shielded keys from spending key
 */
export function shieldedKeysFromSpendingKey(spendingKey: Bytes32): ShieldedKeys {
  const viewingKey = deriveViewingKey(spendingKey);
  const publicKey = derivePublicKey(spendingKey);
  
  return { spendingKey: new Uint8Array(spendingKey), viewingKey, publicKey };
}

/**
 * Derive viewing key from spending key
 * Uses SHA-512 with domain separation (simplified - Rust uses Poseidon)
 */
function deriveViewingKey(spendingKey: Bytes32): Bytes32 {
  const domain = new TextEncoder().encode('ZelanaIVK');
  const hash = sha512(concatBytes(domain, spendingKey));
  return hash.slice(0, 32);
}

/**
 * Derive public key from spending key
 * Uses SHA-512 with domain separation (simplified - Rust uses Poseidon)
 */
function derivePublicKey(spendingKey: Bytes32): Bytes32 {
  const domain = new TextEncoder().encode('ZelanaPK');
  const hash = sha512(concatBytes(domain, spendingKey));
  return hash.slice(0, 32);
}

// ============================================================================
// Note Operations
// ============================================================================

/**
 * Create a new note with random blinding
 */
export function createNote(
  value: bigint,
  ownerPk: Bytes32,
  position?: bigint
): Note {
  return {
    value,
    randomness: randomBytes(32),
    ownerPk: new Uint8Array(ownerPk),
    position,
  };
}

/**
 * Create a note with explicit randomness (for testing/recovery)
 */
export function noteWithRandomness(
  value: bigint,
  ownerPk: Bytes32,
  randomness: Bytes32,
  position?: bigint
): Note {
  return {
    value,
    randomness: new Uint8Array(randomness),
    ownerPk: new Uint8Array(ownerPk),
    position,
  };
}

/**
 * Compute commitment for a note
 * 
 * commitment = Hash(value || randomness || owner_pk)
 * 
 * Note: Rust uses Poseidon hash - this uses SHA-512 for simplicity.
 * For production, this should match the circuit's hash function.
 */
export function computeCommitment(note: Note): Bytes32 {
  const domain = new TextEncoder().encode('ZelanaCommit');
  const message = concatBytes(
    domain,
    u64ToLeBytes(note.value),
    note.randomness,
    note.ownerPk
  );
  const hash = sha512(message);
  return hash.slice(0, 32);
}

/**
 * Derive nullifier for spending a note
 * 
 * nullifier = Hash(nk || commitment || position)
 * 
 * Requires the note to have a position set.
 */
export function computeNullifier(
  note: Note,
  spendingKey: Bytes32
): Bytes32 | null {
  if (note.position === undefined) {
    return null;
  }
  
  const nullifierKey = deriveNullifierKey(spendingKey);
  const commitment = computeCommitment(note);
  
  const domain = new TextEncoder().encode('ZelanaNullifier');
  const message = concatBytes(
    domain,
    nullifierKey,
    commitment,
    u64ToLeBytes(note.position)
  );
  
  const hash = sha512(message);
  return hash.slice(0, 32);
}

/**
 * Derive nullifier key from spending key
 */
function deriveNullifierKey(spendingKey: Bytes32): Bytes32 {
  const domain = new TextEncoder().encode('ZelanaNK');
  const hash = sha512(concatBytes(domain, spendingKey));
  return hash.slice(0, 32);
}

// ============================================================================
// Transaction Building
// ============================================================================

/**
 * ShieldedTransactionBuilder helps construct shielded transactions
 */
export class ShieldedTransactionBuilder {
  private inputs: ShieldedInput[] = [];
  private outputs: ShieldedOutput[] = [];
  private merkleRoot: Bytes32 | null = null;

  /**
   * Add an input (note to spend)
   */
  addInput(input: ShieldedInput): this {
    if (input.note.position === undefined) {
      throw new Error('Input note must have a position');
    }
    this.inputs.push(input);
    return this;
  }

  /**
   * Add an output (note to create)
   */
  addOutput(output: ShieldedOutput): this {
    this.outputs.push(output);
    return this;
  }

  /**
   * Set the merkle root being referenced
   */
  setMerkleRoot(root: Bytes32): this {
    this.merkleRoot = new Uint8Array(root);
    return this;
  }

  /**
   * Validate the transaction
   * - Sum of inputs must equal sum of outputs
   * - All inputs must have positions
   * - Merkle root must be set
   */
  validate(): { valid: boolean; error?: string } {
    if (this.inputs.length === 0) {
      return { valid: false, error: 'No inputs' };
    }
    
    if (this.outputs.length === 0) {
      return { valid: false, error: 'No outputs' };
    }
    
    if (!this.merkleRoot) {
      return { valid: false, error: 'Merkle root not set' };
    }

    // Check balance
    const inputSum = this.inputs.reduce((sum, i) => sum + i.note.value, 0n);
    const outputSum = this.outputs.reduce((sum, o) => sum + o.value, 0n);
    
    if (inputSum !== outputSum) {
      return { 
        valid: false, 
        error: `Balance mismatch: inputs=${inputSum}, outputs=${outputSum}` 
      };
    }

    return { valid: true };
  }

  /**
   * Prepare the transaction for proving
   * 
   * Returns all the data needed to generate a ZK proof.
   */
  prepare(): PreparedShieldedTx {
    const validation = this.validate();
    if (!validation.valid) {
      throw new Error(`Invalid transaction: ${validation.error}`);
    }

    // Compute nullifiers for inputs
    const inputsWithNullifiers = this.inputs.map((input) => {
      const nullifier = computeNullifier(input.note, input.spendingKey);
      if (!nullifier) {
        throw new Error('Failed to compute nullifier');
      }
      return {
        note: input.note,
        merklePath: input.merklePath,
        spendingKey: input.spendingKey,
        nullifier,
      };
    });

    // Create output notes and compute commitments
    const outputsWithCommitments = this.outputs.map((output) => {
      const note = createNote(output.value, output.recipientPk);
      const commitment = computeCommitment(note);
      return { note, commitment, memo: output.memo };
    });

    // Encrypt outputs for recipients
    const encryptedOutputs = outputsWithCommitments.map((o) => 
      encryptNote(o.note, o.memo)
    );

    return {
      nullifiers: inputsWithNullifiers.map((i) => i.nullifier),
      commitments: outputsWithCommitments.map((o) => o.commitment),
      encryptedOutputs,
      merkleRoot: this.merkleRoot!,
      witness: {
        inputs: inputsWithNullifiers,
        outputs: outputsWithCommitments.map((o) => ({
          note: o.note,
          commitment: o.commitment,
        })),
      },
    };
  }

  /**
   * Clear the builder for reuse
   */
  clear(): this {
    this.inputs = [];
    this.outputs = [];
    this.merkleRoot = null;
    return this;
  }
}

// ============================================================================
// Note Encryption (Simplified)
// ============================================================================

/**
 * Encrypt a note for the recipient
 * 
 * Note: This is a simplified version. Production should use:
 * - X25519 ECDH for key agreement
 * - ChaCha20-Poly1305 for authenticated encryption
 * 
 * For now, we use a deterministic encryption based on the note data,
 * which is suitable for the MVP but should be replaced with proper
 * ECIES encryption.
 */
function encryptNote(note: Note, memo?: Uint8Array): EncryptedNote {
  // Generate ephemeral "keypair" (simplified - just random bytes)
  const ephemeralPk = randomBytes(32);
  const nonce = randomBytes(12);
  
  // Serialize note data
  const plaintext = concatBytes(
    u64ToLeBytes(note.value),
    note.randomness,
    memo ? new Uint8Array([...u16ToLeBytes(memo.length), ...memo]) : new Uint8Array([0, 0])
  );
  
  // "Encrypt" with XOR (NOT SECURE - placeholder for real encryption)
  // In production, use X25519 + ChaCha20-Poly1305
  const key = sha512(concatBytes(ephemeralPk, note.ownerPk)).slice(0, 32);
  const ciphertext = xorEncrypt(plaintext, key, nonce);
  
  return {
    ephemeralPk,
    nonce,
    ciphertext,
  };
}

/**
 * Simple XOR encryption (NOT SECURE - placeholder)
 */
function xorEncrypt(data: Uint8Array, key: Uint8Array, nonce: Uint8Array): Uint8Array {
  const stream = sha512(concatBytes(key, nonce));
  const result = new Uint8Array(data.length);
  for (let i = 0; i < data.length; i++) {
    result[i] = data[i] ^ stream[i % stream.length];
  }
  return result;
}

function u16ToLeBytes(value: number): Uint8Array {
  return new Uint8Array([value & 0xff, (value >> 8) & 0xff]);
}

// ============================================================================
// Note Scanning
// ============================================================================

/**
 * Scan encrypted notes for ones belonging to a viewing key
 * 
 * This is used by wallets to find owned notes.
 */
export interface ScanResult {
  /** Position in the commitment tree */
  position: bigint;
  /** Decrypted note */
  note: Note;
  /** Commitment (for verification) */
  commitment: Bytes32;
  /** Decrypted memo */
  memo?: Uint8Array;
}

/**
 * Try to decrypt an encrypted note
 * 
 * Returns the note if decryption succeeds, null otherwise.
 * 
 * Note: This is a placeholder - real implementation needs the
 * recipient's X25519 secret key for ECDH.
 */
export function tryDecryptNote(
  encrypted: EncryptedNote,
  viewingKey: Bytes32,
  ownerPk: Bytes32,
  position: bigint
): Note | null {
  // Placeholder - real decryption would use X25519 ECDH
  // For now, we can't actually decrypt without the private key
  // This function would be called by the API's scan endpoint
  return null;
}

// ============================================================================
// Exports
// ============================================================================

export const shielded = {
  generateKeys: generateShieldedKeys,
  keysFromSpendingKey: shieldedKeysFromSpendingKey,
  createNote,
  noteWithRandomness,
  computeCommitment,
  computeNullifier,
  TransactionBuilder: ShieldedTransactionBuilder,
};
