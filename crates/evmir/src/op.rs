//! EVM opcode definitions and metadata.

use crate::gas::GasTier;
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_config::EvmVersion;
use solar_interface::Symbol;
use std::fmt::Write as _;

pub mod word;

/// Number of bytes in an EVM word.
pub const WORD_BYTES: usize = 32;

/// Maximum number of words on the EVM stack.
pub const MAX_STACK_DEPTH: usize = 1024;

const UNKNOWN_PREFIX: &str = "op_";

/// Declarative properties of an EVM operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OpcodeTraits(u16);

impl OpcodeTraits {
    /// No additional properties.
    pub const NONE: Self = Self(0);

    /// Returns the union of two property sets.
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Returns whether this set contains `trait_`.
    pub const fn contains(self, trait_: Self) -> bool {
        self.0 & trait_.0 == trait_.0
    }
}

/// Availability of an EVM operation across EVM versions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Availability {
    /// Available in every supported EVM version.
    Legacy,
    /// Available from the given EVM version onward.
    Since(EvmVersion),
    /// Gated by [`EvmVersion::has_slot_num`].
    SlotNum,
    /// Gated by [`EvmVersion::has_extended_stack_ops`].
    ExtendedStackOps,
}

/// Declarative metadata for one EVM operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpDef {
    /// Opcode byte.
    pub opcode: u8,
    /// Canonical textual mnemonic.
    pub mnemonic: &'static str,
    /// Number of stack items consumed and produced, unless an immediate operand decides it.
    pub stack_io: Option<(u8, u8)>,
    /// Declarative operation properties.
    pub traits: OpcodeTraits,
    /// Maximum significant result bits; unknown results use the full word.
    pub result_bits: u16,
    /// Observed low bits per operand, in pop order; omitted operands use the full word.
    pub input_bits: &'static [u16],
    /// Gas class in the fork schedule.
    pub gas: GasTier,
    /// Availability across EVM versions.
    pub availability: Availability,
    /// The opcode that computes the same result with the top two stack operands swapped.
    pub swapped: Option<u8>,
    /// Number of immediate bytes that follow the opcode.
    pub immediate: u8,
}

include!(concat!(env!("OUT_DIR"), "/evm_ops.rs"));

/// Returns the generated schema definition for an opcode.
#[must_use]
pub const fn definition(opcode: u8) -> Option<&'static OpDef> {
    match &OPCODE_DEFS[opcode as usize] {
        Some(definition) => Some(definition),
        None => None,
    }
}

/// Returns the canonical mnemonic for an opcode.
#[must_use]
pub const fn mnemonic(opcode: u8) -> Option<&'static str> {
    match definition(opcode) {
        Some(definition) => Some(definition.mnemonic),
        None => None,
    }
}

/// Formats an opcode using its canonical mnemonic or `op_<hex>`.
pub(crate) fn fmt(opcode: u8, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    if let Some(mnemonic) = mnemonic(opcode) {
        f.write_str(mnemonic)
    } else {
        write!(f, "{UNKNOWN_PREFIX}{opcode:02x}")
    }
}

/// Parses a canonical mnemonic or `op_<hex>` into an opcode.
#[must_use]
pub(crate) fn from_ir_mnemonic(mnemonic: &str) -> Option<u8> {
    from_mnemonic(mnemonic).or_else(|| {
        let value = mnemonic.strip_prefix(UNKNOWN_PREFIX)?;
        u8::from_str_radix(value, 16).ok()
    })
}

/// Parses an interned canonical mnemonic or `op_<hex>` into an opcode.
#[must_use]
pub(crate) fn from_ir_symbol(mnemonic: Symbol) -> Option<u8> {
    from_ir_mnemonic(mnemonic.as_str())
}

/// Returns the number of stack items consumed and produced by an opcode.
#[must_use]
pub const fn stack_io(opcode: u8) -> Option<(u8, u8)> {
    match definition(opcode) {
        Some(definition) => definition.stack_io,
        None => None,
    }
}

/// Returns the encoded length of a minimally sized PUSH for an EVM version.
pub fn push_len(evm_version: EvmVersion, value: U256) -> usize {
    usize::from(push_width(evm_version, value)) + 1
}

/// Returns the immediate width of a minimally sized PUSH for an EVM version, where `PUSH0` has
/// width 0.
pub fn push_width(evm_version: EvmVersion, value: U256) -> u8 {
    if value.is_zero() && !evm_version.has_push0() { 1 } else { value.byte_len() as u8 }
}

/// Returns the PUSH opcode for the given width (1-32).
#[must_use]
pub const fn push(width: u8) -> u8 {
    debug_assert!(width >= 1 && width <= 32);
    PUSH1 + width - 1
}

impl OpDef {
    /// Returns whether this operation halts or unconditionally transfers control.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        self.traits.contains(OpcodeTraits::TERMINAL)
    }

    /// Returns whether this operation is available for `evm_version`.
    #[must_use]
    pub fn is_available(self, evm_version: EvmVersion) -> bool {
        match self.availability {
            Availability::Legacy => true,
            Availability::Since(version) => evm_version >= version,
            Availability::SlotNum => evm_version.has_slot_num(),
            Availability::ExtendedStackOps => evm_version.has_extended_stack_ops(),
        }
    }

    /// Returns whether the top two stack operands of this operation may be swapped without changing
    /// its result.
    #[must_use]
    pub const fn is_commutative(self) -> bool {
        self.traits.contains(OpcodeTraits::COMMUTATIVE)
    }

    /// Returns whether this operation is a pure function of its stack operands.
    #[must_use]
    pub const fn is_pure(self) -> bool {
        self.traits.contains(OpcodeTraits::PURE)
    }

    /// Returns whether this operation only reads the executing call's context.
    #[must_use]
    pub const fn is_context_read(self) -> bool {
        self.traits.contains(OpcodeTraits::CONTEXT_READ)
    }

    /// Returns whether this operation replaces the executing call's return data buffer.
    #[must_use]
    pub const fn sets_returndata(self) -> bool {
        self.traits.contains(OpcodeTraits::SETS_RETURNDATA)
    }

    /// Returns whether this operation may write to memory.
    #[must_use]
    pub const fn writes_memory(self) -> bool {
        self.traits.contains(OpcodeTraits::WRITES_MEMORY)
    }

    /// Returns whether this operation may write to storage or transient storage.
    #[must_use]
    pub const fn writes_storage(self) -> bool {
        self.traits.contains(OpcodeTraits::WRITES_STORAGE)
    }

    /// Returns whether this operation depends on the gas left.
    #[must_use]
    pub const fn observes_gas(self) -> bool {
        self.traits.contains(OpcodeTraits::OBSERVES_GAS)
    }

    /// Returns whether this operation behaves the same wherever it runs: it has a fixed stack
    /// effect and neither transfers control nor observes its code position or the gas left. Code
    /// may move such an operation to another position, such as a shared body or a copied tail.
    #[must_use]
    pub const fn is_position_independent(self) -> bool {
        self.stack_io.is_some()
            && !self.is_terminal()
            && !self.observes_gas()
            && !self.traits.contains(OpcodeTraits::POSITION_DEPENDENT)
    }
}

/// Returns the ISLE declarations of the opcode vocabulary: the primitive
/// types EVM IR rules use and one constant per opcode byte.
#[must_use]
pub fn isle_prelude() -> String {
    let mut out = String::from(
        ";; Generated from the EVM opcode table by `op::isle_prelude`; do not edit.\n\
         ;; `cargo nextest run -p solar-codegen evm_isle_prelude` checks this file and\n\
         ;; `SNAPSHOTS=overwrite` refreshes it.\n\n\
         ;; Opcode bytes, named as the constants in `backend::evm::op`.\n",
    );
    for opcode in u8::MIN..=u8::MAX {
        if let Some(definition) = definition(opcode) {
            writeln!(out, "(extern const ${} u8)", definition.mnemonic.to_ascii_uppercase())
                .unwrap();
        }
    }
    out
}

/// Returns the DUP opcode for the given depth (1-16).
#[must_use]
pub const fn dup(n: u8) -> u8 {
    debug_assert!(n >= 1 && n <= 16);
    DUP1 + n - 1
}

/// Returns the SWAP opcode for the given depth (1-16).
#[must_use]
pub const fn swap(n: u8) -> u8 {
    debug_assert!(n >= 1 && n <= 16);
    SWAP1 + n - 1
}

/// A logical EVM stack operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StackOp {
    /// Duplicate the nth stack element.
    Dup(u8),
    /// Swap the top with the nth stack element below it.
    Swap(u8),
    /// Swap two non-top stack elements.
    Exchange(u8, u8),
    /// Remove the top stack element.
    Pop,
}

/// Where a word is after a [`StackOp`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackedWord {
    /// The word is this many positions below the top, counting the top as zero.
    At(usize),
    /// As `At`, and the operation also put a copy of the word on top.
    Copied(usize),
    /// The operation discarded the word.
    Dropped,
}

/// Declarative metadata for a logical EVM stack operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StackOpDef {
    /// Canonical textual mnemonic.
    pub mnemonic: &'static str,
    /// Placeholder opcode used in EVM IR.
    pub ir_opcode: u8,
    /// Gas class of one lowered instruction.
    pub gas: GasTier,
}

/// Target-specific lowering of one logical stack operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StackOpLowering {
    /// One opcode with an optional immediate byte.
    Direct(u8, Option<u8>),
    /// Three `SWAP` opcodes implementing a shallow `EXCHANGE`.
    SwapSequence([u8; 3]),
}

/// Exact cost of a lowered stack operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StackOpMetrics {
    pub static_gas: usize,
    pub assembled_len: usize,
    pub instruction_count: usize,
}

impl StackOp {
    /// Returns the stack depth required to execute this operation.
    #[must_use]
    pub const fn required_depth(self) -> usize {
        match self {
            Self::Dup(depth) => depth as usize,
            Self::Swap(depth) => depth as usize + 1,
            Self::Exchange(first, second) => {
                if first > second {
                    first as usize + 1
                } else {
                    second as usize + 1
                }
            }
            Self::Pop => 1,
        }
    }

    /// Returns where the word `word` positions below the top, counting the top as zero, is after
    /// this operation.
    #[must_use]
    pub const fn track(self, word: usize) -> TrackedWord {
        const fn swapped(word: usize, a: usize, b: usize) -> usize {
            if word == a {
                b
            } else if word == b {
                a
            } else {
                word
            }
        }
        match self {
            Self::Dup(depth) if word + 1 == depth as usize => TrackedWord::Copied(word + 1),
            Self::Dup(_) => TrackedWord::At(word + 1),
            Self::Swap(depth) => TrackedWord::At(swapped(word, 0, depth as usize)),
            Self::Exchange(first, second) => {
                TrackedWord::At(swapped(word, first as usize, second as usize))
            }
            Self::Pop => match word.checked_sub(1) {
                Some(below) => TrackedWord::At(below),
                None => TrackedWord::Dropped,
            },
        }
    }

    /// Returns this operation's net stack growth.
    #[must_use]
    pub const fn net_growth(self) -> isize {
        match self {
            Self::Dup(_) => 1,
            Self::Pop => -1,
            Self::Swap(_) | Self::Exchange(_, _) => 0,
        }
    }

    /// Decodes a one-byte stack opcode.
    #[must_use]
    pub const fn from_single_byte_evm_opcode(opcode: u8) -> Option<Self> {
        match opcode {
            POP => Some(Self::Pop),
            DUP1..=DUP16 => Some(Self::Dup(opcode - DUP1 + 1)),
            SWAP1..=SWAP16 => Some(Self::Swap(opcode - SWAP1 + 1)),
            _ => None,
        }
    }

    /// Returns the one-byte encoding when this operation has one.
    #[must_use]
    pub const fn single_byte_evm_opcode(self) -> Option<u8> {
        match self {
            Self::Dup(n @ 1..=16) => Some(dup(n)),
            Self::Swap(n @ 1..=16) => Some(swap(n)),
            Self::Pop => Some(POP),
            _ => None,
        }
    }

    /// Returns the placeholder opcode used to represent this operation in EVM IR.
    #[must_use]
    pub const fn ir_opcode(self) -> u8 {
        self.definition().ir_opcode
    }

    /// Returns whether the operands are valid independent of the target EVM version.
    #[must_use]
    pub const fn is_valid(self) -> bool {
        match self {
            Self::Dup(n) | Self::Swap(n) => n >= 1 && n <= 235,
            Self::Exchange(n, m) => n >= 1 && n < m && (n as u16) + (m as u16) <= 30,
            Self::Pop => true,
        }
    }

    /// Returns the target-specific lowering of this operation.
    #[must_use]
    pub fn lowering(self, evm_version: EvmVersion) -> Option<StackOpLowering> {
        if !self.is_valid() {
            return None;
        }
        if let Some(opcode) = self.single_byte_evm_opcode() {
            return Some(StackOpLowering::Direct(opcode, None));
        }

        let lowering = match self {
            Self::Dup(n) if evm_version.has_extended_stack_ops() => {
                StackOpLowering::Direct(DUPN, Some(encode_stack_depth(n)))
            }
            Self::Swap(n) if evm_version.has_extended_stack_ops() => {
                StackOpLowering::Direct(SWAPN, Some(encode_stack_depth(n)))
            }
            Self::Exchange(n, m) if evm_version.has_extended_stack_ops() => {
                StackOpLowering::Direct(EXCHANGE, Some(encode_exchange(n, m)))
            }
            Self::Exchange(n, m) if m <= 16 => {
                StackOpLowering::SwapSequence([swap(n), swap(m), swap(n)])
            }
            _ => return None,
        };
        Some(lowering)
    }

    /// Returns the exact cost of this operation after target lowering.
    #[must_use]
    pub fn metrics(self, evm_version: EvmVersion) -> Option<StackOpMetrics> {
        let (assembled_len, instruction_count) = match self.lowering(evm_version)? {
            StackOpLowering::Direct(_, immediate) => (1 + usize::from(immediate.is_some()), 1),
            StackOpLowering::SwapSequence(opcodes) => (opcodes.len(), opcodes.len()),
        };
        let gas_per_instruction = self.definition().gas.gas(evm_version) as usize;
        Some(StackOpMetrics {
            static_gas: instruction_count * gas_per_instruction,
            assembled_len,
            instruction_count,
        })
    }

    /// Returns the exact assembled byte length when this operation can be lowered for the target.
    #[must_use]
    pub fn assembled_len(self, evm_version: EvmVersion) -> Option<usize> {
        self.metrics(evm_version).map(|metrics| metrics.assembled_len)
    }

    /// Returns the `EXCHANGE` represented by a three-swap sequence.
    #[must_use]
    pub const fn from_swaps(first: u8, second: u8, third: u8) -> Option<Self> {
        if first != third || first == second {
            return None;
        }
        let (n, m) = if first < second { (first, second) } else { (second, first) };
        let op = Self::Exchange(n, m);
        if op.is_valid() { Some(op) } else { None }
    }

    /// Matches a cycle through the top that leaves the top in place, `SWAPa SWAPb1 ... SWAPbj
    /// SWAPa` with at least one inner swap, at the start of `ops`. Returns the equivalent
    /// `EXCHANGE a, b1 ... EXCHANGE a, bj` and the number of operations matched.
    pub fn exchange_cycle(
        ops: impl IntoIterator<Item = Self>,
    ) -> Option<(SmallVec<[Self; 4]>, usize)> {
        let mut ops = ops.into_iter();
        let Some(Self::Swap(first)) = ops.next() else { return None };
        let mut exchanges = SmallVec::new();
        for op in ops {
            let Self::Swap(depth) = op else { return None };
            if depth == first {
                let len = exchanges.len() + 2;
                return (len > 2).then_some((exchanges, len));
            }
            exchanges.push(Self::from_swaps(first, depth, first)?);
        }
        None
    }
}

/// Encodes the immediate used by `DUPN` and `SWAPN`.
#[must_use]
pub const fn encode_stack_depth(n: u8) -> u8 {
    debug_assert!(n >= 17 && n <= 235);
    n.wrapping_add(111)
}

/// Decodes a valid `DUPN` or `SWAPN` immediate.
#[must_use]
pub const fn decode_stack_depth(immediate: u8) -> Option<u8> {
    if immediate > 90 && immediate < 128 { None } else { Some(immediate.wrapping_add(145)) }
}

/// Encodes the immediate used by `EXCHANGE`.
#[must_use]
pub const fn encode_exchange(n: u8, m: u8) -> u8 {
    debug_assert!(n >= 1 && n < m && (n as u16) + (m as u16) <= 30);
    let (q, r) = if m <= 16 { (n - 1, m - 1) } else { (29 - m, n - 1) };
    (16 * q + r) ^ 143
}

/// Decodes a valid `EXCHANGE` immediate.
#[must_use]
pub const fn decode_exchange(immediate: u8) -> Option<(u8, u8)> {
    if immediate > 81 && immediate < 128 {
        return None;
    }
    let k = immediate ^ 143;
    let (q, r) = (k / 16, k % 16);
    Some(if q < r { (q + 1, r + 1) } else { (r + 1, 29 - q) })
}

/// Returns whether an opcode halts or unconditionally transfers control.
#[must_use]
pub const fn is_terminal(op: u8) -> bool {
    match definition(op) {
        Some(definition) => definition.is_terminal(),
        None => false,
    }
}

/// Returns whether an opcode is available for `evm_version`.
#[must_use]
pub fn is_available(opcode: u8, evm_version: EvmVersion) -> bool {
    match definition(opcode) {
        Some(definition) => definition.is_available(evm_version),
        None => false,
    }
}

/// Returns whether the top two stack operands of an opcode may be swapped without changing its
/// result.
#[must_use]
pub const fn is_commutative(op: u8) -> bool {
    match definition(op) {
        Some(definition) => definition.is_commutative(),
        None => false,
    }
}

/// Returns the equivalent opcode after swapping its top two stack operands.
#[must_use]
pub const fn swapped_binary_opcode(opcode: u8) -> Option<u8> {
    match definition(opcode) {
        Some(definition) => definition.swapped,
        None => None,
    }
}

/// Returns whether an opcode is pure: it has no side effects and its result is a deterministic
/// function of its stack operands alone (no memory, storage, or environment dependency), so two
/// occurrences with equal operands always produce the same value.
#[must_use]
pub const fn is_pure(op: u8) -> bool {
    match definition(op) {
        Some(definition) => definition.is_pure(),
        None => false,
    }
}

/// Returns whether an opcode runs another frame and so replaces the return data buffer: every
/// call and create.
#[must_use]
pub const fn sets_returndata(op: u8) -> bool {
    match definition(op) {
        Some(definition) => definition.sets_returndata(),
        None => false,
    }
}

/// Returns whether an opcode only reads the executing call's context, with no effect besides its
/// static gas, so an unused result can be dropped.
#[must_use]
pub const fn is_context_read(op: u8) -> bool {
    match definition(op) {
        Some(definition) => definition.is_context_read(),
        None => false,
    }
}

/// Returns whether inserting a push immediately before this opcode preserves its behavior.
///
/// This holds for position-independent operations that produce one word without writing state.
/// It includes opcodes that expand memory or warm an account or storage slot because the push does
/// not affect those changes.
#[must_use]
pub const fn is_unaffected_by_preceding_push(op: u8) -> bool {
    match definition(op) {
        Some(definition) => {
            definition.is_position_independent()
                && matches!(definition.stack_io, Some((_, 1)))
                && !definition.writes_memory()
                && !definition.writes_storage()
                && definition.immediate == 0
        }
        None => false,
    }
}

/// Returns whether an opcode may write to memory, invalidating cached memory reads.
#[must_use]
pub const fn writes_memory(op: u8) -> bool {
    match definition(op) {
        Some(definition) => definition.writes_memory(),
        None => false,
    }
}

/// Returns whether an opcode may write to storage or transient storage, invalidating cached
/// storage reads.
#[must_use]
pub const fn writes_storage(op: u8) -> bool {
    match definition(op) {
        Some(definition) => definition.writes_storage(),
        None => false,
    }
}

/// Returns whether an opcode reads nothing but its operands and calldata, so it gives the same
/// result wherever it runs within a transaction's frame.
#[must_use]
pub const fn reads_only_operands_or_calldata(op: u8) -> bool {
    match definition(op) {
        Some(definition) => {
            definition.is_pure() || definition.traits.contains(OpcodeTraits::CALLDATA_READ)
        }
        None => false,
    }
}

/// Returns whether an opcode is a memory or storage load or store, or `KECCAK256`: an access
/// whose only effect is on the words it names.
#[must_use]
pub const fn is_plain_access(op: u8) -> bool {
    match definition(op) {
        Some(definition) => definition.traits.contains(OpcodeTraits::PLAIN_ACCESS),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opcode_table() {
        use std::fmt::Write;

        const VERSIONS: [EvmVersion; 15] = [
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
        ];

        let mut table = String::new();
        for opcode in u8::MIN..=u8::MAX {
            let Some(def) = definition(opcode) else { continue };
            write!(table, "0x{opcode:02x} {:<16}", def.mnemonic).unwrap();
            match def.stack_io {
                Some((inputs, outputs)) => write!(table, " io={inputs}/{outputs}").unwrap(),
                None => table.push_str(" io=?"),
            }
            assert!(def.result_bits <= 256);
            assert!(def.input_bits.iter().all(|&bits| bits <= 256));
            assert!(
                def.input_bits.len() <= usize::from(def.stack_io.map_or(0, |(inputs, _)| inputs))
            );
            if def.result_bits != 256 || !def.input_bits.is_empty() {
                write!(table, " bits={:?}/{}", def.input_bits, def.result_bits).unwrap();
            }
            let mut available = VERSIONS.into_iter().filter(|&version| def.is_available(version));
            match available.next() {
                Some(first) => {
                    // Availability never regresses, so the first version describes it fully.
                    assert!(
                        VERSIONS
                            .into_iter()
                            .filter(|&version| version >= first)
                            .all(|version| def.is_available(version)),
                        "{} availability is not monotonic",
                        def.mnemonic
                    );
                    write!(table, " since={first}").unwrap();
                }
                None => table.push_str(" since=never"),
            }
            write!(table, " gas={}", def.gas.gas(EvmVersion::Homestead)).unwrap();
            let mut previous = def.gas.gas(EvmVersion::Homestead);
            for version in VERSIONS {
                let gas = def.gas.gas(version);
                if gas != previous {
                    write!(table, " {version}={gas}").unwrap();
                    previous = gas;
                }
            }
            let dynamic = def.gas.dynamic_gas(EvmVersion::Amsterdam);
            if dynamic != 0 {
                write!(table, " +{dynamic}").unwrap();
            }
            let traits = [
                ("pure", def.is_pure()),
                ("commutative", def.is_commutative()),
                ("writes_memory", def.writes_memory()),
                ("writes_storage", def.writes_storage()),
                ("terminal", def.is_terminal()),
                ("observes_gas", def.observes_gas()),
                ("context_read", def.is_context_read()),
                ("sets_returndata", def.sets_returndata()),
            ];
            for (name, set) in traits {
                if set {
                    table.push(' ');
                    table.push_str(name);
                }
            }
            table.push('\n');
        }
        snapbox::assert_data_eq!(table, snapbox::file!["op_table.snap"]);
    }
}
