//! Compiler-owned modules under the reserved `solar:core/` import prefix.
//!
//! A source importing `solar:core/Bytes.sol` gets the text `solar-std` embeds,
//! never a file: the prefix is intercepted before file resolution, so neither
//! a remapping nor a file on disk can stand in for a module, and a source unit
//! supplied under one of these names is set aside in favour of the module.
//! A source that an embedder loads under a reserved name, or that already
//! holds one when an import asks for the module, is accepted only when it is
//! the module's exact text. That is what gives a module's declarations an
//! identity the compiler can trust by name.
//!
//! Each module has a solc-compatible implementation. Loading under the reserved prefix
//! requires the exact embedded text, so a caller cannot replace a compiler-owned declaration.

use self::CoreIntrinsic::*;
use crate::hir;
use solar_interface::{
    Symbol,
    diagnostics::{DiagCtxt, ErrorGuaranteed},
    source_map::{FileName, SourceFile},
    sym,
};

pub use solar_std::{MODULES, Module as CoreModule, PREFIX};

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
    module_path(name).is_some()
}

/// The file name a compiler-owned module is registered under.
pub fn file_name(module: &CoreModule) -> FileName {
    FileName::real(module.path)
}

/// Checks that `file`, when a reserved name holds it, is the compiler-owned module that name
/// reserves, with exactly its text. Every identity check trusts the name alone afterwards.
pub fn check_reserved_source(dcx: &DiagCtxt, file: &SourceFile) -> Result<(), ErrorGuaranteed> {
    let Some(path) = module_path(&file.name) else { return Ok(()) };
    if lookup(path).is_some_and(|module| module.source == file.src.as_str()) {
        return Ok(());
    }
    Err(dcx
        .err(format!("source `{path}` is reserved for a compiler module"))
        .note(format!(
            "names under `{PREFIX}` identify the modules this compiler provides, so a source \
             under one must be that module's exact text"
        ))
        .emit())
}

/// Operations lowered directly from a verified compiler-owned module.
#[derive(Clone, Copy, Debug)]
pub enum CoreIntrinsic {
    LeadingZeros,
    HighestSetBit,
    TrailingZeros,
    WrappingAdd,
    WrappingSub,
    WrappingMul,
    Mul512,
    Read { width: u8, calldata: bool },
    Write(u8),
    Copy { calldata: bool },
    Truncate,
    KeccakRange,
}

/// Identifies declarations by their exact compiler-owned source, never a user function's name.
pub fn intrinsic_of(gcx: crate::Gcx<'_>, function: hir::FunctionId) -> Option<CoreIntrinsic> {
    let id = function;
    let function = gcx.hir.function(id);
    let path = module_path(&gcx.hir.source(function.source).file.name)?;
    let name = gcx.item_name(id).name;
    match path {
        "solar:core/Bits.sol" => match name {
            sym::leadingZeros => Some(LeadingZeros),
            sym::highestSetBit => Some(HighestSetBit),
            sym::trailingZeros => Some(TrailingZeros),
            _ => None,
        },
        "solar:core/Math.sol" => match name {
            sym::wrappingAdd => Some(WrappingAdd),
            sym::wrappingSub => Some(WrappingSub),
            sym::wrappingMul => Some(WrappingMul),
            sym::mul512 => Some(Mul512),
            _ => None,
        },
        "solar:core/Hash.sol" if name == sym::keccak256Range => Some(KeccakRange),
        "solar:core/Arrays.sol" if name == sym::truncate => Some(Truncate),
        "solar:core/Bytes.sol" | "solar:core/CalldataBytes.sol" => {
            let calldata = path == "solar:core/CalldataBytes.sol";
            if name == sym::copyInto {
                return Some(Copy { calldata });
            }
            if name == sym::readUint256BE {
                return Some(Read { width: 32, calldata });
            }
            if !calldata && name == sym::writeUint256BE {
                return Some(Write(32));
            }
            for width in 1..=32 {
                if name == Symbol::intern(&format!("readBytes{width}")) {
                    return Some(Read { width, calldata });
                }
                if !calldata && name == Symbol::intern(&format!("writeBytes{width}")) {
                    return Some(Write(width));
                }
            }
            None
        }
        _ => None,
    }
}

/// A reserved source name, independent of how an embedder represents a virtual file.
pub fn module_path(name: &FileName) -> Option<&str> {
    let name = match name {
        FileName::Custom(name) => name.as_str(),
        FileName::Real(path) => path.to_str()?,
        _ => return None,
    };
    is_reserved_path(name).then_some(name)
}

/// Gives an explicitly supplied module the same source identity as an embedded import.
pub(crate) fn canonical_file_name(name: FileName) -> FileName {
    match module_path(&name) {
        Some(path) => FileName::real(path),
        None => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Compiler;
    use snapbox::{assert_data_eq, str};
    use solar_interface::{ColorChoice, Session};
    use std::path::PathBuf;

    const MAIN: &str = r#"import {Bytes} from "solar:core/Bytes.sol"; contract C {}"#;

    /// Parses `main.sol`, which imports `Bytes`, after `load` registers other sources, and
    /// returns the errors emitted.
    fn parse_errors(load: impl FnOnce(&mut crate::ParsingContext<'_>, &Session) + Send) -> String {
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        sess.dcx.set_flags(|flags| flags.track_diagnostics = false);
        let mut compiler = Compiler::new(sess);
        compiler.enter_mut(|c| {
            let mut pcx = c.parse();
            load(&mut pcx, c.sess());
            pcx.add_file(
                c.sess().source_map().new_source_file(PathBuf::from("main.sol"), MAIN).unwrap(),
            );
            pcx.parse();
        });
        match compiler.sess().dcx.emitted_errors().unwrap() {
            Ok(()) => String::new(),
            Err(errors) => errors.to_string(),
        }
    }

    const FORGED: &str = "library Bytes {}";

    #[test]
    fn loaded_module_must_be_exact() {
        let errors = parse_errors(|pcx, _| {
            let _ = pcx.par_load_files_with_contents([(
                "solar:core/Bytes.sol".to_string(),
                FORGED.to_string(),
            )]);
        });
        assert_data_eq!(
            errors,
            str![[r#"
error: source `solar:core/Bytes.sol` is reserved for a compiler module
  │
  ╰ note: names under `solar:core/` identify the modules this compiler provides, so a source under one must be that module's exact text


"#]]
        );
    }

    #[test]
    fn unknown_reserved_name() {
        let errors = parse_errors(|pcx, _| {
            let _ = pcx.par_load_files_with_contents([(
                "solar:core/Fake.sol".to_string(),
                FORGED.to_string(),
            )]);
        });
        assert_data_eq!(
            errors,
            str![[r#"
error: source `solar:core/Fake.sol` is reserved for a compiler module
  │
  ╰ note: names under `solar:core/` identify the modules this compiler provides, so a source under one must be that module's exact text


"#]]
        );
    }

    #[test]
    fn import_rejects_preloaded_text() {
        let errors = parse_errors(|_, sess| {
            sess.source_map().new_source_file("solar:core/Bytes.sol".to_string(), FORGED).unwrap();
        });
        assert_data_eq!(
            errors,
            str![[r#"
error: source `solar:core/Bytes.sol` is reserved for a compiler module
  │
  ╰ note: names under `solar:core/` identify the modules this compiler provides, so a source under one must be that module's exact text


"#]]
        );
    }

    #[test]
    fn exact_copy_is_the_module() {
        let errors = parse_errors(|pcx, _| {
            let module = lookup("solar:core/Bytes.sol").unwrap();
            pcx.par_load_files_with_contents([(module.path.to_string(), module.source)]).unwrap();
        });
        assert_data_eq!(errors, str![""]);
    }
}
