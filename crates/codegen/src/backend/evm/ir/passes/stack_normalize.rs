//! Bounded normalization of scheduled physical stack operations.
//!
//! Stack scheduling and later deletion passes can leave adjacent `DUP`, `SWAP`, `EXCHANGE`, and
//! `POP` operations that implement a non-minimal physical permutation. This pass splits each block
//! into maximal canonical stack-op runs of at most 24 instructions, symbolically computes the run's
//! input and output layouts, and asks the shared stack shuffler to synthesize an equivalent run.
//! Results are cached by input sequence because generated code often repeats the same shuffle.
//!
//! A replacement must be lowerable on the selected EVM version, must not raise the run's relative
//! stack peak or required entry depth, and must weakly improve encoded bytes, static gas, and
//! instruction count while strictly improving at least one. The Pareto checks prevent
//! target-specific deep stack ops from trading a regression in one objective for a win in another.
//! Instructions with custom stack effects break a run, and metadata from retained positions is
//! transferred to replacement ops.
//!
//! This is deliberately a small late machine-level normalizer, not a second MIR stack scheduler.
//! It repairs local permutations exposed after value identities are gone, then peephole cleanup
//! removes any simpler identities that become adjacent. The length bound keeps symbolic
//! resynthesis and cache keys independent of function size.

use super::EvmPass;
use crate::{
    backend::evm::{
        ir::{Instruction, Module},
        stack::{StackModel, StackOp, lowered_stack_cost, resynthesize_physical_ops},
    },
    mir::ValueId,
};
use smallvec::SmallVec;
use solar_config::EvmVersion;
use solar_data_structures::map::FxHashMap;
use solar_sema::Gcx;

const MAX_STACK_RUN_LEN: usize = 24;

pub(super) struct StackNormalize;

pub(super) struct StackDedup;

impl EvmPass for StackNormalize {
    fn name(&self) -> &'static str {
        "stack-normalize"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        let mut changed = false;
        let mut normalizer = Normalizer::default();
        for block in &mut module.blocks {
            changed |= normalizer.run(&mut block.instructions, gcx.sess.opts.evm_version);
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

struct Normalization {
    start: usize,
    end: usize,
    output: StackRun,
}

#[derive(Default)]
struct Normalizer {
    cache: NormalizationCache,
    scratch: Vec<Instruction>,
    normalizations: Vec<Normalization>,
}

impl Normalizer {
    fn run(&mut self, instructions: &mut Vec<Instruction>, evm_version: EvmVersion) -> bool {
        self.normalizations.clear();
        if !instructions.windows(2).any(|window| window.iter().all(|inst| stack_op(inst).is_some()))
        {
            return false;
        }
        let mut input = StackRun::new();
        let mut cursor = 0;
        while cursor < instructions.len() {
            let run_start = cursor;
            while cursor < instructions.len() && stack_op(&instructions[cursor]).is_some() {
                cursor += 1;
            }
            if cursor == run_start {
                cursor += 1;
                continue;
            }
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
                input.extend(instructions[start..end].iter().filter_map(stack_op));
                if input.len() >= 2
                    && let Some(output) = normalization(&input, evm_version, &mut self.cache)
                {
                    self.normalizations.push(Normalization { start, end, output });
                }
                start = end;
            }
        }
        if self.normalizations.is_empty() {
            return false;
        }

        self.scratch.clear();
        std::mem::swap(instructions, &mut self.scratch);
        instructions.reserve(self.scratch.len());
        let mut source = self.scratch.drain(..).enumerate().peekable();
        for normalization in self.normalizations.drain(..) {
            while source.peek().is_some_and(|&(index, _)| index < normalization.start) {
                instructions.push(source.next().unwrap().1);
            }
            let mut original = source.by_ref().take(normalization.end - normalization.start);
            for op in normalization.output {
                let mut replacement = Instruction::stack_op(op);
                if let Some((_, mut inst)) = original.next() {
                    replacement.metadata = std::mem::take(&mut inst.metadata);
                    replacement.metadata.stack = None;
                }
                instructions.push(replacement);
            }
            original.for_each(drop);
        }
        instructions.extend(source.map(|(_, inst)| inst));
        true
    }
}

fn normalization(
    input: &StackRun,
    evm_version: EvmVersion,
    cache: &mut NormalizationCache,
) -> Option<StackRun> {
    if let Some(output) = cache.get(input) {
        output.clone()
    } else {
        let output = compute_normalization(input, evm_version);
        cache.insert(input.clone(), output.clone());
        output
    }
}

fn compute_normalization(input: &StackRun, evm_version: EvmVersion) -> Option<StackRun> {
    let output = StackRun::from_vec(resynthesize_physical_ops(input, evm_version)?);
    let input_cost = lowered_stack_cost(input, evm_version);
    let output_cost = lowered_stack_cost(&output, evm_version);
    (output_cost.0 <= input_cost.0
        && output_cost.1 <= input_cost.1
        && output_cost.2 <= input_cost.2
        && output_cost != input_cost)
        .then_some(output)
}

fn stack_op(inst: &Instruction) -> Option<StackOp> {
    inst.has_canonical_stack_effect().then_some(())?;
    inst.as_stack_op()
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
        if end != start {
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
    let mut stack = StackModel::from_top_to_bottom(
        (0..source_depth).map(|index| Some(ValueId::from_usize(index))),
    );
    let mut next_value = source_depth;
    for (index, op) in instructions.iter().filter_map(symbolic_stack_op).enumerate() {
        match op {
            SymbolicStackOp::Push => {
                stack.push(ValueId::from_usize(next_value));
                next_value += 1;
            }
            SymbolicStackOp::Physical(StackOp::Swap(depth))
                if stack.top() == stack.peek(usize::from(depth)) =>
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
    if !inst.has_canonical_stack_effect() {
        return None;
    }
    if inst.is_encoded_push() {
        return Some(SymbolicStackOp::Push);
    }
    inst.as_stack_op().map(SymbolicStackOp::Physical)
}
