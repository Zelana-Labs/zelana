#[cfg(test)]
mod tests {
    use chacha20poly1305::aead::rand_core::OsRng;
    use x25519_dalek::{PublicKey, StaticSecret};
    use zelana_transaction::{SignedTransaction, TransactionData};

    use crate::{decrypt_signed_tx, encrypt_signed_tx};
    fn dummy_tx() -> SignedTransaction {
        SignedTransaction {
            data: TransactionData::default(),
            signature: vec![1u8; 64],
            signer_pubkey: [9u8; 32],
        }
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let client_secret = StaticSecret::random_from_rng(OsRng);
        let sequencer_secret = StaticSecret::random_from_rng(OsRng);

        let client_pub = PublicKey::from(&client_secret);
        let sequencer_pub = PublicKey::from(&sequencer_secret);

        let tx = dummy_tx();

        let blob =
            encrypt_signed_tx(&tx, &tx.signer_pubkey, &client_secret, &sequencer_pub, 0).unwrap();

        let recovered = decrypt_signed_tx(&blob, &sequencer_secret, &client_pub).unwrap();
        assert_eq!(
            wincode::serialize(&tx).unwrap(),
            wincode::serialize(&recovered).unwrap()
        );
    }

    #[test]
    fn tamper_flags_fails() {
        let client_secret = StaticSecret::random_from_rng(OsRng);
        let sequencer_secret = StaticSecret::random_from_rng(OsRng);

        let client_pub = PublicKey::from(&client_secret);
        let sequencer_pub = PublicKey::from(&sequencer_secret);

        let tx = dummy_tx();

        let mut blob =
            encrypt_signed_tx(&tx, &tx.signer_pubkey, &client_secret, &sequencer_pub, 0).unwrap();

        blob.flags = 1; // tamper

        let result = decrypt_signed_tx(&blob, &sequencer_secret, &client_pub);
        assert!(result.is_err());
    }
}
