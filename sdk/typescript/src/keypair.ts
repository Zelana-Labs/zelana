/**
 * Zelana Keypair - Ed25519 key management and signing
 * 
 * Provides key generation, import/export, and transaction signing.
 * 
 * Uses human-readable text messages for signing to match the wallet-signer
 * format and work correctly with the Rust sequencer.
 */

import * as ed from '@noble/ed25519';
import { sha512 } from '@noble/hashes/sha512';
import { 
  bytesToHex, 
  hexToBytes, 
  bytesToBase58, 
  base58ToBytes,
  randomBytes
} from './utils';
import type { Bytes32, TransferRequest, WithdrawRequest } from './types';

// Human-Readable Message Builders

/**
 * Build a human-readable transfer message for signing.
 * 
 * This format:
 * 1. Is obviously NOT a Solana transaction (prevents Phantom blocking)
 * 2. Users can read what they're signing (good UX)
 * 3. Similar to EIP-712 on Ethereum
 * 
 * IMPORTANT: The Rust sequencer must build the EXACT same message to verify.
 */
function buildTransferMessage(
  from: Uint8Array,
  to: Uint8Array,
  amount: bigint,
  nonce: bigint,
  chainId: bigint
): string {
  return `Zelana L2 Transfer

From: ${bytesToHex(from)}
To: ${bytesToHex(to)}
Amount: ${amount.toString()} lamports
Nonce: ${nonce.toString()}
Chain ID: ${chainId.toString()}

Sign to authorize this L2 transfer.`;
}

/**
 * Build a human-readable withdrawal message for signing.
 */
function buildWithdrawMessage(
  from: Uint8Array,
  toL1Address: Uint8Array,
  amount: bigint,
  nonce: bigint
): string {
  return `Zelana L2 Withdrawal

From: ${bytesToHex(from)}
To L1: ${bytesToBase58(toL1Address)}
Amount: ${amount.toString()} lamports
Nonce: ${nonce.toString()}

Sign to authorize this withdrawal to Solana L1.`;
}

// Signer Interface

/**
 * Signer interface for L2 transactions.
 * 
 * This allows using different signing mechanisms:
 * - Keypair (local Ed25519 key)
 * - WalletSigner (external wallet like Solana wallet)
 */
export interface Signer {
  /** The public key as 32 bytes */
  readonly publicKey: Bytes32;
  /** The public key as hex string */
  readonly publicKeyHex: string;
  /** The public key as base58 (Solana format) */
  readonly publicKeyBase58: string;
  /** Sign a message and return the 64-byte signature */
  sign(message: Uint8Array): Promise<Uint8Array>;
  /** Sign a transfer and return the signed request */
  signTransfer(to: Bytes32, amount: bigint, nonce: bigint, chainId?: bigint): Promise<TransferRequest>;
  /** Sign a withdrawal and return the signed request */
  signWithdrawal(toL1Address: Bytes32, amount: bigint, nonce: bigint): Promise<WithdrawRequest>;
}

// Configure ed25519 to use sha512
ed.etc.sha512Sync = (...m) => sha512(ed.etc.concatBytes(...m));

/**
 * Zelana Keypair for L2 transactions
 * 
 * Wraps an Ed25519 keypair and provides signing methods for
 * transfers, withdrawals, and other L2 operations.
 * 
 * Implements the Signer interface.
 */
export class Keypair implements Signer {
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
  sign(message: Uint8Array): Promise<Uint8Array> {
    return Promise.resolve(ed.sign(message, this.secretKey));
  }

  /**
   * Sign synchronously (for internal use or when async is not needed)
   */
  signSync(message: Uint8Array): Uint8Array {
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
   * Sign a transfer transaction using human-readable text format.
   * 
   * The message is a human-readable text string, which:
   * 1. Works with Phantom/Privy (not blocked as Solana tx)
   * 2. Matches the format verified by the Rust sequencer
   */
  async signTransfer(
    to: Bytes32,
    amount: bigint,
    nonce: bigint,
    chainId: bigint = BigInt(1)
  ): Promise<TransferRequest> {
    // Build human-readable message
    const messageText = buildTransferMessage(
      this._publicKey,
      to,
      amount,
      nonce,
      chainId
    );

    // Convert to UTF-8 bytes for signing
    const message = new TextEncoder().encode(messageText);
    const signature = await this.sign(message);

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
   * Sign a withdrawal request using human-readable text format.
   */
  async signWithdrawal(
    toL1Address: Bytes32,
    amount: bigint,
    nonce: bigint
  ): Promise<WithdrawRequest> {
    // Build human-readable message
    const messageText = buildWithdrawMessage(
      this._publicKey,
      toL1Address,
      amount,
      nonce
    );

    // Convert to UTF-8 bytes for signing
    const message = new TextEncoder().encode(messageText);
    const signature = await this.sign(message);

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
