#[cfg(test)]
mod tests {
    use crate::{Fr, G1Projective};
    use ark_ec::CurveGroup;
    use ark_std::{test_rng, UniformRand};

    #[test]
    fn test_rng_determinism() {
        // First call to test_rng
        let mut rng1 = test_rng();
        let g1 = G1Projective::rand(&mut rng1).into_affine();
        let s1 = Fr::rand(&mut rng1);

        // Second call to test_rng (simulating another request)
        let mut rng2 = test_rng();
        let g2 = G1Projective::rand(&mut rng2).into_affine();
        let s2 = Fr::rand(&mut rng2);

        println!("Generator 1: {:?}", g1);
        println!("Generator 2: {:?}", g2);
        println!("Same generator? {}", g1 == g2);

        println!("\nFr 1: {}", s1);
        println!("Fr 2: {}", s2);
        println!("Same Fr? {}", s1 == s2);

        // CRITICAL: test_rng() always returns same sequence!
        assert!(g1 == g2, "test_rng should be deterministic");
    }
}
