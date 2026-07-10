// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! The stream → fetch → persist ingestion pipeline (DESIGN §5).

mod assembler;
mod ingestor;
mod source;

pub use assembler::*;
pub use ingestor::*;
pub use source::*;
