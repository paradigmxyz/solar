//! Target cost model.
//!
//! One place answers what code costs on the selected EVM version: the static
//! gas of every opcode across the fork schedule, the encoded size of pushes
//! and stack operations, the price of deposited bytes, and how the active
//! optimization objective ranks gas against bytes. Creation code has its own
//! model, [`Target::priced_for`]: it runs once, and each of its bytes costs the
//! calldata gas of the deployment transaction instead of a code deposit.
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
    mir::{BlockId, Function, Op, Terminator, Value, ValueId, memory::EvmMemoryLayout},
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_config::{EvmVersion, OptimizationMode};
use solar_sema::Gcx;
use std::{cmp::Ordering, ops};

mod stack;
pub(crate) use stack::StackCosts;

/// The gas class of an opcode in the fork schedule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum GasTier {
    /// Halting instructions.
    Zero,
    /// `JUMPDEST`.
    Jumpdest,
    /// Environment reads, `POP`, `PUSH0`.
    Base,
    /// Arithmetic, comparisons, memory words, pushes, `DUP`, `SWAP`.
    VeryLow,
    /// Multiplication, division, `SIGNEXTEND`, `SELFBALANCE`, `CLZ`.
    Low,
    /// `ADDMOD`, `MULMOD`, `JUMP`.
    Mid,
    /// `JUMPI`.
    High,
    /// `EXP`, plus a per-byte exponent charge.
    Exp,
    /// `KECCAK256`, plus a per-word charge.
    Keccak,
    /// Memory copies, plus a per-word charge.
    Copy,
    /// `BLOCKHASH`.
    BlockHash,
    /// `BALANCE`, repriced by EIP-150, EIP-1884, and EIP-2929.
    Balance,
    /// `EXTCODESIZE` and `EXTCODECOPY`, repriced by EIP-150 and EIP-2929.
    ExtCode,
    /// `EXTCODEHASH`, repriced by EIP-1884 and EIP-2929.
    ExtCodeHash,
    /// `SLOAD`, repriced by EIP-150, EIP-1884, and EIP-2929.
    SLoad,
    /// `SSTORE` of a cold slot that changes its value.
    SStore,
    /// `TLOAD` and `TSTORE`.
    Transient,
    /// `LOGn`, plus a per-byte data charge.
    Log(u8),
    /// Contract creation.
    Create,
    /// Message calls to a cold account.
    Call,
    /// `SELFDESTRUCT`, priced by EIP-150.
    SelfDestruct,
}

/// Whether an account or storage slot was already touched in the transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Warmth {
    /// The first access in the transaction, at the full price.
    Cold,
    /// An access after an earlier one, at the reduced price EIP-2929 charges;
    /// earlier schedules do not distinguish the two.
    Warm,
}

impl GasTier {
    /// Gas charged by one copy opcode per copied word.
    pub(crate) const COPY_WORD_GAS: u32 = 3;
    /// Gas of a warm account or storage access under EIP-2929.
    pub(crate) const WARM_ACCESS_GAS: u32 = 100;
    /// The surcharge EIP-2929 adds to a storage write of a cold slot.
    const COLD_SLOAD_GAS: u32 = 2100;

    /// Gas charged on `evm_version` before dynamic components for an access of
    /// the given warmth. Warmth only reprices account and storage accesses,
    /// and only since Berlin.
    pub(crate) const fn gas_at(self, evm_version: EvmVersion, warmth: Warmth) -> u32 {
        if !matches!(warmth, Warmth::Warm) || !since(evm_version, EvmVersion::Berlin) {
            return self.gas(evm_version);
        }
        match self {
            Self::Balance | Self::ExtCode | Self::ExtCodeHash | Self::SLoad | Self::Call => {
                Self::WARM_ACCESS_GAS
            }
            Self::SStore => self.gas(evm_version) - Self::COLD_SLOAD_GAS,
            _ => self.gas(evm_version),
        }
    }

    /// Gas charged on `evm_version` before dynamic components.
    pub(crate) const fn gas(self, evm_version: EvmVersion) -> u32 {
        match self {
            Self::Zero => 0,
            Self::Jumpdest => 1,
            Self::Base => 2,
            Self::VeryLow | Self::Copy => 3,
            Self::Low => 5,
            Self::Mid => 8,
            Self::High | Self::Exp => 10,
            Self::Keccak => 30,
            Self::BlockHash => 20,
            Self::Balance => {
                if since(evm_version, EvmVersion::Berlin) {
                    2600
                } else if since(evm_version, EvmVersion::Istanbul) {
                    700
                } else if since(evm_version, EvmVersion::TangerineWhistle) {
                    400
                } else {
                    20
                }
            }
            Self::ExtCode => {
                if since(evm_version, EvmVersion::Berlin) {
                    2600
                } else if since(evm_version, EvmVersion::TangerineWhistle) {
                    700
                } else {
                    20
                }
            }
            Self::ExtCodeHash => {
                if since(evm_version, EvmVersion::Berlin) {
                    2600
                } else if since(evm_version, EvmVersion::Istanbul) {
                    700
                } else {
                    400
                }
            }
            Self::SLoad => {
                if since(evm_version, EvmVersion::Berlin) {
                    2100
                } else if since(evm_version, EvmVersion::Istanbul) {
                    800
                } else if since(evm_version, EvmVersion::TangerineWhistle) {
                    200
                } else {
                    50
                }
            }
            Self::SStore => 5000,
            Self::Transient => 100,
            Self::Log(topics) => 375 + 375 * topics as u32,
            Self::Create => 32000,
            Self::Call => {
                if since(evm_version, EvmVersion::Berlin) {
                    2600
                } else if since(evm_version, EvmVersion::TangerineWhistle) {
                    700
                } else {
                    40
                }
            }
            Self::SelfDestruct => {
                if since(evm_version, EvmVersion::TangerineWhistle) {
                    5000
                } else {
                    0
                }
            }
        }
    }

    /// Gas charged per unit of runtime-sized work: exponent bytes, hashed or
    /// copied words, and logged bytes.
    pub(crate) const fn dynamic_gas(self, evm_version: EvmVersion) -> u32 {
        match self {
            Self::Exp => {
                if since(evm_version, EvmVersion::SpuriousDragon) {
                    50
                } else {
                    10
                }
            }
            Self::Keccak => 6,
            Self::Copy => Self::COPY_WORD_GAS,
            Self::Log(_) => 8,
            _ => 0,
        }
    }

    /// The stack argument that sizes the dynamic work: the exponent of
    /// `EXP`, the byte length of a hash or a log, the last argument of a copy.
    const fn sizing_argument(self, arguments: usize) -> Option<usize> {
        match self {
            Self::Exp | Self::Keccak | Self::Log(_) => Some(1),
            Self::Copy => arguments.checked_sub(1),
            _ => None,
        }
    }

    /// Units of dynamic work for `arguments`, in stack order with immediates
    /// resolved: exponent bytes, hashed or copied words, or logged bytes. One
    /// unit when the size is not known statically.
    fn dynamic_units(self, arguments: &[Option<U256>]) -> u32 {
        let Some(size) = self.sizing_argument(arguments.len()).and_then(|index| arguments[index])
        else {
            return u32::from(self.sizing_argument(arguments.len()).is_some());
        };
        let units = match self {
            Self::Exp => U256::from(size.bit_len().div_ceil(8)),
            Self::Keccak | Self::Copy => size.div_ceil(U256::from(32)),
            Self::Log(_) => size,
            _ => U256::ZERO,
        };
        u32::try_from(units).unwrap_or(u32::MAX)
    }

    /// Gas of a tier priced the same on every EVM version, usable in constants.
    pub(crate) const fn fixed_gas(self) -> u32 {
        match self {
            Self::Balance
            | Self::ExtCode
            | Self::ExtCodeHash
            | Self::SLoad
            | Self::Call
            | Self::SelfDestruct => panic!("tier is priced per EVM version"),
            _ => self.gas(EvmVersion::Homestead),
        }
    }
}

/// Returns whether `evm_version` includes the `fork` schedule.
const fn since(evm_version: EvmVersion, fork: EvmVersion) -> bool {
    evm_version as u8 >= fork as u8
}

/// Encoded size of an opcode including its immediate bytes.
const fn opcode_bytes(opcode: u8) -> u32 {
    let immediate = match opcode {
        op::PUSH1..=op::PUSH32 => (opcode - op::PUSH1 + 1) as u32,
        _ => 0,
    };
    1 + immediate
}

/// Static gas and encoded size of a code sequence.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct Cost {
    /// Static gas of one execution.
    pub(crate) gas: u32,
    /// Encoded bytes.
    pub(crate) bytes: u32,
}

impl Cost {
    /// Nothing emitted.
    pub(crate) const ZERO: Self = Self::new(0, 0);

    /// More than any real code costs.
    pub(crate) const MAX: Self = Self::new(u32::MAX, u32::MAX);

    /// Creates a cost.
    pub(crate) const fn new(gas: u32, bytes: u32) -> Self {
        Self { gas, bytes }
    }

    /// Whether this costs no more gas and bytes than `other` and less of one.
    pub(crate) fn dominates(self, other: Self) -> bool {
        self.gas <= other.gas && self.bytes <= other.bytes && self != other
    }

    /// Cost of an opcode whose static gas is identical on every EVM version.
    /// Panics for unknown opcodes or a fork-dependent gas tier.
    const fn fixed_opcode(opcode: u8) -> Self {
        let Some(definition) = op::definition(opcode) else { panic!("unknown opcode") };
        Self::new(definition.gas.fixed_gas(), opcode_bytes(opcode))
    }

    /// Sums two costs, saturating.
    pub(crate) const fn plus(self, other: Self) -> Self {
        Self::new(self.gas.saturating_add(other.gas), self.bytes.saturating_add(other.bytes))
    }

    /// Scales a cost, saturating.
    pub(crate) const fn times(self, count: u32) -> Self {
        Self::new(self.gas.saturating_mul(count), self.bytes.saturating_mul(count))
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

/// The cost model of the selected EVM version under the active objective.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Target {
    evm_version: EvmVersion,
    optimization: OptimizationMode,
    expected_executions: u64,
    /// Whether the model prices creation code, which runs once and pays calldata per byte.
    creation: bool,
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

    /// The model of the session's EVM version and optimizer runs for code optimized for
    /// `optimization`, its objective after any `@custom:solar-optimize` tag (see [`objective`]).
    pub(crate) fn new(gcx: Gcx<'_>, optimization: OptimizationMode) -> Self {
        let opts = &gcx.sess.opts;
        let expected_executions = Self::optimizer_runs(optimization, opts.optimizer_runs);
        Self::with(opts.evm_version, optimization, expected_executions)
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
        Self { evm_version, optimization, expected_executions, creation: false }
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

    /// Static gas of one opcode; unknown opcodes are free.
    pub(crate) fn opcode_gas(&self, opcode: u8) -> u32 {
        self.opcode_gas_at(opcode, Warmth::Cold)
    }

    /// Static gas of one opcode touching an account or slot of the given
    /// warmth; unknown opcodes are free.
    pub(crate) fn opcode_gas_at(&self, opcode: u8, warmth: Warmth) -> u32 {
        op::definition(opcode).map_or(0, |def| def.gas.gas_at(self.evm_version, warmth))
    }

    /// Cost of one opcode with its immediate, before dynamic components.
    pub(crate) fn opcode(&self, opcode: u8) -> Cost {
        Cost::new(self.opcode_gas(opcode), opcode_bytes(opcode))
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
        Some(Cost::new(metrics.static_gas as u32, metrics.assembled_len as u32))
    }

    /// Cost of pushing a block label, which resolves to a two-byte push in all but the largest
    /// contracts.
    pub(crate) fn label_push(&self) -> Cost {
        self.opcode(op::PUSH2)
    }

    /// Cost of pushing `value` through its cheapest materialization.
    pub(crate) fn push(&self, value: U256) -> Cost {
        let (bytes, gas) = compact_pushes::immediate_materialization_cost(self.evm_version, value);
        Cost::new(gas as u32, bytes as u32)
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
            return Cost::new(GasTier::VeryLow.gas(self.evm_version), 1);
        };
        let tier = op::definition(lowering.opcode()).map_or(GasTier::VeryLow, |def| def.gas);
        let static_gas = tier.gas_at(self.evm_version, warmth);
        let dynamic_gas = tier.dynamic_gas(self.evm_version);
        if dynamic_gas == 0 {
            return Cost::new(static_gas, 1);
        }
        let mut arguments = SmallVec::<[Option<U256>; 8]>::new();
        // Only the visit matters; the mapped copy is discarded.
        let _ = op.map_values(|value| {
            arguments.push(immediate(value));
            value
        });
        let dynamic = dynamic_gas.saturating_mul(tier.dynamic_units(&arguments));
        Cost::new(static_gas.saturating_add(dynamic), 1)
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
    pub(crate) fn icall(&self, args: usize, returns: usize, frame_words: u64) -> Cost {
        let moved = (args + returns) as u32;
        let protocol = 90 + moved * 24 + u32::try_from(frame_words).unwrap_or(u32::MAX) * 6;
        Cost::new(protocol, 18 + moved * 5)
    }

    /// Cost of one internal return: the jump back through the pushed label
    /// and the protocol handing `returns` results past `params` parameters.
    pub(crate) fn internal_return(&self, params: usize, returns: usize) -> Cost {
        let moved = (params + returns) as u32;
        Cost::new(24 + moved * 8, 8 + moved * 4)
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
        let gas = self.icall(params, returns, frame_words).gas
            + self.internal_return(params, returns).gas;
        let moves = self.dup().times((params + returns) as u32);
        let call = self.label_push() + self.jump() + moves;
        let ret =
            self.opcode(op::JUMPDEST) + self.dup().times(returns as u32) + self.opcode(op::JUMP);
        let shared = Cost::new(gas, call.bytes)
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
        let block = &func.blocks[block];
        let mut cost = Cost::ZERO;
        for &inst in &block.instructions {
            let kind = &func.inst(inst).kind;
            let immediate = |value| match func.value(value) {
                Value::Immediate(immediate) => immediate.as_u256(),
                _ => None,
            };
            cost += self.op(&kind.op(), immediate);
            kind.visit_operands(|operand| {
                if let Some(value) = immediate(operand) {
                    cost += self.push(value);
                }
            });
        }
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

    /// Deployment-lifetime gas of `cost`: expected executions of its runtime
    /// gas plus the price of its bytes.
    pub(crate) fn lifetime_gas(&self, cost: Cost) -> u128 {
        u128::from(cost.gas) * u128::from(self.expected_executions)
            + u128::from(cost.bytes) * u128::from(self.byte_gas())
    }

    /// The objective order of a cost: the dimension optimized first, then
    /// the other one. Gas leads when optimizing for gas, bytes when
    /// optimizing for size. Creation code ranks by its deployment gas, execution plus
    /// calldata, when optimizing for gas.
    pub(crate) fn objective_key(&self, cost: Cost) -> [u32; 2] {
        match self.optimization {
            OptimizationMode::Size => [cost.bytes, cost.gas],
            _ if self.creation => {
                [u32::try_from(self.lifetime_gas(cost)).unwrap_or(u32::MAX), cost.bytes]
            }
            _ => [cost.gas, cost.bytes],
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

    /// Cost of a jump to a block, including the pushed label and the target's landing.
    pub(crate) fn jump(&self) -> Cost {
        self.label_push() + self.opcode(op::JUMP) + self.opcode(op::JUMPDEST)
    }

    /// Cost of a conditional edge, including the pushed label and the target's landing.
    pub(crate) fn branch(&self) -> Cost {
        self.label_push() + self.opcode(op::JUMPI) + self.opcode(op::JUMPDEST)
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
        let loop_body = self.jump()
            + self.branch()
            + self.dup().times(7)
            + self.opcode(op::ADD).times(4)
            + self.opcode(op::LT)
            + self.opcode(op::MLOAD)
            + self.opcode(op::MSTORE)
            + self.opcode(op::POP).times(3)
            + self.push(U256::ZERO)
            + self.push(U256::from(word_size));
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
}

/// The objective code is optimized for: the build's, unless its source selects one with
/// `@custom:solar-optimize`. A selection only picks what an optimized build optimizes for, so code
/// built with `-O none` stays unoptimized.
pub(crate) fn objective(
    build: OptimizationMode,
    selected: Option<OptimizationMode>,
) -> OptimizationMode {
    match (build, selected) {
        (OptimizationMode::None, _) | (_, None) => build,
        (_, Some(selected)) => selected,
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
                constants.extend([U256::ONE << bits, (U256::ONE << bits) - U256::ONE]);
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
        assert_eq!(target.op_at(&load, |_| None, Warmth::Warm), Cost::new(100, 1));
        assert_eq!(target.op(&load, |_| None), Cost::new(2100, 1));
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
        assert_eq!(target.select(false), Cost::new(17, 5));
        assert_eq!(target.select(true), Cost::new(20, 6));
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
        assert_eq!(exponent(None), Cost::new(60, 1));
        assert_eq!(exponent(Some(U256::from(255))), Cost::new(60, 1));
        assert_eq!(exponent(Some(U256::from(1 << 16))), Cost::new(160, 1));
        assert_eq!(exponent(Some(U256::ZERO)), Cost::new(10, 1));
        assert_eq!(
            target.opcode_with_immediates(op::EXP, &[Some(U256::from(2)), Some(U256::from(256))]),
            Cost::new(110, 1),
        );
        assert_eq!(
            Target::with(EvmVersion::Homestead, OptimizationMode::Gas, 200)
                .opcode_with_immediates(op::EXP, &[Some(U256::from(2)), Some(U256::from(256))]),
            Cost::new(30, 1),
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
        assert_eq!(target.block_code_estimate(&function, BlockId::ENTRY), Cost::new(21, 10));
        assert_eq!(target.block_code_estimate(&function, exit), Cost::ZERO);
        assert_eq!(target.code_estimate(&function), Cost::new(21, 10));
    }

    #[test]
    fn pushes_and_stack_ops() {
        let target = Target::with(EvmVersion::Osaka, OptimizationMode::Gas, 200);
        assert_eq!(target.dup(), Cost::new(3, 1));
        assert_eq!(target.opcode(op::POP), Cost::new(2, 1));
        assert_eq!(target.opcode(op::PUSH2), Cost::new(3, 3));
        assert_eq!(target.label_push(), Cost::new(3, 3));
        assert_eq!(target.push(U256::ZERO), Cost::new(2, 1));
        assert_eq!(target.push(U256::from(0x1234)), Cost::new(3, 3));
        assert_eq!(target.copied_words(64), Cost::new(6, 0));
        let legacy = Target::with(EvmVersion::Paris, OptimizationMode::Gas, 200);
        assert_eq!(legacy.push(U256::ZERO), Cost::new(3, 2));
        assert_eq!(target.stack_op(op::StackOp::Swap(16)), Some(Cost::new(3, 1)));
        assert_eq!(target.stack_op(op::StackOp::Pop), Some(Cost::new(2, 1)));
        assert_eq!(target.stack_op(op::StackOp::Exchange(1, 2)), Some(Cost::new(9, 3)));
        assert_eq!(target.stack_op(op::StackOp::Exchange(1, 17)), None);
        let amsterdam = Target::with(EvmVersion::Amsterdam, OptimizationMode::Gas, 200);
        assert_eq!(amsterdam.stack_op(op::StackOp::Dup(17)), Some(Cost::new(3, 2)));
        assert_eq!(amsterdam.stack_op(op::StackOp::Exchange(1, 2)), Some(Cost::new(3, 2)));
        assert!(Cost::new(3, 2).dominates(Cost::new(9, 3)));
        assert!(Cost::new(9, 2).dominates(Cost::new(9, 3)));
        assert!(!Cost::new(9, 3).dominates(Cost::new(9, 3)));
        assert!(!Cost::new(3, 4).dominates(Cost::new(9, 3)));
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
    fn helper_sharing_prices_the_call_protocol_per_site() {
        let target = Target::with(EvmVersion::Osaka, OptimizationMode::Size, 200);
        let (shared, expanded) = target.helper_sharing(100, 2, 1, 3);
        // Three sites each pay a call with a 5-word frame and a return. Each emits two label
        // pushes, a jump, a landing, and three moves; the helper adds a landing, one body, a
        // result move, and the return jump.
        assert_eq!(shared, Cost::new(3 * ((90 + 3 * 24 + 5 * 6) + (24 + 3 * 8)), 3 * 11 + 103));
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
    fn control_flow_costs_distinguish_unconditional_edges() {
        for version in
            [EvmVersion::Homestead, EvmVersion::Paris, EvmVersion::Osaka, EvmVersion::Amsterdam]
        {
            let target = Target::with(version, OptimizationMode::Gas, 200);
            assert_eq!(target.jump(), Cost::new(12, 5));
            assert_eq!(target.branch(), Cost::new(14, 5));
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
    fn tags_select_the_objective_of_optimized_builds() {
        let (none, gas, size) =
            (OptimizationMode::None, OptimizationMode::Gas, OptimizationMode::Size);
        assert_eq!(objective(gas, None), gas);
        assert_eq!(objective(gas, Some(size)), size);
        assert_eq!(objective(size, Some(gas)), gas);
        assert_eq!(objective(none, Some(size)), none);
    }
}
