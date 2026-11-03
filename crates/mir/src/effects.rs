//! Instruction effects shared by elimination, value numbering, and code motion.
//!
//! Resource footprints belong to ModRef and alias analysis. These small derived properties
//! describe behavior that memory purity alone cannot capture: failure, divergence, external
//! termination, allocation identity, and observations of execution. Speculation additionally
//! requires dependence and profitability proofs; a memory read may expand EVM memory.

use super::{EffectKind, InstKind, OpTraits};

/// Control behavior that must survive even when an operation's result is unused.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ControlEffects {
    pub may_revert: bool,
    pub may_diverge: bool,
    pub may_terminate: bool,
}

impl ControlEffects {
    pub const NONE: Self = Self { may_revert: false, may_diverge: false, may_terminate: false };
    pub const UNKNOWN: Self = Self { may_revert: true, may_diverge: true, may_terminate: true };

    pub const fn any(self) -> bool {
        self.may_revert || self.may_diverge || self.may_terminate
    }

    pub fn merge(&mut self, other: Self) {
        self.may_revert |= other.may_revert;
        self.may_diverge |= other.may_diverge;
        self.may_terminate |= other.may_terminate;
    }
}

/// Derived instruction properties, never stored alongside every instruction.
#[derive(Clone, Copy, Debug)]
pub struct InstructionEffects {
    pub control: ControlEffects,
    pub observable: bool,
    pub expands_memory: bool,
    observes_execution: bool,
    has_identity: bool,
    idempotent_write: bool,
}

impl InstructionEffects {
    /// Whether removing an unused operation could remove required behavior.
    pub(crate) const fn must_execute(self) -> bool {
        self.observable || self.control.any()
    }

    /// Whether equal operands and unchanged read/write dependencies allow sharing a result.
    pub const fn can_common(self) -> bool {
        (!self.observable || self.idempotent_write)
            && !self.control.any()
            && !self.observes_execution
            && !self.has_identity
    }

    /// Whether motion still needs an execution guarantee or a more specific safety proof.
    pub const fn can_speculate(self) -> bool {
        self.can_common() && !self.observable && !self.expands_memory
    }

    /// Whether the result reads the state of execution, such as remaining gas or memory size,
    /// so two evaluations with the same operands can differ.
    pub const fn observes_execution(self) -> bool {
        self.observes_execution
    }
}

impl InstKind {
    /// Whether this operation reads remaining gas, directly or during builtin expansion.
    pub const fn observes_gas(&self) -> bool {
        self.traits().contains(OpTraits::OBSERVES_GAS)
    }

    /// Whether this operation reads account state, a balance or deployed code, which
    /// [`may_change_account_state`](Self::may_change_account_state) operations can change.
    pub const fn reads_account_state(&self) -> bool {
        self.traits().contains(OpTraits::READS_ACCOUNT_STATE)
    }

    /// Whether this operation can move value or deploy or destroy code.
    pub const fn may_change_account_state(&self) -> bool {
        self.traits().contains(OpTraits::CHANGES_ACCOUNT_STATE)
    }

    /// Returns context-free effects; use alias and call summaries for resource footprints.
    pub const fn effects(&self) -> InstructionEffects {
        let traits = self.traits();
        let kind = self.effect_kind();
        InstructionEffects {
            control: ControlEffects {
                may_revert: traits.contains(OpTraits::MAY_REVERT),
                may_diverge: traits.contains(OpTraits::MAY_DIVERGE),
                may_terminate: traits.contains(OpTraits::MAY_TERMINATE),
            },
            observable: matches!(
                kind,
                EffectKind::MemoryWrite
                    | EffectKind::StorageWrite
                    | EffectKind::TransientWrite
                    | EffectKind::ImmutableWrite
                    | EffectKind::ExternalCall
                    | EffectKind::ICall
                    | EffectKind::Create
                    | EffectKind::Log
            ),
            expands_memory: matches!(
                kind,
                EffectKind::MemoryRead
                    | EffectKind::MemoryWrite
                    | EffectKind::ExternalCall
                    | EffectKind::Create
                    | EffectKind::Log
            ) || traits.contains(OpTraits::EXPANDS_MEMORY),
            observes_execution: traits.contains(OpTraits::OBSERVES_GAS)
                || traits.contains(OpTraits::OBSERVES_EXECUTION),
            idempotent_write: traits.contains(OpTraits::IDEMPOTENT_WRITE),
            has_identity: traits.contains(OpTraits::FRESH_OBJECT),
        }
    }
}
