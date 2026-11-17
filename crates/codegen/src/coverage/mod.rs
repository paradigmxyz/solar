//! Source coverage instrumentation, enabled with `-Zinstrument-coverage`.
//!
//! The map follows LLVM's source-based coverage model. A walk of the HIR, before any function is
//! lowered, gives every function, modifier, and Yul function a record: a list of regions of the
//! source, each with a count that is a counter or an expression over counters. The walk covers
//! functions that no bytecode contains too. Lowering then emits a `coverage` MIR instruction for a
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
//!
//! The false count of a branch is an expression, the count of the parent minus the true count, so
//! no counter sits on the edge and the code needs no extra jump. A condition that reverts takes
//! neither side.
//!
//! # Counters
//!
//! A counter's sites are:
//! - the entry of a function, modifier, or Yul function body;
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
//! expressions.
//!
//! Sites are keyed by the address of their HIR node, which lowering reaches through the same arena
//! references. A counter whose site lowering never reaches stays at zero, as for code that is
//! never generated.

use alloy_primitives::keccak256;
use solar_data_structures::{index::IndexVec, map::FxHashMap};
use solar_interface::Span;
use solar_sema::{Gcx, hir};

mod walk;

/// An execution count, given by counter hits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Count {
    /// Zero.
    Zero,
    /// The hits of one counter.
    Counter(u32),
    /// The sum of two counts.
    Add(Box<[Count; 2]>),
    /// The difference of two counts.
    Sub(Box<[Count; 2]>),
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
    /// A condition with the counts of its true and false outcomes, `4`.
    Branch(Count, Count),
}

/// The coverage regions of one function, modifier, or Yul function, or of the initializers of one
/// contract.
#[derive(Debug)]
pub struct Record {
    /// What the record belongs to.
    pub owner: RecordOwner,
    /// The virtual file table. The first entry is the source of the record.
    pub files: Vec<hir::SourceId>,
    /// The contiguous range of counters the record owns, as its first counter and its length.
    pub counters: [u32; 2],
    /// The regions, outer regions before the regions nested in them.
    pub regions: Vec<Region>,
}

/// The item a [`Record`] belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordOwner {
    /// A function, modifier, or Yul function.
    Function(hir::FunctionId),
    /// The creation-time code outside the functions of a contract: its state variable initializers
    /// and the base constructor arguments of its inheritance specifiers.
    Init(hir::ContractId),
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
                    RegionKind::Branch(true_count, false_count) => {
                        row[0] = 4;
                        row.extend([
                            encoder.count(true_count) as i64,
                            encoder.count(false_count) as i64,
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
    sites: FxHashMap<Site, u32>,
}

impl CoverageMap {
    fn new(gcx: Gcx<'_>) -> Self {
        walk::walk(gcx)
    }

    /// Returns the coverage records of `source`.
    pub fn source(&self, source: hir::SourceId) -> &SourceCoverage {
        &self.sources[source]
    }

    /// Returns the counter of `site`, if it has one.
    pub(crate) fn counter(&self, site: Site) -> Option<u32> {
        self.sites.get(&site).copied()
    }
}

/// Returns the coverage map of the compilation when `-Zinstrument-coverage` is set.
pub fn coverage_map(gcx: Gcx<'_>) -> Option<&CoverageMap> {
    gcx.sess.opts.unstable.instrument_coverage.then(|| {
        gcx.codegen_state::<crate::backend::evm::CodegenState>()
            .coverage
            .get_or_init(|| CoverageMap::new(gcx))
    })
}

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
