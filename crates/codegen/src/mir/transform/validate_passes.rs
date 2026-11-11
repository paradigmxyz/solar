//! Translation validation of MIR passes: SMT queries that a pass preserved each function it
//! changed.
//!
//! With `-Zvalidate-passes=DIR`, the pass manager keeps a copy of the module before every MIR
//! pass that runs, and after a pass that reports a change hands both versions of each
//! function whose text changed to [`write_queries`]. Each pair becomes one SMT-LIB query in `DIR`
//! whose satisfiability means some input makes the two versions observably differ. `unsat`
//! proves the pass preserved the function for every input, independently of the analyses that
//! justified the rewrite; `sat` gives an input under which they differ, which is a miscompile
//! or a difference the encoding below is too coarse to rule out, and needs triage. A function the
//! encoding does not cover gets a `.skip` file with the reason instead. `tools/prove-analyses
//! --validate-passes` compiles a corpus this way, solves the queries with Z3, reports verdicts per
//! pass, and with `--certify` checks each `unsat` with cvc5 and Ethos like the analysis queries.
//!
//! This complements `-Zprove-analyses`, which proves the facts a pass recorded but not that the
//! rewrite those facts justify is right: a pass that substitutes the wrong value for a proved
//! fact, or rewrites code no fact covers, passes the prover and fails here. Debug info and gas
//! are not compared; the validator checks meaning only.
//!
//! # Encoding
//!
//! Both versions run symbolically from one shared start: the same arguments (within their
//! types), memory, storage, transient storage and world. A function must be loop-free over its
//! reachable blocks, which then run once each in reverse postorder. Each block has a guard, the
//! disjunction over its predecessors of their guard and edge condition; its phis, memory, storage
//! and transient storage are `ite` chains over the same incoming edges. Values follow the
//! operation's declared [`Semantics`], as in `prove_analyses`: pure opcodes with exact 256-bit
//! semantics (`smt_opcode`), masked to the result's width when it is a narrow integer, which
//! wraps at its width until `lower-integers` (a signed opcode on one is skipped), casts, `select`
//! and `ne`; memory is an array of bytes and storage
//! and transient storage arrays of words, updated by every store. Every term is hash-consed, so a
//! computation both versions share is one symbol and a query costs little beyond the rewrite.
//!
//! Byte-level memory is slow to decide, so a load reads through the stores it is built from
//! while their addresses decide it: a store at the same address gives its value, and one the
//! word provably misses is skipped. An address is a root term plus a constant offset, or plus a
//! value a mask or a type bounds, and two accesses from one root are apart when every distance
//! their bounds allow is; accesses from different roots are apart when the bounds asserted on
//! the roots separate them. Only what is left goes to the solver as byte selects.
//!
//! A query takes these facts on trust, and lists the ones it uses in its header:
//!
//! - `argument-type` and `result-type`: an argument, and the result of a call, fits its type;
//! - `free-memory-pointer`: on entry to a function other than the dispatcher and the constructor,
//!   the free-memory pointer lies above the reserved words and below 2^66, as every entry
//!   establishes; the dispatcher and the constructor start with zeroed memory instead;
//! - `memory-gas`: every memory access that runs ends below 2^64, since an execution that touches
//!   memory past it runs out of gas first, and the validator compares executions that end, not
//!   their gas;
//! - `heap-floor` and `allocation`: the heap floor and every allocation the backend places lie
//!   above the reserved words and below 2^66, deferred allocations apart from each other, as
//!   `prove_analyses` takes them.
//!
//! What the code cannot see is uninterpreted, a function of everything it may depend on, and
//! both versions apply the same functions:
//!
//! - transaction inputs (calldata, the caller, the block) are functions of their operands;
//! - the world, a word, stands for everything outside the function: a query relates finitely many
//!   worlds, which an interpretation keeps apart in 2^256 values. Every external call, creation,
//!   log and internal call is an event that maps the world, the operands and the current memory,
//!   storage and transient storage to a new world, and its result and the memory, storage and
//!   transient storage after it are functions of that new world. So two versions agree after a call
//!   only when they made the same calls in the same order with the same inputs. An internal call
//!   may also end the execution (the callee reverts or stops), which is a function of its new world
//!   too;
//! - environment reads (`balance`, `returndatasize`, `gas`, `msize`, ...) are functions of the
//!   world and their operands, and copies into memory (`calldatacopy`, `mcopy`, `codecopy`, ...)
//!   functions of the world, memory and operands that give the new memory;
//! - `keccak256` of a constant number of words is a function of those words, and of any other
//!   length a function of memory, offset and length;
//! - `exp` is exact for constant operands, a power-of-two base and small constant exponents, and
//!   `clz` and any other `exp` are uninterpreted.
//!
//! A version ends in one of its outcomes, and the query asserts that the two end in outcomes
//! that differ: in kind (return, return data, revert, revert with return data,
//! self-destruct, `invalid`, an internal call that ended the execution), or within a kind in
//!
//! - an internal return: the values, storage, transient storage, the world (every call and log
//!   made), the free-memory pointer, and memory between the scratch words and the free-memory
//!   pointer on entry, which the caller had allocated. Memory the function allocated is not
//!   compared, so a pass that corrupts an object a function allocates and returns is missed;
//! - external return data, and stop as returning no bytes: the returned bytes, storage, transient
//!   storage and world;
//! - a revert: the revert data alone, since the rest rolls back;
//! - a continuation, which EVM IR regions end in (see `backend/evm/ir/passes/validate.rs`): the
//!   target address, the stack, all of memory, storage, transient storage and the world.
//!
//! # Limits
//!
//! - Functions with a loop are skipped, as are functions with an operation the encoding does not
//!   model (semantic-phase aggregates, slices, ABI, checked arithmetic, frames, immutables, builtin
//!   calls, `undef`): most of the semantic pipeline is outside the encoding, and passes are
//!   validated once the functions they change are word code. Allocations are modeled: one the
//!   backend places by its instruction's identity, any other as a bump of the free-memory pointer
//!   with its overflow panic and zeroing, which `lower-alloc` may elide on knowledge of the whole
//!   module the query lacks.
//! - Functions whose signature or ABI role a pass changes (`lower-abi` turns an external function
//!   into its wrapper) are skipped.
//! - Passes that change functions together (inlining, specialization, argument and return cleanups
//!   across calls, function merging) are skipped: validating one function alone treats its callees
//!   as unknown and cannot see that a rewrite is justified by a callee's body.
//! - A pass that relies on a callee's summary (CSE or dead store elimination across a call to a
//!   function that writes no memory) gets a counterexample through the call's uninterpreted
//!   functions.
//! - A call's input memory is all of memory, so a pass that removes a store to memory a call never
//!   reads gets a counterexample (marked opaque, like every counterexample through an uninterpreted
//!   function: the model may not be realizable).
//! - Two versions that compute the same value through different uninterpreted applications, such as
//!   `exp(2, x)` and `shl(x, 1)` with a symbolic base, are not proved equal.
//! - Validation checks equivalence, not refinement, so a pass may not make an observable more
//!   defined: none of the modeled operations is undefined.
//!
//! The hook runs in the pass manager after every MIR pass that reports a change, in the canonical
//! pipelines and in a `-Zmir-pipeline` alike, and costs a copy of the module per pass only with
//! the flag.
//!
//! [`Semantics`]: crate::mir::Semantics

use super::prove_analyses::{load_word, store_word};
use crate::{
    backend::evm::{op, select::InstKindOpcode},
    mir::{
        AllocationFailure, AllocationInitialization, AllocationSemantics, ArgIdx, BlockId, Callee,
        EffectKind, Function, FunctionId, InstId, InstKind, Module, PtrLocation, ResultKind,
        Semantics, Terminator, Value, ValueId,
        analysis::{CfgInfo, integers::integer_mask},
        memory::EvmMemoryLayout,
    },
};
use alloy_primitives::U256;
use solar_data_structures::{
    index::IndexVec,
    map::{FxHashMap, FxHashSet, FxIndexMap, FxIndexSet},
};
use std::{
    fmt::Write,
    hash::{DefaultHasher, Hash, Hasher},
    io::Write as _,
    path::Path,
};

pub(crate) use super::prove_analyses::{sanitize, word};

/// Passes whose rewrites depend on other functions, which a query of one function cannot follow.
const INTERPROCEDURAL: &[&str] = &[
    "inline",
    "inline-physical",
    "inline-tagged",
    "call-cleanup",
    "dead-arg-elim",
    "specialize",
    "specialize-function-pointers",
    "merge-equivalent-functions",
    "noreturn-calls",
    "pure-eval",
    "readonly-eval",
    "function-dce",
    "optimize-function",
];

/// Writes a query to `dir`, or stdout when `dir` is `-`, for every function of `after` whose
/// text differs from its version in `before`, the module before `pass` ran.
pub(crate) fn write_queries(
    pass: &str,
    before: &Module,
    after: &Module,
    dir: &Path,
) -> std::io::Result<()> {
    let module = after.name.as_str();
    let to_stdout = dir == Path::new("-");
    if !to_stdout {
        std::fs::create_dir_all(dir)?;
    }
    let by_name: FxHashMap<String, FunctionId> =
        before.functions.iter_enumerated().map(|(id, func)| (func.name.to_string(), id)).collect();
    let mut stdout = String::new();
    for (id, func) in after.functions.iter_enumerated() {
        let name = func.name.to_string();
        let Some(&old_id) = by_name.get(&name) else { continue };
        let old = &before.functions[old_id];
        let old_text = before.function_text(old_id).to_string();
        let new_text = after.function_text(id).to_string();
        if old_text == new_text || func.blocks.is_empty() {
            continue;
        }
        let mut hasher = DefaultHasher::new();
        (pass, &old_text, &new_text).hash(&mut hasher);
        let file = format!(
            "{}-{}-f{}-{:012x}",
            sanitize(module),
            sanitize(pass),
            id.index(),
            hasher.finish() & 0xffff_ffff_ffff
        );
        let header = format!(
            "; module: {module}\n; function: #{} {name}\n; pass: {pass}\n; claim: {pass} preserves #{} {name}\n",
            id.index(),
            id.index(),
        );
        let query = encode(
            pass,
            after.dispatch_entry() == Some(id) || after.constructor() == Some(id),
            (old, &before.functions),
            (func, &after.functions),
        );
        for (file, text) in query_files(&file, &header, query) {
            if to_stdout {
                stdout.push_str(&format!("; query: {file}\n{text}"));
            } else {
                std::fs::write(dir.join(file), text)?;
            }
        }
    }
    if to_stdout {
        std::io::stdout().lock().write_all(stdout.as_bytes())?;
    }
    Ok(())
}

/// The files of a query named `name`, without an extension, under `header`: the query and its
/// abstract tier, or a `.skip` file with the reason it could not be encoded.
pub(crate) fn query_files(
    name: &str,
    header: &str,
    query: Result<Query, String>,
) -> Vec<(String, String)> {
    match query {
        Ok(query) => {
            let header = format!(
                "{header}; opaque: {}\n; axioms: {}\n",
                query.opaque.join(" "),
                query.axioms.join(" ")
            );
            let mut files = vec![(format!("{name}.smt2"), format!("{header}{}", query.text))];
            if let Some(abstracted) = query.abstracted {
                files.push((format!("{name}.abstract.smt2"), format!("{header}{abstracted}")));
            }
            files
        }
        Err(reason) => vec![(format!("{name}.skip"), format!("{header}; skipped: {reason}\n"))],
    }
}

/// A finished query and what its counterexamples rest on.
pub(crate) struct Query {
    pub(crate) text: String,
    /// The query with multiplications, divisions and remainders by unknown values replaced by
    /// uninterpreted functions with the laws they obey, when it has any: it over-approximates
    /// `text`, so `unsat` proves the claim too, often far faster.
    pub(crate) abstracted: Option<String>,
    /// The uninterpreted functions the query applies: a counterexample through them may not be
    /// realizable.
    pub(crate) opaque: Vec<String>,
    /// The facts the query takes on trust.
    pub(crate) axioms: Vec<String>,
}

/// One version of a function, with its module's functions for naming callees.
type Version<'a> = (&'a Function, &'a IndexVec<FunctionId, Function>);

/// Encodes whether some input makes `before` and `after` differ, or the reason it cannot. An
/// `entry`, the dispatcher or the constructor, starts with zeroed memory.
fn encode(
    pass: &str,
    entry: bool,
    before: Version<'_>,
    after: Version<'_>,
) -> Result<Query, String> {
    if INTERPROCEDURAL.contains(&pass) {
        return Err(format!("`{pass}` rewrites functions together"));
    }
    if before.0.params != after.0.params
        || before.0.return_abi() != after.0.return_abi()
        || before.0.return_type() != after.0.return_type()
        || before.0.attributes.is_abi_wrapper != after.0.attributes.is_abi_wrapper
    {
        return Err("signature changed".into());
    }
    let mut encoder = Encoder::default();
    let start = State {
        memory: encoder.declare("mem0", MEMORY),
        storage: encoder.declare("sto0", STORAGE),
        transient: encoder.declare("tra0", STORAGE),
        world: "world0".into(),
    };
    // NOTE: QF_AUFBV has no constant arrays, so an entry's zeroed memory reaches only the words
    // [`Encoder::load`] reads through to the start; a byte read leaves it free, which allows more
    // executions and so stays sound.
    encoder.zeroed = entry;
    if !entry {
        encoder.entry_fmp();
    }
    // An argument fits its type: the callers' `argument` obligations prove it.
    for (arg, ty) in before.0.params.iter_enumerated() {
        let name = encoder.declare(&arg_name(arg), WORD);
        if let Some(bits) = ty.integer_bits()
            && bits < 256
        {
            encoder.axioms.insert("argument-type");
            encoder.values.insert(name.clone(), (U256::ZERO, integer_mask(bits)));
            let _ = writeln!(
                encoder.assertions,
                "(assert (bvule {name} {}))",
                word(integer_mask(bits))
            );
        }
    }
    let old = encoder.run(before, &start)?;
    let new = encoder.run(after, &start)?;
    let mut differences = Vec::new();
    for a in &old {
        for b in &new {
            let differ = encoder.differ(a, b);
            if differ != "false" {
                differences.push(all(&[a.guard.as_str(), b.guard.as_str(), differ.as_str()]));
            }
        }
    }
    let _ = writeln!(encoder.assertions, "(assert {})", any(&differences));
    Ok(encoder.finish())
}

pub(crate) const WORD: &str = "(_ BitVec 256)";
pub(crate) const BOOL: &str = "Bool";
pub(crate) const MEMORY: &str = "(Array (_ BitVec 256) (_ BitVec 8))";
pub(crate) const STORAGE: &str = "(Array (_ BitVec 256) (_ BitVec 256))";
pub(crate) const WORLD: &str = "World";

/// Memory, storage, transient storage and the world at a point of an execution.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct State {
    pub(crate) memory: String,
    pub(crate) storage: String,
    pub(crate) transient: String,
    pub(crate) world: String,
}

/// How an execution ends.
pub(crate) enum Kind {
    Return(Vec<String>),
    ReturnData {
        offset: String,
        size: String,
    },
    Revert {
        offset: String,
        size: String,
    },
    RevertReturndata,
    SelfDestruct(String),
    Invalid,
    /// An internal call or tail call ended the execution in its callee.
    Ended,
    /// Control goes on at `target`, a block's address, with `stack`, bottom first: an exit of an
    /// EVM IR region to code that another query covers.
    Continue {
        target: String,
        stack: Vec<String>,
    },
}

/// An outcome of a version: when it happens, how it ends, and the state it ends with.
pub(crate) struct Outcome {
    pub(crate) guard: String,
    pub(crate) kind: Kind,
    pub(crate) state: State,
}

#[derive(Default)]
pub(crate) struct Encoder {
    /// Declared constants with their sorts.
    constants: FxIndexMap<String, &'static str>,
    /// Uninterpreted functions: name to parameter and result sorts.
    functions: FxIndexMap<String, (Vec<&'static str>, &'static str)>,
    /// Hash-consed definitions: a defining term to the symbol that names it.
    definitions: FxHashMap<String, String>,
    pub(crate) assertions: String,
    pub(crate) axioms: FxIndexSet<&'static str>,
    /// The terms that are applications of uninterpreted functions.
    uninterpreted: FxHashSet<String>,
    /// The deferred allocations declared so far, with their sizes.
    allocations: Vec<(String, U256)>,
    /// Each memory term defined as a write to the one before it.
    writes: FxHashMap<String, MemoryWrite>,
    /// Terms that are a root term plus a constant: see [`Self::split`].
    offsets: FxHashMap<String, (String, U256)>,
    /// Whether memory starts zeroed, in an entry.
    pub(crate) zeroed: bool,
    /// The bounds the query asserts on some terms, which [`Self::load`] reads addresses by.
    bounds: FxHashMap<String, (U256, U256)>,
    /// The bounds of values that are masked, typed arguments or typed uninterpreted results.
    pub(crate) values: FxHashMap<String, (U256, U256)>,
    /// Terms that are a root term plus a bounded value: see [`Self::span`].
    spans: FxHashMap<String, (String, U256, U256)>,
    /// Exact definitions the abstract tier replaces, with their replacements.
    abstractions: Vec<(String, String)>,
    /// The uninterpreted functions the abstract tier declares, with their arities.
    abstract_functions: FxIndexMap<&'static str, usize>,
    /// Whether [`Self::load`] reads through memories merged by [`Self::ite`], as a merge of
    /// the words each one holds.
    pub(crate) read_merges: bool,
    /// The memories defined as merges, with their options.
    merges: FxHashMap<String, Vec<(String, String)>>,
    /// The words [`Self::load`] read through merges, by memory and address.
    merged_loads: FxHashMap<(String, String), String>,
}

impl Encoder {
    pub(crate) fn declare(&mut self, name: &str, sort: &'static str) -> String {
        self.constants.insert(name.to_owned(), sort);
        name.to_owned()
    }

    /// Asserts the Boolean term `term`.
    pub(crate) fn assert(&mut self, term: &str) {
        let _ = writeln!(self.assertions, "(assert {term})");
    }

    /// The symbol defined as `definition`, shared by every term with the same definition.
    pub(crate) fn define(&mut self, sort: &'static str, definition: String) -> String {
        if !definition.starts_with('(') {
            return definition;
        }
        if let Some(name) = self.definitions.get(&definition) {
            return name.clone();
        }
        let prefix = match sort {
            BOOL => "g",
            MEMORY => "m",
            STORAGE => "s",
            WORLD => "w",
            _ => "t",
        };
        let name = format!("{prefix}{}", self.definitions.len());
        self.declare(&name, sort);
        let _ = writeln!(self.assertions, "(assert (= {name} {definition}))");
        // An address a constant away from another: (bvadd a c), (bvadd c a) or (bvsub a c).
        let operands = definition
            .strip_prefix("(bvadd ")
            .map(|rest| (rest, false))
            .or_else(|| definition.strip_prefix("(bvsub ").map(|rest| (rest, true)))
            .and_then(|(rest, sub)| Some((rest.strip_suffix(')')?.split_once(' ')?, sub)))
            .filter(|((a, b), _)| !a.contains('(') && !b.contains('('));
        if let Some(((a, b), sub)) = operands {
            let offset = match (parse_word(a), parse_word(b)) {
                (None, Some(c)) => Some((a, if sub { c.wrapping_neg() } else { c })),
                (Some(c), None) if !sub => Some((b, c)),
                _ => None,
            };
            if let Some((base, c)) = offset {
                let (root, base_offset) = self.split(base);
                self.offsets.insert(name.clone(), (root, base_offset.wrapping_add(c)));
            }
            if !sub {
                self.add_spans(&name, a, b);
            }
        }
        // A mask bounds its result.
        if let Some(rest) = definition.strip_prefix("(bvand ").and_then(|r| r.strip_suffix(')'))
            && let Some((a, b)) = rest.split_once(' ')
            && let Some(mask) = parse_word(b).or_else(|| parse_word(a))
        {
            self.values.insert(name.clone(), (U256::ZERO, mask));
        }
        self.definitions.insert(definition, name.clone());
        name
    }

    /// An application of the uninterpreted function `name`, declared on first use.
    pub(crate) fn apply(
        &mut self,
        name: &str,
        arguments: &[(String, &'static str)],
        result: &'static str,
    ) -> String {
        let sorts: Vec<_> = arguments.iter().map(|&(_, sort)| sort).collect();
        let name = format!("{name}_{}", sorts.len());
        self.functions.entry(name.clone()).or_insert((sorts, result));
        if result == WORLD || arguments.iter().any(|&(_, sort)| sort == WORLD) {
            self.constants.entry("world0".into()).or_insert(WORLD);
        }
        let term = if arguments.is_empty() {
            name
        } else {
            let arguments: Vec<_> = arguments.iter().map(|(term, _)| term.as_str()).collect();
            self.define(result, format!("({name} {})", arguments.join(" ")))
        };
        self.uninterpreted.insert(term.clone());
        term
    }

    /// Bounds an uninterpreted result by what `value` can hold: its integer type and its opcode's
    /// result width. Defined terms keep the width their definition gives, so a rewrite that
    /// breaks a type's invariant still shows.
    fn bound(&mut self, term: &str, func: &Function, value: ValueId) {
        let mut bits = func.value_ty(value).and_then(|ty| ty.integer_bits()).unwrap_or(256);
        if let Some(kind) = func.value_inst_kind(value) {
            if kind.op().result_kind() == ResultKind::I1 {
                bits = 1;
            } else if let Some(definition) = kind.evm_opcode().and_then(op::definition) {
                bits = bits.min(u32::from(definition.result_bits));
            }
        }
        if bits < 256 && self.uninterpreted.contains(term) {
            self.axioms.insert("result-type");
            let _ =
                writeln!(self.assertions, "(assert (bvule {term} {}))", word(integer_mask(bits)));
            self.values.insert(term.to_owned(), (U256::ZERO, integer_mask(bits)));
        }
    }

    /// Records the span of `sum` = `a` + `b` when one of them is a bounded value: the other's root
    /// with the bounds added.
    fn add_spans(&mut self, sum: &str, a: &str, b: &str) {
        let (a, b) = (self.span(a), self.span(b));
        let ((root, lo, hi), (_, value_lo, value_hi)) = match (a.0.is_empty(), b.0.is_empty()) {
            (_, true) => (a, b),
            (true, false) => (b, a),
            (false, false) => return,
        };
        if let (Some(lo), Some(hi)) = (lo.checked_add(value_lo), hi.checked_add(value_hi)) {
            self.spans.insert(sum.to_owned(), (root, lo, hi));
        }
    }

    /// A term as a root term plus an offset within bounds that do not wrap: the root is empty
    /// for a bounded value.
    fn span(&self, term: &str) -> (String, U256, U256) {
        if let Some(constant) = parse_word(term) {
            return (String::new(), constant, constant);
        }
        if let Some(span) = self.spans.get(term) {
            return span.clone();
        }
        if let Some(&(lo, hi)) = self.values.get(term) {
            return (String::new(), lo, hi);
        }
        (term.to_owned(), U256::ZERO, U256::ZERO)
    }

    /// Whether a `size`-byte write at `write` misses the word at `address` by their spans: from one
    /// root, the distance from the word to the write lies within `[32, 2^256 - size]` modulo
    /// 2^256 for every offset in the bounds.
    fn apart_by_spans(&self, write: &str, size: u64, address: &str) -> bool {
        let ((write_root, write_lo, write_hi), (root, lo, hi)) =
            (self.span(write), self.span(address));
        if write_root != root {
            return false;
        }
        let size = U256::from(size);
        let word = U256::from(32);
        if write_lo >= hi {
            write_lo - hi >= word && write_hi - lo <= U256::MAX - (size - U256::ONE)
        } else if lo >= write_hi {
            lo - write_hi >= size && hi - write_lo <= U256::MAX - (word - U256::ONE)
        } else {
            false
        }
    }

    /// The 32-byte word at `address` in `memory`, read through the writes it is built from that
    /// miss it: a write at the same address gives its value, and one at a constant distance from
    /// it of at least its size below and a word above is skipped.
    pub(crate) fn load(&mut self, memory: &str, address: &str) -> String {
        let (root, offset) = self.split(address);
        let mut memory = memory.to_owned();
        while let Some(write) = self.writes.get(&memory) {
            if write.address == address && write.size == Some(32) {
                return write.value.clone();
            }
            let (write_root, write_offset) = self.split(&write.address);
            let ranges = (self.range(&write_root, write_offset), self.range(&root, offset));
            let apart = match (write.size, ranges) {
                (Some(size), _) if write_root == root => {
                    let (after, before) =
                        (write_offset.wrapping_sub(offset), offset.wrapping_sub(write_offset));
                    after >= U256::from(32) && before >= U256::from(size)
                }
                // Roots apart by their bounds: the write ends below the word or starts above it.
                (Some(size), (Some((write_lo, write_hi)), Some((lo, hi)))) => {
                    write_hi + U256::from(size) <= lo || hi + U256::from(32) <= write_lo
                }
                // A region from its address up, which memory cannot hold past 2^256: the word
                // ends below it.
                (None, (Some(_), Some(_))) if write_root == root => {
                    offset + U256::from(32) <= write_offset
                }
                (None, (Some((write_lo, _)), Some((_, hi)))) => hi + U256::from(32) <= write_lo,
                _ => false,
            } || write
                .size
                .is_some_and(|size| self.apart_by_spans(&write.address, size, address));
            if !apart {
                break;
            }
            memory = write.previous.clone();
        }
        if self.zeroed && memory == "mem0" {
            return word(U256::ZERO);
        }
        if let Some(options) = self.merges.get(&memory).cloned() {
            let key = (memory, address.to_owned());
            if let Some(loaded) = self.merged_loads.get(&key) {
                return loaded.clone();
            }
            let options: Vec<_> = options
                .into_iter()
                .map(|(guard, memory)| (guard, self.load(&memory, address)))
                .collect();
            let loaded = self.ite(WORD, options);
            self.merged_loads.insert(key, loaded.clone());
            return loaded;
        }
        self.define(WORD, load_word(&memory, address))
    }

    /// `memory` with the low `size` bytes of `value`, 32 or 1, written big-endian at `address`.
    pub(crate) fn store(&mut self, memory: &str, address: &str, value: &str, size: u64) -> String {
        let definition = if size == 32 {
            store_word(memory, address, value)
        } else {
            format!("(store {memory} {address} ((_ extract 7 0) {value}))")
        };
        let name = self.define(MEMORY, definition);
        let write = MemoryWrite {
            previous: memory.to_owned(),
            address: address.to_owned(),
            size: Some(size),
            value: value.to_owned(),
        };
        self.writes.entry(name.clone()).or_insert(write);
        name
    }

    /// The addresses `root` plus `offset` may take, by the bounds the query asserts on the root,
    /// when they and the 32 bytes after them cannot wrap.
    fn range(&self, root: &str, offset: U256) -> Option<(U256, U256)> {
        let (lo, hi) =
            if root.is_empty() { (U256::ZERO, U256::ZERO) } else { *self.bounds.get(root)? };
        let lo = lo.checked_add(offset)?;
        let hi = hi.checked_add(offset)?;
        hi.checked_add(U256::from(64))?;
        Some((lo, hi))
    }

    /// An access of `size` bytes at `address` when `guard` holds ends below 2^64: an execution
    /// that touches memory past it runs out of gas first, and the validator compares executions
    /// that end.
    pub(crate) fn access(&mut self, guard: &str, address: &str, size: u64) {
        if parse_word(address).is_some_and(|address| address <= U256::from(u64::MAX - size)) {
            return;
        }
        self.axioms.insert("memory-gas");
        let limit = word(U256::from(u64::MAX - size));
        let bound = format!("(bvule {address} {limit})");
        let _ = writeln!(self.assertions, "(assert {})", implies(guard, &bound));
    }

    /// Records that `memory` is `previous` with writes at `address` and above alone.
    pub(crate) fn region(&mut self, memory: &str, previous: String, address: &str) {
        let write =
            MemoryWrite { previous, address: address.to_owned(), size: None, value: String::new() };
        self.writes.entry(memory.to_owned()).or_insert(write);
    }

    /// An address as a root term and a constant offset from it, through additions and
    /// subtractions of constants: the root is empty for a constant.
    fn split(&self, address: &str) -> (String, U256) {
        if let Some(constant) = parse_word(address) {
            return (String::new(), constant);
        }
        self.offsets.get(address).cloned().unwrap_or_else(|| (address.to_owned(), U256::ZERO))
    }

    /// The free-memory pointer on entry, which lies above the reserved words like the heap: the
    /// memory model's invariant every entry establishes and every function keeps.
    pub(crate) fn entry_fmp(&mut self) -> String {
        let pointer = self.load("mem0", &fmp_slot());
        if self.axioms.insert("free-memory-pointer") {
            let (lo, hi) = (word(U256::from(EvmMemoryLayout::HEAP_START)), word(integer_mask(66)));
            let _ = writeln!(
                self.assertions,
                "(assert (and (bvuge {pointer} {lo}) (bvule {pointer} {hi})))"
            );
            self.bounds.insert(pointer.clone(), heap_bounds());
        }
        pointer
    }

    /// Runs one version from `start` and returns its outcomes.
    fn run(&mut self, (func, funcs): Version<'_>, start: &State) -> Result<Vec<Outcome>, String> {
        let cfg = CfgInfo::new(func);
        let rpo = cfg.rpo();
        let order: FxHashMap<BlockId, usize> =
            rpo.iter().enumerate().map(|(index, &block)| (block, index)).collect();
        for &block in rpo {
            if cfg.successors(block).iter().any(|succ| order[succ] <= order[&block]) {
                return Err("loop".into());
            }
        }
        let mut run = Run {
            func,
            funcs,
            values: FxHashMap::default(),
            ends: FxHashMap::default(),
            outcomes: Vec::new(),
        };
        for &block in rpo {
            self.block(&mut run, &cfg, block, start)?;
        }
        Ok(run.outcomes)
    }

    fn block(
        &mut self,
        run: &mut Run<'_>,
        cfg: &CfgInfo,
        block: BlockId,
        start: &State,
    ) -> Result<(), String> {
        let func = run.func;
        // The edges into the block, each with the condition that it is the one taken.
        let mut incoming = Vec::new();
        for &pred in cfg.predecessors(block) {
            if incoming.iter().any(|&(other, _)| other == pred) {
                continue;
            }
            let Some((guard, _)) = run.ends.get(&pred) else { continue };
            let guard = guard.clone();
            let condition = self.edge(run, pred, block)?;
            let taken = self.define(BOOL, and(&guard, &condition));
            incoming.push((pred, taken));
        }
        let (mut guard, mut state) = if block == BlockId::ENTRY {
            ("true".to_owned(), start.clone())
        } else {
            let taken: Vec<_> = incoming.iter().map(|(_, taken)| taken.as_str()).collect();
            let guard = match taken[..] {
                [single] => single.to_owned(),
                _ => self.define(BOOL, any(&taken)),
            };
            let states: Vec<_> = incoming
                .iter()
                .map(|(pred, taken)| (taken.clone(), run.ends[pred].1.clone()))
                .collect();
            let merge = |encoder: &mut Self, sort: &'static str, field: fn(&State) -> &String| {
                let options: Vec<_> = states
                    .iter()
                    .map(|(taken, state)| (taken.clone(), field(state).clone()))
                    .collect();
                encoder.ite(sort, options)
            };
            let state = State {
                memory: merge(self, MEMORY, |state| &state.memory),
                storage: merge(self, STORAGE, |state| &state.storage),
                transient: merge(self, STORAGE, |state| &state.transient),
                world: merge(self, WORLD, |state| &state.world),
            };
            (guard, state)
        };
        for &inst in &func.blocks[block].instructions {
            if let InstKind::Phi(arms) = &func.inst(inst).kind {
                let mut options = Vec::new();
                for (pred, taken) in &incoming {
                    let Some(&(_, value)) = arms.iter().find(|(from, _)| from == pred) else {
                        return Err("phi without an incoming value".into());
                    };
                    options.push((taken.clone(), run.term(value)?));
                }
                let value = func.inst_result_value(inst).ok_or("phi without a result")?;
                let term = self.ite(WORD, options);
                run.values.insert(value, term);
                continue;
            }
            self.inst(run, inst, &mut state, &mut guard)?;
        }
        let kind = match &func.blocks[block].terminator {
            Some(Terminator::Jump(_) | Terminator::Branch { .. } | Terminator::Switch { .. }) => {
                run.ends.insert(block, (guard, state));
                return Ok(());
            }
            Some(Terminator::Return { values }) => {
                Kind::Return(values.iter().map(|&value| run.term(value)).collect::<Result<_, _>>()?)
            }
            Some(Terminator::ReturnData { offset, size }) => {
                Kind::ReturnData { offset: run.term(*offset)?, size: run.term(*size)? }
            }
            Some(Terminator::Revert { offset, size }) => {
                Kind::Revert { offset: run.term(*offset)?, size: run.term(*size)? }
            }
            Some(Terminator::RevertReturndata) => Kind::RevertReturndata,
            // Stopping returns no bytes, like `return(0, 0)`.
            Some(Terminator::Stop) => {
                Kind::ReturnData { offset: word(U256::ZERO), size: word(U256::ZERO) }
            }
            Some(Terminator::SelfDestruct { recipient }) => {
                Kind::SelfDestruct(run.term(*recipient)?)
            }
            Some(Terminator::Invalid) | None => Kind::Invalid,
            Some(Terminator::TailCall { function, args }) => {
                let args = args.iter().map(|&arg| run.term(arg)).collect::<Result<Vec<_>, _>>()?;
                let callee = format!("icall_{}", sanitize(&run.funcs[*function].name.to_string()));
                self.event(&callee, args, &mut state);
                Kind::Ended
            }
        };
        run.outcomes.push(Outcome { guard, kind, state });
        Ok(())
    }

    /// The condition that `pred`'s terminator branches to `target`.
    pub(crate) fn edge(
        &mut self,
        run: &Run<'_>,
        pred: BlockId,
        target: BlockId,
    ) -> Result<String, String> {
        let zero = word(U256::ZERO);
        Ok(match &run.func.blocks[pred].terminator {
            Some(Terminator::Branch { condition, then_block, else_block })
                if then_block != else_block =>
            {
                let condition = run.term(*condition)?;
                if *then_block == target {
                    format!("(not (= {condition} {zero}))")
                } else {
                    format!("(= {condition} {zero})")
                }
            }
            Some(Terminator::Switch { value, default, cases }) => {
                let scrutinee = run.term(*value)?;
                let mut taken = Vec::new();
                let mut unmatched = Vec::new();
                for &(case, case_target) in cases {
                    let case = run.term(case)?;
                    if case_target == target {
                        taken.push(format!("(= {scrutinee} {case})"));
                    }
                    unmatched.push(format!("(not (= {scrutinee} {case}))"));
                }
                if *default == target {
                    taken.push(all(&unmatched));
                }
                any(&taken)
            }
            _ => "true".into(),
        })
    }

    /// `(ite c1 v1 (ite c2 v2 ... vn))` over the options, the last one the default.
    pub(crate) fn ite(&mut self, sort: &'static str, options: Vec<(String, String)>) -> String {
        let Some((_, last)) = options.last() else { return "false".into() };
        if options.iter().all(|(_, value)| value == last) {
            return last.clone();
        }
        let mut term = last.clone();
        for (condition, value) in options.iter().rev().skip(1) {
            term = format!("(ite {condition} {value} {term})");
        }
        let term = self.define(sort, term);
        if sort == MEMORY && self.read_merges {
            self.merges.entry(term.clone()).or_insert(options);
        }
        term
    }

    /// An event of the world: `name` applied to it, `operands` and the state, after which the
    /// state is a function of the new world.
    pub(crate) fn event(&mut self, name: &str, operands: Vec<String>, state: &mut State) -> String {
        let mut arguments = vec![(state.world.clone(), WORLD)];
        arguments.extend(operands.into_iter().map(|operand| (operand, WORD)));
        arguments.push((state.memory.clone(), MEMORY));
        arguments.push((state.storage.clone(), STORAGE));
        arguments.push((state.transient.clone(), STORAGE));
        let world = self.apply(name, &arguments, WORLD);
        let after = [(world.clone(), WORLD)];
        state.memory = self.apply("memory_after", &after, MEMORY);
        state.storage = self.apply("storage_after", &after, STORAGE);
        state.transient = self.apply("transient_after", &after, STORAGE);
        state.world = world.clone();
        world
    }

    pub(crate) fn inst(
        &mut self,
        run: &mut Run<'_>,
        inst: InstId,
        state: &mut State,
        guard: &mut String,
    ) -> Result<(), String> {
        let func = run.func;
        let kind = &func.inst(inst).kind;
        let result = func.inst_result_value(inst);
        let operands = |run: &Run<'_>| {
            kind.operands().iter().map(|&v| run.term(v)).collect::<Result<Vec<_>, _>>()
        };
        let term = match *kind {
            InstKind::Assume { .. } => return Ok(()),
            InstKind::Load(ptr, location) => {
                let ptr = run.term(ptr)?;
                match location {
                    PtrLocation::Memory => {
                        self.access(guard, &ptr, 32);
                        self.load(&state.memory, &ptr)
                    }
                    PtrLocation::Storage => {
                        self.define(WORD, format!("(select {} {ptr})", state.storage))
                    }
                    PtrLocation::Transient => {
                        self.define(WORD, format!("(select {} {ptr})", state.transient))
                    }
                    PtrLocation::Calldata => self.apply("input_calldataload", &[(ptr, WORD)], WORD),
                    _ => return Err(format!("load from {location:?}")),
                }
            }
            InstKind::Store(ptr, value, location) => {
                let (ptr, value) = (run.term(ptr)?, run.term(value)?);
                match location {
                    PtrLocation::Memory => {
                        self.access(guard, &ptr, 32);
                        state.memory = self.store(&state.memory, &ptr, &value, 32);
                    }
                    PtrLocation::Storage => {
                        state.storage = self
                            .define(STORAGE, format!("(store {} {ptr} {value})", state.storage));
                    }
                    PtrLocation::Transient => {
                        state.transient = self
                            .define(STORAGE, format!("(store {} {ptr} {value})", state.transient));
                    }
                    _ => return Err(format!("store to {location:?}")),
                }
                return Ok(());
            }
            InstKind::Store8(ptr, value) => {
                let (ptr, value) = (run.term(ptr)?, run.term(value)?);
                self.access(guard, &ptr, 1);
                state.memory = self.store(&state.memory, &ptr, &value, 1);
                return Ok(());
            }
            InstKind::HeapFloor => {
                // The transaction's heap floor, one for both versions.
                let floor = self.declare("heap_floor", WORD);
                if self.axioms.insert("heap-floor") {
                    let _ = writeln!(
                        self.assertions,
                        "(assert (and (bvuge {floor} {}) (bvule {floor} {})))",
                        word(U256::from(EvmMemoryLayout::HEAP_START)),
                        word(integer_mask(66))
                    );
                }
                floor
            }
            InstKind::Keccak256(offset, size) => {
                let (offset, size) = (run.term(offset)?, run.term(size)?);
                self.keccak(&state.memory, offset, size)
            }
            InstKind::ICall { function: Callee::Function(callee), .. } => {
                let name = format!("icall_{}", sanitize(&run.funcs[callee].name.to_string()));
                let world = self.event(&name, operands(run)?, state);
                // The callee may end the execution, as one more outcome.
                let ended = self.apply("ended", &[(world.clone(), WORLD)], BOOL);
                let outcome = self.define(BOOL, and(guard, &ended));
                run.outcomes.push(Outcome {
                    guard: outcome,
                    kind: Kind::Ended,
                    state: state.clone(),
                });
                *guard = self.define(BOOL, and(guard, &format!("(not {ended})")));
                match result {
                    Some(_) => self.apply("result", &[(world, WORLD)], WORD),
                    None => return Ok(()),
                }
            }
            InstKind::Fmp => self.load(&state.memory, &fmp_slot()),
            InstKind::PtrAdd(ptr, offset) => {
                let (ptr, offset) = (run.term(ptr)?, run.term(offset)?);
                self.define(WORD, format!("(bvadd {ptr} {offset})"))
            }
            InstKind::MemoryZero(offset, size) => {
                let offset = run.term(offset)?;
                self.zero(state, &offset, func.value_u256(size), size, run)?;
                return Ok(());
            }
            InstKind::MappingSlot(key, slot) => {
                // The hash of the key's word and the slot's, as lowering lays them out.
                let (key, slot) = (run.term(key)?, run.term(slot)?);
                self.apply("keccak", &[(key, WORD), (slot, WORD)], WORD)
            }
            InstKind::Alloc { size, semantics } => {
                if func.inst(inst).metadata.deferred_alloc() {
                    self.deferred_alloc(run, inst, size, state)?
                } else {
                    self.alloc(run, size, semantics, state, guard)?
                }
            }
            _ => match kind.semantics() {
                Some(semantics) => self.semantics(run, kind, result, semantics, state)?,
                None => return Err(format!("`{}` is not modeled", kind.mnemonic())),
            },
        };
        if let Some(value) = result {
            if term.is_empty() {
                return Ok(());
            }
            self.bound(&term, func, value);
            run.values.insert(value, term);
        }
        Ok(())
    }

    /// The hash of `size` bytes of `memory` at `offset`: a function of the words for a constant
    /// size of up to eight words, and otherwise of memory, offset and size.
    pub(crate) fn keccak(&mut self, memory: &str, offset: String, size: String) -> String {
        match parse_word(&size) {
            Some(bytes) if bytes % U256::from(32) == U256::ZERO && bytes <= U256::from(256) => {
                let words: Vec<_> = (0..bytes.to::<u64>() / 32)
                    .map(|index| {
                        let address = format!("(bvadd {offset} {})", word(U256::from(index * 32)));
                        (self.load(memory, &address), WORD)
                    })
                    .collect();
                self.apply("keccak", &words, WORD)
            }
            _ => {
                let arguments = [(memory.to_owned(), MEMORY), (offset, WORD), (size, WORD)];
                self.apply("keccak_memory", &arguments, WORD)
            }
        }
    }

    /// Zeroes `size` bytes of memory at `offset`: word by word for a constant size of up to eight
    /// words, and otherwise by an uninterpreted function of memory, offset and size.
    fn zero(
        &mut self,
        state: &mut State,
        offset: &str,
        constant: Option<U256>,
        size: ValueId,
        run: &Run<'_>,
    ) -> Result<(), String> {
        let zero = word(U256::ZERO);
        match constant {
            Some(size) if size % U256::from(32) == U256::ZERO && size <= U256::from(256) => {
                for index in 0..size.to::<u64>() / 32 {
                    let address = format!("(bvadd {offset} {})", word(U256::from(index * 32)));
                    state.memory = self.store(&state.memory, &address, &zero, 32);
                }
            }
            _ => {
                let size = run.term(size)?;
                let arguments =
                    [(state.memory.clone(), MEMORY), (offset.to_owned(), WORD), (size, WORD)];
                let memory = state.memory.clone();
                state.memory = self.apply("zero_fill", &arguments, MEMORY);
                self.region(&state.memory, memory, offset);
            }
        }
        Ok(())
    }

    /// An allocation the backend places: at an address of its own above the reserved words and
    /// apart from every other deferred allocation, as `prove_analyses` takes it, named by the
    /// instruction so both versions agree on it. It writes the free-memory pointer alone.
    fn deferred_alloc(
        &mut self,
        run: &Run<'_>,
        inst: InstId,
        size: ValueId,
        state: &mut State,
    ) -> Result<String, String> {
        let Some(bytes) = run.func.value_u256(size) else {
            return Err("deferred allocation of a runtime size".into());
        };
        let symbol = format!("alloc{}", inst.index());
        if !self.constants.contains_key(&symbol) {
            self.declare(&symbol, WORD);
            self.axioms.insert("allocation");
            let (lo, hi) = (word(U256::from(EvmMemoryLayout::HEAP_START)), word(integer_mask(66)));
            let _ = writeln!(
                self.assertions,
                "(assert (and (bvuge {symbol} {lo}) (bvule {symbol} {hi})))"
            );
            self.bounds.insert(symbol.clone(), heap_bounds());
            for (other, other_size) in self.allocations.clone() {
                let _ = writeln!(
                    self.assertions,
                    "(assert (or (bvule (bvadd {symbol} {}) {other}) (bvule (bvadd {other} {}) {symbol})))",
                    word(bytes),
                    word(other_size)
                );
            }
            self.allocations.push((symbol.clone(), bytes));
        }
        let pointer = self.load(&state.memory, &fmp_slot());
        let bumped = self.apply("deferred_fmp", &[(symbol.clone(), WORD), (pointer, WORD)], WORD);
        state.memory = self.store(&state.memory, &fmp_slot(), &bumped, 32);
        Ok(symbol)
    }

    /// An allocation that bumps the free-memory pointer: it returns the pointer and stores the
    /// pointer plus the size, after a checked one panics with `Panic(0x41)` when the new end
    /// wraps or passes the allocation limit, and a zeroed one zeroes the range.
    fn alloc(
        &mut self,
        run: &mut Run<'_>,
        size: ValueId,
        semantics: AllocationSemantics,
        state: &mut State,
        guard: &mut String,
    ) -> Result<String, String> {
        let pointer = self.load(&state.memory, &fmp_slot());
        let bytes = run.term(size)?;
        let next = self.define(WORD, format!("(bvadd {pointer} {bytes})"));
        if semantics.failure == AllocationFailure::Checked {
            let limit = word(U256::from(EvmMemoryLayout::MAX_ALLOCATION_END));
            let invalid =
                self.define(BOOL, format!("(or (bvult {next} {pointer}) (bvugt {next} {limit}))"));
            let failed = self.define(BOOL, and(guard, &invalid));
            let mut panicked = state.clone();
            let (offset, size) = self.panic(&mut panicked, 0x41);
            run.outcomes.push(Outcome {
                guard: failed,
                kind: Kind::Revert { offset, size },
                state: panicked,
            });
            *guard = self.define(BOOL, and(guard, &format!("(not {invalid})")));
        }
        state.memory = self.store(&state.memory, &fmp_slot(), &next, 32);
        if semantics.initialization == AllocationInitialization::Zeroed {
            self.zero(state, &pointer, run.func.value_u256(size), size, run)?;
        }
        Ok(pointer)
    }

    /// Writes `Panic(code)` to the scratch words and returns the revert data's offset and size.
    fn panic(&mut self, state: &mut State, code: u64) -> (String, String) {
        let selector = word(U256::from(0x4e487b71u64) << 224);
        let zero = word(U256::ZERO);
        state.memory = self.store(&state.memory, &zero, &selector, 32);
        let code = word(U256::from(code));
        state.memory = self.store(&state.memory, &word(U256::from(4)), &code, 32);
        (zero, word(U256::from(36)))
    }

    /// The term of an instruction by its declared semantics, or an empty string for one without a
    /// result.
    fn semantics(
        &mut self,
        run: &Run<'_>,
        kind: &InstKind,
        result: Option<ValueId>,
        semantics: Semantics<'_>,
        state: &mut State,
    ) -> Result<String, String> {
        Ok(match semantics {
            Semantics::Opcode(mnemonic, values) => {
                let operands =
                    values.iter().map(|&v| run.term(v)).collect::<Result<Vec<_>, _>>()?;
                let opcode = kind.evm_opcode();
                if let Some(opcode) = opcode
                    && op::is_pure(opcode)
                {
                    // An operation on a narrow integer wraps at its width until `lower-integers`
                    // materializes the cleanup; a signed one reads its sign bit, which word
                    // semantics do not.
                    let narrow = |value: ValueId| {
                        run.func
                            .value_ty(value)
                            .and_then(|ty| ty.integer_bits())
                            .filter(|&bits| bits < 256)
                    };
                    if matches!(opcode, op::SDIV | op::SMOD | op::SLT | op::SGT | op::SAR)
                        && values.iter().any(|&value| narrow(value).is_some())
                    {
                        return Err(format!("signed `{mnemonic}` on a narrow integer"));
                    }
                    let term = self.pure(opcode, mnemonic, &operands);
                    return Ok(match result.and_then(narrow) {
                        Some(bits) => self.low_bits(&term, bits),
                        None => term,
                    });
                }
                let destination = match copy_destination(kind) {
                    Some(destination) => Some(run.term(destination)?),
                    None => None,
                };
                self.effect(mnemonic, opcode, kind.effect_kind(), operands, destination, state)?
            }
            Semantics::Word(operand) => run.term(operand)?,
            Semantics::LowBits(operand, bits) if (1..=256).contains(&bits) => {
                let operand = run.term(operand)?;
                self.low_bits(&operand, bits)
            }
            Semantics::SignExtend(operand, from, to) if from >= 1 && from <= to && to <= 256 => {
                let operand = run.term(operand)?;
                self.sign_extend(&operand, from, to)
            }
            Semantics::NotEqual(a, b) => {
                let (a, b) = (run.term(a)?, run.term(b)?);
                self.not_equal(&a, &b)
            }
            Semantics::Select(condition, a, b) => {
                let (condition, a, b) = (run.term(condition)?, run.term(a)?, run.term(b)?);
                self.select(&condition, &a, &b)
            }
            _ => return Err(format!("`{}` is not modeled", kind.mnemonic())),
        })
    }

    /// An opcode that is not pure, by its effect: a transaction input is a function of its
    /// operands, a read a function of the world and its operands, a write to memory, a log, a call
    /// or a creation an update of the state. `destination` is where a copy writes. Returns the
    /// result, or an empty string for an operation without one.
    pub(crate) fn effect(
        &mut self,
        mnemonic: &str,
        opcode: Option<u8>,
        effect: EffectKind,
        operands: Vec<String>,
        destination: Option<String>,
        state: &mut State,
    ) -> Result<String, String> {
        let words = |operands: &[String]| -> Vec<(String, &'static str)> {
            operands.iter().map(|operand| (operand.clone(), WORD)).collect()
        };
        if opcode.is_some_and(op::is_tx_input) {
            return Ok(self.apply(&format!("input_{mnemonic}"), &words(&operands), WORD));
        }
        Ok(match effect {
            EffectKind::EnvironmentRead | EffectKind::MemoryRead | EffectKind::Pure => {
                let mut arguments = vec![(state.world.clone(), WORLD)];
                arguments.extend(words(&operands));
                self.apply(&format!("env_{mnemonic}"), &arguments, WORD)
            }
            EffectKind::MemoryWrite => {
                let mut arguments =
                    vec![(state.world.clone(), WORLD), (state.memory.clone(), MEMORY)];
                arguments.extend(words(&operands));
                let memory = state.memory.clone();
                state.memory = self.apply(&format!("copy_{mnemonic}"), &arguments, MEMORY);
                if let Some(destination) = destination {
                    self.region(&state.memory, memory, &destination);
                }
                String::new()
            }
            EffectKind::Log => {
                let mut arguments = vec![(state.world.clone(), WORLD)];
                arguments.extend(words(&operands));
                arguments.push((state.memory.clone(), MEMORY));
                state.world = self.apply(&format!("ev_{mnemonic}"), &arguments, WORLD);
                String::new()
            }
            EffectKind::ExternalCall | EffectKind::Create => {
                let world = self.event(&format!("ev_{mnemonic}"), operands, state);
                self.apply("result", &[(world, WORLD)], WORD)
            }
            other => return Err(format!("`{mnemonic}` with effect {other:?}")),
        })
    }

    /// A pure opcode: exact by `smt_opcode`, and `exp` and `clz`, which it leaves out, exact
    /// where cheap and otherwise uninterpreted.
    pub(crate) fn pure(&mut self, opcode: u8, mnemonic: &str, operands: &[String]) -> String {
        // The operands of a commutative opcode in one order, so that both orders are one term.
        let mut sorted;
        let operands =
            if op::is_commutative(opcode) && operands.len() == 2 && operands[1] < operands[0] {
                sorted = operands.to_vec();
                sorted.swap(0, 1);
                &sorted[..]
            } else {
                operands
            };
        let constants: Option<Vec<U256>> = operands.iter().map(|o| parse_word(o)).collect();
        if let Some(constants) = &constants
            && let Some(value) = op::word::eval_opcode(opcode, constants)
        {
            return word(value);
        }
        let refs: Vec<_> = operands.iter().map(String::as_str).collect();
        if let Some(exact) = op::word::smt_opcode(opcode, &refs) {
            let symbol = self.define(WORD, exact.clone());
            if let Some((abstracted, function)) =
                op::word::smt_opcode_abstract(opcode, &refs, &symbol)
            {
                self.abstractions.push((format!("(= {symbol} {exact})"), abstracted));
                self.abstract_functions.insert(function, refs.len());
            }
            return symbol;
        }
        if opcode == op::EXP
            && let [base, exponent] = operands
            && let Some(exact) = exp(base, exponent)
        {
            return self.define(WORD, exact);
        }
        let arguments: Vec<_> = operands.iter().map(|o| (o.clone(), WORD)).collect();
        self.apply(&format!("op_{mnemonic}"), &arguments, WORD)
    }

    /// The low `bits` bits of the word `term`, which is how a value of a narrow integer type is
    /// kept clean.
    pub(crate) fn low_bits(&mut self, term: &str, bits: u32) -> String {
        if bits >= 256 {
            return term.to_owned();
        }
        self.define(WORD, format!("(bvand {term} {})", word(integer_mask(bits))))
    }

    /// The low `from` bits of the word `term`, sign-extended to `to` bits.
    pub(crate) fn sign_extend(&mut self, term: &str, from: u32, to: u32) -> String {
        let extended =
            format!("((_ sign_extend {}) ((_ extract {} 0) {term}))", 256 - from, from - 1);
        self.define(WORD, format!("(bvand {extended} {})", word(integer_mask(to))))
    }

    /// One when the words `a` and `b` differ, zero otherwise.
    pub(crate) fn not_equal(&mut self, a: &str, b: &str) -> String {
        let (zero, one) = (word(U256::ZERO), word(U256::ONE));
        self.define(WORD, format!("(ite (= {a} {b}) {zero} {one})"))
    }

    /// `a` when the word `condition` is nonzero, otherwise `b`.
    pub(crate) fn select(&mut self, condition: &str, a: &str, b: &str) -> String {
        self.define(WORD, format!("(ite (= {condition} {}) {b} {a})", word(U256::ZERO)))
    }

    /// Whether outcomes `a` and `b` differ in an observable, as a Boolean term.
    pub(crate) fn differ(&mut self, a: &Outcome, b: &Outcome) -> String {
        let (x, y) = (&a.state, &b.state);
        let mut parts = Vec::new();
        fn neq(left: &String, right: &String, parts: &mut Vec<String>) {
            if left != right {
                parts.push(format!("(not (= {left} {right}))"));
            }
        }
        // Storage, transient storage and the world, which a successful end commits.
        let committed = |parts: &mut Vec<String>| {
            neq(&x.storage, &y.storage, parts);
            neq(&x.transient, &y.transient, parts);
            neq(&x.world, &y.world, parts);
        };
        match (&a.kind, &b.kind) {
            (Kind::Return(left), Kind::Return(right)) if left.len() == right.len() => {
                for (left, right) in left.iter().zip(right) {
                    neq(left, right, &mut parts);
                }
                committed(&mut parts);
                // The caller sees the free-memory pointer and the memory it had allocated on entry,
                // not the scratch words or memory above the pointer, where the objects the
                // function allocated and left lie.
                if x.memory != y.memory {
                    let index = self.declare("index", WORD);
                    let entry = self.entry_fmp();
                    let (x_fmp, y_fmp) =
                        (self.load(&x.memory, &fmp_slot()), self.load(&y.memory, &fmp_slot()));
                    parts.push(format!("(not (= {x_fmp} {y_fmp}))"));
                    parts.push(format!(
                        "(and (bvuge {index} {}) (bvult {index} {entry}) (not (= (select {} {index}) (select {} {index}))))",
                        word(U256::from(EvmMemoryLayout::FMP_SLOT)),
                        x.memory,
                        y.memory
                    ));
                }
            }
            (
                Kind::ReturnData { offset: left_offset, size: left_size },
                Kind::ReturnData { offset: right_offset, size: right_size },
            ) => {
                parts.extend(self.data((left_offset, left_size, x), (right_offset, right_size, y)));
                committed(&mut parts);
            }
            (
                Kind::Revert { offset: left_offset, size: left_size },
                Kind::Revert { offset: right_offset, size: right_size },
            ) => {
                parts.extend(self.data((left_offset, left_size, x), (right_offset, right_size, y)))
            }
            (Kind::RevertReturndata, Kind::RevertReturndata) | (Kind::Ended, Kind::Ended) => {
                neq(&x.world, &y.world, &mut parts);
            }
            (Kind::SelfDestruct(left), Kind::SelfDestruct(right)) => {
                neq(left, right, &mut parts);
                committed(&mut parts);
            }
            (Kind::Invalid, Kind::Invalid) => {}
            (
                Kind::Continue { target: left, stack: left_stack },
                Kind::Continue { target: right, stack: right_stack },
            ) if left_stack.len() == right_stack.len() => {
                neq(left, right, &mut parts);
                for (left, right) in left_stack.iter().zip(right_stack) {
                    neq(left, right, &mut parts);
                }
                // The code that runs next may read any of memory.
                neq(&x.memory, &y.memory, &mut parts);
                committed(&mut parts);
            }
            _ => return "true".into(),
        }
        any(&parts)
    }

    /// Whether two byte ranges of memory differ: in size, or at some index below it.
    pub(crate) fn data(
        &mut self,
        (left_offset, left_size, x): (&String, &String, &State),
        (right_offset, right_size, y): (&String, &String, &State),
    ) -> Option<String> {
        if left_offset == right_offset && left_size == right_size && x.memory == y.memory {
            return None;
        }
        let index = self.declare("index", WORD);
        Some(format!(
            "(or (not (= {left_size} {right_size})) (and (bvult {index} {left_size}) (not (= (select {} (bvadd {left_offset} {index})) (select {} (bvadd {right_offset} {index}))))))",
            x.memory, y.memory
        ))
    }

    pub(crate) fn finish(self) -> Query {
        let abstracted = (!self.abstractions.is_empty()).then(|| {
            let mut assertions = self.assertions.clone();
            for (exact, abstracted) in &self.abstractions {
                assertions = assertions.replace(exact.as_str(), abstracted);
            }
            let mut functions = String::new();
            for (name, &arity) in &self.abstract_functions {
                let operands = vec![WORD; arity].join(" ");
                let _ = writeln!(functions, "(declare-fun {name} ({operands}) {WORD})");
            }
            self.render(&functions, &assertions)
        });
        Query {
            text: self.render("", &self.assertions),
            abstracted,
            opaque: self
                .functions
                .keys()
                .filter(|name| !name.starts_with("input_"))
                .cloned()
                .collect(),
            axioms: self.axioms.iter().map(|&axiom| axiom.to_owned()).collect(),
        }
    }

    /// The query with `functions` declared and `assertions` asserted.
    fn render(&self, functions: &str, assertions: &str) -> String {
        let mut out = String::from("(set-logic QF_AUFBV)\n");
        out.push_str(functions);
        // A world is a word: a query relates finitely many worlds, which an interpretation can
        // keep apart in 2^256 values, so the encoding stays in QF_AUFBV and stays sound.
        let render = |sort: &'static str| if sort == WORLD { WORD } else { sort };
        for (name, (parameters, result)) in &self.functions {
            let parameters: Vec<_> = parameters.iter().map(|&sort| render(sort)).collect();
            let _ =
                writeln!(out, "(declare-fun {name} ({}) {})", parameters.join(" "), render(result));
        }
        for (name, &sort) in &self.constants {
            let _ = writeln!(out, "(declare-const {name} {})", render(sort));
        }
        out.push_str(assertions);
        let mut inputs: Vec<_> = self
            .constants
            .iter()
            .filter(|&(name, &sort)| sort == WORD && name.starts_with("arg"))
            .map(|(name, _)| name.as_str())
            .collect();
        if inputs.is_empty() {
            inputs.push("true");
        }
        let _ = writeln!(out, "(check-sat)\n(get-value ({}))", inputs.join(" "));
        out
    }
}

/// A write that defines a memory term.
struct MemoryWrite {
    previous: String,
    address: String,
    /// Its size, or none for a region from the address up of a size the term leaves open.
    size: Option<u64>,
    /// The word a 32-byte write stores.
    value: String,
}

/// One version's execution under construction.
pub(crate) struct Run<'a> {
    pub(crate) func: &'a Function,
    pub(crate) funcs: &'a IndexVec<FunctionId, Function>,
    pub(crate) values: FxHashMap<ValueId, String>,
    /// Each finished block's guard at its end and its state there.
    pub(crate) ends: FxHashMap<BlockId, (String, State)>,
    pub(crate) outcomes: Vec<Outcome>,
}

impl Run<'_> {
    pub(crate) fn term(&self, value: ValueId) -> Result<String, String> {
        if let Some(constant) = self.func.value_u256(value) {
            return Ok(word(constant));
        }
        match *self.func.value(value) {
            Value::Arg(arg) => Ok(arg_name(arg)),
            Value::Inst(_) => self
                .values
                .get(&value)
                .cloned()
                .ok_or_else(|| "value used before its definition".into()),
            Value::Undef(_) => Err("`undef`".into()),
            _ => Err("a constant that is not a word".into()),
        }
    }
}

/// The memory operand a copy writes to.
pub(crate) fn copy_destination(kind: &InstKind) -> Option<ValueId> {
    match *kind {
        InstKind::Copy(destination, ..)
        | InstKind::CodeCopy(destination, ..)
        | InstKind::ExtCodeCopy(_, destination, ..) => Some(destination),
        _ => None,
    }
}

/// The disjunction of Boolean terms, written for solvers that want two operands or more.
pub(crate) fn any(terms: &[impl AsRef<str>]) -> String {
    match terms {
        [] => "false".into(),
        [term] => term.as_ref().to_owned(),
        _ => format!("(or {})", terms.iter().map(AsRef::as_ref).collect::<Vec<_>>().join(" ")),
    }
}

/// The conjunction of Boolean terms.
pub(crate) fn all(terms: &[impl AsRef<str>]) -> String {
    let terms: Vec<_> = terms.iter().map(AsRef::as_ref).filter(|&term| term != "true").collect();
    match &terms[..] {
        [] => "true".into(),
        [term] => (*term).to_owned(),
        _ => format!("(and {})", terms.join(" ")),
    }
}

/// The implication of `b` by `a`.
fn implies(a: &str, b: &str) -> String {
    if a == "true" { b.to_owned() } else { format!("(=> {a} {b})") }
}

/// The conjunction of two Boolean terms.
pub(crate) fn and(a: &str, b: &str) -> String {
    match (a, b) {
        ("true", other) | (other, "true") => other.to_owned(),
        _ => format!("(and {a} {b})"),
    }
}

/// The bounds of a heap pointer: above the reserved words, and below the memory a transaction can
/// pay for.
fn heap_bounds() -> (U256, U256) {
    (U256::from(EvmMemoryLayout::HEAP_START), integer_mask(66))
}

/// The address of the free-memory pointer.
pub(crate) fn fmp_slot() -> String {
    word(U256::from(EvmMemoryLayout::FMP_SLOT))
}

pub(crate) fn arg_name(arg: ArgIdx) -> String {
    format!("arg{}", arg.index())
}

/// The word a term written by [`word`] holds.
pub(crate) fn parse_word(term: &str) -> Option<U256> {
    let hex = term.strip_prefix("#x")?;
    U256::from_str_radix(hex, 16).ok()
}

/// `base ** exponent` for a constant base that is zero or a power of two, or a constant exponent
/// up to 8, by shifts and multiplications.
fn exp(base: &str, exponent: &str) -> Option<String> {
    let (zero, one) = (word(U256::ZERO), word(U256::ONE));
    if let Some(base) = parse_word(base) {
        if base.is_zero() {
            return Some(format!("(ite (= {exponent} {zero}) {one} {zero})"));
        }
        if base == U256::ONE {
            return Some(one);
        }
        if base.is_power_of_two() {
            // (2^k)^e = 2^(k e), zero once k e reaches 256.
            let k = base.trailing_zeros() as u64;
            let limit = word(U256::from(256u64.div_ceil(k)));
            let shift = format!("(bvmul {exponent} {})", word(U256::from(k)));
            return Some(format!("(ite (bvuge {exponent} {limit}) {zero} (bvshl {one} {shift}))"));
        }
        return None;
    }
    let exponent = parse_word(exponent)?;
    if exponent > U256::from(8) {
        return None;
    }
    let mut term = one;
    for _ in 0..exponent.to::<u64>() {
        term = format!("(bvmul {term} {base})");
    }
    Some(term)
}

#[cfg(test)]
mod tests {
    use super::*;
    use solar_smt::certify::lean::word_term;

    /// The words the agreement theorems bind, as the encoder names its operands.
    const DECLARATIONS: &str = "(declare-const a (_ BitVec 256))
(declare-const b (_ BitVec 256))
(declare-const c (_ BitVec 256))
(declare-const e (_ BitVec 256))
(declare-const t (_ BitVec 256))
(declare-fun umul ((_ BitVec 256) (_ BitVec 256)) (_ BitVec 256))
(declare-fun udiv ((_ BitVec 256) (_ BitVec 256)) (_ BitVec 256))
(declare-fun umod ((_ BitVec 256) (_ BitVec 256)) (_ BitVec 256))
(declare-fun addmod3 ((_ BitVec 256) (_ BitVec 256) (_ BitVec 256)) (_ BitVec 256))
(declare-fun mulmod3 ((_ BitVec 256) (_ BitVec 256) (_ BitVec 256)) (_ BitVec 256))";

    /// The pure opcodes the encoder models exactly, with their arities.
    const WORD_OPCODES: &[(u8, &str, usize)] = &[
        (op::ADD, "add", 2),
        (op::MUL, "mul", 2),
        (op::SUB, "sub", 2),
        (op::DIV, "div", 2),
        (op::SDIV, "sdiv", 2),
        (op::MOD, "mod", 2),
        (op::SMOD, "smod", 2),
        (op::ADDMOD, "addmod", 3),
        (op::MULMOD, "mulmod", 3),
        (op::SIGNEXTEND, "signextend", 2),
        (op::LT, "lt", 2),
        (op::GT, "gt", 2),
        (op::SLT, "slt", 2),
        (op::SGT, "sgt", 2),
        (op::EQ, "eq", 2),
        (op::ISZERO, "iszero", 1),
        (op::AND, "and", 2),
        (op::OR, "or", 2),
        (op::XOR, "xor", 2),
        (op::NOT, "not", 1),
        (op::BYTE, "byte", 2),
        (op::SHL, "shl", 2),
        (op::SHR, "shr", 2),
        (op::SAR, "sar", 2),
    ];

    /// MIR operations with an opcode, by name: the opcode, whether their result is an `i1`,
    /// whether they read a sign bit, and their Lean definition over the operands `x` and `y`.
    const MIR_OPCODES: &[(&str, u8, bool, bool, &str)] = &[
        ("add", op::ADD, false, false, "MIR.eval .add x y"),
        ("sub", op::SUB, false, false, "MIR.eval .sub x y"),
        ("mul", op::MUL, false, false, "MIR.eval .mul x y"),
        ("and", op::AND, false, false, "MIR.eval .and x y"),
        ("or", op::OR, false, false, "MIR.eval .or x y"),
        ("xor", op::XOR, false, false, "MIR.eval .xor x y"),
        ("div", op::DIV, false, false, "MIR.eval .div x y"),
        ("mod", op::MOD, false, false, "MIR.eval .mod x y"),
        ("sdiv", op::SDIV, false, true, "MIR.eval .sdiv x y"),
        ("smod", op::SMOD, false, true, "MIR.eval .smod x y"),
        ("shl", op::SHL, false, false, "MIR.eval .shl x y"),
        ("shr", op::SHR, false, false, "MIR.eval .shr x y"),
        ("sar", op::SAR, false, true, "MIR.eval .sar x y"),
        ("not", op::NOT, false, false, "MIR.not x"),
        ("lt", op::LT, true, false, "MIR.compare .lt x y"),
        ("gt", op::GT, true, false, "MIR.compare .gt x y"),
        ("slt", op::SLT, true, true, "MIR.compare .slt x y"),
        ("sgt", op::SGT, true, true, "MIR.compare .sgt x y"),
        ("eq", op::EQ, true, false, "MIR.compare .eq x y"),
    ];

    const WIDTHS: [u32; 3] = [1, 160, 256];

    /// `term` with every symbol the encoder defined replaced by its definition.
    fn expand(encoder: &Encoder, term: &str) -> String {
        let names: FxHashMap<&str, &str> =
            encoder.definitions.iter().map(|(term, name)| (name.as_str(), term.as_str())).collect();
        let mut out = String::new();
        let mut token = String::new();
        let flush = |token: &mut String, out: &mut String| {
            match names.get(token.as_str()) {
                Some(definition) => out.push_str(&expand(encoder, definition)),
                None => out.push_str(token),
            }
            token.clear();
        };
        for ch in term.chars() {
            if matches!(ch, '(' | ')' | ' ') {
                flush(&mut token, &mut out);
                out.push(ch);
            } else {
                token.push(ch);
            }
        }
        flush(&mut token, &mut out);
        out
    }

    /// `term` of an encoder in Lean.
    fn lean(encoder: &Encoder, term: &str) -> String {
        word_term(DECLARATIONS, &expand(encoder, term)).unwrap()
    }

    fn operands(arity: usize) -> Vec<String> {
        ["a", "b", "c"][..arity].iter().map(|name| (*name).to_owned()).collect()
    }

    fn idents(names: &[String]) -> String {
        names
            .iter()
            .map(|name| solar_smt::certify::lean::ident(name).unwrap())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Binders for MIR operands of `widths`, named `x`, `y` and `z`, the encoder's words `a`,
    /// `b` and `c` for them, and hypotheses that each word is its operand zero-extended.
    fn mir_binders(widths: &[u32]) -> String {
        let mut out = String::new();
        let names = ["x", "y", "z"];
        for (name, width) in names.iter().zip(widths) {
            let _ = write!(out, "({name} : BitVec {width}) ");
        }
        let words = operands(widths.len());
        let _ = write!(out, "({} : Word)", idents(&words));
        for (name, word) in names.iter().zip(&words) {
            let ident = solar_smt::certify::lean::ident(word).unwrap();
            let _ = write!(out, "\n    (h{word} : {ident} = {name}.setWidth 256)");
        }
        out
    }

    /// The Lean theorems that the terms [`Encoder`] gives each pure opcode and MIR operation
    /// equal their Lean semantics, regenerated with `SNAPSHOTS=overwrite`.
    #[test]
    fn lean_agreement() {
        let mut out = String::from(
            "import SolarTV.Agreement.Tactic

/-! Agreement of the pass validation encoder with the Lean semantics.

Generated by the `lean_agreement` test in
`crates/codegen/src/mir/transform/validate_passes.rs`: do not edit by hand. Each term is
the one the encoder (`Encoder::pure`, `low_bits`, `sign_extend`, `not_equal` and
`select`) gives an operation on symbolic operands, translated to Lean by the certifier's
`word_term`, so a mistranslation of either fails these proofs.

- `word_X` and `step_X`: the term of each pure opcode the encoder models exactly is its
  `EvmRules.Evm` definition, and so the result `SolarTV.EVM.step` pushes.
- `exp_X`: the constant shapes of `EXP` the encoder computes exactly.
- `abstract_X`: the abstract tier's laws hold of the exact operation, so replacing it
  with an uninterpreted function constrained by them only allows more models.
- `mir_X_iN`: the term of each MIR operation on `iN` operands, which the encoder keeps
  as zero-extended words and masks to their width, is the zero-extended result of its
  `SolarTV.MIR` definition. Signed operations read the sign bit of a word, so the
  encoder rejects them on narrow integers and they appear at `i256` only.
-/

namespace SolarTV.Agreement

open EvmRules

set_option linter.unusedSimpArgs false

",
        );

        for &(opcode, mnemonic, arity) in WORD_OPCODES {
            let mut encoder = Encoder::default();
            let names = operands(arity);
            let term = encoder.pure(opcode, mnemonic, &names);
            let term = lean(&encoder, &term);
            let args = idents(&names);
            let stack: Vec<_> = args.split(' ').collect();
            let _ = writeln!(
                out,
                "theorem word_{mnemonic} ({args} : Word) :
    {term} =
      Evm.{mnemonic} {args} := by
  agree_word

theorem step_{mnemonic} (ctx : EVM.Context) ({args} : Word) (rest : EVM.Stack) :
    EVM.step ctx .{mnemonic} ({} :: rest) =
      some ({term} :: rest) := by
  rw [word_{mnemonic}]; rfl
",
                stack.join(" :: ")
            );
        }

        // The constant shapes of EXP: a base of zero, one or a power of two, and small exponents.
        for base in [0u64, 1, 2, 16, 256] {
            let mut encoder = Encoder::default();
            let term = encoder.pure(op::EXP, "exp", &[word(U256::from(base)), "e".into()]);
            let term = lean(&encoder, &term);
            let _ = writeln!(
                out,
                "theorem exp_base_{base} («$e» : Word) :
    {term} =
      Evm.exp {base}#256 «$e» := by
  agree_exp
"
            );
        }
        for exponent in [0u64, 1, 2, 8] {
            let mut encoder = Encoder::default();
            let term = encoder.pure(op::EXP, "exp", &["a".into(), word(U256::from(exponent))]);
            let term = lean(&encoder, &term);
            let _ = writeln!(
                out,
                "theorem exp_power_{exponent} («$a» : Word) :
    {term} =
      Evm.exp «$a» {exponent}#256 := by
  agree_exp
"
            );
        }

        // The abstract tier's laws, with the exact value for the symbol and the exact operation
        // for the uninterpreted function.
        for (opcode, mnemonic, arity) in [
            (op::MUL, "mul", 2),
            (op::DIV, "div", 2),
            (op::MOD, "mod", 2),
            (op::ADDMOD, "addmod", 3),
            (op::MULMOD, "mulmod", 3),
        ] {
            let names = operands(arity);
            let refs: Vec<_> = names.iter().map(String::as_str).collect();
            let (laws, function) = op::word::smt_opcode_abstract(opcode, &refs, "t").unwrap();
            let laws = word_term(DECLARATIONS, &laws).unwrap();
            let args = idents(&names);
            let function = solar_smt::certify::lean::ident(function).unwrap();
            let binders = vec!["Word"; arity + 1].join(" → ");
            let _ = writeln!(
                out,
                "theorem abstract_{mnemonic} ({args} «$t» : Word) ({function} : {binders})
    (hf : ∀ {}, {function} {} = Evm.{mnemonic} {})
    (ht : «$t» = Evm.{mnemonic} {args}) :
    {laws} := by
  agree_abstract
",
                ["p", "q", "r"][..arity].join(" "),
                ["p", "q", "r"][..arity].join(" "),
                ["p", "q", "r"][..arity].join(" "),
            );
        }

        for width in WIDTHS {
            for &(name, opcode, boolean, signed, definition) in MIR_OPCODES {
                if signed && width < 256 {
                    continue;
                }
                let arity = if opcode == op::NOT { 1 } else { 2 };
                let mut encoder = Encoder::default();
                let term = encoder.pure(opcode, name, &operands(arity));
                let term = encoder.low_bits(&term, if boolean { 1 } else { width });
                let term = lean(&encoder, &term);
                let binders = mir_binders(&vec![width; arity]);
                let _ = writeln!(
                    out,
                    "theorem mir_{name}_i{width} {binders} :
    {term} =
      ({definition}).setWidth 256 := by
  agree_mir
"
                );
            }
            let mut encoder = Encoder::default();
            let term = encoder.not_equal("a", "b");
            let term = lean(&encoder, &term);
            let _ = writeln!(
                out,
                "theorem mir_ne_i{width} {} :
    {term} =
      (MIR.compare .ne x y).setWidth 256 := by
  agree_mir
",
                mir_binders(&[width, width])
            );
            let mut encoder = Encoder::default();
            let term = encoder.select("a", "b", "c");
            let term = lean(&encoder, &term);
            let _ = writeln!(
                out,
                "theorem mir_select_i{width} {} :
    {term} =
      (MIR.select x y z).setWidth 256 := by
  agree_mir
",
                mir_binders(&[1, width, width])
            );
        }

        // Casts: `trunc` keeps the low bits, `zext` the word and `sext` extends the sign bit.
        for from in WIDTHS {
            for to in WIDTHS {
                let mut encoder = Encoder::default();
                let (kind, term) = match from.cmp(&to) {
                    std::cmp::Ordering::Greater => ("trunc", encoder.low_bits("a", to)),
                    std::cmp::Ordering::Less => ("zext", "a".to_owned()),
                    std::cmp::Ordering::Equal => continue,
                };
                let mut casts = vec![(kind, term)];
                if from < to {
                    casts.push(("sext", encoder.sign_extend("a", from, to)));
                }
                for (kind, term) in casts {
                    let term = lean(&encoder, &term);
                    let _ = writeln!(
                        out,
                        "theorem mir_{kind}_i{from}_i{to} {} :
    {term} =
      (MIR.cast .{kind} {to} x).setWidth 256 := by
  agree_mir
",
                        mir_binders(&[from])
                    );
                }
            }
        }

        out.push_str("end SolarTV.Agreement\n");
        snapbox::assert_data_eq!(
            out,
            snapbox::file!["../../../../../solar-tv/SolarTV/Agreement/Encoder.lean"].raw()
        );
    }
}
