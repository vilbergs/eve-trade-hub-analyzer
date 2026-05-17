//! AES-256-GCM encryption for refresh tokens at rest.
//!
//! Blob layout: `0x01 || nonce(12) || ciphertext+tag`.
//! The leading version byte lets us rotate to a different scheme later
//! without ambiguity. Decryption refuses anything that doesn't start with
//! `0x01` and verifies the GCM tag, so any tampering surfaces as an error.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use rand::RngCore;
use rand::rngs::OsRng;

use crate::error::{AppError, AppResult};

const VERSION: u8 = 0x01;
const NONCE_LEN: usize = 12;

pub fn encrypt(key: &[u8; 32], plaintext: &str) -> AppResult<Vec<u8>> {
    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| AppError::Other(format!("aes-gcm encrypt: {e}")))?;

    let mut out = Vec::with_capacity(1 + NONCE_LEN + ciphertext.len());
    out.push(VERSION);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

pub fn decrypt(key: &[u8; 32], blob: &[u8]) -> AppResult<String> {
    if blob.len() < 1 + NONCE_LEN {
        return Err(AppError::Other("encrypted blob too short".into()));
    }
    if blob[0] != VERSION {
        return Err(AppError::Other(format!(
            "unsupported encrypted blob version: {:#x}",
            blob[0]
        )));
    }
    let nonce = Nonce::from_slice(&blob[1..1 + NONCE_LEN]);
    let ciphertext = &blob[1 + NONCE_LEN..];

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| AppError::Other("aes-gcm decrypt: authentication failed".into()))?;
    String::from_utf8(plaintext)
        .map_err(|e| AppError::Other(format!("decrypted bytes are not UTF-8: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> [u8; 32] {
        // Deterministic key for tests; never used outside this module.
        let mut k = [0u8; 32];
        for (i, b) in k.iter_mut().enumerate() {
            *b = i as u8;
        }
        k
    }

    #[test]
    fn round_trip_recovers_plaintext() {
        let key = test_key();
        let blob = encrypt(&key, "hello world").unwrap();
        assert_eq!(blob[0], VERSION);
        assert_eq!(decrypt(&key, &blob).unwrap(), "hello world");
    }

    #[test]
    fn round_trip_empty_string() {
        let key = test_key();
        let blob = encrypt(&key, "").unwrap();
        assert_eq!(decrypt(&key, &blob).unwrap(), "");
    }

    #[test]
    fn distinct_nonces_per_encryption() {
        let key = test_key();
        let a = encrypt(&key, "same plaintext").unwrap();
        let b = encrypt(&key, "same plaintext").unwrap();
        assert_ne!(a, b, "encryptions should differ thanks to fresh nonces");
    }

    #[test]
    fn tampered_ciphertext_fails_to_decrypt() {
        let key = test_key();
        let mut blob = encrypt(&key, "secret").unwrap();
        // Flip a bit in the ciphertext region (after version + nonce).
        let idx = 1 + NONCE_LEN;
        blob[idx] ^= 0x01;
        assert!(decrypt(&key, &blob).is_err());
    }

    #[test]
    fn tampered_nonce_fails_to_decrypt() {
        let key = test_key();
        let mut blob = encrypt(&key, "secret").unwrap();
        blob[1] ^= 0x01;
        assert!(decrypt(&key, &blob).is_err());
    }

    #[test]
    fn wrong_key_fails_to_decrypt() {
        let key = test_key();
        let blob = encrypt(&key, "secret").unwrap();
        let mut other = test_key();
        other[0] ^= 0x01;
        assert!(decrypt(&other, &blob).is_err());
    }

    #[test]
    fn unknown_version_rejected() {
        let key = test_key();
        let mut blob = encrypt(&key, "secret").unwrap();
        blob[0] = 0x02;
        let err = decrypt(&key, &blob).unwrap_err();
        assert!(
            err.to_string()
                .contains("unsupported encrypted blob version")
        );
    }

    #[test]
    fn short_blob_rejected() {
        let key = test_key();
        assert!(decrypt(&key, &[]).is_err());
        assert!(decrypt(&key, &[VERSION]).is_err());
        assert!(decrypt(&key, &[VERSION, 0, 0, 0]).is_err());
    }
}
