use bridge_z::{helpers::StateDefinition, instruction::{BridgeIx, InitParams}, state::{Config, Vault}, ID};
use litesvm::{types::{FailedTransactionMetadata, TransactionMetadata}, LiteSVM};
use solana_sdk::{instruction::{AccountMeta, Instruction}, message::{v0, VersionedMessage}, pubkey::Pubkey, signature::Keypair, signer::Signer, system_instruction, system_program, transaction::{Transaction, VersionedTransaction}};


pub fn setup_svm_and_program() -> (LiteSVM, Keypair, Keypair, Pubkey) {
    let mut svm = LiteSVM::new();
    let fee_payer = Keypair::new();

    svm.airdrop(&fee_payer.pubkey(), 100000000).unwrap();

    let program_id = Pubkey::from(ID);
    svm.add_program_from_file(program_id, "./target/deploy/pinocchio_multisig.so")
        .unwrap();

    let second_keypair = Keypair::new();
    svm.airdrop(&second_keypair.pubkey(), 1000000000).unwrap();

    (svm, fee_payer, second_keypair, program_id)
}

pub struct TestFixture{
    pub svm :LiteSVM,
    pub payer : Keypair,
    pub sequencer:Keypair,
    pub config_pda:Pubkey,
    pub vault_pda:Pubkey
}

impl TestFixture{
    pub fn new()->Self{
        let mut svm = LiteSVM::new();
        let payer = Keypair::new();
        let sequencer = Keypair::new();
        svm.airdrop(&payer.pubkey(), 10_000_000_000).unwrap();
        let  pubket = Pubkey::from(ID);

        svm.airdrop(&sequencer.pubkey(), 10_000_000_000).unwrap();
        
        svm.add_program_from_file(pubket, "./target/deploy/bridge_z.so").unwrap();

        let (config_pda,_) = Pubkey::find_program_address(&[Config::SEED.as_bytes()], &pubket);
        let (vault_pda,_) = Pubkey::find_program_address(&[Vault::SEED.as_bytes(), config_pda.as_ref()], &pubket);

        Self{
            svm,
            payer,
            sequencer,
            config_pda,
            vault_pda
        }
    }
    pub fn build_and_send_transaction(
        &mut self,
        signers: &[&Keypair],
        instruction: Vec<Instruction>,
        ) -> Result<TransactionMetadata, FailedTransactionMetadata> {
        let msg = v0::Message::try_compile(
            &self.payer.pubkey(),
            &instruction,
            &[],
            self.svm.latest_blockhash(),
        )
        .unwrap();

        let mut all_signers = vec![&self.payer];
        all_signers.extend(signers);

        let tx = VersionedTransaction::try_new(VersionedMessage::V0(msg), &all_signers).unwrap();

        self.svm.send_transaction(tx)
    }

    pub fn initialize_bridge(&mut self)->Result<TransactionMetadata,FailedTransactionMetadata>{
        let sequencer_pubkey = self.sequencer.pubkey();
        let ix_data = InitParams{
            sequencer_authority: *sequencer_pubkey.as_array(),
            domain: [1u8;32]
        };
        let mut instruction_data = vec![BridgeIx::INIT as u8];
        instruction_data.extend_from_slice(bytemuck::bytes_of(&ix_data));

        let accounts = vec![
            AccountMeta::new(self.payer.pubkey(), true),
            AccountMeta::new(self.config_pda, false),
            AccountMeta::new(self.vault_pda,false),
            AccountMeta::new(system_program::ID,false)
        ];

        let init_ix = Instruction{
            program_id:Pubkey::from(ID),
            accounts,
            data:instruction_data
        };

 self.build_and_send_transaction(&[], vec![init_ix])
        
    }

    pub fn fund_vault(&mut self, amount: u64) -> Result<TransactionMetadata, FailedTransactionMetadata> {
        let transfer_ix =
            system_instruction::transfer(&self.payer.pubkey(), &self.vault_pda, amount);

        self.build_and_send_transaction(&[], vec![transfer_ix])
    }
}