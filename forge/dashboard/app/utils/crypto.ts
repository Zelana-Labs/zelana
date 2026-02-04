// Client-side cryptography for blind proving

/**
 * Generate a cryptographically secure random salt
 */
export function generateSalt(): Uint8Array {
  return crypto.getRandomValues(new Uint8Array(32));
}

/**
 * Compute SHA-256 hash of a string (for hash preimage circuit)
 * Returns the hash as a hex string
 */
export async function computeSHA256(input: string): Promise<string> {
  const encoder = new TextEncoder();
  const data = encoder.encode(input);
  const hashBuffer = await crypto.subtle.digest('SHA-256', data);
  const hashArray = Array.from(new Uint8Array(hashBuffer));
  return hashArray.map(b => b.toString(16).padStart(2, '0')).join('');
}

/**
 * Convert a preimage to a field-compatible secret for hash preimage circuit
 * This matches the backend's hash_to_field function:
 * 1. Compute SHA-256 of the preimage
 * 2. Take first 31 bytes (to stay within field modulus)
 * 3. Return as hex string
 */
export async function preimageToSecret(preimage: string): Promise<string> {
  const hash = await computeSHA256(preimage);
  // Take first 31 bytes (62 hex chars) to match backend's hash_to_field
  return hash.substring(0, 62);
}

/**
 * Compute SHA-256 commitment to witness
 */
export async function commitToWitness(
  publicWitness: string,
  salt: Uint8Array
): Promise<string> {
  const encoder = new TextEncoder();
  const witnessBytes = encoder.encode(publicWitness);

  // Combine witness and salt
  const combined = new Uint8Array(witnessBytes.length + salt.length);
  combined.set(witnessBytes);
  combined.set(salt, witnessBytes.length);

  // Compute SHA-256
  const hashBuffer = await crypto.subtle.digest('SHA-256', combined);
  const hashArray = Array.from(new Uint8Array(hashBuffer));
  const hashHex = hashArray.map(b => b.toString(16).padStart(2, '0')).join('');

  return hashHex;
}

/**
 * Convert Uint8Array to hex string
 */
export function toHex(bytes: Uint8Array): string {
  return Array.from(bytes)
    .map(b => b.toString(16).padStart(2, '0'))
    .join('');
}

/**
 * Convert hex string to Uint8Array
 */
export function fromHex(hex: string): Uint8Array {
  const bytes = new Uint8Array(hex.length / 2);
  for (let i = 0; i < hex.length; i += 2) {
    bytes[i / 2] = parseInt(hex.substr(i, 2), 16);
  }
  return bytes;
}

/**
 * Generate a random session ID
 */
export function generateSessionId(): string {
  const randomBytes = crypto.getRandomValues(new Uint8Array(16));
  return `session-${toHex(randomBytes)}`;
}
