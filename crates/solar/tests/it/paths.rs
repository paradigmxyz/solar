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

/// A relative import can reach a file in an include path whose name matches an input file.
#[test]
fn imported_source_unit_name_collision() {
    let dir = project(&[
        ("src/Main.sol", "import \"x/A.sol\"; contract M {}"),
        ("lib/x/A.sol", "import \"../Main.sol\"; contract A {}"),
        ("lib/Main.sol", "contract N {}"),
    ]);
    let output =
        compile(dir.path(), &["--base-path", "src", "--include-path", "lib", "src/Main.sol"]);
    assert!(!output.status.success());
    snapbox::assert_data_eq!(
        String::from_utf8(output.stderr).unwrap(),
        snapbox::str![[r#"
error: source unit name `Main.sol` matches multiple files
  │
  [..] note: `[..]/lib/Main.sol` and `[..]/src/Main.sol`
...
error: aborting due to 1 previous error


"#]]
    );
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

/// Like solc, an import resolves to an input file with its source unit name before searching the
/// base path and include paths.
#[test]
fn imports_prefer_input_files() {
    let dir = project(&[
        ("src/A.sol", "import \"X.sol\"; contract A {}"),
        ("src/X.sol", "contract Y {}"),
        ("lib/X.sol", "contract X {}"),
    ]);
    for args in [["lib/X.sol", "src/A.sol"], ["src/A.sol", "lib/X.sol"]] {
        let output = compile(
            dir.path(),
            &[&["--base-path", "src", "--include-path", "lib"][..], &args].concat(),
        );
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let output = serde_json::from_slice::<Value>(&output.stdout).unwrap();
        let names = output["contracts"].as_object().unwrap().keys().collect::<Vec<_>>();
        assert_eq!(names, ["A.sol:A", "X.sol:X"]);
    }
}

#[test]
fn base_path_is_not_a_directory() {
    let dir = project(&[("A.sol", "contract A {}")]);
    let output = compile(dir.path(), &["--base-path", "A.sol", "A.sol"]);
    assert!(!output.status.success());
    snapbox::assert_data_eq!(
        String::from_utf8(output.stderr).unwrap(),
        snapbox::str![[r#"
error: base path `A.sol` is not a directory
...
"#]]
    );
}

/// An input file only replaces the import of its own source unit name, not of a name that
/// normalizes to it.
#[test]
fn input_files_keep_leading_parent_segments() {
    let dir = project(&[
        ("proj/src/A.sol", "import \"shared/X.sol\"; contract A {}"),
        ("proj/shared/X.sol", "contract Inner {}"),
        ("shared/X.sol", "contract Outer {}"),
    ]);
    let output =
        compile(&dir.path().join("proj"), &["src/A.sol", "shared/X.sol", "shared/=../shared/"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let output = serde_json::from_slice::<Value>(&output.stdout).unwrap();
    let names = output["contracts"].as_object().unwrap().keys().collect::<Vec<_>>();
    assert_eq!(names.len(), 3, "{names:?}");
    assert!(names.iter().any(|name| name.ends_with("/shared/X.sol:Outer")), "{names:?}");
}
