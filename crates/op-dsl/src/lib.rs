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

use std::{
    env, fs,
    path::{Path, PathBuf},
};

/// The dialect a set of files declares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialect {
    /// MIR operations.
    Mir,
    /// EVM opcodes, stack operations, and gas tiers.
    Evm,
}

/// The environment variable that lists MIR extension schema directories, like `PATH`.
pub const MIR_EXTENSIONS_VAR: &str = "SOLAR_MIR_OPS_PATH";

/// Compiles named sources of one dialect into named Rust files, or returns rendered diagnostics.
/// `extensions` are MIR extension files, which must declare a namespace.
pub fn compile(
    dialect: Dialect,
    core: &[(String, String)],
    extensions: &[(String, String)],
) -> Result<Vec<(&'static str, String)>, String> {
    let files = [core, extensions].concat();
    let render = |error: syntax::Error| syntax::render(&error, &files);
    let mut parsed = Vec::new();
    for (index, (_, src)) in files.iter().enumerate() {
        parsed.push(syntax::parse(index, src).map_err(render)?);
    }
    match dialect {
        Dialect::Mir => {
            let mut schema = mir::Schema::default();
            for (index, nodes) in parsed.iter().enumerate() {
                schema.add_file(nodes, index >= core.len()).map_err(render)?;
            }
            Ok(vec![("mir_ops.rs", allow_style_lints(&schema.generate().map_err(render)?))])
        }
        Dialect::Evm => {
            assert!(extensions.is_empty(), "EVM schemas take no extensions");
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

/// Compiles every `.ops` file in `dir`, relative to the crate, into `$OUT_DIR`, for build
/// scripts: `mir_ops.rs` for MIR, and `evm_ops.rs` and `evm_gas.rs` for EVM. For MIR, the
/// absolute directories that [`MIR_EXTENSIONS_VAR`] lists add extension files. Declarations may
/// refer to each other across files; operations keep the order of the directories, of the files
/// in each by name, and of the rows within them, so extension operations follow the core ones.
///
/// # Panics
///
/// Panics with the rendered diagnostics when the schema does not compile, or when an extension
/// directory is relative or cannot be read.
pub fn build(dialect: Dialect, dir: &str) {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let core = read_dir(&manifest, &manifest.join(dir));
    let mut extensions = Vec::new();
    if dialect == Dialect::Mir {
        println!("cargo:rerun-if-env-changed={MIR_EXTENSIONS_VAR}");
        let paths = env::var_os(MIR_EXTENSIONS_VAR).unwrap_or_default();
        for dir in env::split_paths(&paths).filter(|dir| !dir.as_os_str().is_empty()) {
            // Build scripts run in the crate's directory, not where cargo was invoked.
            assert!(
                dir.is_absolute(),
                "{MIR_EXTENSIONS_VAR} entry `{}` is not absolute",
                dir.display()
            );
            extensions.extend(read_dir(&manifest, &dir));
        }
    }
    let outputs = compile(dialect, &core, &extensions)
        .unwrap_or_else(|errors| panic!("invalid operation schema:\n{errors}"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    for (name, code) in outputs {
        fs::write(out_dir.join(name), code).unwrap();
    }
}

/// Reads the `.ops` files of `dir` in name order, named relative to `manifest` when inside it.
fn read_dir(manifest: &Path, dir: &Path) -> Vec<(String, String)> {
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut paths = fs::read_dir(dir)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "ops"))
        .collect::<Vec<_>>();
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            println!("cargo:rerun-if-changed={}", path.display());
            let name = path.strip_prefix(manifest).unwrap_or(&path).display().to_string();
            (name, fs::read_to_string(&path).unwrap())
        })
        .collect()
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
        compile(Dialect::Mir, &files, &[("acme.ops".to_string(), extension.to_string())]).map(drop)
    }

    #[test]
    fn extensions() {
        with_core(
            "dialect mir.acme;
             trait acme_hot;
             class Binary(a: value, b: value) -> I256 {
                 traits acme_hot;
                 types a: i256, b: i256;
             }
             op Avg: Binary {
                 effect Pure;
                 commutative a, b;
                 builder;
             }
             op SetReg(value: value) {
                 mnemonic \"acme.setreg\";
                 effect MemoryWrite;
                 side_effects;
                 types value: i256;
             }",
        )
        .unwrap();

        let errors = [
            "dialect mir;
             op Avg(a: value) -> I256 { effect Pure; types a: i256; }",
            "dialect mir.Acme;",
            "dialect mir.acme;
             enum Mode: A | B;",
            "dialect mir.acme;
             trait hot;",
            "dialect mir.acme;
             op Avg(a: value) -> I256 { mnemonic \"avg\"; effect Pure; types a: i256; }",
            "dialect mir.acme;
             op Avg(a: value) -> I256 { types a: i256; }",
            "dialect mir.acme;
             op Call(gas: value?) { effect ExternalCall; types gas: i256; }",
            "dialect mir.acme;
             op Lt(a: value) -> I1 { effect Pure; traits reversible; types a: i256; }",
        ]
        .map(|extension| with_core(extension).unwrap_err())
        .join("\n");
        assert_data_eq!(
            errors,
            str![[r#"
acme.ops:1:1: an extension file names its namespace, as in `dialect mir.acme;`
  dialect mir;
  ^
acme.ops:1:13: a namespace is a lowercase letter followed by lowercase letters and digits
  dialect mir.Acme;
              ^
acme.ops:2:14: a namespaced file declares only `op`, `class`, and `trait`
               enum Mode: A | B;
               ^
acme.ops:2:14: a trait in namespace `acme` is named `acme_...`
               trait hot;
               ^
acme.ops:2:14: mnemonic `avg` is not `acme.` and a lowercase name
               op Avg(a: value) -> I256 { mnemonic "avg"; effect Pure; types a: i256; }
               ^
acme.ops:2:14: an extension operation must declare its `effect`
               op Avg(a: value) -> I256 { types a: i256; }
               ^
acme.ops:2:14: an extension operation takes only `value` operands
               op Call(gas: value?) { effect ExternalCall; types gas: i256; }
               ^
acme.ops:2:58: a `reversible` operation takes two values
               op Lt(a: value) -> I1 { effect Pure; traits reversible; types a: i256; }
                                                           ^
"#]]
        );
    }
}
