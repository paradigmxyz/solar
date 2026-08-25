//! EVM opcode definitions and metadata.

use solar_config::EvmVersion;
use solar_interface::Symbol;

const UNKNOWN_PREFIX: &str = "op_";

macro_rules! opcode_mnemonic {
    (r#return) => {
        "return"
    };
    ($mnemonic:ident) => {
        stringify!($mnemonic)
    };
}

macro_rules! opcode_stack_io {
    (_,_) => {
        None
    };
    ($inputs:literal, $outputs:literal) => {
        Some(($inputs, $outputs))
    };
}

macro_rules! opcodes {
    ($($opcode:literal => $constant:ident => $mnemonic:ident => stack_io($inputs:tt, $outputs:tt);)*) => {
        $(
            #[doc = concat!("Opcode byte for `", stringify!($constant), "`.")]
            #[allow(dead_code)]
            pub(crate) const $constant: u8 = $opcode;
        )*

        /// Maps each opcode byte to its canonical mnemonic.
        static OPCODE_MNEMONICS: [Option<&str>; 256] = {
            let mut map = [None; 256];
            let mut prev = 0;
            $(
                let opcode: u8 = $opcode;
                assert!(opcode == 0 || opcode > prev, "opcodes must be sorted in ascending order");
                prev = opcode;
                map[opcode as usize] = Some(opcode_mnemonic!($mnemonic));
            )*
            let _ = prev;
            map
        };

        /// Returns the canonical mnemonic for an opcode.
        #[must_use]
        pub(crate) const fn mnemonic(opcode: u8) -> Option<&'static str> {
            OPCODE_MNEMONICS[opcode as usize]
        }

        /// Returns the opcode for a canonical mnemonic.
        #[must_use]
        pub(crate) fn from_mnemonic(mnemonic: &str) -> Option<u8> {
            match mnemonic {
                $(opcode_mnemonic!($mnemonic) => Some($opcode),)*
                _ => None,
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
        pub(crate) const fn stack_io(opcode: u8) -> Option<(u8, u8)> {
            match opcode {
                $($opcode => opcode_stack_io!($inputs, $outputs),)*
                _ => None,
            }
        }
    };
}

opcodes! {
    0x00 => STOP => stop => stack_io(0, 0);
    0x01 => ADD => add => stack_io(2, 1);
    0x02 => MUL => mul => stack_io(2, 1);
    0x03 => SUB => sub => stack_io(2, 1);
    0x04 => DIV => div => stack_io(2, 1);
    0x05 => SDIV => sdiv => stack_io(2, 1);
    0x06 => MOD => mod => stack_io(2, 1);
    0x07 => SMOD => smod => stack_io(2, 1);
    0x08 => ADDMOD => addmod => stack_io(3, 1);
    0x09 => MULMOD => mulmod => stack_io(3, 1);
    0x0a => EXP => exp => stack_io(2, 1);
    0x0b => SIGNEXTEND => signextend => stack_io(2, 1);
    0x10 => LT => lt => stack_io(2, 1);
    0x11 => GT => gt => stack_io(2, 1);
    0x12 => SLT => slt => stack_io(2, 1);
    0x13 => SGT => sgt => stack_io(2, 1);
    0x14 => EQ => eq => stack_io(2, 1);
    0x15 => ISZERO => iszero => stack_io(1, 1);
    0x16 => AND => and => stack_io(2, 1);
    0x17 => OR => or => stack_io(2, 1);
    0x18 => XOR => xor => stack_io(2, 1);
    0x19 => NOT => not => stack_io(1, 1);
    0x1a => BYTE => byte => stack_io(2, 1);
    0x1b => SHL => shl => stack_io(2, 1);
    0x1c => SHR => shr => stack_io(2, 1);
    0x1d => SAR => sar => stack_io(2, 1);
    0x1e => CLZ => clz => stack_io(1, 1);
    0x20 => KECCAK256 => keccak256 => stack_io(2, 1);
    0x30 => ADDRESS => address => stack_io(0, 1);
    0x31 => BALANCE => balance => stack_io(1, 1);
    0x32 => ORIGIN => origin => stack_io(0, 1);
    0x33 => CALLER => caller => stack_io(0, 1);
    0x34 => CALLVALUE => callvalue => stack_io(0, 1);
    0x35 => CALLDATALOAD => calldataload => stack_io(1, 1);
    0x36 => CALLDATASIZE => calldatasize => stack_io(0, 1);
    0x37 => CALLDATACOPY => calldatacopy => stack_io(3, 0);
    0x38 => CODESIZE => codesize => stack_io(0, 1);
    0x39 => CODECOPY => codecopy => stack_io(3, 0);
    0x3a => GASPRICE => gasprice => stack_io(0, 1);
    0x3b => EXTCODESIZE => extcodesize => stack_io(1, 1);
    0x3c => EXTCODECOPY => extcodecopy => stack_io(4, 0);
    0x3d => RETURNDATASIZE => returndatasize => stack_io(0, 1);
    0x3e => RETURNDATACOPY => returndatacopy => stack_io(3, 0);
    0x3f => EXTCODEHASH => extcodehash => stack_io(1, 1);
    0x40 => BLOCKHASH => blockhash => stack_io(1, 1);
    0x41 => COINBASE => coinbase => stack_io(0, 1);
    0x42 => TIMESTAMP => timestamp => stack_io(0, 1);
    0x43 => NUMBER => number => stack_io(0, 1);
    0x44 => PREVRANDAO => prevrandao => stack_io(0, 1);
    0x45 => GASLIMIT => gaslimit => stack_io(0, 1);
    0x46 => CHAINID => chainid => stack_io(0, 1);
    0x47 => SELFBALANCE => selfbalance => stack_io(0, 1);
    0x48 => BASEFEE => basefee => stack_io(0, 1);
    0x49 => BLOBHASH => blobhash => stack_io(1, 1);
    0x4a => BLOBBASEFEE => blobbasefee => stack_io(0, 1);
    0x4b => SLOTNUM => slotnum => stack_io(0, 1);
    0x50 => POP => pop => stack_io(1, 0);
    0x51 => MLOAD => mload => stack_io(1, 1);
    0x52 => MSTORE => mstore => stack_io(2, 0);
    0x53 => MSTORE8 => mstore8 => stack_io(2, 0);
    0x54 => SLOAD => sload => stack_io(1, 1);
    0x55 => SSTORE => sstore => stack_io(2, 0);
    0x56 => JUMP => jump => stack_io(1, 0);
    0x57 => JUMPI => jumpi => stack_io(2, 0);
    0x58 => PC => pc => stack_io(0, 1);
    0x59 => MSIZE => msize => stack_io(0, 1);
    0x5a => GAS => gas => stack_io(0, 1);
    0x5b => JUMPDEST => jumpdest => stack_io(0, 0);
    0x5c => TLOAD => tload => stack_io(1, 1);
    0x5d => TSTORE => tstore => stack_io(2, 0);
    0x5e => MCOPY => mcopy => stack_io(3, 0);
    0x5f => PUSH0 => push0 => stack_io(0, 1);
    0x60 => PUSH1 => push1 => stack_io(0, 1);
    0x61 => PUSH2 => push2 => stack_io(0, 1);
    0x62 => PUSH3 => push3 => stack_io(0, 1);
    0x63 => PUSH4 => push4 => stack_io(0, 1);
    0x64 => PUSH5 => push5 => stack_io(0, 1);
    0x65 => PUSH6 => push6 => stack_io(0, 1);
    0x66 => PUSH7 => push7 => stack_io(0, 1);
    0x67 => PUSH8 => push8 => stack_io(0, 1);
    0x68 => PUSH9 => push9 => stack_io(0, 1);
    0x69 => PUSH10 => push10 => stack_io(0, 1);
    0x6a => PUSH11 => push11 => stack_io(0, 1);
    0x6b => PUSH12 => push12 => stack_io(0, 1);
    0x6c => PUSH13 => push13 => stack_io(0, 1);
    0x6d => PUSH14 => push14 => stack_io(0, 1);
    0x6e => PUSH15 => push15 => stack_io(0, 1);
    0x6f => PUSH16 => push16 => stack_io(0, 1);
    0x70 => PUSH17 => push17 => stack_io(0, 1);
    0x71 => PUSH18 => push18 => stack_io(0, 1);
    0x72 => PUSH19 => push19 => stack_io(0, 1);
    0x73 => PUSH20 => push20 => stack_io(0, 1);
    0x74 => PUSH21 => push21 => stack_io(0, 1);
    0x75 => PUSH22 => push22 => stack_io(0, 1);
    0x76 => PUSH23 => push23 => stack_io(0, 1);
    0x77 => PUSH24 => push24 => stack_io(0, 1);
    0x78 => PUSH25 => push25 => stack_io(0, 1);
    0x79 => PUSH26 => push26 => stack_io(0, 1);
    0x7a => PUSH27 => push27 => stack_io(0, 1);
    0x7b => PUSH28 => push28 => stack_io(0, 1);
    0x7c => PUSH29 => push29 => stack_io(0, 1);
    0x7d => PUSH30 => push30 => stack_io(0, 1);
    0x7e => PUSH31 => push31 => stack_io(0, 1);
    0x7f => PUSH32 => push32 => stack_io(0, 1);
    0x80 => DUP1 => dup1 => stack_io(1, 2);
    0x81 => DUP2 => dup2 => stack_io(2, 3);
    0x82 => DUP3 => dup3 => stack_io(3, 4);
    0x83 => DUP4 => dup4 => stack_io(4, 5);
    0x84 => DUP5 => dup5 => stack_io(5, 6);
    0x85 => DUP6 => dup6 => stack_io(6, 7);
    0x86 => DUP7 => dup7 => stack_io(7, 8);
    0x87 => DUP8 => dup8 => stack_io(8, 9);
    0x88 => DUP9 => dup9 => stack_io(9, 10);
    0x89 => DUP10 => dup10 => stack_io(10, 11);
    0x8a => DUP11 => dup11 => stack_io(11, 12);
    0x8b => DUP12 => dup12 => stack_io(12, 13);
    0x8c => DUP13 => dup13 => stack_io(13, 14);
    0x8d => DUP14 => dup14 => stack_io(14, 15);
    0x8e => DUP15 => dup15 => stack_io(15, 16);
    0x8f => DUP16 => dup16 => stack_io(16, 17);
    0x90 => SWAP1 => swap1 => stack_io(2, 2);
    0x91 => SWAP2 => swap2 => stack_io(3, 3);
    0x92 => SWAP3 => swap3 => stack_io(4, 4);
    0x93 => SWAP4 => swap4 => stack_io(5, 5);
    0x94 => SWAP5 => swap5 => stack_io(6, 6);
    0x95 => SWAP6 => swap6 => stack_io(7, 7);
    0x96 => SWAP7 => swap7 => stack_io(8, 8);
    0x97 => SWAP8 => swap8 => stack_io(9, 9);
    0x98 => SWAP9 => swap9 => stack_io(10, 10);
    0x99 => SWAP10 => swap10 => stack_io(11, 11);
    0x9a => SWAP11 => swap11 => stack_io(12, 12);
    0x9b => SWAP12 => swap12 => stack_io(13, 13);
    0x9c => SWAP13 => swap13 => stack_io(14, 14);
    0x9d => SWAP14 => swap14 => stack_io(15, 15);
    0x9e => SWAP15 => swap15 => stack_io(16, 16);
    0x9f => SWAP16 => swap16 => stack_io(17, 17);
    0xa0 => LOG0 => log0 => stack_io(2, 0);
    0xa1 => LOG1 => log1 => stack_io(3, 0);
    0xa2 => LOG2 => log2 => stack_io(4, 0);
    0xa3 => LOG3 => log3 => stack_io(5, 0);
    0xa4 => LOG4 => log4 => stack_io(6, 0);
    0xd0 => DATALOAD => dataload => stack_io(1, 1);
    0xd1 => DATALOADN => dataloadn => stack_io(0, 1);
    0xd2 => DATASIZE => datasize => stack_io(0, 1);
    0xd3 => DATACOPY => datacopy => stack_io(3, 0);
    0xe0 => RJUMP => rjump => stack_io(0, 0);
    0xe1 => RJUMPI => rjumpi => stack_io(1, 0);
    0xe2 => RJUMPV => rjumpv => stack_io(1, 0);
    0xe3 => CALLF => callf => stack_io(_, _);
    0xe4 => RETF => retf => stack_io(_, _);
    0xe5 => JUMPF => jumpf => stack_io(_, _);
    0xe6 => DUPN => dupn => stack_io(0, 1);
    0xe7 => SWAPN => swapn => stack_io(0, 0);
    0xe8 => EXCHANGE => exchange => stack_io(0, 0);
    0xec => EOFCREATE => eofcreate => stack_io(4, 1);
    0xee => RETURNCONTRACT => returncontract => stack_io(2, 0);
    0xf0 => CREATE => create => stack_io(3, 1);
    0xf1 => CALL => call => stack_io(7, 1);
    0xf2 => CALLCODE => callcode => stack_io(7, 1);
    0xf3 => RETURN => r#return => stack_io(2, 0);
    0xf4 => DELEGATECALL => delegatecall => stack_io(6, 1);
    0xf5 => CREATE2 => create2 => stack_io(4, 1);
    0xf7 => RETURNDATALOAD => returndataload => stack_io(1, 1);
    0xf8 => EXTCALL => extcall => stack_io(4, 1);
    0xf9 => EXTDELEGATECALL => extdelegatecall => stack_io(3, 1);
    0xfa => STATICCALL => staticcall => stack_io(6, 1);
    0xfb => EXTSTATICCALL => extstaticcall => stack_io(3, 1);
    0xfd => REVERT => revert => stack_io(2, 0);
    0xfe => INVALID => invalid => stack_io(0, 0);
    0xff => SELFDESTRUCT => selfdestruct => stack_io(1, 0);
}

/// Returns the PUSH opcode for the given width (1-32).
#[must_use]
pub(crate) const fn push(width: u8) -> u8 {
    debug_assert!(width >= 1 && width <= 32);
    PUSH1 + width - 1
}

/// Returns the DUP opcode for the given depth (1-16).
#[must_use]
pub(crate) const fn dup(n: u8) -> u8 {
    debug_assert!(n >= 1 && n <= 16);
    DUP1 + n - 1
}

/// Returns the SWAP opcode for the given depth (1-16).
#[must_use]
pub(crate) const fn swap(n: u8) -> u8 {
    debug_assert!(n >= 1 && n <= 16);
    SWAP1 + n - 1
}

/// A logical EVM stack operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum StackOp {
    /// Duplicate the nth stack element.
    Dup(u8),
    /// Swap the top with the nth stack element below it.
    Swap(u8),
    /// Swap two non-top stack elements.
    Exchange(u8, u8),
    /// Remove the top stack element.
    Pop,
}

/// Target-specific lowering of one logical stack operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StackOpLowering {
    /// One opcode with an optional immediate byte.
    Direct(u8, Option<u8>),
    /// Three legacy `SWAP` opcodes implementing a shallow `EXCHANGE`.
    LegacyExchange([u8; 3]),
}

/// Exact cost of a lowered stack operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StackOpMetrics {
    pub(crate) static_gas: usize,
    pub(crate) assembled_len: usize,
    pub(crate) instruction_count: usize,
}

impl StackOp {
    /// Decodes a legacy one-byte stack opcode.
    #[must_use]
    pub(crate) const fn from_legacy_opcode(opcode: u8) -> Option<Self> {
        match opcode {
            POP => Some(Self::Pop),
            DUP1..=DUP16 => Some(Self::Dup(opcode - DUP1 + 1)),
            SWAP1..=SWAP16 => Some(Self::Swap(opcode - SWAP1 + 1)),
            _ => None,
        }
    }

    /// Returns the one-byte encoding when this operation has one.
    #[must_use]
    pub(crate) const fn legacy_opcode(self) -> Option<u8> {
        match self {
            Self::Dup(n @ 1..=16) => Some(dup(n)),
            Self::Swap(n @ 1..=16) => Some(swap(n)),
            Self::Pop => Some(POP),
            _ => None,
        }
    }

    /// Returns the placeholder opcode used to represent this operation in EVM IR.
    #[must_use]
    pub(crate) const fn ir_opcode(self) -> u8 {
        match self {
            Self::Dup(_) => DUPN,
            Self::Swap(_) => SWAPN,
            Self::Exchange(_, _) => EXCHANGE,
            Self::Pop => POP,
        }
    }

    /// Returns whether the operands are valid independent of the target EVM version.
    #[must_use]
    pub(crate) const fn is_valid(self) -> bool {
        match self {
            Self::Dup(n) | Self::Swap(n) => n >= 1 && n <= 235,
            Self::Exchange(n, m) => n >= 1 && n < m && (n as u16) + (m as u16) <= 30,
            Self::Pop => true,
        }
    }

    /// Returns the target-specific lowering of this operation.
    #[must_use]
    pub(crate) fn lowering(self, evm_version: EvmVersion) -> Option<StackOpLowering> {
        if !self.is_valid() {
            return None;
        }
        if let Some(opcode) = self.legacy_opcode() {
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
                StackOpLowering::LegacyExchange([swap(n), swap(m), swap(n)])
            }
            _ => return None,
        };
        Some(lowering)
    }

    /// Returns the exact cost of this operation after target lowering.
    #[must_use]
    pub(crate) fn metrics(self, evm_version: EvmVersion) -> Option<StackOpMetrics> {
        let (assembled_len, instruction_count) = match self.lowering(evm_version)? {
            StackOpLowering::Direct(_, immediate) => (1 + usize::from(immediate.is_some()), 1),
            StackOpLowering::LegacyExchange(opcodes) => (opcodes.len(), opcodes.len()),
        };
        let gas_per_instruction = if matches!(self, Self::Pop) { 2 } else { 3 };
        Some(StackOpMetrics {
            static_gas: instruction_count * gas_per_instruction,
            assembled_len,
            instruction_count,
        })
    }

    /// Returns the exact assembled byte length when this operation can be lowered for the target.
    #[must_use]
    pub(crate) fn assembled_len(self, evm_version: EvmVersion) -> Option<usize> {
        self.metrics(evm_version).map(|metrics| metrics.assembled_len)
    }

    /// Returns the `EXCHANGE` represented by a three-swap sequence.
    #[must_use]
    pub(crate) const fn from_swaps(first: u8, second: u8, third: u8) -> Option<Self> {
        if first != third || first == second {
            return None;
        }
        let (n, m) = if first < second { (first, second) } else { (second, first) };
        let op = Self::Exchange(n, m);
        if op.is_valid() { Some(op) } else { None }
    }
}

/// Encodes the immediate used by `DUPN` and `SWAPN`.
#[must_use]
pub(crate) const fn encode_stack_depth(n: u8) -> u8 {
    debug_assert!(n >= 17 && n <= 235);
    n.wrapping_add(111)
}

/// Decodes a valid `DUPN` or `SWAPN` immediate.
#[must_use]
pub(crate) const fn decode_stack_depth(immediate: u8) -> Option<u8> {
    if immediate > 90 && immediate < 128 { None } else { Some(immediate.wrapping_add(145)) }
}

/// Encodes the immediate used by `EXCHANGE`.
#[must_use]
pub(crate) const fn encode_exchange(n: u8, m: u8) -> u8 {
    debug_assert!(n >= 1 && n < m && (n as u16) + (m as u16) <= 30);
    let (q, r) = if m <= 16 { (n - 1, m - 1) } else { (29 - m, n - 1) };
    (16 * q + r) ^ 143
}

/// Decodes a valid `EXCHANGE` immediate.
#[must_use]
pub(crate) const fn decode_exchange(immediate: u8) -> Option<(u8, u8)> {
    if immediate > 81 && immediate < 128 {
        return None;
    }
    let k = immediate ^ 143;
    let (q, r) = (k / 16, k % 16);
    Some(if q < r { (q + 1, r + 1) } else { (r + 1, 29 - q) })
}

/// Returns whether an opcode halts or unconditionally transfers control.
#[must_use]
pub(crate) const fn is_terminal(op: u8) -> bool {
    matches!(op, STOP | JUMP | RETURN | REVERT | INVALID | SELFDESTRUCT)
}

/// Returns whether an opcode is available in legacy bytecode for `evm_version`.
#[must_use]
pub(crate) fn is_available(opcode: u8, evm_version: EvmVersion) -> bool {
    match opcode {
        RETURNDATASIZE | RETURNDATACOPY | STATICCALL | REVERT => {
            evm_version >= EvmVersion::Byzantium
        }
        SHL | SHR | SAR | EXTCODEHASH | CREATE2 => evm_version >= EvmVersion::Constantinople,
        CHAINID | SELFBALANCE => evm_version >= EvmVersion::Istanbul,
        BASEFEE => evm_version >= EvmVersion::London,
        PUSH0 => evm_version >= EvmVersion::Shanghai,
        BLOBHASH | BLOBBASEFEE | TLOAD | TSTORE | MCOPY => evm_version >= EvmVersion::Cancun,
        CLZ => evm_version >= EvmVersion::Osaka,
        SLOTNUM => evm_version.has_slot_num(),
        DUPN | SWAPN | EXCHANGE => evm_version.has_extended_stack_ops(),
        DATALOAD | DATALOADN | DATASIZE | DATACOPY | RJUMP | RJUMPI | RJUMPV | CALLF | RETF
        | JUMPF | EOFCREATE | RETURNCONTRACT | RETURNDATALOAD | EXTCALL | EXTDELEGATECALL
        | EXTSTATICCALL => false,
        _ => mnemonic(opcode).is_some(),
    }
}

/// Returns whether an opcode's operands may be swapped without changing its result.
#[must_use]
pub(crate) const fn is_commutative(op: u8) -> bool {
    matches!(op, ADD | MUL | EQ | AND | OR | XOR)
}

/// Returns whether an opcode is pure: it has no side effects and its result is a deterministic
/// function of its stack operands alone (no memory, storage, or environment dependency), so two
/// occurrences with equal operands always produce the same value.
#[must_use]
pub(crate) const fn is_pure(op: u8) -> bool {
    matches!(
        op,
        ADD | MUL
            | SUB
            | DIV
            | SDIV
            | MOD
            | SMOD
            | ADDMOD
            | MULMOD
            | EXP
            | SIGNEXTEND
            | LT
            | GT
            | SLT
            | SGT
            | EQ
            | ISZERO
            | AND
            | OR
            | XOR
            | NOT
            | BYTE
            | SHL
            | SHR
            | SAR
            | CLZ
    )
}

/// Returns whether an opcode only reads its operands or EVM state.
///
/// Unlike [`is_pure`], this includes memory, storage, and environment reads whose result can vary
/// without the opcode changing any state. Position and gas observations are excluded.
#[must_use]
pub(crate) const fn is_read_only(op: u8) -> bool {
    is_pure(op)
        || matches!(
            op,
            KECCAK256
                | ADDRESS
                | BALANCE
                | ORIGIN
                | CALLER
                | CALLVALUE
                | CALLDATALOAD
                | CALLDATASIZE
                | CODESIZE
                | GASPRICE
                | EXTCODESIZE
                | RETURNDATASIZE
                | EXTCODEHASH
                | BLOCKHASH
                | COINBASE
                | TIMESTAMP
                | NUMBER
                | PREVRANDAO
                | GASLIMIT
                | CHAINID
                | SELFBALANCE
                | BASEFEE
                | BLOBHASH
                | BLOBBASEFEE
                | MLOAD
                | SLOAD
                | MSIZE
                | TLOAD
                | PUSH0
                | DATALOAD
                | DATALOADN
                | DATASIZE
                | RETURNDATALOAD
        )
}

/// Returns whether an opcode may write to memory, invalidating cached memory reads.
#[must_use]
pub(crate) const fn writes_memory(op: u8) -> bool {
    matches!(
        op,
        MSTORE
            | MSTORE8
            | MCOPY
            | CALLDATACOPY
            | CODECOPY
            | DATACOPY
            | EXTCODECOPY
            | RETURNDATACOPY
            | CALL
            | CALLCODE
            | DELEGATECALL
            | STATICCALL
            | CALLF
    )
}

/// Returns whether an opcode may write to storage or transient storage, invalidating cached
/// storage reads.
#[must_use]
pub(crate) const fn writes_storage(op: u8) -> bool {
    matches!(
        op,
        SSTORE
            | TSTORE
            | CALL
            | CALLCODE
            | DELEGATECALL
            | STATICCALL
            | CREATE
            | CREATE2
            | EOFCREATE
            | EXTCALL
            | EXTDELEGATECALL
            | CALLF
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eip_8024_immediates() {
        assert_eq!(encode_stack_depth(17), 0x80);
        assert_eq!(decode_stack_depth(0xdb), Some(108));
        assert_eq!(decode_stack_depth(0x5b), None);
        assert_eq!(encode_exchange(2, 3), 0x9d);
        assert_eq!(encode_exchange(1, 19), 0x2f);
        assert_eq!(decode_exchange(0x50), Some((14, 16)));
        assert_eq!(decode_exchange(0x52), None);
        assert_eq!(
            StackOp::Dup(16).lowering(EvmVersion::Osaka),
            Some(StackOpLowering::Direct(DUP16, None))
        );
        assert_eq!(StackOp::Dup(17).lowering(EvmVersion::Osaka), None);
        assert_eq!(
            StackOp::Swap(108).lowering(EvmVersion::Amsterdam),
            Some(StackOpLowering::Direct(SWAPN, Some(0xdb)))
        );
        assert_eq!(StackOp::Exchange(1, 16).assembled_len(EvmVersion::Osaka), Some(3));
        assert_eq!(StackOp::Exchange(1, 17).assembled_len(EvmVersion::Osaka), None);
        assert_eq!(StackOp::Exchange(1, 17).assembled_len(EvmVersion::Amsterdam), Some(2));
        assert_eq!(StackOp::from_swaps(2, 3, 2), Some(StackOp::Exchange(2, 3)));

        for depth in 17..=235 {
            assert_eq!(decode_stack_depth(encode_stack_depth(depth)), Some(depth));
        }
        for immediate in u8::MIN..=u8::MAX {
            if let Some(depth) = decode_stack_depth(immediate) {
                assert_eq!(encode_stack_depth(depth), immediate);
            }
            if let Some((n, m)) = decode_exchange(immediate) {
                assert_eq!(encode_exchange(n, m), immediate);
            }
        }
    }
}
