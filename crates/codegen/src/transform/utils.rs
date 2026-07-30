//! Shared utilities for MIR transforms.

use crate::mir::{BlockId, Function, InstKind, Terminator};
use solar_sema::hir::StateMutability;

/// Whether an external entry must reject nonzero callvalue.
pub(super) fn rejects_callvalue(func: &Function) -> bool {
    matches!(
        func.attributes.state_mutability,
        StateMutability::NonPayable | StateMutability::View | StateMutability::Pure
    )
}

/// Redirects successor predecessor metadata after splitting `from` into a
/// continuation block `to`.
pub(super) fn redirect_successor_predecessors(func: &mut Function, from: BlockId, to: BlockId) {
    let successors =
        func.blocks[to].terminator.as_ref().map(Terminator::successors).unwrap_or_default();
    for successor in successors {
        for predecessor in &mut func.blocks[successor].predecessors {
            if *predecessor == from {
                *predecessor = to;
            }
        }
        let phi_insts: Vec<_> = func.blocks[successor]
            .instructions
            .iter()
            .copied()
            .take_while(|inst| matches!(func.inst(*inst).kind, InstKind::Phi(_)))
            .collect();
        for phi in phi_insts {
            let InstKind::Phi(incoming) = &mut func.inst_mut(phi).kind else { unreachable!() };
            for (predecessor, _) in incoming {
                if *predecessor == from {
                    *predecessor = to;
                }
            }
        }
    }
}

/// Incremental form of the shared dispatch callvalue-hoisting predicate:
/// every external entry (selector-bearing, receive, or fallback) rejects value.
///
/// `LowerAbi` and `LowerDispatch` both use this while performing their
/// existing module scans, so they must observe every function and agree.
pub(super) struct DispatchCallvalue {
    any: bool,
    all_reject: bool,
}

impl Default for DispatchCallvalue {
    fn default() -> Self {
        Self { any: false, all_reject: true }
    }
}

impl DispatchCallvalue {
    pub(super) fn observe(&mut self, func: &Function) {
        let external =
            func.selector.is_some() || func.attributes.is_receive || func.attributes.is_fallback;
        if !external || func.attributes.is_constructor {
            return;
        }
        self.any = true;
        self.all_reject &= rejects_callvalue(func);
    }

    pub(super) const fn hoists(&self) -> bool {
        self.any && self.all_reject
    }
}
