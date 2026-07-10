// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! ElGamal view-key material. The monitor holds only view *secrets* — never spend keys — and
//! uses each as a resource-wide auditor key.

use tari_crypto::{
    keys::{PublicKey, SecretKey},
    ristretto::{RistrettoPublicKey, RistrettoSecretKey},
    tari_utilities::ByteArray,
};
use tari_template_lib_types::crypto::RistrettoPublicKeyBytes;

use crate::error::KeyError;

#[derive(Debug, Clone)]
pub struct ViewKeyPair {
    pub secret: RistrettoSecretKey,
    pub public: RistrettoPublicKeyBytes,
}

impl ViewKeyPair {
    pub fn from_secret(secret: RistrettoSecretKey) -> Self {
        let public = RistrettoPublicKey::from_secret_key(&secret);
        let public = RistrettoPublicKeyBytes::from_bytes(public.as_bytes())
            .expect("compressed Ristretto point is always 32 bytes");
        Self { secret, public }
    }

    pub fn generate() -> Self {
        Self::from_secret(RistrettoSecretKey::random(&mut rand::rng()))
    }

    pub fn from_secret_hex(hex_str: &str) -> Result<Self, KeyError> {
        let bytes = hex::decode(hex_str.trim())?;
        Self::from_secret_bytes(&bytes)
    }

    pub fn from_secret_bytes(bytes: &[u8]) -> Result<Self, KeyError> {
        let secret =
            RistrettoSecretKey::from_canonical_bytes(bytes).map_err(|e| KeyError::InvalidKey(e.to_string()))?;
        Ok(Self::from_secret(secret))
    }

    pub fn secret_bytes(&self) -> [u8; 32] {
        self.secret
            .as_bytes()
            .try_into()
            .expect("Ristretto scalar is always 32 bytes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_hex_roundtrip_preserves_the_public_key() {
        let pair = ViewKeyPair::generate();
        let restored = ViewKeyPair::from_secret_hex(&hex::encode(pair.secret_bytes())).unwrap();
        assert_eq!(restored.public, pair.public);
    }

    #[test]
    fn rejects_invalid_secret_hex() {
        assert!(ViewKeyPair::from_secret_hex("not hex").is_err());
        assert!(ViewKeyPair::from_secret_hex("abcd").is_err());
    }
}
