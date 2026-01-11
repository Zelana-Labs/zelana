//! Shamir Secret Sharing for Threshold Encryption
//!
//! Implements K-of-N secret sharing using a simplified Shamir's scheme.

use rand::RngCore;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A share identifier (1-indexed)
pub type ShareId = u8;

/// A secret share
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Share {
    /// Share identifier (1 to N)
    pub id: ShareId,
    /// Share value (32 bytes)
    pub value: [u8; 32],
}

impl Share {
    /// Create a new share
    pub fn new(id: ShareId, value: [u8; 32]) -> Self {
        Self { id, value }
    }
}

/// Threshold encryption errors
#[derive(Debug, Error)]
pub enum ThresholdError {
    #[error("insufficient shares: got {got}, need {need}")]
    InsufficientShares { got: usize, need: usize },

    #[error("invalid threshold: k={k}, n={n}")]
    InvalidThreshold { k: usize, n: usize },

    #[error("share reconstruction failed")]
    ReconstructionFailed,

    #[error("decryption failed")]
    DecryptionFailed,

    #[error("invalid share format")]
    InvalidShare,
}

/// GF(256) - Galois Field operations for Shamir's scheme
mod gf256 {
    /// Multiplication in GF(256)
    pub fn mul(a: u8, b: u8) -> u8 {
        let mut result: u8 = 0;
        let mut a = a;
        let mut b = b;

        while b != 0 {
            if b & 1 != 0 {
                result ^= a;
            }
            let hi = a & 0x80;
            a <<= 1;
            if hi != 0 {
                a ^= 0x1b; // AES polynomial
            }
            b >>= 1;
        }
        result
    }

    /// Multiplicative inverse in GF(256) using exponentiation
    /// Since a^255 = 1 in GF(256), a^(-1) = a^254
    pub fn inv(a: u8) -> u8 {
        if a == 0 {
            return 0;
        }
        // Compute a^254 using square-and-multiply
        let mut result = a;
        // a^2
        result = mul(result, result);
        let a2 = result;
        // a^4
        result = mul(result, result);
        let a4 = result;
        // a^8
        result = mul(result, result);
        let a8 = result;
        // a^16
        result = mul(result, result);
        let a16 = result;
        // a^32
        result = mul(result, result);
        let a32 = result;
        // a^64
        result = mul(result, result);
        let a64 = result;
        // a^128
        result = mul(result, result);
        let a128 = result;

        // a^254 = a^128 * a^64 * a^32 * a^16 * a^8 * a^4 * a^2
        // = a^(128+64+32+16+8+4+2) = a^254
        result = mul(a128, a64);
        result = mul(result, a32);
        result = mul(result, a16);
        result = mul(result, a8);
        result = mul(result, a4);
        result = mul(result, a2);
        result
    }

    /// Division in GF(256)
    pub fn div(a: u8, b: u8) -> u8 {
        mul(a, inv(b))
    }
}

/// Split a secret into N shares, requiring K to reconstruct
///
/// Uses Shamir's secret sharing over GF(256), applied byte-by-byte
///
/// # Arguments
/// * `secret` - The 32-byte secret to split
/// * `threshold` - K: minimum shares needed to reconstruct
/// * `total` - N: total number of shares to generate
///
/// # Returns
/// Vector of N shares
pub fn split_secret(
    secret: &[u8; 32],
    threshold: usize,
    total: usize,
) -> Result<Vec<Share>, ThresholdError> {
    if threshold > total || threshold == 0 || total == 0 || total > 255 {
        return Err(ThresholdError::InvalidThreshold {
            k: threshold,
            n: total,
        });
    }

    let mut rng = rand::thread_rng();
    let mut shares: Vec<Share> = (1..=total as u8)
        .map(|id| Share::new(id, [0u8; 32]))
        .collect();

    // Process each byte of the secret independently
    for byte_idx in 0..32 {
        // Generate random polynomial coefficients
        // f(x) = secret[byte_idx] + a1*x + a2*x^2 + ... + a_{k-1}*x^{k-1}
        let mut coeffs = vec![secret[byte_idx]];
        for _ in 1..threshold {
            let mut random_byte = [0u8; 1];
            rng.fill_bytes(&mut random_byte);
            coeffs.push(random_byte[0]);
        }

        // Evaluate polynomial at each x = share_id
        for share in shares.iter_mut() {
            let x = share.id;
            let mut y = coeffs[0];
            let mut x_pow = x;

            for coeff in coeffs.iter().skip(1) {
                y ^= gf256::mul(*coeff, x_pow);
                x_pow = gf256::mul(x_pow, x);
            }

            share.value[byte_idx] = y;
        }
    }

    Ok(shares)
}

/// Combine K shares to reconstruct the secret using Lagrange interpolation
///
/// # Arguments
/// * `shares` - At least K shares
/// * `threshold` - K: the threshold used when splitting
///
/// # Returns
/// The reconstructed 32-byte secret
pub fn combine_shares(shares: &[Share], threshold: usize) -> Result<[u8; 32], ThresholdError> {
    if shares.len() < threshold {
        return Err(ThresholdError::InsufficientShares {
            got: shares.len(),
            need: threshold,
        });
    }

    let shares = &shares[..threshold];
    let mut secret = [0u8; 32];

    // Lagrange interpolation at x=0 for each byte
    for byte_idx in 0..32 {
        let mut result: u8 = 0;

        for (i, share_i) in shares.iter().enumerate() {
            let xi = share_i.id;
            let yi = share_i.value[byte_idx];

            // Calculate Lagrange basis polynomial Li(0)
            let mut numerator: u8 = 1;
            let mut denominator: u8 = 1;

            for (j, share_j) in shares.iter().enumerate() {
                if i != j {
                    let xj = share_j.id;
                    // Li(0) = product of (0 - xj) / (xi - xj) = product of xj / (xi ^ xj)
                    numerator = gf256::mul(numerator, xj);
                    denominator = gf256::mul(denominator, xi ^ xj);
                }
            }

            let li = gf256::div(numerator, denominator);
            result ^= gf256::mul(yi, li);
        }

        secret[byte_idx] = result;
    }

    Ok(secret)
}

/// Generate a random 32-byte secret
pub fn random_secret() -> [u8; 32] {
    let mut secret = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut secret);
    secret
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_and_combine() {
        let secret = [42u8; 32];
        let threshold = 3;
        let total = 5;

        let shares = split_secret(&secret, threshold, total).unwrap();
        assert_eq!(shares.len(), total);

        // Reconstruct with exactly threshold shares
        let recovered = combine_shares(&shares[0..3], threshold).unwrap();
        assert_eq!(recovered, secret);

        // Reconstruct with different subset of shares
        let recovered2 = combine_shares(&shares[1..4], threshold).unwrap();
        assert_eq!(recovered2, secret);
    }

    #[test]
    fn test_random_secret() {
        let threshold = 2;
        let total = 3;

        let secret = random_secret();
        let shares = split_secret(&secret, threshold, total).unwrap();
        let recovered = combine_shares(&shares[0..2], threshold).unwrap();
        assert_eq!(recovered, secret);
    }

    #[test]
    fn test_insufficient_shares() {
        let secret = [42u8; 32];
        let shares = split_secret(&secret, 3, 5).unwrap();

        let result = combine_shares(&shares[0..2], 3);
        assert!(matches!(
            result,
            Err(ThresholdError::InsufficientShares { .. })
        ));
    }

    #[test]
    fn test_invalid_threshold() {
        let secret = [42u8; 32];

        // k > n
        assert!(split_secret(&secret, 5, 3).is_err());

        // k = 0
        assert!(split_secret(&secret, 0, 3).is_err());

        // n = 0
        assert!(split_secret(&secret, 3, 0).is_err());
    }
}
