//! Solidity sources of the modules the compiler provides under the reserved `solar:core/`
//! import prefix.
//!
//! The sources are plain files under `crates/std/solidity/`, at their import paths below the
//! prefix, so a project that also builds with another compiler can remap the prefix to a copy
//! of that directory. `gen_modules.py` generates the modules whose sources say so. The compiler
//! embeds the text here and never reads a file for a module.

/// The import prefix reserved for compiler-owned modules.
pub const PREFIX: &str = "solar:core/";

/// One compiler-owned module.
#[derive(Debug)]
pub struct Module {
    /// The import path, which is also the module's source file name.
    pub path: &'static str,
    /// The module's Solidity source.
    pub source: &'static str,
}

/// A module at `path` below [`PREFIX`], with its source from `solidity/path`.
macro_rules! module {
    ($path:literal) => {
        Module {
            path: concat!("solar:core/", $path),
            source: include_str!(concat!("../solidity/", $path)),
        }
    };
}

/// Every compiler-owned module.
pub const MODULES: &[Module] = &[
    module!("Bytes.sol"),
    module!("Arrays.sol"),
    module!("WordArrays.sol"),
    module!("Revert.sol"),
    module!("Hash.sol"),
    module!("Create.sol"),
    module!("Code.sol"),
    module!("Calls.sol"),
    module!("Bits.sol"),
    module!("Math.sol"),
    module!("Cast.sol"),
    module!("Precompiles.sol"),
    module!("Buffers.sol"),
    module!("CalldataBytes.sol"),
    module!("Strings.sol"),
    module!("Abi.sol"),
    module!("codecs/Base64.sol"),
    module!("codecs/Hex.sol"),
    module!("Slots.sol"),
    module!("Return.sol"),
    module!("Build.sol"),
];
