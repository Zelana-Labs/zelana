// Hash Preimage Circuit Handler

import type { CircuitHandler, CircuitSetupInputs, CircuitSetupPayload } from './types';
import { computeSHA256, preimageToSecret } from '../utils/crypto';

/**
 * Hash Preimage Circuit
 *
 * Proves knowledge of a preimage such that Hash(preimage) = target
 * Used for password verification, secret commitments, and proving knowledge of secrets.
 */
export const hashPreimageCircuit: CircuitHandler = {
  metadata: {
    id: 'hash-preimage',
    name: 'Hash Preimage',
    icon: '🔗',
    description: 'Prove you know the input that produces a specific hash output',
    statement: 'I know preimage such that Hash(preimage) = target',
    publicInputs: ['Target Hash (SHA256)'],
    privateWitness: ['Preimage (the secret input)'],
    useCase: 'Password verification, Secret commitments',
    status: 'active',
  },

  setupFields: [
    {
      id: 'preimage',
      label: 'Private Witness: Preimage (your secret)',
      placeholder: 'my_secret_password',
      type: 'text',
      isPrivate: true,
      defaultValue: 'my_secret_password',
      helpText: 'The input that produces your target hash. Will be split and kept secret.',
    },
  ],

  proveFields: [
    {
      id: 'targetHash',
      label: 'Public Input: Target Hash',
      placeholder: '0x1234abcd... (auto-computed from preimage)',
      type: 'hex',
      isPrivate: false,
      helpText: 'The hash you\'re proving you know the preimage for. Everyone can see this, but not the preimage itself!',
    },
  ],

  async processSetup(inputs: CircuitSetupInputs): Promise<CircuitSetupPayload> {
    const preimage = inputs.preimage || 'my_secret_password';

    // For Hash Preimage: the preimage is the witness
    // The secret is hash_to_field(SHA256(preimage)) to match backend
    const secret = await preimageToSecret(preimage);
    const targetHash = await computeSHA256(preimage);

    return {
      secret,
      witness: preimage,
      extraData: {
        targetHash,
      },
    };
  },

  getVerificationLogs(isValid: boolean): string[] {
    if (isValid) {
      return [
        '✅ Step 1: Commitment check - H(witness || salt) matches',
        '✅ Step 2: Verified hash_to_field(SHA256(preimage)) matches public key',
        '✅ Step 3: Verified g^z = C · PK^c (Schnorr equation)',
        '✅ Step 4: All cryptographic checks passed',
        '🎉 Proof is VALID! Preimage was hidden during proving!',
        '🛡️ Privacy was preserved - nodes never saw the preimage!',
      ];
    } else {
      return [
        '❌ Hash verification failed - preimage does not match target hash',
        '❌ Proof is INVALID',
      ];
    }
  },

  infoBanner: {
    title: 'Hash Preimage Circuit',
    description: 'Prove you know a secret preimage that hashes to a specific value without revealing the preimage.',
    howItWorks: [
      'Enter your secret preimage (e.g., a password)',
      'The system computes SHA256(preimage) as the target hash',
      'Distributed nodes prove you know the preimage',
      'Verification confirms the preimage matches the hash',
    ],
    tip: 'Perfect for password verification, secret commitments, and proving knowledge without revealing secrets.',
  },
};
