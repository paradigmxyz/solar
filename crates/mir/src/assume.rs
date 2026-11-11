//! Facts a pass relied on when it rewrote the program, recorded for run-time checks.
//!
//! With `-Zcheck-analyses`, a pass that rewrites code because an analysis proved a fact records
//! that fact where it acted, as `assume <predicate> <source>, lhs, rhs`: SCCP replacing a value
//! with the constant it proved, check elimination folding a branch, dropping a check or an
//! overflow test, or substituting a value it proved equal, CSE and load PRE forwarding a read
//! from a value an alias analysis proved it equals, induction variable simplification
//! relying on a loop bound it proved on entry, or a rule of the e-graph or late constant folding
//! relying on a width a value's definition bounds. Passes that rely on memory, storage or control
//! facts record them as a re-read at the point they relied on them: loop-invariant code motion
//! records each read it hoisted equal to the same read where the loop can exit, storage promotion
//! records the promoted slot unchanged at each exit and the slots it found disjoint different,
//! allocation lowering records the memory a zero fill it dropped as zero and the free memory
//! pointer it did not load as the heap floor, `noreturn-calls` records that execution does not
//! continue past a call it found cannot return, as a fact that is false wherever it runs, and
//! `readonly-eval` keeps a call it evaluated and records its result equal to the constant. No pass
//! reads an `assume`, so a wrong fact cannot prove its own record true, and the pass that records
//! one leaves its operands out of its own rewrite, so they keep the values the program computes.
//! Later passes treat the record as an
//! ordinary use. `lower-assumes` turns every record into a run-time test before the backend.
//!
//! Source specifications travel the same way: with recording on, a function's
//! `@custom:requires` clauses become `spec_requires` records at its entry and its `@custom:ensures`
//! clauses `spec_ensures` records at each return, so passes keep them next to the code they are
//! about and `-Zprove-analyses` proves the source's claims on the optimized program.

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
    /// A source `@custom:requires` clause, recorded at function entry. A premise the caller
    /// promises, not a fact anything proved: no pass relies on it, `lower-assumes` neither tests
    /// nor proves it, and no query assumes it.
    SpecRequires,
    /// A source `@custom:ensures` clause at a return, as `ne cond, 0` where `cond` is the clause
    /// implied by the function's `@custom:requires` clauses. An obligation of the source, tested
    /// and proved like the facts passes record.
    SpecEnsures,
    /// A width an e-graph rule relied on, which the value's definition bounds.
    Egraph,
    /// A width a rule late constant folding reuses from the e-graph relied on.
    ConstFold,
    /// A read loop-invariant code motion hoisted, equal to the same read where the loop exits.
    Licm,
    /// A slot storage promotion kept in memory across a loop, unchanged in storage where the loop
    /// exits, or apart from another slot the loop accesses.
    StoragePromotion,
    /// Memory allocation lowering found still zero, or a free memory pointer it found unwritten.
    LowerAlloc,
    /// A point after a call or check `noreturn-calls` found cannot return, which execution never
    /// reaches.
    NoreturnCalls,
    /// The result of a call `readonly-eval` evaluated over known allocation words, equal to the
    /// constant it folded the call to.
    ReadonlyEval,
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
            Self::SpecRequires => "spec_requires",
            Self::SpecEnsures => "spec_ensures",
            Self::Egraph => "egraph",
            Self::ConstFold => "const_fold",
            Self::Licm => "licm",
            Self::StoragePromotion => "storage_promotion",
            Self::LowerAlloc => "lower_alloc",
            Self::NoreturnCalls => "noreturn_calls",
            Self::ReadonlyEval => "readonly_eval",
        }
    }
}
