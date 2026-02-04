//! Integration test for the full distributed Schnorr proving protocol.
//!
//! This test simulates the entire protocol without network communication
//! to verify the cryptographic correctness.

#[cfg(test)]
mod tests {
    use crate::{
        commitment::{
            commit_witness, generate_challenge_from_commitment, verify_commitment, SALT_SIZE,
        },
        schnorr::Commitment,
        shamir::{lagrange_coefficient, share_secret},
        Fr, G1Projective,
    };
    use ark_ec::CurveGroup;
    use ark_ff::PrimeField;
    use ark_std::{test_rng, UniformRand, Zero};

    /// Full integration test of the distributed Schnorr protocol
    #[test]
    fn test_full_distributed_schnorr_protocol() {
        let mut rng = test_rng();
        let num_nodes = 3;
        let threshold = 3;

        println!("=== SETUP PHASE ===");

        // 1. Generate secret and public witness
        let secret_bytes = [42u8; 32]; // Simple test secret
        let secret = Fr::from_le_bytes_mod_order(&secret_bytes);
        println!("Secret (as field element): {}", secret);

        // 2. Generate public witness and commitment
        let public_witness = b"my_custom_proof_message";
        let salt = [99u8; SALT_SIZE];
        let witness_commitment = commit_witness(public_witness, &salt);
        println!(
            "Witness commitment: {:?}",
            hex::encode(&witness_commitment.hash[..8])
        );

        // 3. Generate random generator
        let generator = G1Projective::rand(&mut rng).into_affine();
        println!("Generator: {:?}", generator);

        // 4. Compute public key
        let public_key = (G1Projective::from(generator) * secret).into_affine();
        println!("Public key: {:?}", public_key);

        // 5. Split secret into shares
        let share_set = share_secret(secret, num_nodes, threshold, &mut rng);
        println!("Created {} shares with threshold {}", num_nodes, threshold);
        for share in &share_set.shares {
            println!("  Share {}: x={}, y={}", share.index, share.x, share.y);
        }

        println!("\n=== PHASE 1: COMMITMENT GENERATION ===");

        // Each node generates a commitment
        let mut node_commitments: Vec<(u32, Commitment)> = Vec::new();
        for share in &share_set.shares {
            let commitment = Commitment::generate(share.index, &generator, &mut rng);
            println!(
                "Node {} commitment: C_{} = g^r_{}",
                share.index, share.index, share.index
            );
            node_commitments.push((share.index as u32, commitment));
        }

        println!("\n=== PHASE 2: AGGREGATION & CHALLENGE ===");

        // Sort by node ID (x-coordinate)
        node_commitments.sort_by_key(|(id, _)| *id);

        let x_coords: Vec<Fr> = node_commitments
            .iter()
            .map(|(id, _)| Fr::from(*id as u64))
            .collect();

        println!(
            "x_coords: {:?}",
            x_coords.iter().map(|x| x.to_string()).collect::<Vec<_>>()
        );

        // Aggregate commitments using Lagrange
        let mut agg_commitment = G1Projective::zero();
        for (i, (node_id, commitment)) in node_commitments.iter().enumerate() {
            let coeff = lagrange_coefficient(&x_coords, i);
            println!(
                "Node {} (x={}): Lagrange coeff = {}",
                node_id, node_id, coeff
            );
            agg_commitment += G1Projective::from(commitment.point) * coeff;
        }
        let agg_commitment_affine = agg_commitment.into_affine();
        println!("Aggregated commitment: {:?}", agg_commitment_affine);

        // Generate challenge from commitment
        let session_id = "test-session";
        let challenge = generate_challenge_from_commitment(
            &generator,
            &witness_commitment,
            &agg_commitment_affine,
            session_id,
        )
        .expect("Challenge generation should succeed");
        println!("Challenge: {}", challenge);

        println!("\n=== PHASE 3: FRAGMENT GENERATION ===");

        // Each node computes response fragment
        let mut fragments: Vec<(u32, Fr)> = Vec::new();
        for (i, (node_id, commitment)) in node_commitments.iter().enumerate() {
            let share = &share_set.shares[i];
            // response = nonce + challenge * share
            let response = commitment.nonce() + (challenge * share.y);
            println!(
                "Node {} response: z_{} = r_{} + c * s_{} = {}",
                node_id, node_id, node_id, node_id, response
            );
            fragments.push((*node_id, response));
        }

        println!("\n=== PHASE 4: RESPONSE AGGREGATION ===");

        // Aggregate responses using Lagrange
        fragments.sort_by_key(|(id, _)| *id);

        let fragment_x_coords: Vec<Fr> = fragments
            .iter()
            .map(|(id, _)| Fr::from(*id as u64))
            .collect();

        assert_eq!(x_coords, fragment_x_coords, "x_coords must match!");

        let mut agg_response = Fr::zero();
        for (i, (node_id, response)) in fragments.iter().enumerate() {
            let coeff = lagrange_coefficient(&x_coords, i);
            println!(
                "Node {} fragment: coeff * z_{} = {} * {}",
                node_id, node_id, coeff, response
            );
            agg_response += *response * coeff;
        }
        println!("Aggregated response: {}", agg_response);

        println!("\n=== VERIFICATION ===");

        // Step 1: Verify commitment
        let commitment_valid = verify_commitment(public_witness, &salt, &witness_commitment);
        println!(
            "Commitment verification: {}",
            if commitment_valid { "PASS" } else { "FAIL" }
        );
        assert!(commitment_valid, "Commitment should verify");

        // Step 2: Verify Schnorr equation: g^z == C * PK^c
        let lhs = (generator * agg_response).into_affine();
        let rhs = (agg_commitment_affine + (public_key * challenge)).into_affine();

        println!("LHS (g^z): {:?}", lhs);
        println!("RHS (C * PK^c): {:?}", rhs);

        let proof_valid = lhs == rhs;
        println!(
            "Schnorr verification: {}",
            if proof_valid { "PASS" } else { "FAIL" }
        );

        // Debug: Let's verify the math step by step
        if !proof_valid {
            println!("\n=== DEBUG: MATHEMATICAL ANALYSIS ===");

            // Verify Lagrange interpolation reconstructs secret
            let mut reconstructed_secret = Fr::zero();
            for (i, share) in share_set.shares.iter().enumerate() {
                let coeff = lagrange_coefficient(&x_coords, i);
                reconstructed_secret += share.y * coeff;
            }
            println!("Reconstructed secret from shares: {}", reconstructed_secret);
            println!("Original secret: {}", secret);
            println!("Secrets match: {}", reconstructed_secret == secret);

            // Verify aggregated nonce
            let mut agg_nonce = Fr::zero();
            for (i, (_, commitment)) in node_commitments.iter().enumerate() {
                let coeff = lagrange_coefficient(&x_coords, i);
                agg_nonce += commitment.nonce() * coeff;
            }
            println!("Aggregated nonce: {}", agg_nonce);

            // Expected: agg_response = agg_nonce + challenge * secret
            let expected_response = agg_nonce + challenge * secret;
            println!("Expected response (agg_nonce + c*s): {}", expected_response);
            println!("Actual response: {}", agg_response);
            println!("Responses match: {}", expected_response == agg_response);

            // Verify commitment aggregation
            let mut expected_agg_commitment = G1Projective::zero();
            for (i, (_, commitment)) in node_commitments.iter().enumerate() {
                let coeff = lagrange_coefficient(&x_coords, i);
                expected_agg_commitment +=
                    G1Projective::from(generator) * (commitment.nonce() * coeff);
            }
            let expected_agg_commitment_affine = expected_agg_commitment.into_affine();
            println!(
                "Expected agg commitment from nonces: {:?}",
                expected_agg_commitment_affine
            );
            println!("Actual agg commitment: {:?}", agg_commitment_affine);
        }

        assert!(proof_valid, "Schnorr proof should verify");
        println!("\n FULL PROTOCOL TEST PASSED!");
    }

    /// Test that Lagrange coefficients work correctly
    #[test]
    fn test_lagrange_reconstruction() {
        let mut rng = test_rng();

        // Create a random secret
        let secret = Fr::rand(&mut rng);

        // Split into 3 shares with threshold 3
        let share_set = share_secret(secret, 3, 3, &mut rng);

        // Reconstruct using Lagrange
        let x_coords: Vec<Fr> = share_set
            .shares
            .iter()
            .map(|s| Fr::from(s.index as u64))
            .collect();

        let mut reconstructed = Fr::zero();
        for (i, share) in share_set.shares.iter().enumerate() {
            let coeff = lagrange_coefficient(&x_coords, i);
            reconstructed += share.y * coeff;
        }

        assert_eq!(
            secret, reconstructed,
            "Lagrange reconstruction should recover secret"
        );
    }

    /// Test basic Schnorr proof without distribution
    #[test]
    fn test_basic_schnorr_proof() {
        let mut rng = test_rng();

        // Setup
        let secret = Fr::rand(&mut rng);
        let generator = G1Projective::rand(&mut rng).into_affine();
        let public_key = (generator * secret).into_affine();

        // Prover generates commitment
        let nonce = Fr::rand(&mut rng);
        let commitment = (generator * nonce).into_affine();

        // Challenge (simplified, not using commitment hash)
        let challenge = Fr::rand(&mut rng);

        // Response
        let response = nonce + challenge * secret;

        // Verification: g^z == C * PK^c
        let lhs = (generator * response).into_affine();
        let rhs = (commitment + (public_key * challenge)).into_affine();

        assert_eq!(lhs, rhs, "Basic Schnorr proof should verify");
    }
}
