//! Shared helper APIs used by MIR analyses, transformations, and backends.

mod constant_fold;
pub(crate) mod evm_word;

pub(crate) use constant_fold::{ConstantFolder, FoldResult};
