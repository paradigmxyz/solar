//! Bounded normalization of scheduled physical stack operations.
//!
//! Stack scheduling and later deletion passes can leave adjacent `DUP`, `SWAP`, `EXCHANGE`, and
//! `POP` operations that implement a non-minimal physical permutation. This pass splits each block
//! into maximal canonical stack-op runs of at most 24 instructions, symbolically computes the run's
//! input and output layouts, and asks the shared stack shuffler to synthesize an equivalent run.
//! Results are cached by input sequence because generated code often repeats the same shuffle.
//! A bounded per-thread cache also retains these pure results across modules and pass
//! invocations. It stores only physical operations, never value identities or metadata,
//! and clears on an EVM-version change. Each rewrite still transfers the current
//! instructions' debug metadata at its original site.
//!
//! A replacement must be lowerable on the selected EVM version and must weakly improve encoded
//! bytes, static gas, and instruction count while strictly improving at least one. The Pareto
//! checks prevent target-specific deep stack ops from trading a regression in one objective for a
//! win in another. Any instruction other than a stack op breaks a run, and metadata from retained
//! positions is transferred to replacement ops.
//!
//! This is deliberately a small late machine-level normalizer, not a second MIR stack scheduler.
//! It repairs local permutations exposed after value identities are gone, then peephole cleanup
//! removes any simpler identities that become adjacent. The length bound keeps symbolic
//! resynthesis and cache keys independent of function size.
//!
//! A run directly before a commutative binary operation or a comparison with a mirrored form may
//! leave the top two words in either order: the alternative also normalizes the run followed by
//! `SWAP1` and wins when cheaper, and then a comparison flips to its mirrored form.
//!
//! `SWAP` is the canonical form of a two-word exchange until the last peephole has run, so
//! consumer rules such as `SWAP1 ADD -> ADD` see every top swap. The ordinary instance never
//! introduces `EXCHANGE`. The final instance runs after the last peephole, only on targets with a
//! native `EXCHANGE`, and may use it wherever it is cheaper. Besides resynthesis, it folds each
//! cycle through the top that leaves the top in place, `SWAPa SWAPb ... SWAPa`, into one
//! `EXCHANGE a, b` per inner swap when that improves the lowered cost.

use super::EvmPass;
use crate::{
    backend::evm::{
        codegen::{StackModel, StackOp, lowered_stack_cost, resynthesize_physical_ops},
        ir::{Instruction, Module},
        op,
    },
    mir::ValueId,
};
use smallvec::SmallVec;
use solar_config::{EvmVersion, OptimizationMode};
use solar_data_structures::map::FxHashMap;
use solar_sema::Gcx;
use std::cell::RefCell;

const MAX_STACK_RUN_LEN: usize = 24;

pub(super) struct StackNormalize {
    /// Whether this is the final instance, which may introduce `EXCHANGE`.
    exchanges: bool,
}

impl StackNormalize {
    pub(super) const EARLY: Self = Self { exchanges: false };
    pub(super) const FINAL: Self = Self { exchanges: true };
}

pub(super) struct StackDedup;

impl EvmPass for StackNormalize {
    fn name(&self) -> &'static str {
        if self.exchanges { "final-stack-normalize" } else { "stack-normalize" }
    }

    fn is_enabled(&self, gcx: Gcx<'_>, _module: &Module) -> bool {
        !matches!(gcx.sess.opts.optimization, OptimizationMode::None)
            && (!self.exchanges || gcx.sess.opts.evm_version.has_extended_stack_ops())
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        let mut changed = false;
        let mut normalizer = Normalizer::default();
        let key = (gcx.sess.opts.evm_version, self.exchanges);
        for block in &mut module.blocks {
            changed |= normalizer.run(&mut block.instructions, key);
        }
        changed
    }
}

impl EvmPass for StackDedup {
    fn name(&self) -> &'static str {
        "stack-dedup"
    }

    fn run_pass(&self, _gcx: Gcx<'_>, module: &mut Module) -> bool {
        let mut changed = false;
        let mut remove = Vec::new();
        for block in &mut module.blocks {
            changed |= remove_redundant_permutations(&mut block.instructions, &mut remove);
        }
        changed
    }
}

type StackRun = SmallVec<[StackOp; MAX_STACK_RUN_LEN]>;
type NormalizationCache = FxHashMap<StackRun, Option<StackRun>>;

/// Reuse the common physical shuffles without retaining unbounded compiler state.
const MAX_SHARED_NORMALIZATIONS: usize = 4096;

/// The target and whether the output may contain `EXCHANGE`.
type NormalizationKey = (EvmVersion, bool);

thread_local! {
    static SHARED_NORMALIZATIONS: RefCell<SharedNormalizations> = RefCell::default();
}

#[derive(Default)]
struct SharedNormalizations {
    evm_version: Option<EvmVersion>,
    /// Entries without and with `EXCHANGE` in the output.
    entries: [NormalizationCache; 2],
}

impl SharedNormalizations {
    fn get(&mut self, input: &StackRun, key: NormalizationKey) -> Option<StackRun> {
        let (evm_version, exchanges) = key;
        if self.evm_version != Some(evm_version) {
            self.entries.iter_mut().for_each(NormalizationCache::clear);
            self.evm_version = Some(evm_version);
        }
        let entries = &mut self.entries[usize::from(exchanges)];
        if let Some(output) = entries.get(input) {
            return output.clone();
        }
        let output = compute_normalization(input, key);
        if entries.len() == MAX_SHARED_NORMALIZATIONS {
            entries.clear();
        }
        entries.insert(input.clone(), output.clone());
        output
    }
}

struct Normalization {
    start: usize,
    end: usize,
    output: StackRun,
    /// The opcode that replaces the consumer at `end` when the output leaves the top two words
    /// exchanged.
    consumer: Option<u8>,
}

#[derive(Default)]
struct Normalizer {
    cache: NormalizationCache,
    scratch: Vec<Instruction>,
    normalizations: Vec<Normalization>,
}

impl Normalizer {
    fn run(&mut self, instructions: &mut Vec<Instruction>, key: NormalizationKey) -> bool {
        self.normalizations.clear();
        if !instructions
            .windows(2)
            .any(|window| window.iter().all(|inst| inst.as_stack_op().is_some()))
        {
            return false;
        }
        let mut input = StackRun::new();
        let mut cursor = 0;
        while cursor < instructions.len() {
            let run_start = cursor;
            while cursor < instructions.len() && instructions[cursor].as_stack_op().is_some() {
                cursor += 1;
            }
            if cursor == run_start {
                cursor += 1;
                continue;
            }
            // A consumer that reads the top two words in either order, and the opcode that
            // reads them exchanged.
            let consumer = instructions
                .get(cursor)
                .filter(|inst| !inst.keeps_with_next())
                .and_then(Instruction::as_evm_opcode)
                .and_then(op::swapped_binary_opcode);
            let mut start = run_start;
            while start < cursor {
                let remaining = cursor - start;
                let len = if remaining == MAX_STACK_RUN_LEN + 1 {
                    MAX_STACK_RUN_LEN - 1
                } else {
                    remaining.min(MAX_STACK_RUN_LEN)
                };
                let end = start + len;
                input.clear();
                input.extend(instructions[start..end].iter().filter_map(Instruction::as_stack_op));
                let consumer = consumer.filter(|_| {
                    end == cursor
                        && !instructions[start..end].iter().any(Instruction::keeps_with_next)
                });
                if input.len() >= 2 {
                    let mut best =
                        normalization(&input, key, &mut self.cache).map(|output| (output, None));
                    if let Some(consumer) = consumer {
                        // run; consumer => run; swap 1; mirrored consumer
                        let cost = lowered_stack_cost(
                            best.as_ref().map_or(&input, |(output, _)| output),
                            key.0,
                        );
                        input.push(StackOp::Swap(1));
                        if let Some(output) = normalization(&input, key, &mut self.cache)
                            && improves(lowered_stack_cost(&output, key.0), cost)
                        {
                            best = Some((output, Some(consumer)));
                        }
                    }
                    if let Some((output, consumer)) = best {
                        self.normalizations.push(Normalization { start, end, output, consumer });
                    }
                }
                start = end;
            }
        }
        if self.normalizations.is_empty() {
            return false;
        }
        for normalization in &self.normalizations {
            if let Some(opcode) = normalization.consumer
                && instructions[normalization.end].as_evm_opcode() != Some(opcode)
            {
                instructions[normalization.end]
                    .replace_preserving_metadata(Instruction::opcode(opcode));
            }
        }

        self.scratch.clear();
        std::mem::swap(instructions, &mut self.scratch);
        instructions.reserve(self.scratch.len());
        let mut source = self.scratch.drain(..).enumerate().peekable();
        for normalization in self.normalizations.drain(..) {
            while source.peek().is_some_and(|&(index, _)| index < normalization.start) {
                instructions.push(source.next().unwrap().1);
            }
            // Replacements take the replaced operations' debug information positionally; a
            // longer output repeats the last original's, a shorter one absorbs the leftovers.
            let mut original = source.by_ref().take(normalization.end - normalization.start);
            let first = instructions.len();
            for op in normalization.output {
                let mut replacement = Instruction::stack_op(op);
                match original.next() {
                    Some((_, mut inst)) => {
                        replacement.metadata = std::mem::take(&mut inst.metadata);
                    }
                    None => match instructions.last() {
                        Some(last) if instructions.len() > first => {
                            replacement.metadata.copy_source_debug_from(&last.metadata);
                        }
                        _ => replacement.metadata.mark_debug_info_dropped(),
                    },
                }
                instructions.push(replacement);
            }
            for (_, inst) in original {
                if let Some(last) = instructions.last_mut() {
                    last.metadata.absorb_debug_info(&inst.metadata);
                }
            }
        }
        instructions.extend(source.map(|(_, inst)| inst));
        true
    }
}

fn normalization(
    input: &StackRun,
    key: NormalizationKey,
    cache: &mut NormalizationCache,
) -> Option<StackRun> {
    if let Some(output) = cache.get(input) {
        output.clone()
    } else {
        let output = SHARED_NORMALIZATIONS.with_borrow_mut(|shared| shared.get(input, key));
        cache.insert(input.clone(), output.clone());
        output
    }
}

fn compute_normalization(
    input: &StackRun,
    (evm_version, exchanges): NormalizationKey,
) -> Option<StackRun> {
    let input_cost = lowered_stack_cost(input, evm_version);
    let resynthesized = resynthesize_physical_ops(input, evm_version, exchanges)
        .map(StackRun::from_vec)
        .filter(|output| improves(lowered_stack_cost(output, evm_version), input_cost));
    if !exchanges {
        return resynthesized;
    }
    // Resynthesis rebuilds a run from its permutation and can miss a cycle through the top
    // inside a longer run, so the final instance also folds such cycles in place.
    fold_exchange_cycles(resynthesized.as_ref().unwrap_or(input), evm_version).or(resynthesized)
}

/// Whether `cost` weakly improves every lowered objective of `than` and strictly improves one.
fn improves(cost: (usize, usize, usize), than: (usize, usize, usize)) -> bool {
    cost.0 <= than.0 && cost.1 <= than.1 && cost.2 <= than.2 && cost != than
}

/// Rewrites each `SWAPa SWAPb1 ... SWAPbj SWAPa`, a cycle through the top that leaves the top in
/// place, as `EXCHANGE a, b1 ... EXCHANGE a, bj` where that improves the lowered cost. Returns
/// `None` when nothing folds.
fn fold_exchange_cycles(run: &[StackOp], evm_version: EvmVersion) -> Option<StackRun> {
    let mut output = StackRun::new();
    let mut folded = false;
    let mut index = 0;
    while index < run.len() {
        // swap a; swap b1; ...; swap bj; swap a
        // => exchange a, b1; ...; exchange a, bj
        if let Some((exchanges, len)) = StackOp::exchange_cycle(run[index..].iter().copied())
            && improves(
                lowered_stack_cost(&exchanges, evm_version),
                lowered_stack_cost(&run[index..index + len], evm_version),
            )
        {
            output.extend(exchanges);
            folded = true;
            index += len;
        } else {
            output.push(run[index]);
            index += 1;
        }
    }
    folded.then_some(output)
}

fn remove_redundant_permutations(
    instructions: &mut Vec<Instruction>,
    remove: &mut Vec<usize>,
) -> bool {
    remove.clear();
    let mut start = 0;
    while start < instructions.len() {
        let mut end = start;
        while end < instructions.len() && symbolic_stack_op(&instructions[end]).is_some() {
            end += 1;
        }
        // Only a `DUP` makes two stack slots hold the same value.
        if instructions[start..end]
            .iter()
            .any(|inst| matches!(inst.as_stack_op(), Some(StackOp::Dup(_))))
        {
            find_redundant_permutations(&instructions[start..end], start, remove);
        }
        start = end + 1;
    }
    if remove.is_empty() {
        return false;
    }
    let mut index = 0;
    let mut removed = remove.iter().copied().peekable();
    instructions.retain(|_| {
        let keep = removed.peek().copied() != Some(index);
        if !keep {
            removed.next();
        }
        index += 1;
        keep
    });
    true
}

fn find_redundant_permutations(
    instructions: &[Instruction],
    offset: usize,
    remove: &mut Vec<usize>,
) {
    let mut depth = 0isize;
    let mut required = 0isize;
    for op in instructions.iter().filter_map(symbolic_stack_op) {
        match op {
            SymbolicStackOp::Push => depth += 1,
            SymbolicStackOp::Physical(op) => {
                required = required.max(op.required_depth() as isize - depth);
                depth += op.net_growth();
            }
        }
    }
    let source_depth = required as usize;
    let mut stack = StackModel::from_top_to_bottom((0..source_depth).map(ValueId::from_usize));
    let mut next_value = source_depth;
    for (index, op) in instructions.iter().filter_map(symbolic_stack_op).enumerate() {
        match op {
            SymbolicStackOp::Push => {
                stack.push(ValueId::from_usize(next_value));
                next_value += 1;
            }
            SymbolicStackOp::Physical(StackOp::Swap(depth))
                if stack.peek(0) == stack.peek(usize::from(depth)) =>
            {
                remove.push(offset + index);
            }
            SymbolicStackOp::Physical(StackOp::Exchange(first, second))
                if stack.peek(usize::from(first)) == stack.peek(usize::from(second)) =>
            {
                remove.push(offset + index);
            }
            SymbolicStackOp::Physical(op) => stack.apply(op),
        }
    }
}

#[derive(Clone, Copy)]
enum SymbolicStackOp {
    Push,
    Physical(StackOp),
}

fn symbolic_stack_op(inst: &Instruction) -> Option<SymbolicStackOp> {
    if inst.is_encoded_push() {
        return Some(SymbolicStackOp::Push);
    }
    inst.as_stack_op().map(SymbolicStackOp::Physical)
}
