//! Target cost model.
//!
//! One place answers what code costs on the selected EVM version: the static
//! gas of every opcode across the fork schedule, the encoded size of pushes
//! and stack operations, the price of deposited bytes, and how the active
//! optimization objective ranks gas against bytes. Creation code has its own
//! model, [`Target::priced_for`]: it runs once, and each of its bytes costs the
//! calldata gas of the deployment transaction instead of a code deposit. Cold code, which does
//! not run on success, is priced by [`Target::priced_for_cold`] like a size build.
//! Every pass that chooses between equivalent code shapes consults [`Target`]
//! instead of carrying its own constants, so a schedule change or a new EVM
//! version lands here once.
//!
//! Gas is the amount charged before dynamic components such as memory
//! expansion or copied words. Account and storage accesses are priced cold
//! unless a query asks for the warm price of a slot or account the
//! transaction already touched. The model ranks alternatives, it never bills
//! an execution.

use crate::{
    backend::evm::{ir::compact_pushes, op, select},
    mir::{
        BlockId, Function, InstId, Op, Terminator, Value, ValueId, analysis::AddressSpace,
        memory::EvmMemoryLayout,
    },
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_config::{EvmVersion, OptimizationMode, Tune};
use solar_evmir::gas::since;
use solar_sema::Gcx;
use std::{cmp::Ordering, ops};

mod stack;
pub(crate) use stack::StackCosts;

pub(crate) use solar_evmir::gas::{GasTier, Warmth};

/// Encoded size of an opcode including its immediate bytes.
const fn opcode_bytes(opcode: u8) -> u32 {
    let immediate = match opcode {
        op::PUSH1..=op::PUSH32 => (opcode - op::PUSH1 + 1) as u32,
        _ => 0,
    };
    1 + immediate
}

/// Static gas, encoded size, and executed instructions of a code sequence.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct Cost {
    /// Static gas of one execution.
    pub(crate) gas: u32,
    /// Encoded bytes.
    pub(crate) bytes: u32,
    /// Instructions of one execution. Execution engines that dispatch each instruction pay for
    /// them; see [`Target::runtime_gas`].
    pub(crate) steps: u32,
}

impl Cost {
    /// Nothing emitted.
    pub(crate) const ZERO: Self = Self::new(0, 0);

    /// More than any real code costs.
    pub(crate) const MAX: Self = Self::new(u32::MAX, u32::MAX).with_steps(u32::MAX);

    /// Creates a cost of code whose executed instructions are not counted.
    pub(crate) const fn new(gas: u32, bytes: u32) -> Self {
        Self { gas, bytes, steps: 0 }
    }

    /// This cost with `steps` executed instructions.
    pub(crate) const fn with_steps(self, steps: u32) -> Self {
        Self { steps, ..self }
    }

    /// Cost of an opcode whose static gas is identical on every EVM version.
    /// Panics for unknown opcodes or a fork-dependent gas tier.
    pub(crate) const fn fixed_opcode(opcode: u8) -> Self {
        let Some(definition) = op::definition(opcode) else { panic!("unknown opcode") };
        Self::new(definition.gas.fixed_gas(), opcode_bytes(opcode)).with_steps(1)
    }

    /// Sums two costs, saturating.
    pub(crate) const fn plus(self, other: Self) -> Self {
        Self {
            gas: self.gas.saturating_add(other.gas),
            bytes: self.bytes.saturating_add(other.bytes),
            steps: self.steps.saturating_add(other.steps),
        }
    }

    /// Subtracts `other` from every dimension, saturating at zero.
    pub(crate) const fn saturating_sub(self, other: Self) -> Self {
        Self {
            gas: self.gas.saturating_sub(other.gas),
            bytes: self.bytes.saturating_sub(other.bytes),
            steps: self.steps.saturating_sub(other.steps),
        }
    }

    /// Scales a cost, saturating.
    pub(crate) const fn times(self, count: u32) -> Self {
        Self {
            gas: self.gas.saturating_mul(count),
            bytes: self.bytes.saturating_mul(count),
            steps: self.steps.saturating_mul(count),
        }
    }
}

impl ops::Add for Cost {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        self.plus(other)
    }
}

impl ops::AddAssign for Cost {
    fn add_assign(&mut self, other: Self) {
        *self = self.plus(other);
    }
}

impl std::iter::Sum for Cost {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::ZERO, Self::plus)
    }
}

/// What a branch's test becomes when the branch turns around.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TurnedTest {
    /// An `ISZERO` follows the condition.
    Adds,
    /// The condition's own trailing `ISZERO` goes.
    Drops,
    /// A trailing `EQ` becomes a `SUB`, or the reverse, at the same cost.
    Swaps,
}

/// The cost model of the selected EVM version under the active objective.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Target {
    evm_version: EvmVersion,
    optimization: OptimizationMode,
    expected_executions: u64,
    /// Whether the model prices creation code, which runs once and pays calldata per byte.
    creation: bool,
    /// The execution engine the code is tuned for.
    tune: Tune,
}

impl Target {
    /// Gas charged per deposited byte of runtime code (EIP-170 `CODEDEPOSIT`).
    pub(crate) const CODE_DEPOSIT_GAS_PER_BYTE: u32 = 200;
    /// Gas of a nonzero calldata byte since Istanbul (EIP-2028).
    const CALLDATA_NONZERO_BYTE_GAS: u32 = 16;
    /// Gas of a nonzero calldata byte before Istanbul.
    const LEGACY_CALLDATA_NONZERO_BYTE_GAS: u32 = 68;
    /// Expected executions per deployment when the optimizer runs are not set;
    /// solc's convention.
    pub(crate) const DEFAULT_EXPECTED_EXECUTIONS: u64 = 200;
    /// No loop runs more than `2^MAX_TRIP_COUNT_BITS` times: every execution
    /// client and block header keeps gas in 64 bits, and each iteration of a
    /// MIR loop executes at least one priced instruction (its counter update
    /// or its back-edge jump), so a counter stepping by a small constant never
    /// travels further than `step << 64` from where it started.
    pub(crate) const MAX_TRIP_COUNT_BITS: usize = 64;
    /// How many times a loop without a computable trip count is assumed to
    /// run per invocation when weighing code growth inside it: GCC's estimate
    /// for such loops.
    pub(crate) const UNCOUNTED_LOOP_ITERATIONS: u64 = 10;
    /// Gas an interpreter's dispatch of one instruction is worth, charged per executed
    /// instruction under [`Tune::Interpreter`]. Measured on evm2's interpreter (AMD Zen 5,
    /// tail-call dispatch): every opcode costs about 7.3 cycles whatever its push width or
    /// `DUP` and `SWAP` depth, a `JUMPDEST` 7.6, while heavy opcodes cost about one cycle per
    /// gas (`EXP` of 2 k: 57 gas against 51 to 56 cycles). One dispatch is then worth about 4
    /// gas of work beyond the 3 gas of a cheap opcode. A fit of whole programs agrees: cycles grew
    /// by 6.7 per executed opcode (R² 0.93), about 3.7 gas each.
    pub(crate) const INTERPRETER_DISPATCH_GAS: u32 = 4;

    /// How many consecutive MIR instructions must repeat for the backend's tail merge and
    /// outlining to share them. Six kept corpus bytecode smallest in `-Osize`; shorter runs
    /// match common idioms that no pass shares, and longer ones miss shared wrappers.
    pub(crate) const SHARED_RUN_INSTRUCTIONS: usize = 6;
    /// How many times more often the stack planner counts a block for each loop around it.
    pub(crate) const LOOP_BLOCK_WEIGHT: u32 = 8;
    /// The most the stack planner counts any block, however deeply nested.
    pub(crate) const MAX_BLOCK_WEIGHT: u32 = 512;
    /// The optimizer runs from which loop transformations that copy a loop's
    /// body, unrolling and peeling, may run: below them, as at the default 200,
    /// a build still values its code size, and the copies grow a contract built
    /// around one loop by half or more.
    pub(crate) const LOOP_COPY_MIN_RUNS: u64 = 10_000;
    /// The most bytes the copies of a completely unrolled loop's body may take, whatever its
    /// pricing, so that a contract near the code size limit cannot grow past it.
    pub(crate) const MAX_LOOP_COPY_BYTES: u32 = 256;
    /// The most bytes one copy of a loop block into a path that jumps to it may take: a latch
    /// and the header test that the paths of a branching loop body then repeat. On the safe
    /// Solady benchmarks the gas saved stops growing past this size, while the runtime corpus
    /// keeps growing.
    pub(crate) const MAX_LOOP_JOIN_COPY_BYTES: u32 = 40;
    /// Below [`Self::LOOP_COPY_MIN_RUNS`], the copies of loop blocks into the paths that jump to
    /// them may add one part in this many of a small module's code.
    pub(crate) const LOOP_JOIN_COPY_SHARE: u64 = 20;
    /// The code size below which a module counts as small when its copies are weighed against
    /// it: a contract built around a loop or two, a sixth of EIP-170's limit.
    pub(crate) const SMALL_CODE_BYTES: u64 = 4_096;

    /// The optimizer runs from which loop unrolling and peeling pay under [`Tune::Jit`], the
    /// default 200: a JIT compiles the copies to straight-line machine code, so an unrolled loop
    /// ran 5% fewer instructions at 3.5% more bytes on the runtime corpus.
    pub(crate) const JIT_LOOP_UNROLL_MIN_RUNS: u64 = Self::DEFAULT_EXPECTED_EXECUTIONS;
    /// The most words a loop may carry across its back edge to be unrolled or peeled under
    /// [`Tune::Jit`]: a JIT keeps a 256-bit word in four registers, and a loop that carries more
    /// spills them to the stack on every iteration, as an unrolled sum over an array did.
    pub(crate) const JIT_MAX_UNROLLED_LOOP_WORDS: usize = 3;

    /// The model of the session's EVM version and optimizer runs for code optimized for
    /// `optimization`, resolved by [`OptimizationMode::select`] after any
    /// `@custom:solar-optimize` tag.
    pub(crate) fn new(gcx: Gcx<'_>, optimization: OptimizationMode) -> Self {
        let opts = &gcx.sess.opts;
        let expected_executions = Self::optimizer_runs(optimization, opts.optimizer_runs);
        Self::with(opts.evm_version, optimization, expected_executions).tuned(opts.unstable.tune)
    }

    /// Resolves the run count used by enabled lifetime-aware optimizations.
    const fn optimizer_runs(optimization: OptimizationMode, runs: Option<u64>) -> u64 {
        if matches!(optimization, OptimizationMode::None) {
            Self::DEFAULT_EXPECTED_EXECUTIONS
        } else if let Some(runs) = runs {
            runs
        } else {
            Self::DEFAULT_EXPECTED_EXECUTIONS
        }
    }

    /// A model with explicit parameters.
    pub(crate) const fn with(
        evm_version: EvmVersion,
        optimization: OptimizationMode,
        expected_executions: u64,
    ) -> Self {
        Self { evm_version, optimization, expected_executions, creation: false, tune: Tune::None }
    }

    /// This model with code tuned for the execution engine `tune`.
    pub(crate) const fn tuned(self, tune: Tune) -> Self {
        Self { tune, ..self }
    }

    /// This model for code that runs only at deployment when `creation` is set, itself
    /// otherwise. Creation code runs once per deployment, and the transaction that carries it
    /// pays calldata gas for every byte, so the gas objective ranks it by deployment gas. Initcode
    /// that a factory embeds is priced the same way, though its bytes then cost the deposit and
    /// it runs on every `CREATE`. The EIP-3860 charge of two gas per 32 bytes of initcode is not
    /// priced, and a zero byte of calldata, which costs 4 gas, counts as a nonzero one. Other
    /// objectives keep the runtime model: size already ranks bytes first, and measurements showed
    /// no clear gain from pricing creation code differently there.
    pub(crate) const fn priced_for(self, creation: bool) -> Self {
        if !creation || !self.optimization.is_gas() {
            return self;
        }
        Self { expected_executions: 1, creation: true, ..self }
    }

    /// This model for code on paths that do not run on success, such as revert and panic
    /// blocks, when `cold` is set, itself otherwise. Extra gas there is not paid by a call that
    /// succeeds, and each byte is deposited anyway, so the gas objective prices cold code like a
    /// size build: bytes first. Size builds already rank bytes first, and `-O none` keeps its
    /// own model.
    pub(crate) const fn priced_for_cold(self, cold: bool) -> Self {
        if !cold || !self.optimization.is_gas() {
            return self;
        }
        Self { optimization: OptimizationMode::Size, ..self }
    }

    /// Whether a bare `@custom:solar-inline` admits a clone that adds `added_bytes` and removes
    /// `removed_bytes`, whatever gas it saves. The gas objective lets such a function add as many
    /// bytes again as the call protocol it removes, past what profitability accepts; size admits
    /// only a clone that does not grow the code, and `-O none` inlines nothing.
    pub(crate) fn inline_hint_admits(&self, added_bytes: usize, removed_bytes: usize) -> bool {
        let allowed = if self.optimization.is_gas() {
            removed_bytes.saturating_mul(2)
        } else {
            removed_bytes
        };
        !self.optimization.is_none() && added_bytes <= allowed
    }

    /// Whether the model prices creation code, which only the gas objective does.
    pub(crate) const fn is_creation(&self) -> bool {
        self.creation
    }

    /// Gas of one deployed byte: calldata for creation code, the code deposit for runtime code.
    pub(crate) const fn byte_gas(&self) -> u32 {
        if !self.creation {
            Self::CODE_DEPOSIT_GAS_PER_BYTE
        } else if since(self.evm_version, EvmVersion::Istanbul) {
            Self::CALLDATA_NONZERO_BYTE_GAS
        } else {
            Self::LEGACY_CALLDATA_NONZERO_BYTE_GAS
        }
    }

    pub(crate) fn evm_version(&self) -> EvmVersion {
        self.evm_version
    }

    pub(crate) fn optimization(&self) -> OptimizationMode {
        self.optimization
    }

    /// Whether the stack planner searches for cheaper plans; `-O none` takes its first plan.
    pub(crate) fn optimizes_stack_plans(&self) -> bool {
        !matches!(self.optimization, OptimizationMode::None)
    }

    pub(crate) fn expected_executions(&self) -> u64 {
        self.expected_executions
    }

    /// The weight of a block inside one more loop than a block of weight `weight`.
    pub(crate) const fn nested_block_weight(weight: u32) -> u32 {
        let weight = weight.saturating_mul(Self::LOOP_BLOCK_WEIGHT);
        if weight < Self::MAX_BLOCK_WEIGHT { weight } else { Self::MAX_BLOCK_WEIGHT }
    }

    /// Whether the build expects enough executions to copy loop bodies for runtime gas.
    pub(crate) fn copies_loops(&self) -> bool {
        self.expected_executions >= Self::LOOP_COPY_MIN_RUNS
    }

    /// Whether the build expects enough executions to unroll and peel loops for runtime gas:
    /// from [`Self::LOOP_COPY_MIN_RUNS`] on, or from [`Self::JIT_LOOP_UNROLL_MIN_RUNS`] under
    /// [`Tune::Jit`].
    pub(crate) fn unrolls_loops(&self) -> bool {
        let min_runs =
            if self.is_jit() { Self::JIT_LOOP_UNROLL_MIN_RUNS } else { Self::LOOP_COPY_MIN_RUNS };
        self.expected_executions >= min_runs
    }

    /// Whether a loop carrying `words` words across its back edge may be unrolled or peeled:
    /// always, except under [`Tune::Jit`], which allows [`Self::JIT_MAX_UNROLLED_LOOP_WORDS`].
    pub(crate) fn unrolls_loop_carrying(&self, words: usize) -> bool {
        !self.is_jit() || words <= Self::JIT_MAX_UNROLLED_LOOP_WORDS
    }

    /// Whether copying a loop block whose code costs `copy`, entered on `paths` paths, into a loop
    /// block that jumps to it pays. In gas builds the copy saves the jump's label push, `JUMP` and
    /// `JUMPDEST` on each iteration through the edge: [`Self::UNCOUNTED_LOOP_ITERATIONS`] per
    /// execution, shared equally by the paths into the block, against the deposit of the copied
    /// bytes beyond the removed label push and `JUMP`. No copy is larger than
    /// [`Self::MAX_LOOP_JOIN_COPY_BYTES`].
    pub(crate) fn duplicates_loop_block(&self, copy: Cost, paths: u32) -> bool {
        if !self.optimization.is_gas() || paths == 0 || copy.bytes > Self::MAX_LOOP_JOIN_COPY_BYTES
        {
            return false;
        }
        let (added, saved) = self.loop_block_copy(copy, paths);
        !self.deposit_repays_gas(added, saved)
    }

    /// The bytes a copy of a loop block whose code costs `copy`, entered on `paths` paths, adds
    /// beyond the label push and `JUMP` it replaces, and the gas it saves on each execution: the
    /// jump on [`Self::UNCOUNTED_LOOP_ITERATIONS`] iterations, shared equally by the paths.
    pub(crate) fn loop_block_copy(&self, copy: Cost, paths: u32) -> (u64, u64) {
        let removed = self.label_push() + self.opcode(op::JUMP);
        let saved =
            u64::from(self.jump().gas) * Self::UNCOUNTED_LOOP_ITERATIONS / u64::from(paths.max(1));
        (u64::from(copy.bytes.saturating_sub(removed.bytes)), saved)
    }

    /// The most bytes the copies of loop blocks may add to a module of `code_bytes` bytes. From
    /// [`Self::LOOP_COPY_MIN_RUNS`] on, and in a module of at least [`Self::SMALL_CODE_BYTES`],
    /// every copy that pays goes. Below those runs, as at the default 200, a build still values
    /// its code size, and in a small module a couple of copies of its one loop grow it by a tenth
    /// or more: they may add one part in [`Self::LOOP_JOIN_COPY_SHARE`] of its code.
    pub(crate) fn loop_join_copy_budget(&self, code_bytes: u64) -> u64 {
        if self.copies_loops() || code_bytes >= Self::SMALL_CODE_BYTES {
            u64::MAX
        } else {
            code_bytes / Self::LOOP_JOIN_COPY_SHARE
        }
    }

    /// Whether a loop rewritten into one copy keeps an optional test of its length before the
    /// copy's setup, which then runs only for a nonzero length, as the loop body did. A test
    /// required to avoid new memory accesses on an empty copy must stay regardless of this cost.
    ///
    /// The test is one branch on every copy. The setup it skips for an empty copy holds the
    /// range checks with their panic branch, any overlap test, and the copy instruction, several
    /// times the branch, and no longer masks its checks with a nonzero length. Gas builds take
    /// that trade; size builds keep the branch's bytes.
    pub(crate) fn tests_empty_copies(&self) -> bool {
        self.optimization.is_gas()
    }

    /// Static gas of one opcode; unknown opcodes are free.
    pub(crate) fn opcode_gas(&self, opcode: u8) -> u32 {
        self.opcode_gas_at(opcode, Warmth::Cold)
    }

    /// Static gas of one opcode touching an account or slot of the given
    /// warmth; unknown opcodes are free.
    pub(crate) fn opcode_gas_at(&self, opcode: u8, warmth: Warmth) -> u32 {
        op::definition(opcode).map_or(0, |def| def.gas.gas_at(self.evm_version, warmth))
    }

    /// Static gas of reading again a location in `space` that an earlier access already touched:
    /// a warm storage slot, a transient slot, a memory word, or an immutable's pushed constant.
    pub(crate) fn repeated_read_gas(&self, space: AddressSpace) -> u32 {
        match space {
            AddressSpace::Storage => self.opcode_gas_at(op::SLOAD, Warmth::Warm),
            AddressSpace::Transient => self.opcode_gas(op::TLOAD),
            AddressSpace::Memory => self.opcode_gas(op::MLOAD),
            AddressSpace::Immutable => self.opcode_gas(op::PUSH32),
        }
    }

    /// Cost of one opcode with its immediate, before dynamic components.
    pub(crate) fn opcode(&self, opcode: u8) -> Cost {
        Cost::new(self.opcode_gas(opcode), opcode_bytes(opcode)).with_steps(1)
    }

    /// Cost of an opcode whose arguments are known in EVM pop order.
    /// Includes operand-dependent work such as the exponent bytes of `EXP`;
    /// memory expansion and state-dependent access prices remain excluded.
    pub(crate) fn opcode_with_immediates(&self, opcode: u8, arguments: &[Option<U256>]) -> Cost {
        let mut cost = self.opcode(opcode);
        if let Some(definition) = op::definition(opcode) {
            cost.gas = cost.gas.saturating_add(
                definition
                    .gas
                    .dynamic_gas(self.evm_version)
                    .saturating_mul(definition.gas.dynamic_units(arguments)),
            );
        }
        cost
    }

    /// Cost of one `DUP`.
    pub(crate) fn dup(&self) -> Cost {
        self.opcode(op::DUP1)
    }

    /// Cost of one logical stack operation after target lowering, or `None` when the target
    /// cannot lower it. An `EXCHANGE` before Amsterdam costs its three swaps.
    pub(crate) fn stack_op(&self, stack_op: op::StackOp) -> Option<Cost> {
        let metrics = stack_op.metrics(self.evm_version)?;
        Some(
            Cost::new(metrics.static_gas as u32, metrics.assembled_len as u32)
                .with_steps(metrics.instruction_count as u32),
        )
    }

    /// Cost of pushing a block label, which resolves to a two-byte push in all but the largest
    /// contracts.
    pub(crate) fn label_push(&self) -> Cost {
        self.opcode(op::PUSH2)
    }

    /// Cost of pushing `value` through the materialization the push selector emits: the one
    /// with the fewest bytes, whatever the tune.
    pub(crate) fn push(&self, value: U256) -> Cost {
        compact_pushes::immediate_materialization(self.evm_version, value)
    }

    /// Cost of pushing `value` as one literal `PUSHn`, the form a push keeps when its compact
    /// recipe does not repay.
    pub(crate) fn literal_push(&self, value: U256) -> Cost {
        compact_pushes::literal_push(self.evm_version, value)
    }

    /// Whether reading `value` as an immediate costs no more gas than a `DUP` and at most one
    /// byte more, as `PUSH0` and `PUSH1` do. A wider constant makes every read it replaces larger.
    pub(crate) fn is_cheap_immediate(&self, value: U256) -> bool {
        let push = self.push(value);
        push.gas <= self.dup().gas && push.bytes <= self.opcode(op::PUSH1).bytes
    }

    /// Expected cost of one MIR operation: its opcode, or a very-low instruction
    /// for operations the backend expands by hand, plus its dynamic work sized
    /// from the immediate operands `immediate` resolves, or one unit otherwise.
    pub(crate) fn op(&self, op: &Op, immediate: impl Fn(ValueId) -> Option<U256>) -> Cost {
        self.op_at(op, immediate, Warmth::Cold)
    }

    /// Cost of one operation whose account or storage access has the given
    /// warmth, with dynamic work sized from immediate operands.
    pub(crate) fn op_at(
        &self,
        op: &Op,
        immediate: impl Fn(ValueId) -> Option<U256>,
        warmth: Warmth,
    ) -> Cost {
        if let Op::Trunc { bits, .. } | Op::PtrToInt { bits, .. } = *op {
            return if bits < 256 {
                self.push(U256::MAX >> (256 - bits)) + self.opcode(op::AND)
            } else {
                Cost::ZERO
            };
        }
        if let Op::Sext { from_bits, to_bits, .. } = *op {
            if from_bits == 1 {
                return if to_bits == 256 {
                    self.push(U256::ZERO) + self.opcode(op::SUB)
                } else if to_bits == 160 && self.evm_version.has_bitwise_shifting() {
                    self.push(U256::ZERO)
                        + self.opcode(op::SUB)
                        + self.push(U256::from(96))
                        + self.opcode(op::SHR)
                } else if to_bits < 256 {
                    self.push(U256::MAX >> (256 - to_bits)) + self.opcode(op::MUL)
                } else {
                    self.push(U256::MAX) + self.opcode(op::MUL)
                };
            }
            let extension = if from_bits != 0 && from_bits.is_multiple_of(8) && from_bits <= 256 {
                self.push(U256::from(from_bits / 8 - 1)) + self.opcode(op::SIGNEXTEND)
            } else {
                let shift = self.push(U256::from(256_u32.saturating_sub(from_bits)));
                shift + shift + self.opcode(op::SHL) + self.opcode(op::SAR)
            };
            return extension
                + if to_bits < 256 {
                    self.push(U256::MAX >> (256 - to_bits)) + self.opcode(op::AND)
                } else {
                    Cost::ZERO
                };
        }
        if matches!(op, Op::Zext { .. } | Op::IntToPtr { .. }) {
            return Cost::ZERO;
        }
        if matches!(op, Op::Eq { .. } | Op::Ne { .. }) {
            let comparison =
                if Self::zero_test_input(op, &immediate).is_some() { op::ISZERO } else { op::EQ };
            return self.opcode(comparison)
                + if matches!(op, Op::Ne { .. }) { self.opcode(op::ISZERO) } else { Cost::ZERO };
        }
        let Some(lowering) = select::opcode_lowering(op) else {
            return Cost::new(GasTier::VeryLow.gas(self.evm_version), 1).with_steps(1);
        };
        let tier = op::definition(lowering.opcode()).map_or(GasTier::VeryLow, |def| def.gas);
        let static_gas = tier.gas_at(self.evm_version, warmth);
        let dynamic_gas = tier.dynamic_gas(self.evm_version);
        if dynamic_gas == 0 {
            return Cost::new(static_gas, 1).with_steps(1);
        }
        let mut arguments = SmallVec::<[Option<U256>; 8]>::new();
        // Only the visit matters; the mapped copy is discarded.
        let _ = op.map_values(|value| {
            arguments.push(immediate(value));
            value
        });
        let dynamic = dynamic_gas.saturating_mul(tier.dynamic_units(&arguments));
        Cost::new(static_gas.saturating_add(dynamic), 1).with_steps(1)
    }

    /// Whether an opcode-lowered MIR operation's gas depends on operands or access warmth.
    pub(crate) fn op_has_variable_gas(&self, op: &Op) -> bool {
        select::opcode_lowering(op)
            .and_then(|lowering| op::definition(lowering.opcode()))
            .is_some_and(|definition| {
                let tier = definition.gas;
                tier.dynamic_gas(self.evm_version) != 0
                    || tier.gas_at(self.evm_version, Warmth::Cold)
                        != tier.gas_at(self.evm_version, Warmth::Warm)
            })
    }

    /// Returns the sole materialized operand of an equality or inequality with zero.
    pub(crate) fn zero_test_input(
        op: &Op,
        immediate: impl Fn(ValueId) -> Option<U256>,
    ) -> Option<ValueId> {
        let (Op::Eq { a, b } | Op::Ne { a, b }) = *op else { return None };
        if immediate(a) == Some(U256::ZERO) {
            Some(b)
        } else if immediate(b) == Some(U256::ZERO) {
            Some(a)
        } else {
            None
        }
    }

    /// Cost of the generic Select emitter, excluding initial operand placement.
    pub(crate) fn select(&self, normalize_condition: bool) -> Cost {
        // dup3; swap1; sub; mul; add.
        let sequence = [op::DUP3, op::SWAP1, op::SUB, op::MUL, op::ADD]
            .into_iter()
            .map(|opcode| self.opcode(opcode))
            .sum::<Cost>();
        if normalize_condition { sequence + self.opcode(op::ISZERO) } else { sequence }
    }

    /// Gas of copying one more word with a copy opcode.
    pub(crate) fn copy_word_gas(&self) -> u32 {
        GasTier::Copy.dynamic_gas(self.evm_version)
    }

    /// Word gas of copying `size` bytes with a copy opcode, beyond its static gas.
    pub(crate) fn copied_words(&self, size: usize) -> Cost {
        let words = u32::try_from(size.div_ceil(32)).unwrap_or(u32::MAX);
        Cost::new(self.copy_word_gas().saturating_mul(words), 0)
    }

    /// Cost of one internal call site with its return landing: the pushed
    /// return label, the jump, the landing, and the frame protocol moving
    /// `args` arguments in and `returns` results out, whose gas grows with
    /// the callee's frame of `frame_words` words.
    ///
    /// The gas and bytes are an empirical fit. Its instructions follow from the gas: the label
    /// push, jump, and landing count exactly, and every further 3 gas, the price of the `DUP`,
    /// `PUSH`, `SWAP`, `MLOAD`, and `MSTORE` that make up the rest of the protocol, is one
    /// instruction.
    pub(crate) fn icall(&self, args: usize, returns: usize, frame_words: u64) -> Cost {
        let moved = (args + returns) as u32;
        let protocol = 90 + moved * 24 + u32::try_from(frame_words).unwrap_or(u32::MAX) * 6;
        Cost::new(protocol, 18 + moved * 5).with_steps(self.protocol_steps(protocol, self.jump()))
    }

    /// Cost of one internal return: the jump back through the pushed label
    /// and the protocol handing `returns` results past `params` parameters.
    /// Its instructions follow from the gas as in [`Self::icall`], with the jump counted exactly.
    pub(crate) fn internal_return(&self, params: usize, returns: usize) -> Cost {
        let moved = (params + returns) as u32;
        let protocol = 24 + moved * 8;
        Cost::new(protocol, 8 + moved * 4)
            .with_steps(self.protocol_steps(protocol, self.opcode(op::JUMP)))
    }

    /// Instructions of a fitted protocol of `gas`, which includes the exactly known sequence
    /// `known`: its own instructions, and one for each 3 gas beyond it.
    fn protocol_steps(&self, gas: u32, known: Cost) -> u32 {
        known.steps + gas.saturating_sub(known.gas).div_ceil(GasTier::VeryLow.gas(self.evm_version))
    }

    /// Prices sharing one helper with `body_bytes` of body, `params` parameters, and `returns`
    /// results among `sites` call sites against expanding the body at every site, as
    /// `(shared, expanded)`. The body runs in both shapes, so sharing adds the call protocol's
    /// gas at every site. Its bytes are what a call emits: each site pushes its return label,
    /// jumps to the helper, lands on return, and moves each argument and result once; the helper
    /// adds one landing, one copy of the body, and a return jump after moving its results.
    pub(crate) fn helper_sharing(
        &self,
        body_bytes: u32,
        params: usize,
        returns: usize,
        sites: u32,
    ) -> (Cost, Cost) {
        let frame_words = EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE / EvmMemoryLayout::WORD_SIZE
            + (params + returns) as u64;
        let protocol =
            self.icall(params, returns, frame_words) + self.internal_return(params, returns);
        let moves = self.dup().times((params + returns) as u32);
        let call = self.label_push() + self.jump() + moves;
        let ret =
            self.opcode(op::JUMPDEST) + self.dup().times(returns as u32) + self.opcode(op::JUMP);
        let shared = Cost::new(protocol.gas, call.bytes)
            .with_steps(protocol.steps)
            .times(sites)
            .plus(Cost::new(0, body_bytes.saturating_add(ret.bytes)));
        (shared, Cost::new(0, body_bytes).times(sites))
    }

    /// Estimated code of a function body: every operation with the pushes
    /// of its immediates, and a pushed label with a jump and a landing per
    /// edge. Sizes the shape of a helper before deciding to share it.
    pub(crate) fn code_estimate(&self, func: &Function) -> Cost {
        func.blocks.indices().map(|block| self.block_code_estimate(func, block)).sum()
    }

    /// Estimated code of one block, priced as in [`Self::code_estimate`].
    pub(crate) fn block_code_estimate(&self, func: &Function, block: BlockId) -> Cost {
        let mut cost = self.block_instructions_estimate(func, block);
        let block = &func.blocks[block];
        if let Some(terminator) = &block.terminator {
            // An unconditional edge does not pay the conditional jump's higher gas price.
            // Other CFG edges retain the conservative conditional-edge estimate.
            if matches!(terminator, Terminator::Jump(_)) {
                cost += self.jump();
            } else {
                for _ in 0..terminator.successors().len() {
                    cost += self.branch();
                }
            }
        }
        cost
    }

    /// Estimated code of the instructions of one block with the pushes of their immediates,
    /// without its terminator.
    pub(crate) fn block_instructions_estimate(&self, func: &Function, block: BlockId) -> Cost {
        func.blocks[block]
            .instructions
            .iter()
            .map(|&inst| self.instruction_estimate(func, inst))
            .sum()
    }

    /// Estimated code of one instruction: its operation and the pushes of its immediate operands.
    pub(crate) fn instruction_estimate(&self, func: &Function, inst: InstId) -> Cost {
        let kind = &func.inst(inst).kind;
        let immediate = |value| match func.value(value) {
            Value::Immediate(immediate) => immediate.as_u256(),
            _ => None,
        };
        let mut cost = self.op(&kind.op(), immediate);
        kind.visit_operands(|operand| {
            if let Some(value) = immediate(operand) {
                cost += self.push(value);
            }
        });
        cost
    }

    /// Whether indexed jump tables may pack entries wider than one byte into words and pick them
    /// with a shift and a mask, when `requested` by `-Zpack-wide-jump-tables`. A bytecode JIT
    /// cannot resolve the pick, and one unresolved jump makes it drop every jump it resolved in
    /// the contract.
    pub(crate) const fn packs_wide_jump_tables(&self, requested: bool) -> bool {
        requested && !self.is_jit()
    }

    /// Whether the code is tuned for a bytecode JIT.
    const fn is_jit(&self) -> bool {
        matches!(self.tune, Tune::Jit)
    }

    /// Whether the engine pays for the dispatch of each executed instruction.
    const fn charges_dispatch(&self) -> bool {
        matches!(self.tune, Tune::Interpreter)
    }

    /// The runtime gas every ranking of `cost` uses: its static gas, plus under
    /// [`Tune::Interpreter`] the dispatch of each executed instruction.
    pub(crate) fn runtime_gas(&self, cost: Cost) -> u32 {
        if self.charges_dispatch() {
            cost.gas.saturating_add(cost.steps.saturating_mul(Self::INTERPRETER_DISPATCH_GAS))
        } else {
            cost.gas
        }
    }

    /// Whether `a` costs no more than `b` in every dimension the model ranks and less in one.
    /// Executed instructions count only when the engine dispatches them.
    pub(crate) fn dominates(&self, a: Cost, b: Cost) -> bool {
        let ranked = |cost: Cost| {
            let steps = if self.charges_dispatch() { cost.steps } else { 0 };
            (cost.gas, cost.bytes, steps)
        };
        let (a, b) = (ranked(a), ranked(b));
        a.0 <= b.0 && a.1 <= b.1 && a.2 <= b.2 && a != b
    }

    /// Deployment-lifetime gas of `cost`: expected executions of its runtime
    /// gas plus the price of its bytes.
    pub(crate) fn lifetime_gas(&self, cost: Cost) -> u128 {
        u128::from(self.runtime_gas(cost)) * u128::from(self.expected_executions)
            + u128::from(cost.bytes) * u128::from(self.byte_gas())
    }

    /// The objective order of a cost: the dimension optimized first, then
    /// the other one. Gas leads when optimizing for gas, bytes when
    /// optimizing for size. Creation code ranks by its deployment gas, execution plus
    /// calldata, when optimizing for gas.
    pub(crate) fn objective_key(&self, cost: Cost) -> [u32; 2] {
        match self.optimization {
            OptimizationMode::Size => [cost.bytes, self.runtime_gas(cost)],
            _ if self.creation => {
                [u32::try_from(self.lifetime_gas(cost)).unwrap_or(u32::MAX), cost.bytes]
            }
            _ => [self.runtime_gas(cost), cost.bytes],
        }
    }

    /// Ranks two costs under the objective.
    pub(crate) fn cmp(&self, a: Cost, b: Cost) -> Ordering {
        self.objective_key(a).cmp(&self.objective_key(b))
    }

    /// Ranks two costs over the deployment lifetime when optimizing for gas, and
    /// under the objective otherwise.
    pub(crate) fn cmp_lifetime(&self, a: Cost, b: Cost) -> Ordering {
        if !self.optimization.is_gas() {
            return self.cmp(a, b);
        }
        self.lifetime_gas(a).cmp(&self.lifetime_gas(b)).then_with(|| self.cmp(a, b))
    }

    /// Whether depositing `bytes` fewer bytes outweighs spending `gas` more gas
    /// on each expected execution.
    pub(crate) fn deposit_repays_gas(self, bytes: u64, gas: u64) -> bool {
        u128::from(bytes) * u128::from(self.byte_gas())
            > u128::from(gas) * u128::from(self.expected_executions)
    }

    /// Whether a compact recipe for an immediate, `saved_bytes` shorter than its literal push and
    /// `extra_gas` dearer to run, is the better code. Size builds, and cold code priced by
    /// [`Self::priced_for_cold`], always take the bytes.
    /// Gas and `-O none` builds take them when the deposit they save covers the extra gas over
    /// the expected executions, [`Self::UNCOUNTED_LOOP_ITERATIONS`] times over for a push inside a
    /// loop, so a hot loop pushes a mask as a literal rather than rebuilding it on every
    /// iteration; a tie takes the bytes, which cost no more over the deployment's lifetime.
    ///
    /// The push is priced over at most [`Self::DEFAULT_EXPECTED_EXECUTIONS`]: it runs only on the
    /// calls whose path reaches it, a fraction of the deployment's calls once they spread over its
    /// entry points. Priced at the full count, a contract built for 1,000 runs keeps each
    /// left-aligned string literal as a `PUSH32`, 26 bytes longer than a shifted `PUSH3`, to save
    /// 6 gas on the calls that reach it, and each `not(31)` of its loops as a `PUSH32`, 30 bytes
    /// longer than `PUSH1 31 NOT`, to save 3 gas an iteration.
    pub(crate) fn compacts_immediate(
        &self,
        saved_bytes: u64,
        extra_gas: u64,
        in_loop: bool,
    ) -> bool {
        if self.optimization.is_size() {
            return true;
        }
        let iterations = if in_loop { Self::UNCOUNTED_LOOP_ITERATIONS } else { 1 };
        let executions = self.expected_executions.min(Self::DEFAULT_EXPECTED_EXECUTIONS);
        saved_bytes != 0
            && u128::from(saved_bytes) * u128::from(self.byte_gas())
                >= u128::from(extra_gas) * u128::from(iterations) * u128::from(executions)
    }

    /// Whether a change that saves `gas_saving` gas and `byte_saving` bytes
    /// per site improves the objective; negative savings are growth.
    pub(crate) fn improves(&self, gas_saving: i64, byte_saving: i64) -> bool {
        if self.creation {
            i128::from(gas_saving) + i128::from(byte_saving) * i128::from(self.byte_gas()) > 0
        } else if self.optimization.is_gas() {
            gas_saving > 0 && byte_saving >= 0
        } else {
            byte_saving > 0
        }
    }

    /// Whether code that cannot reach a normal exit counts as cold, as do the code after a call
    /// and the edges into cold blocks. Marking it moves failure loops and the code around them
    /// out of line, which saves gas on the paths that succeed. The move saves no bytes, and the
    /// layout it changes can widen jump labels, so a size build leaves that code in place and
    /// moves only the blocks that abort or lead only to aborts.
    pub(crate) const fn marks_failure_paths_cold(&self) -> bool {
        self.optimization.is_gas()
    }

    /// Cost of a jump to a block, including the pushed label and the target's landing.
    pub(crate) fn jump(&self) -> Cost {
        self.label_push() + self.opcode(op::JUMP) + self.opcode(op::JUMPDEST)
    }

    /// Cost of a conditional edge, including the pushed label and the target's landing.
    pub(crate) fn branch(&self) -> Cost {
        self.label_push() + self.opcode(op::JUMPI) + self.opcode(op::JUMPDEST)
    }

    /// Whether turning the branch `PUSH next; JUMPI; PUSH other; JUMP` around into `ISZERO; PUSH
    /// other; JUMPI`, falling into the `next` block that follows it, is the better code, when the
    /// branch enters `next` `next_runs` times for every `other_runs` times it enters `other`.
    /// `test` is what the test becomes, and the `JUMPDEST` of `next` goes unless another
    /// reference keeps it (`keeps_landing`).
    ///
    /// The turned branch runs the test on both edges but jumps to `other` once, so it saves gas
    /// when `other` is entered often enough. A gas build also leaves a branch whose `next` is the
    /// hot edge of a loop, even when the test is free: falling into the loop body keeps the loop
    /// passes from rotating the header into its latch, which saves a jump on every iteration.
    /// Size builds take the bytes unless the extra gas of a hot loop costs more over the expected
    /// executions than the deposit of the bytes saved.
    pub(crate) fn falls_through_turned_branch(
        &self,
        next_runs: u64,
        other_runs: u64,
        test: TurnedTest,
        keeps_landing: bool,
    ) -> bool {
        let price = |cost: Cost| (i64::from(self.runtime_gas(cost)), i64::from(cost.bytes));
        let (iszero_gas, iszero_bytes) = price(self.opcode(op::ISZERO));
        let (test_gas, test_bytes) = match test {
            TurnedTest::Adds => (iszero_gas, iszero_bytes),
            TurnedTest::Drops => (-iszero_gas, -iszero_bytes),
            TurnedTest::Swaps => (0, 0),
        };
        let (jump_gas, jump_bytes) = price(self.label_push() + self.opcode(op::JUMP));
        let (freed_gas, freed_bytes) =
            if keeps_landing { (0, 0) } else { price(self.opcode(op::JUMPDEST)) };
        // Entering `next` pays the test, where it paid the landing if that goes.
        let next_extra = test_gas - freed_gas;
        // Entering `other` pays the test, where it paid a second transfer.
        let other_saving = jump_gas - test_gas;
        let gas_saving = i128::from(other_runs) * i128::from(other_saving)
            - i128::from(next_runs) * i128::from(next_extra);
        let gas_saving = gas_saving.clamp(i64::MIN.into(), i64::MAX.into()) as i64;
        let byte_saving = jump_bytes + freed_bytes - test_bytes;
        let loop_continues = other_runs != 0 && next_runs > other_runs;
        self.improves(gas_saving, byte_saving)
            && !(loop_continues && self.optimization.is_gas())
            && (gas_saving >= 0
                || !self.optimization.is_size()
                || self.deposit_repays_gas(byte_saving.unsigned_abs(), gas_saving.unsigned_abs()))
    }

    /// Whether encoding static calldata elements of `element_words` words keeps a block copy
    /// behind a wrap guard, with the per-word loop as the fallback, rather than running the
    /// loop alone. Solc loads each word at its own address, and a block copy reads zeros where
    /// the range crosses 2^256, so the guard sends only a range that wraps to the loop. The
    /// guard adds a copy and a branch to code that holds the loop already, and one element
    /// is priced: the copy must save gas on it to pay for those bytes, which only the gas
    /// objective counts.
    pub(crate) fn guards_block_copy(&self, element_words: u64) -> bool {
        let words = u32::try_from(element_words).unwrap_or(u32::MAX);
        // One iteration: a load, a store, and the address step for each word, and the branch.
        let step = self.opcode(op::CALLDATALOAD) + self.opcode(op::MSTORE) + self.opcode(op::ADD);
        let looped = step.times(words) + self.branch();
        // Overflow test of the range end, the branch around the loop, and the copy.
        let guard = self.opcode(op::ADD) + self.opcode(op::LT) + self.branch();
        let copy = self.opcode(op::CALLDATACOPY)
            + self.copied_words(usize::try_from(element_words.saturating_mul(32)).unwrap_or(0));
        // The loop stays in the guarded shape as the fallback, so only its bytes remain.
        let guarded = Cost::new(guard.gas + copy.gas, guard.bytes + copy.bytes + looped.bytes);
        self.cmp(guarded, looped).is_lt()
    }
    /// The factor `2**shift` that a constant shift lowered to `MUL` or `DIV` pushes, or `None`
    /// when computing it with `EXP` costs less over the deployment lifetime. Shifts of 256 or
    /// more give a zero factor, which matches the wrapped `EXP` result.
    pub(crate) fn legacy_shift_factor(&self, shift: U256) -> Option<U256> {
        let two = U256::from(2);
        let factor =
            if shift < U256::from(256) { U256::ONE << shift.to::<usize>() } else { U256::ZERO };
        let folded = self.opcode(op::push(factor.byte_len().max(1) as u8));
        let computed = self.push(shift)
            + self.push(two)
            + self.opcode_with_immediates(op::EXP, &[Some(two), Some(shift)]);
        self.cmp_lifetime(folded, computed).is_le().then_some(factor)
    }

    /// Whether legacy splat expansion beats the selected ordinary lowering in size.
    /// For word stores, the first two stores are common; price each remaining materialized
    /// word, offset, store, and stack moves. For a data copy, include the pool and the splat's
    /// two seed stores. Each copy pays for its own loop, including setup and stack moves,
    /// without relying on helper sharing or later unrolling.
    pub(crate) fn legacy_splat_is_profitable(
        &self,
        word: U256,
        size: usize,
        data_copy: bool,
    ) -> bool {
        let word_size = op::WORD_BYTES;
        let loop_body = self.word_copy_loop();
        let mut copies = Cost::ZERO;
        let mut filled = 2 * word_size;
        while filled < size {
            let chunk = filled.min(size - filled);
            copies += loop_body + self.push(U256::from(filled)) + self.push(U256::from(chunk));
            filled += chunk;
        }
        if data_copy {
            let seed = self.push(word)
                + self.dup().times(3)
                + self.opcode(op::MSTORE).times(2)
                + self.push(U256::from(word_size))
                + self.opcode(op::ADD);
            let copy = self.push(U256::from(size))
                + self.opcode(op::PUSH3)
                + self.dup()
                + self.opcode(op::CODECOPY);
            ((copies + seed).bytes as usize) < size.saturating_add(copy.bytes as usize)
        } else {
            let stores = (2..size / word_size)
                .map(|index| {
                    self.push(word)
                        + self.push(U256::from(index * word_size))
                        + self.dup().times(2)
                        + self.opcode(op::ADD)
                        + self.opcode(op::MSTORE)
                })
                .sum::<Cost>();
            copies.bytes < stores.bytes
        }
    }

    /// One iteration of a word-by-word memory copy loop, with its counter and cursor moves.
    fn word_copy_loop(&self) -> Cost {
        self.jump()
            + self.branch()
            + self.dup().times(7)
            + self.opcode(op::ADD).times(4)
            + self.opcode(op::LT)
            + self.opcode(op::MLOAD)
            + self.opcode(op::MSTORE)
            + self.opcode(op::POP).times(3)
            + self.push(U256::ZERO)
            + self.push(U256::from(op::WORD_BYTES))
    }

    /// Whether an ABI encoding writes a literal byte string as stores of its own words, beside
    /// the preserved object that already holds it, instead of copying that object. `bytes`
    /// reads the literal, which only the size objective needs. `removes_original` says nothing
    /// reads the object afterwards, so folding also drops its allocation and stores. Gas ranks
    /// the stores above the copy loop that runs before MCOPY and below the copy instruction
    /// that follows it. Size compares the stores with the length computation, padding store,
    /// and copy they replace, and folds whenever the object goes away, since the stores then
    /// only move.
    pub(crate) fn folds_preserved_literal(
        &self,
        removes_original: bool,
        bytes: impl FnOnce() -> Option<Vec<u8>>,
    ) -> bool {
        match self.optimization {
            OptimizationMode::Gas => !self.evm_version.has_mcopy(),
            OptimizationMode::Size => {
                if removes_original {
                    return true;
                }
                let Some(bytes) = bytes() else { return false };
                let word_size = op::WORD_BYTES;
                let size = U256::from(bytes.len().next_multiple_of(word_size));
                // mstore dest, len; mstore dest + 32 + 32 * i, word; tail = dest + 32 + size
                let folded = self.push(U256::from(bytes.len()))
                    + self.dup()
                    + self.opcode(op::MSTORE)
                    + bytes
                        .chunks(word_size)
                        .map(|chunk| {
                            let mut word = [0; op::WORD_BYTES];
                            word[..chunk.len()].copy_from_slice(chunk);
                            self.push(U256::from_be_bytes(word))
                                + self.push(U256::from(word_size))
                                + self.dup()
                                + self.opcode(op::ADD)
                                + self.opcode(op::MSTORE)
                        })
                        .sum::<Cost>()
                    + self.push(size)
                    + self.opcode(op::ADD);
                // len = mload object; padded = (len + 31) & ~31; mstore dest + padded, 0;
                // mstore dest, len; copy data to dest + 32; tail = dest + 32 + padded
                let copy = if self.evm_version.has_mcopy() {
                    self.opcode(op::MCOPY)
                } else {
                    self.word_copy_loop()
                };
                let copied = self.dup()
                    + self.opcode(op::MLOAD)
                    + self.push(U256::from(word_size - 1)).times(2)
                    + self.opcode(op::ADD)
                    + self.opcode(op::NOT)
                    + self.opcode(op::AND)
                    + (self.dup() + self.opcode(op::ADD)).times(2)
                    + self.push(U256::ZERO)
                    + (self.dup() + self.opcode(op::MSTORE)).times(2)
                    + (self.push(U256::from(word_size)) + self.opcode(op::ADD)).times(2)
                    + self.dup().times(3)
                    + copy;
                folded.bytes < copied.bytes
            }
            _ => false,
        }
    }

    /// Whether a `KECCAK256` over constant words, whose result `uses` operands read, becomes the
    /// immediate `hash` at each use. `keccak` is the hash with its operands and `stores` the
    /// constant stores the fold leaves dead, which only they read. The hash saves gas on every
    /// execution but costs a `PUSH32` per use where the result would stay on the stack and be
    /// duplicated, so it needs the stores to pay for those bytes. Inside a loop, the gas saved
    /// on [`Self::UNCOUNTED_LOOP_ITERATIONS`] iterations may repay the deposit of the bytes
    /// instead.
    pub(crate) fn folds_constant_keccak(
        &self,
        hash: U256,
        keccak: Cost,
        stores: Cost,
        uses: u32,
        in_loop: bool,
    ) -> bool {
        let kept = keccak + stores + self.dup().times(uses.saturating_sub(1));
        let folded = self.push(hash).times(uses);
        let gas_saving = i64::from(self.runtime_gas(kept)) - i64::from(self.runtime_gas(folded));
        let byte_saving = i64::from(kept.bytes) - i64::from(folded.bytes);
        self.improves(gas_saving, byte_saving)
            || in_loop
                && self.optimization.is_gas()
                && gas_saving > 0
                && !self.deposit_repays_gas(
                    byte_saving.unsigned_abs(),
                    gas_saving.unsigned_abs() * Self::UNCOUNTED_LOOP_ITERATIONS,
                )
    }

    /// Whether one shared copy of a terminal block, entered by a jump from each of `sites`,
    /// takes fewer bytes than a copy at every site. When optimizing for size, a copy no larger
    /// than a jump to and from a shared body stays with the backend, whose terminal-block sharing
    /// and outlining handle it in its final layout; sharing it earlier grew the measured code.
    pub(crate) fn shares_terminal_copy(&self, copy: Cost, sites: u32) -> bool {
        let jump = self.label_push() + self.opcode(op::JUMP);
        if matches!(self.optimization, OptimizationMode::Size) && copy.bytes <= jump.times(2).bytes
        {
            return false;
        }
        (jump.times(sites) + copy + self.opcode(op::JUMPDEST)).bytes < copy.times(sites).bytes
    }

    /// Whether code shared from a site inside a loop pays for itself when the bytes it saves at
    /// that site, `saved_bytes`, cost `transfer_gas` more on every iteration: the deposit of the
    /// bytes against the transfers over the expected executions,
    /// [`Self::UNCOUNTED_LOOP_ITERATIONS`] times per execution. This holds in size builds too,
    /// which otherwise take the bytes, so a hot loop keeps code that only its size made worth
    /// sharing.
    pub(crate) fn shares_in_loop(&self, saved_bytes: u64, transfer_gas: u64) -> bool {
        self.deposit_repays_gas(
            saved_bytes,
            transfer_gas.saturating_mul(Self::UNCOUNTED_LOOP_ITERATIONS),
        )
    }

    /// Cost of `words` words of a function's fixed spill frame on each call of an external entry
    /// that can reach the function. The frames of the functions an entry reaches lie below its
    /// heap, so each word moves the heap up by one, and every call expands memory over the word
    /// once its first allocation does, whether or not the function runs. The stores and reloads
    /// that use the word are priced with the code that runs them.
    pub(crate) fn frame_words(&self, words: u32) -> Cost {
        Cost::new(GasTier::MEMORY_WORD_GAS.saturating_mul(words), 0)
    }

    /// Bytes saved when an outlined body's suffix and return share an existing return tail.
    /// One copy replaces the suffix, the output rotations and the return jump with a jump to
    /// the shared tail. Charge its pushed label, jump and additional landing.
    pub(crate) fn shared_return_tail_savings(&self, suffix_bytes: u32, outputs: u16) -> u32 {
        suffix_bytes
            .saturating_add(self.opcode(op::SWAP1).times(u32::from(outputs)).bytes)
            .saturating_add(self.opcode(op::JUMP).bytes)
            .saturating_sub(self.jump().bytes)
    }

    /// Cost of a zero read from `RETURNDATASIZE`, which holds zero until the frame runs a call or
    /// create, or `None` when it does not dominate the literal zero push: Shanghai and later push
    /// zero at the same price, and earlier targets before Byzantium lack the opcode. Pricing that
    /// ignores the frame's calls, as the selector's and every pass before the late
    /// `returndata-zero` pass do, keeps the literal price, which bounds the final cost from above.
    pub(crate) fn returndata_zero(&self) -> Option<Cost> {
        let cost = self.opcode(op::RETURNDATASIZE);
        (self.evm_version.supports_returndata()
            && self.dominates(cost, self.encoded_push(U256::ZERO)))
        .then_some(cost)
    }

    /// A single encoded PUSH, without the arithmetic recipes used for materialized constants.
    /// Labels and already selected EVM IR immediates use this encoding during assembly.
    pub(crate) fn encoded_push(&self, value: U256) -> Cost {
        if value.is_zero() && self.evm_version.has_push0() {
            self.opcode(op::PUSH0)
        } else {
            self.opcode(op::push(value.byte_len().max(1) as u8))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{Function, Immediate, InstKind, Instruction, MirType, PtrLocation, Value};
    use solar_interface::Ident;
    use std::fmt::Write;

    /// Export actual target prices for the offline rule search. The search never
    /// maintains its own gas schedule or guesses constant materialization costs.
    #[test]
    fn word_rule_costs() {
        let mut table = format!("deposit {}\n", Target::CODE_DEPOSIT_GAS_PER_BYTE);
        for version in [
            EvmVersion::Homestead,
            EvmVersion::TangerineWhistle,
            EvmVersion::SpuriousDragon,
            EvmVersion::Byzantium,
            EvmVersion::Constantinople,
            EvmVersion::Petersburg,
            EvmVersion::Istanbul,
            EvmVersion::Berlin,
            EvmVersion::London,
            EvmVersion::Paris,
            EvmVersion::Shanghai,
            EvmVersion::Cancun,
            EvmVersion::Prague,
            EvmVersion::Osaka,
            EvmVersion::Amsterdam,
        ] {
            let target =
                Target::with(version, OptimizationMode::Gas, Target::DEFAULT_EXPECTED_EXECUTIONS);
            writeln!(table, "fork {version}").unwrap();
            let mut constants = vec![U256::ZERO, U256::ONE, U256::MAX];
            constants.extend((0..32).map(U256::from));
            constants
                .extend([2, 7, 8, 16, 31, 32, 64, 128, 160, 224, 248, 255, 256].map(U256::from));
            for bits in [8, 16, 32, 64, 128, 160, 224, 248, 255] {
                constants.extend([
                    U256::ONE << bits,
                    (U256::ONE << bits) - U256::ONE,
                    U256::MAX << bits,
                ]);
            }
            constants.sort_unstable();
            constants.dedup();
            for value in constants {
                let cost = target.push(value);
                writeln!(table, "constant {value} {} {}", cost.gas, cost.bytes).unwrap();
            }
            let duplicate = target.dup();
            writeln!(table, "variable {} {}", duplicate.gas, duplicate.bytes).unwrap();
            for opcode in u8::MIN..=u8::MAX {
                if let Some(def) = op::definition(opcode)
                    && def.is_available(version)
                    && def.is_pure()
                    && def.gas.dynamic_gas(version) == 0
                    && let Some((inputs, 1)) = def.stack_io
                {
                    let cost = target.opcode(opcode);
                    writeln!(
                        table,
                        "op {} {inputs} {} {} {}",
                        def.mnemonic,
                        cost.gas,
                        cost.bytes,
                        if def.is_commutative() { "commutative" } else { "ordered" }
                    )
                    .unwrap();
                }
            }
        }
        snapbox::assert_data_eq!(table, snapbox::file!["word_rule_costs.snap"]);
    }

    #[test]
    fn schedule_is_monotonic_and_fixed_tiers_agree() {
        const VERSIONS: [EvmVersion; 4] = [
            EvmVersion::Homestead,
            EvmVersion::Istanbul,
            EvmVersion::Berlin,
            EvmVersion::Amsterdam,
        ];
        for tier in [GasTier::Balance, GasTier::ExtCode, GasTier::SLoad, GasTier::Call] {
            let mut previous = 0;
            for version in VERSIONS {
                assert!(tier.gas(version) >= previous, "{tier:?} regresses at {version}");
                previous = tier.gas(version);
            }
        }
        for tier in [GasTier::Base, GasTier::VeryLow, GasTier::Mid, GasTier::High] {
            assert_eq!(tier.fixed_gas(), tier.gas(EvmVersion::Amsterdam));
        }
    }

    #[test]
    fn warm_accesses_are_repriced_since_berlin() {
        for tier in [GasTier::Balance, GasTier::ExtCode, GasTier::ExtCodeHash, GasTier::SLoad] {
            assert_eq!(tier.gas_at(EvmVersion::Berlin, Warmth::Warm), GasTier::WARM_ACCESS_GAS);
            assert_eq!(tier.gas_at(EvmVersion::Osaka, Warmth::Warm), GasTier::WARM_ACCESS_GAS);
            assert_eq!(
                tier.gas_at(EvmVersion::Istanbul, Warmth::Warm),
                tier.gas(EvmVersion::Istanbul)
            );
            assert_eq!(tier.gas_at(EvmVersion::Osaka, Warmth::Cold), tier.gas(EvmVersion::Osaka));
        }
        assert_eq!(GasTier::Call.gas_at(EvmVersion::Osaka, Warmth::Warm), 100);
        assert_eq!(GasTier::SStore.gas_at(EvmVersion::Osaka, Warmth::Warm), 2900);
        assert_eq!(GasTier::SStore.gas_at(EvmVersion::Istanbul, Warmth::Warm), 5000);
        assert_eq!(GasTier::VeryLow.gas_at(EvmVersion::Osaka, Warmth::Warm), 3);
        let target = Target::with(EvmVersion::Osaka, OptimizationMode::Gas, 200);
        assert_eq!(target.opcode_gas_at(op::SLOAD, Warmth::Warm), 100);
        assert_eq!(target.opcode_gas_at(op::SLOAD, Warmth::Cold), 2100);
        let mut function = Function::new(Ident::DUMMY);
        let slot = function.alloc_value(Value::Immediate(Immediate::I256(U256::ZERO)));
        let load = InstKind::Load(slot, PtrLocation::Storage).op();
        assert_eq!(target.op_at(&load, |_| None, Warmth::Warm), Cost::new(100, 1).with_steps(1));
        assert_eq!(target.op(&load, |_| None), Cost::new(2100, 1).with_steps(1));
        for version in [EvmVersion::Istanbul, EvmVersion::Berlin, EvmVersion::Osaka] {
            let target = Target::with(version, OptimizationMode::Gas, 200);
            for kind in
                [InstKind::Balance(slot), InstKind::ExtCodeSize(slot), InstKind::ExtCodeHash(slot)]
            {
                assert_eq!(target.op_has_variable_gas(&kind.op()), version >= EvmVersion::Berlin);
            }
        }
    }

    #[test]
    fn select_prices_the_emitted_sequence() {
        let target = Target::with(EvmVersion::Osaka, OptimizationMode::Gas, 200);
        assert_eq!(target.select(false), Cost::new(17, 5).with_steps(5));
        assert_eq!(target.select(true), Cost::new(20, 6).with_steps(6));
    }

    #[test]
    fn objectives_rank_costs() {
        let gas = Target::with(EvmVersion::Osaka, OptimizationMode::Gas, 200);
        let size = Target::with(EvmVersion::Osaka, OptimizationMode::Size, 200);
        let cheap_gas = Cost::new(3, 4);
        let cheap_bytes = Cost::new(5, 3);
        assert_eq!(gas.cmp(cheap_gas, cheap_bytes), Ordering::Less);
        assert_eq!(size.cmp(cheap_gas, cheap_bytes), Ordering::Greater);
        assert_eq!(gas.lifetime_gas(Cost::new(1, 1)), 400);
        let once = Target::with(EvmVersion::Osaka, OptimizationMode::Gas, 1);
        assert_eq!(gas.cmp_lifetime(Cost::new(10, 1), Cost::new(1, 10)), Ordering::Greater);
        assert_eq!(once.cmp_lifetime(Cost::new(10, 1), Cost::new(1, 10)), Ordering::Less);
        assert_eq!(size.cmp_lifetime(cheap_gas, cheap_bytes), Ordering::Greater);
        assert!(gas.improves(1, 0));
        assert!(!gas.improves(1, -1));
        assert!(size.improves(-5, 1));
    }

    #[test]
    fn creation_code_pays_calldata_and_runs_once() {
        let creation = |version, optimization, runs| {
            Target::with(version, optimization, runs).priced_for(true)
        };
        let runtime = Target::with(EvmVersion::Osaka, OptimizationMode::Gas, 10_000);
        let gas = creation(EvmVersion::Osaka, OptimizationMode::Gas, 10_000);
        assert_eq!((runtime.byte_gas(), runtime.expected_executions()), (200, 10_000));
        assert_eq!((gas.byte_gas(), gas.expected_executions()), (16, 1));
        assert_eq!(creation(EvmVersion::Berlin, OptimizationMode::Gas, 200).byte_gas(), 16);
        assert_eq!(creation(EvmVersion::Petersburg, OptimizationMode::Gas, 200).byte_gas(), 68);
        assert_eq!(runtime.priced_for(false).byte_gas(), 200);
        // The checked `p * 3` shapes: a `PUSH32` compare against a division.
        let compare = Cost::new(9, 35);
        let divide = Cost::new(17, 6);
        assert_eq!(runtime.cmp(compare, divide), Ordering::Less);
        assert_eq!(gas.cmp(compare, divide), Ordering::Greater);
        assert_eq!(gas.lifetime_gas(compare), 9 + 35 * 16);
        // Size and unoptimized builds keep the runtime model.
        let size = creation(EvmVersion::Osaka, OptimizationMode::Size, 10_000);
        assert_eq!((size.byte_gas(), size.expected_executions()), (200, 10_000));
        assert_eq!(size.cmp(compare, divide), Ordering::Greater);
        assert!(!creation(EvmVersion::Osaka, OptimizationMode::None, 200).is_creation());
        // One execution saves too little gas to copy a loop body at these runs.
        assert!(runtime.copies_loops());
        assert!(!gas.copies_loops());
        assert!(!gas.deposit_repays_gas(0, 1));
        assert!(gas.deposit_repays_gas(1, 15));
        assert!(!gas.deposit_repays_gas(1, 16));
        assert!(runtime.improves(1, 0));
        assert!(!runtime.improves(1, -1));
        assert!(gas.improves(-15, 1));
        assert!(!gas.improves(-16, 1));
        assert!(gas.improves(17, -1));
        assert!(!size.improves(100, 0));
    }

    #[test]
    fn optimizer_runs_require_optimization() {
        assert_eq!(
            Target::optimizer_runs(OptimizationMode::None, Some(u64::MAX)),
            Target::DEFAULT_EXPECTED_EXECUTIONS
        );
        assert_eq!(Target::optimizer_runs(OptimizationMode::Gas, Some(u64::MAX)), u64::MAX);
        assert_eq!(Target::optimizer_runs(OptimizationMode::Size, Some(1)), 1);
    }

    #[test]
    fn dynamic_work_is_sized_from_immediates() {
        let target = Target::with(EvmVersion::Osaka, OptimizationMode::Gas, 200);
        let exponent = |value: Option<U256>| {
            target.op(&Op::Exp { a: ValueId::from_usize(0), b: ValueId::from_usize(1) }, |id| {
                (id.index() == 1).then_some(value).flatten()
            })
        };
        assert_eq!(exponent(None), Cost::new(60, 1).with_steps(1));
        assert_eq!(exponent(Some(U256::from(255))), Cost::new(60, 1).with_steps(1));
        assert_eq!(exponent(Some(U256::from(1 << 16))), Cost::new(160, 1).with_steps(1));
        assert_eq!(exponent(Some(U256::ZERO)), Cost::new(10, 1).with_steps(1));
        assert_eq!(
            target.opcode_with_immediates(op::EXP, &[Some(U256::from(2)), Some(U256::from(256))]),
            Cost::new(110, 1).with_steps(1),
        );
        assert_eq!(
            Target::with(EvmVersion::Homestead, OptimizationMode::Gas, 200)
                .opcode_with_immediates(op::EXP, &[Some(U256::from(2)), Some(U256::from(256))]),
            Cost::new(30, 1).with_steps(1),
        );
        assert_eq!(GasTier::Keccak.dynamic_units(&[None, Some(U256::from(64))]), 2);
        assert_eq!(GasTier::Copy.dynamic_units(&[None, None, Some(U256::from(33))]), 2);
        assert_eq!(GasTier::Log(1).dynamic_units(&[None, Some(U256::from(5)), None]), 5);
        assert_eq!(GasTier::VeryLow.dynamic_units(&[None, None]), 0);
        let a = ValueId::from_usize(0);
        let b = ValueId::from_usize(1);
        for version in [EvmVersion::Homestead, EvmVersion::Osaka] {
            let target = Target::with(version, OptimizationMode::Gas, 200);
            assert!(target.op_has_variable_gas(&Op::Exp { a, b }));
            assert!(!target.op_has_variable_gas(&Op::Add { a, b }));
        }
    }

    #[test]
    fn block_estimates_add_up_to_the_function() {
        let target = Target::with(EvmVersion::Osaka, OptimizationMode::Gas, 200);
        let mut function = Function::new(Ident::DUMMY);
        let exit = function.alloc_block();
        let one = function.alloc_value(Value::Immediate(Immediate::I256(U256::from(1))));
        let (add, sum) = function
            .alloc_value_inst(Instruction::new(InstKind::Add(one, one), Some(MirType::I256)));
        function.blocks[BlockId::ENTRY].instructions.push(add);
        function.blocks[BlockId::ENTRY].set_generated_terminator(Terminator::Jump(exit));
        function.blocks[exit]
            .set_generated_terminator(Terminator::Return { values: smallvec::smallvec![sum] });
        // `add` with both pushes of its immediate, then a pushed label, a jump and a landing.
        assert_eq!(
            target.block_code_estimate(&function, BlockId::ENTRY),
            Cost::new(21, 10).with_steps(6)
        );
        assert_eq!(target.block_code_estimate(&function, exit), Cost::ZERO);
        assert_eq!(target.code_estimate(&function), Cost::new(21, 10).with_steps(6));
    }

    #[test]
    fn pushes_and_stack_ops() {
        let target = Target::with(EvmVersion::Osaka, OptimizationMode::Gas, 200);
        let one = |gas, bytes| Cost::new(gas, bytes).with_steps(1);
        assert_eq!(target.dup(), one(3, 1));
        assert_eq!(target.opcode(op::POP), one(2, 1));
        assert_eq!(target.opcode(op::PUSH2), one(3, 3));
        assert_eq!(target.label_push(), one(3, 3));
        assert_eq!(target.push(U256::ZERO), one(2, 1));
        assert_eq!(target.push(U256::from(0x1234)), one(3, 3));
        // `PUSH0; NOT`.
        assert_eq!(target.push(U256::MAX), Cost::new(5, 2).with_steps(2));
        assert_eq!(target.copied_words(64), Cost::new(6, 0));
        let legacy = Target::with(EvmVersion::Paris, OptimizationMode::Gas, 200);
        assert_eq!(legacy.push(U256::ZERO), one(3, 2));
        assert_eq!(target.stack_op(op::StackOp::Swap(16)), Some(one(3, 1)));
        assert_eq!(target.stack_op(op::StackOp::Pop), Some(one(2, 1)));
        // Three swaps before Amsterdam.
        assert_eq!(
            target.stack_op(op::StackOp::Exchange(1, 2)),
            Some(Cost::new(9, 3).with_steps(3))
        );
        assert_eq!(target.stack_op(op::StackOp::Exchange(1, 17)), None);
        let amsterdam = Target::with(EvmVersion::Amsterdam, OptimizationMode::Gas, 200);
        assert_eq!(amsterdam.stack_op(op::StackOp::Dup(17)), Some(one(3, 2)));
        assert_eq!(amsterdam.stack_op(op::StackOp::Exchange(1, 2)), Some(one(3, 2)));
        assert!(target.dominates(Cost::new(3, 2), Cost::new(9, 3)));
        assert!(target.dominates(Cost::new(9, 2), Cost::new(9, 3)));
        assert!(!target.dominates(Cost::new(9, 3), Cost::new(9, 3)));
        assert!(!target.dominates(Cost::new(3, 4), Cost::new(9, 3)));
    }

    #[test]
    fn interpreter_charges_each_executed_instruction() {
        let model = |tune| Target::with(EvmVersion::Osaka, OptimizationMode::Gas, 200).tuned(tune);
        let none = model(Tune::None);
        let interpreter = model(Tune::Interpreter);
        let cost = Cost::new(10, 4).with_steps(3);
        assert_eq!(Target::INTERPRETER_DISPATCH_GAS, 4);
        assert_eq!(none.runtime_gas(cost), 10);
        assert_eq!(interpreter.runtime_gas(cost), 22);
        assert_eq!(interpreter.objective_key(cost), [22, 4]);
        assert_eq!(none.objective_key(cost), [10, 4]);
        assert_eq!(interpreter.lifetime_gas(cost), 22 * 200 + 4 * 200);
        // Fewer gas and bytes in more instructions dominates only where dispatch is free.
        let more_steps = Cost::new(9, 4).with_steps(5);
        assert!(none.dominates(more_steps, cost));
        assert!(!interpreter.dominates(more_steps, cost));
        assert!(interpreter.dominates(Cost::new(10, 4).with_steps(2), cost));
        assert!(!none.dominates(Cost::new(10, 4).with_steps(2), cost));
    }

    #[test]
    fn jit_never_packs_wide_jump_tables() {
        let model = |tune| Target::with(EvmVersion::Osaka, OptimizationMode::Gas, 200).tuned(tune);
        for tune in [Tune::None, Tune::Interpreter] {
            assert!(model(tune).packs_wide_jump_tables(true));
            assert!(!model(tune).packs_wide_jump_tables(false));
        }
        assert!(!model(Tune::Jit).packs_wide_jump_tables(true));
        assert!(!model(Tune::Jit).packs_wide_jump_tables(false));
    }

    #[test]
    fn every_tune_selects_the_fewest_bytes() {
        let address_mask = (U256::ONE << 160) - U256::ONE;
        for tune in [Tune::None, Tune::Interpreter, Tune::Jit] {
            for optimization in [OptimizationMode::Gas, OptimizationMode::Size] {
                let target = Target::with(EvmVersion::Osaka, optimization, 200).tuned(tune);
                // `PUSH0; NOT; PUSH1 0x60; SHR` against `PUSH20 0xff..ff`.
                assert_eq!(target.push(address_mask), Cost::new(11, 5).with_steps(4));
                // `PUSH0; NOT`.
                assert_eq!(target.push(U256::MAX), Cost::new(5, 2).with_steps(2));
            }
        }
    }

    #[test]
    fn nested_block_weights() {
        assert_eq!(Target::nested_block_weight(1), 8);
        assert_eq!(Target::nested_block_weight(64), 512);
        assert_eq!(Target::nested_block_weight(512), 512);
    }

    #[test]
    fn cheap_immediates_fit_one_push_byte() {
        let target = Target::with(EvmVersion::Osaka, OptimizationMode::Gas, 200);
        for value in [0, 1, 64, 255] {
            assert!(target.is_cheap_immediate(U256::from(value)), "{value}");
        }
        assert!(!target.is_cheap_immediate(U256::from(256)));
        assert!(!target.is_cheap_immediate(U256::MAX));
        assert!(!target.is_cheap_immediate(U256::from(1) << 159));
    }

    #[test]
    fn loop_copies_need_many_runs() {
        let target = |runs| Target::with(EvmVersion::Osaka, OptimizationMode::Gas, runs);
        assert!(!target(Target::DEFAULT_EXPECTED_EXECUTIONS).copies_loops());
        assert!(!target(Target::LOOP_COPY_MIN_RUNS - 1).copies_loops());
        assert!(target(Target::LOOP_COPY_MIN_RUNS).copies_loops());
        assert!(target(u64::from(u32::MAX)).copies_loops());
    }

    #[test]
    fn jit_unrolls_loops_from_the_default_runs() {
        let target =
            |runs, tune| Target::with(EvmVersion::Osaka, OptimizationMode::Gas, runs).tuned(tune);
        for tune in [Tune::None, Tune::Interpreter] {
            assert!(!target(Target::DEFAULT_EXPECTED_EXECUTIONS, tune).unrolls_loops());
            assert!(target(Target::LOOP_COPY_MIN_RUNS, tune).unrolls_loops());
            assert!(target(Target::DEFAULT_EXPECTED_EXECUTIONS, tune).unrolls_loop_carrying(9));
        }
        let jit = |runs| target(runs, Tune::Jit);
        assert!(!jit(Target::JIT_LOOP_UNROLL_MIN_RUNS - 1).unrolls_loops());
        assert!(jit(Target::DEFAULT_EXPECTED_EXECUTIONS).unrolls_loops());
        assert!(jit(Target::LOOP_COPY_MIN_RUNS).unrolls_loops());
        // Loop copies other than unrolling and peeling still wait for the many runs.
        assert!(!jit(Target::DEFAULT_EXPECTED_EXECUTIONS).copies_loops());
        let words = Target::JIT_MAX_UNROLLED_LOOP_WORDS;
        assert!(jit(200).unrolls_loop_carrying(words));
        assert!(!jit(200).unrolls_loop_carrying(words + 1));
    }

    #[test]
    fn compact_immediates_repay_their_gas() {
        // `type(uint128).max`: `PUSH16` is 17 bytes for 3 gas, `PUSH0 NOT PUSH1 128 SHR` 5 bytes
        // for 11: 12 bytes deposit 2,400 gas against 8 gas on 200 executions, 1,600, or 16,000
        // ten times over in a loop.
        let target = Target::with(EvmVersion::Osaka, OptimizationMode::Gas, 200);
        assert!(target.compacts_immediate(12, 8, false));
        assert!(!target.compacts_immediate(12, 8, true));
        // `type(uint64).max`: 4 bytes, 800 gas, do not repay 8 gas on 200 executions.
        assert!(!target.compacts_immediate(4, 8, false));
        // A recipe that costs no more gas always wins.
        assert!(target.compacts_immediate(1, 0, true));
        let size = Target::with(EvmVersion::Osaka, OptimizationMode::Size, 200);
        assert!(size.compacts_immediate(12, 8, true));
        // `-O none` prices the push like a gas build: a loop keeps the literal.
        let none = Target::with(EvmVersion::Osaka, OptimizationMode::None, 200);
        assert!(none.compacts_immediate(12, 8, false));
        assert!(!none.compacts_immediate(12, 8, true));
        assert!(!none.compacts_immediate(4, 8, false));
        // A tie takes the bytes: `not(31)` in a loop saves 30 bytes, 6,000 gas, for 3 gas ten
        // times over on 200 executions, 6,000.
        assert!(target.compacts_immediate(30, 3, true));
        // A recipe that saves no bytes does not win.
        assert!(!target.compacts_immediate(0, 0, false));
        // More runs than the default price a push at the default: the 26 bytes of a left-aligned
        // `"abc"`, 5,200 gas, repay 6 gas on 200 executions, though not on 1,000, nor ten times
        // over in a loop, and `not(31)` still ties in a loop.
        let hot = Target::with(EvmVersion::Osaka, OptimizationMode::Gas, 1_000);
        assert!(hot.compacts_immediate(26, 6, false));
        assert!(!hot.compacts_immediate(26, 6, true));
        assert!(!hot.compacts_immediate(4, 8, false));
        assert!(hot.compacts_immediate(30, 3, true));
    }

    #[test]
    fn cold_code_is_priced_like_a_size_build() {
        let gas = Target::with(EvmVersion::Osaka, OptimizationMode::Gas, 200);
        let size = Target::with(EvmVersion::Osaka, OptimizationMode::Size, 200);
        let none = Target::with(EvmVersion::Osaka, OptimizationMode::None, 200);
        // One byte, 200 gas of deposit, does not repay 6 extra gas on 200 executions, 1,200.
        assert!(!gas.compacts_immediate(1, 6, false));
        assert!(gas.priced_for_cold(true).compacts_immediate(1, 6, false));
        assert_eq!(gas.priced_for_cold(true), size);
        assert_eq!(gas.priced_for_cold(false), gas);
        assert_eq!(size.priced_for_cold(true), size);
        assert_eq!(none.priced_for_cold(true), none);
    }

    #[test]
    fn deposit_repays_gas_over_expected_executions() {
        let target = Target::with(EvmVersion::Osaka, OptimizationMode::Gas, 200);
        // 10 bytes deposit 2,000 gas; 9 gas on 200 executions is 1,800.
        assert!(target.deposit_repays_gas(10, 9));
        assert!(!target.deposit_repays_gas(10, 10));
        assert!(!target.deposit_repays_gas(0, 0));
        let hot = Target::with(EvmVersion::Osaka, OptimizationMode::Gas, 10_000);
        assert!(!hot.deposit_repays_gas(10, 1));
    }

    #[test]
    fn turned_branch_prices_both_edges() {
        let gas = Target::with(EvmVersion::Osaka, OptimizationMode::Gas, 200);
        // The added `ISZERO` costs 3 gas on `next`, less the landing that goes, and a jump to
        // `other` saves 11 gas of transfer less that test.
        assert!(gas.falls_through_turned_branch(1, 1, TurnedTest::Adds, false));
        assert!(gas.falls_through_turned_branch(1, 4, TurnedTest::Adds, true));
        assert!(!gas.falls_through_turned_branch(10, 1, TurnedTest::Adds, false));
        assert!(!gas.falls_through_turned_branch(1, 0, TurnedTest::Adds, false));
        // A swapped test is free, so the landing that goes pays alone, and a kept landing ties.
        assert!(gas.falls_through_turned_branch(1, 0, TurnedTest::Swaps, false));
        assert!(!gas.falls_through_turned_branch(1, 0, TurnedTest::Swaps, true));
        // A size build at one run takes the bytes of a hot loop, and not at a million runs.
        let size = Target::with(EvmVersion::Osaka, OptimizationMode::Size, 1);
        assert!(size.falls_through_turned_branch(10, 1, TurnedTest::Adds, false));
        // A free test still stays out of the hot edge of a loop in a gas build.
        assert!(!gas.falls_through_turned_branch(10, 1, TurnedTest::Swaps, false));
        assert!(size.falls_through_turned_branch(10, 1, TurnedTest::Swaps, false));
        let hot = Target::with(EvmVersion::Osaka, OptimizationMode::Size, 1_000_000);
        assert!(!hot.falls_through_turned_branch(10, 1, TurnedTest::Adds, false));
    }

    #[test]
    fn helper_sharing_prices_the_call_protocol_per_site() {
        let target = Target::with(EvmVersion::Osaka, OptimizationMode::Size, 200);
        let (shared, expanded) = target.helper_sharing(100, 2, 1, 3);
        // Three sites each pay a call with a 5-word frame and a return. Each emits two label
        // pushes, a jump, a landing, and three moves; the helper adds a landing, one body, a
        // result move, and the return jump.
        assert_eq!(
            shared,
            Cost::new(3 * ((90 + 3 * 24 + 5 * 6) + (24 + 3 * 8)), 3 * 11 + 103)
                // Per site: 63 instructions to call (3 exactly, then 60 for the other 180 gas) and
                // 15 to return (1 exactly, then 14 for the other 40 gas).
                .with_steps(3 * (63 + 15))
        );
        assert_eq!(expanded, Cost::new(0, 300));
        assert!(target.cmp(shared, expanded).is_lt());
        let (shared, expanded) = target.helper_sharing(10, 2, 1, 2);
        assert!(target.cmp(shared, expanded).is_gt());
        // A second result costs a move at each site and in the helper: 51 bytes against 52
        // expanded with one result, 54 with two.
        let (shared, expanded) = target.helper_sharing(26, 2, 1, 2);
        assert!(target.cmp(shared, expanded).is_lt());
        let (shared, expanded) = target.helper_sharing(26, 2, 2, 2);
        assert!(target.cmp(shared, expanded).is_gt());
    }

    #[test]
    fn calldata_block_copy_guard_pays_only_for_gas() {
        let size = Target::with(EvmVersion::Osaka, OptimizationMode::Size, 200);
        let gas = Target::with(EvmVersion::Osaka, OptimizationMode::Gas, 200);
        // The guard adds a copy and a branch to the loop it keeps: bytes under the size
        // objective, and a copy that beats the loop's gas on even one element under gas.
        assert!(!size.guards_block_copy(2));
        assert!(gas.guards_block_copy(2));
        assert!(gas.guards_block_copy(8));
    }

    #[test]
    fn control_flow_costs_distinguish_unconditional_edges() {
        for version in
            [EvmVersion::Homestead, EvmVersion::Paris, EvmVersion::Osaka, EvmVersion::Amsterdam]
        {
            let target = Target::with(version, OptimizationMode::Gas, 200);
            assert_eq!(target.jump(), Cost::new(12, 5).with_steps(3));
            assert_eq!(target.branch(), Cost::new(14, 5).with_steps(3));
            let mut function = Function::new(Ident::DUMMY);
            let then_block = function.alloc_block();
            let else_block = function.alloc_block();
            let condition = function.alloc_value(Value::Immediate(Immediate::I1(true)));
            function.blocks[BlockId::ENTRY].set_generated_terminator(Terminator::Branch {
                condition,
                then_block,
                else_block,
            });
            assert_eq!(
                target.block_code_estimate(&function, BlockId::ENTRY),
                target.branch().times(2)
            );
            function.blocks[BlockId::ENTRY].set_generated_terminator(Terminator::Jump(then_block));
            assert_eq!(target.block_code_estimate(&function, BlockId::ENTRY), target.jump());
        }
    }

    #[test]
    fn legacy_splats_price_copy_loops_against_stores() {
        let target = Target::with(EvmVersion::Paris, OptimizationMode::Size, 1);
        assert!(!target.legacy_splat_is_profitable(U256::from(1), 160, false));
        assert!(!target.legacy_splat_is_profitable(U256::from(1), 256, false));
        assert!(target.legacy_splat_is_profitable(U256::from(1), 1024, false));
        assert!(target.legacy_splat_is_profitable(U256::from(1), 2048, false));
        assert!(target.legacy_splat_is_profitable(U256::from(1), 5120, false));
        let word = U256::from_be_bytes([0x11; 32]);
        assert!(target.legacy_splat_is_profitable(word, 96, true));
        assert!(target.legacy_splat_is_profitable(word, 5120, true));
    }

    #[test]
    fn preserved_literals_fold_by_objective() {
        let folds = |target: Target, removes_original, len| {
            target.folds_preserved_literal(removes_original, || Some(vec![0x41; len]))
        };
        let gas = |evm_version| Target::with(evm_version, OptimizationMode::Gas, 200);
        let size = |evm_version| Target::with(evm_version, OptimizationMode::Size, 1);
        // Gas folds only where the copy would be a loop, and never reads the literal.
        assert!(gas(EvmVersion::Paris).folds_preserved_literal(false, || None));
        assert!(!gas(EvmVersion::Cancun).folds_preserved_literal(true, || None));
        // The stores of a short string cost fewer bytes than the length math and a copy.
        assert!(folds(size(EvmVersion::Cancun), false, 4));
        assert!(!folds(size(EvmVersion::Cancun), false, 32));
        // The copy loop is longer than MCOPY, so longer strings still fold before Cancun.
        assert!(folds(size(EvmVersion::Paris), false, 32));
        assert!(!folds(size(EvmVersion::Paris), false, 64));
        // Folding that drops the original object only moves the stores.
        assert!(folds(size(EvmVersion::Cancun), true, 64));
        assert!(!size(EvmVersion::Cancun).folds_preserved_literal(false, || None));
    }

    #[test]
    fn terminal_copy_sharing() {
        let gas = Target::with(EvmVersion::Osaka, OptimizationMode::Gas, 200);
        let size = Target::with(EvmVersion::Osaka, OptimizationMode::Size, 200);
        // `mstore 0, 1; revert 0, 32`: four 4-byte jumps and one landing beat four copies.
        let small = Cost::new(0, 8);
        assert!(gas.shares_terminal_copy(small, 4));
        assert!(!size.shares_terminal_copy(small, 4));
        // A 9-byte copy shared by three sites: 12 + 9 + 1 bytes against 27.
        let medium = Cost::new(0, 9);
        assert!(gas.shares_terminal_copy(medium, 3));
        assert!(size.shares_terminal_copy(medium, 3));
        // One site never pays for a jump.
        assert!(!gas.shares_terminal_copy(Cost::new(0, 40), 1));
    }

    #[test]
    fn legacy_shifts_push_the_factor_when_it_costs_less() {
        let shift = |bits| U256::from(bits);
        let size = Target::with(EvmVersion::Byzantium, OptimizationMode::Size, 1);
        assert_eq!(size.legacy_shift_factor(shift(8)), Some(U256::from(256)));
        assert_eq!(size.legacy_shift_factor(shift(224)), None);
        // 24 more bytes of deposit outweigh the 63 gas of `EXP` unless the code runs often.
        let few = Target::with(EvmVersion::Byzantium, OptimizationMode::Gas, 1);
        assert_eq!(few.legacy_shift_factor(shift(224)), None);
        let many = Target::with(EvmVersion::Byzantium, OptimizationMode::Gas, 200);
        assert_eq!(many.legacy_shift_factor(shift(224)), Some(U256::ONE << 224));
        assert_eq!(many.legacy_shift_factor(shift(256)), Some(U256::ZERO));
    }

    #[test]
    fn loop_sites_share_by_lifetime_gas() {
        // A pooled `PUSH32` saves 25 bytes at a site and adds 27 gas of transfers to every
        // iteration: 5,000 gas of deposit against 54,000 over 200 runs of ten iterations, or 270
        // over one run.
        for optimization in [OptimizationMode::Gas, OptimizationMode::Size] {
            let target = |runs| Target::with(EvmVersion::Cancun, optimization, runs);
            assert!(!target(200).shares_in_loop(25, 27));
            assert!(target(1).shares_in_loop(25, 27));
            assert!(!target(1).shares_in_loop(0, 27));
        }
    }

    #[test]
    fn loop_blocks_are_duplicated_by_lifetime_gas() {
        let gas = Target::with(EvmVersion::Cancun, OptimizationMode::Gas, 200);
        // A 12-byte latch on one of three paths saves 12 gas on a third of ten iterations: 8,000
        // gas over 200 runs against 8 more bytes, 1,600 gas of deposit.
        assert!(gas.duplicates_loop_block(Cost::new(30, 12), 3));
        // Then the saving repays at most 40 added bytes, and on the only path any copy that
        // fits the size limit.
        assert!(gas.duplicates_loop_block(Cost::new(0, 40), 3));
        assert!(!gas.duplicates_loop_block(Cost::new(0, 40), 4));
        assert!(gas.duplicates_loop_block(Cost::new(0, Target::MAX_LOOP_JOIN_COPY_BYTES), 1));
        assert!(!gas.duplicates_loop_block(Cost::new(0, Target::MAX_LOOP_JOIN_COPY_BYTES + 1), 1));
        // One expected execution repays no added byte.
        let once = Target::with(EvmVersion::Cancun, OptimizationMode::Gas, 1);
        assert!(once.duplicates_loop_block(Cost::new(0, 4), 1));
        assert!(!once.duplicates_loop_block(Cost::new(0, 5), 1));
        let size = Target::with(EvmVersion::Cancun, OptimizationMode::Size, 200);
        assert!(!size.duplicates_loop_block(Cost::new(30, 12), 1));
    }

    #[test]
    fn loop_join_copies_keep_to_a_share_of_the_code() {
        let gas = Target::with(EvmVersion::Cancun, OptimizationMode::Gas, 200);
        // A 12-byte latch adds 8 bytes beyond the label push and jump it replaces, and saves the
        // jump on a third of ten iterations.
        assert_eq!(
            gas.loop_block_copy(Cost::new(30, 12), 3),
            (8, u64::from(gas.jump().gas) * 10 / 3)
        );
        // At 200 runs, a 194-byte contract built around one loop may grow by 9 bytes, the
        // largest small one by 204, and a large one by every copy that pays.
        assert_eq!(gas.loop_join_copy_budget(194), 9);
        assert_eq!(gas.loop_join_copy_budget(Target::SMALL_CODE_BYTES - 1), 204);
        assert_eq!(gas.loop_join_copy_budget(Target::SMALL_CODE_BYTES), u64::MAX);
        // From the runs that copy loop bodies on, every copy that pays goes.
        let hot =
            Target::with(EvmVersion::Cancun, OptimizationMode::Gas, Target::LOOP_COPY_MIN_RUNS);
        assert_eq!(hot.loop_join_copy_budget(194), u64::MAX);
    }

    #[test]
    fn inline_hint_admits_growth_only_for_gas() {
        let target = |optimization| Target::with(EvmVersion::Cancun, optimization, 200);
        let gas = target(OptimizationMode::Gas);
        assert!(gas.inline_hint_admits(20, 10));
        assert!(!gas.inline_hint_admits(21, 10));
        let size = target(OptimizationMode::Size);
        assert!(size.inline_hint_admits(10, 10));
        assert!(!size.inline_hint_admits(11, 10));
        assert!(!target(OptimizationMode::None).inline_hint_admits(0, 10));
    }

    #[test]
    fn empty_copies_are_tested_for_gas() {
        let target = |optimization| Target::with(EvmVersion::Cancun, optimization, 200);
        assert!(target(OptimizationMode::Gas).tests_empty_copies());
        assert!(!target(OptimizationMode::Size).tests_empty_copies());
        // The setup an empty copy skips costs more than the branch that tests it.
        let gas = target(OptimizationMode::Gas);
        let setup =
            gas.opcode(op::LT).times(2) + gas.opcode(op::OR) + gas.branch() + gas.opcode(op::MCOPY);
        assert!(setup.gas > (gas.opcode(op::ISZERO) + gas.branch()).gas);
    }

    #[test]
    fn constant_keccaks_fold_when_their_dead_stores_pay_for_the_hash() {
        let hash = U256::from_be_bytes(alloy_primitives::keccak256([0; 64]).0);
        let word = U256::from_be_bytes(alloy_primitives::keccak256([1]).0);
        for optimization in [OptimizationMode::Gas, OptimizationMode::Size] {
            let target = Target::with(EvmVersion::Osaka, optimization, 200);
            // `keccak256 0, 64`.
            let keccak = target.push(U256::from(64))
                + target.push(U256::ZERO)
                + target.opcode_with_immediates(
                    op::KECCAK256,
                    &[Some(U256::ZERO), Some(U256::from(64))],
                );
            let store = |word: U256, address: u64| {
                target.push(word) + target.push(U256::from(address)) + target.opcode(op::MSTORE)
            };
            let small = store(U256::from(7), 0) + store(U256::from(5), 32);
            let full = store(word, 0) + store(word, 32);
            // The hash costs 29 bytes more than the instruction, whatever gas it saves.
            assert!(!target.folds_constant_keccak(hash, keccak, Cost::ZERO, 1, false));
            assert!(!target.folds_constant_keccak(hash, keccak, small, 1, false));
            // Dead stores of full words repay it once, but not a second use.
            assert!(target.folds_constant_keccak(hash, keccak, store(word, 0), 1, false));
            assert!(target.folds_constant_keccak(hash, keccak, full, 2, false));
            assert!(!target.folds_constant_keccak(hash, keccak, full, 3, false));
            // A hash nothing reads is dead whatever the stores.
            assert!(target.folds_constant_keccak(hash, keccak, Cost::ZERO, 0, false));
            // A short hash costs fewer bytes than the instruction and wins on its own.
            assert!(target.folds_constant_keccak(U256::from(1), keccak, Cost::ZERO, 1, false));
            // A loop repays the bytes with the gas of its iterations, when optimizing for gas.
            assert_eq!(
                target.folds_constant_keccak(hash, keccak, Cost::ZERO, 1, true),
                optimization.is_gas()
            );
        }
    }
    #[test]
    fn frame_words_cost_memory_expansion() {
        // A word below the heap costs every call one more word of expansion, and no bytes.
        for optimization in [OptimizationMode::Gas, OptimizationMode::Size] {
            let target = Target::with(EvmVersion::Cancun, optimization, 200);
            assert_eq!(target.frame_words(0), Cost::ZERO);
            assert_eq!(target.frame_words(10), Cost::new(30, 0));
        }
    }

    #[test]
    fn outlined_return_tails_pay_for_their_jump() {
        let target = Target::with(EvmVersion::Cancun, OptimizationMode::Size, 200);
        let suffix = target.dup()
            + target.opcode(op::ADD)
            + target.push(U256::from(64))
            + target.opcode(op::MSTORE);
        assert_eq!(target.shared_return_tail_savings(suffix.bytes, 1), 2);
        assert_eq!(target.shared_return_tail_savings(0, 1), 0);
        assert_eq!(target.shared_return_tail_savings(suffix.bytes, 0), 1);
    }

    #[test]
    fn returndata_zero_replaces_only_the_pre_shanghai_push() {
        let price = |version| Target::with(version, OptimizationMode::Gas, 200).returndata_zero();
        let paris = Target::with(EvmVersion::Paris, OptimizationMode::Gas, 200);
        let returndata_zero = price(EvmVersion::Paris).unwrap();
        assert_eq!(returndata_zero, paris.opcode(op::RETURNDATASIZE));
        // Cheaper than the literal push and than a `DUP` in gas, and no larger.
        assert!(paris.dominates(returndata_zero, paris.push(U256::ZERO)));
        assert!(paris.dominates(returndata_zero, paris.dup()));
        assert!(price(EvmVersion::Byzantium).is_some());
        assert!(price(EvmVersion::Homestead).is_none());
        assert!(price(EvmVersion::Shanghai).is_none());
        assert!(price(EvmVersion::Osaka).is_none());
    }

    #[test]
    fn encoded_pushes_keep_their_literal_width() {
        let paris = Target::with(EvmVersion::Paris, OptimizationMode::Size, 200);
        let cancun = Target::with(EvmVersion::Cancun, OptimizationMode::Size, 200);
        assert_eq!(paris.encoded_push(U256::ZERO), paris.opcode(op::PUSH1));
        assert_eq!(cancun.encoded_push(U256::ZERO), cancun.opcode(op::PUSH0));
        for (value, opcode) in [(1, op::PUSH1), (255, op::PUSH1), (256, op::PUSH2)] {
            assert_eq!(cancun.encoded_push(U256::from(value)), cancun.opcode(opcode));
        }
        assert_eq!(cancun.encoded_push(U256::MAX), cancun.opcode(op::PUSH32));
        assert!(cancun.push(U256::MAX).bytes < cancun.encoded_push(U256::MAX).bytes);
    }

    #[test]
    fn failure_paths_are_cold_for_gas_only() {
        let target = |optimization| Target::with(EvmVersion::Osaka, optimization, 200);
        assert!(target(OptimizationMode::Gas).marks_failure_paths_cold());
        assert!(!target(OptimizationMode::Size).marks_failure_paths_cold());
        assert!(!target(OptimizationMode::None).marks_failure_paths_cold());
    }
}
