//! Shamir's Secret Sharing implementation
//!
//! Splits a secret into `n` shares such that any `t` shares can reconstruct it,
//! but `t-1` shares reveal nothing about the secret.

use ark_bn254::Fr;
use ark_ff::{Field, UniformRand};
use ark_std::rand::Rng;
use ark_std::{One, Zero};

/// A single share of a secret
#[derive(Clone, Debug)]
pub struct SecretShare {
    /// Share index (1-indexed)
    pub index: usize,
    /// X-coordinate in the polynomial
    pub x: Fr,
    /// Y-coordinate (the share value)
    pub y: Fr,
}

/// A set of shares with metadata
#[derive(Clone, Debug)]
pub struct ShareSet {
    pub shares: Vec<SecretShare>,
    pub threshold: usize,
}

/// Split a secret into `n` shares with threshold `t`
///
/// Uses a random polynomial of degree `t-1` where the constant term is the secret.
///
/// # Arguments
/// * `secret` - The secret to split
/// * `n` - Total number of shares to generate
/// * `t` - Threshold (minimum shares needed to reconstruct)
/// * `rng` - Random number generator
///
/// # Returns
/// A `ShareSet` containing `n` shares
pub fn share_secret<R: Rng>(secret: Fr, n: usize, t: usize, rng: &mut R) -> ShareSet {
    assert!(t <= n, "Threshold cannot exceed number of shares");
    assert!(t >= 1, "Threshold must be at least 1");

    // Polynomial: f(x) = secret + a_1*x + a_2*x^2 + ... + a_{t-1}*x^{t-1}
    let mut coefficients = vec![secret];
    for _ in 1..t {
        coefficients.push(Fr::rand(rng));
    }

    let shares = (1..=n)
        .map(|i| {
            let x = Fr::from(i as u64);
            let y = evaluate_polynomial(&coefficients, x);
            SecretShare { index: i, x, y }
        })
        .collect();

    ShareSet {
        shares,
        threshold: t,
    }
}

/// Reconstruct the secret from shares using Lagrange interpolation
///
/// # Arguments
/// * `shares` - At least `threshold` shares
///
/// # Returns
/// The reconstructed secret (f(0))
pub fn reconstruct_secret(shares: &[SecretShare]) -> Fr {
    lagrange_interpolate_at_zero(
        &shares.iter().map(|s| s.x).collect::<Vec<_>>(),
        &shares.iter().map(|s| s.y).collect::<Vec<_>>(),
    )
}

/// Compute Lagrange coefficient λ_i(0) for interpolating at x=0
///
/// λ_i(0) = Π_{j≠i} (0 - x_j) / (x_i - x_j) = Π_{j≠i} x_j / (x_j - x_i)
pub fn lagrange_coefficient(x_coords: &[Fr], i: usize) -> Fr {
    let mut numerator = Fr::one();
    let mut denominator = Fr::one();

    for (j, &x_j) in x_coords.iter().enumerate() {
        if i != j {
            numerator *= x_j;
            denominator *= x_j - x_coords[i];
        }
    }

    numerator
        * denominator
            .inverse()
            .expect("Denominator should be non-zero")
}

/// Lagrange interpolation to find f(0)
fn lagrange_interpolate_at_zero(x_coords: &[Fr], y_values: &[Fr]) -> Fr {
    x_coords
        .iter()
        .enumerate()
        .map(|(i, _)| y_values[i] * lagrange_coefficient(x_coords, i))
        .fold(Fr::zero(), |acc, term| acc + term)
}

/// Evaluate polynomial at point x using Horner's method
fn evaluate_polynomial(coefficients: &[Fr], x: Fr) -> Fr {
    let mut result = Fr::zero();
    let mut x_power = Fr::one();

    for coeff in coefficients {
        result += *coeff * x_power;
        x_power *= x;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use ark_std::test_rng;

    #[test]
    fn test_share_and_reconstruct() {
        let mut rng = test_rng();
        let secret = Fr::from(12345u64);

        let share_set = share_secret(secret, 5, 3, &mut rng);

        // Reconstruct with exactly threshold shares
        let recovered = reconstruct_secret(&share_set.shares[0..3]);
        assert_eq!(secret, recovered);

        // Reconstruct with different subset
        let alt_shares = vec![
            share_set.shares[0].clone(),
            share_set.shares[2].clone(),
            share_set.shares[4].clone(),
        ];
        let recovered = reconstruct_secret(&alt_shares);
        assert_eq!(secret, recovered);

        // Reconstruct with all shares
        let recovered = reconstruct_secret(&share_set.shares);
        assert_eq!(secret, recovered);
    }

    #[test]
    fn test_lagrange_coefficients_sum_to_one() {
        let x_coords = vec![Fr::from(1u64), Fr::from(2u64), Fr::from(3u64)];
        let sum: Fr = (0..3).map(|i| lagrange_coefficient(&x_coords, i)).sum();
        assert_eq!(sum, Fr::one());
    }

    #[test]
    fn test_random_secrets() {
        let mut rng = test_rng();

        for _ in 0..10 {
            let secret = Fr::rand(&mut rng);
            let share_set = share_secret(secret, 7, 4, &mut rng);
            let recovered = reconstruct_secret(&share_set.shares[0..4]);
            assert_eq!(secret, recovered);
        }
    }
}
