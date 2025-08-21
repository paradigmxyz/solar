#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/favicon.ico"
)]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

use std::{fmt, num::NonZeroUsize};
use strum::EnumIs;

#[macro_use]
mod macros;

mod opts;
pub use opts::{Opts, UnstableOpts};

mod utils;

#[cfg(feature = "version")]
pub mod version;

/// Whether the target is single-threaded.
///
/// We still allow passing `-j` greater than 1, but it should gracefully handle the error when
/// spawning the thread pool.
///
/// Modified from `libtest`: <https://github.com/rust-lang/rust/blob/96cfc75584359ae7ad11cc45968059f29e7b44b7/library/test/src/lib.rs#L605-L607>
pub const SINGLE_THREADED_TARGET: bool =
    cfg!(target_os = "emscripten") || cfg!(target_family = "wasm") || cfg!(target_os = "zkvm");

str_enum! {
    /// Compiler stage.
    #[derive(strum::EnumIs)]
    #[strum(serialize_all = "lowercase")]
    #[non_exhaustive]
    pub enum CompilerStage {
        /// Source code was parsed into an AST.
        #[strum(serialize = "parsed", serialize = "parsing")]
        Parsed,
        /// Source code was parsed, and all imports were recursively resolved and parsed.
        #[strum(serialize = "parsed-and-imported")]
        ParsedAndImported,
        /// ASTs were lowered to HIR.
        #[strum(serialize = "lowered", serialize = "lowering")]
        Lowered,
        /// HIR was analyzed.
        #[strum(serialize = "analyzed", serialize = "analysis")]
        Analyzed,
    }
}

str_enum! {
    /// Source code language.
    #[derive(Default)]
    #[derive(strum::EnumIs)]
    #[strum(serialize_all = "lowercase")]
    #[non_exhaustive]
    pub enum Language {
        #[default]
        Solidity,
        Yul,
    }
}

str_enum! {
    /// A version specifier of the EVM we want to compile to.
    ///
    /// Defaults to the latest version deployed on Ethereum Mainnet at the time of compiler release.
    #[derive(Default)]
    #[strum(serialize_all = "camelCase")]
    #[non_exhaustive]
    pub enum EvmVersion {
        // NOTE: Order matters.
        Homestead,
        TangerineWhistle,
        SpuriousDragon,
        Byzantium,
        Constantinople,
        Petersburg,
        Istanbul,
        Berlin,
        London,
        Paris,
        Shanghai,
        Cancun,
        #[default]
        Prague,
        Osaka,
    }
}

impl EvmVersion {
    pub fn supports_returndata(self) -> bool {
        self >= Self::Byzantium
    }
    pub fn has_static_call(self) -> bool {
        self >= Self::Byzantium
    }
    pub fn has_bitwise_shifting(self) -> bool {
        self >= Self::Constantinople
    }
    pub fn has_create2(self) -> bool {
        self >= Self::Constantinople
    }
    pub fn has_ext_code_hash(self) -> bool {
        self >= Self::Constantinople
    }
    pub fn has_chain_id(self) -> bool {
        self >= Self::Istanbul
    }
    pub fn has_self_balance(self) -> bool {
        self >= Self::Istanbul
    }
    pub fn has_base_fee(self) -> bool {
        self >= Self::London
    }
    pub fn has_blob_base_fee(self) -> bool {
        self >= Self::Cancun
    }
    pub fn has_prev_randao(self) -> bool {
        self >= Self::Paris
    }
    pub fn has_push0(self) -> bool {
        self >= Self::Shanghai
    }
}

str_enum! {
    /// Type of output for the compiler to emit.
    #[strum(serialize_all = "kebab-case")]
    #[non_exhaustive]
    pub enum CompilerOutput {
        /// JSON ABI.
        Abi,
        // /// Creation bytecode.
        // Bin,
        // /// Runtime bytecode.
        // BinRuntime,
        /// Function signature hashes.
        Hashes,
    }
}

/// `-Zdump=kind[=paths...]`.
#[derive(Clone, Debug)]
pub struct Dump {
    pub kind: DumpKind,
    pub paths: Option<Vec<String>>,
}

impl std::str::FromStr for Dump {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (kind, paths) = if let Some((kind, paths)) = s.split_once('=') {
            let paths = paths.split(',').map(ToString::to_string).collect();
            (kind, Some(paths))
        } else {
            (s, None)
        };
        Ok(Self { kind: kind.parse::<DumpKind>().map_err(|e| e.to_string())?, paths })
    }
}

str_enum! {
    /// What kind of output to dump. See [`Dump`].
    #[derive(EnumIs)]
    #[strum(serialize_all = "kebab-case")]
    #[non_exhaustive]
    pub enum DumpKind {
        /// Print the AST.
        Ast,
        /// Print the HIR.
        Hir,
    }
}

str_enum! {
    /// How errors and other messages are produced.
    #[derive(Default)]
    #[strum(serialize_all = "kebab-case")]
    #[non_exhaustive]
    pub enum ErrorFormat {
        /// Human-readable output.
        #[default]
        Human,
        /// Solc-like JSON output.
        Json,
        /// Rustc-like JSON output.
        RustcJson,
    }
}

/// A single import remapping: `[context:]prefix=path`.
#[derive(Clone)]
pub struct ImportRemapping {
    /// The remapping context, or empty string if none.
    pub context: String,
    pub prefix: String,
    pub path: String,
}

impl std::str::FromStr for ImportRemapping {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some((prefix_, path)) = s.split_once('=') {
            let (context, prefix) = prefix_.split_once(':').unzip();
            let prefix = prefix.unwrap_or(prefix_);
            if prefix.is_empty() {
                return Err("empty prefix");
            }
            Ok(Self {
                context: context.unwrap_or_default().into(),
                prefix: prefix.into(),
                path: path.into(),
            })
        } else {
            Err("missing '='")
        }
    }
}

impl fmt::Display for ImportRemapping {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.context.is_empty() {
            write!(f, "{}:", self.context)?;
        }
        write!(f, "{}={}", self.prefix, self.path)
    }
}

impl fmt::Debug for ImportRemapping {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ImportRemapping({self})")
    }
}

/// Wrapper to implement a custom `Default` value for the number of threads.
#[derive(Clone, Copy)]
pub struct Threads(pub NonZeroUsize);

impl From<Threads> for NonZeroUsize {
    fn from(threads: Threads) -> Self {
        threads.0
    }
}

impl From<NonZeroUsize> for Threads {
    fn from(n: NonZeroUsize) -> Self {
        Self(n)
    }
}

impl From<usize> for Threads {
    fn from(n: usize) -> Self {
        Self::resolve(n)
    }
}

impl Default for Threads {
    fn default() -> Self {
        Self::resolve(if SINGLE_THREADED_TARGET { 1 } else { 8 })
    }
}

impl std::str::FromStr for Threads {
    type Err = <NonZeroUsize as std::str::FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<usize>().map(Self::resolve)
    }
}

impl std::fmt::Display for Threads {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::fmt::Debug for Threads {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Threads {
    /// Resolves the number of threads to use.
    pub fn resolve(n: usize) -> Self {
        Self(
            NonZeroUsize::new(n)
                .or_else(|| std::thread::available_parallelism().ok())
                .unwrap_or(NonZeroUsize::MIN),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strum::IntoEnumIterator;

    #[cfg(not(feature = "serde"))]
    use serde_json as _;

    #[test]
    fn string_enum() {
        for value in EvmVersion::iter() {
            let s = value.to_str();
            assert_eq!(value.to_string(), s);
            assert_eq!(value, s.parse().unwrap());

            #[cfg(feature = "serde")]
            {
                let json_s = format!("\"{value}\"");
                assert_eq!(serde_json::to_string(&value).unwrap(), json_s);
                assert_eq!(serde_json::from_str::<EvmVersion>(&json_s).unwrap(), value);
            }
        }
    }
}
