//! Command-line path resolution and source unit names.

use serde_json::Value;
use std::{
    path::Path,
    process::{Command, Output},
};

const SOLAR: &str = env!("CARGO_BIN_EXE_solar");

fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (path, contents) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
    dir
}

fn compile(dir: &Path, args: &[&str]) -> Output {
    Command::new(SOLAR).current_dir(dir).arg("--emit=abi").args(args).output().unwrap()
}

/// Inputs are relative to the current directory, and files are named relative to the base path
/// or the include path that contains them, like solc does.
#[test]
fn base_and_include_paths_name_sources() {
    let dir = project(&[
        ("src/A.sol", "import \"dep/C.sol\"; contract A {}"),
        ("lib/dep/C.sol", "import \"./D.sol\"; contract C {}"),
        ("lib/dep/D.sol", "contract D {}"),
    ]);
    let output = compile(dir.path(), &["--base-path", "src", "--include-path", "lib", "src/A.sol"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let output = serde_json::from_slice::<Value>(&output.stdout).unwrap();
    let names = output["contracts"].as_object().unwrap().keys().collect::<Vec<_>>();
    assert_eq!(names, ["A.sol:A", "dep/C.sol:C", "dep/D.sol:D"]);
}

/// Like solc, an import resolves to a loaded source with its source unit name before searching the
/// base path and include paths.
#[test]
fn imports_prefer_loaded_sources() {
    let dir = project(&[
        ("src/Main.sol", "import \"x/A.sol\"; contract M {}"),
        ("lib/x/A.sol", "import \"../Main.sol\"; contract A {}"),
        ("lib/Main.sol", "contract N {}"),
    ]);
    let output =
        compile(dir.path(), &["--base-path", "src", "--include-path", "lib", "src/Main.sol"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let output = serde_json::from_slice::<Value>(&output.stdout).unwrap();
    let names = output["contracts"].as_object().unwrap().keys().collect::<Vec<_>>();
    assert_eq!(names, ["Main.sol:M", "x/A.sol:A"]);
}

#[test]
fn source_unit_name_collision() {
    let dir = project(&[("src/X.sol", "contract X {}"), ("lib/X.sol", "contract Y {}")]);
    let args = ["--base-path", "src", "--include-path", "lib", "src/X.sol", "lib/X.sol"];
    let output = compile(dir.path(), &args);
    assert!(!output.status.success());
    snapbox::assert_data_eq!(
        String::from_utf8(output.stderr).unwrap(),
        snapbox::str![[r#"
error: source unit name `X.sol` matches multiple files
  │
  [..] note: `[..]/lib/X.sol` and `[..]/src/X.sol`
...
error: aborting due to 1 previous error


"#]]
    );
}

/// Without a base path, the current directory is the base path, and repeating an include path
/// does not make imports ambiguous.
#[test]
fn include_paths_without_base_path() {
    let dir = project(&[
        ("src/A.sol", "import \"dep/C.sol\"; contract A {}"),
        ("lib/dep/C.sol", "contract C {}"),
    ]);
    let output = compile(dir.path(), &["--include-path", "lib", "-I", "lib", "src/A.sol"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let output = serde_json::from_slice::<Value>(&output.stdout).unwrap();
    let names = output["contracts"].as_object().unwrap().keys().collect::<Vec<_>>();
    assert_eq!(names, ["lib/dep/C.sol:C", "src/A.sol:A"]);
}
