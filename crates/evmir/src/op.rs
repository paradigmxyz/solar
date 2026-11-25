//! EVM opcode definitions and metadata.

use crate::gas::GasTier;
use alloy_primitives::U256;
use evm2_bytecode::{SpecId, encode_pair, encode_single, opcode::OpCode};
use smallvec::SmallVec;
use solar_config::EvmVersion;
use solar_interface::Symbol;
use std::fmt::Write as _;

pub mod word;
pub use word::eval_opcode;

/// Number of bytes in an EVM word.
pub const WORD_BYTES: usize = 32;

/// Maximum number of words on the EVM stack.
pub const MAX_STACK_DEPTH: usize = 1024;

const UNKNOWN_PREFIX: &str = "op_";

/// Declarative properties of an EVM operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OpcodeTraits(u8);

impl OpcodeTraits {
    /// No additional properties.
    pub const NONE: Self = Self(0);
    /// The result is a deterministic function of the stack operands alone: no memory, storage,
    /// or environment dependency, so two occurrences with equal operands produce the same value.
    pub const PURE: Self = Self(1 << 0);
    /// The top two stack operands may be swapped without changing the result; deeper operands keep
    /// their place.
    pub const COMMUTATIVE: Self = Self(1 << 1);
    /// The operation may write to memory, invalidating cached memory reads.
    pub const WRITES_MEMORY: Self = Self(1 << 2);
    /// The operation may write to storage or transient storage, invalidating cached storage
    /// reads.
    pub const WRITES_STORAGE: Self = Self(1 << 3);
    /// The operation halts or unconditionally transfers control.
    pub const TERMINAL: Self = Self(1 << 4);
    /// The operation depends on the gas left: `GAS` reads it, calls and creates forward all but
    /// a 64th of it, and `SSTORE` fails unless more than the 2,300-gas call stipend remains. Work
    /// that moves before it changes what it sees.
    pub const OBSERVES_GAS: Self = Self(1 << 5);
    /// The operation only reads the executing call's context: the result touches no memory,
    /// storage, or other account, and the operation has no effect besides its static gas, so an
    /// unused result can be dropped. Account, storage and memory reads are not context reads:
    /// they warm what they touch or expand memory.
    pub const CONTEXT_READ: Self = Self(1 << 6);
    /// The operation runs another frame, so the executing call's return data buffer afterwards
    /// holds that frame's output: every call and create.
    pub const SETS_RETURNDATA: Self = Self(1 << 7);

    /// Returns the union of two property sets.
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Returns whether this set contains `trait_`.
    pub const fn contains(self, trait_: Self) -> bool {
        self.0 & trait_.0 == trait_.0
    }
}

macro_rules! opcode_mnemonic {
    (r#return) => {
        "return"
    };
    ($mnemonic:ident) => {
        stringify!($mnemonic)
    };
}

macro_rules! opcode_traits {
    () => {
        OpcodeTraits::NONE
    };
    ($($trait:ident)|+) => {
        OpcodeTraits::NONE $(.union(OpcodeTraits::$trait))+
    };
}

/// Maps a row's gas class name to its [`GasTier`].
macro_rules! opcode_gas {
    (zero) => {
        GasTier::Zero
    };
    (jumpdest) => {
        GasTier::Jumpdest
    };
    (base) => {
        GasTier::Base
    };
    (verylow) => {
        GasTier::VeryLow
    };
    (low) => {
        GasTier::Low
    };
    (mid) => {
        GasTier::Mid
    };
    (high) => {
        GasTier::High
    };
    (exp) => {
        GasTier::Exp
    };
    (keccak) => {
        GasTier::Keccak
    };
    (copy) => {
        GasTier::Copy
    };
    (blockhash) => {
        GasTier::BlockHash
    };
    (balance) => {
        GasTier::Balance
    };
    (extcode) => {
        GasTier::ExtCode
    };
    (extcodehash) => {
        GasTier::ExtCodeHash
    };
    (sload) => {
        GasTier::SLoad
    };
    (sstore) => {
        GasTier::SStore
    };
    (transient) => {
        GasTier::Transient
    };
    (log0) => {
        GasTier::Log(0)
    };
    (log1) => {
        GasTier::Log(1)
    };
    (log2) => {
        GasTier::Log(2)
    };
    (log3) => {
        GasTier::Log(3)
    };
    (log4) => {
        GasTier::Log(4)
    };
    (create) => {
        GasTier::Create
    };
    (call) => {
        GasTier::Call
    };
    (selfdestruct) => {
        GasTier::SelfDestruct
    };
}

macro_rules! opcode_result_bits {
    () => {
        256
    };
    ($bits:literal) => {
        $bits
    };
}

macro_rules! opcodes {
    ($(
        $opcode:literal => $constant:ident => $mnemonic:ident
            => traits($($trait:ident)|*)
            => gas($gas:tt)
            $(=> result_bits($result_bits:literal))?
            $(=> input_bits($($input_bits:literal),*))?;
    )*) => {
        $(
            #[doc = concat!("Opcode byte for `", stringify!($constant), "`.")]
            #[allow(dead_code)]
            pub const $constant: u8 = $opcode;
        )*

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
        }

        /// Maps each opcode byte to its generated schema definition.
        static OPCODE_DEFS: [Option<OpDef>; 256] = {
            let mut map = [None; 256];
            let mut prev = 0;
            $(
                let opcode: u8 = $opcode;
                assert!(opcode == 0 || opcode > prev, "opcodes must be sorted in ascending order");
                prev = opcode;
                map[opcode as usize] = Some(OpDef {
                    opcode,
                    mnemonic: opcode_mnemonic!($mnemonic),
                    stack_io: match opcode {
                        // The immediate operand decides these.
                        DUPN | SWAPN | EXCHANGE => None,
                        _ => Some(OpCode::new_or_unknown(opcode).input_output()),
                    },
                    traits: opcode_traits!($($trait)|*),
                    result_bits: opcode_result_bits!($($result_bits)?),
                    input_bits: &[$($($input_bits),*)?],
                    gas: opcode_gas!($gas),
                });
            )*
            let _ = prev;
            map
        };

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

        /// Returns the opcode for a canonical mnemonic.
        #[must_use]
        pub fn from_mnemonic(mnemonic: &str) -> Option<u8> {
            match mnemonic {
                $(opcode_mnemonic!($mnemonic) => Some($opcode),)*
                _ => None,
            }
        }

        /// Formats an opcode using its canonical mnemonic or `op_<hex>`.
        pub fn fmt(opcode: u8, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            if let Some(mnemonic) = mnemonic(opcode) {
                f.write_str(mnemonic)
            } else {
                write!(f, "{UNKNOWN_PREFIX}{opcode:02x}")
            }
        }

        /// Parses a canonical mnemonic or `op_<hex>` into an opcode.
        #[must_use]
        pub fn from_ir_mnemonic(mnemonic: &str) -> Option<u8> {
            from_mnemonic(mnemonic).or_else(|| {
                let value = mnemonic.strip_prefix(UNKNOWN_PREFIX)?;
                u8::from_str_radix(value, 16).ok()
            })
        }

        /// Parses an interned canonical mnemonic or `op_<hex>` into an opcode.
        #[must_use]
        pub fn from_ir_symbol(mnemonic: Symbol) -> Option<u8> {
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
    };
}

opcodes! {
    0x00 => STOP => stop => traits(TERMINAL) => gas(zero);
    0x01 => ADD => add => traits(PURE | COMMUTATIVE) => gas(verylow);
    0x02 => MUL => mul => traits(PURE | COMMUTATIVE) => gas(low);
    0x03 => SUB => sub => traits(PURE) => gas(verylow);
    0x04 => DIV => div => traits(PURE) => gas(low);
    0x05 => SDIV => sdiv => traits(PURE) => gas(low);
    0x06 => MOD => mod => traits(PURE) => gas(low);
    0x07 => SMOD => smod => traits(PURE) => gas(low);
    0x08 => ADDMOD => addmod => traits(PURE | COMMUTATIVE) => gas(mid);
    0x09 => MULMOD => mulmod => traits(PURE | COMMUTATIVE) => gas(mid);
    0x0a => EXP => exp => traits(PURE) => gas(exp);
    0x0b => SIGNEXTEND => signextend => traits(PURE) => gas(low);
    0x10 => LT => lt => traits(PURE) => gas(verylow) => result_bits(1);
    0x11 => GT => gt => traits(PURE) => gas(verylow) => result_bits(1);
    0x12 => SLT => slt => traits(PURE) => gas(verylow) => result_bits(1);
    0x13 => SGT => sgt => traits(PURE) => gas(verylow) => result_bits(1);
    0x14 => EQ => eq => traits(PURE | COMMUTATIVE) => gas(verylow) => result_bits(1);
    0x15 => ISZERO => iszero => traits(PURE) => gas(verylow) => result_bits(1);
    0x16 => AND => and => traits(PURE | COMMUTATIVE) => gas(verylow);
    0x17 => OR => or => traits(PURE | COMMUTATIVE) => gas(verylow);
    0x18 => XOR => xor => traits(PURE | COMMUTATIVE) => gas(verylow);
    0x19 => NOT => not => traits(PURE) => gas(verylow);
    0x1a => BYTE => byte => traits(PURE) => gas(verylow) => result_bits(8);
    0x1b => SHL => shl => traits(PURE) => gas(verylow);
    0x1c => SHR => shr => traits(PURE) => gas(verylow);
    0x1d => SAR => sar => traits(PURE) => gas(verylow);
    0x1e => CLZ => clz => traits(PURE) => gas(low) => result_bits(9);
    0x20 => KECCAK256 => keccak256 => traits() => gas(keccak);
    0x30 => ADDRESS => address => traits(CONTEXT_READ) => gas(base) => result_bits(160);
    0x31 => BALANCE => balance => traits() => gas(balance) => input_bits(160);
    0x32 => ORIGIN => origin => traits(CONTEXT_READ) => gas(base) => result_bits(160);
    0x33 => CALLER => caller => traits(CONTEXT_READ) => gas(base) => result_bits(160);
    0x34 => CALLVALUE => callvalue => traits(CONTEXT_READ) => gas(base);
    0x35 => CALLDATALOAD => calldataload => traits(CONTEXT_READ) => gas(verylow);
    // Calldata and code lengths are gas-bounded, even before code-size caps, and fit in 64 bits.
    0x36 => CALLDATASIZE => calldatasize => traits(CONTEXT_READ) => gas(base) => result_bits(64);
    0x37 => CALLDATACOPY => calldatacopy => traits(WRITES_MEMORY) => gas(copy);
    0x38 => CODESIZE => codesize => traits(CONTEXT_READ) => gas(base) => result_bits(64);
    0x39 => CODECOPY => codecopy => traits(WRITES_MEMORY) => gas(copy);
    0x3a => GASPRICE => gasprice => traits(CONTEXT_READ) => gas(base);
    0x3b => EXTCODESIZE => extcodesize => traits() => gas(extcode) => result_bits(64) => input_bits(160);
    0x3c => EXTCODECOPY => extcodecopy => traits(WRITES_MEMORY) => gas(extcode) => input_bits(160);
    // Return data comes from gas-bounded EVM memory or precompile output, so its length fits in 64 bits.
    0x3d => RETURNDATASIZE => returndatasize => traits(CONTEXT_READ) => gas(base) => result_bits(64);
    0x3e => RETURNDATACOPY => returndatacopy => traits(WRITES_MEMORY) => gas(copy);
    0x3f => EXTCODEHASH => extcodehash => traits() => gas(extcodehash) => input_bits(160);
    0x40 => BLOCKHASH => blockhash => traits() => gas(blockhash);
    0x41 => COINBASE => coinbase => traits(CONTEXT_READ) => gas(base) => result_bits(160);
    0x42 => TIMESTAMP => timestamp => traits(CONTEXT_READ) => gas(base);
    0x43 => NUMBER => number => traits(CONTEXT_READ) => gas(base);
    0x44 => PREVRANDAO => prevrandao => traits(CONTEXT_READ) => gas(base);
    0x45 => GASLIMIT => gaslimit => traits(CONTEXT_READ) => gas(base);
    0x46 => CHAINID => chainid => traits(CONTEXT_READ) => gas(base);
    0x47 => SELFBALANCE => selfbalance => traits(CONTEXT_READ) => gas(low);
    0x48 => BASEFEE => basefee => traits(CONTEXT_READ) => gas(base);
    0x49 => BLOBHASH => blobhash => traits(CONTEXT_READ) => gas(verylow);
    0x4a => BLOBBASEFEE => blobbasefee => traits(CONTEXT_READ) => gas(base);
    0x4b => SLOTNUM => slotnum => traits() => gas(base);
    0x50 => POP => pop => traits() => gas(base) => input_bits(0);
    0x51 => MLOAD => mload => traits() => gas(verylow);
    0x52 => MSTORE => mstore => traits(WRITES_MEMORY) => gas(verylow);
    0x53 => MSTORE8 => mstore8 => traits(WRITES_MEMORY) => gas(verylow) => input_bits(256, 8);
    0x54 => SLOAD => sload => traits() => gas(sload);
    0x55 => SSTORE => sstore => traits(WRITES_STORAGE | OBSERVES_GAS) => gas(sstore);
    0x56 => JUMP => jump => traits(TERMINAL) => gas(mid);
    0x57 => JUMPI => jumpi => traits() => gas(high);
    0x58 => PC => pc => traits(CONTEXT_READ) => gas(base);
    0x59 => MSIZE => msize => traits(CONTEXT_READ) => gas(base);
    0x5a => GAS => gas => traits(CONTEXT_READ | OBSERVES_GAS) => gas(base);
    0x5b => JUMPDEST => jumpdest => traits() => gas(jumpdest);
    0x5c => TLOAD => tload => traits() => gas(transient);
    0x5d => TSTORE => tstore => traits(WRITES_STORAGE) => gas(transient);
    0x5e => MCOPY => mcopy => traits(WRITES_MEMORY) => gas(copy);
    0x5f => PUSH0 => push0 => traits() => gas(base);
    0x60 => PUSH1 => push1 => traits() => gas(verylow);
    0x61 => PUSH2 => push2 => traits() => gas(verylow);
    0x62 => PUSH3 => push3 => traits() => gas(verylow);
    0x63 => PUSH4 => push4 => traits() => gas(verylow);
    0x64 => PUSH5 => push5 => traits() => gas(verylow);
    0x65 => PUSH6 => push6 => traits() => gas(verylow);
    0x66 => PUSH7 => push7 => traits() => gas(verylow);
    0x67 => PUSH8 => push8 => traits() => gas(verylow);
    0x68 => PUSH9 => push9 => traits() => gas(verylow);
    0x69 => PUSH10 => push10 => traits() => gas(verylow);
    0x6a => PUSH11 => push11 => traits() => gas(verylow);
    0x6b => PUSH12 => push12 => traits() => gas(verylow);
    0x6c => PUSH13 => push13 => traits() => gas(verylow);
    0x6d => PUSH14 => push14 => traits() => gas(verylow);
    0x6e => PUSH15 => push15 => traits() => gas(verylow);
    0x6f => PUSH16 => push16 => traits() => gas(verylow);
    0x70 => PUSH17 => push17 => traits() => gas(verylow);
    0x71 => PUSH18 => push18 => traits() => gas(verylow);
    0x72 => PUSH19 => push19 => traits() => gas(verylow);
    0x73 => PUSH20 => push20 => traits() => gas(verylow);
    0x74 => PUSH21 => push21 => traits() => gas(verylow);
    0x75 => PUSH22 => push22 => traits() => gas(verylow);
    0x76 => PUSH23 => push23 => traits() => gas(verylow);
    0x77 => PUSH24 => push24 => traits() => gas(verylow);
    0x78 => PUSH25 => push25 => traits() => gas(verylow);
    0x79 => PUSH26 => push26 => traits() => gas(verylow);
    0x7a => PUSH27 => push27 => traits() => gas(verylow);
    0x7b => PUSH28 => push28 => traits() => gas(verylow);
    0x7c => PUSH29 => push29 => traits() => gas(verylow);
    0x7d => PUSH30 => push30 => traits() => gas(verylow);
    0x7e => PUSH31 => push31 => traits() => gas(verylow);
    0x7f => PUSH32 => push32 => traits() => gas(verylow);
    0x80 => DUP1 => dup1 => traits() => gas(verylow);
    0x81 => DUP2 => dup2 => traits() => gas(verylow);
    0x82 => DUP3 => dup3 => traits() => gas(verylow);
    0x83 => DUP4 => dup4 => traits() => gas(verylow);
    0x84 => DUP5 => dup5 => traits() => gas(verylow);
    0x85 => DUP6 => dup6 => traits() => gas(verylow);
    0x86 => DUP7 => dup7 => traits() => gas(verylow);
    0x87 => DUP8 => dup8 => traits() => gas(verylow);
    0x88 => DUP9 => dup9 => traits() => gas(verylow);
    0x89 => DUP10 => dup10 => traits() => gas(verylow);
    0x8a => DUP11 => dup11 => traits() => gas(verylow);
    0x8b => DUP12 => dup12 => traits() => gas(verylow);
    0x8c => DUP13 => dup13 => traits() => gas(verylow);
    0x8d => DUP14 => dup14 => traits() => gas(verylow);
    0x8e => DUP15 => dup15 => traits() => gas(verylow);
    0x8f => DUP16 => dup16 => traits() => gas(verylow);
    0x90 => SWAP1 => swap1 => traits() => gas(verylow);
    0x91 => SWAP2 => swap2 => traits() => gas(verylow);
    0x92 => SWAP3 => swap3 => traits() => gas(verylow);
    0x93 => SWAP4 => swap4 => traits() => gas(verylow);
    0x94 => SWAP5 => swap5 => traits() => gas(verylow);
    0x95 => SWAP6 => swap6 => traits() => gas(verylow);
    0x96 => SWAP7 => swap7 => traits() => gas(verylow);
    0x97 => SWAP8 => swap8 => traits() => gas(verylow);
    0x98 => SWAP9 => swap9 => traits() => gas(verylow);
    0x99 => SWAP10 => swap10 => traits() => gas(verylow);
    0x9a => SWAP11 => swap11 => traits() => gas(verylow);
    0x9b => SWAP12 => swap12 => traits() => gas(verylow);
    0x9c => SWAP13 => swap13 => traits() => gas(verylow);
    0x9d => SWAP14 => swap14 => traits() => gas(verylow);
    0x9e => SWAP15 => swap15 => traits() => gas(verylow);
    0x9f => SWAP16 => swap16 => traits() => gas(verylow);
    0xa0 => LOG0 => log0 => traits() => gas(log0);
    0xa1 => LOG1 => log1 => traits() => gas(log1);
    0xa2 => LOG2 => log2 => traits() => gas(log2);
    0xa3 => LOG3 => log3 => traits() => gas(log3);
    0xa4 => LOG4 => log4 => traits() => gas(log4);
    0xe6 => DUPN => dupn => traits() => gas(verylow);
    0xe7 => SWAPN => swapn => traits() => gas(verylow);
    0xe8 => EXCHANGE => exchange => traits() => gas(verylow);
    0xf0 => CREATE => create => traits(WRITES_STORAGE | OBSERVES_GAS | SETS_RETURNDATA) => gas(create) => result_bits(160);
    0xf1 => CALL => call => traits(WRITES_MEMORY | WRITES_STORAGE | OBSERVES_GAS | SETS_RETURNDATA) => gas(call) => result_bits(1) => input_bits(256, 160);
    0xf2 => CALLCODE => callcode => traits(WRITES_MEMORY | WRITES_STORAGE | OBSERVES_GAS | SETS_RETURNDATA) => gas(call) => result_bits(1) => input_bits(256, 160);
    0xf3 => RETURN => r#return => traits(TERMINAL) => gas(zero);
    0xf4 => DELEGATECALL => delegatecall => traits(WRITES_MEMORY | WRITES_STORAGE | OBSERVES_GAS | SETS_RETURNDATA) => gas(call) => result_bits(1) => input_bits(256, 160);
    0xf5 => CREATE2 => create2 => traits(WRITES_STORAGE | OBSERVES_GAS | SETS_RETURNDATA) => gas(create) => result_bits(160);
    0xfa => STATICCALL => staticcall => traits(WRITES_MEMORY | WRITES_STORAGE | OBSERVES_GAS | SETS_RETURNDATA) => gas(call) => result_bits(1) => input_bits(256, 160);
    0xfd => REVERT => revert => traits(TERMINAL) => gas(zero);
    0xfe => INVALID => invalid => traits(TERMINAL) => gas(zero);
    0xff => SELFDESTRUCT => selfdestruct => traits(TERMINAL) => gas(selfdestruct) => input_bits(160);
}

/// Returns the `evm2` spec that `evm_version` selects.
#[must_use]
pub fn spec_id(evm_version: EvmVersion) -> SpecId {
    match evm_version {
        EvmVersion::Homestead => SpecId::HOMESTEAD,
        EvmVersion::TangerineWhistle => SpecId::TANGERINE,
        EvmVersion::SpuriousDragon => SpecId::SPURIOUS_DRAGON,
        EvmVersion::Byzantium => SpecId::BYZANTIUM,
        EvmVersion::Constantinople | EvmVersion::Petersburg => SpecId::PETERSBURG,
        EvmVersion::Istanbul => SpecId::ISTANBUL,
        EvmVersion::Berlin => SpecId::BERLIN,
        EvmVersion::London => SpecId::LONDON,
        EvmVersion::Paris => SpecId::MERGE,
        EvmVersion::Shanghai => SpecId::SHANGHAI,
        EvmVersion::Cancun => SpecId::CANCUN,
        EvmVersion::Prague => SpecId::PRAGUE,
        EvmVersion::Osaka => SpecId::OSAKA,
        // `EvmVersion` is `#[non_exhaustive]`: a version without a spec is the latest.
        _ => SpecId::AMSTERDAM,
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
        // `INVALID` is the designated invalid instruction, which every version executes.
        self.opcode == INVALID
            || OpCode::new_or_unknown(self.opcode).is_enabled(spec_id(evm_version))
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
            && !matches!(self.opcode, JUMPI | JUMPDEST | PC)
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

macro_rules! define_stack_op_schema {
    (
        $(
            $pattern:pat
            => $mnemonic:literal
            => $opcode:ident
            => $gas:ident;
        )+
    ) => {
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

        impl StackOp {
            /// Returns the generated definition for this logical operation.
            #[must_use]
            pub const fn definition(self) -> StackOpDef {
                match self {
                    $(
                        $pattern => StackOpDef {
                            mnemonic: $mnemonic,
                            ir_opcode: $opcode,
                            gas: opcode_gas!($gas),
                        },
                    )+
                }
            }
        }
    };
}

define_stack_op_schema! {
    Self::Dup(_) => "dup" => DUPN => verylow;
    Self::Swap(_) => "swap" => SWAPN => verylow;
    Self::Exchange(_, _) => "exchange" => EXCHANGE => verylow;
    Self::Pop => "pop" => POP => base;
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
                StackOpLowering::Direct(DUPN, Some(encode_single(n)?))
            }
            Self::Swap(n) if evm_version.has_extended_stack_ops() => {
                StackOpLowering::Direct(SWAPN, Some(encode_single(n)?))
            }
            Self::Exchange(n, m) if evm_version.has_extended_stack_ops() => {
                StackOpLowering::Direct(EXCHANGE, Some(encode_pair(n, m)?))
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
    definition(opcode).is_some_and(|definition| definition.is_available(evm_version))
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
    if is_commutative(opcode) {
        return Some(opcode);
    }
    Some(match opcode {
        LT => GT,
        GT => LT,
        SLT => SGT,
        SGT => SLT,
        _ => return None,
    })
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
                && !matches!(op, PUSH1..=PUSH32)
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
    is_pure(op) || matches!(op, CALLDATALOAD | CALLDATASIZE)
}

/// Returns whether an opcode is a memory or storage load or store, or `KECCAK256`: an access
/// whose only effect is on the words it names.
#[must_use]
pub const fn is_plain_access(op: u8) -> bool {
    matches!(op, MLOAD | MSTORE | MSTORE8 | SLOAD | SSTORE | TLOAD | TSTORE | KECCAK256)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write;

    #[test]
    fn opcode_table() {
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
            assert!(
                def.mnemonic.eq_ignore_ascii_case(
                    OpCode::new_or_unknown(opcode).name_in(SpecId::AMSTERDAM)
                ),
                "{} differs from the evm2 name",
                def.mnemonic
            );
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
