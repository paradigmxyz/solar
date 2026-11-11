//! Run-time checks of the facts optimizations relied on.
//!
//! This follows John Regehr's "Checking Up on Dataflow Analyses": every fact an optimization
//! relied on becomes a run-time assertion, so executing the program on any input, a UI test's
//! calls, a Foundry suite or a fuzzer's corpus, tests the analysis that proved it. A failed
//! assertion names the exact fact that is false, instead of a miscompile that has to be traced
//! back to it.
//!
//! With `-Zcheck-analyses`, on by default in debug builds of the compiler like `-Zvalidate-ir`,
//! a pass that rewrites code because an analysis proved a fact records that fact where it acts,
//! as `assume <predicate> <source>, lhs, rhs` (see `solar_mir`'s `assume` module):
//!
//! - SCCP, every constant it substitutes for a value;
//! - check elimination, every branch it folds, check it drops, equality it propagates, operand a
//!   reduced branch condition drops, mask integer cleanup removes and pair of words it fuses into
//!   one test, and for every checked operation it makes unchecked other than `pow`, that the
//!   failure test lowering would emit is false;
//! - CSE and load PRE, every memory, storage or transient read they forward, which they keep as the
//!   evidence;
//! - induction variable simplification, the bound on entry that lets it skip clamping a loop's exit
//!   bound.
//!
//! No pass reads an `assume`, so a wrong fact cannot prove its own record, and the recording pass
//! leaves the record's operands out of its own rewrite, so they keep the values the program
//! computes. A later algebraic rewrite of a record is sound by construction, and a later
//! analysis-based one records its own fact.
//!
//! This pass runs last in the lowered pipeline, after every pass that records, and turns each
//! record into a test at the end of its block, where its operands are available. A record whose
//! operands a later rewrite made constants that satisfy it is dropped. The pass is required and
//! also runs after a custom `-Zmir-pipeline` that leaves records in lowered MIR, so no `assume`
//! reaches the backend:
//!
//! ```text
//! bb:   ...; c = not predicate(lhs, rhs); jumpi c, site, next
//! site: jump trap
//! next: ...the original terminator
//! trap: fact = phi [site: FACT], ...; lhs = phi [site: lhs], ...; rhs = phi [site: rhs], ...
//!       revert AnalysisFactViolated(fact, lhs, rhs)
//! ```
//!
//! A violated fact reverts with `AnalysisFactViolated(uint256,uint256,uint256)` (selector
//! `0xed572b27`): the fact and its two operands' words. The payload stays in the scratch words a
//! panic uses, so the checks move no memory the program can observe. The fact word packs the
//! predicate (1: `eq`, 2: `ne`, 3: `ult`, 4: `ule`) in bits 104 and up, the pass that relied on
//! it (1: `sccp`, 2: `check-elim`, 3: `cse`, 4: `load-pre`, 5: `indvar-simplify`, 7: a source
//! `@custom:ensures` clause) in bits 96 to 103, then the function's index in its module, the block
//! and the left operand, as MIR text
//! numbers them (an argument with bit 31 set), in 32-bit fields, so
//! `-Zcheck-analyses=true -Zdump=mir` shows the claim next to the code it was made about.
//!
//! The checks change the program's gas and size, never its other behavior: a fact that holds
//! passes its test. Facts are tested where their block ends rather than where they were used, so
//! a block that ends early, in a call that reverts, skips its tests. Facts behind dead store
//! elimination, storage promotion and a `pow` that cannot overflow are not recorded, nor is an
//! analysis result only cost decisions read. A record keeps the values it is about alive, so a
//! checked build can optimize slightly differently from an unchecked one; a record costs nothing
//! in the cost model, which keeps that difference small.
//!
//! Source specifications are records too (see the function lowering's `spec` module): a
//! `@custom:ensures` clause is a `spec_ensures` record at each return, tested and proved like any
//! fact. A `@custom:requires` clause is a `spec_requires` record at function entry, a premise no
//! one proved: it is neither tested nor proved, and no query assumes it, since every
//! `spec_ensures` record already carries the function's premises as the left side of an
//! implication. This pass drops premises with the pure computations only they read, so none
//! reaches the backend.

use crate::mir::{
    AssumePredicate, BlockId, EffectKind, FactSource, Function, FunctionBuilder, FunctionId,
    InstId, InstKind, Instruction, InstructionMetadata, MirPhase, MirType, Module, Terminator,
    Value, ValueId,
    pass::{MirPass, ModuleAnalyses},
    transform::utils::redirect_successor_predecessors,
    utils::rebuild_predecessors,
};
use alloy_primitives::U256;
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec, map::FxHashMap};
use solar_evmir::debug::DebugLocKind;
use solar_interface::Session;

/// The left-aligned selector of `AnalysisFactViolated(uint256,uint256,uint256)`.
const FACT_VIOLATED_SELECTOR: U256 = U256::from_limbs([0, 0, 0, 0xed57_2b27_u64 << 32]);

/// Turns every recorded fact into a run-time check; see the module docs.
pub(crate) struct LowerAssumes;

impl MirPass for LowerAssumes {
    fn name(&self) -> &'static str {
        "lower-assumes"
    }

    // Every `assume` becomes a check here, so none reaches the backend.
    fn is_required(&self) -> bool {
        true
    }

    fn is_enabled(&self, _gcx: solar_sema::Gcx<'_>, module: &Module) -> bool {
        module.phase() == MirPhase::Lowered && has_records(module)
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        _analyses: &mut ModuleAnalyses,
    ) -> bool {
        let mut changed = false;
        let module_name = module.name.to_string();
        let mut write_error = None;
        for (id, func) in module.functions.iter_mut_enumerated() {
            // Dropping records that hold by their constants changes the function too.
            let had_records = func
                .instructions()
                .any(|inst| matches!(func.inst(inst).kind, InstKind::Assume { .. }));
            let facts = take_assumed(func);
            if let Some(dir) = &gcx.sess.opts.unstable.prove_analyses
                && write_error.is_none()
                && let Err(error) =
                    super::prove_analyses::write_queries(&module_name, id, func, &facts, dir)
            {
                write_error = Some((dir, error));
            }
            if !facts.is_empty() {
                instrument(func, id, &facts);
            }
            changed |= had_records;
        }
        if let Some((dir, error)) = write_error {
            gcx.dcx()
                .err(format!("cannot write the analysis queries to `{}`: {error}", dir.display()))
                .emit();
        }
        changed
    }
}

/// Returns whether passes record the facts they rely on for run-time checks: `-Zcheck-analyses`,
/// on by default in debug builds like `-Zvalidate-ir`.
pub fn should_check_analyses(sess: &Session) -> bool {
    sess.opts.unstable.check_analyses.unwrap_or(cfg!(debug_assertions))
        || sess.opts.unstable.prove_analyses.is_some()
}

/// Records that `predicate(lhs, rhs)` held at the end of `block` when `source` relied on it,
/// for [`LowerAssumes`] to test. Facts about values that are not integers are not recorded.
pub(super) fn record_fact(
    func: &mut Function,
    block: BlockId,
    source: FactSource,
    predicate: AssumePredicate,
    lhs: ValueId,
    rhs: ValueId,
) {
    if [lhs, rhs]
        .iter()
        .any(|&value| func.value_ty(value).and_then(MirType::integer_bits).is_none())
    {
        return;
    }
    // assume predicate source, lhs, rhs
    let kind = InstKind::Assume { predicate, source, lhs, rhs };
    let inst = func.alloc_inst(Instruction::new(kind, None).with_debug_loc_dropped());
    func.blocks[block].instructions.push(inst);
}

/// Records a source specification clause `condition`, an `i1`, at the end of `block`: a premise
/// for [`FactSource::SpecRequires`], an obligation for [`FactSource::SpecEnsures`].
pub(crate) fn record_spec(
    func: &mut Function,
    block: BlockId,
    source: FactSource,
    condition: ValueId,
    zero: ValueId,
) {
    record_fact(func, block, source, AssumePredicate::Ne, condition, zero);
}

/// Whether `module` holds a recorded fact, which only [`LowerAssumes`] lowers.
pub(crate) fn has_records(module: &Module) -> bool {
    module.functions.iter().any(|func| {
        func.instructions().any(|id| matches!(func.inst(id).kind, InstKind::Assume { .. }))
    })
}

/// Whether `kind` reads memory, storage or transient storage, a read a pass forwards from an
/// earlier value on the strength of an alias analysis.
pub(super) fn is_forwardable_read(kind: &InstKind) -> bool {
    matches!(
        kind.effect_kind(),
        EffectKind::MemoryRead | EffectKind::StorageRead | EffectKind::TransientRead
    )
}

/// The values something other than an `assume` reads. A pass that records a fact skips values
/// only records still read: rewriting them again would rewrite the record and lose its evidence.
#[derive(Debug)]
pub(super) struct NonRecordReads(DenseBitSet<ValueId>);

impl NonRecordReads {
    pub(super) fn new(func: &Function) -> Self {
        let mut read = DenseBitSet::new_empty(func.num_values());
        for block in &func.blocks {
            for &inst in &block.instructions {
                if !matches!(func.inst(inst).kind, InstKind::Assume { .. }) {
                    func.inst(inst).kind.visit_operands(|value| {
                        read.insert(value);
                    });
                }
            }
            if let Some(terminator) = &block.terminator {
                terminator.visit_operands(|value| {
                    read.insert(value);
                });
            }
        }
        Self(read)
    }

    /// Whether only records read `value`. A value allocated since [`Self::new`] counts as read.
    pub(super) fn only_records(&self, value: ValueId) -> bool {
        value.index() < self.0.domain_size() && !self.0.contains(value)
    }
}

/// Records, after a pass replaced every `(read, value, replacement)` read's value and removed its
/// other dead instructions, that each kept read equals its replacement, following replacements
/// that were themselves replaced.
pub(super) fn record_forwarded(
    func: &mut Function,
    source: FactSource,
    forwarded: &[(InstId, ValueId, ValueId)],
    replacements: &FxHashMap<ValueId, ValueId>,
) {
    if forwarded.is_empty() {
        return;
    }
    let blocks = func.inst_block_table();
    for &(read, value, mut replacement) in forwarded {
        while let Some(&next) = replacements.get(&replacement)
            && next != replacement
        {
            replacement = next;
        }
        if let Some(block) = blocks[read] {
            // assume eq source, read, replacement
            record_fact(func, block, source, AssumePredicate::Eq, value, replacement);
        }
    }
}

/// A recorded fact: `predicate(lhs, rhs)` held where `block` ends when `source` relied on it.
pub(super) struct Fact {
    pub(super) block: BlockId,
    pub(super) source: FactSource,
    pub(super) predicate: AssumePredicate,
    pub(super) lhs: ValueId,
    pub(super) rhs: ValueId,
}

impl Fact {
    /// The fact word: the predicate and the source, then the function, block and left operand
    /// as MIR text names them, an argument with bit 31 of its field set.
    fn word(
        &self,
        function: FunctionId,
        func: &Function,
        names: &IndexVec<InstId, Option<u32>>,
    ) -> U256 {
        let value = match *func.value(self.lhs) {
            Value::Inst(inst) => names[inst].unwrap_or(u32::MAX),
            Value::Arg(arg) => (1 << 31) | arg.index() as u32,
            _ => u32::MAX,
        };
        (U256::from(predicate_id(self.predicate)) << 104)
            | (U256::from(source_id(self.source)) << 96)
            | (U256::from(function.index()) << 64)
            | (U256::from(self.block.index()) << 32)
            | U256::from(value)
    }
}

/// The number the fact word gives a predicate.
const fn predicate_id(predicate: AssumePredicate) -> u8 {
    match predicate {
        AssumePredicate::Eq => 1,
        AssumePredicate::Ne => 2,
        AssumePredicate::Ult => 3,
        AssumePredicate::Ule => 4,
    }
}

/// The number the fact word gives the pass that relied on a fact.
const fn source_id(source: FactSource) -> u8 {
    match source {
        FactSource::Sccp => 1,
        FactSource::CheckElim => 2,
        FactSource::Cse => 3,
        FactSource::LoadPre => 4,
        FactSource::IndvarSimplify => 5,
        FactSource::SpecRequires => 6,
        FactSource::SpecEnsures => 7,
    }
}

/// Whether `predicate(lhs, rhs)` holds.
fn holds(predicate: AssumePredicate, lhs: U256, rhs: U256) -> bool {
    match predicate {
        AssumePredicate::Eq => lhs == rhs,
        AssumePredicate::Ne => lhs != rhs,
        AssumePredicate::Ult => lhs < rhs,
        AssumePredicate::Ule => lhs <= rhs,
    }
}

/// Removes every `assume` from `func`, returning the facts they record in block order, all but
/// specification premises, which are dropped with the pure computations only they read.
fn take_assumed(func: &mut Function) -> Vec<Fact> {
    let mut facts = Vec::new();
    let mut premises = Vec::new();
    for block in func.blocks.indices() {
        let mut instructions = std::mem::take(&mut func.blocks[block].instructions);
        instructions.retain(|&id| {
            let InstKind::Assume { predicate, source, lhs, rhs } = func.inst(id).kind else {
                return true;
            };
            if source == FactSource::SpecRequires {
                premises.extend([lhs, rhs]);
                return false;
            }
            // A later rewrite may have replaced both operands with constants that satisfy the
            // fact; constants that do not still fail wherever the record is reached.
            let decided = func.value_u256(lhs).zip(func.value_u256(rhs));
            if !decided.is_some_and(|(lhs, rhs)| holds(predicate, lhs, rhs)) {
                facts.push(Fact { block, source, predicate, lhs, rhs });
            }
            false
        });
        func.blocks[block].instructions = instructions;
    }
    remove_unused_pure(func, premises);
    facts
}

/// Removes the pure instructions computing `roots`, and those computing their operands, once
/// nothing reads them.
fn remove_unused_pure(func: &mut Function, roots: Vec<ValueId>) {
    if roots.is_empty() {
        return;
    }
    let mut uses: IndexVec<ValueId, u32> = IndexVec::from_vec(vec![0; func.num_values()]);
    for block in &func.blocks {
        for &inst in &block.instructions {
            func.inst(inst).kind.visit_operands(|value| uses[value] += 1);
        }
        if let Some(terminator) = &block.terminator {
            terminator.visit_operands(|value| uses[value] += 1);
        }
    }
    let mut dead = DenseBitSet::new_empty(func.num_insts());
    let mut worklist = roots;
    while let Some(value) = worklist.pop() {
        if let Value::Inst(inst) = *func.value(value)
            && uses[value] == 0
            && !func.inst(inst).kind.has_side_effects()
            && dead.insert(inst)
        {
            func.inst(inst).kind.visit_operands(|operand| {
                uses[operand] -= 1;
                worklist.push(operand);
            });
        }
    }
    for block in func.blocks.iter_mut() {
        block.instructions.retain(|inst| !dead.contains(*inst));
    }
}

fn instrument(func: &mut Function, function: FunctionId, facts: &[Fact]) {
    // vN names the Nth instruction with a result, in allocation order, as MIR text does.
    let mut next = 0;
    let names: IndexVec<InstId, _> = (0..func.num_insts())
        .map(|index| {
            func.inst(InstId::from_usize(index)).result_ty.map(|_| {
                next += 1;
                next - 1
            })
        })
        .collect();
    let fact_words: Vec<_> = facts.iter().map(|fact| fact.word(function, func, &names)).collect();
    let mut dropped = InstructionMetadata::EMPTY;
    dropped.drop_debug_loc(DebugLocKind::Dropped);
    let mut builder = FunctionBuilder::new(func);
    builder.set_debug_context(&dropped);
    let trap = builder.create_block();
    let mut incoming = Vec::with_capacity(facts.len());
    let mut fact_words = fact_words.into_iter();
    for facts in facts.chunk_by(|a, b| a.block == b.block) {
        let block = facts[0].block;
        let (terminator, metadata) = builder.func_mut().blocks[block].take_terminator();
        let terminator = terminator.expect("instrumented block has a terminator");
        let mut current = block;
        for fact in facts {
            builder.switch_to_block(current);
            // l = zext lhs to i256; r = zext rhs to i256
            let lhs = builder.cast(fact.lhs, MirType::I256);
            let rhs = builder.cast(fact.rhs, MirType::I256);
            let violated = match fact.predicate {
                // c = ne l, r
                AssumePredicate::Eq => builder.emit_inst(InstKind::Ne(lhs, rhs), Some(MirType::I1)),
                // c = eq l, r
                AssumePredicate::Ne => builder.emit_inst(InstKind::Eq(lhs, rhs), Some(MirType::I1)),
                // c = eq (lt l, r), 0
                AssumePredicate::Ult => {
                    let holds = builder.emit_inst(InstKind::Lt(lhs, rhs), Some(MirType::I1));
                    let zero = builder.imm_bool(false);
                    builder.emit_inst(InstKind::Eq(holds, zero), Some(MirType::I1))
                }
                // c = gt l, r
                AssumePredicate::Ule => {
                    builder.emit_inst(InstKind::Gt(lhs, rhs), Some(MirType::I1))
                }
            };
            // jumpi c, site, next
            let site = builder.create_block();
            let next = builder.create_block();
            builder.func_mut().blocks[current].set_generated_terminator(Terminator::Branch {
                condition: violated,
                then_block: site,
                else_block: next,
            });
            // site: jump trap
            builder.func_mut().blocks[site].set_generated_terminator(Terminator::Jump(trap));
            builder.switch_to_block(site);
            let fact_word = builder.imm(fact_words.next().expect("one word per fact"));
            incoming.push((site, [fact_word, lhs, rhs]));
            current = next;
        }
        // next: ...the original terminator
        builder.func_mut().blocks[current].set_terminator(terminator, metadata);
        if current != block {
            redirect_successor_predecessors(builder.func_mut(), block, current);
        }
    }

    // trap: fact = phi [site: FACT], ...; lhs = phi [site: l], ...; rhs = phi [site: r], ...
    //       revert AnalysisFactViolated(fact, lhs, rhs)
    builder.switch_to_block(trap);
    let words: Vec<_> = (0..3)
        .map(|index| {
            builder.phi(incoming.iter().map(|&(site, words)| (site, words[index])).collect())
        })
        .collect();
    builder.revert_scratch(FACT_VIOLATED_SELECTOR, &words);
    rebuild_predecessors(func);
}
