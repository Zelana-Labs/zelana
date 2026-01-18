// use anyhow::Result;
// use log::info;
// use std::path::PathBuf;

// use x25519_dalek::{PublicKey, StaticSecret};
// use zelana_account::{AccountId, AccountState};
// use zelana_core::sequencer::db::RocksDbStore;
// use zelana_core::sequencer::ingest::state_ingest_server;
// use zelana_core::storage::StateStore;

// #[tokio::main]
// async fn main() -> Result<()> {
//     env_logger::init();

//     // --------------------------------
//     // Local DB path (dev only)
//     // --------------------------------
//     let mut db_path = PathBuf::from("./.dev-db");
//     std::fs::create_dir_all(&db_path)?;
//     db_path.push("rocksdb");

//     let db = RocksDbStore::open(&db_path)?;

//     // --------------------------------
//     // Deterministic sequencer key
//     // --------------------------------
//     let sequencer_secret = StaticSecret::from([42u8; 32]);
//     let sequencer_pub = PublicKey::from(&sequencer_secret);

//     info!("DEV SEQUENCER pubkey: {:?}", sequencer_pub.to_bytes());

//     // --------------------------------
//     // Pre-funded dev account
//     // MUST match client wallet
//     // --------------------------------
//     let dev_account = AccountId(
//         hex::decode("ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d211")?
//             .try_into()
//             .unwrap(),
//     );

//     db.set_account_state(
//         dev_account,
//         AccountState {
//             balance: 1_000,
//             nonce: 0,
//         },
//     )?;

//     info!("Pre-funded account {} with 1000", dev_account.to_hex());

//     // --------------------------------
//     // Start ingest server
//     // --------------------------------
//     let port = 8080;
//     info!("Starting ingest server on {}", port);

//     state_ingest_server(db, sequencer_secret, port).await;

//     Ok(())
// }

fn main() {}
