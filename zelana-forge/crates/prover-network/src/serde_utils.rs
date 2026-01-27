//! Serialization utilities for arkworks types.
//!
//! Provides Base64 encoding/decoding for field elements and group elements
//! using arkworks' canonical serialization format.

use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use prover_core::{Fr, G1Affine};
use serde::{Deserialize, Deserializer, Serializer};
use thiserror::Error;

/// Errors that can occur during serialization
#[derive(Error, Debug)]
pub enum SerializationError {
    #[error("Failed to serialize: {0}")]
    SerializeError(String),

    #[error("Failed to deserialize: {0}")]
    DeserializeError(String),

    #[error("Invalid base64: {0}")]
    Base64Error(String),
}

/// Serialize a field element to base64 string
pub fn serialize_fr<S>(fr: &Fr, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut bytes = Vec::new();
    fr.serialize_compressed(&mut bytes)
        .map_err(|e| serde::ser::Error::custom(format!("Failed to serialize Fr: {}", e)))?;

    // Manual base64 encoding using standard alphabet
    let encoded = base64_encode(&bytes);
    serializer.serialize_str(&encoded)
}

/// Deserialize a field element from base64 string
pub fn deserialize_fr<'de, D>(deserializer: D) -> Result<Fr, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    let bytes = base64_decode(&s)
        .map_err(|e| serde::de::Error::custom(format!("Invalid base64: {}", e)))?;

    Fr::deserialize_compressed(&bytes[..])
        .map_err(|e| serde::de::Error::custom(format!("Failed to deserialize Fr: {}", e)))
}

/// Serialize a group element to base64 string
pub fn serialize_g1<S>(point: &G1Affine, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut bytes = Vec::new();
    point
        .serialize_compressed(&mut bytes)
        .map_err(|e| serde::ser::Error::custom(format!("Failed to serialize G1: {}", e)))?;

    let encoded = base64_encode(&bytes);
    serializer.serialize_str(&encoded)
}

/// Deserialize a group element from base64 string
pub fn deserialize_g1<'de, D>(deserializer: D) -> Result<G1Affine, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    let bytes = base64_decode(&s)
        .map_err(|e| serde::de::Error::custom(format!("Invalid base64: {}", e)))?;

    G1Affine::deserialize_compressed(&bytes[..])
        .map_err(|e| serde::de::Error::custom(format!("Failed to deserialize G1: {}", e)))
}

/// Base64 encoding using standard alphabet (no external crate)
fn base64_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut result = Vec::new();
    let mut i = 0;

    while i + 2 < input.len() {
        let b1 = input[i];
        let b2 = input[i + 1];
        let b3 = input[i + 2];

        result.push(ALPHABET[(b1 >> 2) as usize]);
        result.push(ALPHABET[(((b1 & 0x03) << 4) | (b2 >> 4)) as usize]);
        result.push(ALPHABET[(((b2 & 0x0F) << 2) | (b3 >> 6)) as usize]);
        result.push(ALPHABET[(b3 & 0x3F) as usize]);

        i += 3;
    }

    // Handle remaining bytes
    match input.len() - i {
        1 => {
            let b1 = input[i];
            result.push(ALPHABET[(b1 >> 2) as usize]);
            result.push(ALPHABET[((b1 & 0x03) << 4) as usize]);
            result.push(b'=');
            result.push(b'=');
        }
        2 => {
            let b1 = input[i];
            let b2 = input[i + 1];
            result.push(ALPHABET[(b1 >> 2) as usize]);
            result.push(ALPHABET[(((b1 & 0x03) << 4) | (b2 >> 4)) as usize]);
            result.push(ALPHABET[((b2 & 0x0F) << 2) as usize]);
            result.push(b'=');
        }
        _ => {}
    }

    String::from_utf8(result).expect("Base64 output should be valid UTF-8")
}

/// Base64 decoding using standard alphabet (no external crate)
fn base64_decode(input: &str) -> Result<Vec<u8>, SerializationError> {
    let input = input.as_bytes();

    // Create reverse lookup table
    let mut decode_table = [0xFF; 256];
    for (i, &c) in b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
        .iter()
        .enumerate()
    {
        decode_table[c as usize] = i as u8;
    }

    let mut result = Vec::new();
    let mut i = 0;

    while i + 3 < input.len() {
        let c1 = decode_table[input[i] as usize];
        let c2 = decode_table[input[i + 1] as usize];
        let c3 = decode_table[input[i + 2] as usize];
        let c4 = decode_table[input[i + 3] as usize];

        if c1 == 0xFF || c2 == 0xFF {
            return Err(SerializationError::Base64Error(
                "Invalid character".to_string(),
            ));
        }

        result.push((c1 << 2) | (c2 >> 4));

        if input[i + 2] != b'=' {
            result.push((c2 << 4) | (c3 >> 2));
        }

        if input[i + 3] != b'=' {
            result.push((c3 << 6) | c4);
        }

        i += 4;
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ark_std::{test_rng, UniformRand};

    #[test]
    fn test_base64_encode_decode() {
        let test_cases = vec![
            b"hello".as_slice(),
            b"world",
            b"a",
            b"ab",
            b"abc",
            b"",
            &[0u8; 32],
            &[255u8; 32],
        ];

        for input in test_cases {
            let encoded = base64_encode(input);
            let decoded = base64_decode(&encoded).unwrap();
            assert_eq!(input, decoded.as_slice());
        }
    }

    #[test]
    fn test_fr_serialization() {
        let mut rng = test_rng();

        for _ in 0..10 {
            let original = Fr::rand(&mut rng);

            // Serialize to string
            let mut bytes = Vec::new();
            original.serialize_compressed(&mut bytes).unwrap();
            let encoded = base64_encode(&bytes);

            // Deserialize back
            let decoded_bytes = base64_decode(&encoded).unwrap();
            let recovered = Fr::deserialize_compressed(&decoded_bytes[..]).unwrap();

            assert_eq!(original, recovered);
        }
    }

    #[test]
    fn test_g1_serialization() {
        use ark_ec::CurveGroup;
        use prover_core::G1Projective;
        let mut rng = test_rng();

        for _ in 0..10 {
            let original = G1Projective::rand(&mut rng).into_affine();

            // Serialize to string
            let mut bytes = Vec::new();
            original.serialize_compressed(&mut bytes).unwrap();
            let encoded = base64_encode(&bytes);

            // Deserialize back
            let decoded_bytes = base64_decode(&encoded).unwrap();
            let recovered = G1Affine::deserialize_compressed(&decoded_bytes[..]).unwrap();

            assert_eq!(original, recovered);
        }
    }
}
