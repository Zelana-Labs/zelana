use ark_bn254::Fr;
use ark_ff::UniformRand;
use ark_std::rand::SeedableRng;
use ark_std::rand::rngs::StdRng;
use prover::*;

/// Get a random seed from the operating system
fn get_random_seed() -> [u8; 32] {
    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed).expect("Failed to get random bytes from OS");
    seed
}

fn main() {
    println!("=================================================");
    println!("   Distributed Zero-Knowledge Proof System");
    println!("=================================================\n");

    // Initialize random number generator
    // Using StdRng seeded from system entropy
    let mut rng = StdRng::from_seed(get_random_seed());

    // Configuration
    let num_nodes = 7;
    let threshold = 4;

    println!("Configuration:");
    println!("  - Total Nodes: {}", num_nodes);
    println!("  - Threshold: {} nodes required", threshold);
    println!("  - Security: No single node knows the complete secret\n");

    // Create the distributed proof system
    let mut system = DistributedProofSystem::new(num_nodes, threshold, &mut rng);

    // Generate a secret
    let secret = Fr::rand(&mut rng);
    println!("Secret generated: <hidden>\n");

    // Setup: Distribute the secret among nodes
    system.setup(secret, &mut rng);

    // Execute proving protocol with first threshold nodes
    println!("\n=================================================");
    println!("   Executing Distributed Proving Protocol");
    println!("=================================================");

    let proof = system.prove(&mut rng);

    // Verify the proof
    println!("\n=================================================");
    println!("   Verifying Distributed Proof");
    println!("=================================================");

    let is_valid = system.verify(&proof);

    if is_valid {
        println!("\n✓ SUCCESS: Proof is valid!");
        println!("  - No single node had the complete secret");
        println!("  - {} nodes collaborated to create the proof", threshold);
        println!("  - The proof convinces the verifier without revealing the secret");
    } else {
        println!("\n✗ FAILURE: Proof verification failed!");
    }

    // Test with a different subset of nodes
    println!("\n=================================================");
    println!("   Testing with Different Node Subset");
    println!("=================================================");

    // Use nodes 2, 4, 5, 6 (indices 1, 3, 4, 5 - 0-indexed)
    let alt_nodes = vec![1, 3, 4, 5];
    println!(
        "\nProving with nodes {:?}...",
        alt_nodes.iter().map(|&i| i + 1).collect::<Vec<_>>()
    );

    let proof2 = system
        .prove_with_nodes(&alt_nodes, &mut rng)
        .expect("Should succeed with threshold nodes");

    let is_valid2 = system.verify(&proof2);
    if is_valid2 {
        println!("\n✓ SUCCESS: Alternative subset also produces valid proof!");
    }

    // Demonstrate security properties
    system.demonstrate_security(&mut rng);

    println!("\n=================================================");
    println!("   Key Security Properties");
    println!("=================================================");
    println!("✓ Trustless: No single node can be trusted with the secret");
    println!(
        "✓ Threshold Security: Requires {} out of {} nodes",
        threshold, num_nodes
    );
    println!("✓ Privacy: Secret remains hidden from all parties");
    println!("✓ Verifiable: Anyone can verify the proof");
    println!("✓ Non-Interactive: Uses Fiat-Shamir transform");

    println!("\n=================================================");
    println!("   Privacy Guarantees");
    println!("=================================================");
    println!("• The secret is never reconstructed during proving");
    println!("• Each node only sees its own share");
    println!("• The coordinator only sees commitments, not shares");
    println!("• The verifier learns only that the prover knows the secret");
    println!(
        "• Even a malicious subset of < {} nodes learns nothing",
        threshold
    );

    println!("\n=================================================\n");
}
