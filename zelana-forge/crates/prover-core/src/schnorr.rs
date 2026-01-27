//! Distributed Schnorr proof implementation
//!
//! Implements a threshold Schnorr signature scheme where:
//! - No single party knows the full secret
//! - Any `t` parties can collaborate to produce a valid proof
//! - The proof is zero-knowledge (verifier learns nothing about the secret)

use crate::errors::*;
use crate::shamir::{lagrange_coefficient, SecretShare};
use ark_bn254::{Fr, G1Affine, G1Projective};
use ark_ec::CurveGroup;
use ark_ff::{PrimeField, UniformRand};
use ark_serialize::CanonicalSerialize;
use ark_std::rand::Rng;
use ark_std::Zero;
use sha2::{Digest, Sha256};

/// Public parameters for the proof system
#[derive(Clone, Debug)]
pub struct PublicParams {
    /// Generator point
    pub generator: G1Affine,
    /// Public key: g^secret
    pub public_key: G1Affine,
}

impl PublicParams {
    /// Create new public parameters
    pub fn new<R: Rng>(secret: Fr, rng: &mut R) -> Self {
        let generator = G1Projective::rand(rng).into_affine();
        let public_key = (generator * secret).into_affine();
        Self {
            generator,
            public_key,
        }
    }

    /// Create from existing generator with a secret
    pub fn from_generator(generator: G1Affine, secret: Fr) -> Self {
        let public_key = (generator * secret).into_affine();
        Self {
            generator,
            public_key,
        }
    }
}

/// A commitment in the Schnorr protocol (Phase 1)
#[derive(Clone, Debug)]
pub struct Commitment {
    /// Node index
    pub node_id: usize,
    /// The commitment point: g^nonce
    pub point: G1Affine,
    /// The nonce (kept secret by the node)
    nonce: Fr,
}

impl Commitment {
    /// Generate a new commitment
    pub fn generate<R: Rng>(node_id: usize, generator: &G1Affine, rng: &mut R) -> Self {
        let nonce = Fr::rand(rng);
        let point = (*generator * nonce).into_affine();
        Self {
            node_id,
            point,
            nonce,
        }
    }

    /// Get the nonce (only for the node that created it)
    pub fn nonce(&self) -> Fr {
        self.nonce
    }
}

/// A proof fragment from a single node (Phase 3)
#[derive(Clone, Debug)]
pub struct ProofFragment {
    /// Node index (1-indexed)
    pub node_id: usize,
    /// Commitment point
    pub commitment: G1Affine,
    /// Response: nonce + challenge * share
    pub response: Fr,
}

impl ProofFragment {
    /// Create a proof fragment
    pub fn create(share: &SecretShare, commitment: &Commitment, challenge: Fr) -> Self {
        assert_eq!(
            share.index, commitment.node_id,
            "Share and commitment must be from same node"
        );

        let response = commitment.nonce + (challenge * share.y);

        Self {
            node_id: share.index,
            commitment: commitment.point,
            response,
        }
    }
}

/// Complete distributed Schnorr proof
#[derive(Clone, Debug)]
pub struct DistributedProof {
    /// Aggregated commitment
    pub commitment: G1Affine,
    /// Fiat-Shamir challenge
    pub challenge: Fr,
    /// Aggregated response
    pub response: Fr,
}

impl DistributedProof {
    /// Aggregate proof fragments into a complete proof
    ///
    /// Uses Lagrange interpolation to combine fragments such that
    /// the result is equivalent to a proof created with the full secret.
    pub fn aggregate(fragments: &[ProofFragment], challenge: Fr) -> Result<Self> {
        if fragments.is_empty() {
            return Err(ProverError::InsufficientFragments { needed: 1, got: 0 });
        }

        let x_coords: Vec<Fr> = fragments
            .iter()
            .map(|f| Fr::from(f.node_id as u64))
            .collect();

        let mut agg_commitment = G1Projective::zero();
        let mut agg_response = Fr::zero();

        for (i, fragment) in fragments.iter().enumerate() {
            let coeff = lagrange_coefficient(&x_coords, i);

            agg_commitment += G1Projective::from(fragment.commitment) * coeff;
            agg_response += fragment.response * coeff;
        }

        Ok(Self {
            commitment: agg_commitment.into_affine(),
            challenge,
            response: agg_response,
        })
    }

    /// Verify the proof
    ///
    /// Checks: g^response == commitment * public_key^challenge
    pub fn verify(&self, params: &PublicParams) -> bool {
        let lhs = (params.generator * self.response).into_affine();
        let rhs = (self.commitment + (params.public_key * self.challenge)).into_affine();
        lhs == rhs
    }
}

/// Generate Fiat-Shamir challenge from commitments
///
/// Uses SHA-256 to hash the public parameters and all commitments
pub fn generate_challenge(
    generator: &G1Affine,
    public_key: &G1Affine,
    commitments: &[G1Affine],
) -> Fr {
    let mut hasher = Sha256::new();

    // Hash generator
    let mut buf = Vec::new();
    generator
        .serialize_compressed(&mut buf)
        .expect("Serialization should not fail");
    hasher.update(&buf);

    // Hash public key
    buf.clear();
    public_key
        .serialize_compressed(&mut buf)
        .expect("Serialization should not fail");
    hasher.update(&buf);

    // Hash all commitments
    for comm in commitments {
        buf.clear();
        comm.serialize_compressed(&mut buf)
            .expect("Serialization should not fail");
        hasher.update(&buf);
    }

    let hash = hasher.finalize();
    Fr::from_le_bytes_mod_order(&hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shamir::share_secret;
    use ark_std::test_rng;

    #[test]
    fn test_distributed_proof() {
        let mut rng = test_rng();

        // Setup
        let secret = Fr::rand(&mut rng);
        let params = PublicParams::new(secret, &mut rng);
        let share_set = share_secret(secret, 5, 3, &mut rng);

        // Phase 1: Generate commitments
        let commitments: Vec<Commitment> = share_set.shares[0..3]
            .iter()
            .map(|s| Commitment::generate(s.index, &params.generator, &mut rng))
            .collect();

        // Phase 2: Generate challenge
        let commitment_points: Vec<G1Affine> = commitments.iter().map(|c| c.point).collect();
        let challenge =
            generate_challenge(&params.generator, &params.public_key, &commitment_points);

        // Phase 3: Generate fragments
        let fragments: Vec<ProofFragment> = share_set.shares[0..3]
            .iter()
            .zip(commitments.iter())
            .map(|(share, comm)| ProofFragment::create(share, comm, challenge))
            .collect();

        // Phase 4: Aggregate
        let proof = DistributedProof::aggregate(&fragments, challenge).unwrap();

        // Verify
        assert!(proof.verify(&params));
    }

    #[test]
    fn test_different_subsets_produce_valid_proofs() {
        let mut rng = test_rng();

        let secret = Fr::rand(&mut rng);
        let params = PublicParams::new(secret, &mut rng);
        let share_set = share_secret(secret, 7, 4, &mut rng);

        // Test with different subsets of 4 nodes
        let subsets = vec![vec![0, 1, 2, 3], vec![1, 3, 4, 6], vec![0, 2, 4, 6]];

        for indices in subsets {
            let shares: Vec<_> = indices.iter().map(|&i| &share_set.shares[i]).collect();

            let commitments: Vec<Commitment> = shares
                .iter()
                .map(|s| Commitment::generate(s.index, &params.generator, &mut rng))
                .collect();

            let commitment_points: Vec<G1Affine> = commitments.iter().map(|c| c.point).collect();
            let challenge =
                generate_challenge(&params.generator, &params.public_key, &commitment_points);

            let fragments: Vec<ProofFragment> = shares
                .iter()
                .zip(commitments.iter())
                .map(|(share, comm)| ProofFragment::create(share, comm, challenge))
                .collect();

            let proof = DistributedProof::aggregate(&fragments, challenge).unwrap();
            assert!(
                proof.verify(&params),
                "Proof should verify for subset {:?}",
                indices
            );
        }
    }
}
