/**
 * Tests for Zelana SDK Keypair
 */

import { describe, it, expect } from 'bun:test';
import { Keypair, PublicKey } from '../src/keypair';
import { bytesToHex, hexToBytes } from '../src/utils';

describe('Keypair', () => {
  describe('generation', () => {
    it('should generate random keypairs', () => {
      const kp1 = Keypair.generate();
      const kp2 = Keypair.generate();
      
      expect(kp1.publicKey.length).toBe(32);
      expect(kp2.publicKey.length).toBe(32);
      
      // Should be different
      expect(bytesToHex(kp1.publicKey)).not.toBe(bytesToHex(kp2.publicKey));
    });

    it('should create from secret key', () => {
      const secret = new Uint8Array(32);
      for (let i = 0; i < 32; i++) secret[i] = i;
      
      const kp1 = Keypair.fromSecretKey(secret);
      const kp2 = Keypair.fromSecretKey(secret);
      
      // Same secret -> same public key
      expect(bytesToHex(kp1.publicKey)).toBe(bytesToHex(kp2.publicKey));
    });

    it('should create from hex', () => {
      const hex = '0000000000000000000000000000000000000000000000000000000000000001';
      const kp = Keypair.fromHex(hex);
      expect(kp.publicKey.length).toBe(32);
    });
  });

  describe('signing', () => {
    it('should sign and verify messages', () => {
      const kp = Keypair.generate();
      const message = new Uint8Array([1, 2, 3, 4, 5]);
      
      const signature = kp.sign(message);
      expect(signature.length).toBe(64);
      
      const valid = Keypair.verify(signature, message, kp.publicKey);
      expect(valid).toBe(true);
    });

    it('should reject invalid signatures', () => {
      const kp = Keypair.generate();
      const message = new Uint8Array([1, 2, 3, 4, 5]);
      const signature = kp.sign(message);
      
      // Tamper with signature
      signature[0] ^= 0xff;
      
      const valid = Keypair.verify(signature, message, kp.publicKey);
      expect(valid).toBe(false);
    });

    it('should reject wrong public key', () => {
      const kp1 = Keypair.generate();
      const kp2 = Keypair.generate();
      const message = new Uint8Array([1, 2, 3, 4, 5]);
      
      const signature = kp1.sign(message);
      const valid = Keypair.verify(signature, message, kp2.publicKey);
      expect(valid).toBe(false);
    });
  });

  describe('signTransfer', () => {
    it('should create valid transfer request', () => {
      const sender = Keypair.generate();
      const recipient = new Uint8Array(32);
      for (let i = 0; i < 32; i++) recipient[i] = i;
      
      const request = sender.signTransfer(
        recipient,
        BigInt(1000000),
        BigInt(0),
        BigInt(1)
      );
      
      expect(request.from).toEqual(sender.publicKey);
      expect(request.to).toEqual(recipient);
      expect(request.amount).toBe(BigInt(1000000));
      expect(request.nonce).toBe(BigInt(0));
      expect(request.chainId).toBe(BigInt(1));
      expect(request.signature.length).toBe(64);
      expect(request.signerPubkey).toEqual(sender.publicKey);
    });

    it('should be deterministic', () => {
      const sender = Keypair.fromHex('0'.repeat(64));
      const recipient = new Uint8Array(32);
      
      const req1 = sender.signTransfer(recipient, BigInt(1000), BigInt(0), BigInt(1));
      const req2 = sender.signTransfer(recipient, BigInt(1000), BigInt(0), BigInt(1));
      
      expect(bytesToHex(req1.signature)).toBe(bytesToHex(req2.signature));
    });
  });

  describe('signWithdrawal', () => {
    it('should create valid withdrawal request', () => {
      const sender = Keypair.generate();
      const l1Address = new Uint8Array(32);
      for (let i = 0; i < 32; i++) l1Address[i] = i + 1;
      
      const request = sender.signWithdrawal(
        l1Address,
        BigInt(5000000),
        BigInt(1)
      );
      
      expect(request.from).toEqual(sender.publicKey);
      expect(request.toL1Address).toEqual(l1Address);
      expect(request.amount).toBe(BigInt(5000000));
      expect(request.nonce).toBe(BigInt(1));
      expect(request.signature.length).toBe(64);
      expect(request.signerPubkey).toEqual(sender.publicKey);
    });
  });
});

describe('PublicKey', () => {
  it('should create from bytes', () => {
    const bytes = new Uint8Array(32);
    for (let i = 0; i < 32; i++) bytes[i] = i;
    
    const pk = new PublicKey(bytes);
    expect(pk.toBytes()).toEqual(bytes);
  });

  it('should create from hex string', () => {
    const hex = '0102030405060708091011121314151617181920212223242526272829303132';
    const pk = new PublicKey(hex);
    expect(pk.toHex()).toBe(hex);
  });

  it('should create from base58 string', () => {
    const kp = Keypair.generate();
    const b58 = kp.publicKeyBase58;
    
    const pk = new PublicKey(b58);
    expect(pk.toBytes()).toEqual(kp.publicKey);
  });

  it('should convert between formats', () => {
    const kp = Keypair.generate();
    const pk = new PublicKey(kp.publicKey);
    
    // Roundtrip through base58
    const b58 = pk.toBase58();
    const pk2 = new PublicKey(b58);
    expect(pk2.toBytes()).toEqual(kp.publicKey);
    
    // Roundtrip through hex
    const hex = pk.toHex();
    const pk3 = new PublicKey(hex);
    expect(pk3.toBytes()).toEqual(kp.publicKey);
  });

  it('should check equality', () => {
    const bytes = new Uint8Array(32);
    for (let i = 0; i < 32; i++) bytes[i] = i;
    
    const pk1 = new PublicKey(bytes);
    const pk2 = new PublicKey(bytes);
    const pk3 = new PublicKey(new Uint8Array(32)); // zeros
    
    expect(pk1.equals(pk2)).toBe(true);
    expect(pk1.equals(pk3)).toBe(false);
    expect(pk1.equals(bytes)).toBe(true);
  });

  it('should reject invalid length', () => {
    expect(() => new PublicKey(new Uint8Array(31))).toThrow();
    expect(() => new PublicKey(new Uint8Array(33))).toThrow();
  });
});
