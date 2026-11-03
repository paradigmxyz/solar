//! The operation definition language of Solar's MIR and EVM IR.
//!
//! Every MIR operation and EVM opcode, with every property the compiler reads from it, is
//! declared in `.ops` files: `crates/mir/ops/` and `crates/evmir/ops/`. Build scripts compile them
//! with this crate into Rust that the IR crates include. The language has one generic syntax
//! (see [`syntax`]) and one schema per dialect, which declares its own vocabulary in the same
//! files, so another crate can add operations with the same properties by adding a file.
//!
//! See `docs/OP_DSL.md` for the language reference and design.

#![cfg_attr(docsrs, feature(doc_cfg))]
// Build scripts read schema files directly; there is no source map yet.
#![allow(clippy::disallowed_methods)]

pub mod common;
pub mod evm;
pub mod mir;
pub mod syntax;

#[cfg(test)]
use snapbox as _;

use std::{env, fs, path::PathBuf};

/// The dialect a set of files declares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialect {
    /// MIR operations.
    Mir,
    /// EVM opcodes, stack operations, and gas tiers.
    Evm,
}

/// Compiles named sources of one dialect into named Rust files, or returns rendered diagnostics.
pub fn compile(
    dialect: Dialect,
    files: &[(String, String)],
) -> Result<Vec<(&'static str, String)>, String> {
    let render = |error: syntax::Error| syntax::render(&error, files);
    let mut parsed = Vec::new();
    for (index, (_, src)) in files.iter().enumerate() {
        parsed.push(syntax::parse(index, src).map_err(render)?);
    }
    match dialect {
        Dialect::Mir => {
            let mut schema = mir::Schema::default();
            for nodes in &parsed {
                schema.add_file(nodes).map_err(render)?;
            }
            Ok(vec![("mir_ops.rs", allow_style_lints(&schema.generate().map_err(render)?))])
        }
        Dialect::Evm => {
            let mut schema = evm::Schema::default();
            for nodes in &parsed {
                schema.add_file(nodes).map_err(render)?;
            }
            let (ops, gas) = schema.generate().map_err(render)?;
            Ok(vec![
                ("evm_ops.rs", allow_style_lints(&ops)),
                ("evm_gas.rs", allow_style_lints(&gas)),
            ])
        }
    }
}

/// Exempts every top-level generated item from style lints, which generated code need not meet.
fn allow_style_lints(code: &str) -> String {
    let mut out = String::with_capacity(code.len());
    for line in code.lines() {
        if ["impl ", "pub ", "pub(crate) ", "const ", "static ", "fn "]
            .iter()
            .any(|item| line.starts_with(item))
        {
            out.push_str(
                "#[allow(clippy::all, clippy::nursery, clippy::pedantic, unused_parens)]\n",
            );
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

impl Dialect {
    /// The environment variable that lists extension schema directories, like `PATH`.
    pub const fn extension_var(self) -> &'static str {
        match self {
            Self::Mir => "SOLAR_MIR_OPS_PATH",
            Self::Evm => "SOLAR_EVM_OPS_PATH",
        }
    }
}

/// Compiles every `.ops` file in `dir`, relative to the crate, and then in each directory that
/// [`Dialect::extension_var`] lists, into `$OUT_DIR`, for build scripts: `mir_ops.rs` for MIR,
/// and `evm_ops.rs` and `evm_gas.rs` for EVM. Declarations may refer to each other across files;
/// operations keep the order of the directories, of the files in each by name, and of the rows
/// within them, so extension operations follow the core ones.
///
/// # Panics
///
/// Panics with the rendered diagnostics when the schema does not compile.
pub fn build(dialect: Dialect, dir: &str) {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let var = dialect.extension_var();
    println!("cargo:rerun-if-env-changed={var}");
    let extensions = env::var_os(var)
        .map(|paths| env::split_paths(&paths).collect::<Vec<_>>())
        .unwrap_or_default();
    let mut files = Vec::new();
    for dir in std::iter::once(manifest.join(dir)).chain(extensions) {
        println!("cargo:rerun-if-changed={}", dir.display());
        let mut paths = fs::read_dir(&dir)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", dir.display()))
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "ops"))
            .collect::<Vec<_>>();
        paths.sort();
        for path in paths {
            println!("cargo:rerun-if-changed={}", path.display());
            let name = path.strip_prefix(&manifest).unwrap_or(&path).display().to_string();
            files.push((name, fs::read_to_string(&path).unwrap()));
        }
    }
    let outputs = compile(dialect, &files)
        .unwrap_or_else(|errors| panic!("invalid operation schema:\n{errors}"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    for (name, code) in outputs {
        fs::write(out_dir.join(name), code).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snapbox::{assert_data_eq, str};

    /// Compiles the MIR core schema with one extension file.
    fn with_core(extension: &str) -> Result<(), String> {
        let mut files = Vec::new();
        for name in ["vocabulary.ops", "core.ops"] {
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../mir/ops").join(name);
            files.push((name.to_string(), fs::read_to_string(path).unwrap()));
        }
        files.push(("acme.ops".to_string(), extension.to_string()));
        compile(Dialect::Mir, &files).map(drop)
    }

    #[test]
    fn extensions() {
        with_core(
            "dialect mir.acme;
             op AcmeAvg(a: value, b: value) -> I256 {
                 effect Pure;
                 commutative a, b;
                 types a: i256, b: i256;
             }
             op AcmeCall(gas: value?) {
                 effect ExternalCall;
                 traits observes_gas if gas is absent;
                 types gas: i256;
             }",
        )
        .unwrap();

        let errors = [
            "dialect mir.acme;
             op Avg(a: value) -> I256 { effect Pure; types a: i256; }",
            "dialect mir.acme;
             op AcmeAvg(a: value) -> I256 { mnemonic \"avg\"; effect Pure; types a: i256; }",
            "dialect mir.acme;
             op AcmeAvg(a: value) -> I256 { types a: i256; }",
            "dialect mir.acme;
             op AcmeAlloc(size: value, semantics: AllocationSemantics) {
                 effect MemoryWrite;
                 traits may_revert if semantics.initialization is Zeroed;
                 types size: i256;
             }",
            "dialect mir.acme;
             op AcmeCall(gas: value?) { effect Pure; traits observes_gas if gas; types gas: i256; }",
        ]
        .map(|extension| with_core(extension).unwrap_err())
        .join("\n");
        assert_data_eq!(
            errors,
            str![[r#"
acme.ops:2:14: an operation in namespace `acme` is named `Acme...`
               op Avg(a: value) -> I256 { effect Pure; types a: i256; }
               ^
acme.ops:2:14: mnemonic `avg` must start with `acme.`
               op AcmeAvg(a: value) -> I256 { mnemonic "avg"; effect Pure; types a: i256; }
               ^
acme.ops:2:14: an extension operation must declare its `effect`
               op AcmeAvg(a: value) -> I256 { types a: i256; }
               ^
acme.ops:4:39: attribute `AllocationSemantics` declares no field `initialization`
                   traits may_revert if semantics.initialization is Zeroed;
                                        ^
acme.ops:2:77: `gas` is a value operand, not an attribute
               op AcmeCall(gas: value?) { effect Pure; traits observes_gas if gas; types gas: i256; }
                                                                              ^
"#]]
        );
    }
}
