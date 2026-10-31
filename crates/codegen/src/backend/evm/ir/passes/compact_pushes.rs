//! Target-dependent synthesis of immediates.
//!
//! A literal `PUSHn` is not always the cheapest way to construct a 256-bit constant. For each
//! concrete immediate this pass searches the programs that compute it from smaller constants by
//! inverse rules, and emits the one the [`Target`] prefers, [`Target::cmp_immediate`]. The literal
//! is the first candidate and wins ties, so the pass never emits what the target ranks worse.
//!
//! The search is recursive: a rule rewrites the value `v` into one or two child values, which are
//! synthesized in turn, up to [`MAX_DEPTH`] rules deep. The rules, in terms of the program for `v`:
//!
//! - `NOT` of `!v`.
//! - `SHL` by the trailing zero bits `tz` of `v`, of `v >> tz` or of the same with the high `tz`
//!   bits set, which the shift discards and which can make the child a `NOT` of something small.
//! - `SHR` by the leading zero bits `lz` of `v`, of `v << lz` with the low `lz` bits set, which the
//!   shift fills anyway: `(!x) >> s` shapes.
//! - `DIV` of the all-ones word by `d`, where `MAX / d == v`, either `MAX / v` or the smallest such
//!   `d`. Reciprocals of decimal scales, such as `MAX / 10000`, fold to this.
//! - `MUL` of a byte-period repunit (`0x0101...`, `0x00010001...`, ...) by the quotient.
//! - A split at the top of the longest internal run of zero bytes into the `OR` of two fields, or
//!   at the top of a run of `0xff` bytes into the difference `(hi + 2^k) - (2^k - lo)`: the
//!   one-split case of solc's `ConstantOptimiser`.
//! - A perfect square, as `DUP1 MUL` of its root.
//! - Powers of ten, `10^k` and `c * 10^k`, through `EXP`, priced by its exponent.
//! - Doubling, `a | (a << s)` of a repeated half, as `DUP1 PUSH s SHL OR`.
//!
//! Shifts need Constantinople, and `PUSH0` Shanghai; the search uses only what the target has.
//! A literal of at most four bytes beats every program in bytes and gas, so only wider ones are
//! searched. A rule that adds opcodes runs only when its children are at least four bytes
//! shorter than the value, or are a reciprocal `MAX / d`, so constants with no structure cost one
//! pass over the rules. Each thread memoizes the programs it finds.
//!
//! The target's objective picks between programs by one linear key: size builds, and cold blocks
//! priced like them by [`Target::priced_for_cold`], rank bytes and then gas; gas builds add the
//! deposit of a program's bytes to its gas over the expected executions, and over the iterations
//! of a loop for a push inside one, so a loop does not rebuild a constant on every iteration. A
//! sum of such keys is the key of the sum, so each value needs one best program, whatever the
//! rule that builds its parent. At `-O none`, a block also counts as in a loop when its function
//! is called from a loop or by recursion. `Target::push` uses the same selector, so other EVM IR
//! passes can compare a prospective rewrite with the bytes, static gas, and instructions that
//! this pass will emit.
//!
//! Programs are expanded once, with their children, so one pass reaches a fixed point. The
//! default pipeline expands them before structural cleanup because tail merging and outlining
//! profit from the concrete instruction shape. Push reordering and assembly-only lowering use the
//! same API for constants they inspect or introduce later. Before increasing a literal's
//! transient stack use, query upper entry depths from the stack verifier and account for each
//! preceding instruction. Growing cycles and dynamically entered paths have unknown entry depths;
//! there, keep the original straight-line path's peak as an upper bound. A physical conditional
//! jump ends that path, since its taken edge can skip later stack growth. Keep the literal when
//! the complete program would overflow.
//!
//! Reusing a constant already live on the stack is the stack scheduler's job, not this pass's.

use super::EvmPass;
use crate::{
    backend::evm::{
        codegen::MAX_STACK_DEPTH,
        ir::{Instruction, Metadata, Module, verify::Verifier},
        op,
    },
    target::{Cost, Target},
};
use alloy_primitives::U256;
use smallvec::{SmallVec, smallvec};
use solar_config::{EvmVersion, OptimizationMode};
use solar_data_structures::map::{FxHashMap, FxHashSet};
use solar_sema::Gcx;
use std::{cell::RefCell, rc::Rc};

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

/// The fewest bytes a rule that adds a push and a shift must save to be worth searching.
const MIN_SAVED_BYTES: usize = 4;

/// The same for `NOT`, which adds one byte.
const MIN_NOT_SAVED_BYTES: usize = 2;

/// How many rules deep a program for one immediate may nest.
const MAX_DEPTH: u8 = 3;

fn compact_pushes(gcx: Gcx<'_>, module: &mut Module) -> bool {
    let evm_version = gcx.sess.opts.evm_version;
    let mut changed = false;
    let mut scratch = Vec::new();
    let mut limits = Vec::new();
    let mut candidates = Vec::new();
    let mut entry_depths = None;
    for block_id in module.blocks.indices() {
        let block = &module.blocks[block_id];
        if !block.instructions.iter().any(|inst| {
            inst.concrete_immediate()
                .is_some_and(|value| push_width(evm_version, value) >= MIN_COMPACT_MASK_WIDTH)
        }) {
            continue;
        }
        // A block's function may select an objective other than the contract's.
        let optimization = block.metadata.optimization.unwrap_or(module.optimization);
        let target = Target::new(gcx, optimization)
            .priced_for_cold(block.metadata.hotness.is_cold())
            .priced_for(module.code_follows);
        // `-O none` runs no inlining, so a loop calls the helpers that hold its hot code; gas
        // builds keep the loop-only rule.
        let in_loop = (block.metadata.in_loop
            || (module.optimization.is_none() && block.metadata.repeats))
            && !block.metadata.hotness.is_cold();
        candidates.clear();
        candidates.extend(block.instructions.iter().enumerate().filter_map(|(at, inst)| {
            let materialization =
                ImmediateMaterialization::compact(&target, inst.concrete_immediate()?, in_loop)?;
            Some((at, materialization))
        }));
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
                // push value => selected_program(value)
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

/// The program the target prefers for one concrete immediate.
pub(in crate::backend) struct ImmediateMaterialization {
    program: Rc<Program>,
}

impl ImmediateMaterialization {
    /// Returns the materialization selected for `value` on `evm_version` under the default
    /// model, outside a loop. Prefer [`Self::select`] when the caller has a [`Target`].
    pub(in crate::backend) fn new(evm_version: EvmVersion, value: U256) -> Self {
        Self::select(&default_model(evm_version), value, false)
    }

    /// Returns the materialization `target` prefers for `value`, inside a loop or not.
    pub(in crate::backend) fn select(target: &Target, value: U256, in_loop: bool) -> Self {
        Self { program: synthesize(target, value, in_loop) }
    }

    /// Returns the materialization of `value` that beats its literal push, if any.
    fn compact(target: &Target, value: U256, in_loop: bool) -> Option<Self> {
        if push_width(target.evm_version(), value) < MIN_COMPACT_MASK_WIDTH {
            return None;
        }
        let program = synthesize(target, value, in_loop);
        (!program.is_literal()).then_some(Self { program })
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
                    op::stack_io(opcode).expect("compact immediate programs use known EVM opcodes");
                depth = depth - usize::from(inputs) + usize::from(outputs);
                peak = peak.max(depth);
            }
        });
        debug_assert_eq!(depth, 1);
        peak
    }

    /// Visits each concrete instruction in execution order.
    pub(in crate::backend) fn for_each(&self, mut f: impl FnMut(ImmediateMaterializationOp)) {
        self.program.ops.iter().for_each(|&op| f(op));
    }
}

/// The model that evm-version-only callers select under: a gas build at the default runs.
const fn default_model(evm_version: EvmVersion) -> Target {
    Target::with(evm_version, OptimizationMode::Gas, Target::DEFAULT_EXPECTED_EXECUTIONS)
}

/// Materializes `value` as `target` prefers it outside a loop.
pub(super) fn materialize_immediate(
    instructions: &mut Vec<Instruction>,
    target: &Target,
    value: U256,
) {
    materialize(
        instructions,
        &ImmediateMaterialization::select(target, value, false),
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

/// Returns the cost of pushing `value` as `target` prefers it outside a loop.
pub(crate) fn push_cost(target: &Target, value: U256) -> Cost {
    if push_width(target.evm_version(), value) < MIN_COMPACT_MASK_WIDTH {
        return literal_push(target.evm_version(), value);
    }
    synthesize(target, value, false).cost
}

/// Returns the cost of pushing `value` as one literal `PUSHn`.
pub(crate) fn literal_push(evm_version: EvmVersion, value: U256) -> Cost {
    literal_cost(push_width(evm_version, value))
}

/// Returns the byte length and gas cost of the materialization selected under the default model.
pub(crate) fn immediate_materialization_cost(
    evm_version: EvmVersion,
    value: U256,
) -> (usize, usize) {
    let cost = push_cost(&default_model(evm_version), value);
    (cost.bytes as usize, cost.gas as usize)
}

pub(in crate::backend) fn immediate_materialization_len(
    evm_version: EvmVersion,
    value: U256,
) -> usize {
    push_cost(&default_model(evm_version), value).bytes as usize
}

/// A sequence of instructions that leaves one value on the stack, with its cost.
#[derive(Clone, Debug)]
struct Program {
    cost: Cost,
    ops: SmallVec<[ImmediateMaterializationOp; 8]>,
}

impl Program {
    fn literal(evm_version: EvmVersion, value: U256) -> Self {
        let cost = literal_cost(push_width(evm_version, value));
        Self { cost, ops: smallvec![ImmediateMaterializationOp::Push(value)] }
    }

    fn is_literal(&self) -> bool {
        self.ops.len() == 1
    }

    fn push_literal(&mut self, evm_version: EvmVersion, value: U256) {
        self.cost += literal_cost(push_width(evm_version, value));
        self.ops.push(ImmediateMaterializationOp::Push(value));
    }

    fn extend(&mut self, other: &Self) {
        self.cost += other.cost;
        self.ops.extend_from_slice(&other.ops);
    }

    fn push_opcode(&mut self, opcode: u8, cost: Cost) {
        self.cost += cost;
        self.ops.push(ImmediateMaterializationOp::Opcode(opcode));
    }
}

/// One step of a candidate program.
#[derive(Clone, Copy)]
enum Step {
    /// A value pushed by its own best program.
    Value(U256),
    Opcode(u8),
    /// `10^exponent`, pushed as `PUSH exponent; PUSH 10; EXP`.
    PowerOfTen(u32),
}

/// What a search prices programs by.
#[derive(Clone, Copy)]
struct Pricing {
    target: Target,
    in_loop: bool,
}

thread_local! {
    /// Every pass and cost query of a thread shares one memo of the wide constants it has seen.
    static SYNTHESIZER: RefCell<Synthesizer> = RefCell::default();
}

/// Returns the program `target` prefers for `value`, inside a loop or not.
fn synthesize(target: &Target, value: U256, in_loop: bool) -> Rc<Program> {
    SYNTHESIZER.with_borrow_mut(|synthesizer| synthesizer.program(target, value, in_loop))
}

/// The programs found for each value, memoized by the pricing and the depth left to search.
#[derive(Default)]
struct Synthesizer {
    memo: FxHashMap<(Target, bool, u8, U256), Rc<Program>>,
    /// The values, with whether shifts exist, that no rule applies to, whatever the pricing.
    barren: FxHashSet<(bool, U256)>,
}

impl Synthesizer {
    /// The programs memoized before the memo starts over.
    const MEMO_LIMIT: usize = 1 << 14;

    fn program(&mut self, target: &Target, value: U256, in_loop: bool) -> Rc<Program> {
        let evm_version = target.evm_version();
        if push_width(evm_version, value) < MIN_COMPACT_MASK_WIDTH || self.is_barren(target, value)
        {
            return Rc::new(Program::literal(evm_version, value));
        }
        self.search(Pricing { target: *target, in_loop }, value, MAX_DEPTH)
    }

    /// Appends the best program for `value` that nests at most `depth` rules to `out`.
    fn append(&mut self, pricing: Pricing, value: U256, depth: u8, out: &mut Program) {
        let evm_version = pricing.target.evm_version();
        // A literal of up to four bytes beats every program in bytes and gas.
        if depth == 0
            || push_width(evm_version, value) < MIN_COMPACT_MASK_WIDTH
            || self.is_barren(&pricing.target, value)
        {
            return out.push_literal(evm_version, value);
        }
        out.extend(&self.search(pricing, value, depth));
    }

    fn is_barren(&self, target: &Target, value: U256) -> bool {
        self.barren.contains(&(target.evm_version().has_bitwise_shifting(), value))
    }

    fn search(&mut self, pricing: Pricing, value: U256, depth: u8) -> Rc<Program> {
        let key = (pricing.target, pricing.in_loop, depth, value);
        if let Some(program) = self.memo.get(&key) {
            return Rc::clone(program);
        }
        let program = Rc::new(self.synthesize(pricing, value, depth));
        if self.memo.len() >= Self::MEMO_LIMIT {
            self.memo.clear();
            self.barren.clear();
        }
        self.memo.insert(key, Rc::clone(&program));
        program
    }

    // Not inlined, so the literal path of `search` stays small.
    #[inline(never)]
    fn synthesize(&mut self, pricing: Pricing, value: U256, depth: u8) -> Program {
        let target = &pricing.target;
        let shifts = target.evm_version().has_bitwise_shifting();
        let one = U256::ONE;
        let mut best = Program::literal(target.evm_version(), value);
        let mut offers = 0;
        let mut offer = |this: &mut Self, steps: &[Step]| {
            offers += 1;
            let program = this.build(pricing, steps, depth - 1);
            if target.cmp_immediate(program.cost, best.cost, pricing.in_loop).is_lt() {
                best = program;
            }
        };
        let [not, shl, shr, div, mul, or, sub, dup] =
            [op::NOT, op::SHL, op::SHR, op::DIV, op::MUL, op::OR, op::SUB, op::DUP1]
                .map(Step::Opcode);
        let max = Step::Value(U256::MAX);

        // Each rule below runs only when its children could be at least four bytes shorter than
        // `value`, the saving that pays for the extra opcodes; this keeps the search from
        // expanding the children of constants that have no structure.
        let width = usize::from(push_width(target.evm_version(), value));
        let shorter =
            |child: U256, saved: usize| span_bytes(child) + saved <= width || is_reciprocal(child);

        // push !value; not
        if shorter(!value, MIN_NOT_SAVED_BYTES) {
            offer(self, &[Step::Value(!value), not]);
        }

        let tz = value.trailing_zeros();
        let lz = value.leading_zeros();
        if shifts && tz > 0 {
            // The shift discards the high bits of its operand, so set them for a `NOT` child.
            let filled = (value >> tz) | !(U256::MAX >> tz);
            // Shifting by whole bytes first keeps the literal readable when it costs the same.
            let whole = tz / 8 * 8;
            for shift in [whole, tz].into_iter().take(if whole == tz { 1 } else { 2 }) {
                if shift > 0 && shorter(value >> shift, MIN_SAVED_BYTES) {
                    // push value >> shift; push shift; shl
                    let steps = [Step::Value(value >> shift), Step::Value(U256::from(shift)), shl];
                    offer(self, &steps);
                }
            }
            if shorter(filled, MIN_SAVED_BYTES) || shorter(!filled, MIN_SAVED_BYTES) {
                // push filled; push tz; shl
                offer(self, &[Step::Value(filled), Step::Value(U256::from(tz)), shl]);
            }
        }
        if shifts && lz > 0 {
            // The shift fills the high bits with zeros, so set the low bits of its operand.
            let filled = (value << lz) | !(U256::MAX << lz);
            if shorter(filled, MIN_SAVED_BYTES) || shorter(!filled, MIN_SAVED_BYTES) {
                // push (value << lz) | ones; push lz; shr
                offer(self, &[Step::Value(filled), Step::Value(U256::from(lz)), shr]);
            }
        }

        if value != U256::MAX {
            let largest = U256::MAX / value;
            let smallest = U256::MAX / (value + one) + one;
            for divisor in [largest, smallest] {
                if divisor > one && floors_to(divisor, value) {
                    // push divisor; push MAX; div
                    offer(self, &[Step::Value(divisor), max, div]);
                }
            }
        }
        // A multiple of the repunit of `period` bytes repeats one factor in every period.
        let bytes = value.to_be_bytes::<32>();
        for period in [1usize, 2, 4, 8, 16] {
            let ones = U256::MAX / ((one << (period * 8)) - one);
            if value != ones && bytes.chunks_exact(period).all(|chunk| chunk == &bytes[..period]) {
                // push value / ones; push ones; mul
                offer(self, &[Step::Value(value / ones), Step::Value(ones), mul]);
            }
        }

        if let Some(bits) = longest_run_top(&bytes, 0x00, true) {
            let mask = (one << bits) - one;
            // push value & !mask; push value & mask; or
            offer(self, &[Step::Value(value & !mask), Step::Value(value & mask), or]);
        }
        if let Some(bits) = longest_run_top(&bytes, 0xff, false) {
            let carry = one << bits;
            let mask = carry - one;
            let (high, low) = (value & !mask, value & mask);
            // push 2^k - low; push high + 2^k; sub
            offer(self, &[Step::Value(carry - low), Step::Value(high.wrapping_add(carry)), sub]);
        }

        // Squares are 0, 1, 4 or 9 modulo 16.
        let root = if matches!(value.as_limbs()[0] & 0xf, 0 | 1 | 4 | 9) {
            value.root(2)
        } else {
            U256::ZERO
        };
        if root > one && root * root == value {
            // push root; dup1; mul
            offer(self, &[Step::Value(root), dup, mul]);
        }

        let ten = U256::from(10);
        let (mut factor, mut exponent) = (value, 0u32);
        // A multiple of 100 is a multiple of 4.
        if value.as_limbs()[0] & 3 == 0 {
            while (factor % ten).is_zero() {
                factor /= ten;
                exponent += 1;
            }
        }
        if exponent >= 2 {
            if factor == one {
                // push k; push 10; exp
                offer(self, &[Step::PowerOfTen(exponent)]);
            } else {
                // push factor; push k; push 10; exp; mul
                offer(self, &[Step::Value(factor), Step::PowerOfTen(exponent), mul]);
            }
        }

        if shifts {
            for shift in (8..=128).step_by(8) {
                let half = value >> shift;
                if (half << shift) | half == value {
                    // push half; dup1; push shift; shl; or
                    offer(self, &[Step::Value(half), dup, Step::Value(U256::from(shift)), shl, or]);
                }
            }
        }

        if offers == 0 {
            self.barren.insert((shifts, value));
        }
        best
    }

    /// The program that runs `steps`, searching each value `depth` rules deep.
    fn build(&mut self, pricing: Pricing, steps: &[Step], depth: u8) -> Program {
        let target = &pricing.target;
        let evm_version = target.evm_version();
        let mut program = Program { cost: Cost::ZERO, ops: SmallVec::new() };
        for &step in steps {
            match step {
                Step::Value(value) => self.append(pricing, value, depth, &mut program),
                Step::Opcode(opcode) => program.push_opcode(opcode, target.opcode(opcode)),
                Step::PowerOfTen(exponent) => {
                    let (exponent, base) = (U256::from(exponent), U256::from(10));
                    program.push_literal(evm_version, exponent);
                    program.push_literal(evm_version, base);
                    let arguments = [Some(base), Some(exponent)];
                    let cost = target.opcode_with_immediates(op::EXP, &arguments);
                    program.push_opcode(op::EXP, cost);
                }
            }
        }
        program
    }
}

/// The bit position of the top of the longest run of `byte` in big-endian `bytes` that does not
/// reach the top of the word. A run at the bottom counts unless `internal`.
fn longest_run_top(bytes: &[u8; 32], byte: u8, internal: bool) -> Option<usize> {
    let mut best: Option<(usize, usize)> = None;
    let mut start = 0;
    while start < bytes.len() {
        if bytes[start] != byte {
            start += 1;
            continue;
        }
        let len = bytes[start..].iter().take_while(|&&b| b == byte).count();
        let end = start + len;
        if start > 0
            && (!internal || end < bytes.len())
            && best.is_none_or(|(_, longest)| len > longest)
        {
            best = Some(((bytes.len() - start) * 8, len));
        }
        start = end;
    }
    best.filter(|&(_, len)| len >= MIN_SAVED_BYTES).map(|(bits, _)| bits)
}

/// Whether `value` is `MAX / d` for a divisor `d` of at most eight bytes.
fn is_reciprocal(value: U256) -> bool {
    if value < U256::from(2) {
        return false;
    }
    let divisor = U256::MAX / value;
    divisor.byte_len() <= 8 && floors_to(divisor, value)
}

/// Whether `MAX / divisor == quotient`, without dividing.
fn floors_to(divisor: U256, quotient: U256) -> bool {
    quotient.checked_mul(divisor).is_some_and(|product| U256::MAX - product < divisor)
}

/// The bytes from the first to the last nonzero byte of `value`.
fn span_bytes(value: U256) -> usize {
    if value.is_zero() {
        0
    } else {
        (256 - value.leading_zeros() - value.trailing_zeros()).div_ceil(8)
    }
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
    use alloy_primitives::{U256, uint};
    use solar_config::OptimizationMode;

    /// The value the program leaves on the stack, and the cost of its instructions.
    fn run(target: &Target, materialization: &ImmediateMaterialization) -> (U256, Cost) {
        let (mut stack, mut cost) = (Vec::<U256>::new(), Cost::ZERO);
        materialization.for_each(|op| match op {
            ImmediateMaterializationOp::Push(value) => {
                stack.push(value);
                cost += literal_cost(push_width(target.evm_version(), value));
            }
            ImmediateMaterializationOp::Opcode(opcode) => {
                let first = stack.pop().unwrap();
                let (result, opcode_cost) = match opcode {
                    op::NOT => (!first, target.opcode(opcode)),
                    op::DUP1 => {
                        stack.push(first);
                        (first, target.opcode(opcode))
                    }
                    _ => {
                        let second = stack.pop().unwrap();
                        let result = match opcode {
                            op::SHL => second << first,
                            op::SHR => second >> first,
                            op::DIV => first / second,
                            op::MUL => first.wrapping_mul(second),
                            op::OR => first | second,
                            op::SUB => first.wrapping_sub(second),
                            op::EXP => first.pow(second),
                            _ => unreachable!("unexpected opcode {opcode:#x}"),
                        };
                        let arguments = [Some(first), Some(second)];
                        (result, target.opcode_with_immediates(opcode, &arguments))
                    }
                };
                stack.push(result);
                cost += opcode_cost;
            }
        });
        assert_eq!(stack.len(), 1);
        (stack[0], cost)
    }

    /// `Target::push` prices the instructions the selector emits, and they compute the value.
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
        let ten = U256::from(10);
        values.extend([
            // Error(string)'s selector.
            uint!(0x08c379a0_U256) << 224,
            uint!(0x3fffffffffffffffffffffffffffffffffffffffffffffffffffffbfffff0c_U256),
            // The P-256 prime.
            uint!(0xffffffff00000001000000000000000000000000ffffffffffffffffffffffff_U256),
            // A zero split, and a split at a run of `0xff`.
            (U256::MAX << 160) | U256::from(0xff),
            uint!(0xabcdef0123456789abcd_U256) << 128 | U256::MAX >> 130,
            uint!(0xfffffffffffffffffffffffffffffffffffffffffffffffffe_U256) << 8
                | U256::from(0x11),
            // Reciprocals of decimal scales, repunit multiples, squares, and powers of ten.
            U256::MAX / U256::from(10_000),
            U256::MAX / ten.pow(U256::from(18)),
            U256::MAX / U256::from(86_400),
            U256::MAX / U256::from(0xff) * U256::from(0x2a),
            ten.pow(U256::from(36)),
            ten.pow(U256::from(18)),
            ten.pow(U256::from(27)) * U256::from(12_345),
            uint!(0x123456789abcdef0123456789abcdef1_U256).pow(U256::from(2)),
            // A repeated half.
            uint!(0x0123456789abcdef0123456789abcdef_U256) * ((U256::ONE << 128) + U256::ONE),
        ]);
        let gas = Target::with(EvmVersion::Osaka, OptimizationMode::Gas, 200);
        let size = Target::with(EvmVersion::Osaka, OptimizationMode::Size, 200);
        let targets = [
            gas,
            size,
            gas.priced_for_cold(true),
            gas.priced_for(true),
            Target::with(EvmVersion::Paris, OptimizationMode::Gas, 200),
            Target::with(EvmVersion::Byzantium, OptimizationMode::Size, 200),
        ];
        for target in targets {
            for &value in &values {
                let materialization = ImmediateMaterialization::select(&target, value, false);
                let (computed, emitted) = run(&target, &materialization);
                assert_eq!(computed, value, "{value} on {target:?}");
                assert_eq!(target.push(value), emitted, "{value} on {target:?}");
            }
        }
    }

    /// A mask shifted by any amount costs `PUSH0 NOT PUSH1 s SHL|SHR`: five bytes and eleven gas.
    #[test]
    fn shifted_masks_cost_one_shift() {
        for value in [U256::MAX << 160, U256::MAX << 100, U256::MAX >> 1, U256::MAX >> 129] {
            let cost = Target::with(EvmVersion::Osaka, OptimizationMode::Gas, 200).push(value);
            assert_eq!((cost.bytes, cost.gas), (5, 11), "{value}");
            let cost = Target::with(EvmVersion::Paris, OptimizationMode::Gas, 200).push(value);
            assert_eq!((cost.bytes, cost.gas), (6, 12), "{value} on paris");
        }
    }
}
