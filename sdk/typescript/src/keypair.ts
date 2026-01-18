/**
 * Zelana Keypair - Ed25519 key management and signing
 * 
 * Provides key generation, import/export, and transaction signing.
 */

import * as ed from '@noble/ed25519';
import { sha512 } from '@noble/hashes/sha512';
import { 
  bytesToHex, 
  hexToBytes, 
  bytesToBase58, 
  base58ToBytes,
  u64ToLeBytes,
  concatBytes,
  randomBytes
} from './utils';
import type { Bytes32, TransferRequest, WithdrawRequest } from './types';

// Configure ed25519 to use sha512
ed.etc.sha512Sync = (...m) => sha512(ed.etc.concatBytes(...m));

/**
 * Zelana Keypair for L2 transactions
 * 
 * Wraps an Ed25519 keypair and provides signing methods for
 * transfers, withdrawals, and other L2 operations.
 */
export class Keypair {
  private readonly secretKey: Uint8Array; // 32-byte seed
  private readonly _publicKey: Uint8Array; // 32-byte public key

  private constructor(secretKey: Uint8Array, publicKey: Uint8Array) {
    this.secretKey = secretKey;
    this._publicKey = publicKey;
  }

  /**
   * Generate a new random keypair
   */
  static generate(): Keypair {
    const secretKey = randomBytes(32);
    const publicKey = ed.getPublicKey(secretKey);
    return new Keypair(secretKey, publicKey);
  }

  /**
   * Create keypair from a 32-byte secret key (seed)
   */
  static fromSecretKey(secretKey: Uint8Array): Keypair {
    if (secretKey.length !== 32) {
      throw new Error('Secret key must be 32 bytes');
    }
    const publicKey = ed.getPublicKey(secretKey);
    return new Keypair(new Uint8Array(secretKey), publicKey);
  }

  /**
   * Create keypair from hex-encoded secret key
   */
  static fromHex(hex: string): Keypair {
    return Keypair.fromSecretKey(hexToBytes(hex));
  }

  /**
   * Create keypair from base58-encoded secret key
   */
  static fromBase58(base58: string): Keypair {
    return Keypair.fromSecretKey(base58ToBytes(base58));
  }

  /**
   * Get the public key as Uint8Array
   */
  get publicKey(): Bytes32 {
    return new Uint8Array(this._publicKey);
  }

  /**
   * Get the public key as hex string
   */
  get publicKeyHex(): string {
    return bytesToHex(this._publicKey);
  }

  /**
   * Get the public key as base58 string (Solana format)
   */
  get publicKeyBase58(): string {
    return bytesToBase58(this._publicKey);
  }

  /**
   * Get the secret key as hex string (be careful with this!)
   */
  get secretKeyHex(): string {
    return bytesToHex(this.secretKey);
  }

  /**
   * Get the secret key as base58 string
   */
  get secretKeyBase58(): string {
    return bytesToBase58(this.secretKey);
  }

  /**
   * Sign arbitrary message bytes
   */
  sign(message: Uint8Array): Uint8Array {
    return ed.sign(message, this.secretKey);
  }

  /**
   * Verify a signature (static method)
   */
  static verify(signature: Uint8Array, message: Uint8Array, publicKey: Uint8Array): boolean {
    try {
      return ed.verify(signature, message, publicKey);
    } catch {
      return false;
    }
  }

  /**
   * Sign a transfer transaction
   * 
   * The message format matches the Rust TransactionData wincode serialization:
   * - from: [u8; 32]
   * - to: [u8; 32]
   * - amount: u64 (little-endian)
   * - nonce: u64 (little-endian)
   * - chain_id: u64 (little-endian)
   */
  signTransfer(
    to: Bytes32,
    amount: bigint,
    nonce: bigint,
    chainId: bigint = BigInt(1)
  ): TransferRequest {
    // Build the message (wincode serialization of TransactionData)
    const message = concatBytes(
      this._publicKey,        // from
      to,                     // to
      u64ToLeBytes(amount),   // amount
      u64ToLeBytes(nonce),    // nonce
      u64ToLeBytes(chainId)   // chain_id
    );

    const signature = this.sign(message);

    return {
      from: this.publicKey,
      to: new Uint8Array(to),
      amount,
      nonce,
      chainId,
      signature,
      signerPubkey: this.publicKey
    };
  }

  /**
   * Sign a withdrawal request
   * 
   * The message format matches the Rust withdrawal verification:
   * - from: [u8; 32]
   * - to_l1_address: [u8; 32]
   * - amount: u64 (little-endian)
   * - nonce: u64 (little-endian)
   */
  signWithdrawal(
    toL1Address: Bytes32,
    amount: bigint,
    nonce: bigint
  ): WithdrawRequest {
    // Build the message
    const message = concatBytes(
      this._publicKey,          // from
      toL1Address,              // to_l1_address
      u64ToLeBytes(amount),     // amount
      u64ToLeBytes(nonce)       // nonce
    );

    const signature = this.sign(message);

    return {
      from: this.publicKey,
      toL1Address: new Uint8Array(toL1Address),
      amount,
      nonce,
      signature,
      signerPubkey: this.publicKey
    };
  }
}

/**
 * PublicKey wrapper for address representation
 */
export class PublicKey {
  private readonly bytes: Uint8Array;

  constructor(input: Uint8Array | string) {
    if (typeof input === 'string') {
      // Try base58 first, then hex
      try {
        const decoded = base58ToBytes(input);
        if (decoded.length === 32) {
          this.bytes = decoded;
          return;
        }
      } catch {
        // Not base58, try hex
      }
      this.bytes = hexToBytes(input);
    } else {
      this.bytes = new Uint8Array(input);
    }

    if (this.bytes.length !== 32) {
      throw new Error('Public key must be 32 bytes');
    }
  }

  /**
   * Get bytes representation
   */
  toBytes(): Bytes32 {
    return new Uint8Array(this.bytes);
  }

  /**
   * Get hex string representation
   */
  toHex(): string {
    return bytesToHex(this.bytes);
  }

  /**
   * Get base58 representation (Solana format)
   */
  toBase58(): string {
    return bytesToBase58(this.bytes);
  }

  /**
   * Default string representation is base58
   */
  toString(): string {
    return this.toBase58();
  }

  /**
   * Check equality with another public key
   */
  equals(other: PublicKey | Uint8Array): boolean {
    const otherBytes = other instanceof PublicKey ? other.bytes : other;
    if (this.bytes.length !== otherBytes.length) return false;
    for (let i = 0; i < this.bytes.length; i++) {
      if (this.bytes[i] !== otherBytes[i]) return false;
    }
    return true;
  }
}
