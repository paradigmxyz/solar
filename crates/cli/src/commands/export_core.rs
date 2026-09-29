//! `solar export-core`: the compiler-owned modules as files other compilers can read.
//!
//! Each `solar:core/` module is written under the directory at its import path, so a source that
//! imports `solar:core/v1/Bytes.sol` resolves unchanged: solc finds the file with
//! `--include-path <dir>`, and Foundry from a tree written at the project root. A remapping cannot
//! supply it, because remapping syntax reads the colon as a context separator. The files hold the
//! modules' portable bodies, which other compilers compile as written; this compiler resolves the
//! same imports to its own modules and never reads the files, and a standard JSON input that
//! carries an exact copy of a module is accepted without a warning.
//!
//! The paths are predictable, so a tree prepared with a symbolic link at one of them, such as
//! `solar:core/v1` pointing elsewhere, would have the export write outside the directory. No
//! component below the directory may be a symbolic link; the directory itself is the user's
//! choice.

use crate::args::ExportCoreArgs;
use std::{fs, io, path::Path, process::ExitCode};

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

/// Writes every module under `dir`, refusing to write through a symbolic link below it.
fn export(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    for module in solar_sema::core::MODULES {
        let mut path = dir.to_path_buf();
        let mut components = Path::new(module.path).components().peekable();
        while let Some(component) = components.next() {
            path.push(component);
            let is_file = components.peek().is_none();
            match fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.file_type().is_symlink() => return Err(symlink(&path)),
                Ok(_) => {}
                Err(err) if err.kind() == io::ErrorKind::NotFound && !is_file => {
                    fs::create_dir(&path)?;
                }
                Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                Err(err) => return Err(err),
            }
        }
        fs::write(&path, module.source)?;
    }
    Ok(())
}

fn symlink(path: &Path) -> io::Error {
    io::Error::other(format!("refusing to write through the symbolic link `{}`", path.display()))
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

    /// A link at a module's directory or at the module itself is not followed, so nothing outside
    /// the export directory is written.
    #[cfg(unix)]
    #[test]
    #[allow(clippy::disallowed_methods)]
    fn refuses_symbolic_links() {
        let root =
            std::env::temp_dir().join(format!("solar-export-core-links-{}", std::process::id()));
        let (dir, outside) = (root.join("export"), root.join("outside"));
        fs::create_dir_all(&dir).unwrap();
        fs::create_dir_all(&outside).unwrap();

        std::os::unix::fs::symlink(&outside, dir.join("solar:core")).unwrap();
        let err = export(&dir).unwrap_err();
        assert_eq!(
            err.to_string(),
            format!(
                "refusing to write through the symbolic link `{}`",
                dir.join("solar:core").display()
            )
        );
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);

        fs::remove_file(dir.join("solar:core")).unwrap();
        let module = &solar_sema::core::MODULES[0];
        let path = dir.join(module.path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let target = outside.join("target");
        fs::write(&target, "kept").unwrap();
        std::os::unix::fs::symlink(&target, &path).unwrap();
        assert!(export(&dir).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"kept");

        fs::remove_dir_all(root).unwrap();
    }
}
