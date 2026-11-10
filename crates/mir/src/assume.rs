//! Facts a pass relied on when it rewrote the program, recorded for run-time checks.
//!
//! With `-Zcheck-analyses`, a pass that rewrites code because an analysis proved a fact records
//! that fact where it acted, as `assume <predicate> <source>, lhs, rhs`: SCCP replacing a value
//! with the constant it proved, check elimination folding a branch, dropping a check or an
//! overflow test, or substituting a value it proved equal, CSE and load PRE forwarding a read
//! from a value an alias analysis proved it equals, or induction variable simplification
//! relying on a loop bound it proved on entry. No pass reads an `assume`, so a wrong fact cannot
//! prove its own record true, and the pass that records one leaves its operands out of its own
//! rewrite, so they keep the values the program computes. Later passes treat the record as an
//! ordinary use. `lower-assumes` turns every record into a run-time test before the backend.

/// The relation an `assume` claims between its operands, compared as unsigned words.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AssumePredicate {
    Eq,
    Ne,
    /// Unsigned `lhs < rhs`.
    Ult,
    /// Unsigned `lhs <= rhs`.
    Ule,
}

impl AssumePredicate {
    /// The name in MIR text.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Eq => "eq",
            Self::Ne => "ne",
            Self::Ult => "ult",
            Self::Ule => "ule",
        }
    }
}

/// The pass whose rewrite relied on an `assume`d fact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FactSource {
    /// A constant SCCP proved and substituted.
    Sccp,
    /// A branch, check, overflow test or equality check elimination decided.
    CheckElim,
    /// A memory, storage or transient read CSE forwarded from an earlier value.
    Cse,
    /// A read partial redundancy elimination forwarded from the values reaching it.
    LoadPre,
    /// A loop bound induction variable simplification proved on entry instead of clamping it.
    IndvarSimplify,
}

impl FactSource {
    /// The name in MIR text: the pass's command-line name, with `_` for `-`.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Sccp => "sccp",
            Self::CheckElim => "check_elim",
            Self::Cse => "cse",
            Self::LoadPre => "load_pre",
            Self::IndvarSimplify => "indvar_simplify",
        }
    }
}
