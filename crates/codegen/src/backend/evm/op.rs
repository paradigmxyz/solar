//! EVM opcode definitions and metadata.

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

/// Returns whether an opcode halts or unconditionally transfers control.
#[must_use]
pub(crate) const fn is_terminal(op: u8) -> bool {
    matches!(op, STOP | JUMP | RETURN | REVERT | INVALID | SELFDESTRUCT)
}
