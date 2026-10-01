//! On-demand, summary-based interprocedural analysis.
//!
//! An [`InterproceduralAnalysis`] computes one summary per function and calling
//! [`Context`]. Summaries are requested lazily through [`SummaryEngine::summary`], the
//! analogue of Infer's `analyze_dependency`: the first query for a callee runs its
//! intraprocedural analysis, and later queries reuse the cached result. Only functions
//! transitively reachable from a query are ever analyzed, so asking about one entry point
//! does not analyze the whole module.
//!
//! A context pairs a call string of at most `k` call sites with an analysis-defined entry
//! abstraction, such as the storage locations of pointer arguments (object sensitivity) or
//! argument intervals. With `k = 0` every call uses the empty call string and the analysis'
//! most general entry, so each function has exactly one summary that callers instantiate:
//! the functional approach of Sharir and Pnueli. Analyses whose summaries are parametric in
//! formal parameters, like storage paths relative to `arg0`, lose no precision at `k = 0`.
//! Non-distributive domains gain precision from `k > 0`, bounded by the number of distinct
//! contexts per function ([`ContextPolicy::max_contexts`]); further contexts collapse to the
//! most general entry. Calls that re-enter an active function also use the most general
//! entry, so recursion never generates new contexts.
//!
//! Recursive queries follow Tarjan-style fixpoint iteration. Querying a function that is
//! still being summarized returns its current approximation, which starts at the analysis'
//! bottom summary, and marks every frame above it as dependent on that frame. A dependent
//! frame's result stays provisional; the head of the cycle re-summarizes until its summary
//! stops growing, widening after a few rounds, and only then are the results computed during
//! its final round made permanent. Missing bodies receive the analysis' unknown summary.

use super::lattice::JoinSemiLattice;
use crate::mir::{BlockId, FunctionId, InstId, Module};
use smallvec::SmallVec;
use solar_data_structures::map::FxHashMap;
use std::{fmt, hash::Hash};

/// Number of fixpoint rounds for a recursive cycle before summaries are widened.
const SUMMARY_WIDEN_DELAY: u32 = 3;

/// A call instruction in a specific function.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct CallSite {
    /// The calling function.
    pub(crate) caller: FunctionId,
    /// The call instruction, or `None` for a tail call terminator of `block`.
    pub(crate) inst: Option<InstId>,
    /// The block that contains the call.
    pub(crate) block: BlockId,
}

impl fmt::Display for CallSite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.inst {
            Some(inst) => write!(f, "f{}:i{}", self.caller.index(), inst.index()),
            None => write!(f, "f{}:bb{}", self.caller.index(), self.block.index()),
        }
    }
}

/// A calling context: the most recent call sites plus an entry abstraction.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Context<E> {
    /// Up to `k` most recent call sites, outermost first.
    pub(crate) call_string: SmallVec<[CallSite; 2]>,
    /// Analysis-defined abstraction of the state at function entry.
    pub(crate) entry: E,
}

impl<E: fmt::Display> fmt::Display for Context<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[")?;
        for (i, site) in self.call_string.iter().enumerate() {
            if i != 0 {
                write!(f, " ")?;
            }
            write!(f, "{site}")?;
        }
        write!(f, "] {}", self.entry)
    }
}

/// Bounds the calling contexts an analysis may distinguish.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ContextPolicy {
    /// Call-string length; zero selects pure summaries.
    pub(crate) k: usize,
    /// Maximum distinct contexts per function before entries collapse to the most general one.
    pub(crate) max_contexts: usize,
}

impl ContextPolicy {
    /// One summary per function.
    pub(crate) const INSENSITIVE: Self = Self { k: 0, max_contexts: 1 };

    /// Call strings of length `k`, with at most 16 contexts per function.
    pub(crate) const fn call_strings(k: usize) -> Self {
        if k == 0 { Self::INSENSITIVE } else { Self { k, max_contexts: 16 } }
    }
}

/// A summary-based interprocedural dataflow problem.
pub(crate) trait InterproceduralAnalysis: Sized {
    /// Entry abstraction distinguishing contexts.
    type Entry: Clone + Eq + Hash + fmt::Debug;
    /// Per-context function summary.
    type Summary: JoinSemiLattice + PartialEq + fmt::Debug;

    /// Returns the most general entry of `func`, used when contexts are not distinguished.
    fn general_entry(&self, module: &Module, func: FunctionId) -> Self::Entry;

    /// Returns the initial approximation of a summary under computation.
    fn bottom_summary(&self, module: &Module, func: FunctionId) -> Self::Summary;

    /// Returns the conservative summary of a function without a body.
    fn unknown_summary(&self, module: &Module, func: FunctionId) -> Self::Summary;

    /// Computes the summary of `func` in `context`, querying callees through `engine`.
    fn summarize(
        engine: &mut SummaryEngine<'_, Self>,
        func: FunctionId,
        context: &Context<Self::Entry>,
    ) -> Self::Summary;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EntryState {
    /// Summary computation is active at this stack depth.
    Active(usize),
    /// Computed from a value of an enclosing active frame during head round `epoch`.
    Provisional(u64),
    /// Computed from final callee summaries.
    Final,
}

#[derive(Debug)]
struct Entry<S> {
    state: EntryState,
    summary: S,
}

struct Frame<K> {
    key: K,
    /// Lowest stack depth this frame's result depends on.
    lowlink: usize,
    /// Whether a recursive query observed this frame's approximation.
    queried_recursively: bool,
}

/// Counters describing the work an engine performed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct EngineStats {
    /// Number of `summarize` invocations, including fixpoint rounds.
    pub(crate) summarizations: usize,
    /// Number of queries answered from the cache.
    pub(crate) cache_hits: usize,
}

type Key<E> = (FunctionId, Context<E>);

/// A final summary with its function and context.
pub(crate) type FinalSummary<'a, A> = (
    FunctionId,
    &'a Context<<A as InterproceduralAnalysis>::Entry>,
    &'a <A as InterproceduralAnalysis>::Summary,
);

/// Lazily computes and caches the summaries of one interprocedural analysis.
pub(crate) struct SummaryEngine<'m, A: InterproceduralAnalysis> {
    /// The analyzed module.
    pub(crate) module: &'m Module,
    /// Analysis configuration and shared tables.
    pub(crate) analysis: A,
    /// Context sensitivity bounds.
    pub(crate) policy: ContextPolicy,
    cache: FxHashMap<Key<A::Entry>, Entry<A::Summary>>,
    contexts: FxHashMap<FunctionId, usize>,
    stack: Vec<Frame<Key<A::Entry>>>,
    /// Provisional results with the stack depth they depend on and their round.
    provisional: Vec<(Key<A::Entry>, usize, u64)>,
    epoch: u64,
    /// Work counters.
    pub(crate) stats: EngineStats,
}

impl<'m, A: InterproceduralAnalysis> SummaryEngine<'m, A> {
    /// Creates an engine without any computed summaries.
    pub(crate) fn new(module: &'m Module, analysis: A, policy: ContextPolicy) -> Self {
        Self {
            module,
            analysis,
            policy,
            cache: FxHashMap::default(),
            contexts: FxHashMap::default(),
            stack: Vec::new(),
            provisional: Vec::new(),
            epoch: 0,
            stats: EngineStats::default(),
        }
    }

    /// Returns the most general context of `func`.
    pub(crate) fn general_context(&self, func: FunctionId) -> Context<A::Entry> {
        Context {
            call_string: SmallVec::new(),
            entry: self.analysis.general_entry(self.module, func),
        }
    }

    /// Returns the context for calling `callee` from `site` in `caller_context` with the
    /// entry abstraction `entry`, applying the policy's bounds.
    pub(crate) fn callee_context(
        &mut self,
        caller_context: &Context<A::Entry>,
        site: CallSite,
        callee: FunctionId,
        entry: A::Entry,
    ) -> Context<A::Entry> {
        if self.policy.k == 0 || self.is_active(callee) {
            return self.general_context(callee);
        }
        let mut call_string = caller_context.call_string.clone();
        call_string.push(site);
        if call_string.len() > self.policy.k {
            call_string.remove(0);
        }
        let context = Context { call_string, entry };
        let key = (callee, context);
        if self.cache.contains_key(&key) {
            return key.1;
        }
        let count = self.contexts.entry(callee).or_default();
        if *count >= self.policy.max_contexts {
            return self.general_context(callee);
        }
        *count += 1;
        key.1
    }

    /// Returns whether a summary of `func` is currently being computed.
    pub(crate) fn is_active(&self, func: FunctionId) -> bool {
        self.stack.iter().any(|frame| frame.key.0 == func)
    }

    /// Returns the summary of `func` in `context`, computing it on demand.
    ///
    /// Inside a recursive cycle the result may be an approximation that later rounds grow.
    pub(crate) fn summary(&mut self, func: FunctionId, context: &Context<A::Entry>) -> A::Summary {
        let key = (func, context.clone());
        if let Some(entry) = self.cache.get(&key) {
            match entry.state {
                EntryState::Final => {
                    self.stats.cache_hits += 1;
                    return entry.summary.clone();
                }
                EntryState::Active(depth) => {
                    let summary = entry.summary.clone();
                    self.stack[depth].queried_recursively = true;
                    for frame in &mut self.stack[depth + 1..] {
                        frame.lowlink = frame.lowlink.min(depth);
                    }
                    return summary;
                }
                EntryState::Provisional(epoch) if epoch == self.epoch => {
                    self.stats.cache_hits += 1;
                    let summary = entry.summary.clone();
                    return summary;
                }
                EntryState::Provisional(_) => {}
            }
        }
        if self.module.function(func).blocks.is_empty()
            || self.module.function(func).blocks[BlockId::ENTRY].terminator.is_none()
        {
            let summary = self.analysis.unknown_summary(self.module, func);
            self.cache.insert(key, Entry { state: EntryState::Final, summary: summary.clone() });
            return summary;
        }

        let depth = self.stack.len();
        let initial = match self.cache.remove(&key) {
            Some(entry) => entry.summary,
            None => self.analysis.bottom_summary(self.module, func),
        };
        self.cache
            .insert(key.clone(), Entry { state: EntryState::Active(depth), summary: initial });
        self.stack.push(Frame { key: key.clone(), lowlink: depth, queried_recursively: false });

        let mut rounds = 0;
        loop {
            self.stats.summarizations += 1;
            let computed = A::summarize(self, func, context);
            let entry = self.cache.get_mut(&key).expect("active summary entry");
            let changed = if rounds >= SUMMARY_WIDEN_DELAY {
                entry.summary.widen(&computed)
            } else {
                entry.summary.join(&computed)
            };
            let frame = self.stack.last().expect("active summary frame");
            if frame.lowlink < depth {
                // Part of a cycle headed further down the stack: the head decides when
                // this value is final.
                let lowlink = frame.lowlink;
                self.stack.pop();
                if let Some(parent) = self.stack.last_mut() {
                    parent.lowlink = parent.lowlink.min(lowlink);
                }
                let entry = self.cache.get_mut(&key).expect("active summary entry");
                entry.state = EntryState::Provisional(self.epoch);
                let summary = entry.summary.clone();
                self.provisional.push((key, lowlink, self.epoch));
                return summary;
            }
            if changed && frame.queried_recursively {
                // Another round: results derived from the old approximation are stale.
                rounds += 1;
                self.epoch += 1;
                self.stack.last_mut().expect("active summary frame").queried_recursively = false;
                continue;
            }
            // This head's value is stable, so everything computed during the last round is.
            self.stack.pop();
            // Results that depended on this frame were final if computed in its last round;
            // older ones stay provisional and are recomputed when queried again.
            let epoch = self.epoch;
            let mut index = 0;
            while index < self.provisional.len() {
                if self.provisional[index].1 < depth {
                    index += 1;
                    continue;
                }
                let (dependent, _, computed) = self.provisional.swap_remove(index);
                if computed == epoch
                    && let Some(entry) = self.cache.get_mut(&dependent)
                    && entry.state == EntryState::Provisional(epoch)
                {
                    entry.state = EntryState::Final;
                }
            }
            let entry = self.cache.get_mut(&key).expect("active summary entry");
            entry.state = EntryState::Final;
            return entry.summary.clone();
        }
    }

    /// Returns every final summary together with its context, ordered by function.
    pub(crate) fn final_summaries(&self) -> Vec<FinalSummary<'_, A>> {
        let mut summaries = self
            .cache
            .iter()
            .filter(|(_, entry)| entry.state == EntryState::Final)
            .map(|((func, context), entry)| (*func, context, &entry.summary))
            .collect::<Vec<_>>();
        summaries.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.call_string.cmp(&b.1.call_string)));
        summaries
    }
}
