/**
 * Note Encryption
 *
 * Encrypts note data for the recipient using ECDH + ChaCha20-Poly1305.
 * This implementation matches the Rust privacy SDK exactly.
 *
 * Flow:
 * 1. Sender generates ephemeral X25519 keypair (epk, esk)
 * 2. Shared secret = ECDH(esk, recipient_pk)
 * 3. Encryption key = HKDF(shared_secret, "zelana-note-v1")
 * 4. Ciphertext = ChaCha20-Poly1305(key, nonce, plaintext)
 * 5. Output = (epk, nonce, ciphertext)
 */

import { x25519 } from '@noble/curves/ed25519.js';
import { chacha20poly1305 } from '@noble/ciphers/chacha.js';
import { blake3 } from '@noble/hashes/blake3';
import { randomBytes as nobleRandomBytes } from '@noble/hashes/utils';
import type { Bytes32 } from './types.js';

/**
 * Encrypted note structure (matches Rust EncryptedNote)
 */
export interface EncryptedNote {
  /** Ephemeral public key for ECDH (32 bytes) */
  ephemeralPk: Uint8Array;
  /** Nonce for ChaCha20-Poly1305 (12 bytes) */
  nonce: Uint8Array;
  /** Encrypted note data with authentication tag */
  ciphertext: Uint8Array;
}

/**
 * Note plaintext structure
 */
interface NotePlaintext {
  /** Note value in lamports */
  value: bigint;
  /** Random blinding factor (32 bytes) */
  randomness: Uint8Array;
  /** Optional memo (max 512 bytes) */
  memo?: Uint8Array;
}

/**
 * Derive encryption key from shared secret using BLAKE3
 * Matches Rust: blake3::Hasher::new_derive_key("zelana-note-v1")
 */
function deriveNoteKey(sharedSecret: Uint8Array, ephemeralPk: Uint8Array): Uint8Array {
  // BLAKE3 key derivation with domain separator
  const context = new TextEncoder().encode('zelana-note-v1');
  
  // Create a keyed hash using the context as a domain separator
  // BLAKE3's derive_key mode is: BLAKE3(context || 0x01 || input)
  // We use a simpler approach that matches the Rust behavior:
  // derive_key(context, input) where input = shared_secret || ephemeral_pk
  const input = new Uint8Array(sharedSecret.length + ephemeralPk.length);
  input.set(sharedSecret, 0);
  input.set(ephemeralPk, sharedSecret.length);
  
  // Use BLAKE3 derive_key mode
  return blake3(input, { context });
}

/**
 * Serialize plaintext for encryption
 * Format: value (8 bytes LE) + randomness (32 bytes) + memo_len (2 bytes LE) + memo
 */
function serializePlaintext(pt: NotePlaintext): Uint8Array {
  const memoLen = pt.memo?.length ?? 0;
  const bytes = new Uint8Array(8 + 32 + 2 + memoLen);
  
  // Value (8 bytes, little-endian)
  const view = new DataView(bytes.buffer);
  view.setBigUint64(0, pt.value, true);
  
  // Randomness (32 bytes)
  bytes.set(pt.randomness, 8);
  
  // Memo length (2 bytes, little-endian)
  view.setUint16(40, memoLen, true);
  
  // Memo
  if (pt.memo && memoLen > 0) {
    bytes.set(pt.memo.slice(0, 512), 42);
  }
  
  return bytes;
}

/**
 * Deserialize plaintext after decryption
 */
function deserializePlaintext(bytes: Uint8Array): NotePlaintext | null {
  if (bytes.length < 42) {
    return null; // 8 + 32 + 2 minimum
  }
  
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  
  // Value (8 bytes, little-endian)
  const value = view.getBigUint64(0, true);
  
  // Randomness (32 bytes)
  const randomness = bytes.slice(8, 40);
  
  // Memo length (2 bytes)
  const memoLen = view.getUint16(40, true);
  
  if (bytes.length < 42 + memoLen) {
    return null;
  }
  
  // Memo
  const memo = memoLen > 0 ? bytes.slice(42, 42 + memoLen) : undefined;
  
  return { value, randomness, memo };
}

/**
 * Encrypt a note for a recipient
 *
 * @param value - Note value in lamports
 * @param randomness - Random blinding factor (32 bytes)
 * @param recipientPk - Recipient's X25519 public key
 * @param memo - Optional memo (max 512 bytes)
 * @returns Encrypted note
 */
export function encryptNote(
  value: bigint,
  randomness: Bytes32,
  recipientPk: Bytes32,
  memo?: Uint8Array
): EncryptedNote {
  // Generate ephemeral keypair
  const ephemeralSk = nobleRandomBytes(32);
  const ephemeralPk = x25519.getPublicKey(ephemeralSk);
  
  // ECDH shared secret
  const sharedSecret = x25519.getSharedSecret(ephemeralSk, recipientPk);
  
  // Derive encryption key
  const encryptionKey = deriveNoteKey(sharedSecret, ephemeralPk);
  
  // Create plaintext
  const plaintext = serializePlaintext({
    value,
    randomness: new Uint8Array(randomness),
    memo: memo?.slice(0, 512),
  });
  
  // Generate random nonce (12 bytes)
  const nonce = nobleRandomBytes(12);
  
  // Encrypt with ChaCha20-Poly1305
  const cipher = chacha20poly1305(encryptionKey, nonce);
  const ciphertext = cipher.encrypt(plaintext);
  
  return {
    ephemeralPk,
    nonce,
    ciphertext,
  };
}

/**
 * Decrypt a note using recipient's secret key
 *
 * @param encrypted - The encrypted note
 * @param recipientSk - Recipient's X25519 secret key (32 bytes)
 * @returns Decrypted plaintext or null if decryption fails
 */
export function decryptNote(
  encrypted: EncryptedNote,
  recipientSk: Bytes32
): { value: bigint; randomness: Uint8Array; memo?: Uint8Array } | null {
  try {
    // ECDH shared secret
    const sharedSecret = x25519.getSharedSecret(recipientSk, encrypted.ephemeralPk);
    
    // Derive encryption key
    const encryptionKey = deriveNoteKey(sharedSecret, encrypted.ephemeralPk);
    
    // Decrypt with ChaCha20-Poly1305
    const cipher = chacha20poly1305(encryptionKey, encrypted.nonce);
    const plaintext = cipher.decrypt(encrypted.ciphertext);
    
    // Deserialize
    const pt = deserializePlaintext(plaintext);
    if (!pt) {
      return null;
    }
    
    return {
      value: pt.value,
      randomness: pt.randomness,
      memo: pt.memo,
    };
  } catch {
    // Decryption failed (wrong key, tampered ciphertext, etc.)
    return null;
  }
}

/**
 * Serialize an encrypted note to bytes for transmission
 * Format: ephemeral_pk (32) + nonce (12) + ciphertext (variable)
 */
export function serializeEncryptedNote(note: EncryptedNote): Uint8Array {
  const bytes = new Uint8Array(32 + 12 + note.ciphertext.length);
  bytes.set(note.ephemeralPk, 0);
  bytes.set(note.nonce, 32);
  bytes.set(note.ciphertext, 44);
  return bytes;
}

/**
 * Deserialize an encrypted note from bytes
 */
export function deserializeEncryptedNote(bytes: Uint8Array): EncryptedNote | null {
  if (bytes.length < 44 + 42 + 16) {
    // Minimum: 32 (epk) + 12 (nonce) + 42 (min plaintext) + 16 (tag)
    return null;
  }
  
  return {
    ephemeralPk: bytes.slice(0, 32),
    nonce: bytes.slice(32, 44),
    ciphertext: bytes.slice(44),
  };
}

/**
 * Generate an X25519 keypair
 */
export function generateX25519Keypair(): { secretKey: Uint8Array; publicKey: Uint8Array } {
  const secretKey = nobleRandomBytes(32);
  const publicKey = x25519.getPublicKey(secretKey);
  return { secretKey, publicKey };
}

/**
 * Derive X25519 public key from secret key
 */
export function x25519PublicKey(secretKey: Bytes32): Uint8Array {
  return x25519.getPublicKey(secretKey);
}
