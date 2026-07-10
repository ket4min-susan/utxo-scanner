// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Core library for the Ootle stealth-UTXO monitor.
//!
//! Implements the stream → fetch → persist ingestion pipeline (see `DESIGN.md`): a per-resource
//! task drains the indexer's stealth UTXO update stream, resolves each shard's `Unspent` frames to
//! full outputs via a batched fetch, persists them, applies spends, and advances a per-shard
//! resume cursor. Decryption of the persisted ciphertexts is a separate stage (rows are inserted
//! as `Decrypting`).

pub mod decrypt;
pub mod error;
pub mod ingest;
pub mod keys;
pub mod monitor;
pub mod resource;
pub mod store;
#[cfg(test)]
pub(crate) mod test_utils;
pub mod types;

pub use error::*;
