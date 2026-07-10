// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use ootle_rs::provider::{ProviderError, UtxoWatcherError};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("migration error: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("{entity} not found")]
    NotFound { entity: &'static str },
    #[error("corrupt row in {table}: {details}")]
    CorruptRow { table: &'static str, details: String },
}

impl StoreError {
    pub fn is_not_found(&self) -> bool {
        matches!(self, StoreError::NotFound { .. })
    }
}

/// Converts a `NotFound` error into `Ok(None)`, propagating all other errors.
pub trait Optional<T> {
    fn optional(self) -> Result<Option<T>, StoreError>;
}

impl<T> Optional<T> for Result<T, StoreError> {
    fn optional(self) -> Result<Option<T>, StoreError> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(err) if err.is_not_found() => Ok(None),
            Err(err) => Err(err),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum IngestError {
    #[error("UTXO stream error: {0}")]
    Watch(#[from] UtxoWatcherError),
    #[error("UTXO fetch error: {0}")]
    Fetch(#[from] ProviderError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("stream protocol violation: {0}")]
    Protocol(String),
}

#[derive(Debug, thiserror::Error)]
pub enum DecryptError {
    #[error("failed to open value-lookup file {path}: {source}")]
    OpenLookup { path: String, source: std::io::Error },
    #[error("value-lookup file {path} contains no records")]
    EmptyLookup { path: String },
}

#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("invalid secret key hex: {0}")]
    InvalidHex(#[from] hex::FromHexError),
    #[error("invalid secret key: {0}")]
    InvalidKey(String),
}
