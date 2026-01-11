use std::{env, str::FromStr};

use solana_client::rpc_client::RpcClient;
use solana_commitment_config::CommitmentConfig;
use solana_sdk::{
    message::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signer::Signer,
    transaction::Transaction,
};
use tokio::time::Duration;
use zelana_keypair::Keypair;
use zelana_transaction::{DepositParams, TransactionData};
use zephyr::client::ZelanaClient;

const MIN_BALANCE: u64 = 2_000_000_000;
const LAMPORTS_PER_SOL: f64 = 1_000_000_000.0;

const DOMAIN: &[u8] = b"solana";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let bridge_id_str = env::var("BRIDGE_PROGRAM_ID")
        .unwrap_or_else(|_| "9HXapBN9otLGnQNGv1HRk91DGqMNvMAvQqohL7gPW1sd".to_string());
    let program_id = Pubkey::from_str(&bridge_id_str)?;

    let home = env::var("HOME")?;
    let user1 = Keypair::from_file(&format!("{}/.config/solana/zelana/test.json", home))?;
    let user2 = Keypair::from_file(&format!("{}/.config/solana/zelana/test2.json", home))?;

    let user1_solkey = Keypair::solana_keypair(&user1);
    let user2_solkey = Keypair::solana_keypair(&user2);
    let user1_id = user1.account_id();
    let user2_id = user2.account_id();

    println!("USER1: {} | {}", user1_id.to_hex(), user1_solkey.pubkey());
    println!("USER2: {} | {}", user2_id.to_hex(), user2_solkey.pubkey());

    let rpc =
        RpcClient::new_with_commitment("http://127.0.0.1:8899", CommitmentConfig::confirmed());

    airdrop_if_needed(&rpc, &user1_solkey.pubkey(), "User1").await?;
    airdrop_if_needed(&rpc, &user2_solkey.pubkey(), "User2").await?;

    // Deposit both users
    deposit_to_l2(&rpc, &program_id, &user1_solkey, 1_300_000_000, 101).await?;
    deposit_to_l2(&rpc, &program_id, &user2_solkey, 1_300_000_000, 102).await?;

    let mut client = ZelanaClient::connect("127.0.0.1:9000").await?;

    let signed_tx = user1.sign_transaction(TransactionData {
        from: user1_id,
        to: user2_id,
        amount: 10,
        nonce: 0,
        chain_id: 1,
    });

    client.send_transaction(signed_tx).await?;
    println!("✓ Transaction sent");

    Ok(())
}

async fn deposit_to_l2(
    rpc: &RpcClient,
    program_id: &Pubkey,
    user_solkey: &solana_sdk::signature::Keypair,
    amount: u64,
    nonce: u64,
) -> anyhow::Result<()> {
    println!("🚀 Depositing {} lamports to Bridge...", amount);

    let mut domain_padded = [0u8; 32];
    domain_padded[..DOMAIN.len()].copy_from_slice(DOMAIN);

    let (config_pda, _) = Pubkey::find_program_address(&[b"config", &domain_padded], program_id);
    let (vault_pda, _) = Pubkey::find_program_address(&[b"vault", &domain_padded], program_id);
    let (receipt_pda, _) = Pubkey::find_program_address(
        &[
            b"receipt",
            &domain_padded,
            user_solkey.pubkey().as_ref(),
            &nonce.to_le_bytes(),
        ],
        program_id,
    );

    let params = DepositParams { amount, nonce };
    let mut data = vec![1];
    data.extend(wincode::serialize(&params)?);

    let system_id = Pubkey::from_str("11111111111111111111111111111111")?;
    let deposit_ix = Instruction {
        program_id: *program_id,
        accounts: vec![
            AccountMeta::new(user_solkey.pubkey(), true),
            AccountMeta::new_readonly(config_pda, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new(receipt_pda, false),
            AccountMeta::new_readonly(system_id, false),
        ],
        data,
    };

    let tx = Transaction::new_signed_with_payer(
        &[deposit_ix],
        Some(&user_solkey.pubkey()),
        &[user_solkey],
        rpc.get_latest_blockhash()?,
    );

    let sig = rpc.send_and_confirm_transaction(&tx)?;
    println!("✅ Deposit Confirmed. Sig: {}", sig);

    Ok(())
}

async fn airdrop_if_needed(rpc: &RpcClient, pubkey: &Pubkey, name: &str) -> anyhow::Result<()> {
    let balance = rpc.get_balance(pubkey)?;
    println!("{}: {:.2} SOL", name, balance as f64 / LAMPORTS_PER_SOL);

    if balance < MIN_BALANCE {
        let sig = rpc.request_airdrop(pubkey, MIN_BALANCE)?;
        while !rpc.confirm_transaction(&sig)? {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        println!("{}: ✓ Airdropped", name);
    }
    Ok(())
}
