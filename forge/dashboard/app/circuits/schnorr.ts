// Schnorr Signature Circuit Handler

import type { CircuitHandler, CircuitSetupInputs, CircuitSetupPayload } from './types';

/**
 * Schnorr Signature Circuit
 *
 * Proves knowledge of a secret key s such that PublicKey = g^s
 * Used for authentication, digital signatures, and key ownership proofs.
 */
export const schnorrCircuit: CircuitHandler = {
  metadata: {
    id: 'schnorr',
    name: 'Schnorr Signature',
    icon: '',
    description: 'Prove you know the secret key for a public key (like authentication)',
    statement: 'I know secret s such that PublicKey = g^s',
    publicInputs: ['Public Key (g^s)', 'Message to sign'],
    privateWitness: ['Secret Key (s)'],
    useCase: 'Authentication, Digital Signatures, Key Ownership',
    status: 'active',
  },

  setupFields: [
    {
      id: 'secret',
      label: 'Private Witness: Secret Key',
      placeholder: '0123456789abcdef... (64 chars hex, leave empty for random)',
      type: 'hex',
      isPrivate: true,
      defaultValue: 'abc123', // Demo key
      helpText: 'Will be split using Shamir\'s Secret Sharing. Never leaves the system.',
      validation: {
        maxLength: 64,
      },
    },
    {
      id: 'message',
      label: 'Public Input: Message to Sign',
      placeholder: 'Enter message to sign/authorize',
      type: 'text',
      isPrivate: false,
      defaultValue: 'my_custom_proof_message',
      helpText: 'This message will be bound to the proof (like signing a message). The public key from Setup is also public.',
    },
  ],

  async processSetup(inputs: CircuitSetupInputs): Promise<CircuitSetupPayload> {
    // For Schnorr: use custom secret or generate random one
    const secret = inputs.secret?.trim() || generateRandomHex(64);
    const witness = inputs.message || 'my_custom_proof_message';

    return {
      secret,
      witness,
    };
  },

  getVerificationLogs(isValid: boolean): string[] {
    if (isValid) {
      return [
        ' Step 1: Commitment check - H(witness || salt) matches',
        ' Step 2: Verified g^z = C · PK^c (Schnorr equation)',
        ' Step 3: Challenge matches H(commitment)',
        ' Step 4: All cryptographic checks passed',
        ' Proof is VALID! Witness was hidden during proving!',
        '️ Privacy was preserved - nodes never saw the witness!',
      ];
    } else {
      return [
        ' Verification equation failed',
        ' Proof is INVALID',
      ];
    }
  },
};

/**
 * Generate a random hex string of specified length
 */
function generateRandomHex(length: number): string {
  return Array.from({ length }, () =>
    Math.floor(Math.random() * 16).toString(16)
  ).join('');
}
