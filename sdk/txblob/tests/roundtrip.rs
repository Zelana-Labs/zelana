use txblob::{decrypt_signed_tx, encrypt_signed_tx};
use x25519_dalek::{PublicKey, StaticSecret};
use zelana_account::AccountId;
use zelana_transaction::{SignedTransaction, TransactionData};

fn dummy_signed_tx() -> SignedTransaction {
    SignedTransaction {
        data: TransactionData {
            from: AccountId([1u8; 32]),
            to: AccountId([2u8; 32]),
            amount: 10,
            nonce: 0,
            chain_id: 1, // REQUIRED
        },
        signature: vec![7u8; 64],
        signer_pubkey: [1u8; 32],
    }
}

#[test]
fn encrypt_decrypt_roundtrip() {
    let tx = dummy_signed_tx();

    let client_secret = StaticSecret::random();
    let client_pub = PublicKey::from(&client_secret);

    let sequencer_secret = StaticSecret::random();
    let sequencer_pub = PublicKey::from(&sequencer_secret);

    let blob = encrypt_signed_tx(
        &tx,
        &tx.signer_pubkey,
        &client_secret,
        &sequencer_pub,
        0, // flags
    )
    .expect("encryption failed");

    let decrypted =
        decrypt_signed_tx(&blob, &sequencer_secret, &client_pub).expect("decryption failed");

    assert_eq!(tx.data, decrypted.data);
    assert_eq!(tx.signature, decrypted.signature);
    assert_eq!(tx.signer_pubkey, decrypted.signer_pubkey);
}
