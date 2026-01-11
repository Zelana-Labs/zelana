use reqwest::Client;
use tokio::time::{Duration, sleep};
use txblob::{EncryptedTxBlobV1, encrypt_signed_tx};
use x25519_dalek::{PublicKey, StaticSecret};
use zelana_keypair::Keypair;
use zelana_transaction::TransactionData;
use zephyr::client::ZelanaClient;
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    // 1. Use a Deterministic Wallet (So we can pre-fund it)
    // We use a seed of all 7s
    let seed = [7u8; 64];
    let wallet = Keypair::from_seed(&seed);
    let id = wallet.account_id();

    println!("Identity: {}", id.to_hex());

    let http = Client::new();
    let url = "http://127.0.0.1:8080/submit_tx";

    let client_secret = StaticSecret::random();
    let client_pub = PublicKey::from(&client_secret);

    let sequencer_pub = PublicKey::from(&StaticSecret::from([42u8; 32]));

    for nonce in 0..5 {
        println!("CLIENT: Sending tx with nonce {}", nonce);

        let tx_data = TransactionData {
            from: id,
            to: id,
            amount: 10,
            nonce,
            chain_id: 1,
        };

        let signed_tx = wallet.sign_transaction(tx_data);

        let encrypted: EncryptedTxBlobV1 = encrypt_signed_tx(
            &signed_tx,
            &wallet.public_keys().signer_pk,
            &client_secret,
            &sequencer_pub,
            0, // flags
        )
        .unwrap();

        let blob_bytes = wincode::serialize(&encrypted)?;

        let res = http
            .post(url)
            .json(&serde_json::json!({
                "blob": blob_bytes,
                "client_pubkey": client_pub.to_bytes(),
            }))
            .send()
            .await?;

        println!("CLIENT: status = {}", res.status());

        sleep(Duration::from_millis(200)).await;
    }

    println!("CLIENT: Done");
    Ok(())
    // let my_id = wallet.account_id();
    // println!("CLIENT: Identity: {}", my_id.to_hex());

    // println!("CLIENT: Connecting...");
    // let mut client = ZelanaClient::connect("127.0.0.1:9000").await?;
    // println!("CLIENT: Connected!");

    // // 2. Send Txs starting from Nonce 0
    // // We send 5 transactions: Nonce 0, 1, 2, 3, 4
    // for i in 5..10 {
    //     println!("CLIENT: Sending Tx #{} (Nonce: {})...", i + 1, i);

    //     let tx_data = TransactionData {
    //         from: my_id,
    //         to: my_id, // Self-transfer
    //         amount: 10,
    //         nonce: i, // <--- CORRECT: Starts at 0
    //         chain_id: 1,
    //     };

    //     let signed_tx = wallet.sign_transaction(tx_data);
    //     client.send_transaction(signed_tx).await?;

    //     sleep(Duration::from_millis(200)).await;
    // }

    // println!("CLIENT: Done.");
    // Ok(())
}
