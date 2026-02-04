/**
 * Circuit Registry - Central hub for all ZK circuits
 *
 * To add a new circuit:
 * 1. Create a new handler file (e.g., my-circuit.ts)
 * 2. Import and add it to the registry below
 * 3. Implement the CircuitHandler interface
 * 4. Add backend support in prover-network/src/messages.rs (CircuitType enum)
 * 5. Add verification logic in prover-coordinator/src/main.rs
 *
 * That's it! The frontend will automatically pick up the new circuit.
 */

import type { CircuitHandler, CircuitMetadata, CircuitRegistry } from './types';
import { schnorrCircuit } from './schnorr';
import { hashPreimageCircuit } from './hash-preimage';

// CIRCUIT REGISTRY - Add new circuits here

export const circuitRegistry: CircuitRegistry = {
  'schnorr': schnorrCircuit,
  'hash-preimage': hashPreimageCircuit,

  // Coming soon circuits (UI only - no backend yet)
  'range-proof': {
    metadata: {
      id: 'range-proof',
      name: 'Range Proof',
      icon: '',
      description: 'Prove a committed value is within a range without revealing it',
      statement: 'My committed value is between min and max',
      publicInputs: ['Commitment', 'Min/Max bounds'],
      privateWitness: ['Actual value', 'Commitment randomness'],
      useCase: 'Age verification, Balance proofs',
      status: 'coming-soon',
    },
    setupFields: [],
    async processSetup() {
      throw new Error('Range proof circuit not yet implemented');
    },
  },

  'merkle-membership': {
    metadata: {
      id: 'merkle-membership',
      name: 'Merkle Membership',
      icon: '',
      description: 'Prove an element is in a Merkle tree without revealing which one',
      statement: 'This leaf is in the Merkle tree',
      publicInputs: ['Merkle Root', 'Leaf value'],
      privateWitness: ['Merkle path (sibling hashes)'],
      useCase: 'Allowlist membership, Anonymous voting',
      status: 'coming-soon',
    },
    setupFields: [],
    async processSetup() {
      throw new Error('Merkle membership circuit not yet implemented');
    },
  },
};

// HELPER FUNCTIONS

/**
 * Get all circuits as an array (for UI rendering)
 */
export function getAllCircuits(): CircuitMetadata[] {
  return Object.values(circuitRegistry).map(handler => handler.metadata);
}

/**
 * Get only active circuits (backend implemented)
 */
export function getActiveCircuits(): CircuitMetadata[] {
  return getAllCircuits().filter(c => c.status === 'active');
}

/**
 * Get circuits available in UI (active + ui-only)
 */
export function getAvailableCircuits(): CircuitMetadata[] {
  return getAllCircuits().filter(c => c.status === 'active' || c.status === 'ui-only');
}

/**
 * Get a specific circuit handler by ID
 */
export function getCircuitHandler(circuitId: string): CircuitHandler | undefined {
  return circuitRegistry[circuitId];
}

/**
 * Get circuit metadata by ID
 */
export function getCircuitMetadata(circuitId: string): CircuitMetadata | undefined {
  return circuitRegistry[circuitId]?.metadata;
}

/**
 * Check if a circuit is fully implemented (active)
 */
export function isCircuitActive(circuitId: string): boolean {
  return circuitRegistry[circuitId]?.metadata.status === 'active';
}

/**
 * Get the list of circuit IDs that are active
 */
export function getActiveCircuitIds(): string[] {
  return Object.keys(circuitRegistry).filter(id =>
    circuitRegistry[id].metadata.status === 'active'
  );
}

// Re-export types for convenience
export type { CircuitHandler, CircuitMetadata, CircuitInputField, CircuitSetupInputs, CircuitSetupPayload, CircuitStatus } from './types';
