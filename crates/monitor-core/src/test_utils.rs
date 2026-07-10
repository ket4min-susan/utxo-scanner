// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use tari_template_lib_types::{
    ObjectKey,
    ResourceAddress,
    UtxoId,
    crypto::{RistrettoPublicKeyBytes, UtxoTag},
};

use crate::{store::Store, types::NewUnspentUtxo};

pub fn test_resource_address() -> ResourceAddress {
    ResourceAddress::new(ObjectKey::from_array([7u8; ObjectKey::LENGTH]))
}

pub fn utxo_id(n: u8) -> UtxoId {
    UtxoId::from_array([n; 32])
}

pub fn nonce(n: u8) -> RistrettoPublicKeyBytes {
    RistrettoPublicKeyBytes::from_bytes(&[n; 32]).unwrap()
}

pub fn new_unspent(n: u8) -> NewUnspentUtxo {
    NewUnspentUtxo {
        id: utxo_id(n),
        tag: UtxoTag::new(n as u32),
        fetch_nonce: nonce(n),
        ciphertext: Some(vec![n; 64]),
    }
}

/// An in-memory store seeded with one view key and one active resource.
pub async fn store_with_resource() -> (Store, ResourceAddress) {
    let store = Store::in_memory().await.unwrap();
    let address = test_resource_address();
    let key = store.insert_view_key("test", &[1u8; 32], &nonce(9)).await.unwrap();
    store.insert_resource(&address, key.id, 6, 0).await.unwrap();
    (store, address)
}
