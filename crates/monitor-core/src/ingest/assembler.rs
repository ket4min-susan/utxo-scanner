// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use ootle_rs::provider::StealthUtxoFrame;
use tari_ootle_common_types::shard::Shard;

use crate::types::{ShardBatch, SpendEvent, SpendKind};

#[derive(Debug, thiserror::Error)]
pub enum AssemblyError {
    #[error("StartOfShard for shard {new} received while shard {pending} is still open")]
    StartWhileOpen { pending: Shard, new: Shard },
    #[error("update frame received before any StartOfShard")]
    UpdateWithoutStart,
    #[error("EndOfShard received before any StartOfShard")]
    EndWithoutStart,
    #[error("EndOfShard for shard {end} does not match open shard {open}")]
    ShardMismatch { open: Shard, end: Shard },
}

/// Groups the flat frame sequence of a stream pass into per-shard [`ShardBatch`]es.
///
/// Frames arrive as `StartOfShard, update*, EndOfShard` groups; [`push`](Self::push) returns the
/// completed batch on each `EndOfShard`. Any other sequence is a protocol violation.
#[derive(Debug, Default)]
pub struct BatchAssembler {
    current: Option<ShardBatch>,
}

impl BatchAssembler {
    pub fn new() -> Self {
        Self::default()
    }

    /// True when no shard group is partially assembled. A stream that ends while the assembler is
    /// not idle was truncated mid-shard.
    pub fn is_idle(&self) -> bool {
        self.current.is_none()
    }

    pub fn push(&mut self, frame: StealthUtxoFrame) -> Result<Option<ShardBatch>, AssemblyError> {
        match frame {
            StealthUtxoFrame::StartOfShard {
                shard,
                max_state_version,
                num_updates,
            } => {
                if let Some(open) = &self.current {
                    return Err(AssemblyError::StartWhileOpen {
                        pending: open.shard,
                        new: shard,
                    });
                }
                self.current = Some(ShardBatch {
                    shard,
                    max_state_version,
                    num_updates,
                    unspent: Vec::new(),
                    spends: Vec::new(),
                });
                Ok(None)
            },
            StealthUtxoFrame::Unspent { tag, public_nonce } => {
                let batch = self.current.as_mut().ok_or(AssemblyError::UpdateWithoutStart)?;
                batch.unspent.push((tag, public_nonce));
                Ok(None)
            },
            StealthUtxoFrame::Spent { id, version } => {
                let batch = self.current.as_mut().ok_or(AssemblyError::UpdateWithoutStart)?;
                batch.spends.push(SpendEvent {
                    id,
                    kind: SpendKind::Spent,
                    substate_version: version,
                });
                Ok(None)
            },
            StealthUtxoFrame::Burnt { id, version } => {
                let batch = self.current.as_mut().ok_or(AssemblyError::UpdateWithoutStart)?;
                batch.spends.push(SpendEvent {
                    id,
                    kind: SpendKind::Burnt,
                    substate_version: version,
                });
                Ok(None)
            },
            StealthUtxoFrame::EndOfShard {
                shard,
                max_state_version,
            } => {
                let mut batch = self.current.take().ok_or(AssemblyError::EndWithoutStart)?;
                if batch.shard != shard {
                    return Err(AssemblyError::ShardMismatch {
                        open: batch.shard,
                        end: shard,
                    });
                }
                // The end-of-shard watermark is authoritative for the resume cursor.
                if max_state_version > batch.max_state_version {
                    batch.max_state_version = max_state_version;
                }
                Ok(Some(batch))
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use tari_ootle_common_types::StateVersion;
    use tari_template_lib_types::crypto::UtxoTag;

    use super::*;
    use crate::{
        test_utils::{nonce, utxo_id},
        types::SpendKind,
    };

    fn sos(shard: u32, version: u64, num_updates: u32) -> StealthUtxoFrame {
        StealthUtxoFrame::StartOfShard {
            shard: Shard::from(shard),
            max_state_version: StateVersion::new(version),
            num_updates,
        }
    }

    fn eos(shard: u32, version: u64) -> StealthUtxoFrame {
        StealthUtxoFrame::EndOfShard {
            shard: Shard::from(shard),
            max_state_version: StateVersion::new(version),
        }
    }

    #[test]
    fn assembles_a_full_shard_group() {
        let mut assembler = BatchAssembler::new();
        assert!(assembler.push(sos(1, 10, 3)).unwrap().is_none());
        assert!(
            assembler
                .push(StealthUtxoFrame::Unspent {
                    tag: UtxoTag::new(42),
                    public_nonce: nonce(1),
                })
                .unwrap()
                .is_none()
        );
        assert!(
            assembler
                .push(StealthUtxoFrame::Spent {
                    id: utxo_id(2),
                    version: 1,
                })
                .unwrap()
                .is_none()
        );
        assert!(
            assembler
                .push(StealthUtxoFrame::Burnt {
                    id: utxo_id(3),
                    version: 1,
                })
                .unwrap()
                .is_none()
        );
        let batch = assembler.push(eos(1, 12)).unwrap().unwrap();

        assert_eq!(batch.shard, Shard::from(1));
        assert_eq!(batch.max_state_version, StateVersion::new(12));
        assert_eq!(batch.num_updates, 3);
        assert_eq!(batch.unspent, vec![(UtxoTag::new(42), nonce(1))]);
        assert_eq!(batch.spends.len(), 2);
        assert_eq!(batch.spends[0].id, utxo_id(2));
        assert_eq!(batch.spends[0].kind, SpendKind::Spent);
        assert_eq!(batch.spends[1].id, utxo_id(3));
        assert_eq!(batch.spends[1].kind, SpendKind::Burnt);
        assert!(assembler.is_idle());
    }

    #[test]
    fn end_watermark_never_lowers_the_start_watermark() {
        let mut assembler = BatchAssembler::new();
        assembler.push(sos(1, 20, 0)).unwrap();
        let batch = assembler.push(eos(1, 15)).unwrap().unwrap();
        assert_eq!(batch.max_state_version, StateVersion::new(20));
    }

    #[test]
    fn start_while_open_is_an_error() {
        let mut assembler = BatchAssembler::new();
        assembler.push(sos(1, 10, 0)).unwrap();
        let err = assembler.push(sos(2, 10, 0)).unwrap_err();
        assert!(matches!(err, AssemblyError::StartWhileOpen { .. }));
    }

    #[test]
    fn update_without_start_is_an_error() {
        let mut assembler = BatchAssembler::new();
        let err = assembler
            .push(StealthUtxoFrame::Unspent {
                tag: UtxoTag::new(1),
                public_nonce: nonce(1),
            })
            .unwrap_err();
        assert!(matches!(err, AssemblyError::UpdateWithoutStart));
    }

    #[test]
    fn end_without_start_is_an_error() {
        let mut assembler = BatchAssembler::new();
        let err = assembler.push(eos(1, 10)).unwrap_err();
        assert!(matches!(err, AssemblyError::EndWithoutStart));
    }

    #[test]
    fn mismatched_end_shard_is_an_error() {
        let mut assembler = BatchAssembler::new();
        assembler.push(sos(1, 10, 0)).unwrap();
        let err = assembler.push(eos(2, 10)).unwrap_err();
        assert!(matches!(err, AssemblyError::ShardMismatch { .. }));
    }
}
