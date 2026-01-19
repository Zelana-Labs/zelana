use txblob::{encrypt_signed_tx, tx_blob_hash};
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
fn identical_blob_has_same_hash() {
    let tx = dummy_signed_tx();

    let client_secret = StaticSecret::random();
    let sequencer_secret = StaticSecret::random();
    let sequencer_pub = PublicKey::from(&sequencer_secret);

    let blob =
        encrypt_signed_tx(&tx, &tx.signer_pubkey, &client_secret, &sequencer_pub, 0).unwrap();

    let h1 = tx_blob_hash(&blob);
    let h2 = tx_blob_hash(&blob);

    assert_eq!(h1, h2);
}

#[test]
fn different_blobs_have_different_hashes() {
    let tx = dummy_signed_tx();

    let client_secret = StaticSecret::random();
    let sequencer_secret = StaticSecret::random();
    let sequencer_pub = PublicKey::from(&sequencer_secret);

    let blob1 =
        encrypt_signed_tx(&tx, &tx.signer_pubkey, &client_secret, &sequencer_pub, 0).unwrap();

    let blob2 =
        encrypt_signed_tx(&tx, &tx.signer_pubkey, &client_secret, &sequencer_pub, 0).unwrap();

    let h1 = tx_blob_hash(&blob1);
    let h2 = tx_blob_hash(&blob2);

    assert_ne!(h1, h2);
}
