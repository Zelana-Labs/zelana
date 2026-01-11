use ed25519_dalek::{Signer as EdSigner, SigningKey};
use solana_client::rpc_client::RpcClient;
use solana_commitment_config::CommitmentConfig;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use std::env;
use std::str::FromStr;
use std::time::Duration;
use tokio::time::sleep;
use zelana_account::AccountId;
use zelana_transaction::{DepositParams, SignedTransaction, TransactionData};
use zephyr::client::ZelanaClient;

const DOMAIN: &[u8] = b"solana";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let rpc_url = "http://127.0.0.1:8899";
    let bridge_id_str = env::var("BRIDGE_PROGRAM_ID")
        .unwrap_or_else(|_| "9HXapBN9otLGnQNGv1HRk91DGqMNvMAvQqohL7gPW1sd".to_string());
    let program_id = Pubkey::from_str(&bridge_id_str)?;
    let sequencer_url = "127.0.0.1:9000";

    // 1. Setup Identity
    let user = Keypair::new();
    println!("👤 User Identity: {}", user.pubkey());

    let mut acc_bytes = [0u8; 32];
    acc_bytes.copy_from_slice(user.pubkey().as_ref());
    let my_l2_id = AccountId(acc_bytes);

    // 2. Fund L1 Account
    println!("💸 Airdropping L1 SOL...");
    let rpc = RpcClient::new_with_commitment(rpc_url.to_string(), CommitmentConfig::confirmed());
    let sig = rpc.request_airdrop(&user.pubkey(), 2_000_000_000)?;
    while !rpc.confirm_transaction(&sig)? {
        sleep(Duration::from_millis(100)).await;
    }

    // 3. DEPOSIT to L2
    println!("🚀 Depositing 1 SOL to Bridge...");

    // Derive PDAs - matching working example exactly
    let mut domain_padded = [0u8; 32];
    domain_padded[..DOMAIN.len()].copy_from_slice(DOMAIN);

    let (config_pda, _) = Pubkey::find_program_address(&[b"config", &domain_padded], &program_id);
    let (vault_pda, _) = Pubkey::find_program_address(&[b"vault", &domain_padded], &program_id);

    let nonce: u64 = 500;
    let (receipt_pda, _) = Pubkey::find_program_address(
        &[
            b"receipt",
            &domain_padded,
            user.pubkey().as_ref(),
            &nonce.to_le_bytes(),
        ],
        &program_id,
    );

    let amount = 1_000_000_000;
    let params = DepositParams { amount, nonce };

    // Use wincode serialization like the working example
    let mut data = vec![1];
    data.extend(wincode::serialize(&params)?);

    let system_id = Pubkey::from_str("11111111111111111111111111111111")?;

    let deposit_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(user.pubkey(), true),
            AccountMeta::new_readonly(config_pda, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new(receipt_pda, false),
            AccountMeta::new_readonly(system_id, false),
        ],
        data,
    };

    let tx = Transaction::new_signed_with_payer(
        &[deposit_ix],
        Some(&user.pubkey()),
        &[&user],
        rpc.get_latest_blockhash()?,
    );

    let sig = rpc.send_and_confirm_transaction(&tx)?;
    println!("✅ Deposit Confirmed on L1. Sig: {}", sig);

    // 4. Wait for Indexer
    println!("⏳ Waiting 5s for Sequencer to index...");
    sleep(Duration::from_secs(5)).await;

    // 5. Connect to L2
    println!("🔌 Connecting to Zelana L2...");
    let mut client = ZelanaClient::connect(sequencer_url).await?;

    // 6. Send L2 Transfer
    println!("💸 Sending L2 Transfer...");

    let tx_data = TransactionData {
        from: my_l2_id,
        to: my_l2_id,
        amount: 50,
        nonce: 0,
        chain_id: 1,
    };

    let msg = wincode::serialize(&tx_data)?;
    let signing_key = SigningKey::from_bytes(&user.secret_bytes()[0..32].try_into().unwrap());
    let signature = signing_key.sign(&msg).to_bytes().to_vec();

    let signed_tx = SignedTransaction {
        data: tx_data,
        signature,
        signer_pubkey: user.pubkey().to_bytes(),
    };

    client.send_transaction(signed_tx).await?;
    println!("🎉 L2 Transaction Sent! Check Sequencer logs for 'COMMITTED'.");

    Ok(())
}
