//! Source coverage instrumentation, enabled with `-Zinstrument-coverage`.
//!
//! The map follows LLVM's source-based coverage model. A walk of the HIR, before any function is
//! lowered, gives every function, modifier, and Yul function a record: a list of regions of the
//! source, each with a count that is a counter or an expression over counters. The record also
//! lists the virtual files of the modifiers it expands. The walk covers functions that no bytecode
//! contains too. Lowering then emits a `coverage` MIR instruction for a
//! counter wherever control reaches its site; the backend emits it as `COVERAGE; PUSHn <counter>`.
//! The instrumenting EVM reads the counter from the push and skips it, so a probe costs no gas or
//! stack, and JUMPDEST analysis still sees a whole push. Counters start at 1 and are unique within
//! the compilation, so a probe names its region without a table of program counters.
//!
//! # Regions
//!
//! Regions follow Clang's placement:
//! - a code region for each function body, each statement written in the source, each path a
//!   decision takes, and the right operand of `&&` and `||`, nested;
//! - a branch region on each leaf condition of a decision, a `switch` case, and a `try` clause;
//! - a skipped region for code that constant folding removes.
//!
//! The false count of a branch is an expression, the count of the parent minus the true count, so
//! no counter sits on the edge and the code needs no extra jump. A condition that reverts takes
//! neither side.
//!
//! # Modifiers
//!
//! Each invocation of a modifier lowers the body once more, so it has counters of its own, inside
//! the range of the function that invokes it. The function's record has an expansion region at the
//! invocation, and repeats the regions of the body under a virtual file for the expansion. The
//! record of the modifier owns no counters. Tools such as `llvm-cov` add up the regions of a span
//! across records, so the expansions carry the body and the record of a modifier that something
//! invokes has one code region, over its header from the `modifier` keyword to the `{` of the body,
//! that counts the sum of the entry counters of its expansions. A modifier that nothing invokes
//! keeps the regions of its body, counted zero, so its lines show as missed.
//!
//! # Counters
//!
//! A counter's sites are:
//! - the entry of a function, modifier expansion, or Yul function body;
//! - the start of a statement written in the source, unless the statement starts a path whose own
//!   counter immediately precedes it, such as the first statement of a body;
//! - the point after each leaf condition of a decision is evaluated;
//! - the start of the path a decision takes when its condition holds, and of the right operand of
//!   `&&` and `||`;
//! - the start of each clause of a `try` statement and of each case of a `switch`;
//! - the start of each state variable initializer and base constructor argument list that has a
//!   decision.
//!
//! Decisions are the conditions of `if`, loops, `?:`, and `require`, and any `&&` and `||` tree.
//! Lowering folds a constant expression without evaluating its parts, so the walk skips constant
//! expressions, and reports the path that a constant condition removes as skipped. A constant has
//! no branch: lowering evaluates the one path it takes, and a `&&` or `||` whose left operand is
//! constant is its right operand, or a constant that skips it.
//!
//! A constant whose initializer is no compile-time constant is evaluated each time the code uses
//! it, so its initializer counts like the initializer of a state variable.
//!
//! With `-Zcoverage-mcdc`, a decision of two or more conditions also has three kinds of MC/DC
//! probe, `COVERAGE; PUSH4 <probe>` with bit 31 of the probe set. Bits 30 and 29 are the operation,
//! `0` to begin, `1` to add, and `2` to end the decision; bits 28 to 8 are its ID, and bits 7 to 0
//! the weight an add carries. A decision begins before its first condition, adds the weight of each
//! condition that holds, and ends once its value is known, so the sum of the weights is the index
//! of the test vector the run took.
//!
//! Sites are keyed by the address of their HIR node, which lowering reaches through the same arena
//! references. A counter whose site lowering never reaches stays at zero, as for code that is
//! never generated. Each expansion of a modifier lowers the same HIR nodes, so a site also has the
//! expansion it belongs to. A virtual modifier dispatches to an override in some derived contracts,
//! so an invocation has an expansion for each modifier it can reach.
//!
//! A `require` whose message reverts while it is evaluated counts as a failed condition, since the
//! counter that ends the condition follows the message.

use crate::backend::evm::op;
use alloy_primitives::keccak256;
use solar_data_structures::{index::IndexVec, map::FxHashMap};
use solar_interface::Span;
use solar_sema::{Gcx, hir};

mod mcdc;
mod walk;

/// An execution count, given by counter hits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Count {
    /// Zero.
    Zero,
    /// The hits of one counter.
    Counter(u32),
    /// The sum of two counts.
    Add(Box<[Self; 2]>),
    /// The difference of two counts.
    Sub(Box<[Self; 2]>),
}

impl Count {
    pub(crate) fn add(self, rhs: Self) -> Self {
        match (self, rhs) {
            (Self::Zero, count) | (count, Self::Zero) => count,
            (lhs, rhs) => Self::Add(Box::new([lhs, rhs])),
        }
    }

    pub(crate) fn sub(self, rhs: Self) -> Self {
        match rhs {
            Self::Zero => self,
            rhs => Self::Sub(Box::new([self, rhs])),
        }
    }
}

/// A region of a source.
#[derive(Clone, Debug)]
pub struct Region {
    /// The index of the file of the region in [`Record::files`].
    pub file: u32,
    /// The source span.
    pub span: Span,
    /// What the region counts.
    pub kind: RegionKind,
}

/// What a region counts, numbered like LLVM's `CounterMappingRegion::RegionKind`.
#[derive(Clone, Debug)]
pub enum RegionKind {
    /// Code, `0`.
    Code(Count),
    /// The expansion of a modifier into the file at this index, `1`.
    Expansion(Count, u32),
    /// Code that constant folding removes, `2`.
    Skipped,
    /// A condition with the counts of its true and false outcomes, `4`.
    Branch(Count, Count),
    /// An MC/DC decision, given by its index in [`Record::decisions`], `5`.
    Decision(u32),
    /// A condition of an MC/DC decision, `6`.
    McdcBranch {
        /// How often the condition held.
        holds: Count,
        /// How often the condition failed.
        fails: Count,
        /// The ID of the condition in its decision, in the order of evaluation.
        condition: u32,
        /// The ID of the condition evaluated when this one fails, or `-1` if the decision ends.
        false_next: i32,
        /// The ID of the condition evaluated when this one holds, or `-1` if the decision ends.
        true_next: i32,
    },
}

/// An MC/DC decision of a record.
#[derive(Debug)]
pub struct Decision {
    /// The ID that the probes of the decision name, unique within the compilation.
    pub id: u32,
    /// The number of conditions.
    pub conditions: u32,
    /// The test vectors, in the order of their indexes. A vector has a character for each
    /// condition, `T`, `F`, or `-` when evaluation skipped it, then one for the outcome.
    pub vectors: Vec<String>,
}

/// The coverage regions of one function, modifier, or Yul function, or of the initializers of one
/// contract.
#[derive(Debug)]
pub struct Record {
    /// What the record belongs to.
    pub owner: RecordOwner,
    /// The virtual file table. The first entry is the source of the record; each modifier
    /// expansion adds one.
    pub files: Vec<hir::SourceId>,
    /// The contiguous range of counters the record owns, as its first counter and its length.
    pub counters: [u32; 2],
    /// The regions, outer regions before the regions nested in them.
    pub regions: Vec<Region>,
    /// The MC/DC decisions.
    pub decisions: Vec<Decision>,
}

/// The item a [`Record`] belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordOwner {
    /// A function, modifier, or Yul function.
    Function(hir::FunctionId),
    /// The code outside the functions of a contract that runs at creation, its state variable
    /// initializers and the base constructor arguments of its inheritance specifiers, and the
    /// initializers of its constants that run where they are used.
    Init(hir::ContractId),
    /// The initializers of the constants of a source that run where they are used and belong to no
    /// contract.
    Constants(hir::SourceId),
}

/// A [`Record`] in the numbers of the Standard JSON output.
#[derive(Debug)]
pub struct EncodedRecord {
    /// A structural hash of the counters, expressions, and regions.
    pub hash: u64,
    /// The expressions, as pairs of counter values local to the record.
    pub expressions: Vec<[u64; 2]>,
    /// The regions, as arrays of numbers: the kind, the file, the start and length in bytes, and
    /// the fields of the kind.
    pub regions: Vec<Vec<i64>>,
}

impl Record {
    /// Returns the numbers of the Standard JSON output for this record.
    pub fn encode(&self, gcx: Gcx<'_>) -> EncodedRecord {
        let starts = self
            .files
            .iter()
            .map(|&file| gcx.hir.source(file).file.start_pos.0)
            .collect::<Vec<_>>();
        let (expressions, regions) = self.encode_with(&starts, false);
        // The hash names own counters by their position in the range, so edits elsewhere in the
        // compilation, which renumber counters, leave it unchanged.
        let (hash_expressions, hash_regions) = self.encode_with(&starts, true);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.counters[1].to_le_bytes());
        for expression in &hash_expressions {
            bytes.extend(expression.iter().flat_map(|value| value.to_le_bytes()));
        }
        for region in &hash_regions {
            bytes.extend(region.iter().flat_map(|value| value.to_le_bytes()));
            bytes.push(0xff);
        }
        let hash = u64::from_be_bytes(keccak256(&bytes)[..8].try_into().unwrap());
        EncodedRecord { hash, expressions, regions }
    }

    fn encode_with(&self, starts: &[u32], relative: bool) -> (Vec<[u64; 2]>, Vec<Vec<i64>>) {
        let mut encoder = Encoder {
            first: self.counters[0],
            end: self.counters[0] + self.counters[1],
            relative,
            expressions: Vec::new(),
            interned: FxHashMap::default(),
        };
        let regions = self
            .regions
            .iter()
            .map(|region| {
                let start = region.span.lo().0 - starts[region.file as usize];
                let len = region.span.hi().0 - region.span.lo().0;
                let mut row = vec![0, i64::from(region.file), i64::from(start), i64::from(len)];
                match &region.kind {
                    RegionKind::Code(count) => row.push(encoder.count(count) as i64),
                    RegionKind::Expansion(count, file) => {
                        row[0] = 1;
                        row.extend([encoder.count(count) as i64, i64::from(*file)]);
                    }
                    RegionKind::Branch(true_count, false_count) => {
                        row[0] = 4;
                        row.extend([
                            encoder.count(true_count) as i64,
                            encoder.count(false_count) as i64,
                        ]);
                    }
                    RegionKind::Skipped => row[0] = 2,
                    RegionKind::Decision(decision) => {
                        row[0] = 5;
                        row.push(i64::from(*decision));
                    }
                    RegionKind::McdcBranch { holds, fails, condition, false_next, true_next } => {
                        row[0] = 6;
                        row.extend([
                            encoder.count(holds) as i64,
                            encoder.count(fails) as i64,
                            i64::from(*condition),
                            i64::from(*false_next),
                            i64::from(*true_next),
                        ]);
                    }
                }
                row
            })
            .collect();
        (encoder.expressions, regions)
    }
}

/// Encodes counts as tagged integers, as LLVM does: the low two bits are `0` for zero, `1` for a
/// counter, `2` for the difference of an expression, and `3` for its sum.
struct Encoder {
    first: u32,
    end: u32,
    relative: bool,
    expressions: Vec<[u64; 2]>,
    interned: FxHashMap<[u64; 2], u64>,
}

impl Encoder {
    fn count(&mut self, count: &Count) -> u64 {
        match count {
            Count::Zero => 0,
            Count::Counter(id) => {
                let id = if self.relative {
                    if (self.first..self.end).contains(id) {
                        u64::from(id - self.first)
                    } else {
                        u64::from(*id) | 1 << 40
                    }
                } else {
                    u64::from(*id)
                };
                id << 2 | 1
            }
            Count::Add(pair) => self.expression(pair) << 2 | 3,
            Count::Sub(pair) => self.expression(pair) << 2 | 2,
        }
    }

    fn expression(&mut self, [lhs, rhs]: &[Count; 2]) -> u64 {
        let pair = [self.count(lhs), self.count(rhs)];
        let next = self.expressions.len() as u64;
        let index = *self.interned.entry(pair).or_insert(next);
        if index == next {
            self.expressions.push(pair);
        }
        index
    }
}

/// The coverage records of one source.
#[derive(Debug, Default)]
pub struct SourceCoverage {
    /// The records of the functions, modifiers, and Yul functions defined in the source and of the
    /// contracts' initializers, in source order.
    pub records: Vec<Record>,
}

/// The coverage map of a compilation.
#[derive(Debug, Default)]
pub struct CoverageMap {
    sources: IndexVec<hir::SourceId, SourceCoverage>,
    sites: FxHashMap<(Scope, Site), u32>,
}

impl CoverageMap {
    fn new(gcx: Gcx<'_>) -> Self {
        walk::walk(gcx)
    }

    /// Returns the coverage records of `source`.
    pub fn source(&self, source: hir::SourceId) -> &SourceCoverage {
        &self.sources[source]
    }

    /// Returns the counter of `site` in modifier expansion `scope`, if it has one. Code outside
    /// every expansion has the default scope.
    pub(crate) fn counter(&self, scope: Scope, site: Site) -> Option<u32> {
        self.sites.get(&(scope, site)).copied()
    }
}

/// Returns the aliases of `aliases` that hold in `code`: those whose kept counter it hits.
///
/// An alias `[dropped, kept]` holds in every code object that has the probes of the kept counter,
/// since the dropped counter was always hit with it. Reading `code` as a sequence of
/// instructions can only find too many probes in data, which adds aliases whose counters nothing
/// hits.
pub(crate) fn aliases_in_code(aliases: &[[u32; 2]], code: &[u8]) -> Vec<[u32; 2]> {
    if aliases.is_empty() {
        return Vec::new();
    }
    let mut counters = solar_data_structures::map::FxHashSet::default();
    let mut index = 0;
    let mut after_coverage = false;
    while let Some(&opcode) = code.get(index) {
        index += 1;
        if (op::PUSH1..=op::PUSH32).contains(&opcode) {
            let width = usize::from(opcode - op::PUSH1) + 1;
            if after_coverage && width <= 4 {
                let counter = code.get(index..index + width).map_or(0, |bytes| {
                    bytes.iter().fold(0_u32, |counter, &byte| counter << 8 | u32::from(byte))
                });
                counters.insert(counter);
            }
            index += width;
        }
        after_coverage = opcode == op::COVERAGE;
    }
    aliases.iter().copied().filter(|[_, kept]| counters.contains(kept)).collect()
}

/// Returns the coverage map of the compilation when `-Zinstrument-coverage` is set.
pub fn coverage_map(gcx: Gcx<'_>) -> Option<&CoverageMap> {
    gcx.sess.opts.unstable.instrument_coverage.then(|| {
        gcx.codegen_state::<crate::backend::evm::CodegenState>()
            .coverage
            .get_or_init(|| CoverageMap::new(gcx))
    })
}

/// A modifier expansion: the address of the invocation and the modifier it dispatches to, which is
/// an override in some derived contracts, or the default scope for code outside every expansion.
pub(crate) type Scope = (usize, u32);

/// A place where lowering emits a counter, keyed by the address of its HIR node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Site {
    /// The entry of a function body.
    Function(hir::FunctionId),
    /// The start of a statement.
    Stmt(usize),
    /// After a leaf condition of a decision, keyed by the leaf expression.
    Leaf(usize),
    /// The start of a path: the path a decision takes when its condition holds, keyed by the `if`
    /// statement, the true arm of a `?:`, or the condition of a `require`; the right operand of
    /// `&&` or `||`; a `try` clause; or a `switch` case.
    Arm(usize),
    /// After the selector of a `switch` statement, keyed by the statement.
    Decision(usize),
    /// After the value of a decision that no branch consumes, keyed by the `&&` or `||`
    /// expression. Lowering branches on the value to count its true outcomes.
    Value(usize),
    /// The start of an initializer or base constructor argument list, keyed by the variable or the
    /// modifier node.
    Init(usize),
    /// The MC/DC probe that begins a decision, keyed by the `&&` or `||` expression that ends the
    /// unary operators of the decision. Its value is the probe, not a counter.
    McdcBegin(usize),
    /// The MC/DC probe that adds the weight of the true outcome of a leaf, keyed by the leaf.
    McdcAdd(usize),
    /// The MC/DC probe that ends a decision, keyed like [`Site::McdcBegin`].
    McdcEnd(usize),
}

impl Site {
    /// Returns the address key of `node`.
    pub(crate) fn key<T>(node: &T) -> usize {
        std::ptr::from_ref(node) as usize
    }

    /// Returns the address key of `expr`, which is the same for the expression with and without
    /// parentheses.
    pub(crate) fn expr(expr: &hir::Expr<'_>) -> usize {
        Self::key(expr.peel_parens())
    }
}
