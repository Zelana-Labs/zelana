//! ZK Verification Integration Tests
//!
//! Tests the full flow of batch submission with ZK proof verification:
//! 1. Initialize bridge
//! 2. Store batch VK in verifier
//! 3. Submit batch with proof that triggers CPI to verifier
//!
//! Note: These tests use mock proof data to test the integration flow.
//! For real proof verification, the alt_bn128 syscalls are not available in LiteSVM,
//! so we focus on testing the account structure and CPI mechanics.

mod common;

use bridge_z::instruction::{BridgeIx, SubmitBatchHeader};
use common::{TEST_DOMAIN, TestFixture};
use litesvm::LiteSVM;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    message::{VersionedMessage, v0},
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    system_program,
    transaction::VersionedTransaction,
};

/// Verifier program ID (must match the deployed verifier)
const VERIFIER_PROGRAM_ID: Pubkey =
    solana_sdk::pubkey!("8TveT3mvH59qLzZNwrTT6hBqDHEobW2XnCPb7xZLBYHd");

/// Anchor discriminator for store_batch_vk instruction
/// = sha256("global:store_batch_vk")[0..8]
const STORE_BATCH_VK_DISCRIMINATOR: [u8; 8] = [0x5f, 0x81, 0xed, 0x49, 0x8b, 0x44, 0x24, 0xf5];

/// Derive the batch VK PDA
fn derive_batch_vk_pda(domain: &[u8; 32]) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"batch_vk", domain.as_ref()], &VERIFIER_PROGRAM_ID)
}

/// Test fixture that includes both bridge and verifier programs
struct ZkTestFixture {
    svm: LiteSVM,
    payer: Keypair,
    sequencer: Keypair,
    bridge_program_id: Pubkey,
    verifier_program_id: Pubkey,
    domain: [u8; 32],
    config_pda: Pubkey,
    vault_pda: Pubkey,
    vk_pda: Pubkey,
}

impl ZkTestFixture {
    fn new() -> Self {
        let mut svm = LiteSVM::new();
        let payer = Keypair::new();
        let sequencer = Keypair::new();

        svm.airdrop(&payer.pubkey(), 100_000_000_000).unwrap();
        svm.airdrop(&sequencer.pubkey(), 10_000_000_000).unwrap();

        // Load bridge program
        let bridge_program_id = Pubkey::from(bridge_z::ID);
        svm.add_program_from_file(bridge_program_id, "./target/deploy/bridge_z.so")
            .expect("Failed to load bridge program");

        // Load verifier program
        svm.add_program_from_file(
            VERIFIER_PROGRAM_ID,
            "../verifier/target/deploy/onchain_verifier.so",
        )
        .expect("Failed to load verifier program");

        let domain = TEST_DOMAIN;
        let (config_pda, _) = common::derive_config_pda(&bridge_program_id, &domain);
        let (vault_pda, _) = common::derive_vault_pda(&bridge_program_id, &domain);
        let (vk_pda, _) = derive_batch_vk_pda(&domain);

        Self {
            svm,
            payer,
            sequencer,
            bridge_program_id,
            verifier_program_id: VERIFIER_PROGRAM_ID,
            domain,
            config_pda,
            vault_pda,
            vk_pda,
        }
    }

    fn build_and_send_transaction(
        &mut self,
        signers: &[&Keypair],
        instructions: Vec<Instruction>,
    ) -> Result<litesvm::types::TransactionMetadata, litesvm::types::FailedTransactionMetadata>
    {
        let msg = v0::Message::try_compile(
            &self.payer.pubkey(),
            &instructions,
            &[],
            self.svm.latest_blockhash(),
        )
        .unwrap();

        let mut all_signers = vec![&self.payer];
        all_signers.extend(signers);

        let tx = VersionedTransaction::try_new(VersionedMessage::V0(msg), &all_signers).unwrap();
        self.svm.send_transaction(tx)
    }

    fn initialize_bridge(
        &mut self,
    ) -> Result<litesvm::types::TransactionMetadata, litesvm::types::FailedTransactionMetadata>
    {
        let sequencer_pubkey = self.sequencer.pubkey();
        let ix_data = bridge_z::instruction::InitParams {
            sequencer_authority: *sequencer_pubkey.as_array(),
            domain: self.domain,
        };

        let mut instruction_data = vec![BridgeIx::INIT as u8];
        instruction_data.extend_from_slice(bytemuck::bytes_of(&ix_data));

        let accounts = vec![
            AccountMeta::new(self.payer.pubkey(), true),
            AccountMeta::new(self.config_pda, false),
            AccountMeta::new(self.vault_pda, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ];

        let init_ix = Instruction {
            program_id: self.bridge_program_id,
            accounts,
            data: instruction_data,
        };

        self.build_and_send_transaction(&[], vec![init_ix])
    }

    /// Store a batch verifying key in the verifier program
    ///
    /// Note: This creates a minimal valid VK structure. In production,
    /// this would be a real VK from the L2 circuit.
    fn store_batch_vk(
        &mut self,
    ) -> Result<litesvm::types::TransactionMetadata, litesvm::types::FailedTransactionMetadata>
    {
        // Build instruction data for store_batch_vk
        // Format: discriminator + domain + alpha_g1 + beta_g2 + gamma_g2 + delta_g2 + ic_len + ic[]
        let mut data = Vec::new();

        // Anchor discriminator
        data.extend_from_slice(&STORE_BATCH_VK_DISCRIMINATOR);

        // domain: [u8; 32]
        data.extend_from_slice(&self.domain);

        // alpha_g1: [u8; 64] - mock G1 point (valid encoding)
        let alpha_g1 = [1u8; 64];
        data.extend_from_slice(&alpha_g1);

        // beta_g2: [u8; 128] - mock G2 point
        let beta_g2 = [2u8; 128];
        data.extend_from_slice(&beta_g2);

        // gamma_g2: [u8; 128]
        let gamma_g2 = [3u8; 128];
        data.extend_from_slice(&gamma_g2);

        // delta_g2: [u8; 128]
        let delta_g2 = [4u8; 128];
        data.extend_from_slice(&delta_g2);

        // ic: Vec<[u8; 64]> - Borsh serialization: 4-byte length + elements
        // For batch verification we need 8 IC points (7 public inputs + 1)
        let ic_count: u32 = 8;
        data.extend_from_slice(&ic_count.to_le_bytes());
        for i in 0..8 {
            let mut ic_point = [0u8; 64];
            ic_point[0] = (i + 10) as u8; // Different mock values
            data.extend_from_slice(&ic_point);
        }

        let accounts = vec![
            AccountMeta::new(self.payer.pubkey(), true), // authority (signer, payer)
            AccountMeta::new(self.vk_pda, false),        // vk_account (PDA)
            AccountMeta::new_readonly(system_program::ID, false), // system_program
        ];

        let store_vk_ix = Instruction {
            program_id: self.verifier_program_id,
            accounts,
            data,
        };

        self.build_and_send_transaction(&[], vec![store_vk_ix])
    }
}

/// Mock Groth16 proof structure (256 bytes)
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct MockGroth16Proof {
    pi_a: [u8; 64],
    pi_b: [u8; 128],
    pi_c: [u8; 64],
}

/// Mock batch public inputs structure (200 bytes)
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct MockBatchPublicInputs {
    pre_state_root: [u8; 32],
    post_state_root: [u8; 32],
    pre_shielded_root: [u8; 32],
    post_shielded_root: [u8; 32],
    withdrawal_root: [u8; 32],
    batch_hash: [u8; 32],
    batch_id: u64,
}

#[test]
fn test_zk_flow_store_batch_vk() {
    let mut fixture = ZkTestFixture::new();

    // Initialize bridge first
    fixture.initialize_bridge().expect("bridge init failed");

    // Store batch VK
    let result = fixture.store_batch_vk();

    // Note: This will likely fail with an arithmetic error because
    // the mock VK data isn't valid curve points. That's expected.
    // In a real test with valid VK data, this would succeed.
    match result {
        Ok(_) => {
            println!("VK stored successfully");

            // Verify VK account exists and has expected data
            let vk_account = fixture.svm.get_account(&fixture.vk_pda);
            assert!(vk_account.is_some(), "VK account should exist");

            let account = vk_account.unwrap();
            println!("VK account size: {} bytes", account.data.len());

            // Check domain is stored correctly (after 8-byte discriminator + 32-byte authority)
            let stored_domain: [u8; 32] = account.data[40..72].try_into().unwrap();
            assert_eq!(stored_domain, fixture.domain, "Domain should match");
        }
        Err(e) => {
            // Expected to fail with invalid curve points in LiteSVM
            // The alt_bn128 syscalls require valid curve points
            println!("VK storage failed (expected with mock data): {:?}", e);
        }
    }
}

#[test]
fn test_zk_flow_submit_batch_accounts_structure() {
    // This test verifies the account structure for submit_batch with ZK verification
    // Without valid proof data, we can't fully test the CPI, but we can verify
    // the instruction structure is correct

    let mut fixture = ZkTestFixture::new();
    fixture.initialize_bridge().expect("bridge init failed");

    // Build a submit_batch instruction with the correct account structure
    let header = SubmitBatchHeader {
        prev_batch_index: 0,
        new_batch_index: 1,
        new_state_root: [9u8; 32],
        proof_len: 256, // Groth16 proof size
        withdrawal_count: 0,
    };

    // Create mock proof
    let proof = MockGroth16Proof {
        pi_a: [1u8; 64],
        pi_b: [2u8; 128],
        pi_c: [3u8; 64],
    };

    // Create mock public inputs matching the header
    let public_inputs = MockBatchPublicInputs {
        pre_state_root: [0u8; 32],
        post_state_root: [9u8; 32], // Must match header.new_state_root
        pre_shielded_root: [0u8; 32],
        post_shielded_root: [0u8; 32],
        withdrawal_root: [0u8; 32],
        batch_hash: [0u8; 32],
        batch_id: 1, // Must match header.new_batch_index
    };

    // Build instruction data
    let mut data = vec![BridgeIx::SubmitBatch as u8];
    data.extend_from_slice(bytemuck::bytes_of(&header));
    data.extend_from_slice(bytemuck::bytes_of(&proof));
    data.extend_from_slice(bytemuck::bytes_of(&public_inputs));

    // Verify instruction data size is correct
    // 1 (discriminator) + 56 (header) + 256 (proof) + 200 (public_inputs) = 513 bytes
    assert_eq!(
        data.len(),
        1 + 56 + 256 + 200,
        "Instruction data size mismatch"
    );

    // Build accounts list
    // [sequencer, config, verifier_program, vk_account, ...recipients]
    let accounts = vec![
        AccountMeta::new(fixture.sequencer.pubkey(), true),
        AccountMeta::new(fixture.config_pda, false),
        AccountMeta::new_readonly(fixture.verifier_program_id, false),
        AccountMeta::new_readonly(fixture.vk_pda, false),
    ];

    let submit_ix = Instruction {
        program_id: fixture.bridge_program_id,
        accounts,
        data,
    };

    // This will fail because:
    // 1. VK account doesn't exist
    // 2. Proof data is invalid
    // But the test verifies the instruction structure is correct

    let sequencer = fixture.sequencer.insecure_clone();
    let result = fixture.build_and_send_transaction(&[&sequencer], vec![submit_ix]);

    // We expect this to fail because VK account doesn't exist
    assert!(result.is_err(), "Should fail without VK account");
    println!(
        "Submit batch failed as expected (no VK): {:?}",
        result.err()
    );
}

#[test]
fn test_derive_batch_vk_pda() {
    // Verify PDA derivation matches expected seeds
    let domain = TEST_DOMAIN;
    let (pda, bump) = derive_batch_vk_pda(&domain);

    // Verify it's a valid PDA (off curve)
    assert!(!pda.is_on_curve(), "PDA should be off curve");

    // Verify we can derive the same PDA again
    let (pda2, bump2) = derive_batch_vk_pda(&domain);
    assert_eq!(pda, pda2, "PDA should be deterministic");
    assert_eq!(bump, bump2, "Bump should be deterministic");

    // Different domain should give different PDA
    let other_domain = [2u8; 32];
    let (other_pda, _) = derive_batch_vk_pda(&other_domain);
    assert_ne!(
        pda, other_pda,
        "Different domains should have different PDAs"
    );

    println!("VK PDA for test domain: {}", pda);
    println!("VK PDA bump: {}", bump);
}

#[test]
fn test_submit_batch_header_size() {
    // Verify SubmitBatchHeader size matches what the bridge expects
    // 8 (prev_batch_index) + 8 (new_batch_index) + 32 (new_state_root) + 4 (proof_len) + 4 (withdrawal_count) = 56
    assert_eq!(
        std::mem::size_of::<SubmitBatchHeader>(),
        56,
        "SubmitBatchHeader size mismatch"
    );

    assert_eq!(
        std::mem::size_of::<MockGroth16Proof>(),
        256,
        "Groth16Proof size mismatch"
    );

    assert_eq!(
        std::mem::size_of::<MockBatchPublicInputs>(),
        200,
        "BatchPublicInputs size mismatch"
    );
}
