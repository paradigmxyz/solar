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
use solar_interface::{Symbol, source_map::FileName};
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
    match path {
        "solar:core/v1/Bytes.sol" => Some(BYTES.get_or_init(|| {
            // The names are built here, so the whole family shares one
            // definition instead of sixty-eight symbols.
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
            }
            table.insert(Symbol::intern("readUint256BE"), CoreIntrinsic::ReadUint256Be);
            table.insert(Symbol::intern("writeUint256BE"), CoreIntrinsic::WriteUint256Be);
            table.insert(Symbol::intern("copyInto"), CoreIntrinsic::CopyInto);
            table.insert(Symbol::intern("fill"), CoreIntrinsic::Fill);
            table
        })),
        "solar:core/v1/Arrays.sol" => Some(ARRAYS.get_or_init(|| {
            // Every overload shares the name; the lowering reads the array
            // kind off the declared parameter type.
            FxHashMap::from_iter([(Symbol::intern("truncate"), CoreIntrinsic::Truncate)])
        })),
        _ => None,
    }
}
