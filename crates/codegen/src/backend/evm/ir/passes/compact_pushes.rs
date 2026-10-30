//! Target-dependent selection of compact immediate materializations.
//!
//! A literal `PUSHn` is not always the shortest way to construct a 256-bit constant. For each
//! concrete immediate, this pass compares the literal encoding with a fixed set of equivalent
//! recipes: `PUSH0; NOT` for an all-ones word, shifted left or right by any amount for a high or
//! low mask, `NOT` of a shorter inverse, and a shift of a shorter value with trailing zero bytes.
//! It then emits the recipe with the fewest encoded bytes, whatever the tune, keeping the literal
//! on ties so the pass never increases code size. In size builds and cold blocks, division of the
//! all-ones word and multiplication also synthesize repeated byte patterns. Gas-mode hot blocks
//! retain the existing shift and inverse recipes.
//! In gas builds the cost model prices a cold block like a size build,
//! [`Target::priced_for_cold`], so extra gas does not keep a literal there.
//!
//! Selection accounts for the active EVM version: `PUSH0` and shift opcodes are used only when the
//! target supports them. A gas build keeps the literal where the recipe's extra runtime gas
//! outweighs the deposit of the bytes it saves, as the cost model prices it over the expected
//! executions and, for a push in a hot loop, over the loop's iterations, so a loop does not
//! rebuild a mask on every iteration. At `-O none`, a block also counts as in
//! a loop when its function is called from a loop or by recursion. `Target::push` uses the same
//! selector, so other EVM IR passes can compare a prospective rewrite with the bytes, static gas,
//! and instructions that this pass will emit. NOTE: the size-only repeated-pattern recipes are
//! emitted here; the common cost helper conservatively retains the larger estimate for those
//! constants.
//!
//! Recipe emission recursively selects materializations for child pushes, so one pass reaches a
//! fixed point. The default pipeline expands recipes once before structural cleanup because tail
//! merging and outlining profit from the concrete instruction shape. Push reordering and
//! assembly-only lowering use the same recipe API for constants they inspect or introduce later.
//! Before increasing a literal's transient stack use, query upper entry depths from the stack
//! verifier and account for each preceding instruction. Growing cycles and dynamically entered
//! paths have unknown entry depths; there, keep the original straight-line path's peak as an upper
//! bound. A physical conditional jump ends that path, since its taken edge can skip later stack
//! growth. Keep the literal when the complete, recursively expanded recipe would overflow.

use super::EvmPass;
use crate::{
    backend::evm::{
        codegen::MAX_STACK_DEPTH,
        ir::{Instruction, Metadata, Module, verify::Verifier},
        op::{self, WORD_BYTES},
    },
    target::{Cost, Target},
};
use alloy_primitives::U256;
use arrayvec::ArrayVec;
use solar_config::EvmVersion;
use solar_sema::Gcx;

pub(super) struct CompactPushes;

impl EvmPass for CompactPushes {
    fn name(&self) -> &'static str {
        "compact-pushes"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        compact_pushes(gcx, module)
    }
}

const MIN_COMPACT_MASK_WIDTH: u8 = 5;

fn compact_pushes(gcx: Gcx<'_>, module: &mut Module) -> bool {
    let evm_version = gcx.sess.opts.evm_version;
    let mut changed = false;
    let mut scratch = Vec::new();
    let mut limits = Vec::new();
    let mut candidates = Vec::new();
    let mut entry_depths = None;
    for block_id in module.blocks.indices() {
        let block = &module.blocks[block_id];
        // A block's function may select an objective other than the contract's.
        let optimization = block.metadata.optimization.unwrap_or(module.optimization);
        let repeated = optimization.is_size() || block.metadata.hotness.is_cold();
        candidates.clear();
        candidates.extend(block.instructions.iter().enumerate().filter_map(|(at, inst)| {
            let materialization = ImmediateMaterialization::with_repeated(
                evm_version,
                inst.concrete_immediate()?,
                repeated,
            );
            (!materialization.is_literal()).then_some((at, materialization))
        }));
        if candidates.is_empty() {
            continue;
        }
        let target = Target::new(gcx, optimization)
            .priced_for_cold(block.metadata.hotness.is_cold())
            .priced_for(module.code_follows);
        // `-O none` runs no inlining, so a loop calls the helpers that hold its hot code; gas
        // builds keep the loop-only rule.
        let in_loop = (block.metadata.in_loop
            || (module.optimization.is_none() && block.metadata.repeats))
            && !block.metadata.hotness.is_cold();
        candidates.retain(|(_, materialization)| materialization.repays(&target, in_loop));
        if candidates.is_empty() {
            continue;
        }
        if entry_depths.is_none()
            && candidates.iter().any(|(_, materialization)| materialization.stack_peak() > 1)
        {
            entry_depths = Some(Verifier::stack_entry_upper_bounds(module, evm_version));
        }
        let block = &module.blocks[block_id];
        let mut stack =
            entry_depths.as_ref().and_then(|depths| depths[block_id]).unwrap_or(MAX_STACK_DEPTH);
        // The original path must fit as well. Propagate its stack limit backwards, stopping at
        // physical branches so growth on a fallthrough cannot justify growth on a taken edge.
        limits.resize(block.instructions.len(), MAX_STACK_DEPTH);
        let mut limit = MAX_STACK_DEPTH
            - block
                .terminator
                .as_ref()
                .map_or(0, |term| term.kind.lowering_stack_growth(module.next_block(block_id)));
        for (at, inst) in block.instructions.iter().enumerate().rev() {
            if inst
                .as_evm_opcode()
                .is_some_and(|opcode| opcode == op::JUMPI || op::is_terminal(opcode))
            {
                limit = MAX_STACK_DEPTH;
            }
            let effect = inst.stack_effect();
            limit = (limit + usize::from(effect.inputs))
                .saturating_sub(usize::from(effect.outputs))
                .min(MAX_STACK_DEPTH);
            limits[at] = limit;
        }
        let block = &mut module.blocks[block_id];
        scratch.clear();
        std::mem::swap(&mut block.instructions, &mut scratch);
        block.instructions.reserve(scratch.len());
        let mut pending = candidates.iter().peekable();
        for (at, inst) in scratch.drain(..).enumerate() {
            let effect = inst.stack_effect();
            let before = stack.min(limits[at]);
            stack = before.saturating_sub(usize::from(effect.inputs)) + usize::from(effect.outputs);
            if let Some((_, materialization)) = pending.next_if(|(index, _)| *index == at)
                && materialization.stack_peak() <= MAX_STACK_DEPTH - before
            {
                // push value => selected_recipe(value)
                materialize(&mut block.instructions, materialization, &inst.metadata);
                changed = true;
            } else {
                block.instructions.push(inst);
            }
        }
    }
    changed
}

/// One instruction in a selected immediate materialization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::backend) enum ImmediateMaterializationOp {
    Push(U256),
    Opcode(u8),
}

/// The shortest selected materialization for one concrete immediate.
pub(in crate::backend) struct ImmediateMaterialization {
    evm_version: EvmVersion,
    value: U256,
    repeated: bool,
    selected: Selected,
}

impl ImmediateMaterialization {
    /// Returns the materialization selected for `value` on `evm_version`.
    pub(in crate::backend) fn new(evm_version: EvmVersion, value: U256) -> Self {
        Self::with_repeated(evm_version, value, false)
    }

    fn with_repeated(evm_version: EvmVersion, value: U256, repeated: bool) -> Self {
        Self { evm_version, value, repeated, selected: select_recipe(evm_version, value, repeated) }
    }

    fn is_literal(&self) -> bool {
        self.selected.steps.is_empty()
    }

    /// Returns the materialization's maximum relative stack height.
    pub(in crate::backend) fn stack_peak(&self) -> usize {
        let (mut depth, mut peak) = (0usize, 0usize);
        self.for_each(|materialized| match materialized {
            ImmediateMaterializationOp::Push(_) => {
                depth += 1;
                peak = peak.max(depth);
            }
            ImmediateMaterializationOp::Opcode(opcode) => {
                let (inputs, outputs) =
                    op::stack_io(opcode).expect("compact immediate recipes use known EVM opcodes");
                depth = depth - usize::from(inputs) + usize::from(outputs);
            }
        });
        debug_assert_eq!(depth, 1);
        peak
    }

    /// Whether the selected recipe is better code than the literal push, by the target's pricing
    /// of the bytes it saves against the runtime gas it adds, inside a loop or not.
    fn repays(&self, target: &Target, in_loop: bool) -> bool {
        let literal = literal_cost(push_width(self.evm_version, self.value));
        let recipe = self.selected.cost;
        target.compacts_immediate(
            u64::from(literal.bytes.saturating_sub(recipe.bytes)),
            u64::from(target.runtime_gas(recipe).saturating_sub(target.runtime_gas(literal))),
            in_loop,
        )
    }

    /// Visits each concrete instruction in execution order.
    pub(in crate::backend) fn for_each(&self, mut f: impl FnMut(ImmediateMaterializationOp)) {
        self.visit(&mut f);
    }

    fn visit(&self, f: &mut impl FnMut(ImmediateMaterializationOp)) {
        if self.is_literal() {
            return f(ImmediateMaterializationOp::Push(self.value));
        }
        for step in &self.selected.steps {
            match *step {
                Step::Immediate(value) => {
                    Self::with_repeated(self.evm_version, value, self.repeated).visit(f);
                }
                Step::Opcode(opcode) => f(ImmediateMaterializationOp::Opcode(opcode)),
            }
        }
    }
}

pub(super) fn materialize_immediate(
    instructions: &mut Vec<Instruction>,
    evm_version: EvmVersion,
    value: U256,
) {
    materialize(
        instructions,
        &ImmediateMaterialization::new(evm_version, value),
        &Metadata::default(),
    );
}

/// Every replacement carries the source debug information of the push it materializes.
fn materialize(
    instructions: &mut Vec<Instruction>,
    materialization: &ImmediateMaterialization,
    source: &Metadata,
) {
    materialization.for_each(|op| {
        let replacement = match op {
            ImmediateMaterializationOp::Push(value) => Instruction::push_value(value),
            ImmediateMaterializationOp::Opcode(opcode) => Instruction::opcode(opcode),
        };
        instructions.push(replacement.with_source_debug(source));
    });
}

/// Returns the cost of the selected immediate materialization.
pub(crate) fn immediate_materialization(evm_version: EvmVersion, value: U256) -> Cost {
    select_recipe(evm_version, value, false).cost
}

/// Returns the cost of the push that [`compact_pushes`] leaves for `value` in code `target`
/// prices, inside a loop or not: the selected recipe where it repays against the literal, as
/// `Target::push` prices it, and the literal otherwise.
pub(crate) fn emitted_immediate(target: &Target, value: U256, in_loop: bool) -> Cost {
    let materialization = ImmediateMaterialization::new(target.evm_version(), value);
    if materialization.is_literal() || materialization.repays(target, in_loop) {
        materialization.selected.cost
    } else {
        literal_cost(push_width(target.evm_version(), value))
    }
}
/// Returns the cost of pushing `value` as one literal `PUSHn`.
pub(crate) fn literal_push(evm_version: EvmVersion, value: U256) -> Cost {
    literal_cost(push_width(evm_version, value))
}

/// Returns the byte length and gas cost of the selected immediate materialization.
pub(crate) fn immediate_materialization_cost(
    evm_version: EvmVersion,
    value: U256,
) -> (usize, usize) {
    let cost = immediate_materialization(evm_version, value);
    (cost.bytes as usize, cost.gas as usize)
}

pub(in crate::backend) fn immediate_materialization_len(
    evm_version: EvmVersion,
    value: U256,
) -> usize {
    immediate_materialization(evm_version, value).bytes as usize
}

/// One step of a recipe.
#[derive(Clone, Copy)]
enum Step {
    /// An immediate pushed by its own selected materialization.
    Immediate(U256),
    Opcode(u8),
}

/// The materialization selected for one concrete immediate, with its cost. Its `steps` are
/// empty for a literal push.
struct Selected {
    cost: Cost,
    steps: ArrayVec<Step, 5>,
}

fn select_recipe(evm_version: EvmVersion, value: U256, repeated: bool) -> Selected {
    let width = push_width(evm_version, value);
    let literal = Selected { cost: literal_cost(width), steps: ArrayVec::new() };
    // NOT recipes require a full-width input. A shifted nonzero literal needs
    // at least two PUSH1s and SHL (five bytes), so PUSH4 and shorter already win
    // or tie every recipe. Keep the literal on ties, as the full search does.
    if width < MIN_COMPACT_MASK_WIDTH {
        return literal;
    }
    select_compact(evm_version, value, repeated, width, literal)
}

// Not inlined, so the literal path of `select_recipe` stays small.
#[inline(never)]
fn select_compact(
    evm_version: EvmVersion,
    value: U256,
    repeated: bool,
    width: u8,
    literal: Selected,
) -> Selected {
    let mut best = literal;
    let mut consider = |steps: &[Step]| {
        let cost = steps
            .iter()
            .map(|step| match *step {
                Step::Immediate(word) => select_recipe(evm_version, word, repeated).cost,
                Step::Opcode(opcode) => Cost::fixed_opcode(opcode),
            })
            .sum::<Cost>();
        if cost.bytes < best.cost.bytes {
            best = Selected { cost, steps: steps.iter().copied().collect() };
        }
    };
    let (zero, max) = (Step::Immediate(U256::ZERO), Step::Immediate(U256::MAX));
    let [not, shr, shl, div, mul] = [op::NOT, op::SHR, op::SHL, op::DIV, op::MUL].map(Step::Opcode);

    if value == U256::MAX {
        consider(&[zero, not]);
    }

    let shift = value.leading_zeros();
    if evm_version.has_bitwise_shifting() && value == U256::MAX >> shift {
        consider(&[zero, not, Step::Immediate(U256::from(shift)), shr]);
    }

    if usize::from(width) == WORD_BYTES {
        let inverted = !value;
        if push_width(evm_version, inverted) < width {
            consider(&[Step::Immediate(inverted), not]);
        }
    }

    // Restrict divisions and multiplications to size-first code. Every child is narrower,
    // so recursive selection terminates, and strict byte savings are required.
    let narrower = |word| push_width(evm_version, word) < width;
    if repeated && value < U256::MAX {
        let divisor = U256::MAX / value;
        if (U256::MAX % value).is_zero() && narrower(divisor) {
            consider(&[Step::Immediate(divisor), max, div]);
        }
        for period in [1usize, 2, 4, 8, 16] {
            let divisor = (U256::ONE << (period * 8)) - U256::ONE;
            let ones = U256::MAX / divisor;
            if !(value % ones).is_zero() {
                continue;
            }
            let factor = value / ones;
            if factor > U256::ONE && narrower(factor) && narrower(divisor) {
                consider(&[Step::Immediate(divisor), max, div, Step::Immediate(factor), mul]);
            }
        }
    }

    let trailing_zero_bytes = value.trailing_zeros() / 8;
    if evm_version.has_bitwise_shifting() && trailing_zero_bytes > 0 {
        let shift = trailing_zero_bytes * 8;
        consider(&[Step::Immediate(value >> shift), Step::Immediate(U256::from(shift)), shl]);
    }

    // `MAX << s` for any `s`. Considered last so a tie keeps the cheaper recipes above.
    let shift = value.trailing_zeros();
    if evm_version.has_bitwise_shifting() && value == U256::MAX << shift {
        consider(&[zero, not, Step::Immediate(U256::from(shift)), shl]);
    }

    best
}

/// The cost of one literal push of `width` bytes, where `PUSH0` has width 0.
fn literal_cost(width: u8) -> Cost {
    const PUSH0: Cost = Cost::fixed_opcode(op::PUSH0);
    const PUSH1: Cost = Cost::fixed_opcode(op::PUSH1);
    Cost { bytes: 1 + u32::from(width), ..if width == 0 { PUSH0 } else { PUSH1 } }
}

fn push_width(evm_version: EvmVersion, value: U256) -> u8 {
    if value.is_zero() && !evm_version.has_push0() { 1 } else { value.byte_len() as u8 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::target::Target;
    use solar_config::OptimizationMode;

    /// `Target::push` prices the instructions the selector emits.
    #[test]
    fn selected_cost_matches_emitted_instructions() {
        let mut values = vec![U256::ZERO, U256::ONE, U256::MAX, U256::from(0x1234_5678_9abc_u64)];
        for bits in [1usize, 40, 64, 160, 248, 255] {
            values.extend([
                U256::ONE << bits,
                (U256::ONE << bits) - U256::ONE,
                !(U256::ONE << bits),
                U256::MAX << bits,
                U256::MAX >> bits,
            ]);
        }
        for evm_version in [EvmVersion::Paris, EvmVersion::Osaka] {
            for &value in &values {
                let mut emitted = Cost::ZERO;
                ImmediateMaterialization::new(evm_version, value).for_each(|op| {
                    emitted += match op {
                        ImmediateMaterializationOp::Push(value) => {
                            literal_cost(push_width(evm_version, value))
                        }
                        ImmediateMaterializationOp::Opcode(opcode) => Cost::fixed_opcode(opcode),
                    };
                });
                let cost = Target::with(evm_version, OptimizationMode::Gas, 200).push(value);
                assert_eq!(cost, emitted, "{value} on {evm_version}");
            }
        }
    }

    /// A mask shifted by any amount costs `PUSH0 NOT PUSH1 s SHL|SHR`: five bytes and eleven gas.
    #[test]
    fn shifted_masks_cost_one_shift() {
        for value in [U256::MAX << 160, U256::MAX << 100, U256::MAX >> 1, U256::MAX >> 129] {
            let cost = immediate_materialization(EvmVersion::Osaka, value);
            assert_eq!((cost.bytes, cost.gas), (5, 11), "{value}");
            let cost = immediate_materialization(EvmVersion::Paris, value);
            assert_eq!((cost.bytes, cost.gas), (6, 12), "{value} on paris");
        }
    }
}
