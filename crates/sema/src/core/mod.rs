//! Compiler-owned modules under the reserved `solar:core/` import prefix.
//!
//! A source importing `solar:core/v1/Bytes.sol` gets the text embedded here,
//! never a file: the prefix is intercepted before file resolution, so neither
//! a remapping nor a file on disk can stand in for a module, and a source unit
//! supplied under one of these names is set aside in favour of the module.
//! That is what gives a module's declarations an identity the compiler can
//! trust.
//!
//! Every function in a module carries a body that any Solidity compiler
//! accepts and that defines the operation's behaviour, so the same source
//! serves any other compiler and differs only in gas. This compiler lowers the
//! entry points listed in [`CoreIntrinsic`] directly by module identity; the
//! body is what runs under `-Zno-core-intrinsics`, which is how the two are
//! compared.

use crate::hir;
use solar_data_structures::map::FxHashMap;
use solar_interface::{Symbol, source_map::FileName, sym};
use std::sync::OnceLock;

/// The import prefix reserved for compiler-owned modules.
pub const PREFIX: &str = "solar:core/";

/// One compiler-owned module.
#[derive(Debug)]
pub struct CoreModule {
    /// The import path, which is also the module's source file name.
    pub path: &'static str,
    /// The module's Solidity source.
    pub source: &'static str,
}

/// Every compiler-owned module.
pub const MODULES: &[CoreModule] = &[
    CoreModule { path: "solar:core/v1/Bytes.sol", source: include_str!("v1/Bytes.sol") },
    CoreModule { path: "solar:core/v1/Arrays.sol", source: include_str!("v1/Arrays.sol") },
    CoreModule { path: "solar:core/v1/WordArrays.sol", source: include_str!("v1/WordArrays.sol") },
    CoreModule { path: "solar:core/v1/Revert.sol", source: include_str!("v1/Revert.sol") },
    CoreModule { path: "solar:core/v1/Hash.sol", source: include_str!("v1/Hash.sol") },
    CoreModule { path: "solar:core/v1/Create.sol", source: include_str!("v1/Create.sol") },
    CoreModule { path: "solar:core/v1/Code.sol", source: include_str!("v1/Code.sol") },
    CoreModule { path: "solar:core/v1/Calls.sol", source: include_str!("v1/Calls.sol") },
    CoreModule { path: "solar:core/v1/Bits.sol", source: include_str!("v1/Bits.sol") },
    CoreModule { path: "solar:core/v1/Math.sol", source: include_str!("v1/Math.sol") },
    CoreModule { path: "solar:core/v1/Cast.sol", source: include_str!("v1/Cast.sol") },
    CoreModule {
        path: "solar:core/v1/Precompiles.sol",
        source: include_str!("v1/Precompiles.sol"),
    },
    CoreModule { path: "solar:core/v1/Buffers.sol", source: include_str!("v1/Buffers.sol") },
    CoreModule {
        path: "solar:core/v1/CalldataBytes.sol",
        source: include_str!("v1/CalldataBytes.sol"),
    },
    CoreModule { path: "solar:core/v1/Strings.sol", source: include_str!("v1/Strings.sol") },
    CoreModule { path: "solar:core/v1/Abi.sol", source: include_str!("v1/Abi.sol") },
    CoreModule {
        path: "solar:core/v1/codecs/Base64.sol",
        source: include_str!("v1/codecs/Base64.sol"),
    },
    CoreModule { path: "solar:core/v1/codecs/Hex.sol", source: include_str!("v1/codecs/Hex.sol") },
];

/// Whether `path` lies under the reserved prefix.
pub fn is_reserved_path(path: &str) -> bool {
    path.starts_with(PREFIX)
}

/// Returns the module imported as `path`, if there is one.
pub fn lookup(path: &str) -> Option<&'static CoreModule> {
    MODULES.iter().find(|module| module.path == path)
}

/// Whether `name` is a compiler-owned module's source file.
pub fn is_core_file(name: &FileName) -> bool {
    matches!(name, FileName::Custom(path) if is_reserved_path(path))
}

/// The file name a compiler-owned module is registered under.
pub fn file_name(module: &CoreModule) -> FileName {
    FileName::Custom(module.path.to_string())
}

/// An operation the compiler lowers directly instead of calling its body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoreIntrinsic {
    /// Base64 encoding with standard/URL alphabets and optional padding.
    Base64Encode,
    /// Validated Base64 decoding; the optional flag also accepts IMAP.
    Base64Decode,
    /// `Bytes.readBytesN(b, offset)`: the `N` bytes at `offset`, left-aligned.
    ReadBytes(u8),
    /// `Bytes.readUint256BE(b, offset)`: the word at `offset`.
    ReadUint256Be,
    /// `Bytes.writeBytesN(b, offset, value)`: the leading `N` bytes of `value`.
    WriteBytes(u8),
    /// `Bytes.writeUint256BE(b, offset, value)`: the word at `offset`.
    WriteUint256Be,
    /// `Bytes.copyInto(dst, dstOffset, src, srcOffset, count)`, a move.
    CopyInto,
    /// `Bytes.fill(dst, offset, count, value)`.
    Fill,
    /// `Arrays.truncate(a, n)` for every supported array type.
    Truncate,
    /// `WordArrays.hasDuplicate(a)` for one-word dynamic arrays.
    ArrayHasDuplicate,
    /// `Revert.raw(data)`: revert with exactly `data`.
    RevertRaw,
    /// `Hash.keccak256Range(b, offset, count)`: hash a range where it lies.
    Keccak256Range,
    /// `Create.deploy(initcode, value)`: create, reverting on failure.
    Deploy,
    /// `Create.deploy2(initcode, salt, value)`: create2, reverting on failure.
    Deploy2,
    /// `Code.copyInto(dst, dstOffset, target, start, count)`: a checked
    /// `extcodecopy`.
    CodeCopyInto,
    /// `Bits.leadingZeros(x)`: `clz`, on targets that have it.
    LeadingZeros,
    /// `Bits.highestSetBit(x)`: `255 - clz`, with 256 for zero.
    HighestSetBit,
    /// `Bits.trailingZeros(x)`: the highest set bit of the isolated lowest one.
    TrailingZeros,
    /// `Calls.callInto(target, value, gasLimit, payload, output)`.
    CallInto,
    /// `Calls.staticCallInto(target, gasLimit, payload, output)`.
    StaticCallInto,
    /// `Calls.delegateCallInto(target, gasLimit, payload, output)`.
    DelegateCallInto,
    /// `Bytes.tryReadBytesN(b, offset)`: a read that answers instead of
    /// reverting. The payload is `N`.
    TryReadBytes(u8),
    /// `Bytes.tryReadUint256BE(b, offset)`.
    TryReadUint256Be,
    /// `CalldataBytes.readBytesN(b, offset)`: a checked load of `N` bytes
    /// from a calldata slice. The payload is `N`.
    CalldataReadBytes(u8),
    /// `CalldataBytes.readUint256BE(b, offset)`.
    CalldataReadUint256Be,
    /// `CalldataBytes.copyInto(dst, dstOffset, src, srcOffset, count)`.
    CalldataCopyInto,
    /// `CalldataBytes.tryReadBytesN(b, offset)`. The payload is `N`.
    CalldataTryReadBytes(u8),
    /// `CalldataBytes.tryReadUint256BE(b, offset)`.
    CalldataTryReadUint256Be,
    /// `Create.tryDeploy(initcode, value)`: create, reporting failure.
    TryDeploy,
    /// `Create.tryDeploy2(initcode, salt, value)`: create2, reporting failure.
    TryDeploy2,
    /// `Create.tryDeployInto(initcode, value, diagnostics)`: create, with a
    /// failed constructor's revert data bounded into the caller's buffer.
    TryDeployInto,
    /// `Math.mul512(x, y)`: both words of the product.
    Mul512,
    /// `Math.wrappingAdd(x, y)`.
    WrappingAdd,
    /// `Math.wrappingSub(x, y)`.
    WrappingSub,
    /// `Math.wrappingMul(x, y)`.
    WrappingMul,
}

/// Returns the intrinsic `function` names, if it is one.
///
/// Identity is the pair of the compiler-owned module the function is declared
/// in and its name; a function of the same name in any other file is an
/// ordinary function.
pub fn intrinsic_of(gcx: crate::ty::Gcx<'_>, function: hir::FunctionId) -> Option<CoreIntrinsic> {
    let f = gcx.hir.function(function);
    let FileName::Custom(path) = &gcx.hir.source(f.source).file.name else { return None };
    if !is_reserved_path(path) {
        return None;
    }
    let name = gcx.item_name(function).name;
    intrinsics_of_module(path)?.get(&name).copied()
}

/// The intrinsic table of the module at `path`.
fn intrinsics_of_module(path: &str) -> Option<&'static FxHashMap<Symbol, CoreIntrinsic>> {
    static BYTES: OnceLock<FxHashMap<Symbol, CoreIntrinsic>> = OnceLock::new();
    static ARRAYS: OnceLock<FxHashMap<Symbol, CoreIntrinsic>> = OnceLock::new();
    static WORD_ARRAYS: OnceLock<FxHashMap<Symbol, CoreIntrinsic>> = OnceLock::new();
    static REVERT: OnceLock<FxHashMap<Symbol, CoreIntrinsic>> = OnceLock::new();
    static HASH: OnceLock<FxHashMap<Symbol, CoreIntrinsic>> = OnceLock::new();
    static CREATE: OnceLock<FxHashMap<Symbol, CoreIntrinsic>> = OnceLock::new();
    static CODE: OnceLock<FxHashMap<Symbol, CoreIntrinsic>> = OnceLock::new();
    static BITS: OnceLock<FxHashMap<Symbol, CoreIntrinsic>> = OnceLock::new();
    static CALLS: OnceLock<FxHashMap<Symbol, CoreIntrinsic>> = OnceLock::new();
    static CALLDATA_BYTES: OnceLock<FxHashMap<Symbol, CoreIntrinsic>> = OnceLock::new();
    static MATH: OnceLock<FxHashMap<Symbol, CoreIntrinsic>> = OnceLock::new();
    static BASE64: OnceLock<FxHashMap<Symbol, CoreIntrinsic>> = OnceLock::new();
    match path {
        "solar:core/v1/codecs/Base64.sol" => Some(BASE64.get_or_init(|| {
            FxHashMap::from_iter([
                (sym::encode, CoreIntrinsic::Base64Encode),
                (sym::decode, CoreIntrinsic::Base64Decode),
            ])
        })),
        "solar:core/v1/Bytes.sol" => Some(BYTES.get_or_init(|| {
            // The names are built here, so each family shares one
            // definition instead of a symbol per width.
            let mut table = FxHashMap::default();
            for width in 1..=32u8 {
                table.insert(
                    Symbol::intern(&format!("readBytes{width}")),
                    CoreIntrinsic::ReadBytes(width),
                );
                table.insert(
                    Symbol::intern(&format!("writeBytes{width}")),
                    CoreIntrinsic::WriteBytes(width),
                );
                table.insert(
                    Symbol::intern(&format!("tryReadBytes{width}")),
                    CoreIntrinsic::TryReadBytes(width),
                );
            }
            table.insert(sym::readUint256BE, CoreIntrinsic::ReadUint256Be);
            table.insert(sym::writeUint256BE, CoreIntrinsic::WriteUint256Be);
            table.insert(sym::tryReadUint256BE, CoreIntrinsic::TryReadUint256Be);
            table.insert(sym::copyInto, CoreIntrinsic::CopyInto);
            table.insert(sym::fill, CoreIntrinsic::Fill);
            table
        })),
        "solar:core/v1/Arrays.sol" => Some(ARRAYS.get_or_init(|| {
            // Every overload shares the name; the lowering reads the array
            // kind off the declared parameter type.
            FxHashMap::from_iter([(sym::truncate, CoreIntrinsic::Truncate)])
        })),
        "solar:core/v1/WordArrays.sol" => Some(WORD_ARRAYS.get_or_init(|| {
            FxHashMap::from_iter([(
                Symbol::intern("hasDuplicate"),
                CoreIntrinsic::ArrayHasDuplicate,
            )])
        })),
        "solar:core/v1/Revert.sol" => Some(
            REVERT.get_or_init(|| FxHashMap::from_iter([(sym::raw, CoreIntrinsic::RevertRaw)])),
        ),
        "solar:core/v1/Hash.sol" => Some(HASH.get_or_init(|| {
            FxHashMap::from_iter([(sym::keccak256Range, CoreIntrinsic::Keccak256Range)])
        })),
        "solar:core/v1/Create.sol" => Some(CREATE.get_or_init(|| {
            // `predict2` is arithmetic and stays a call to its body.
            FxHashMap::from_iter([
                (sym::deploy, CoreIntrinsic::Deploy),
                (sym::deploy2, CoreIntrinsic::Deploy2),
                (sym::tryDeploy, CoreIntrinsic::TryDeploy),
                (sym::tryDeploy2, CoreIntrinsic::TryDeploy2),
                (sym::tryDeployInto, CoreIntrinsic::TryDeployInto),
            ])
        })),
        "solar:core/v1/Code.sol" => Some(CODE.get_or_init(|| {
            // `read` is library code over `copyInto`.
            FxHashMap::from_iter([(sym::copyInto, CoreIntrinsic::CodeCopyInto)])
        })),
        "solar:core/v1/Bits.sol" => Some(BITS.get_or_init(|| {
            // `popCount` has no instruction to lower to.
            FxHashMap::from_iter([
                (sym::leadingZeros, CoreIntrinsic::LeadingZeros),
                (sym::highestSetBit, CoreIntrinsic::HighestSetBit),
                (sym::trailingZeros, CoreIntrinsic::TrailingZeros),
            ])
        })),
        "solar:core/v1/Calls.sol" => Some(CALLS.get_or_init(|| {
            FxHashMap::from_iter([
                (sym::callInto, CoreIntrinsic::CallInto),
                (sym::staticCallInto, CoreIntrinsic::StaticCallInto),
                (sym::delegateCallInto, CoreIntrinsic::DelegateCallInto),
            ])
        })),
        "solar:core/v1/CalldataBytes.sol" => Some(CALLDATA_BYTES.get_or_init(|| {
            let mut table = FxHashMap::default();
            for width in 1..=32u8 {
                table.insert(
                    Symbol::intern(&format!("readBytes{width}")),
                    CoreIntrinsic::CalldataReadBytes(width),
                );
                table.insert(
                    Symbol::intern(&format!("tryReadBytes{width}")),
                    CoreIntrinsic::CalldataTryReadBytes(width),
                );
            }
            table.insert(sym::readUint256BE, CoreIntrinsic::CalldataReadUint256Be);
            table.insert(sym::tryReadUint256BE, CoreIntrinsic::CalldataTryReadUint256Be);
            table.insert(sym::copyInto, CoreIntrinsic::CalldataCopyInto);
            table
        })),
        "solar:core/v1/Math.sol" => Some(MATH.get_or_init(|| {
            // `mulDiv` is a long division and stays a call to its body.
            FxHashMap::from_iter([
                (sym::mul512, CoreIntrinsic::Mul512),
                (sym::wrappingAdd, CoreIntrinsic::WrappingAdd),
                (sym::wrappingSub, CoreIntrinsic::WrappingSub),
                (sym::wrappingMul, CoreIntrinsic::WrappingMul),
            ])
        })),
        // `Cast`, `Precompiles`, `Buffers`, `Strings` and the codecs are library
        // code throughout.
        _ => None,
    }
}
