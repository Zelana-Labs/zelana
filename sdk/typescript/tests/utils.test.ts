/**
 * Tests for Zelana SDK utilities
 */

import { describe, it, expect } from 'bun:test';
import {
  bytesToHex,
  hexToBytes,
  bytesToBase58,
  base58ToBytes,
  u64ToLeBytes,
  leBytesToU64,
  concatBytes,
  bytesEqual,
} from '../src/utils';

describe('Hex encoding', () => {
  it('should encode bytes to hex', () => {
    const bytes = new Uint8Array([0x00, 0x01, 0x02, 0xff]);
    expect(bytesToHex(bytes)).toBe('000102ff');
  });

  it('should decode hex to bytes', () => {
    const bytes = hexToBytes('000102ff');
    expect(bytes).toEqual(new Uint8Array([0x00, 0x01, 0x02, 0xff]));
  });

  it('should handle 0x prefix', () => {
    const bytes = hexToBytes('0x000102ff');
    expect(bytes).toEqual(new Uint8Array([0x00, 0x01, 0x02, 0xff]));
  });

  it('should roundtrip hex encoding', () => {
    const original = new Uint8Array(32);
    for (let i = 0; i < 32; i++) original[i] = i;
    const hex = bytesToHex(original);
    const decoded = hexToBytes(hex);
    expect(decoded).toEqual(original);
  });

  it('should throw on invalid hex', () => {
    expect(() => hexToBytes('0g')).toThrow();
    expect(() => hexToBytes('abc')).toThrow(); // odd length
  });
});

describe('Base58 encoding', () => {
  it('should encode bytes to base58', () => {
    // Known test vector: "Hello World!" in base58
    const bytes = new Uint8Array([
      0x48, 0x65, 0x6c, 0x6c, 0x6f, 0x20, 
      0x57, 0x6f, 0x72, 0x6c, 0x64, 0x21
    ]);
    const b58 = bytesToBase58(bytes);
    expect(b58).toBe('2NEpo7TZRRrLZSi2U');
  });

  it('should decode base58 to bytes', () => {
    const bytes = base58ToBytes('2NEpo7TZRRrLZSi2U');
    expect(bytesToHex(bytes)).toBe('48656c6c6f20576f726c6421');
  });

  it('should handle leading zeros (as 1s)', () => {
    const bytes = new Uint8Array([0, 0, 0, 1]);
    const b58 = bytesToBase58(bytes);
    expect(b58.startsWith('111')).toBe(true);
    
    const decoded = base58ToBytes(b58);
    expect(decoded).toEqual(bytes);
  });

  it('should roundtrip 32-byte pubkey', () => {
    const pubkey = new Uint8Array(32);
    for (let i = 0; i < 32; i++) pubkey[i] = i + 1;
    
    const b58 = bytesToBase58(pubkey);
    const decoded = base58ToBytes(b58);
    expect(decoded).toEqual(pubkey);
  });
});

describe('u64 encoding', () => {
  it('should encode small values', () => {
    const bytes = u64ToLeBytes(BigInt(1));
    expect(bytes).toEqual(new Uint8Array([1, 0, 0, 0, 0, 0, 0, 0]));
  });

  it('should encode max u64', () => {
    const max = BigInt('18446744073709551615');
    const bytes = u64ToLeBytes(max);
    expect(bytes).toEqual(new Uint8Array([255, 255, 255, 255, 255, 255, 255, 255]));
  });

  it('should roundtrip u64 values', () => {
    const testValues = [
      BigInt(0),
      BigInt(1),
      BigInt(1000000),
      BigInt('9223372036854775807'), // max i64
      BigInt('18446744073709551615'), // max u64
    ];

    for (const value of testValues) {
      const bytes = u64ToLeBytes(value);
      const decoded = leBytesToU64(bytes);
      expect(decoded).toBe(value);
    }
  });
});

describe('Byte array utilities', () => {
  it('should concatenate arrays', () => {
    const a = new Uint8Array([1, 2]);
    const b = new Uint8Array([3, 4]);
    const c = new Uint8Array([5]);
    
    const result = concatBytes(a, b, c);
    expect(result).toEqual(new Uint8Array([1, 2, 3, 4, 5]));
  });

  it('should check equality', () => {
    const a = new Uint8Array([1, 2, 3]);
    const b = new Uint8Array([1, 2, 3]);
    const c = new Uint8Array([1, 2, 4]);
    
    expect(bytesEqual(a, b)).toBe(true);
    expect(bytesEqual(a, c)).toBe(false);
  });
});
