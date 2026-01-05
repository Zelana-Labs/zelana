use anyhow::Result;
use solana_sdk::signer::Signer;
use std::env;
use zelana_keypair::Keypair;
use zelana_transaction::{Transaction, TransactionData, TransactionType};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let home = env::var("HOME")?;
    let user1 = Keypair::from_file(&format!("{}/.config/solana/zelana/test.json", home))?;
    let user2 = Keypair::from_file(&format!("{}/.config/solana/zelana/test2.json", home))?;

    let user1_id = user1.account_id();
    let user2_id = user2.account_id();

    println!("USER1: {}", user1_id.to_hex());
    println!("USER2: {}", user2_id.to_hex());

    // Create transaction data
    let tx_data = TransactionData {
        from: user1_id,
        to: user2_id,
        amount: 100,
        nonce: 9,
        chain_id: 1,
    };

    // Sign the transaction using user1's keypair
    let signed_tx = user1.sign_transaction(tx_data);

    // Wrap in Transaction type that the server expects
    let tx = Transaction {
        tx_type: TransactionType::Transfer(signed_tx.clone()),
        sender: zelana_pubkey::Pubkey(user1.public_keys().signer_pk),
        signature: signed_tx.signature,
    };

    // Serialize with wincode
    let encoded = wincode::serialize(&tx)?;

    println!("Sending transaction to sequencer...");
    println!("Serialized size: {} bytes", encoded.len());

    // Send POST request
    let client = reqwest::Client::new();
    let response = client
        .post("http://localhost:8080/submit_tx")
        .header("Content-Type", "application/octet-stream")
        .body(encoded)
        .send()
        .await?;

    println!("Status: {}", response.status());
    println!("Response: {}", response.text().await?);

    Ok(())
}