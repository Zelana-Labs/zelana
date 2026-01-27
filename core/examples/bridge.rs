use solana_client::rpc_client::RpcClient;
use solana_commitment_config::CommitmentConfig;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use std::str::FromStr;
use zelana_transaction::{DepositParams, InitParams};
use zelana_config::ZelanaConfig;

const DOMAIN: &[u8] = b"solana";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = ZelanaConfig::global();

    let rpc_url = &config.solana.rpc_url;
    let program_id = Pubkey::from_str(&config.solana.bridge_program_id)?;

    let payer = Keypair::new();
    let sequencer = Keypair::new();

    let rpc = RpcClient::new_with_commitment(rpc_url.to_string(), CommitmentConfig::confirmed());

    // Airdrop
    let sig = rpc.request_airdrop(&payer.pubkey(), 2_000_000_000)?;
    while !rpc.confirm_transaction(&sig)? {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    // Derive PDAs
    let mut domain_padded = [0u8; 32];
    domain_padded[..DOMAIN.len()].copy_from_slice(DOMAIN);

    let (config_pda, _) = Pubkey::find_program_address(&[b"config", &domain_padded], &program_id);
    let (vault_pda, _) = Pubkey::find_program_address(&[b"vault", &domain_padded], &program_id);
    let system_id = Pubkey::from_str("11111111111111111111111111111111")?;

    // Initialize bridge if needed
    if rpc.get_account(&config_pda).is_err() {
        let init_params = InitParams {
            sequencer_authority: sequencer.pubkey().to_bytes(),
            domain: domain_padded,
        };

        let mut init_data = vec![0];
        init_data.extend(wincode::serialize(&init_params)?);

        let init_ix = Instruction {
            program_id,
            accounts: vec![
                AccountMeta::new(payer.pubkey(), true),
                AccountMeta::new(config_pda, false),
                AccountMeta::new(vault_pda, false),
                AccountMeta::new_readonly(system_id, false),
            ],
            data: init_data,
        };

        let tx = Transaction::new_signed_with_payer(
            &[init_ix],
            Some(&payer.pubkey()),
            &[&payer],
            rpc.get_latest_blockhash()?,
        );

        rpc.send_and_confirm_transaction(&tx)?;
        println!("✅ Bridge Initialized");
    }

    // Deposit
    let nonce: u64 = 619;
    let (receipt_pda, _) = Pubkey::find_program_address(
        &[
            b"receipt",
            &domain_padded,
            payer.pubkey().as_ref(),
            &nonce.to_le_bytes(),
        ],
        &program_id,
    );

    let params = DepositParams {
        amount: 1_000_000_000,
        nonce,
    };
    let mut deposit_data = vec![1];
    deposit_data.extend(wincode::serialize(&params)?);

    let deposit_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new_readonly(config_pda, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new(receipt_pda, false),
            AccountMeta::new_readonly(system_id, false),
        ],
        data: deposit_data,
    };

    let tx = Transaction::new_signed_with_payer(
        &[deposit_ix],
        Some(&payer.pubkey()),
        &[&payer],
        rpc.get_latest_blockhash()?,
    );

    let sig = rpc.send_and_confirm_transaction(&tx)?;
    println!("✅ Deposit Confirmed! Sig: {}", sig);

    Ok(())
}
