//! WASM Bindings for Ownership Prover
//!
//! This module provides JavaScript-friendly bindings for the ownership prover.
//! It's designed to be used in the browser via the TypeScript SDK.
//!
//! # Usage from JavaScript
//!
//! ```javascript
//! import init, { computeCommitment, computeNullifier, generateWitness } from '@zelana/ownership-prover';
//!
//! await init();
//!
//! const witness = generateWitness(spendingKeyHex, noteValue, blindingHex, position);
//! console.log(witness);
//! // { commitment: "...", nullifier: "...", blindedProxy: "..." }
//! ```

use crate::{
    bytes_to_field, compute_blinded_proxy as rust_compute_blinded_proxy,
    compute_commitment as rust_compute_commitment, compute_nullifier as rust_compute_nullifier,
    derive_public_key, field_to_bytes, OwnershipWitness,
};
use wasm_bindgen::prelude::*;

/// Initialize panic hook for better error messages in browser console
#[wasm_bindgen(start)]
pub fn init_panic_hook() {
    console_error_panic_hook::set_once();
}

/// Parse a hex string to bytes
fn hex_to_bytes32(hex: &str) -> Result<[u8; 32], JsValue> {
    let hex = hex.strip_prefix("0x").unwrap_or(hex);
    let bytes = hex::decode(hex).map_err(|e| JsValue::from_str(&format!("Invalid hex: {}", e)))?;

    if bytes.len() != 32 {
        return Err(JsValue::from_str(&format!(
            "Expected 32 bytes, got {}",
            bytes.len()
        )));
    }

    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

/// Convert bytes to hex string
fn bytes_to_hex(bytes: &[u8]) -> String {
    hex::encode(bytes)
}

/// Derive public key from spending key
///
/// @param spending_key_hex - 32-byte spending key as hex string
/// @returns Public key as hex string (32 bytes)
#[wasm_bindgen(js_name = derivePublicKey)]
pub fn wasm_derive_public_key(spending_key_hex: &str) -> Result<String, JsValue> {
    let spending_key = hex_to_bytes32(spending_key_hex)?;
    let sk = bytes_to_field(&spending_key);
    let pk = derive_public_key(sk);
    Ok(bytes_to_hex(&field_to_bytes(pk)))
}

/// Compute note commitment
///
/// @param owner_pk_hex - Owner's public key as hex (32 bytes)
/// @param value - Note value in lamports (u64)
/// @param blinding_hex - Random blinding factor as hex (32 bytes)
/// @returns Commitment as hex string (32 bytes)
#[wasm_bindgen(js_name = computeCommitment)]
pub fn wasm_compute_commitment(
    owner_pk_hex: &str,
    value: u64,
    blinding_hex: &str,
) -> Result<String, JsValue> {
    let owner_pk = hex_to_bytes32(owner_pk_hex)?;
    let blinding = hex_to_bytes32(blinding_hex)?;

    let pk = bytes_to_field(&owner_pk);
    let b = bytes_to_field(&blinding);
    let cm = rust_compute_commitment(pk, value, b);

    Ok(bytes_to_hex(&field_to_bytes(cm)))
}

/// Compute nullifier
///
/// @param spending_key_hex - Spending key as hex (32 bytes)
/// @param commitment_hex - Note commitment as hex (32 bytes)
/// @param position - Note position in commitment tree (u64)
/// @returns Nullifier as hex string (32 bytes)
#[wasm_bindgen(js_name = computeNullifier)]
pub fn wasm_compute_nullifier(
    spending_key_hex: &str,
    commitment_hex: &str,
    position: u64,
) -> Result<String, JsValue> {
    let spending_key = hex_to_bytes32(spending_key_hex)?;
    let commitment = hex_to_bytes32(commitment_hex)?;

    let sk = bytes_to_field(&spending_key);
    let cm = bytes_to_field(&commitment);
    let nf = rust_compute_nullifier(sk, cm, position);

    Ok(bytes_to_hex(&field_to_bytes(nf)))
}

/// Compute blinded proxy for delegation
///
/// @param commitment_hex - Note commitment as hex (32 bytes)
/// @param position - Note position in commitment tree (u64)
/// @returns Blinded proxy as hex string (32 bytes)
#[wasm_bindgen(js_name = computeBlindedProxy)]
pub fn wasm_compute_blinded_proxy(commitment_hex: &str, position: u64) -> Result<String, JsValue> {
    let commitment = hex_to_bytes32(commitment_hex)?;
    let cm = bytes_to_field(&commitment);
    let bp = rust_compute_blinded_proxy(cm, position);
    Ok(bytes_to_hex(&field_to_bytes(bp)))
}

/// Generate complete witness for ownership proof
///
/// This computes all public outputs from the private inputs.
///
/// @param spending_key_hex - Spending key as hex (32 bytes)
/// @param value - Note value in lamports (u64)
/// @param blinding_hex - Random blinding factor as hex (32 bytes)
/// @param position - Note position in commitment tree (u64)
/// @returns JSON object with commitment, nullifier, and blindedProxy
#[wasm_bindgen(js_name = generateWitness)]
pub fn wasm_generate_witness(
    spending_key_hex: &str,
    value: u64,
    blinding_hex: &str,
    position: u64,
) -> Result<JsValue, JsValue> {
    let spending_key = hex_to_bytes32(spending_key_hex)?;
    let blinding = hex_to_bytes32(blinding_hex)?;

    let sk = bytes_to_field(&spending_key);
    let b = bytes_to_field(&blinding);

    let witness = OwnershipWitness::from_private_inputs(sk, value, b, position);

    // Return as JSON object
    let result = serde_json::json!({
        "commitment": bytes_to_hex(&field_to_bytes(witness.commitment)),
        "nullifier": bytes_to_hex(&field_to_bytes(witness.nullifier)),
        "blindedProxy": bytes_to_hex(&field_to_bytes(witness.blinded_proxy)),
        "ownerPk": bytes_to_hex(&field_to_bytes(derive_public_key(sk))),
    });

    serde_wasm_bindgen::to_value(&result)
        .map_err(|e| JsValue::from_str(&format!("JSON error: {}", e)))
}

/// Verify that computed values match expected values
///
/// This is useful for debugging before generating a proof.
///
/// @param spending_key_hex - Spending key as hex (32 bytes)
/// @param value - Note value in lamports (u64)
/// @param blinding_hex - Random blinding factor as hex (32 bytes)
/// @param position - Note position in commitment tree (u64)
/// @param expected_commitment_hex - Expected commitment as hex (32 bytes)
/// @param expected_nullifier_hex - Expected nullifier as hex (32 bytes)
/// @param expected_proxy_hex - Expected blinded proxy as hex (32 bytes)
/// @returns true if all values match, false otherwise
#[wasm_bindgen(js_name = verifyWitness)]
pub fn wasm_verify_witness(
    spending_key_hex: &str,
    value: u64,
    blinding_hex: &str,
    position: u64,
    expected_commitment_hex: &str,
    expected_nullifier_hex: &str,
    expected_proxy_hex: &str,
) -> Result<bool, JsValue> {
    let spending_key = hex_to_bytes32(spending_key_hex)?;
    let blinding = hex_to_bytes32(blinding_hex)?;
    let expected_commitment = hex_to_bytes32(expected_commitment_hex)?;
    let expected_nullifier = hex_to_bytes32(expected_nullifier_hex)?;
    let expected_proxy = hex_to_bytes32(expected_proxy_hex)?;

    let sk = bytes_to_field(&spending_key);
    let b = bytes_to_field(&blinding);

    let witness = OwnershipWitness::from_private_inputs(sk, value, b, position);

    Ok(field_to_bytes(witness.commitment) == expected_commitment
        && field_to_bytes(witness.nullifier) == expected_nullifier
        && field_to_bytes(witness.blinded_proxy) == expected_proxy)
}
