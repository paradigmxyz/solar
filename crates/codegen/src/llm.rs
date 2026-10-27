//! Hooks for LLM-backed rewriting of MIR functions.
//!
//! The `llm-optimize` MIR pass, enabled with `-Zllm-optimize`, offers eligible functions of lowered
//! MIR to a rewriter and asks it for cheaper equivalents over several rounds. A rewriter is
//! anything implementing [`LlmRewriter`]: the command line binds one that asks a model to its
//! session when it is built with its `llm` feature, and an embedder binds its own to the session it
//! compiles in with [`bind_rewriter`]. A rewriter serves only the session it is bound to, and the
//! sessions forked from it, so no compilation reaches a rewriter that another one chose.
//!
//! Rewriters are never trusted. The pass parses each candidate with a stricter grammar than MIR
//! files use, checks its signature, operations, and callees, validates it, runs it against the
//! original on generated inputs, and prices it with the target cost model. It keeps a candidate
//! only when every check passes and the candidate is cheaper than the best so far, and each
//! [`Verdict`] tells the rewriter which check decided, so the next proposal can improve on it.

use solar_config::{EvmVersion, OptimizationMode};
use solar_interface::{Session, SessionBinding, SessionBindings};
use std::{fmt, sync::Arc};

/// The rewriters bound to sessions.
static REWRITERS: SessionBindings<dyn LlmRewriter> = SessionBindings::new();

/// Proposes rewrites of MIR functions.
pub trait LlmRewriter: Send + Sync {
    /// Opens a rewriting session for one function.
    fn session(&self, request: &RewriteRequest) -> Result<Box<dyn LlmSession>, LlmError>;

    /// Hears that the function `request` describes keeps the rewrite the cache recorded for it,
    /// at `cost`, so no session asks about it.
    fn cached(&self, request: &RewriteRequest, cost: CostReport) {
        let _ = (request, cost);
    }
}

/// One conversation about rewriting one function.
pub trait LlmSession: Send {
    /// Proposes the next candidate. `verdict` judges the previous proposal, and is `None` before
    /// the first one.
    fn propose(&mut self, verdict: Option<&Verdict>) -> Result<Proposal, LlmError>;

    /// Hears how the session ended: `verdict` judges the last proposal when no later call to
    /// [`propose`](Self::propose) heard it, and `kept` is the cost of the best candidate, when
    /// one beat the original.
    fn finish(&mut self, verdict: Option<&Verdict>, kept: Option<CostReport>) {
        let _ = (verdict, kept);
    }
}

/// What a rewriter is asked to improve.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct RewriteRequest {
    /// The name of the module holding the function, usually its contract's.
    pub module_name: String,
    /// The function's name, as its text spells it after `fn @`.
    pub function_name: String,
    /// The function as candidate text: lowered MIR without metadata.
    pub function_text: String,
    /// The functions it calls, in the same form. A candidate may call these and no others.
    pub context: String,
    /// The objective costs rank by.
    pub objective: OptimizationMode,
    /// Expected executions per deployment, which weighs gas against deployed bytes.
    pub optimizer_runs: u64,
    /// The target EVM version, which decides the available operations and their prices.
    pub evm_version: EvmVersion,
    /// The cost of the function as it is.
    pub baseline: CostReport,
    /// The operations a candidate may use, separated by spaces.
    pub vocabulary: String,
}

/// The modeled cost of a function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CostReport {
    /// Average gas of one call over the generated inputs it returns on.
    pub gas: u64,
    /// Estimated bytes of its code.
    pub bytes: u64,
}

impl fmt::Display for CostReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} gas, {} bytes", self.gas, self.bytes)
    }
}

/// A rewriter's answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Proposal {
    /// A candidate replacement, as candidate text.
    Candidate(String),
    /// The rewriter has nothing cheaper to offer.
    Done,
}

/// The pass's judgment of a proposal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// The candidate passed every check and is now the best.
    Accepted {
        /// Its cost.
        cost: CostReport,
    },
    /// The candidate failed a check.
    Rejected {
        /// The check it failed.
        stage: Stage,
        /// Why it failed.
        reason: String,
        /// The inputs it failed on, for a behavior difference.
        counterexample: Option<String>,
    },
}

/// The checks a candidate goes through, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Parsing the candidate text.
    Parse,
    /// Its signature, operations, callees, and stack pressure.
    Constraints,
    /// The MIR validator.
    Validation,
    /// Running it against the original.
    Equivalence,
    /// Comparing its cost with the best so far.
    Cost,
}

impl fmt::Display for Stage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Parse => "parse",
            Self::Constraints => "constraints",
            Self::Validation => "validation",
            Self::Equivalence => "equivalence",
            Self::Cost => "cost",
        })
    }
}

/// A rewriter failure. It never fails a compilation: the function keeps the best candidate so
/// far, or stays as it is.
#[derive(Clone, Debug)]
pub struct LlmError(String);

impl LlmError {
    /// Creates an error with a message.
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for LlmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for LlmError {}

/// Binds the rewriter that `-Zllm-optimize=live` asks in `sess`, and in sessions forked from it,
/// until the returned binding drops. No other session asks it, so a compilation's functions only
/// reach the rewriter, and the model and endpoint behind it, that its own session chose.
pub fn bind_rewriter(
    sess: &Session,
    rewriter: Arc<dyn LlmRewriter>,
) -> SessionBinding<dyn LlmRewriter> {
    REWRITERS.bind(sess, rewriter)
}

/// Returns the rewriter `-Zllm-optimize=live` asks in `sess`: the one bound to it last, if any.
pub fn rewriter(sess: &Session) -> Option<Arc<dyn LlmRewriter>> {
    REWRITERS.get(sess)
}
