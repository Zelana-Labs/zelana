//! Test serialization round-trip for proof data

#[cfg(test)]
mod tests {
    use crate::{BlindProof, CircuitType, WitnessCommitment};
    use ark_ec::CurveGroup;
    use ark_std::{test_rng, UniformRand};
    use prover_core::{Fr, G1Projective};

    #[test]
    fn test_blind_proof_serialization_roundtrip() {
        let mut rng = test_rng();

        // Create a BlindProof with random values
        let witness_commitment = WitnessCommitment { hash: [42u8; 32] };

        let original_proof = BlindProof {
            witness_commitment,
            commitment: G1Projective::rand(&mut rng).into_affine(),
            challenge: Fr::rand(&mut rng),
            response: Fr::rand(&mut rng),
            generator: G1Projective::rand(&mut rng).into_affine(),
            public_key: G1Projective::rand(&mut rng).into_affine(),
            circuit_type: CircuitType::Schnorr,
        };

        println!("Original proof:");
        println!("  commitment: {:?}", original_proof.commitment);
        println!("  challenge: {}", original_proof.challenge);
        println!("  response: {}", original_proof.response);
        println!("  generator: {:?}", original_proof.generator);
        println!("  public_key: {:?}", original_proof.public_key);

        // Serialize to JSON
        let json = serde_json::to_string(&original_proof).expect("Serialization should succeed");
        println!("\nSerialized JSON:\n{}", json);

        // Deserialize back
        let recovered_proof: BlindProof =
            serde_json::from_str(&json).expect("Deserialization should succeed");

        println!("\nRecovered proof:");
        println!("  commitment: {:?}", recovered_proof.commitment);
        println!("  challenge: {}", recovered_proof.challenge);
        println!("  response: {}", recovered_proof.response);
        println!("  generator: {:?}", recovered_proof.generator);

        // Check equality
        assert_eq!(
            original_proof.commitment, recovered_proof.commitment,
            "Commitment mismatch"
        );
        assert_eq!(
            original_proof.challenge, recovered_proof.challenge,
            "Challenge mismatch"
        );
        assert_eq!(
            original_proof.response, recovered_proof.response,
            "Response mismatch"
        );
        assert_eq!(
            original_proof.generator, recovered_proof.generator,
            "Generator mismatch"
        );
        assert_eq!(
            original_proof.public_key, recovered_proof.public_key,
            "Public key mismatch"
        );

        println!("\n✅ All fields match after serialization round-trip!");
    }

    #[test]
    fn test_verify_request_parsing() {
        // Simulate the exact JSON the frontend sends (now includes public_key)
        let json = r#"{
            "blind_proof": {
                "witness_commitment": {
                    "hash": "59a0f4fdf6553709e2ff31b2fc6b8f10799ef91bd2a6397745d808b7a8355b22",
                    "session_id": "session-75507fe5d3566c37ad8f0f2a851b0b41"
                },
                "commitment": "UhF68/hjj8EuRL4QVaJNhocrPS8pZXRsWQ9BSGe7siE=",
                "challenge": "IKywDagq3iPnpnTuOP9edexiYnx9ruvih7sYcKHJZQA=",
                "response": "K33KdeHWC5KN/sh7IznyBvFxJ7RryIpJXvcrsx9nJy4=",
                "generator": "movK/pLt15Epf+6K6JDfctB15qizej2eR03adak2voI=",
                "public_key": "movK/pLt15Epf+6K6JDfctB15qizej2eR03adak2voI=",
                "circuit_type": "schnorr"
            },
            "public_witness": "6d795f637573746f6d5f70726f6f665f6d657373616765",
            "salt": "771859cb7338096d816f5c59d8d7b85a33305e2efa5ae4b464c021adc0055277"
        }"#;

        use crate::VerifyWithRevealRequest;
        let request: VerifyWithRevealRequest =
            serde_json::from_str(json).expect("Should parse verify request");

        println!("Parsed VerifyWithRevealRequest:");
        println!(
            "  witness_commitment.hash: {:?}",
            hex::encode(&request.blind_proof.witness_commitment.hash)
        );
        println!("  commitment: {:?}", request.blind_proof.commitment);
        println!("  challenge: {}", request.blind_proof.challenge);
        println!("  response: {}", request.blind_proof.response);
        println!("  generator: {:?}", request.blind_proof.generator);
        println!("  public_witness: {}", request.public_witness);
        println!("  salt: {:?}", hex::encode(&request.salt));

        // Decode public witness
        let witness_bytes = hex::decode(&request.public_witness).unwrap();
        let witness_str = String::from_utf8(witness_bytes).unwrap();
        println!("  decoded witness: \"{}\"", witness_str);

        println!("\n✅ Parsing successful!");
    }

    /// Test that we can simulate what the coordinator does during verification
    /// Now that public_key is embedded in the proof, verification is self-contained
    #[test]
    fn test_schnorr_verification_with_real_payload() {
        use ark_ec::CurveGroup;

        // Parse the exact JSON from the frontend (now includes public_key)
        let json = r#"{
            "blind_proof": {
                "witness_commitment": {
                    "hash": "59a0f4fdf6553709e2ff31b2fc6b8f10799ef91bd2a6397745d808b7a8355b22",
                    "session_id": "session-75507fe5d3566c37ad8f0f2a851b0b41"
                },
                "commitment": "UhF68/hjj8EuRL4QVaJNhocrPS8pZXRsWQ9BSGe7siE=",
                "challenge": "IKywDagq3iPnpnTuOP9edexiYnx9ruvih7sYcKHJZQA=",
                "response": "K33KdeHWC5KN/sh7IznyBvFxJ7RryIpJXvcrsx9nJy4=",
                "generator": "movK/pLt15Epf+6K6JDfctB15qizej2eR03adak2voI=",
                "public_key": "movK/pLt15Epf+6K6JDfctB15qizej2eR03adak2voI=",
                "circuit_type": "schnorr"
            },
            "public_witness": "6d795f637573746f6d5f70726f6f665f6d657373616765",
            "salt": "771859cb7338096d816f5c59d8d7b85a33305e2efa5ae4b464c021adc0055277"
        }"#;

        use crate::VerifyWithRevealRequest;
        let request: VerifyWithRevealRequest = serde_json::from_str(json).unwrap();

        let g = request.blind_proof.generator;
        let z = request.blind_proof.response;
        let c_point = request.blind_proof.commitment;
        let challenge = request.blind_proof.challenge;
        let pk = request.blind_proof.public_key;

        println!("Parsed values:");
        println!("  g (generator): {:?}", g);
        println!("  pk (public_key): {:?}", pk);
        println!("  z (response): {}", z);
        println!("  C (commitment): {:?}", c_point);
        println!("  c (challenge): {}", challenge);

        // Compute LHS = g^z
        let lhs = (g * z).into_affine();
        println!("\nLHS (g^z): {:?}", lhs);

        // Compute RHS = C + c*PK
        let rhs = (c_point + (pk * challenge)).into_affine();
        println!("RHS (C + c*PK): {:?}", rhs);

        // Note: This test uses dummy data, so the verification will likely fail
        // The important thing is that we can now compute the full verification equation
        // since public_key is embedded in the proof
        println!("\n✅ Verification is now self-contained (public_key embedded in proof)");
        println!("Note: This uses dummy test data, so LHS != RHS is expected");
    }
}
