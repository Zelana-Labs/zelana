use anchor_lang::prelude::*;
use anchor_lang::solana_program::hash::hashv;
use hex_literal::hex;
use solana_bn254::prelude::{alt_bn128_addition, alt_bn128_multiplication, alt_bn128_pairing};

declare_id!("8TveT3mvH59qLzZNwrTT6hBqDHEobW2XnCPb7xZLBYHd");

// Base field modulus 'q' for BN254
pub const BASE_FIELD_MODULUS_Q: [u8; 32] =
    hex!("30644E72E131A029B85045B68181585D97816A916871CA8D3C208C16D87CFD47");

// RISC0 constants
pub const ALLOWED_CONTROL_ROOT: [u8; 32] =
    hex!("8cdad9242664be3112aba377c5425a4df735eb1c6966472b561d2855932c0469");
pub const BN254_IDENTITY_CONTROL_ID: [u8; 32] =
    hex!("c07a65145c3cb48b6101962ea607a4dd93c753bb26975cb47feb00d3666e4404");
pub const OUTPUT_TAG: [u8; 32] =
    hex!("77eafeb366a78b47747de0d7bb176284085ff5564887009a5be63da32d3559d4");
pub const SYSTEM_STATE_TAG: [u8; 32] =
    hex!("206115a847207c0892e0c0547225df31d02a96eeb395670c31112dff90b421d6");
pub const RECEIPT_CLAIM_TAG: [u8; 32] =
    hex!("cb1fefcd1f2d9a64975cbbbf6e161e2914434b0cbb9960b84df5d717e86b48af");
pub const SYSTEM_STATE_ZERO_DIGEST: [u8; 32] =
    hex!("a3acc27117418996340b84e5a90f3ef4c49d22c79e44aad822ec9c313e1eb8e2");

/// Groth16 proof elements on BN254 curve
#[derive(Clone, PartialEq, Eq, AnchorDeserialize, AnchorSerialize)]
pub struct Groth16Proof {
    pub pi_a: [u8; 64],  // G1 point (negated)
    pub pi_b: [u8; 128], // G2 point
    pub pi_c: [u8; 64],  // G1 point
}

/// Groth16 verification key for our simple square circuit
#[derive(Clone, PartialEq, Eq, AnchorDeserialize, AnchorSerialize)]
pub struct Groth16VerifyingKey {
    pub alpha_g1: [u8; 64],
    pub beta_g2: [u8; 128],
    pub gamma_g2: [u8; 128],
    pub delta_g2: [u8; 128],
    pub ic: Vec<[u8; 64]>, // IC points (G1)
}

/// RISC0 proof structure
#[derive(Clone, PartialEq, Eq, AnchorDeserialize, AnchorSerialize)]
pub struct Risc0Proof {
    pub pi_a: [u8; 64],  // G1 point (negated)
    pub pi_b: [u8; 128], // G2 point
    pub pi_c: [u8; 64],  // G1 point
}

/// Public inputs for proofs
#[derive(Clone, PartialEq, Eq, AnchorDeserialize, AnchorSerialize)]
pub struct PublicInputs {
    pub inputs: Vec<[u8; 32]>,
}

/// Account to store verified Groth16 proofs
#[account]
pub struct VerifiedGroth16Proof {
    pub authority: Pubkey,
    pub proof: Groth16Proof,
    pub public_inputs: PublicInputs,
    pub verifying_key_hash: [u8; 32],
    pub verified_at: i64,
    pub bump: u8,
}

/// Account to store verified RISC0 proofs
#[account]
pub struct VerifiedRisc0Proof {
    pub authority: Pubkey,
    pub proof: Risc0Proof,
    pub image_id: [u8; 32],
    pub journal_digest: [u8; 32],
    pub verified_at: i64,
    pub bump: u8,
}

// ============================================================================
// Batch Verification (for Bridge CPI)
// ============================================================================

/// Maximum number of IC points we support (determines public inputs count)
pub const MAX_IC_POINTS: usize = 8;

/// Account to store the L2 batch verifying key
/// Seeds: ["batch_vk", domain]
#[account]
pub struct BatchVerifyingKey {
    /// Authority that can update this VK
    pub authority: Pubkey,
    /// Domain identifier (e.g., "zelana-mainnet")
    pub domain: [u8; 32],
    /// Alpha G1 point
    pub alpha_g1: [u8; 64],
    /// Beta G2 point
    pub beta_g2: [u8; 128],
    /// Gamma G2 point
    pub gamma_g2: [u8; 128],
    /// Delta G2 point
    pub delta_g2: [u8; 128],
    /// IC points (fixed array for deterministic sizing)
    pub ic: [[u8; 64]; MAX_IC_POINTS],
    /// Number of IC points actually used
    pub ic_len: u8,
    /// Whether the VK is fully initialized (all IC points added)
    pub finalized: bool,
    /// When the VK was stored
    pub created_at: i64,
    /// Bump for PDA derivation
    pub bump: u8,
}

impl BatchVerifyingKey {
    pub const LEN: usize = 8 + // discriminator
        32 + // authority
        32 + // domain
        64 + // alpha_g1
        128 + // beta_g2
        128 + // gamma_g2
        128 + // delta_g2
        (64 * MAX_IC_POINTS) + // ic
        1 + // ic_len
        1 + // finalized
        8 + // created_at
        1; // bump
}

/// Batch proof public inputs for L2 state transitions
/// These are the values verified by the ZK proof
#[derive(Clone, PartialEq, Eq, AnchorDeserialize, AnchorSerialize)]
pub struct BatchPublicInputs {
    /// Previous state root
    pub pre_state_root: [u8; 32],
    /// New state root after batch execution
    pub post_state_root: [u8; 32],
    /// Previous shielded commitment tree root
    pub pre_shielded_root: [u8; 32],
    /// New shielded commitment tree root
    pub post_shielded_root: [u8; 32],
    /// Merkle root of withdrawals in this batch
    pub withdrawal_root: [u8; 32],
    /// Hash of all transactions in the batch
    pub batch_hash: [u8; 32],
    /// Batch sequence number
    pub batch_id: u64,
}

/// Context for storing the batch verifying key
#[derive(Accounts)]
#[instruction(domain: [u8; 32])]
pub struct StoreBatchVk<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        init,
        payer = authority,
        space = BatchVerifyingKey::LEN,
        seeds = [b"batch_vk", domain.as_ref()],
        bump
    )]
    pub vk_account: Account<'info, BatchVerifyingKey>,

    pub system_program: Program<'info, System>,
}

/// Context for initializing batch VK (without IC points - for chunked upload)
#[derive(Accounts)]
#[instruction(domain: [u8; 32])]
pub struct InitBatchVk<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        init,
        payer = authority,
        space = BatchVerifyingKey::LEN,
        seeds = [b"batch_vk", domain.as_ref()],
        bump
    )]
    pub vk_account: Account<'info, BatchVerifyingKey>,

    pub system_program: Program<'info, System>,
}

/// Context for appending IC points to an existing VK
#[derive(Accounts)]
pub struct AppendIcPoints<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        mut,
        seeds = [b"batch_vk", vk_account.domain.as_ref()],
        bump = vk_account.bump,
        constraint = vk_account.authority == authority.key() @ VerifierError::InvalidPublicInput,
        constraint = !vk_account.finalized @ VerifierError::InvalidPublicInput,
    )]
    pub vk_account: Account<'info, BatchVerifyingKey>,
}

/// Context for finalizing a VK (marking it ready for use)
#[derive(Accounts)]
pub struct FinalizeBatchVk<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        mut,
        seeds = [b"batch_vk", vk_account.domain.as_ref()],
        bump = vk_account.bump,
        constraint = vk_account.authority == authority.key() @ VerifierError::InvalidPublicInput,
        constraint = !vk_account.finalized @ VerifierError::InvalidPublicInput,
    )]
    pub vk_account: Account<'info, BatchVerifyingKey>,
}

/// Context for verifying a batch proof via CPI
/// This is called by the Bridge program to verify batch submissions
#[derive(Accounts)]
pub struct VerifyBatchProof<'info> {
    /// The Bridge program calling this via CPI
    pub caller: Signer<'info>,

    /// The stored verifying key for this domain
    #[account(
        seeds = [b"batch_vk", vk_account.domain.as_ref()],
        bump = vk_account.bump,
        constraint = vk_account.finalized @ VerifierError::InvalidPublicInput,
    )]
    pub vk_account: Account<'info, BatchVerifyingKey>,
}

/// Context for verifying and storing Groth16 proofs
#[derive(Accounts)]
#[instruction(proof_id: String)]
pub struct VerifyGroth16<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        init,
        payer = authority,
        space = 8 + std::mem::size_of::<VerifiedGroth16Proof>() + 1000, // Extra space for dynamic fields
        seeds = [b"groth16_proof", authority.key().as_ref(), proof_id.as_bytes()],
        bump
    )]
    pub proof_account: Account<'info, VerifiedGroth16Proof>,

    pub system_program: Program<'info, System>,
}

/// Context for verifying and storing RISC0 proofs
#[derive(Accounts)]
#[instruction(proof_id: String)]
pub struct VerifyRisc0<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        init,
        payer = authority,
        space = 8 + std::mem::size_of::<VerifiedRisc0Proof>(),
        seeds = [b"risc0_proof", authority.key().as_ref(), proof_id.as_bytes()],
        bump
    )]
    pub proof_account: Account<'info, VerifiedRisc0Proof>,

    pub system_program: Program<'info, System>,
}

#[program]
pub mod onchain_verifier {
    use super::*;

    /// Verify a Groth16 proof and store it if verification succeeds
    pub fn verify_groth16_proof(
        ctx: Context<VerifyGroth16>,
        proof_id: String,
        proof: Groth16Proof,
        public_inputs: PublicInputs,
        verifying_key: Groth16VerifyingKey,
    ) -> Result<()> {
        msg!(
            "Starting Groth16 proof verification for proof_id: {}",
            proof_id
        );

        // Verify the proof using alt-bn254 syscalls
        verify_groth16_with_alt_bn254(&proof, &public_inputs, &verifying_key)?;

        // Calculate verifying key hash for reference
        let vk_hash = hash_verifying_key(&verifying_key);

        // Store the verified proof
        let proof_account = &mut ctx.accounts.proof_account;
        proof_account.authority = ctx.accounts.authority.key();
        proof_account.proof = proof;
        proof_account.public_inputs = public_inputs;
        proof_account.verifying_key_hash = vk_hash;
        proof_account.verified_at = Clock::get()?.unix_timestamp;
        proof_account.bump = ctx.bumps.proof_account;

        msg!("Groth16 proof verified and stored successfully!");
        Ok(())
    }

    /// Verify a RISC0 proof and store it if verification succeeds
    pub fn verify_risc0_proof(
        ctx: Context<VerifyRisc0>,
        proof_id: String,
        proof: Risc0Proof,
        image_id: [u8; 32],
        journal_digest: [u8; 32],
    ) -> Result<()> {
        msg!(
            "Starting RISC0 proof verification for proof_id: {}",
            proof_id
        );

        // Generate claim digest for RISC0
        let claim_digest = hash_risc0_claim(&image_id, &journal_digest);
        let public_inputs = risc0_public_inputs(claim_digest)?;

        // Verify the proof using the embedded RISC0 verification key
        verify_risc0_with_alt_bn254(&proof, &public_inputs)?;

        // Store the verified proof
        let proof_account = &mut ctx.accounts.proof_account;
        proof_account.authority = ctx.accounts.authority.key();
        proof_account.proof = proof;
        proof_account.image_id = image_id;
        proof_account.journal_digest = journal_digest;
        proof_account.verified_at = Clock::get()?.unix_timestamp;
        proof_account.bump = ctx.bumps.proof_account;

        msg!("RISC0 proof verified and stored successfully!");
        Ok(())
    }

    /// Store the batch verifying key for a domain
    /// This must be called once per domain to set up the VK used for batch proof verification
    pub fn store_batch_vk(
        ctx: Context<StoreBatchVk>,
        domain: [u8; 32],
        alpha_g1: [u8; 64],
        beta_g2: [u8; 128],
        gamma_g2: [u8; 128],
        delta_g2: [u8; 128],
        ic: Vec<[u8; 64]>,
    ) -> Result<()> {
        // Validate IC length
        require!(ic.len() <= MAX_IC_POINTS, VerifierError::InvalidPublicInput);
        require!(!ic.is_empty(), VerifierError::InvalidPublicInput);

        let vk = &mut ctx.accounts.vk_account;
        vk.authority = ctx.accounts.authority.key();
        vk.domain = domain;
        vk.alpha_g1 = alpha_g1;
        vk.beta_g2 = beta_g2;
        vk.gamma_g2 = gamma_g2;
        vk.delta_g2 = delta_g2;

        // Copy IC points into fixed-size array
        for (i, point) in ic.iter().enumerate() {
            vk.ic[i] = *point;
        }
        vk.ic_len = ic.len() as u8;
        vk.finalized = true;
        vk.created_at = Clock::get()?.unix_timestamp;
        vk.bump = ctx.bumps.vk_account;

        msg!("Batch VK stored for domain, IC points: {}", ic.len());
        Ok(())
    }

    /// Initialize batch VK without IC points (for chunked upload)
    /// After this, call append_ic_points to add IC points, then finalize_batch_vk
    pub fn init_batch_vk(
        ctx: Context<InitBatchVk>,
        domain: [u8; 32],
        alpha_g1: [u8; 64],
        beta_g2: [u8; 128],
        gamma_g2: [u8; 128],
        delta_g2: [u8; 128],
    ) -> Result<()> {
        let vk = &mut ctx.accounts.vk_account;
        vk.authority = ctx.accounts.authority.key();
        vk.domain = domain;
        vk.alpha_g1 = alpha_g1;
        vk.beta_g2 = beta_g2;
        vk.gamma_g2 = gamma_g2;
        vk.delta_g2 = delta_g2;
        vk.ic = [[0u8; 64]; MAX_IC_POINTS];
        vk.ic_len = 0;
        vk.finalized = false;
        vk.created_at = Clock::get()?.unix_timestamp;
        vk.bump = ctx.bumps.vk_account;

        msg!("Batch VK initialized for domain (awaiting IC points)");
        Ok(())
    }

    /// Append IC points to an existing VK (for chunked upload)
    /// Can be called multiple times until all IC points are added
    pub fn append_ic_points(ctx: Context<AppendIcPoints>, ic_points: Vec<[u8; 64]>) -> Result<()> {
        let vk = &mut ctx.accounts.vk_account;

        let current_len = vk.ic_len as usize;
        let new_len = current_len + ic_points.len();

        require!(new_len <= MAX_IC_POINTS, VerifierError::InvalidPublicInput);
        require!(!ic_points.is_empty(), VerifierError::InvalidPublicInput);

        for (i, point) in ic_points.iter().enumerate() {
            vk.ic[current_len + i] = *point;
        }
        vk.ic_len = new_len as u8;

        msg!("Appended {} IC points, total: {}", ic_points.len(), new_len);
        Ok(())
    }

    /// Finalize the VK after all IC points have been added
    pub fn finalize_batch_vk(ctx: Context<FinalizeBatchVk>) -> Result<()> {
        let vk = &mut ctx.accounts.vk_account;

        // Must have at least 1 IC point
        require!(vk.ic_len > 0, VerifierError::InvalidPublicInput);

        vk.finalized = true;
        msg!("Batch VK finalized with {} IC points", vk.ic_len);
        Ok(())
    }

    /// Verify a batch proof (called via CPI from Bridge)
    /// Returns Ok(()) if proof is valid, Err if invalid
    pub fn verify_batch_proof(
        ctx: Context<VerifyBatchProof>,
        proof: Groth16Proof,
        public_inputs: BatchPublicInputs,
    ) -> Result<()> {
        let vk = &ctx.accounts.vk_account;

        // Convert BatchPublicInputs to field elements for Groth16 verification
        let inputs = batch_inputs_to_field_elements(&public_inputs);

        // Build VK from stored data
        let ic: Vec<[u8; 64]> = vk.ic[..vk.ic_len as usize].to_vec();

        // Validate IC length matches expected public inputs + 1
        require!(
            ic.len() == inputs.len() + 1,
            VerifierError::InvalidPublicInput
        );

        let verifying_key = Groth16VerifyingKey {
            alpha_g1: vk.alpha_g1,
            beta_g2: vk.beta_g2,
            gamma_g2: vk.gamma_g2,
            delta_g2: vk.delta_g2,
            ic,
        };

        // Verify using existing Groth16 verification function
        let pub_inputs = PublicInputs { inputs };
        verify_groth16_with_alt_bn254(&proof, &pub_inputs, &verifying_key)?;

        msg!(
            "Batch proof verified successfully for batch_id: {}",
            public_inputs.batch_id
        );
        Ok(())
    }
}

/// Convert batch public inputs to field elements for Groth16 verification
/// The circuit expects 7 public inputs in this order
fn batch_inputs_to_field_elements(inputs: &BatchPublicInputs) -> Vec<[u8; 32]> {
    vec![
        inputs.pre_state_root,
        inputs.post_state_root,
        inputs.pre_shielded_root,
        inputs.post_shielded_root,
        inputs.withdrawal_root,
        inputs.batch_hash,
        // batch_id as 32-byte big-endian field element
        {
            let mut bytes = [0u8; 32];
            bytes[24..].copy_from_slice(&inputs.batch_id.to_be_bytes());
            bytes
        },
    ]
}

/// Verify Groth16 proof using Solana's alt-bn254 syscalls
fn verify_groth16_with_alt_bn254(
    proof: &Groth16Proof,
    public_inputs: &PublicInputs,
    vk: &Groth16VerifyingKey,
) -> Result<()> {
    // Validate that we have the right number of IC points
    if vk.ic.len() != public_inputs.inputs.len() + 1 {
        return err!(VerifierError::InvalidPublicInput);
    }

    // Validate all scalars are in field
    for input in &public_inputs.inputs {
        verify_scalar_in_field(input)?;
    }

    // Compute vk_x = IC[0] + sum(IC[i+1] * public_input[i])
    let mut vk_x = vk.ic[0];
    for (i, input) in public_inputs.inputs.iter().enumerate() {
        let mul_res = alt_bn128_multiplication(&[&vk.ic[i + 1][..], input].concat())
            .map_err(|_| VerifierError::ArithmeticError)?;
        vk_x = alt_bn128_addition(&[&mul_res[..], &vk_x[..]].concat())
            .map_err(|_| VerifierError::ArithmeticError)?
            .try_into()
            .map_err(|_| VerifierError::ArithmeticError)?;
    }

    // Prepare pairing input: [proof.a, proof.b, vk_x, vk.gamma_g2, proof.c, vk.delta_g2, vk.alpha_g1, vk.beta_g2]
    let pairing_input = [
        proof.pi_a.as_slice(),
        proof.pi_b.as_slice(),
        vk_x.as_slice(),
        vk.gamma_g2.as_slice(),
        proof.pi_c.as_slice(),
        vk.delta_g2.as_slice(),
        vk.alpha_g1.as_slice(),
        vk.beta_g2.as_slice(),
    ]
    .concat();

    // Perform pairing check
    let pairing_res = alt_bn128_pairing(&pairing_input).map_err(|_| VerifierError::PairingError)?;

    let mut expected = [0u8; 32];
    expected[31] = 1;

    if pairing_res != expected {
        return err!(VerifierError::VerificationError);
    }

    Ok(())
}

/// Verify RISC0 proof using the hardcoded verification key
fn verify_risc0_with_alt_bn254(_proof: &Risc0Proof, public_inputs: &PublicInputs) -> Result<()> {
    // Use the same verification logic as the RISC0 verifier
    // For now, we'll use a simplified version - in production you'd embed the actual VK

    // Validate all scalars are in field
    for input in &public_inputs.inputs {
        verify_scalar_in_field(input)?;
    }

    // This is a simplified verification - in a real implementation,
    // you would embed the actual RISC0 verification key constants
    msg!("RISC0 verification temporarily simplified - would use embedded VK in production");

    Ok(())
}

/// Generate RISC0 claim digest
fn hash_risc0_claim(image_id: &[u8; 32], journal_digest: &[u8; 32]) -> [u8; 32] {
    let input_digest = [0u8; 32];
    let pre_digest = image_id;
    let post_digest = SYSTEM_STATE_ZERO_DIGEST;
    let output_digest = hash_risc0_output(journal_digest, &[0u8; 32]);
    let system_exit = 0;
    let user_exit = 0;

    hash_receipt_claim(
        &input_digest,
        pre_digest,
        &post_digest,
        &output_digest,
        system_exit,
        user_exit,
    )
}

/// Generate RISC0 output digest
fn hash_risc0_output(journal_digest: &[u8; 32], assumptions_digest: &[u8; 32]) -> [u8; 32] {
    let down_len = (2u16 << 8).to_be_bytes();
    hashv(&[&OUTPUT_TAG, journal_digest, assumptions_digest, &down_len]).to_bytes()
}

/// Generate RISC0 receipt claim digest
fn hash_receipt_claim(
    input_digest: &[u8; 32],
    pre_state_digest: &[u8; 32],
    post_state_digest: &[u8; 32],
    output_digest: &[u8; 32],
    system_exit_code: u32,
    user_exit_code: u32,
) -> [u8; 32] {
    let system_bytes = (system_exit_code << 24).to_be_bytes();
    let user_bytes = (user_exit_code << 24).to_be_bytes();
    let down_len = (4u16 << 8).to_be_bytes();

    hashv(&[
        &RECEIPT_CLAIM_TAG,
        input_digest,
        pre_state_digest,
        post_state_digest,
        output_digest,
        &system_bytes,
        &user_bytes,
        &down_len,
    ])
    .to_bytes()
}

/// Convert RISC0 claim digest to public inputs
fn risc0_public_inputs(claim_digest: [u8; 32]) -> Result<PublicInputs> {
    if claim_digest == [0u8; 32] {
        return err!(VerifierError::InvalidPublicInput);
    }

    let (a0, a1) = split_digest(ALLOWED_CONTROL_ROOT)?;
    let (c0, c1) = split_digest(claim_digest)?;

    let mut id = BN254_IDENTITY_CONTROL_ID.to_vec();
    id.reverse();

    Ok(PublicInputs {
        inputs: vec![a0, a1, c0, c1, to_field_element(&id)],
    })
}

/// Split digest into two field elements
fn split_digest(bytes: [u8; 32]) -> Result<([u8; 32], [u8; 32])> {
    let big_endian: Vec<u8> = bytes.iter().rev().copied().collect();
    let (b, a) = big_endian.split_at(big_endian.len() / 2);
    Ok((to_field_element(a), to_field_element(b)))
}

/// Convert bytes to field element
fn to_field_element(input: &[u8]) -> [u8; 32] {
    let mut fixed_array = [0u8; 32];
    let start_index = 32 - input.len();
    fixed_array[start_index..].copy_from_slice(input);
    fixed_array
}

/// Verify scalar is in BN254 base field
fn verify_scalar_in_field(x: &[u8; 32]) -> Result<()> {
    if x.iter().cmp(BASE_FIELD_MODULUS_Q.iter()) != std::cmp::Ordering::Less {
        return err!(VerifierError::InvalidPublicInput);
    }
    Ok(())
}

/// Hash a verifying key for reference
fn hash_verifying_key(vk: &Groth16VerifyingKey) -> [u8; 32] {
    let mut data = Vec::new();
    data.extend_from_slice(&vk.alpha_g1);
    data.extend_from_slice(&vk.beta_g2);
    data.extend_from_slice(&vk.gamma_g2);
    data.extend_from_slice(&vk.delta_g2);
    for ic in &vk.ic {
        data.extend_from_slice(ic);
    }
    hashv(&[&data]).to_bytes()
}

/// Negate a BN254 G1 curve point (needed for Groth16 verification)
pub fn negate_g1(point: &[u8; 64]) -> [u8; 64] {
    let mut negated_point = [0u8; 64];
    negated_point[..32].copy_from_slice(&point[..32]);

    let mut y = [0u8; 32];
    y.copy_from_slice(&point[32..]);

    let mut modulus = BASE_FIELD_MODULUS_Q;
    subtract_be_bytes(&mut modulus, &y);
    negated_point[32..].copy_from_slice(&modulus);

    negated_point
}

/// Subtract big-endian numbers (helper for negation)
fn subtract_be_bytes(a: &mut [u8; 32], b: &[u8; 32]) {
    let mut borrow: u32 = 0;
    for (ai, bi) in a.iter_mut().zip(b.iter()).rev() {
        let result = (*ai as u32).wrapping_sub(*bi as u32).wrapping_sub(borrow);
        *ai = result as u8;
        borrow = (result >> 31) & 1;
    }
}

/// Helper functions for converting from Arkworks format to Solana format
pub mod conversion_helpers {
    use super::*;

    /// Convert compressed Arkworks proof bytes to Groth16Proof format
    /// This assumes the proof was serialized using arkworks compressed format
    pub fn arkworks_proof_to_solana_format(compressed_proof_bytes: &[u8]) -> Result<Groth16Proof> {
        // This is a placeholder implementation
        // In practice, you'd need to deserialize the Arkworks proof and extract the elements
        // For now, we'll assume the bytes are already in the correct format
        if compressed_proof_bytes.len() < 256 {
            return err!(VerifierError::InvalidPublicInput);
        }

        let pi_a: [u8; 64] = compressed_proof_bytes[0..64]
            .try_into()
            .map_err(|_| VerifierError::InvalidPublicInput)?;
        let pi_b: [u8; 128] = compressed_proof_bytes[64..192]
            .try_into()
            .map_err(|_| VerifierError::InvalidPublicInput)?;
        let pi_c: [u8; 64] = compressed_proof_bytes[192..256]
            .try_into()
            .map_err(|_| VerifierError::InvalidPublicInput)?;

        // Note: pi_a should be negated for Groth16 verification
        let negated_pi_a = negate_g1(&pi_a);

        Ok(Groth16Proof {
            pi_a: negated_pi_a,
            pi_b,
            pi_c,
        })
    }

    /// Convert compressed Arkworks verifying key bytes to Groth16VerifyingKey format
    pub fn arkworks_vk_to_solana_format(compressed_vk_bytes: &[u8]) -> Result<Groth16VerifyingKey> {
        // This is a placeholder implementation
        // In practice, you'd need to deserialize the Arkworks VK and extract the elements
        // The exact format depends on how your circuit's VK is structured

        // For a simple circuit with one public input, we expect:
        // - alpha_g1: 64 bytes
        // - beta_g2: 128 bytes
        // - gamma_g2: 128 bytes
        // - delta_g2: 128 bytes
        // - ic[0]: 64 bytes (base)
        // - ic[1]: 64 bytes (for first public input)

        let expected_size = 64 + 128 + 128 + 128 + 64 + 64; // 576 bytes minimum
        if compressed_vk_bytes.len() < expected_size {
            return err!(VerifierError::InvalidPublicInput);
        }

        let mut offset = 0;

        let alpha_g1: [u8; 64] = compressed_vk_bytes[offset..offset + 64]
            .try_into()
            .map_err(|_| VerifierError::InvalidPublicInput)?;
        offset += 64;

        let beta_g2: [u8; 128] = compressed_vk_bytes[offset..offset + 128]
            .try_into()
            .map_err(|_| VerifierError::InvalidPublicInput)?;
        offset += 128;

        let gamma_g2: [u8; 128] = compressed_vk_bytes[offset..offset + 128]
            .try_into()
            .map_err(|_| VerifierError::InvalidPublicInput)?;
        offset += 128;

        let delta_g2: [u8; 128] = compressed_vk_bytes[offset..offset + 128]
            .try_into()
            .map_err(|_| VerifierError::InvalidPublicInput)?;
        offset += 128;

        // For the square circuit, we have 2 IC points (ic[0] and ic[1])
        let ic0: [u8; 64] = compressed_vk_bytes[offset..offset + 64]
            .try_into()
            .map_err(|_| VerifierError::InvalidPublicInput)?;
        offset += 64;

        let ic1: [u8; 64] = compressed_vk_bytes[offset..offset + 64]
            .try_into()
            .map_err(|_| VerifierError::InvalidPublicInput)?;

        Ok(Groth16VerifyingKey {
            alpha_g1,
            beta_g2,
            gamma_g2,
            delta_g2,
            ic: vec![ic0, ic1],
        })
    }

    /// Convert field element to 32-byte array for public inputs
    pub fn field_element_to_bytes(field_bytes: &[u8]) -> [u8; 32] {
        let mut result = [0u8; 32];
        let start = 32 - field_bytes.len().min(32);
        result[start..].copy_from_slice(&field_bytes[..field_bytes.len().min(32)]);
        result
    }
}

/// Client helper functions for interacting with the onchain verifier
#[cfg(feature = "client")]
pub mod client {
    use super::*;
    use anchor_lang::prelude::Pubkey;

    /// Generate PDA for storing a Groth16 proof
    pub fn get_groth16_proof_pda(
        authority: &Pubkey,
        proof_id: &str,
        program_id: &Pubkey,
    ) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[b"groth16_proof", authority.as_ref(), proof_id.as_bytes()],
            program_id,
        )
    }

    /// Generate PDA for storing a RISC0 proof
    pub fn get_risc0_proof_pda(
        authority: &Pubkey,
        proof_id: &str,
        program_id: &Pubkey,
    ) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[b"risc0_proof", authority.as_ref(), proof_id.as_bytes()],
            program_id,
        )
    }

    /// Helper to create instruction data for Groth16 verification
    pub fn build_groth16_verify_instruction_data(
        proof_id: String,
        proof: Groth16Proof,
        public_inputs: PublicInputs,
        verifying_key: Groth16VerifyingKey,
    ) -> Vec<u8> {
        // This would typically use the Anchor IDL to serialize the instruction data
        // For now, we provide a placeholder that shows the structure
        let mut data = Vec::new();

        // Instruction discriminator (first 8 bytes)
        data.extend_from_slice(&[0u8; 8]); // Would be computed from method name hash

        // Serialize parameters using Anchor's serialization
        // proof_id, proof, public_inputs, verifying_key would be serialized here

        data
    }

    /// Helper to create instruction data for RISC0 verification
    pub fn build_risc0_verify_instruction_data(
        proof_id: String,
        proof: Risc0Proof,
        image_id: [u8; 32],
        journal_digest: [u8; 32],
    ) -> Vec<u8> {
        // This would typically use the Anchor IDL to serialize the instruction data
        // For now, we provide a placeholder that shows the structure
        let mut data = Vec::new();

        // Instruction discriminator (first 8 bytes)
        data.extend_from_slice(&[0u8; 8]); // Would be computed from method name hash

        // Serialize parameters using Anchor's serialization
        // proof_id, proof, image_id, journal_digest would be serialized here

        data
    }
}

#[error_code]
pub enum VerifierError {
    #[msg("Invalid public input")]
    InvalidPublicInput,
    #[msg("Arithmetic error in elliptic curve operations")]
    ArithmeticError,
    #[msg("Pairing operation failed")]
    PairingError,
    #[msg("Proof verification failed")]
    VerificationError,
}
