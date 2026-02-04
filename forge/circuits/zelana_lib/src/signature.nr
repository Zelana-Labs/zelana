// Signature Verification
//
// Zelana uses signature verification for transparent transactions (transfers).
// For shielded transactions, the authentication is inside the ZK proof.
//
// Noir 1.0 provides embedded curve operations for BN254/Grumpkin.
// We implement a simplified Schnorr-like verification here.
//
// NOTE: In production, use a proper EdDSA or Schnorr library.
// For this MVP, we verify that the signature is deterministically 
// linked to the message and public key.

use std::embedded_curve_ops::{EmbeddedCurvePoint, EmbeddedCurveScalar};
use std::embedded_curve_ops::multi_scalar_mul;

use crate::poseidon::{hash_2, hash_5};

/// Public key structure (point on embedded curve)
pub struct PublicKey {
    pub x: Field,
    pub y: Field,
}

impl PublicKey {
    /// Create a public key from x, y coordinates
    pub fn new(x: Field, y: Field) -> Self {
        PublicKey { x, y }
    }
    
    /// Create from a single Field (for simpler interfaces)
    pub fn from_field(pk: Field) -> Self {
        // In practice, this would need proper decompression
        PublicKey { x: pk, y: 0 }
    }
    
    /// Hash the public key to a single Field (for account ID)
    pub fn hash(self) -> Field {
        hash_2(self.x, self.y)
    }
    
    /// Convert to embedded curve point
    pub fn to_point(self) -> EmbeddedCurvePoint {
        EmbeddedCurvePoint { x: self.x, y: self.y, is_infinite: false }
    }
}

/// Signature structure for Schnorr-style signatures
/// 
/// A Schnorr signature is (R, s) where:
/// - R is a curve point (commitment)
/// - s is a scalar (response)
/// 
/// Verification: s*G == R + e*P where e = H(R, P, m)
pub struct Signature {
    pub r_x: Field,  // R point x-coordinate
    pub r_y: Field,  // R point y-coordinate  
    pub s: Field,    // s scalar
}

impl Signature {
    pub fn new(r_x: Field, r_y: Field, s: Field) -> Self {
        Signature { r_x, r_y, s }
    }
    
    /// Get R as a curve point
    pub fn r_point(self) -> EmbeddedCurvePoint {
        EmbeddedCurvePoint { x: self.r_x, y: self.r_y, is_infinite: false }
    }
}

/// Verify a Schnorr signature on a message
///
/// This implements the verification equation: s*G == R + e*P
/// where e = H(R.x, R.y, P.x, P.y, message)
///
/// # Arguments
/// * `pub_key` - Public key (P)
/// * `signature` - Signature (R, s)
/// * `message` - Message hash (single Field)
///
/// # Returns
/// true if signature is valid
pub fn verify_schnorr_signature(
    pub_key: PublicKey,
    signature: Signature,
    message: Field
) -> bool {
    // Compute challenge: e = H(R.x, R.y, P.x, P.y, message)
    let e = hash_5(signature.r_x, signature.r_y, pub_key.x, pub_key.y, message);
    
    // Get scalars for multi_scalar_mul
    let s_scalar = EmbeddedCurveScalar::from_field(signature.s);
    let e_scalar = EmbeddedCurveScalar::from_field(e);
    let one_scalar = EmbeddedCurveScalar::from_field(1);
    
    // Compute s*G using generator point
    // The generator point for BN254/Grumpkin embedded curve
    // G = (1, 2) for the standard generator
    let generator = EmbeddedCurvePoint { 
        x: 1, 
        y: 17631683881184975370165255887551781615748388533673675138860,
        is_infinite: false 
    };
    
    // Compute s*G
    let s_g = multi_scalar_mul([generator], [s_scalar]);
    
    // Compute R + e*P
    // We need to compute: R + e*P
    // Using multi_scalar_mul: 1*R + e*P
    let r_point = signature.r_point();
    let p_point = pub_key.to_point();
    
    let r_plus_ep = multi_scalar_mul([r_point, p_point], [one_scalar, e_scalar]);
    
    // Verify: s*G == R + e*P
    (s_g.x == r_plus_ep.x) & (s_g.y == r_plus_ep.y)
}

/// Simplified signature verification for testing/MVP
/// 
/// This version just verifies that the signature is non-zero and
/// deterministically depends on the inputs. Use for initial testing only.
pub fn verify_simple_signature(
    pub_key: Field,
    signature: Field,
    message: Field
) -> bool {
    // For testing: verify signature is a hash of message with pubkey
    // In production, replace with proper cryptographic verification
    let expected = hash_2(pub_key, message);
    
    // Signature should be derived from message and key
    // This is NOT cryptographically secure - just for circuit testing
    (signature != 0) & (signature == expected)
}

/// Transaction signature data
pub struct TransactionSignature {
    pub r_x: Field,
    pub r_y: Field,
    pub s: Field,
}

impl TransactionSignature {
    /// Convert to Signature struct
    pub fn to_signature(self) -> Signature {
        Signature::new(self.r_x, self.r_y, self.s)
    }
}

/// Verify a transfer transaction signature
/// 
/// The message is: hash(sender, receiver, amount, nonce, chain_id)
pub fn verify_transfer_signature(
    sender_pk: PublicKey,
    signature: Signature,
    sender: Field,
    receiver: Field,
    amount: Field,
    nonce: Field,
    chain_id: Field
) -> bool {
    // Compute message hash
    let message = hash_5(sender, receiver, amount, nonce, chain_id);
    
    // Verify signature on message
    verify_schnorr_signature(sender_pk, signature, message)
}

#[test]
fn test_pubkey_hash() {
    let pk = PublicKey::new(123, 456);
    let h = pk.hash();
    
    // Hash should be deterministic
    let h2 = pk.hash();
    assert(h == h2);
}

#[test]
fn test_simple_signature() {
    let pubkey = 12345;
    let message = 67890;
    
    // Generate a "valid" signature (for testing)
    let signature = hash_2(pubkey, message);
    
    assert(verify_simple_signature(pubkey, signature, message));
}

#[test]
fn test_simple_signature_fails_wrong_sig() {
    let pubkey = 12345;
    let message = 67890;
    
    // Wrong signature should fail
    let wrong_sig = 99999;
    
    assert(!verify_simple_signature(pubkey, wrong_sig, message));
}
