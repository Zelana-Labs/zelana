/**
 * Utility functions for encoding, hashing, and byte manipulation
 */

// ============================================================================
// Hex Encoding
// ============================================================================

const HEX_CHARS = '0123456789abcdef';

/**
 * Convert bytes to hex string
 */
export function bytesToHex(bytes: Uint8Array): string {
  let hex = '';
  for (let i = 0; i < bytes.length; i++) {
    hex += HEX_CHARS[bytes[i] >> 4];
    hex += HEX_CHARS[bytes[i] & 0x0f];
  }
  return hex;
}

/**
 * Convert hex string to bytes
 */
export function hexToBytes(hex: string): Uint8Array {
  if (hex.startsWith('0x')) {
    hex = hex.slice(2);
  }
  if (hex.length % 2 !== 0) {
    throw new Error('Invalid hex string: odd length');
  }
  const bytes = new Uint8Array(hex.length / 2);
  for (let i = 0; i < bytes.length; i++) {
    const hi = parseInt(hex[i * 2], 16);
    const lo = parseInt(hex[i * 2 + 1], 16);
    if (isNaN(hi) || isNaN(lo)) {
      throw new Error(`Invalid hex character at position ${i * 2}`);
    }
    bytes[i] = (hi << 4) | lo;
  }
  return bytes;
}

// ============================================================================
// Base58 Encoding (Solana-compatible)
// ============================================================================

const BASE58_ALPHABET = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
const BASE58_MAP = new Map<string, number>();
for (let i = 0; i < BASE58_ALPHABET.length; i++) {
  BASE58_MAP.set(BASE58_ALPHABET[i], i);
}

/**
 * Encode bytes to base58 string
 */
export function bytesToBase58(bytes: Uint8Array): string {
  if (bytes.length === 0) return '';

  // Count leading zeros
  let zeros = 0;
  while (zeros < bytes.length && bytes[zeros] === 0) {
    zeros++;
  }

  // Allocate enough space
  const size = Math.floor((bytes.length - zeros) * 138 / 100) + 1;
  const b58 = new Uint8Array(size);

  // Convert to base58
  let length = 0;
  for (let i = zeros; i < bytes.length; i++) {
    let carry = bytes[i];
    let j = 0;
    for (let it = size - 1; (carry !== 0 || j < length) && it >= 0; it--, j++) {
      carry += 256 * b58[it];
      b58[it] = carry % 58;
      carry = Math.floor(carry / 58);
    }
    length = j;
  }

  // Skip leading zeros in base58 result
  let it = size - length;
  while (it < size && b58[it] === 0) {
    it++;
  }

  // Build string
  let str = '1'.repeat(zeros);
  while (it < size) {
    str += BASE58_ALPHABET[b58[it++]];
  }

  return str;
}

/**
 * Decode base58 string to bytes
 */
export function base58ToBytes(str: string): Uint8Array {
  if (str.length === 0) return new Uint8Array(0);

  // Count leading '1's (zeros in decoded form)
  let zeros = 0;
  while (zeros < str.length && str[zeros] === '1') {
    zeros++;
  }

  // Allocate enough space
  const size = Math.floor((str.length - zeros) * 733 / 1000) + 1;
  const b256 = new Uint8Array(size);

  // Convert from base58
  let length = 0;
  for (let i = zeros; i < str.length; i++) {
    const value = BASE58_MAP.get(str[i]);
    if (value === undefined) {
      throw new Error(`Invalid base58 character: ${str[i]}`);
    }
    let carry = value;
    let j = 0;
    for (let it = size - 1; (carry !== 0 || j < length) && it >= 0; it--, j++) {
      carry += 58 * b256[it];
      b256[it] = carry % 256;
      carry = Math.floor(carry / 256);
    }
    length = j;
  }

  // Skip leading zeros in base256 result
  let it = size - length;

  // Build result with leading zeros
  const result = new Uint8Array(zeros + (size - it));
  let idx = zeros;
  while (it < size) {
    result[idx++] = b256[it++];
  }

  return result;
}

// ============================================================================
// Little-endian encoding for wincode compatibility
// ============================================================================

/**
 * Encode u64 as little-endian bytes
 */
export function u64ToLeBytes(value: bigint): Uint8Array {
  const bytes = new Uint8Array(8);
  for (let i = 0; i < 8; i++) {
    bytes[i] = Number((value >> BigInt(i * 8)) & BigInt(0xff));
  }
  return bytes;
}

/**
 * Decode little-endian bytes to u64
 */
export function leBytesToU64(bytes: Uint8Array): bigint {
  if (bytes.length !== 8) {
    throw new Error('Expected 8 bytes for u64');
  }
  let value = BigInt(0);
  for (let i = 0; i < 8; i++) {
    value |= BigInt(bytes[i]) << BigInt(i * 8);
  }
  return value;
}

/**
 * Encode u32 as little-endian bytes
 */
export function u32ToLeBytes(value: number): Uint8Array {
  const bytes = new Uint8Array(4);
  bytes[0] = value & 0xff;
  bytes[1] = (value >> 8) & 0xff;
  bytes[2] = (value >> 16) & 0xff;
  bytes[3] = (value >> 24) & 0xff;
  return bytes;
}

// ============================================================================
// Byte array utilities
// ============================================================================

/**
 * Concatenate multiple Uint8Arrays
 */
export function concatBytes(...arrays: Uint8Array[]): Uint8Array {
  const totalLength = arrays.reduce((sum, arr) => sum + arr.length, 0);
  const result = new Uint8Array(totalLength);
  let offset = 0;
  for (const arr of arrays) {
    result.set(arr, offset);
    offset += arr.length;
  }
  return result;
}

/**
 * Check if two byte arrays are equal
 */
export function bytesEqual(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) {
    if (a[i] !== b[i]) return false;
  }
  return true;
}

/**
 * Create a zero-filled Uint8Array of specified length
 */
export function zeroBytes(length: number): Uint8Array {
  return new Uint8Array(length);
}

/**
 * Generate random bytes
 */
export function randomBytes(length: number): Uint8Array {
  const bytes = new Uint8Array(length);
  if (typeof crypto !== 'undefined' && crypto.getRandomValues) {
    crypto.getRandomValues(bytes);
  } else {
    // Fallback for Node.js
    for (let i = 0; i < length; i++) {
      bytes[i] = Math.floor(Math.random() * 256);
    }
  }
  return bytes;
}
