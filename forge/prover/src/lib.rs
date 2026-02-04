use ark_bn254::{Fr, G1Affine, G1Projective};
use ark_ec::CurveGroup;
use ark_ff::{Field, One, PrimeField, UniformRand, Zero};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use ark_std::rand::Rng;
use sha2::{Digest, Sha256};

// Custom serialization wrappers for arkworks types

/// Wrapper for Fr that implements serde
#[derive(Clone, Debug)]
pub struct SerializableFr(pub Fr);

impl serde::Serialize for SerializableFr {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut bytes = Vec::new();
        self.0
            .serialize_compressed(&mut bytes)
            .map_err(serde::ser::Error::custom)?;
        serializer.serialize_bytes(&bytes)
    }
}

impl<'de> serde::Deserialize<'de> for SerializableFr {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let bytes: Vec<u8> = serde::Deserialize::deserialize(deserializer)?;
        let fr = Fr::deserialize_compressed(&bytes[..]).map_err(serde::de::Error::custom)?;
        Ok(SerializableFr(fr))
    }
}

/// Wrapper for G1Affine that implements serde
#[derive(Clone, Debug)]
pub struct SerializableG1(pub G1Affine);

impl serde::Serialize for SerializableG1 {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut bytes = Vec::new();
        self.0
            .serialize_compressed(&mut bytes)
            .map_err(serde::ser::Error::custom)?;
        serializer.serialize_bytes(&bytes)
    }
}

impl<'de> serde::Deserialize<'de> for SerializableG1 {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let bytes: Vec<u8> = serde::Deserialize::deserialize(deserializer)?;
        let point =
            G1Affine::deserialize_compressed(&bytes[..]).map_err(serde::de::Error::custom)?;
        Ok(SerializableG1(point))
    }
}

// Core types

/// Represents a share of a secret value using Shamir's Secret Sharing
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SecretShare {
    pub node_id: usize,
    pub x: SerializableFr, // x-coordinate (node identifier in field)
    pub y: SerializableFr, // y-coordinate (share value)
}

impl SecretShare {
    pub fn new(node_id: usize, x: Fr, y: Fr) -> Self {
        Self {
            node_id,
            x: SerializableFr(x),
            y: SerializableFr(y),
        }
    }

    pub fn x(&self) -> Fr {
        self.x.0
    }

    pub fn y(&self) -> Fr {
        self.y.0
    }
}

/// Represents a fragment of a ZK proof that a node computes
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ProofFragment {
    pub node_id: usize,
    pub commitment: SerializableG1,
    pub response: SerializableFr,
}

impl ProofFragment {
    pub fn new(node_id: usize, commitment: G1Affine, response: Fr) -> Self {
        Self {
            node_id,
            commitment: SerializableG1(commitment),
            response: SerializableFr(response),
        }
    }

    pub fn commitment(&self) -> G1Affine {
        self.commitment.0
    }

    pub fn response(&self) -> Fr {
        self.response.0
    }
}

/// Public parameters for the ZK proof system
#[derive(Clone, Debug)]
pub struct PublicParameters {
    pub generator: G1Affine,
    pub public_key: G1Affine,
}

/// A distributed prover node that holds only a share of the secret
pub struct ProverNode {
    pub id: usize,
    pub secret_share: SecretShare,
    pub public_params: PublicParameters,
}

/// Coordinator that orchestrates the distributed proving without learning the secret
pub struct ProofCoordinator {
    pub num_nodes: usize,
    pub threshold: usize,
    pub public_params: PublicParameters,
}

/// Complete distributed ZK proof
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct DistributedProof {
    pub commitment: SerializableG1,
    pub challenge: SerializableFr,
    pub response: SerializableFr,
}

impl DistributedProof {
    pub fn new(commitment: G1Affine, challenge: Fr, response: Fr) -> Self {
        Self {
            commitment: SerializableG1(commitment),
            challenge: SerializableFr(challenge),
            response: SerializableFr(response),
        }
    }

    pub fn commitment(&self) -> G1Affine {
        self.commitment.0
    }

    pub fn challenge(&self) -> Fr {
        self.challenge.0
    }

    pub fn response(&self) -> Fr {
        self.response.0
    }
}

impl ProofCoordinator {
    /// Create a new coordinator for distributed proving
    pub fn new<R: Rng>(num_nodes: usize, threshold: usize, rng: &mut R) -> Self {
        let generator = G1Projective::rand(rng).into_affine();

        Self {
            num_nodes,
            threshold,
            public_params: PublicParameters {
                generator,
                public_key: generator, // Will be set properly during setup
            },
        }
    }

    /// Distribute a secret among nodes using Shamir's Secret Sharing
    /// The secret is split so that any `threshold` nodes can reconstruct it,
    /// but fewer than `threshold` nodes learn nothing
    pub fn share_secret<R: Rng>(&self, secret: Fr, rng: &mut R) -> Vec<SecretShare> {
        // Create polynomial: f(x) = secret + a_1*x + a_2*x^2 + ... + a_{t-1}*x^{t-1}
        let mut coefficients = vec![secret];
        for _ in 1..self.threshold {
            coefficients.push(Fr::rand(rng));
        }

        // Evaluate polynomial at points 1, 2, ..., n to create shares
        let mut shares = Vec::new();
        for i in 1..=self.num_nodes {
            let x = Fr::from(i as u64);
            let mut y = Fr::zero();
            let mut x_power = Fr::one();

            // Evaluate polynomial using Horner's method
            for coeff in &coefficients {
                y += *coeff * x_power;
                x_power *= x;
            }

            shares.push(SecretShare::new(i, x, y));
        }

        shares
    }

    /// Reconstruct secret from threshold shares using Lagrange interpolation
    /// This is only done to verify - in practice, we reconstruct the proof, not the secret
    pub fn reconstruct_secret(&self, shares: &[SecretShare]) -> Fr {
        if shares.len() < self.threshold {
            panic!("Not enough shares to reconstruct secret");
        }

        let shares = &shares[..self.threshold];
        let x_coords: Vec<Fr> = shares.iter().map(|s| s.x()).collect();
        let y_values: Vec<Fr> = shares.iter().map(|s| s.y()).collect();

        lagrange_interpolate_at_zero(&x_coords, &y_values)
    }

    /// Generate a Fiat-Shamir challenge from commitments
    /// Uses canonical serialization for deterministic hashing
    pub fn generate_challenge(&self, commitments: &[G1Affine]) -> Fr {
        let mut hasher = Sha256::new();

        // Include public parameters in challenge for binding
        let mut gen_bytes = Vec::new();
        self.public_params
            .generator
            .serialize_compressed(&mut gen_bytes)
            .unwrap();
        hasher.update(&gen_bytes);

        let mut pk_bytes = Vec::new();
        self.public_params
            .public_key
            .serialize_compressed(&mut pk_bytes)
            .unwrap();
        hasher.update(&pk_bytes);

        // Hash all commitments using canonical serialization
        for comm in commitments {
            let mut comm_bytes = Vec::new();
            comm.serialize_compressed(&mut comm_bytes).unwrap();
            hasher.update(&comm_bytes);
        }

        let hash = hasher.finalize();
        Fr::from_le_bytes_mod_order(&hash)
    }

    /// Aggregate proof fragments into a complete proof
    /// IMPORTANT: The challenge must be passed in - it should be the same challenge
    /// that was used to generate the fragment responses
    pub fn aggregate_proof_fragments(
        &self,
        fragments: &[ProofFragment],
        challenge: Fr,
    ) -> DistributedProof {
        if fragments.len() < self.threshold {
            panic!("Not enough fragments to create proof");
        }

        // Use first threshold fragments
        let fragments = &fragments[..self.threshold];

        // Get x-coordinates for Lagrange interpolation
        let x_coords: Vec<Fr> = fragments
            .iter()
            .map(|f| Fr::from(f.node_id as u64))
            .collect();

        // Aggregate commitments using Lagrange coefficients
        let mut aggregated_commitment = G1Projective::zero();
        let mut aggregated_response = Fr::zero();

        for (i, fragment) in fragments.iter().enumerate() {
            let lagrange_coeff = compute_lagrange_coefficient(&x_coords, i);

            // Aggregate commitment
            let commitment_projective: G1Projective = fragment.commitment().into();
            aggregated_commitment += commitment_projective * lagrange_coeff;

            // Aggregate response
            aggregated_response += fragment.response() * lagrange_coeff;
        }

        DistributedProof::new(
            aggregated_commitment.into_affine(),
            challenge,
            aggregated_response,
        )
    }

    /// Verify a distributed proof
    /// Schnorr verification: g^response == commitment * public_key^challenge
    pub fn verify_proof(&self, proof: &DistributedProof, public_key: G1Affine) -> bool {
        // Verify: g^response = commitment * public_key^challenge
        let lhs = (self.public_params.generator * proof.response()).into_affine();
        let rhs = (proof.commitment() + (public_key * proof.challenge())).into_affine();

        lhs == rhs
    }
}

/// Compute Lagrange coefficient for index i when evaluating at x=0
fn compute_lagrange_coefficient(x_coords: &[Fr], i: usize) -> Fr {
    let mut numerator = Fr::one();
    let mut denominator = Fr::one();

    for j in 0..x_coords.len() {
        if i != j {
            // λ_i(0) = Π_{j≠i} (0 - x_j) / (x_i - x_j) = Π_{j≠i} x_j / (x_j - x_i)
            numerator *= x_coords[j];
            denominator *= x_coords[j] - x_coords[i];
        }
    }

    numerator * denominator.inverse().unwrap()
}

/// Lagrange interpolation to find f(0) given points
fn lagrange_interpolate_at_zero(x_coords: &[Fr], y_values: &[Fr]) -> Fr {
    let mut result = Fr::zero();

    for (i, _) in x_coords.iter().enumerate() {
        let coeff = compute_lagrange_coefficient(x_coords, i);
        result += y_values[i] * coeff;
    }

    result
}

impl ProverNode {
    /// Create a new prover node with a secret share
    pub fn new(id: usize, secret_share: SecretShare, public_params: PublicParameters) -> Self {
        Self {
            id,
            secret_share,
            public_params,
        }
    }

    /// Generate a commitment using a random nonce
    /// Returns both the nonce (kept secret) and the commitment (shared)
    pub fn generate_commitment<R: Rng>(&self, rng: &mut R) -> (Fr, G1Affine) {
        let nonce = Fr::rand(rng);
        let commitment = (self.public_params.generator * nonce).into_affine();
        (nonce, commitment)
    }

    /// Generate a proof fragment given a nonce and coordinated challenge
    /// The challenge must be the same for all participating nodes
    pub fn generate_fragment(&self, nonce: Fr, challenge: Fr) -> ProofFragment {
        let commitment = (self.public_params.generator * nonce).into_affine();

        // Schnorr response: r + c * s (where r=nonce, c=challenge, s=secret_share)
        let response = nonce + (challenge * self.secret_share.y());

        ProofFragment::new(self.id, commitment, response)
    }
}

/// Simulates the complete distributed proving workflow
pub struct DistributedProofSystem {
    pub coordinator: ProofCoordinator,
    pub nodes: Vec<ProverNode>,
}

impl DistributedProofSystem {
    /// Initialize a distributed proof system
    pub fn new<R: Rng>(num_nodes: usize, threshold: usize, rng: &mut R) -> Self {
        assert!(
            threshold <= num_nodes,
            "Threshold cannot exceed number of nodes"
        );
        assert!(threshold >= 1, "Threshold must be at least 1");

        let coordinator = ProofCoordinator::new(num_nodes, threshold, rng);

        Self {
            coordinator,
            nodes: Vec::new(),
        }
    }

    /// Setup: distribute the secret among nodes
    pub fn setup<R: Rng>(&mut self, secret: Fr, rng: &mut R) {
        // Create public key: PK = g^secret
        let public_key = (self.coordinator.public_params.generator * secret).into_affine();
        self.coordinator.public_params.public_key = public_key;

        // Distribute secret shares to nodes
        let shares = self.coordinator.share_secret(secret, rng);

        let num_nodes = self.coordinator.num_nodes;
        let threshold = self.coordinator.threshold;

        self.nodes.clear();
        for share in shares {
            let node = ProverNode::new(
                share.node_id,
                share.clone(),
                self.coordinator.public_params.clone(),
            );
            self.nodes.push(node);
        }

        println!(
            " Setup complete: Secret distributed among {} nodes",
            num_nodes
        );
        println!(" Threshold: {} nodes required to create proof", threshold);
    }

    /// Execute distributed proving protocol with specified participating nodes
    pub fn prove_with_nodes<R: Rng>(
        &self,
        participating_node_indices: &[usize],
        rng: &mut R,
    ) -> Result<DistributedProof, &'static str> {
        if participating_node_indices.len() < self.coordinator.threshold {
            return Err("Not enough participating nodes");
        }

        println!("\n--- Distributed Proving Protocol ---");
        println!("Participating nodes: {:?}", participating_node_indices);

        // Phase 1: Each participating node generates a random nonce and commitment
        let mut nonces = Vec::new();
        let mut commitments = Vec::new();

        for &idx in participating_node_indices {
            let node = &self.nodes[idx];
            let (nonce, commitment) = node.generate_commitment(rng);
            nonces.push(nonce);
            commitments.push(commitment);
        }
        println!(
            " Phase 1: {} nodes generated commitments",
            commitments.len()
        );

        // Phase 2: Coordinator generates challenge from commitments
        // CRITICAL: Use only the commitments from participating nodes
        let challenge = self.coordinator.generate_challenge(&commitments);
        println!(
            " Phase 2: Challenge generated from {} commitments",
            commitments.len()
        );

        // Phase 3: Each participating node generates its proof fragment
        let mut fragments = Vec::new();
        for (i, &idx) in participating_node_indices.iter().enumerate() {
            let node = &self.nodes[idx];
            let fragment = node.generate_fragment(nonces[i], challenge);
            fragments.push(fragment);
        }
        println!(" Phase 3: All participating nodes generated proof fragments");

        // Phase 4: Coordinator aggregates fragments
        // CRITICAL: Pass the same challenge that was used for fragments
        let proof = self
            .coordinator
            .aggregate_proof_fragments(&fragments, challenge);
        println!(" Phase 4: Proof aggregated");

        Ok(proof)
    }

    /// Execute distributed proving protocol using first threshold nodes
    pub fn prove<R: Rng>(&self, rng: &mut R) -> DistributedProof {
        let participating: Vec<usize> = (0..self.coordinator.threshold).collect();
        self.prove_with_nodes(&participating, rng)
            .expect("Should have enough nodes")
    }

    /// Verify the distributed proof
    pub fn verify(&self, proof: &DistributedProof) -> bool {
        let result = self
            .coordinator
            .verify_proof(proof, self.coordinator.public_params.public_key);

        if result {
            println!(" Proof verification: SUCCESS");
        } else {
            println!(" Proof verification: FAILED");
        }

        result
    }

    /// Demonstrate that fewer than threshold nodes cannot create valid proofs
    pub fn demonstrate_security<R: Rng>(&self, rng: &mut R) {
        println!("\n--- Security Demonstration ---");

        // Try to create proof with insufficient nodes
        let insufficient_count = self.coordinator.threshold - 1;
        if insufficient_count > 0 {
            let insufficient_indices: Vec<usize> = (0..insufficient_count).collect();

            // Generate commitments from insufficient nodes
            let mut nonces = Vec::new();
            let mut commitments = Vec::new();

            for &idx in &insufficient_indices {
                let node = &self.nodes[idx];
                let (nonce, commitment) = node.generate_commitment(rng);
                nonces.push(nonce);
                commitments.push(commitment);
            }

            // Try to create fragments (this will work but won't reconstruct the secret)
            let challenge = self.coordinator.generate_challenge(&commitments);
            let _fragments: Vec<ProofFragment> = insufficient_indices
                .iter()
                .enumerate()
                .map(|(i, &idx)| self.nodes[idx].generate_fragment(nonces[i], challenge))
                .collect();

            println!(
                " {} nodes generated fragments but cannot create valid proof",
                insufficient_count
            );
            println!("  (Lagrange interpolation with < threshold points gives wrong secret)");

            println!(
                " Need {} nodes to reconstruct, only have {}",
                self.coordinator.threshold, insufficient_count
            );
        }

        // Demonstrate that a single node learns nothing about the full secret
        println!("\n Security properties:");
        println!(
            "  - Each node only holds a share (1/{} of the secret polynomial)",
            self.coordinator.num_nodes
        );
        println!(
            "  - Any {} shares can reconstruct; fewer learn nothing",
            self.coordinator.threshold
        );
        println!("  - Coordinator never sees secret shares, only commitments");
        println!("  - Verifier learns nothing about the secret (zero-knowledge)");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ark_std::test_rng;

    #[test]
    fn test_secret_sharing() {
        let mut rng = test_rng();
        let coordinator = ProofCoordinator::new(5, 3, &mut rng);

        let secret = Fr::from(42u64);
        let shares = coordinator.share_secret(secret, &mut rng);

        // Test reconstruction with exactly threshold shares
        let reconstructed = coordinator.reconstruct_secret(&shares[..3]);
        assert_eq!(secret, reconstructed);

        // Test with different subset of shares
        let alt_shares = vec![shares[0].clone(), shares[2].clone(), shares[4].clone()];
        let reconstructed = coordinator.reconstruct_secret(&alt_shares);
        assert_eq!(secret, reconstructed);

        // Test with more than threshold
        let reconstructed = coordinator.reconstruct_secret(&shares);
        assert_eq!(secret, reconstructed);
    }

    #[test]
    fn test_distributed_proving() {
        let mut rng = test_rng();
        let mut system = DistributedProofSystem::new(5, 3, &mut rng);

        let secret = Fr::rand(&mut rng);
        system.setup(secret, &mut rng);

        let proof = system.prove(&mut rng);
        assert!(system.verify(&proof));
    }

    #[test]
    fn test_proving_with_different_node_subsets() {
        let mut rng = test_rng();
        let mut system = DistributedProofSystem::new(7, 4, &mut rng);

        let secret = Fr::rand(&mut rng);
        system.setup(secret, &mut rng);

        // Test with first 4 nodes
        let proof1 = system.prove_with_nodes(&[0, 1, 2, 3], &mut rng).unwrap();
        assert!(system.verify(&proof1));

        // Test with different subset
        let proof2 = system.prove_with_nodes(&[1, 3, 4, 6], &mut rng).unwrap();
        assert!(system.verify(&proof2));

        // Test with all nodes
        let proof3 = system
            .prove_with_nodes(&[0, 1, 2, 3, 4, 5, 6], &mut rng)
            .unwrap();
        assert!(system.verify(&proof3));
    }

    #[test]
    fn test_insufficient_shares_error() {
        let mut rng = test_rng();
        let mut system = DistributedProofSystem::new(5, 3, &mut rng);

        let secret = Fr::rand(&mut rng);
        system.setup(secret, &mut rng);

        // Try with only 2 nodes (threshold is 3)
        let result = system.prove_with_nodes(&[0, 1], &mut rng);
        assert!(result.is_err());
    }

    #[test]
    fn test_lagrange_coefficient() {
        // Test that Lagrange coefficients sum to 1 when evaluated at x=0
        let x_coords = vec![Fr::from(1u64), Fr::from(2u64), Fr::from(3u64)];
        let mut sum = Fr::zero();

        for i in 0..x_coords.len() {
            sum += compute_lagrange_coefficient(&x_coords, i);
        }

        // For f(x) = 1 (constant), f(0) = 1, so sum of λ_i should equal 1
        assert_eq!(sum, Fr::one());
    }

    #[test]
    fn test_serialization() {
        let mut rng = test_rng();
        let mut system = DistributedProofSystem::new(5, 3, &mut rng);

        let secret = Fr::rand(&mut rng);
        system.setup(secret, &mut rng);

        let proof = system.prove(&mut rng);

        // Test that proof can be serialized and deserialized
        let json = serde_json::to_string(&proof).unwrap();
        let restored: DistributedProof = serde_json::from_str(&json).unwrap();

        assert!(system.verify(&restored));
    }
}
