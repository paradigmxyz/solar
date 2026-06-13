//! Shared helper APIs used by MIR analyses, transformations, and backends.

pub(crate) mod cfg;
pub(crate) mod const_eval;
mod constant_fold;

pub(crate) use cfg::{repair_reachability_phis, split_edge};
pub(crate) use constant_fold::{ConstantFolder, FoldResult};
