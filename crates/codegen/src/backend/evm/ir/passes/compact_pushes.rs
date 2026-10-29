//! Target-dependent selection of compact immediate materializations.
//!
//! A literal `PUSHn` is not always the shortest way to construct a 256-bit constant. For each
//! concrete immediate, this pass compares the literal encoding with a fixed set of equivalent
//! recipes: `PUSH0; NOT` for an all-ones word, `NOT` of a shorter inverse, and shift-based forms
//! for masks or values with trailing zero bytes. It then emits the recipe with the fewest encoded
//! bytes, whatever the tune, keeping the literal on ties so the pass never increases code size. In
//! size builds and cold blocks, division of the all-ones word and multiplication also synthesize
//! repeated byte patterns. Gas-mode hot blocks retain the existing shift and inverse recipes.
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
const EVM_WORD_BITS: usize = WORD_BYTES * 8;
const MIN_COMPACT_MASK_WIDTH: u8 = 5;

fn compact_pushes(gcx: Gcx<'_>, module: &mut Module) -> bool {
    let evm_version = gcx.sess.opts.evm_version;
    let mut changed = false;
    let mut scratch = Vec::new();
    let mut limits = Vec::new();
    let mut entry_depths = None;
    for block_id in module.blocks.indices() {
        let block = &module.blocks[block_id];
        // A block's function may select an objective other than the contract's.
        let target = Target::new(gcx, block.metadata.optimization.unwrap_or(module.optimization))
            .priced_for(module.code_follows);
        // `-O none` runs no inlining, so a loop calls the helpers that hold its hot code; gas
        // builds keep the loop-only rule.
        let in_loop = (block.metadata.in_loop
            || (module.optimization.is_none() && block.metadata.repeats))
            && !block.metadata.hotness.is_cold();
        let repeated = target.optimization().is_size() || block.metadata.hotness.is_cold();
        if !block.instructions.iter().any(|inst| {
            inst.concrete_immediate().is_some_and(|value| {
                !matches!(select_recipe(evm_version, value, repeated).recipe, CompactPush::Literal)
            })
        }) {
            continue;
        }
        if entry_depths.is_none()
            && block.instructions.iter().any(|inst| {
                inst.concrete_immediate().is_some_and(|value| {
                    ImmediateMaterialization::with_repeated(evm_version, value, repeated)
                        .stack_peak()
                        > 1
                })
            })
        {
            entry_depths = Some(Verifier::stack_entry_upper_bounds(module, evm_version));
        }
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
        for (at, inst) in scratch.drain(..).enumerate() {
            let effect = inst.stack_effect();
            let (inputs, outputs) = (usize::from(effect.inputs), usize::from(effect.outputs));
            let before = stack.min(limits[at]);
            stack = before.saturating_sub(inputs) + outputs;
            let Some(value) = inst.concrete_immediate() else {
                block.instructions.push(inst);
                continue;
            };
            let materialization =
                ImmediateMaterialization::with_repeated(evm_version, value, repeated);
            if matches!(materialization.recipe, CompactPush::Literal)
                || !materialization.repays(&target, in_loop)
                || materialization.stack_peak() > MAX_STACK_DEPTH - before
            {
                block.instructions.push(inst);
            } else {
                // push value => selected_recipe(value)
                materialize_selected(&mut block.instructions, materialization, &inst.metadata);
                changed = true;
            }
        }
    }
    changed
}

fn push(value: U256) -> Instruction {
    Instruction::push_value(value)
}

/// One instruction in a selected immediate materialization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::backend) enum ImmediateMaterializationOp {
    Push(U256),
    Opcode(u8),
}

/// The shortest selected materialization for one concrete immediate.
#[derive(Clone, Copy)]
pub(in crate::backend) struct ImmediateMaterialization {
    evm_version: EvmVersion,
    value: U256,
    recipe: CompactPush,
    repeated: bool,
}

impl ImmediateMaterialization {
    /// Returns the materialization selected for `value` on `evm_version`.
    pub(in crate::backend) fn new(evm_version: EvmVersion, value: U256) -> Self {
        Self::with_repeated(evm_version, value, false)
    }

    fn with_repeated(evm_version: EvmVersion, value: U256, repeated: bool) -> Self {
        Self {
            evm_version,
            value,
            repeated,
            recipe: select_recipe(evm_version, value, repeated).recipe,
        }
    }

    fn child(self, value: U256) -> Self {
        Self::with_repeated(self.evm_version, value, self.repeated)
    }

    /// Returns the materialization's maximum relative stack height.
    pub(in crate::backend) fn stack_peak(self) -> usize {
        self.metrics().stack_peak
    }

    /// Whether the selected recipe is better code than the literal push, by the target's pricing
    /// of the bytes it saves against the runtime gas it adds, inside a loop or not.
    fn repays(self, target: &Target, in_loop: bool) -> bool {
        let literal = select_literal(self.evm_version, self.value);
        let recipe = select_recipe(self.evm_version, self.value, self.repeated).cost;
        target.compacts_immediate(
            u64::from(literal.bytes.saturating_sub(recipe.bytes)),
            u64::from(target.runtime_gas(recipe).saturating_sub(target.runtime_gas(literal))),
            in_loop,
        )
    }

    /// Visits each concrete instruction in execution order.
    pub(in crate::backend) fn for_each(self, mut f: impl FnMut(ImmediateMaterializationOp)) {
        self.for_each_inner(&mut f);
    }

    fn for_each_inner(self, f: &mut impl FnMut(ImmediateMaterializationOp)) {
        let push = ImmediateMaterializationOp::Push;
        let opcode = ImmediateMaterializationOp::Opcode;
        match self.recipe {
            CompactPush::Literal => f(push(self.value)),
            CompactPush::FullWord => {
                self.child(U256::ZERO).for_each_inner(f);
                f(opcode(op::NOT));
            }
            CompactPush::LowerAllOnesMask { shift } => {
                self.child(U256::ZERO).for_each_inner(f);
                f(opcode(op::NOT));
                self.child(U256::from(shift)).for_each_inner(f);
                f(opcode(op::SHR));
            }
            CompactPush::Not => {
                self.child(!self.value).for_each_inner(f);
                f(opcode(op::NOT));
            }
            CompactPush::Shl { shift } => {
                self.child(self.value >> usize::from(shift)).for_each_inner(f);
                self.child(U256::from(shift)).for_each_inner(f);
                f(opcode(op::SHL));
            }
            // divisor; MAX; DIV
            CompactPush::AllOnesDiv { divisor } => {
                self.child(divisor).for_each_inner(f);
                self.child(U256::MAX).for_each_inner(f);
                f(opcode(op::DIV));
            }
            // divisor; MAX; DIV; factor; MUL
            CompactPush::Repeat { factor, divisor } => {
                self.child(divisor).for_each_inner(f);
                self.child(U256::MAX).for_each_inner(f);
                f(opcode(op::DIV));
                self.child(factor).for_each_inner(f);
                f(opcode(op::MUL));
            }
        }
    }

    fn metrics(self) -> ImmediateMaterializationMetrics {
        // A literal is one push, which the visitor below would measure the same way.
        if matches!(self.recipe, CompactPush::Literal) {
            let (encoded_len, static_gas) = literal_cost(self.evm_version, self.value);
            return ImmediateMaterializationMetrics {
                encoded_len,
                static_gas,
                steps: 1,
                stack_peak: 1,
            };
        }
        let mut metrics = ImmediateMaterializationMetrics::default();
        let mut depth = 0usize;
        self.for_each(|materialized| match materialized {
            ImmediateMaterializationOp::Push(value) => {
                let (len, gas) = literal_cost(self.evm_version, value);
                metrics.encoded_len += len;
                metrics.static_gas += gas;
                metrics.steps += 1;
                depth += 1;
                metrics.stack_peak = metrics.stack_peak.max(depth);
            }
            ImmediateMaterializationOp::Opcode(opcode) => {
                let (inputs, outputs) =
                    op::stack_io(opcode).expect("compact immediate recipes use known EVM opcodes");
                depth = depth - usize::from(inputs) + usize::from(outputs);
                metrics.encoded_len += 1;
                metrics.static_gas += fixed_gas(opcode);
                metrics.steps += 1;
            }
        });
        debug_assert_eq!(depth, 1);
        metrics
    }
}

#[derive(Default)]
struct ImmediateMaterializationMetrics {
    encoded_len: usize,
    static_gas: usize,
    steps: usize,
    stack_peak: usize,
}

pub(super) fn materialize_immediate(
    instructions: &mut Vec<Instruction>,
    evm_version: EvmVersion,
    value: U256,
) {
    materialize_selected(
        instructions,
        ImmediateMaterialization::new(evm_version, value),
        &Metadata::default(),
    );
}

/// Every replacement carries the source debug information of the push it materializes.
fn materialize_selected(
    instructions: &mut Vec<Instruction>,
    materialization: ImmediateMaterialization,
    source: &Metadata,
) {
    materialization.for_each(|op| {
        let mut replacement = match op {
            ImmediateMaterializationOp::Push(value) => push(value),
            ImmediateMaterializationOp::Opcode(opcode) => Instruction::opcode(opcode),
        };
        replacement.metadata.copy_source_debug_from(source);
        instructions.push(replacement);
    });
}

/// The materialization selected for one concrete immediate, with its cost.
#[derive(Clone, Copy)]
struct Selected {
    cost: Cost,
    recipe: CompactPush,
}

/// Returns the cost of the selected immediate materialization.
pub(crate) fn immediate_materialization(evm_version: EvmVersion, value: U256) -> Cost {
    select(evm_version, value).cost
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
    select(evm_version, value).cost.bytes as usize
}

fn select(evm_version: EvmVersion, value: U256) -> Selected {
    select_recipe(evm_version, value, false)
}

fn select_recipe(evm_version: EvmVersion, value: U256, repeated: bool) -> Selected {
    let width = push_width(evm_version, value);
    let literal =
        Selected { cost: select_literal(evm_version, value), recipe: CompactPush::Literal };
    // NOT recipes require a full-width input. A shifted nonzero literal needs
    // at least two PUSH1s and SHL (five bytes), so PUSH4 and shorter already win
    // or tie every recipe. Keep the literal on ties, as the full search does.
    if width < MIN_COMPACT_MASK_WIDTH {
        return literal;
    }
    let mut best = literal;
    let mut consider = |cost: Cost, recipe| {
        if cost.bytes < best.cost.bytes {
            best = Selected { cost, recipe };
        }
    };
    let zero = select_literal(evm_version, U256::ZERO);

    if value == U256::MAX {
        consider(zero + Cost::fixed_opcode(op::NOT), CompactPush::FullWord);
    }

    if evm_version.has_bitwise_shifting() && width >= MIN_COMPACT_MASK_WIDTH {
        let bytes = value.to_be_bytes::<WORD_BYTES>();
        let start = WORD_BYTES - width as usize;
        if bytes[start..].iter().all(|&byte| byte == 0xff) {
            let shift = EVM_WORD_BITS - usize::from(width) * 8;
            consider(
                zero + Cost::fixed_opcode(op::NOT)
                    + select(evm_version, U256::from(shift)).cost
                    + Cost::fixed_opcode(op::SHR),
                CompactPush::LowerAllOnesMask { shift: shift as u8 },
            );
        }
    }

    if width as usize == WORD_BYTES {
        let inverted = !value;
        if push_width(evm_version, inverted) < width {
            consider(
                select_recipe(evm_version, inverted, repeated).cost + Cost::fixed_opcode(op::NOT),
                CompactPush::Not,
            );
        }
    }

    // Restrict divisions and multiplications to size-first code. Every child is narrower,
    // so recursive selection terminates, and strict byte savings are required.
    let child = |word| select_recipe(evm_version, word, repeated).cost;
    let narrower = |word| push_width(evm_version, word) < width;
    if repeated && value < U256::MAX {
        let full = child(U256::MAX);
        let divisor = U256::MAX / value;
        if (U256::MAX % value).is_zero() && narrower(divisor) {
            consider(
                child(divisor) + full + Cost::fixed_opcode(op::DIV),
                CompactPush::AllOnesDiv { divisor },
            );
        }
        for period in [1usize, 2, 4, 8, 16] {
            let divisor = (U256::ONE << (period * 8)) - U256::ONE;
            let ones = U256::MAX / divisor;
            if !(value % ones).is_zero() {
                continue;
            }
            let factor = value / ones;
            if factor > U256::ONE && narrower(factor) && narrower(divisor) {
                consider(
                    child(factor)
                        + child(divisor)
                        + full
                        + Cost::fixed_opcode(op::DIV)
                        + Cost::fixed_opcode(op::MUL),
                    CompactPush::Repeat { factor, divisor },
                );
            }
        }
    }

    let trailing_zero_bytes = value.trailing_zeros() / 8;
    if evm_version.has_bitwise_shifting()
        && trailing_zero_bytes > 0
        && trailing_zero_bytes < WORD_BYTES
    {
        let shift = trailing_zero_bytes * 8;
        let shifted = value >> shift;
        consider(
            select_recipe(evm_version, shifted, repeated).cost
                + select_recipe(evm_version, U256::from(shift), repeated).cost
                + Cost::fixed_opcode(op::SHL),
            CompactPush::Shl { shift: shift as u8 },
        );
    }

    best
}

/// The cost of one literal `PUSHn` of `value`.
fn select_literal(evm_version: EvmVersion, value: U256) -> Cost {
    let (len, gas) = literal_cost(evm_version, value);
    Cost::new(gas as u32, len as u32).with_steps(1)
}

/// Returns the byte length and gas cost of one literal `PUSHn` of `value`.
pub(crate) fn literal_cost(evm_version: EvmVersion, value: U256) -> (usize, usize) {
    (
        fixed_push_len(evm_version, push_width(evm_version, value)),
        fixed_gas(if value.is_zero() && evm_version.has_push0() { op::PUSH0 } else { op::PUSH1 }),
    )
}

/// The static gas of a recipe opcode, which no EVM version changes.
const fn fixed_gas(opcode: u8) -> usize {
    match op::definition(opcode) {
        Some(definition) => definition.gas.fixed_gas() as usize,
        None => panic!("compact immediate recipes use known opcodes"),
    }
}

fn fixed_push_len(evm_version: EvmVersion, width: u8) -> usize {
    if width == 0 { zero_push_len(evm_version) } else { 1 + width as usize }
}

fn zero_push_len(evm_version: EvmVersion) -> usize {
    if evm_version.has_push0() { 1 } else { 2 }
}

fn push_width(evm_version: EvmVersion, value: U256) -> u8 {
    if value.is_zero() && !evm_version.has_push0() { 1 } else { value.byte_len() as u8 }
}

#[derive(Clone, Copy)]
enum CompactPush {
    AllOnesDiv { divisor: U256 },
    Repeat { factor: U256, divisor: U256 },
    Literal,
    FullWord,
    LowerAllOnesMask { shift: u8 },
    Not,
    Shl { shift: u8 },
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
        for bits in [40usize, 64, 160, 248, 255] {
            values.extend([
                U256::ONE << bits,
                (U256::ONE << bits) - U256::ONE,
                !(U256::ONE << bits),
            ]);
        }
        for evm_version in [EvmVersion::Paris, EvmVersion::Osaka] {
            for &value in &values {
                let metrics = ImmediateMaterialization::new(evm_version, value).metrics();
                let cost = Target::with(evm_version, OptimizationMode::Gas, 200).push(value);
                assert_eq!(
                    (cost.bytes as usize, cost.gas as usize, cost.steps as usize),
                    (metrics.encoded_len, metrics.static_gas, metrics.steps),
                    "{value} on {evm_version}"
                );
            }
        }
    }
}
