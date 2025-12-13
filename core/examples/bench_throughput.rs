use std::time::Instant;
use zelana_keypair::Keypair;
use zelana_transaction::TransactionData;
use zephyr::client::ZelanaClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let wallet = Keypair::new_random();
    let mut client = ZelanaClient::connect("127.0.0.1:9000").await?;

    // Prepare a tx
    let tx = wallet.sign_transaction(TransactionData {
        from: wallet.account_id(),
        to: wallet.account_id(),
        amount: 1,
        nonce: 0, // In bench we ignore nonce ordering for raw speed
        chain_id: 1,
    });

    let count = 10_000;
    println!("Starting Benchmark: {} transactions via UDP...", count);

    let start = Instant::now();

    for _ in 0..count {
        client.send_transaction(tx.clone()).await?;
    }

    let duration = start.elapsed();
    let tps = count as f64 / duration.as_secs_f64();

    println!("Finished!");
    println!("Total Time: {:?}", duration);
    println!("Throughput: {:.2} TPS (Client-Side Send Rate)", tps);
    println!("Note: This measures how fast we can encrypt & blast UDP.");

    Ok(())
}
