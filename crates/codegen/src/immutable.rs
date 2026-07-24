//! Deployment-time immutable staging layout.
//!
//! Immutable assignments are lowered to ordinary MIR memory stores, while the
//! deployment postlude reads the same words to patch runtime `PUSH<N>`
//! placeholders. This module keeps both sides on one layout.

use crate::mir::ImmutableId;

/// First constructor-memory word reserved for immutable values.
pub(crate) const IMMUTABLE_STAGING_BASE: u64 = 0x2000;
const IMMUTABLE_STAGING_WORD_BYTES: usize = 32;

/// Returns the constructor-memory address assigned to an immutable.
pub(crate) fn immutable_staging_addr(id: ImmutableId) -> u64 {
    IMMUTABLE_STAGING_BASE + (id.index() * IMMUTABLE_STAGING_WORD_BYTES) as u64
}

/// Returns the first constructor-memory address after all immutable words.
pub(crate) fn immutable_staging_end(count: usize) -> u64 {
    IMMUTABLE_STAGING_BASE + (count * IMMUTABLE_STAGING_WORD_BYTES) as u64
}
