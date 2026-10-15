//! What a PDP verifies comes from a database someone may write to (ADR 0080). Whatever the
//! payload, signature, and key ID: verifying never panics, nothing unsigned verifies, a
//! signature verifies its own payload, and changing one byte of either breaks it.
#![no_main]

use arbitrary::Arbitrary;
use awzy_signing::{BundleSigner, BundleVerifier};
use libfuzzer_sys::fuzz_target;
use p256::ecdsa::SigningKey;

#[derive(Debug, Arbitrary)]
struct Input<'a> {
    payload: &'a [u8],
    signature: &'a [u8],
    key_id: &'a str,
    flip: usize,
}

fuzz_target!(|input: Input<'_>| {
    let key = SigningKey::from_slice(&[7; 32]).expect("a fixed 32-byte scalar in range");
    let verifier = BundleVerifier::from_keys([*key.verifying_key()]).expect("a P-256 key has a key ID");
    let signer = BundleSigner::from_key(key).expect("a P-256 key signs");

    // Forged: an arbitrary signature, under any key ID, never verifies.
    assert!(verifier.verify_payload(input.payload, input.signature, input.key_id).is_err());
    assert!(verifier.verify_payload(input.payload, input.signature, signer.key_id()).is_err());

    // Genuine: verifies, and not once a byte of the payload or signature changes.
    let signature = signer.sign_payload(input.payload);
    assert!(verifier.verify_payload(input.payload, &signature, signer.key_id()).is_ok());
    if !input.payload.is_empty() {
        let mut payload = input.payload.to_vec();
        let i = input.flip % payload.len();
        payload[i] ^= 0x01;
        assert!(verifier.verify_payload(&payload, &signature, signer.key_id()).is_err());
    }
    let mut forged = signature.clone();
    let i = input.flip % forged.len();
    forged[i] ^= 0x01;
    assert!(verifier.verify_payload(input.payload, &forged, signer.key_id()).is_err());
});
