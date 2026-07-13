// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::str::FromStr;

use chrono::{DateTime, Utc};
use tari_ootle_common_types::{StateVersion, shard::Shard};
use tari_template_lib_types::{
    ResourceAddress,
    UtxoId,
    crypto::{RistrettoPublicKeyBytes, UtxoTag},
};

/// Whether an output is currently spendable, driven purely by stream frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpendState {
    Unspent,
    Spent,
    Burnt,
}

impl SpendState {
    pub fn as_str(&self) -> &'static str {
        match self {
            SpendState::Unspent => "Unspent",
            SpendState::Spent => "Spent",
            SpendState::Burnt => "Burnt",
        }
    }
}

impl FromStr for SpendState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Unspent" => Ok(SpendState::Unspent),
            "Spent" => Ok(SpendState::Spent),
            "Burnt" => Ok(SpendState::Burnt),
            other => Err(format!("invalid spend state: {other}")),
        }
    }
}

/// Progress of value decryption for an output, driven by the decrypt worker (and by ingestion for
/// `NoCiphertext`). Orthogonal to [`SpendState`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecryptState {
    /// Ciphertext persisted, awaiting a decrypt worker.
    Decrypting,
    Decrypted,
    /// Hard failure: ciphertext malformed or absent on a viewable resource. Never retried.
    Failed,
    /// Value lies beyond the lookup file's coverage. Retryable with a wider file.
    FailedOutOfRange,
    /// The output was already spent when first observed — the indexer had pruned its ciphertext,
    /// so there is nothing to decrypt. Terminal.
    NoCiphertext,
}

impl DecryptState {
    pub fn as_str(&self) -> &'static str {
        match self {
            DecryptState::Decrypting => "Decrypting",
            DecryptState::Decrypted => "Decrypted",
            DecryptState::Failed => "Failed",
            DecryptState::FailedOutOfRange => "FailedOutOfRange",
            DecryptState::NoCiphertext => "NoCiphertext",
        }
    }
}

impl FromStr for DecryptState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Decrypting" => Ok(DecryptState::Decrypting),
            "Decrypted" => Ok(DecryptState::Decrypted),
            "Failed" => Ok(DecryptState::Failed),
            "FailedOutOfRange" => Ok(DecryptState::FailedOutOfRange),
            "NoCiphertext" => Ok(DecryptState::NoCiphertext),
            other => Err(format!("invalid decrypt state: {other}")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ViewKeyRow {
    pub id: i64,
    pub label: String,
    pub secret: [u8; 32],
    pub public: RistrettoPublicKeyBytes,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ResourceRow {
    pub resource_address: ResourceAddress,
    pub view_key_id: i64,
    pub divisibility: u8,
    pub from_epoch: u64,
    pub active: bool,
    pub added_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct UtxoRow {
    /// sqlite rowid — a monotonic insertion counter, used as the discovery-order sort key and
    /// keyset-pagination cursor.
    pub rowid: i64,
    pub resource_address: ResourceAddress,
    pub utxo_id: UtxoId,
    pub tag: Option<UtxoTag>,
    pub fetch_nonce: Option<RistrettoPublicKeyBytes>,
    /// The ElGamal viewable-balance ciphertext: `encrypted(32) || public_nonce(32)`.
    pub ciphertext: Option<Vec<u8>>,
    pub value: Option<u64>,
    pub spend_state: SpendState,
    pub decrypt_state: DecryptState,
    pub fail_lookup_max: Option<u64>,
    pub created_epoch: Option<u64>,
    pub created_state_version: Option<u64>,
    pub spent_epoch: Option<u64>,
    pub spent_state_version: Option<u64>,
    pub first_seen_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A fully-resolved unspent output ready for persistence, as produced by
/// [`fetch_unspent`](crate::ingest::UtxoSource::fetch_unspent).
#[derive(Debug, Clone)]
pub struct NewUnspentUtxo {
    pub id: UtxoId,
    pub tag: UtxoTag,
    pub fetch_nonce: RistrettoPublicKeyBytes,
    /// `encrypted(32) || public_nonce(32)`; `None` when the output carries no viewable balance
    /// (an invariant violation for a viewable resource — persisted as [`DecryptState::Failed`]).
    pub ciphertext: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpendKind {
    Spent,
    Burnt,
}

impl SpendKind {
    pub fn as_spend_state(&self) -> SpendState {
        match self {
            SpendKind::Spent => SpendState::Spent,
            SpendKind::Burnt => SpendState::Burnt,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SpendEvent {
    pub id: UtxoId,
    pub kind: SpendKind,
    pub substate_version: u32,
}

/// One shard's worth of stream updates, assembled from a `StartOfShard ..= EndOfShard` frame
/// group by [`BatchAssembler`](crate::ingest::BatchAssembler).
#[derive(Debug, Clone)]
pub struct ShardBatch {
    pub shard: Shard,
    /// The shard's high-watermark state version — the resume cursor after this batch is applied.
    pub max_state_version: StateVersion,
    /// Number of updates the indexer reported for this pass. Equal to the request's
    /// `per_shard_limit` when the shard has more updates to drain.
    pub num_updates: u32,
    pub unspent: Vec<(UtxoTag, RistrettoPublicKeyBytes)>,
    pub spends: Vec<SpendEvent>,
}

impl ShardBatch {
    pub fn is_empty(&self) -> bool {
        self.unspent.is_empty() && self.spends.is_empty()
    }
}
