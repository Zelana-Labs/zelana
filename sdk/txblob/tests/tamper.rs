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
            chain_id: 1,
        },
        signature: vec![7u8; 64],
        signer_pubkey: [1u8; 32],
    }
}

fn keypair() -> (StaticSecret, PublicKey) {
    let sk = StaticSecret::random();
    let pk = PublicKey::from(&sk);
    (sk, pk)
}

#[test]
fn tampered_ciphertext_fails() {
    let tx = dummy_signed_tx();

    let client_secret = StaticSecret::random();
    let client_pub = PublicKey::from(&client_secret);

    let sequencer_secret = StaticSecret::random();
    let sequencer_pub = PublicKey::from(&sequencer_secret);

    let mut blob =
        encrypt_signed_tx(&tx, &tx.signer_pubkey, &client_secret, &sequencer_pub, 0).unwrap();

    // Flip a bit in ciphertext
    blob.ciphertext[0] ^= 0x01;

    let res = decrypt_signed_tx(&blob, &sequencer_secret, &client_pub);

    assert!(res.is_err());
}

#[test]
fn tampered_flags_fails() {
    let tx = dummy_signed_tx();

    let client_secret = StaticSecret::random();
    let client_pub = PublicKey::from(&client_secret);

    let sequencer_secret = StaticSecret::random();
    let sequencer_pub = PublicKey::from(&sequencer_secret);

    let mut blob =
        encrypt_signed_tx(&tx, &tx.signer_pubkey, &client_secret, &sequencer_pub, 0).unwrap();

    // Tamper with authenticated metadata
    blob.flags ^= 0x01;

    let res = decrypt_signed_tx(&blob, &sequencer_secret, &client_pub);

    assert!(res.is_err());
}
