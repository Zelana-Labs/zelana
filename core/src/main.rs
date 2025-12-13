// Copyright 2025 Zelana Labs
// Licensed under the Apache License, Version 2.0

use core::sequencer::executor::TransactionExecutor;
use core::sequencer::ingest;
use core::sequencer::session::{SessionKeys, SessionManager};
use log::{debug, error, info, warn};
use std::{env, sync::Arc};
use tokio::net::UdpSocket;
use x25519_dalek::PublicKey;
use zelana_transaction::{SignedTransaction, TransactionType};
use zephyr::EphemeralKeyPair;
use zephyr::packet::{KIND_SERVER_HELLO, Packet};

const MAX_DATAGRAM_SIZE: usize = 1500; // Standard MTU safe limit

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    info!("Sequencer  Starting...");

    //Bind UDP Socket
    let socket = Arc::new(UdpSocket::bind("0.0.0.0:9000").await?);
    info!("Listening on UDP 0.0.0.0:9000");

    //Initialize State
    let sessions = Arc::new(SessionManager::new());
    let executor = TransactionExecutor::new("./data/sequencer_db")?;

    let db_handle = executor.db.clone();
    tokio::spawn(async move {
        let bridge_id = env::var("BRIDGE_PROGRAM_ID").unwrap_or_else(|_| "GuiZ...".to_string());
        let wss_url =
            env::var("SOLANA_WSS_URL").unwrap_or_else(|_| "ws://127.0.0.1:8900".to_string());

        ingest::start_indexer(db_handle, wss_url, bridge_id).await;
    });

    let mut buf = [0u8; MAX_DATAGRAM_SIZE];

    loop {
        //Receive Packet
        let (len, peer) = match socket.recv_from(&mut buf).await {
            Ok(v) => v,
            Err(e) => {
                error!("UDP Receive Error: {}", e);
                continue;
            }
        };

        let packet_data = &buf[..len];

        //Zero-Copy Parse
        match Packet::parse(packet_data) {
            Ok(Packet::ClientHello { public_key }) => {
                debug!("ClientHello from {}", peer);

                //Generate Server Ephemeral Keys
                let server_keys = EphemeralKeyPair::generate();
                let server_pub_bytes = *server_keys.pk.as_bytes();

                //Convert client public key bytes → x25519_dalek::PublicKey
                // public_key: & [u8; 32]
                let client_public = PublicKey::from(*public_key);

                //Derive Session (EphemeralSecret × PublicKey → SharedSecret)
                let shared = server_keys.sk.diffie_hellman(&client_public);
                let shared_secret = shared.to_bytes(); // [u8; 32]

                let session = SessionKeys::derive(shared_secret, public_key, &server_pub_bytes);

                //Store Session
                sessions.insert(peer, session);

                //Send ServerHello
                let mut response = Vec::with_capacity(33);
                response.push(KIND_SERVER_HELLO);
                response.extend_from_slice(&server_pub_bytes);

                if let Err(e) = socket.send_to(&response, peer).await {
                    warn!("Failed to send ServerHello to {}: {}", peer, e);
                }
            }

            Ok(Packet::AppData { nonce, ciphertext }) => {
                //Lookup Session
                let decrypted_opt =
                    sessions.get_mut(&peer, |session| session.keys.decrypt(nonce, ciphertext));

                match decrypted_opt {
                    Some(Ok(plaintext)) => {
                        //Handle Transaction
                        match handle_transaction(&plaintext, &executor).await {
                            Ok(_) => debug!("Tx Executed from {}", peer),
                            Err(e) => warn!("Tx Failed from {}: {}", peer, e),
                        }
                    }
                    Some(Err(e)) => {
                        warn!("Decryption failed for {}: {}", peer, e);
                        // Potential Replay Attack or Bad Key - Drop Session
                    }
                    None => {
                        debug!("Unknown Peer {}, ignoring AppData", peer);
                        // Client sent data but we have no session (Server restarted?)
                        // Ideally send a "Reset" packet here so client reconnects
                    }
                }
            }

            Ok(Packet::ServerHello { .. }) => {
                // Clients send ClientHello, not ServerHello. Ignore.
            }

            Err(e) => {
                warn!("Malformed packet from {}: {}", peer, e);
            }
        }
    }
}

/// Decodes and routes the transaction to the executor
async fn handle_transaction(
    plaintext: &[u8],
    executor: &TransactionExecutor,
) -> anyhow::Result<()> {
    //Deserialize
    let tx: TransactionType = wincode::deserialize(plaintext)?;

    match tx {
        TransactionType::Transfer(signed_tx) => {
            //Validate Signature (Anti-Spoofing)
            // Even though ZK proves this later, we MUST check it now to protect the Sequencer.
            verify_signature(&signed_tx)?;

            //Execute
            executor.process(signed_tx).await?;
        }
        _ => {
            // Handle Deposits/Withdrawals
        }
    }
    Ok(())
}

fn verify_signature(tx: &SignedTransaction) -> anyhow::Result<()> {
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};

    let vk = VerifyingKey::from_bytes(&tx.signer_pubkey)?;
    let sig = Signature::from_slice(&tx.signature)?;

    // Re-serialize data to verify (Must match SDK serialization exactly)
    let msg = wincode::serialize(&tx.data)?;

    vk.verify(&msg, &sig)?;
    Ok(())
}
