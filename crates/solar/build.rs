use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

const SOLSMITH: &str = r#"#!/usr/bin/env python3
"""Run the SolSmith generator from a stable repository-local entrypoint."""

from __future__ import annotations

import importlib.util
import sys
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parents[1] / "fandango"
sys.path.insert(0, str(SCRIPT_DIR))
sys.argv[0] = "solsmith"

spec = importlib.util.spec_from_file_location(
    "_solar_solsmith",
    SCRIPT_DIR / "solsmith.py",
)
if spec is None or spec.loader is None:
    raise RuntimeError("could not load SolSmith")
module = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = module
spec.loader.exec_module(module)
raise SystemExit(module.main())
"#;

const SOLREDUCE: &str = r#"#!/usr/bin/env python3
"""Run the SolReduce reducer from a stable repository-local entrypoint."""

from __future__ import annotations

import importlib.util
import sys
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parents[1] / "fandango"
sys.path.insert(0, str(SCRIPT_DIR))
sys.argv[0] = "solreduce"

spec = importlib.util.spec_from_file_location(
    "_solar_solreduce",
    SCRIPT_DIR / "reduce_runtime_failure.py",
)
if spec is None or spec.loader is None:
    raise RuntimeError("could not load SolReduce")
module = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = module
spec.loader.exec_module(module)
raise SystemExit(module.main())
"#;

fn main() {
    install_fuzz_bins();
}

fn install_fuzz_bins() {
    let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") else {
        return;
    };
    let workspace = PathBuf::from(manifest_dir).join("../..");
    let fuzz = workspace.join("fuzz");
    let fandango = fuzz.join("fandango");
    let solsmith = fandango.join("solsmith.py");
    let solreduce = fandango.join("reduce_runtime_failure.py");
    if !solsmith.is_file() || !solreduce.is_file() {
        return;
    }

    println!("cargo:rerun-if-changed={}", solsmith.display());
    println!("cargo:rerun-if-changed={}", solreduce.display());

    if let Err(err) = install_fuzz_bins_inner(&fuzz) {
        println!("cargo:warning=failed to install fuzz bin wrappers: {err}");
    }
}

fn install_fuzz_bins_inner(fuzz: &Path) -> io::Result<()> {
    let bin = fuzz.join("bin");
    fs::create_dir_all(&bin)?;
    write_executable(&bin.join("solsmith"), SOLSMITH)?;
    write_executable(&bin.join("solreduce"), SOLREDUCE)?;
    Ok(())
}

fn write_executable(path: &Path, contents: &str) -> io::Result<()> {
    fs::write(path, contents)?;
    set_executable(path)
}

#[cfg(unix)]
fn set_executable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> io::Result<()> {
    Ok(())
}
