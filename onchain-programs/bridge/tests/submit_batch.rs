mod common;

use bridge_z::{
    instruction::{BatchPublicInputs, BridgeIx, Groth16Proof, SubmitBatchHeader},
    state::Config,
};
use common::TestFixture;
use litesvm::LiteSVM;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
};

/// Verifier program ID (must match the deployed verifier)
const VERIFIER_PROGRAM_ID: Pubkey =
    solana_sdk::pubkey!("8TveT3mvH59qLzZNwrTT6hBqDHEobW2XnCPb7xZLBYHd");

/// Derive the batch VK PDA
fn derive_batch_vk_pda(domain: &[u8; 32]) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"batch_vk", domain.as_ref()], &VERIFIER_PROGRAM_ID)
}

/// Extended test fixture that loads both bridge and verifier programs
struct SubmitBatchFixture {
    svm: LiteSVM,
    payer: Keypair,
    sequencer: Keypair,
    program_id: Pubkey,
    config_pda: Pubkey,
    vk_pda: Pubkey,
    domain: [u8; 32],
}

impl SubmitBatchFixture {
    fn new() -> Self {
        let mut svm = LiteSVM::new();
        let payer = Keypair::new();
        let sequencer = Keypair::new();

        svm.airdrop(&payer.pubkey(), 100_000_000_000).unwrap();
        svm.airdrop(&sequencer.pubkey(), 10_000_000_000).unwrap();

        // Load bridge program
        let program_id = Pubkey::from(bridge_z::ID);
        svm.add_program_from_file(program_id, "./target/deploy/bridge_z.so")
            .expect("Failed to load bridge program");

        // Load verifier program
        svm.add_program_from_file(
            VERIFIER_PROGRAM_ID,
            "../verifier/target/deploy/onchain_verifier.so",
        )
        .expect("Failed to load verifier program");

        let domain = common::TEST_DOMAIN;
        let (config_pda, _) = common::derive_config_pda(&program_id, &domain);
        let (vk_pda, _) = derive_batch_vk_pda(&domain);

        Self {
            svm,
            payer,
            sequencer,
            program_id,
            config_pda,
            vk_pda,
            domain,
        }
    }

    fn build_and_send_transaction(
        &mut self,
        signers: &[&Keypair],
        instructions: Vec<Instruction>,
    ) -> Result<litesvm::types::TransactionMetadata, litesvm::types::FailedTransactionMetadata>
    {
        use solana_sdk::{
            message::{VersionedMessage, v0},
            transaction::VersionedTransaction,
        };

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

        let (vault_pda, _) = common::derive_vault_pda(&self.program_id, &self.domain);
        let accounts = vec![
            AccountMeta::new(self.payer.pubkey(), true),
            AccountMeta::new(self.config_pda, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new_readonly(solana_sdk::system_program::ID, false),
        ];

        let init_ix = Instruction {
            program_id: self.program_id,
            accounts,
            data: instruction_data,
        };

        self.build_and_send_transaction(&[], vec![init_ix])
    }

    /// Build a submit_batch instruction with proof data
    fn build_submit_batch_ix(
        &self,
        prev_batch_index: u64,
        new_batch_index: u64,
        new_state_root: [u8; 32],
        signer: &Pubkey,
    ) -> Instruction {
        let header = SubmitBatchHeader {
            prev_batch_index,
            new_batch_index,
            new_state_root,
            proof_len: 256, // Groth16Proof::LEN
            withdrawal_count: 0,
        };

        // Create mock proof
        let proof = Groth16Proof {
            pi_a: [1u8; 64],
            pi_b: [2u8; 128],
            pi_c: [3u8; 64],
        };

        // Create mock public inputs matching the header
        let public_inputs = BatchPublicInputs {
            pre_state_root: [0u8; 32], // Initial state root
            post_state_root: new_state_root,
            pre_shielded_root: [0u8; 32],
            post_shielded_root: [0u8; 32],
            withdrawal_root: [0u8; 32],
            batch_hash: [0u8; 32],
            batch_id: new_batch_index,
        };

        // Build instruction data
        let mut data = vec![BridgeIx::SubmitBatch as u8];
        data.extend_from_slice(bytemuck::bytes_of(&header));
        data.extend_from_slice(bytemuck::bytes_of(&proof));
        data.extend_from_slice(bytemuck::bytes_of(&public_inputs));

        // Accounts: [sequencer, config, verifier_program, vk_account]
        let accounts = vec![
            AccountMeta::new(*signer, true),
            AccountMeta::new(self.config_pda, false),
            AccountMeta::new_readonly(VERIFIER_PROGRAM_ID, false),
            AccountMeta::new_readonly(self.vk_pda, false),
        ];

        Instruction {
            program_id: self.program_id,
            accounts,
            data,
        }
    }
}

/// Test that submit_batch fails when VK account doesn't exist
/// This is the expected behavior since ZK verification is now required
#[test]
fn test_submit_batch_requires_vk_account() {
    let mut fixture = SubmitBatchFixture::new();
    fixture.initialize_bridge().expect("bridge init failed");

    let ix = fixture.build_submit_batch_ix(
        0,         // prev_batch_index
        1,         // new_batch_index
        [9u8; 32], // new_state_root
        &fixture.sequencer.pubkey(),
    );

    let sequencer = fixture.sequencer.insecure_clone();
    let result = fixture.build_and_send_transaction(&[&sequencer], vec![ix]);

    // Should fail because VK account doesn't exist
    assert!(
        result.is_err(),
        "submit_batch should fail without VK account"
    );
}

#[test]
fn test_submit_batch_wrong_sequencer_fails() {
    let mut fixture = SubmitBatchFixture::new();
    fixture.initialize_bridge().expect("bridge init failed");

    let unauthorized = solana_sdk::signature::Keypair::new();
    fixture
        .svm
        .airdrop(&unauthorized.pubkey(), 10_000_000_000)
        .unwrap();

    let ix = fixture.build_submit_batch_ix(
        0,
        1,
        [1u8; 32],
        &unauthorized.pubkey(), // Wrong sequencer
    );

    let result = fixture.build_and_send_transaction(&[&unauthorized], vec![ix]);
    assert!(result.is_err(), "unauthorized submit must fail");
}

#[test]
fn test_submit_batch_invalid_batch_sequence_fails() {
    let mut fixture = SubmitBatchFixture::new();
    fixture.initialize_bridge().expect("bridge init failed");

    // Try to submit batch 2 when we're at batch 0
    let ix = fixture.build_submit_batch_ix(
        1, // Wrong prev_batch_index (should be 0)
        2, // Wrong new_batch_index (should be 1)
        [9u8; 32],
        &fixture.sequencer.pubkey(),
    );

    let sequencer = fixture.sequencer.insecure_clone();
    let result = fixture.build_and_send_transaction(&[&sequencer], vec![ix]);

    assert!(result.is_err(), "invalid batch sequence must fail");
}
