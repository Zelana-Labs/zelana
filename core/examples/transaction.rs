use tokio::time::{Duration, sleep};
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

    let my_id = wallet.account_id();
    println!("CLIENT: Identity: {}", my_id.to_hex());

    println!("CLIENT: Connecting...");
    let mut client = ZelanaClient::connect("127.0.0.1:9000").await?;
    println!("CLIENT: Connected!");

    // 2. Send Txs starting from Nonce 0
    // We send 5 transactions: Nonce 0, 1, 2, 3, 4
    for i in 5..10 {
        println!("CLIENT: Sending Tx #{} (Nonce: {})...", i + 1, i);

        let tx_data = TransactionData {
            from: my_id,
            to: my_id, // Self-transfer
            amount: 10,
            nonce: i, // <--- CORRECT: Starts at 0
            chain_id: 1,
        };

        let signed_tx = wallet.sign_transaction(tx_data);
        client.send_transaction(signed_tx).await?;

        sleep(Duration::from_millis(200)).await;
    }

    println!("CLIENT: Done.");
    Ok(())
}
