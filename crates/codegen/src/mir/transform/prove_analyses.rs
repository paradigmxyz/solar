//! SMT queries that prove, for all inputs, the facts passes record for `-Zcheck-analyses`.
//!
//! With `-Zprove-analyses=DIR`, which also turns recording on, [`lower-assumes`] hands this
//! module every `assume` record of a function before turning them into run-time checks, and each
//! record becomes an SMT-LIB query in `DIR` whose satisfiability means the fact can fail. Z3
//! answering `unsat` proves the fact for every execution, so its run-time check can never fire;
//! `sat` gives a counterexample, which is either an analysis bug or a case the encoding below is
//! too coarse to rule out, and needs triage. With `-` for `DIR`, the queries go to stdout
//! instead, each after a `; query: NAME` line. `tools/prove-analyses` runs a corpus through Z3.
//!
//! A query encodes the claim at the end of the record's block `B` as straight-line SSA over
//! 256-bit words, with the records the run-time checks would have tested earlier as
//! assumptions. That is the induction behind Regehr's checks: in the first execution that
//! violates any check, every check that ran before it passed. So a query assumes
//!
//! - the definition of every value in the blocks that dominate `B` and in `B`, by the operation's
//!   declared [`Semantics`]: pure opcodes, casts, `select`, and `ne` exactly, with EVM word
//!   semantics. Transaction inputs (calldata, the caller, the block) are functions of their
//!   operands, so two reads agree. Memory, storage and transient reads, calls and hashes are
//!   unconstrained but for the width their opcode produces. Every one of these values is the
//!   instance computed after the last execution of the dominating block that checked it, so the
//!   records below are about it;
//! - the branch condition of every dominating edge into a block with a single predecessor;
//! - at each join, that execution came through one of its predecessors: the context up to that
//!   predecessor's end below the join's dominators, under fresh names and with the facts checked
//!   on it, the edge's condition, and each phi equal to its incoming value on that edge. For a
//!   loop's back edge this is the earlier pass, whose facts are the induction hypothesis. Joins
//!   inside such a context are encoded by their phis alone, each one of its incoming values;
//! - every record at the end of a dominating block, and the records before this one in `B`.
//!
//! The query then asks whether the recorded predicate can fail.
//!
//! Memory is an array of bytes, and storage and transient storage arrays of words, threaded
//! through the dominator chain: loads read the current array, and stores, `mstore8` included,
//! update it. Any other write to a location, and any call or create, replaces every array by a
//! fresh one. At a join on the chain, each array is the one its predecessor's context ends with,
//! in the disjunction over predecessors; entering any other block along an edge other than from
//! the block before it in the chain replaces them too, since another path may have written them. Fresh arrays, and memory at the function's
//! entry, are opaque. So a read CSE or load PRE forwarded from an earlier store is proven when the
//! store dominates it with no unknown write between.
//!
//! Each query comes in up to two tiers. The exact tier is written as `NAME.smt2`. When the
//! query multiplies, divides or takes a remainder by a value that is not a constant, the
//! abstract tier `NAME.abstract.smt2` replaces those operations by uninterpreted functions
//! with the laws they obey (zero and one operands, `x % n < n`, `x / n <= x`). It allows more
//! executions than the exact tier, so `unsat` there proves the fact, usually far faster than
//! 256-bit bit-vector division; `sat` there decides nothing. A header lists the opaque values
//! the goal depends on (loads, calls, hashes, and earlier loop passes the encoding does not
//! follow), whose model values a real execution may not produce, and the facts taken on trust.
//!
//! [`lower-assumes`]: super::lower_assumes

use super::lower_assumes::Fact;
use crate::{
    backend::evm::{op, select::InstKindOpcode},
    mir::{
        AssumePredicate, BlockId, EffectKind, Function, FunctionId, InstId, InstKind, PtrLocation,
        ResultKind, Semantics, Terminator, Value, ValueId,
        analysis::{CfgInfo, integers::integer_mask},
        memory::EvmMemoryLayout,
    },
};
use alloy_primitives::U256;
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::IndexVec,
    map::{FxHashMap, FxIndexSet},
};
use std::{fmt::Write, io::Write as _, path::Path};

/// Writes a query for every fact `func` recorded to `dir`, or to stdout when `dir` is `-`.
///
/// A query's name is its module, function, block, value, analysis and its index among the
/// function's facts, so it does not depend on the order functions compile in.
pub(super) fn write_queries(
    module: &str,
    id: FunctionId,
    func: &Function,
    facts: &[Fact],
    dir: &Path,
) -> std::io::Result<()> {
    if facts.is_empty() || func.blocks.is_empty() {
        return Ok(());
    }
    let to_stdout = dir == Path::new("-");
    if !to_stdout {
        std::fs::create_dir_all(dir)?;
    }
    let prover = Prover::new(func, facts);
    let mut stdout = String::new();
    for (index, fact) in facts.iter().enumerate() {
        let goal = Goal { fact: index };
        let query = prover.query(&goal);
        let (lhs, rhs) = (prover.term_name(fact.lhs), prover.term_name(fact.rhs));
        let name = format!(
            "{}-f{}-bb{}-{lhs}-{}-{index}.smt2",
            sanitize(module),
            id.index(),
            fact.block.index(),
            fact.source.name(),
        );
        let definition = match func.value(fact.lhs) {
            Value::Inst(inst) => func.inst(*inst).kind.mnemonic(),
            Value::Arg(_) => "argument",
            _ => "constant",
        };
        let header = format!(
            "; module: {module}\n; function: #{} {}\n; claim: {} {lhs} {} {rhs} at the end of bb{}\n; definition: {definition}\n; opaque: {}\n; axioms: {}\n",
            id.index(),
            func.name,
            fact.source.name(),
            fact.predicate.name(),
            fact.block.index(),
            query.opaque.join(" "),
            query.axioms.join(" "),
        );
        let mut files = vec![(name.clone(), header.clone() + &query.exact)];
        if let Some(abstracted) = &query.abstracted {
            files.push((name.replace(".smt2", ".abstract.smt2"), header + abstracted));
        }
        for (name, text) in files {
            if to_stdout {
                stdout.push_str(&format!("; query: {name}\n{text}"));
            } else {
                std::fs::write(dir.join(name), text)?;
            }
        }
    }
    if to_stdout {
        // One write per function keeps its queries together when modules compile in parallel.
        std::io::stdout().lock().write_all(stdout.as_bytes())?;
    }
    Ok(())
}

/// A file-name-safe form of a module name.
fn sanitize(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect()
}

/// A fact to prove: the facts recorded before it in its block, and those at the ends of the
/// blocks dominating it, ran before it.
struct Goal {
    /// Its index among the function's facts, which are in block and instruction order.
    fact: usize,
}

struct Prover<'a> {
    func: &'a Function,
    facts: &'a [Fact],
    cfg: CfgInfo,
    definitions: IndexVec<InstId, Option<BlockId>>,
    names: FxHashMap<InstId, u32>,
}

impl<'a> Prover<'a> {
    fn new(func: &'a Function, facts: &'a [Fact]) -> Self {
        // vN names the Nth instruction with a result, in allocation order.
        let names = (0..func.num_insts())
            .map(InstId::from_usize)
            .filter(|&id| func.inst(id).result_ty.is_some())
            .zip(0..)
            .collect();
        Self { func, facts, cfg: CfgInfo::new(func), definitions: func.inst_block_table(), names }
    }

    /// A value's name in a query's file name: its MIR text name, or its constant.
    fn term_name(&self, value: ValueId) -> String {
        if let Some(constant) = self.func.value_u256(value) {
            return format!("{constant:#x}");
        }
        match *self.func.value(value) {
            Value::Inst(inst) => format!("v{}", self.names.get(&inst).copied().unwrap_or(u32::MAX)),
            Value::Arg(arg) => format!("arg{}", arg.index()),
            _ => format!("value{}", value.index()),
        }
    }

    fn def_block(&self, value: ValueId) -> Option<BlockId> {
        match self.func.value(value) {
            Value::Inst(inst) => self.definitions[*inst],
            _ => None,
        }
    }

    fn query(&self, goal: &Goal) -> Encoded {
        let mut query = Query::new(self.func, &self.names, &self.definitions);
        let fact = &self.facts[goal.fact];
        self.encode_context(&mut query, fact.block, None, Some(goal.fact), 0);
        let claim = self.claim(&mut query, fact, None);
        query.assert(format!("(not {claim})"));
        let operands = [query.term(fact.lhs), query.term(fact.rhs)];
        query.finish(&operands)
    }

    /// The fact's predicate over its operands, with `substitute`'s term for its value.
    fn claim(
        &self,
        query: &mut Query<'_>,
        fact: &Fact,
        substitute: Option<(ValueId, &str)>,
    ) -> String {
        let mut term = |value: ValueId| match substitute {
            Some((replaced, term)) if replaced == value => term.to_owned(),
            _ => query.term(value),
        };
        let (lhs, rhs) = (term(fact.lhs), term(fact.rhs));
        match fact.predicate {
            AssumePredicate::Eq => format!("(= {lhs} {rhs})"),
            AssumePredicate::Ne => format!("(not (= {lhs} {rhs}))"),
            AssumePredicate::Ult => format!("(bvult {lhs} {rhs})"),
            AssumePredicate::Ule => format!("(bvule {lhs} {rhs})"),
        }
    }

    /// Encodes the execution up to the end of `block` (or its first `prefix` instructions):
    /// the definitions and edge conditions of its dominator chain and the facts checked on it.
    /// `assumed_in_block` limits the facts of `block` itself to those checked before the goal;
    /// `None` assumes all of them.
    fn encode_context(
        &self,
        query: &mut Query<'_>,
        block: BlockId,
        prefix: Option<usize>,
        assumed_in_block: Option<usize>,
        depth: u32,
    ) {
        // From the entry down to the goal's block.
        let mut chain = self.cfg.dominators().self_and_dominators(block);
        chain.reverse();
        let mut in_chain = DenseBitSet::new_empty(self.func.blocks.len());
        for &block in &chain {
            in_chain.insert(block);
        }
        let mut previous = None;
        for &current in &chain {
            // The enclosing query already encodes the blocks it shares.
            if query.is_shared(current) {
                previous = Some(current);
                continue;
            }
            let body = &self.func.blocks[current];
            let joined = depth == 0 && body.predecessors.len() > 1;
            // Memory and storage flow on along an edge from the block before in the chain; a
            // join merges them from its predecessors' contexts. Reached any other way, another
            // path may have written them.
            if !joined
                && let Some(previous) = previous
                && body.predecessors.as_slice() != [previous]
            {
                query.havoc_states();
            }
            previous = Some(current);
            let count = if current == block {
                prefix.unwrap_or(body.instructions.len())
            } else {
                body.instructions.len()
            };
            if joined {
                self.encode_join(query, current);
            }
            for &inst in &body.instructions[..count] {
                let Some(value) = self.func.inst_result_value(inst) else {
                    query.apply_effects(inst);
                    continue;
                };
                if let InstKind::Phi(incoming) = &self.func.inst(inst).kind {
                    if joined {
                        let term = query.declare(value);
                        query.guarantee(&term, value);
                    } else {
                        self.encode_phi(query, current, value, incoming);
                    }
                } else {
                    query.define(value);
                }
                query.apply_effects(inst);
            }
            // The edge into this block, when it has a single predecessor.
            if let [predecessor] = body.predecessors.as_slice()
                && in_chain.contains(*predecessor)
            {
                self.encode_edge(query, *predecessor, current);
            }
        }
        // Facts that ran before: at the ends of the dominating blocks, and at the end of this
        // block those before the goal.
        for (index, fact) in self.facts.iter().enumerate() {
            if query.is_shared(fact.block) {
                continue;
            }
            let before = if fact.block == block {
                assumed_in_block.is_none_or(|assumed| index < assumed)
            } else {
                in_chain.contains(fact.block)
            };
            if before && query.has(fact.lhs) && query.has(fact.rhs) {
                let claim = self.claim(query, fact, None);
                query.assert(claim);
            }
        }
    }

    /// Execution reached the join through one of its predecessors: the disjunction, over them,
    /// of the context up to the predecessor's end below the join's dominators, under fresh
    /// names, with the facts checked on it, the edge's condition, and each phi equal to its
    /// incoming value on that edge. For a loop's back edge this is the earlier pass through the
    /// loop, and its facts the induction hypothesis: in the first execution that violates a
    /// check, every check that ran before passed. Inside such a context, joins are encoded by
    /// their phis alone.
    fn encode_join(&self, query: &mut Query<'_>, join: BlockId) {
        let mut shared = DenseBitSet::new_empty(self.func.blocks.len());
        for block in self.cfg.dominators().self_and_dominators(join) {
            if block != join {
                shared.insert(block);
            }
        }
        let phis: Vec<_> = self.func.blocks[join]
            .instructions
            .iter()
            .filter_map(|&inst| match &self.func.inst(inst).kind {
                InstKind::Phi(incoming) => Some((self.func.inst_result_value(inst)?, incoming)),
                _ => None,
            })
            .collect();
        // Memory, storage and transient storage at the join are each one of the predecessors'.
        let merged: [String; 3] = std::array::from_fn(|state| query.new_state(state));
        let mut paths = Vec::new();
        for (index, &predecessor) in self.func.blocks[join].predecessors.iter().enumerate() {
            if !self.cfg.is_reachable(predecessor) {
                continue;
            }
            let outer = std::mem::take(&mut query.assertions);
            let saved = query.enter(format!("j{}_{}_", join.index(), index), shared.clone());
            self.encode_context(query, predecessor, None, None, 1);
            self.encode_edge(query, predecessor, join);
            let incoming: Vec<_> = phis
                .iter()
                .filter_map(|&(phi, incoming)| {
                    let &(_, value) = incoming.iter().find(|&&(from, _)| from == predecessor)?;
                    Some((phi, query.term(value)))
                })
                .collect();
            let ends = query.states.clone();
            query.leave(saved);
            for (state, end) in ends.into_iter().enumerate() {
                query.depend(&merged[state], [end.clone()]);
                query.assert(format!("(= {} {end})", merged[state]));
            }
            for (phi, term) in incoming {
                let phi = query.declare(phi);
                query.depend(&phi, [term.clone()]);
                query.assert(format!("(= {phi} {term})"));
            }
            let path = std::mem::replace(&mut query.assertions, outer);
            let terms: Vec<_> = path
                .lines()
                .filter_map(|line| line.strip_prefix("(assert ")?.strip_suffix(')'))
                .collect();
            paths.push(format!("(and true {})", terms.join(" ")));
        }
        if paths.is_empty() {
            query.havoc_states();
        } else {
            query.assert(format!("(or false {})", paths.join(" ")));
            query.states = merged;
        }
    }

    /// The condition under which `predecessor`'s terminator branches to `target`.
    fn encode_edge(&self, query: &mut Query<'_>, predecessor: BlockId, target: BlockId) {
        match &self.func.blocks[predecessor].terminator {
            Some(Terminator::Branch { condition, then_block, else_block })
                if then_block != else_block =>
            {
                let condition = query.term(*condition);
                let zero = word(U256::ZERO);
                let is_zero = format!("(= {condition} {zero})");
                query.assert(if *then_block == target {
                    format!("(not {is_zero})")
                } else {
                    is_zero
                });
            }
            Some(Terminator::Switch { value, default, cases }) => {
                let scrutinee = query.term(*value);
                let matched: Vec<_> = cases
                    .iter()
                    .filter(|&&(_, case_target)| case_target == target)
                    .map(|&(case, _)| format!("(= {scrutinee} {})", query.term(case)))
                    .collect();
                if !matched.is_empty() {
                    query.assert(format!("(or false {})", matched.join(" ")));
                } else if *default == target {
                    for &(case, _) in cases {
                        let case = query.term(case);
                        query.assert(format!("(not (= {scrutinee} {case}))"));
                    }
                }
            }
            _ => {}
        }
    }

    /// `phi = one of its incoming values`, inside a predecessor's context of [`Self::encode_join`]:
    /// the value itself when it is defined above the phi's block, or else a fresh word with the
    /// facts its predecessor checked at its end and the flow-insensitive facts about it.
    fn encode_phi(
        &self,
        query: &mut Query<'_>,
        join: BlockId,
        phi: ValueId,
        incoming: &[(BlockId, ValueId)],
    ) {
        let phi_term = query.declare(phi);
        let mut options = Vec::new();
        for (index, &(predecessor, value)) in incoming.iter().enumerate() {
            let above = match self.def_block(value) {
                Some(def) => def != join && self.cfg.dominators().dominates(def, join),
                None => true,
            };
            if above {
                let term = query.term(value);
                query.depend(&phi_term, [term.clone()]);
                options.push(format!("(= {phi_term} {term})"));
                continue;
            }
            let fresh = query.fresh(&format!("{}in{}_{}", query.prefix, phi.index(), index));
            query.opaque.insert(fresh.clone());
            query.depend(&phi_term, [fresh.clone()]);
            query.guarantee(&fresh, value);
            // The facts its predecessor recorded about it, whose other operand the query has.
            for fact in self.facts.iter().filter(|fact| fact.block == predecessor) {
                let other = if fact.lhs == value { fact.rhs } else { fact.lhs };
                if (fact.lhs == value || fact.rhs == value) && query.has(other) {
                    let claim = self.claim(query, fact, Some((value, &fresh)));
                    query.assert(claim);
                }
            }
            options.push(format!("(= {phi_term} {fresh})"));
        }
        if !options.is_empty() {
            query.assert(format!("(or false {})", options.join(" ")));
        }
        query.guarantee(&phi_term, phi);
    }
}

/// One query under construction.
struct Query<'a> {
    func: &'a Function,
    names: &'a FxHashMap<InstId, u32>,
    definitions: &'a IndexVec<InstId, Option<BlockId>>,
    declared: FxIndexSet<String>,
    assertions: String,
    /// Exact definitions the abstract tier replaces, with their replacements.
    abstractions: Vec<(String, String)>,
    /// Uninterpreted functions the abstract tier declares.
    functions: FxIndexSet<&'static str>,
    /// The transaction's inputs the query reads, as functions of their operands with their
    /// arities: two reads of one input agree.
    inputs: FxIndexSet<(String, usize)>,
    /// Symbols whose values the query leaves free: loads, calls, hashes, state, and loop
    /// values from an earlier pass that the encoding does not follow.
    opaque: FxIndexSet<String>,
    /// The symbols each symbol's definition reads.
    deps: FxHashMap<String, Vec<String>>,
    /// The facts the query takes on trust, by name.
    axioms: FxIndexSet<&'static str>,
    /// The current memory, storage and transient storage, as SMT arrays.
    states: [String; 3],
    /// Every array the query declares, with whether it is byte-addressed memory.
    arrays: FxIndexSet<(String, bool)>,
    /// The states each nested context entered from, to restore when it ends.
    saved_states: Vec<[String; 3]>,
    /// Prepended to the names of values defined outside `shared`, for an earlier pass.
    prefix: String,
    shared: Option<DenseBitSet<BlockId>>,
}

impl<'a> Query<'a> {
    fn new(
        func: &'a Function,
        names: &'a FxHashMap<InstId, u32>,
        definitions: &'a IndexVec<InstId, Option<BlockId>>,
    ) -> Self {
        Self {
            func,
            names,
            definitions,
            declared: FxIndexSet::default(),
            assertions: String::new(),
            abstractions: Vec::new(),
            functions: FxIndexSet::default(),
            inputs: FxIndexSet::default(),
            opaque: FxIndexSet::default(),
            deps: FxHashMap::default(),
            axioms: FxIndexSet::default(),
            prefix: String::new(),
            shared: None,
            states: ["mem0".into(), "sto0".into(), "tra0".into()],
            arrays: [("mem0".into(), true), ("sto0".into(), false), ("tra0".into(), false)]
                .into_iter()
                .collect(),
            saved_states: Vec::new(),
        }
        // Memory on entry is the caller's, which the query does not follow.
        .with_opaque("mem0")
    }

    fn with_opaque(mut self, symbol: &str) -> Self {
        self.opaque.insert(symbol.to_owned());
        self
    }

    /// Replaces memory, storage and transient storage with states the query knows nothing about.
    fn havoc_states(&mut self) {
        for state in 0..3 {
            self.havoc_state(state);
        }
    }

    fn havoc_state(&mut self, state: usize) {
        let name = self.new_state(state);
        self.opaque.insert(name.clone());
        self.states[state] = name;
    }

    /// Declares a new array for `state`: memory, storage or transient storage.
    fn new_state(&mut self, state: usize) -> String {
        let name = format!("{}{}{}", self.prefix, ["mem", "sto", "tra"][state], self.arrays.len());
        self.arrays.insert((name.clone(), state == 0));
        name
    }

    /// A new state of `state` equal to `update` of the previous one.
    fn update_state(
        &mut self,
        state: usize,
        update: impl FnOnce(&str) -> String,
        reads: Vec<String>,
    ) {
        let previous = self.states[state].clone();
        let name = format!("{}{}{}", self.prefix, ["mem", "sto", "tra"][state], self.arrays.len());
        self.arrays.insert((name.clone(), state == 0));
        let definition = update(&previous);
        self.assert(format!("(= {name} {definition})"));
        self.depend(&name, reads.into_iter().chain([previous]));
        self.states[state] = name;
    }

    /// Updates the states for what `inst` writes: memory, storage and transient stores exactly,
    /// and every other write, call or creation by forgetting what it may change.
    fn apply_effects(&mut self, inst: InstId) {
        let kind = &self.func.inst(inst).kind;
        match *kind {
            InstKind::Store(ptr, value, location) => {
                let state = match location {
                    PtrLocation::Memory => 0,
                    PtrLocation::Storage => 1,
                    PtrLocation::Transient => 2,
                    _ => return,
                };
                let (ptr, value) = (self.term(ptr), self.term(value));
                let reads = vec![ptr.clone(), value.clone()];
                if state == 0 {
                    self.update_state(0, |memory| store_word(memory, &ptr, &value), reads);
                } else {
                    self.update_state(
                        state,
                        |array| format!("(store {array} {ptr} {value})"),
                        reads,
                    );
                }
            }
            InstKind::Store8(ptr, value) => {
                let (ptr, value) = (self.term(ptr), self.term(value));
                let reads = vec![ptr.clone(), value.clone()];
                self.update_state(
                    0,
                    |memory| format!("(store {memory} {ptr} ((_ extract 7 0) {value}))"),
                    reads,
                );
            }
            _ => match kind.effect_kind() {
                EffectKind::MemoryWrite => self.havoc_state(0),
                EffectKind::StorageWrite => self.havoc_state(1),
                EffectKind::TransientWrite => self.havoc_state(2),
                EffectKind::ExternalCall | EffectKind::ICall | EffectKind::Create => {
                    self.havoc_states();
                }
                _ => {}
            },
        }
    }

    /// Names values defined outside `shared` with `prefix` until [`Self::leave`].
    fn enter(
        &mut self,
        prefix: String,
        shared: DenseBitSet<BlockId>,
    ) -> (String, Option<DenseBitSet<BlockId>>) {
        let prefix = format!("{}{prefix}", self.prefix);
        self.saved_states.push(self.states.clone());
        (std::mem::replace(&mut self.prefix, prefix), self.shared.replace(shared))
    }

    fn is_shared(&self, block: BlockId) -> bool {
        self.shared.as_ref().is_some_and(|shared| shared.contains(block))
    }

    fn leave(&mut self, (prefix, shared): (String, Option<DenseBitSet<BlockId>>)) {
        self.prefix = prefix;
        self.shared = shared;
        if let Some(states) = self.saved_states.pop() {
            self.states = states;
        }
    }

    fn assert(&mut self, term: String) {
        let _ = writeln!(self.assertions, "(assert {term})");
    }

    fn symbol(&self, value: ValueId) -> String {
        match *self.func.value(value) {
            Value::Inst(inst) => {
                let name = format!("v{}", self.names.get(&inst).copied().unwrap_or(u32::MAX));
                let shared = match (&self.shared, self.definitions[inst]) {
                    (Some(shared), Some(block)) => shared.contains(block),
                    _ => true,
                };
                if shared { name } else { format!("{}{name}", self.prefix) }
            }
            Value::Arg(arg) => format!("arg{}", arg.index()),
            _ => format!("undef{}", value.index()),
        }
    }

    fn has(&self, value: ValueId) -> bool {
        self.func.value_u256(value).is_some() || self.declared.contains(&self.symbol(value))
    }

    fn declare(&mut self, value: ValueId) -> String {
        let symbol = self.symbol(value);
        // An argument fits its type: the callers' `argument` obligations prove it.
        if self.declared.insert(symbol.clone())
            && let Value::Arg(arg) = *self.func.value(value)
            && let Some(bits) = self.func.arg_ty(arg).integer_bits()
            && bits < 256
        {
            self.assert(format!("(bvule {symbol} {})", word(integer_mask(bits))));
            self.axioms.insert("argument-type");
        }
        symbol
    }

    /// Records that `symbol`'s definition reads `terms`.
    fn depend(&mut self, symbol: &str, terms: impl IntoIterator<Item = String>) {
        let terms = terms.into_iter().filter(|term| !term.starts_with('#'));
        self.deps.entry(symbol.to_owned()).or_default().extend(terms);
    }

    fn fresh(&mut self, name: &str) -> String {
        self.declared.insert(name.to_owned());
        name.to_owned()
    }

    /// The term for a value: its constant, or its symbol, free unless something defined it.
    fn term(&mut self, value: ValueId) -> String {
        match self.func.value_u256(value) {
            Some(constant) => word(constant),
            None => self.declare(value),
        }
    }

    /// Bounds `term` by what the operation defining `value` can produce: one bit for a boolean,
    /// the result width of a narrow opcode.
    fn guarantee(&mut self, term: &str, value: ValueId) {
        let Some(kind) = self.func.value_inst_kind(value) else { return };
        let bits = if kind.op().result_kind() == ResultKind::I1 {
            1
        } else {
            kind.evm_opcode().and_then(op::definition).map_or(256, |def| u32::from(def.result_bits))
        };
        if bits < 256 {
            self.assert(format!("(bvule {term} {})", word(integer_mask(bits))));
        }
    }

    /// Defines a value by the semantics of the instruction that produces it.
    fn define(&mut self, value: ValueId) {
        let symbol = self.declare(value);
        let Value::Inst(inst) = *self.func.value(value) else { return };
        let kind = &self.func.inst(inst).kind;
        if matches!(kind, InstKind::HeapFloor) {
            let (lo, hi) = (U256::from(EvmMemoryLayout::HEAP_START), integer_mask(66));
            self.assert(within(&symbol, lo, hi));
            self.axioms.insert("heap-floor");
            return;
        }
        let mut reads = Vec::new();
        // A read of memory, storage or transient storage reads the current state.
        if let InstKind::Load(ptr, location) = *kind
            && let Some(state) = match location {
                PtrLocation::Memory => Some(0),
                PtrLocation::Storage => Some(1),
                PtrLocation::Transient => Some(2),
                _ => None,
            }
        {
            let ptr = self.term(ptr);
            let array = self.states[state].clone();
            let read = if state == 0 {
                load_word(&array, &ptr)
            } else {
                format!("(select {array} {ptr})")
            };
            self.depend(&symbol, [ptr, array]);
            self.assert(format!("(= {symbol} {read})"));
            return;
        }
        // A transaction's input is the same at every read with the same operands.
        if let Some(opcode) = kind.evm_opcode()
            && op::is_tx_input(opcode)
        {
            let operands: Vec<_> =
                kind.operands().iter().map(|&operand| self.term(operand)).collect();
            let function = format!("input_{}", op::mnemonic(opcode).unwrap_or("unknown"));
            let read = if operands.is_empty() {
                function.clone()
            } else {
                format!("({function} {})", operands.join(" "))
            };
            self.inputs.insert((function, operands.len()));
            self.depend(&symbol, operands);
            self.assert(format!("(= {symbol} {read})"));
            self.guarantee(&symbol, value);
            return;
        }
        let definition = match kind.semantics() {
            Some(Semantics::Opcode(_, operands)) => {
                let operands: Vec<_> = operands.iter().map(|&operand| self.term(operand)).collect();
                reads.extend(operands.iter().cloned());
                let operands: Vec<_> = operands.iter().map(String::as_str).collect();
                let opcode = kind.evm_opcode().filter(|&opcode| op::is_pure(opcode));
                let exact = opcode.and_then(|opcode| op::word::smt_opcode(opcode, &operands));
                if let (Some(opcode), Some(exact)) = (opcode, &exact)
                    && let Some((abstracted, function)) =
                        op::word::smt_opcode_abstract(opcode, &operands, &symbol)
                {
                    self.abstractions.push((format!("(= {symbol} {exact})"), abstracted));
                    self.functions.insert(function);
                }
                exact
            }
            Some(Semantics::Word(operand)) => Some(self.term(operand)),
            Some(Semantics::LowBits(operand, bits)) if (1..=256).contains(&bits) => {
                Some(format!("(bvand {} {})", self.term(operand), word(integer_mask(bits))))
            }
            Some(Semantics::SignExtend(operand, from, to))
                if from >= 1 && from <= to && to <= 256 =>
            {
                let operand = self.term(operand);
                let extended = format!(
                    "((_ sign_extend {}) ((_ extract {} 0) {operand}))",
                    256 - from,
                    from - 1
                );
                Some(format!("(bvand {extended} {})", word(integer_mask(to))))
            }
            Some(Semantics::NotEqual(a, b)) => {
                let (a, b) = (self.term(a), self.term(b));
                Some(format!("(ite (= {a} {b}) {} {})", word(U256::ZERO), word(U256::ONE)))
            }
            Some(Semantics::Select(condition, a, b)) => {
                let (condition, a, b) = (self.term(condition), self.term(a), self.term(b));
                Some(format!("(ite (= {condition} {}) {b} {a})", word(U256::ZERO)))
            }
            _ => None,
        };
        match definition {
            Some(definition) => {
                // Every symbol a definition mentions is an operand.
                reads.extend(
                    definition
                        .split(['(', ')', ' '])
                        .filter(|token| token.starts_with('v') || token.starts_with("arg"))
                        .map(str::to_owned),
                );
                self.depend(&symbol, reads);
                self.assert(format!("(= {symbol} {definition})"));
            }
            None => {
                self.opaque.insert(symbol.clone());
                self.guarantee(&symbol, value);
            }
        }
    }

    fn finish(self, goals: &[String]) -> Encoded {
        let render = |assertions: &str, functions: &FxIndexSet<&'static str>| {
            let mut out = String::from(
                "(set-logic QF_AUFBV)
",
            );
            for name in &self.declared {
                let _ = writeln!(out, "(declare-const {name} (_ BitVec 256))");
            }
            for (name, bytes) in &self.arrays {
                let element = if *bytes { 8 } else { 256 };
                let _ = writeln!(
                    out,
                    "(declare-const {name} (Array (_ BitVec 256) (_ BitVec {element})))"
                );
            }
            for (name, arity) in &self.inputs {
                let operands = vec!["(_ BitVec 256)"; *arity].join(" ");
                let _ = writeln!(out, "(declare-fun {name} ({operands}) (_ BitVec 256))");
            }
            for name in functions {
                let arity = if name.ends_with("mod3") { 3 } else { 2 };
                let operands = vec!["(_ BitVec 256)"; arity].join(" ");
                let _ = writeln!(out, "(declare-fun {name} ({operands}) (_ BitVec 256))");
            }
            out.push_str(assertions);
            let _ = writeln!(out, "(check-sat)\n(get-value ({}))\n(get-model)", goals.join(" "));
            out
        };
        let exact = render(&self.assertions, &FxIndexSet::default());
        let abstracted = (!self.abstractions.is_empty()).then(|| {
            let mut assertions = self.assertions.clone();
            for (exact, abstracted) in &self.abstractions {
                assertions = assertions.replace(exact.as_str(), abstracted);
            }
            render(&assertions, &self.functions)
        });
        // The opaque symbols the goal depends on.
        let mut opaque = Vec::new();
        let mut seen = FxIndexSet::default();
        let mut pending = goals.to_vec();
        while let Some(symbol) = pending.pop() {
            if !seen.insert(symbol.clone()) {
                continue;
            }
            if self.opaque.contains(&symbol) {
                opaque.push(symbol.clone());
            }
            if let Some(reads) = self.deps.get(&symbol) {
                pending.extend(reads.iter().cloned());
            }
        }
        opaque.sort();
        Encoded {
            exact,
            abstracted,
            opaque,
            axioms: self.axioms.iter().map(|&axiom| axiom.to_owned()).collect(),
        }
    }
}

/// A query's two tiers and what its counterexamples rest on.
struct Encoded {
    /// Exact 256-bit word semantics.
    exact: String,
    /// Multiplications, divisions and remainders by unknown values replaced by uninterpreted
    /// functions with the bounds they obey, when the query has any. It over-approximates
    /// `exact`, so `unsat` proves the fact too, often far faster; `sat` is inconclusive.
    abstracted: Option<String>,
    /// The opaque symbols the goal depends on: a counterexample is concrete only without them.
    opaque: Vec<String>,
    /// The facts the query takes on trust.
    axioms: Vec<String>,
}

/// The big-endian word at `ptr` in byte-addressed `memory`.
fn load_word(memory: &str, ptr: &str) -> String {
    let bytes: Vec<_> = (0..32u64)
        .map(|offset| format!("(select {memory} (bvadd {ptr} {}))", word(U256::from(offset))))
        .collect();
    format!("(concat {})", bytes.join(" "))
}

/// `memory` with the big-endian word `value` written at `ptr`.
fn store_word(memory: &str, ptr: &str, value: &str) -> String {
    let mut out = memory.to_owned();
    for offset in 0..32u64 {
        let high = 255 - 8 * offset;
        out = format!(
            "(store {out} (bvadd {ptr} {}) ((_ extract {high} {}) {value}))",
            word(U256::from(offset)),
            high - 7
        );
    }
    out
}

fn word(value: U256) -> String {
    op::word::smt_word(value)
}

/// `lo <= term <= hi`, leaving out the bound that always holds.
fn within(term: &str, lo: U256, hi: U256) -> String {
    let mut parts = Vec::new();
    if !lo.is_zero() {
        parts.push(format!("(bvuge {term} {})", word(lo)));
    }
    if hi != U256::MAX {
        parts.push(format!("(bvule {term} {})", word(hi)));
    }
    format!("(and true {})", parts.join(" "))
}
