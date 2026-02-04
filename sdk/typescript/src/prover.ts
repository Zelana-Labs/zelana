/**
 * Ownership Prover - Client-Side ZK Proof Generation
 *
 * This module provides the client-side proving functionality for Split Proving.
 * Users generate lightweight ownership proofs in their browser, which are then
 * verified by the sequencer and enhanced with Merkle proofs by the Swarm.
 *
 * Architecture:
 * 1. User computes witness using WASM MiMC (matches Noir circuit exactly)
 * 2. User generates ownership proof using Noir WASM prover (~500ms)
 * 3. Proof is sent to sequencer for verification and batching
 * 4. Swarm generates validity proof with Merkle membership (heavy work)
 */

import type { Bytes32 } from './types';
import { bytesToHex, hexToBytes } from './utils';

// Types

/**
 * Witness data computed from private inputs
 */
export interface OwnershipWitness {
  /** Owner's derived public key (32 bytes hex) */
  ownerPk: string;
  /** Note commitment (32 bytes hex) */
  commitment: string;
  /** Nullifier revealed when spending (32 bytes hex) */
  nullifier: string;
  /** Blinded proxy for swarm delegation (32 bytes hex) */
  blindedProxy: string;
}

/**
 * Complete ownership proof ready for submission
 */
export interface OwnershipProof {
  /** The ZK proof bytes */
  proof: Uint8Array;
  /** Public inputs to the circuit */
  publicInputs: {
    commitment: Bytes32;
    nullifier: Bytes32;
    blindedProxy: Bytes32;
  };
}

/**
 * Request to submit a delegated shielded transaction
 */
export interface DelegatedShieldedRequest {
  /** Ownership proof (serialized) */
  ownershipProof: Uint8Array;
  /** Nullifier being spent */
  nullifier: Bytes32;
  /** Input commitment */
  commitment: Bytes32;
  /** Blinded proxy for swarm lookup */
  blindedProxy: Bytes32;
  /** Output commitment for new note */
  outputCommitment: Bytes32;
  /** Encrypted note data */
  ciphertext: Uint8Array;
  /** Ephemeral key for decryption */
  ephemeralKey: Bytes32;
}

// WASM Module Interface

/**
 * Interface for the ownership-prover WASM module
 */
interface OwnershipProverWasm {
  derivePublicKey(spendingKeyHex: string): string;
  computeCommitment(ownerPkHex: string, value: bigint, blindingHex: string): string;
  computeNullifier(spendingKeyHex: string, commitmentHex: string, position: bigint): string;
  computeBlindedProxy(commitmentHex: string, position: bigint): string;
  generateWitness(
    spendingKeyHex: string,
    value: bigint,
    blindingHex: string,
    position: bigint
  ): OwnershipWitness;
  verifyWitness(
    spendingKeyHex: string,
    value: bigint,
    blindingHex: string,
    position: bigint,
    expectedCommitmentHex: string,
    expectedNullifierHex: string,
    expectedProxyHex: string
  ): boolean;
}

// OwnershipProver Class

/**
 * OwnershipProver handles client-side ownership proof generation.
 *
 * This class:
 * 1. Loads the WASM module for MiMC hash computation
 * 2. Loads the Noir circuit for proof generation
 * 3. Provides methods to compute witnesses and generate proofs
 *
 * Usage:
 * ```typescript
 * const prover = new OwnershipProver();
 * await prover.init();
 *
 * // Compute witness from private inputs
 * const witness = prover.computeWitness(spendingKey, value, blinding, position);
 *
 * // Generate proof
 * const proof = await prover.prove(spendingKey, value, blinding, position);
 * ```
 */
export class OwnershipProver {
  private wasmModule: OwnershipProverWasm | null = null;
  private noirCircuit: unknown = null;
  private backend: unknown = null;
  private initialized = false;

  /**
   * Initialize the prover
   *
   * This loads:
   * 1. The ownership-prover WASM module (MiMC hash)
   * 2. The Noir circuit definition
   * 3. The Barretenberg backend for proving
   *
   * @param options Configuration options
   */
  async init(options?: {
    /** URL to fetch WASM from (defaults to CDN) */
    wasmUrl?: string;
    /** URL to fetch circuit JSON from (defaults to CDN) */
    circuitUrl?: string;
    /** Skip Noir initialization (for witness-only mode) */
    witnessOnly?: boolean;
  }): Promise<void> {
    if (this.initialized) {
      return;
    }

    // Load WASM module
    try {
      // Dynamic import - the WASM module will be bundled or fetched
      const wasmModule = await this.loadWasmModule(options?.wasmUrl);
      this.wasmModule = wasmModule;
    } catch (error) {
      throw new Error(`Failed to load ownership-prover WASM: ${error}`);
    }

    // Load Noir circuit (unless witness-only mode)
    if (!options?.witnessOnly) {
      try {
        await this.loadNoirCircuit(options?.circuitUrl);
      } catch (error) {
        console.warn('Noir circuit not loaded, proof generation disabled:', error);
        // Continue without Noir - can still compute witnesses
      }
    }

    this.initialized = true;
  }

  /**
   * Load the WASM module
   */
  private async loadWasmModule(wasmUrl?: string): Promise<OwnershipProverWasm> {
    // Use fetch-based loading for browser compatibility
    // The 'zelana-ownership-prover' package is only available in specific environments
    if (wasmUrl) {
      // Dynamic import from URL - use Function constructor to avoid bundler issues
      const importFn = new Function('url', 'return import(url)');
      const module = await importFn(wasmUrl);
      await module.default();
      return module as OwnershipProverWasm;
    }
    
    // Try dynamic import as fallback (works in Node.js environments with the package installed)
    try {
      // Using eval to prevent bundlers from trying to resolve this at build time
      const dynamicImport = new Function('specifier', 'return import(specifier)');
      const module = await dynamicImport('zelana-ownership-prover');
      await module.default();
      return module as OwnershipProverWasm;
    } catch {
      throw new Error('Could not load WASM module. Please provide a wasmUrl or install zelana-ownership-prover.');
    }
  }

  /**
   * Load the Noir circuit and backend
   */
  private async loadNoirCircuit(circuitUrl?: string): Promise<void> {
    // This will be implemented when we integrate with @noir-lang/noir_js
    // For now, just log that we would load the circuit
    console.log('Noir circuit loading not yet implemented');
    this.noirCircuit = null;
    this.backend = null;
  }

  /**
   * Check if the prover is initialized
   */
  isInitialized(): boolean {
    return this.initialized;
  }

  /**
   * Check if proof generation is available
   */
  canProve(): boolean {
    return this.initialized && this.noirCircuit !== null;
  }

  // Cryptographic Operations (using WASM MiMC)

  /**
   * Derive public key from spending key
   *
   * pk = MiMC_hash3(PK_DOMAIN, spending_key, 0)
   */
  derivePublicKey(spendingKey: Bytes32): Bytes32 {
    this.ensureInitialized();
    const hex = this.wasmModule!.derivePublicKey(bytesToHex(spendingKey));
    return hexToBytes(hex);
  }

  /**
   * Compute note commitment
   *
   * commitment = MiMC_hash3(owner_pk, value, blinding)
   */
  computeCommitment(ownerPk: Bytes32, value: bigint, blinding: Bytes32): Bytes32 {
    this.ensureInitialized();
    const hex = this.wasmModule!.computeCommitment(
      bytesToHex(ownerPk),
      value,
      bytesToHex(blinding)
    );
    return hexToBytes(hex);
  }

  /**
   * Compute nullifier
   *
   * nullifier = MiMC_hash4(NULLIFIER_DOMAIN, spending_key, commitment, position)
   */
  computeNullifier(spendingKey: Bytes32, commitment: Bytes32, position: bigint): Bytes32 {
    this.ensureInitialized();
    const hex = this.wasmModule!.computeNullifier(
      bytesToHex(spendingKey),
      bytesToHex(commitment),
      position
    );
    return hexToBytes(hex);
  }

  /**
   * Compute blinded proxy for swarm delegation
   *
   * blinded_proxy = MiMC_hash3(DELEGATE_DOMAIN, commitment, position)
   */
  computeBlindedProxy(commitment: Bytes32, position: bigint): Bytes32 {
    this.ensureInitialized();
    const hex = this.wasmModule!.computeBlindedProxy(bytesToHex(commitment), position);
    return hexToBytes(hex);
  }

  /**
   * Compute complete witness from private inputs
   *
   * This computes all public outputs that will be revealed to the sequencer.
   */
  computeWitness(
    spendingKey: Bytes32,
    value: bigint,
    blinding: Bytes32,
    position: bigint
  ): OwnershipWitness {
    this.ensureInitialized();
    return this.wasmModule!.generateWitness(
      bytesToHex(spendingKey),
      value,
      bytesToHex(blinding),
      position
    );
  }

  /**
   * Verify that computed witness matches expected values
   *
   * Useful for debugging before generating a proof.
   */
  verifyWitness(
    spendingKey: Bytes32,
    value: bigint,
    blinding: Bytes32,
    position: bigint,
    expectedCommitment: Bytes32,
    expectedNullifier: Bytes32,
    expectedProxy: Bytes32
  ): boolean {
    this.ensureInitialized();
    return this.wasmModule!.verifyWitness(
      bytesToHex(spendingKey),
      value,
      bytesToHex(blinding),
      position,
      bytesToHex(expectedCommitment),
      bytesToHex(expectedNullifier),
      bytesToHex(expectedProxy)
    );
  }

  // Proof Generation

  /**
   * Generate an ownership proof
   *
   * This is the main entry point for proof generation. It:
   * 1. Computes the witness using WASM MiMC
   * 2. Generates the Noir proof using Barretenberg
   *
   * @param spendingKey The user's spending key (secret)
   * @param value The note value in lamports
   * @param blinding Random blinding factor
   * @param position Position in the commitment tree
   * @returns The proof and public inputs
   */
  async prove(
    spendingKey: Bytes32,
    value: bigint,
    blinding: Bytes32,
    position: bigint
  ): Promise<OwnershipProof> {
    this.ensureInitialized();

    if (!this.canProve()) {
      throw new Error('Noir circuit not loaded - proof generation unavailable');
    }

    // 1. Compute witness
    const witness = this.computeWitness(spendingKey, value, blinding, position);

    // 2. Generate proof using Noir
    // TODO: Implement when integrating with @noir-lang/noir_js
    throw new Error('Proof generation not yet implemented');
  }

  /**
   * Verify an ownership proof locally
   *
   * This is for debugging/testing. In production, the sequencer verifies proofs.
   */
  async verify(proof: OwnershipProof): Promise<boolean> {
    if (!this.canProve()) {
      throw new Error('Noir circuit not loaded - verification unavailable');
    }

    // TODO: Implement when integrating with @noir-lang/noir_js
    throw new Error('Proof verification not yet implemented');
  }

  // Helpers

  private ensureInitialized(): void {
    if (!this.initialized || !this.wasmModule) {
      throw new Error('Prover not initialized - call init() first');
    }
  }
}

// Standalone Functions (for use without class)

let globalProver: OwnershipProver | null = null;

/**
 * Get or create the global prover instance
 */
export async function getProver(): Promise<OwnershipProver> {
  if (!globalProver) {
    globalProver = new OwnershipProver();
    await globalProver.init({ witnessOnly: true });
  }
  return globalProver;
}

/**
 * Compute witness using the global prover
 */
export async function computeOwnershipWitness(
  spendingKey: Bytes32,
  value: bigint,
  blinding: Bytes32,
  position: bigint
): Promise<OwnershipWitness> {
  const prover = await getProver();
  return prover.computeWitness(spendingKey, value, blinding, position);
}

// Mock Prover (for development/testing without WASM)

/**
 * MockOwnershipProver provides a fake prover for development.
 *
 * It generates deterministic "proofs" using SHA-256 instead of real ZK proofs.
 * This is useful for:
 * - Development without WASM setup
 * - Testing the API flow
 * - Benchmarking non-proof overhead
 *
 * WARNING: This does NOT provide any cryptographic security!
 */
export class MockOwnershipProver extends OwnershipProver {
  async init(): Promise<void> {
    // No-op for mock
  }

  isInitialized(): boolean {
    return true;
  }

  canProve(): boolean {
    return true;
  }

  derivePublicKey(spendingKey: Bytes32): Bytes32 {
    // Return a deterministic "public key"
    return this.mockHash('PK', spendingKey);
  }

  computeCommitment(ownerPk: Bytes32, value: bigint, blinding: Bytes32): Bytes32 {
    // Return a deterministic "commitment"
    const valueBytes = new Uint8Array(8);
    new DataView(valueBytes.buffer).setBigUint64(0, value, true);
    return this.mockHash('CM', ownerPk, valueBytes, blinding);
  }

  computeNullifier(spendingKey: Bytes32, commitment: Bytes32, position: bigint): Bytes32 {
    const posBytes = new Uint8Array(8);
    new DataView(posBytes.buffer).setBigUint64(0, position, true);
    return this.mockHash('NF', spendingKey, commitment, posBytes);
  }

  computeBlindedProxy(commitment: Bytes32, position: bigint): Bytes32 {
    const posBytes = new Uint8Array(8);
    new DataView(posBytes.buffer).setBigUint64(0, position, true);
    return this.mockHash('BP', commitment, posBytes);
  }

  computeWitness(
    spendingKey: Bytes32,
    value: bigint,
    blinding: Bytes32,
    position: bigint
  ): OwnershipWitness {
    const ownerPk = this.derivePublicKey(spendingKey);
    const commitment = this.computeCommitment(ownerPk, value, blinding);
    const nullifier = this.computeNullifier(spendingKey, commitment, position);
    const blindedProxy = this.computeBlindedProxy(commitment, position);

    return {
      ownerPk: bytesToHex(ownerPk),
      commitment: bytesToHex(commitment),
      nullifier: bytesToHex(nullifier),
      blindedProxy: bytesToHex(blindedProxy),
    };
  }

  async prove(
    spendingKey: Bytes32,
    value: bigint,
    blinding: Bytes32,
    position: bigint
  ): Promise<OwnershipProof> {
    const witness = this.computeWitness(spendingKey, value, blinding, position);

    // Generate a fake "proof" that's just a hash of the inputs
    const fakeProof = this.mockHash(
      'PROOF',
      hexToBytes(witness.commitment),
      hexToBytes(witness.nullifier)
    );

    return {
      proof: fakeProof,
      publicInputs: {
        commitment: hexToBytes(witness.commitment),
        nullifier: hexToBytes(witness.nullifier),
        blindedProxy: hexToBytes(witness.blindedProxy),
      },
    };
  }

  private mockHash(domain: string, ...inputs: Uint8Array[]): Bytes32 {
    // Simple deterministic hash using the Web Crypto API
    // This is synchronous for simplicity (uses a simple XOR-based hash)
    const domainBytes = new TextEncoder().encode(domain);
    const combined = new Uint8Array(
      domainBytes.length + inputs.reduce((acc, i) => acc + i.length, 0)
    );

    let offset = 0;
    combined.set(domainBytes, offset);
    offset += domainBytes.length;

    for (const input of inputs) {
      combined.set(input, offset);
      offset += input.length;
    }

    // Simple deterministic "hash" - NOT cryptographically secure!
    const result = new Uint8Array(32);
    for (let i = 0; i < combined.length; i++) {
      result[i % 32] ^= combined[i];
      result[(i + 1) % 32] ^= (combined[i] << 4) | (combined[i] >> 4);
    }

    return result;
  }
}

// Export a singleton mock prover for testing
export const mockProver = new MockOwnershipProver();
