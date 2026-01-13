use ark_bn254::Fr;
use ark_r1cs_std::fields::fp::FpVar;
use ark_relations::r1cs::{ConstraintSystemRef, SynthesisError};

use ark_crypto_primitives::sponge::{
    constraints::CryptographicSpongeVar, poseidon::constraints::PoseidonSpongeVar,
};

use super::poseidon::poseidon_config;

/// Poseidon hash of two field elements INSIDE the circuit
pub fn hash2(
    cs: ConstraintSystemRef<Fr>,
    a: &FpVar<Fr>,
    b: &FpVar<Fr>,
) -> Result<FpVar<Fr>, SynthesisError> {
    // IMPORTANT: bind config to avoid temporary borrow issues
    let config = poseidon_config();

    let mut sponge = PoseidonSpongeVar::new(cs, &config);
    sponge.absorb(a)?;
    sponge.absorb(b)?;

    Ok(sponge.squeeze_field_elements(1)?[0].clone())
}
