//! `solar export-core`: the compiler-owned modules as files other compilers can read.
//!
//! Each `solar:core/` module is written under the directory at its import path, so a source that
//! imports `solar:core/v1/Bytes.sol` resolves unchanged: solc finds the file with
//! `--include-path <dir>`, and Foundry from a tree written at the project root. A remapping cannot
//! supply it, because remapping syntax reads the colon as a context separator. The files hold the
//! modules' portable bodies, which other compilers compile as written; this compiler resolves the
//! same imports to its own modules and never reads the files, and a standard JSON input that
//! carries an exact copy of a module is accepted without a warning.

use crate::args::ExportCoreArgs;
use std::process::ExitCode;

pub(crate) fn run(args: ExportCoreArgs) -> ExitCode {
    match export(&args.dir) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!(
                "error: failed to export the core modules to `{}`: {err}",
                args.dir.display()
            );
            ExitCode::FAILURE
        }
    }
}

/// Writes every module under `dir`.
fn export(dir: &std::path::Path) -> std::io::Result<()> {
    for module in solar_sema::core::MODULES {
        let path = dir.join(module.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, module.source)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // The test reads back what it wrote, not a source file.
    #[test]
    #[allow(clippy::disallowed_methods)]
    fn writes_every_module_at_its_import_path() {
        let dir = std::env::temp_dir().join(format!("solar-export-core-{}", std::process::id()));
        export(&dir).unwrap();
        for module in solar_sema::core::MODULES {
            assert_eq!(std::fs::read(dir.join(module.path)).unwrap(), module.source.as_bytes());
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
