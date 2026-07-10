// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use futures::Stream;
use ootle_rs::provider::{IndexerProvider, StealthUtxoFrame, StealthUtxoWatchRequest, UtxoWatcherError};
use tari_ootle_common_types::engine_types::Utxo;
use tari_template_lib_types::{
    ResourceAddress,
    UtxoId,
    crypto::{RistrettoPublicKeyBytes, UtxoTag},
};
use tracing::warn;

use crate::{error::IngestError, types::NewUnspentUtxo};

/// The ingestion loop's view of the indexer: a frame stream plus the batched fetch that resolves
/// `Unspent` fetch keys to full outputs. Abstracted so the loop is testable without an indexer.
pub trait UtxoSource: Send + Sync {
    fn watch(
        &self,
        request: StealthUtxoWatchRequest,
    ) -> impl Stream<Item = Result<StealthUtxoFrame, UtxoWatcherError>> + Send;

    fn fetch_unspent(
        &self,
        resource_address: ResourceAddress,
        pairs: &[(UtxoTag, RistrettoPublicKeyBytes)],
    ) -> impl Future<Output = Result<Vec<NewUnspentUtxo>, IngestError>> + Send;
}

/// [`UtxoSource`] backed by an [`IndexerProvider`].
#[derive(Debug, Clone)]
pub struct IndexerUtxoSource<W> {
    provider: IndexerProvider<W>,
}

impl<W> IndexerUtxoSource<W> {
    pub fn new(provider: IndexerProvider<W>) -> Self {
        Self { provider }
    }
}

impl<W: Send + Sync> UtxoSource for IndexerUtxoSource<W> {
    fn watch(
        &self,
        request: StealthUtxoWatchRequest,
    ) -> impl Stream<Item = Result<StealthUtxoFrame, UtxoWatcherError>> + Send {
        self.provider.watch_stealth_utxos(request).into_stream()
    }

    async fn fetch_unspent(
        &self,
        resource_address: ResourceAddress,
        pairs: &[(UtxoTag, RistrettoPublicKeyBytes)],
    ) -> Result<Vec<NewUnspentUtxo>, IngestError> {
        let fetched = self.provider.fetch_unspent_utxos(resource_address, pairs).await?;
        Ok(fetched.into_iter().filter_map(convert_fetched_utxo).collect())
    }
}

/// Converts a fetched `(UtxoId, Utxo)` substate into the persistence shape, extracting the tag,
/// fetch nonce, and the 64-byte viewable-balance ciphertext (`encrypted || public_nonce`).
pub fn convert_fetched_utxo((id, utxo): (UtxoId, Utxo)) -> Option<NewUnspentUtxo> {
    let Some(output) = utxo.into_output() else {
        // /utxos/fetch returns unspent outputs only; a burnt (output-less) substate here is an
        // indexer bug.
        warn!(utxo_id = %id, "fetched UTXO has no output body; skipping");
        return None;
    };
    let ciphertext = output.output.viewable_balance.as_ref().map(|vb| {
        let mut bytes = Vec::with_capacity(64);
        bytes.extend_from_slice(vb.encrypted.as_bytes());
        bytes.extend_from_slice(vb.public_nonce.as_bytes());
        bytes
    });
    Some(NewUnspentUtxo {
        id,
        tag: output.tag,
        fetch_nonce: output.output.public_nonce,
        ciphertext,
    })
}
