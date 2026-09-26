//! Shares the thread budget between queued or running contracts and their local passes.
//!
//! Contract jobs take priority while they fill the pool. As the ready graph narrows,
//! each remaining contract can use spare workers for independent MIR bodies or EVM blocks.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Clone, Debug, Default)]
pub(crate) struct Scheduling {
    contracts: Arc<AtomicUsize>,
}

impl Scheduling {
    pub(crate) fn add_contracts(&self, count: usize) {
        self.contracts.fetch_add(count, Ordering::Relaxed);
    }

    pub(crate) fn finish_contract(&self) {
        let previous = self.contracts.fetch_sub(1, Ordering::Relaxed);
        debug_assert!(previous > 0);
    }

    pub(crate) fn threads(&self, maximum: usize) -> usize {
        (maximum / self.contracts.load(Ordering::Relaxed).max(1)).max(1)
    }
}
