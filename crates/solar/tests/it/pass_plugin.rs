//! Real native libraries exercise the public loader and transactional IR validation.

use libloading::Library;
use serde_json::{Value, json};
use snapbox::{assert_data_eq, str};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

mod artifact;
mod edgecov_tests;
mod frontend;
mod frontend_review;
mod graph_tests;
mod review_tests;
mod runtime;
mod split_tests;
use artifact::artifact;

const SOLAR: &str = env!("CARGO_BIN_EXE_solar");
const MIR: &str = "@module PluginTest\n@phase lowered\nfn @entry() [entry] {\n bb0:\n v0 = calldataload 0\n v1 = add v0, 0\n mstore 0, v1\n returndata 0, 32\n}\n";
const EVM: &str = "@module PluginTest\nbb0:\n calldatasize\n not\n not\n push 0\n mstore\n push 32\n push 0\n return\n";

struct EmptyHost;

impl solar_mir_interpreter::Host for EmptyHost {
    fn read(&mut self, _: u8, _: &[alloy_primitives::U256]) -> Option<alloy_primitives::U256> {
        None
    }
    fn storage(&mut self, _: alloy_primitives::U256) -> alloy_primitives::U256 {
        alloy_primitives::U256::ZERO
    }
}

impl solar_evmir_interpreter::Host for EmptyHost {
    fn read(&mut self, _: u8, _: &[alloy_primitives::U256]) -> Option<alloy_primitives::U256> {
        None
    }
    fn storage(&mut self, _: alloy_primitives::U256) -> alloy_primitives::U256 {
        alloy_primitives::U256::ZERO
    }
}

fn sample() -> PathBuf {
    library_path("solar_pass_plugin_example")
}

fn library_path(package: &str) -> PathBuf {
    library_for_binary(package, Path::new(SOLAR)).expect("Cargo must build the Rust plugin")
}

fn library_for_binary(package: &str, binary: &Path) -> Option<PathBuf> {
    let profile = binary.parent()?;
    [profile.join("deps"), profile.to_path_buf()]
        .into_iter()
        .find_map(|directory| library_in(package, &directory))
}

fn library_in(package: &str, directory: &Path) -> Option<PathBuf> {
    let stem = format!("{}{package}", std::env::consts::DLL_PREFIX);
    let direct = directory.join(format!("{stem}{}", std::env::consts::DLL_SUFFIX));
    if direct.is_file() && compatible_fixture(&direct) {
        return Some(direct);
    }
    let mut candidates = fs::read_dir(directory)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            let Some(suffix) = path
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.strip_prefix(&stem))
                .and_then(|name| name.strip_suffix(std::env::consts::DLL_SUFFIX))
            else {
                return false;
            };
            suffix.strip_prefix('-').is_some_and(|hash| {
                !hash.is_empty() && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
        })
        .collect::<Vec<_>>();
    candidates.sort();
    candidates.into_iter().find(|path| compatible_fixture(path))
}

fn compatible_fixture(path: &Path) -> bool {
    // Cargo may emit only a hashed dylib in `deps`. Ignore stale artifacts from other SDK
    // builds instead of selecting whichever filename happens to appear first.
    // SAFETY: These are trusted test fixtures. Only read the fixed-size fingerprint;
    // incompatible Rust entry points are never called.
    let Ok(library) = (unsafe { Library::new(path) }) else { return false };
    let Ok(fingerprint) =
        (unsafe { library.get::<*const [u8; 32]>(solar_pass_plugin::__private::ABI_SYMBOL) })
    else {
        return false;
    };
    // SAFETY: Fixture libraries export this symbol as a 32-byte array.
    unsafe { **fingerprint == solar_pass_plugin::ABI_FINGERPRINT }
}

fn run(path: &Path, plugin: &Path, flags: &[&str]) -> Output {
    let mut command = Command::new(SOLAR);
    command
        .args(["--allow=2264", "-j1", "--error-format=rustc-json"])
        .arg(format!("-Zload-pass-plugin={}", plugin.display()))
        .args(flags)
        .arg(path);
    if let Some(mode) = plugin
        .file_stem()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix(std::env::consts::DLL_PREFIX))
        .and_then(|name| name.strip_prefix("broken_"))
    {
        command.env("SOLAR_TEST_PASS_PLUGIN_MODE", mode);
    }
    command.output().unwrap()
}

fn diagnostic_messages(output: &Output) -> Vec<String> {
    String::from_utf8_lossy(&output.stderr)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|value| value["level"] == "error")
        .map(|value| value["message"].as_str().unwrap().to_owned())
        .collect()
}

fn messages(output: &Output) -> String {
    serde_json::to_string_pretty(&diagnostic_messages(output)).unwrap()
}

fn broken(directory: &Path, mode: &str) -> PathBuf {
    let library = directory.join(format!(
        "{}broken_{mode}{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    ));
    if !matches!(mode, "missing" | "missing_abi" | "wrong_abi") {
        fs::copy(library_path("solar_pass_plugin_fixture"), &library).unwrap();
        return library;
    }
    let fingerprint = directory.join("abi.bin");
    fs::write(&fingerprint, solar_pass_plugin::ABI_FINGERPRINT).unwrap();
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/it/auxiliary/pass_plugin_abi.rs");
    let output = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
        .args(["--edition=2024", "--crate-type=dylib", "--crate-name=broken_plugin", "--cfg"])
        .env("SOLAR_TEST_ABI_FILE", fingerprint)
        .arg(mode)
        .arg(source)
        .arg("-o")
        .arg(&library)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    library
}

#[test]
fn plugin_discovery_from_relocated_test_executable() {
    const MARKER: &str = "SOLAR_TEST_RELOCATED_PLUGIN_LOOKUP";
    if std::env::var_os(MARKER).is_some() {
        assert!(sample().is_file());
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let relocated = directory.path().join(format!("it{}", std::env::consts::EXE_SUFFIX));
    fs::copy(std::env::current_exe().unwrap(), &relocated).unwrap();
    let output = Command::new(relocated)
        .args(["--exact", "pass_plugin::plugin_discovery_from_relocated_test_executable"])
        .env(MARKER, "1")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn hashed_plugin_artifacts_load_both_ir_layers() {
    let directory = tempfile::tempdir().unwrap();
    let deps = directory.path().join("deps");
    fs::create_dir(&deps).unwrap();
    let mismatched = broken(directory.path(), "wrong_abi");
    for package in ["solar_pass_plugin_example", "solar_pass_plugin_fixture"] {
        let hashed = deps.join(format!(
            "{}{package}-0123456789abcdef{}",
            std::env::consts::DLL_PREFIX,
            std::env::consts::DLL_SUFFIX
        ));
        fs::copy(library_path(package), &hashed).unwrap();
        assert_eq!(library_in(package, &deps).as_ref(), Some(&hashed));
        assert_eq!(
            library_for_binary(package, &directory.path().join("solar")),
            Some(hashed.clone())
        );
        // A leftover un-hashed library must not shadow a matching hashed build.
        let stale = deps.join(format!(
            "{}{package}{}",
            std::env::consts::DLL_PREFIX,
            std::env::consts::DLL_SUFFIX
        ));
        fs::copy(&mismatched, stale).unwrap();
        assert_eq!(library_in(package, &deps).as_ref(), Some(&hashed));
    }
    let plugin = library_in("solar_pass_plugin_example", &deps).unwrap();
    for (extension, source, pipeline) in [
        ("mir", MIR, "-Zmir-pipeline=example::simplify-lowered"),
        ("evmir", EVM, "-Zevm-ir-pipeline=example::remove-double-not"),
    ] {
        let path = directory.path().join(format!("test.{extension}"));
        fs::write(&path, source).unwrap();
        let output = run(&path, &plugin, &["-Ogas", pipeline, "--emit=bin-runtime"]);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let input = alloy_primitives::U256::from(123).to_be_bytes::<32>();
        let expected = if extension == "mir" {
            input.to_vec()
        } else {
            alloy_primitives::U256::from(input.len()).to_be_bytes::<32>().to_vec()
        };
        assert_eq!(runtime::execute(&artifact(&output), &input), expected);
    }
}

#[test]
fn native_passes_transform_both_ir_layers() {
    let directory = tempfile::tempdir().unwrap();
    let mir = directory.path().join("test.mir");
    let evm = directory.path().join("test.evmir");
    fs::write(&mir, MIR).unwrap();
    fs::write(&evm, EVM).unwrap();
    let sample = sample();
    let mir_result = run(
        &mir,
        &sample,
        &["-Ogas", "-Zmir-pipeline=example::simplify-lowered", "-Zprint-after-each"],
    );
    assert!(mir_result.status.success(), "{}", String::from_utf8_lossy(&mir_result.stderr));
    let mir_output = String::from_utf8(mir_result.stdout).unwrap();
    assert_data_eq!(
        mir_output.replace(&mir.display().to_string(), "$INPUT"),
        str![[r#"
; === $INPUT (after example::simplify-lowered) ===
@module PluginTest
@phase lowered
; runtime dispatch entry
fn @entry() [entry] {
  bb0:
    v0 = calldataload 0
    v1 = or v0, 0
    mstore 0, v1
    returndata 0, 32
}

"#]]
    );
    let evm_result = run(
        &evm,
        &sample,
        &["-Ogas", "-Zevm-ir-pipeline=example::remove-double-not", "-Zprint-after-each"],
    );
    assert!(evm_result.status.success(), "{}", String::from_utf8_lossy(&evm_result.stderr));
    let evm_output = String::from_utf8(evm_result.stdout).unwrap();
    assert_data_eq!(
        evm_output.replace(&evm.display().to_string(), "$INPUT"),
        str![[r#"
; === $INPUT (after example::remove-double-not) ===
@module PluginTest
bb0:
  calldatasize
  push 0
  mstore
  push 32
  push 0
  return

"#]]
    );
    let mir_text = &mir_output[mir_output.find("@module").unwrap()..];
    let evm_text = &evm_output[evm_output.find("@module").unwrap()..];
    for value in [0_u64, 1, 31, 256, u64::MAX] {
        let input = alloy_primitives::U256::from(value).to_be_bytes::<32>();
        let expected = input.to_vec();
        let actual =
            solar_mir_interpreter::transact(mir_text, &input, &mut EmptyHost, Default::default())
                .unwrap();
        assert_eq!(actual.outcome, solar_mir_interpreter::Outcome::Success(expected));
        let calldata = &input[..(value % 33) as usize];
        let actual = solar_evmir_interpreter::transact(
            evm_text,
            calldata,
            &mut EmptyHost,
            Default::default(),
        )
        .unwrap();
        let expected = alloy_primitives::U256::from(calldata.len()).to_be_bytes::<32>().to_vec();
        assert_eq!(actual.outcome, solar_evmir_interpreter::Outcome::Success(expected));
    }
}

#[test]
fn native_plugin_preserves_optimized_and_debug_bytecode() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("test.sol");
    fs::write(&source, "contract C { function f(uint256 x) external pure returns (uint256) { unchecked { return x + 0; } } }").unwrap();
    let sample = sample();
    for mode in ["none", "gas", "size"] {
        let baseline = Command::new(SOLAR)
            .args(["--allow=2264", "-O", mode, "--emit=bin,bin-runtime"])
            .arg(&source)
            .output()
            .unwrap();
        assert!(baseline.status.success());
        let compiled = run(&source, &sample, &["-O", mode, "--emit=bin,bin-runtime"]);
        assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
        let debug = run(
            &source,
            &sample,
            &["-O", mode, "--emit=bin,bin-runtime,srcmap-runtime,ethdebug-runtime"],
        );
        assert!(debug.status.success(), "{}", String::from_utf8_lossy(&debug.stderr));
        let mut annotated: Value = serde_json::from_slice(&debug.stdout).unwrap();
        let compiled: Value = serde_json::from_slice(&compiled.stdout).unwrap();
        for contract in annotated["contracts"].as_object_mut().unwrap().values_mut() {
            contract.as_object_mut().unwrap().remove("srcmap-runtime");
            contract.as_object_mut().unwrap().remove("ethdebug-runtime");
        }
        assert_eq!(
            annotated["contracts"], compiled["contracts"],
            "debug info changed bytecode at {mode}"
        );
        if mode != "none" {
            let baseline: Value = serde_json::from_slice(&baseline.stdout).unwrap();
            assert_eq!(
                baseline["contracts"], compiled["contracts"],
                "identity rewrite changed optimized bytecode"
            );
        }
    }
}

#[test]
fn plugin_loading_precedes_parsing() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("test.sol");
    fs::write(&source, "contract {").unwrap();
    let library = broken(directory.path(), "wrong_abi");
    let output = run(&source, &library, &["--emit=abi"]);
    assert!(!output.status.success());
    #[allow(clippy::disallowed_methods)]
    let canonical = library.canonicalize().unwrap();
    let diagnostics = diagnostic_messages(&output)
        .into_iter()
        .map(|message| message.replace(&canonical.display().to_string(), "$PLUGIN"))
        .collect::<Vec<_>>();
    assert_data_eq!(
        serde_json::to_string_pretty(&diagnostics).unwrap(),
        str![[r#"
[
  "pass plugin `$PLUGIN` was built with an incompatible Rust SDK or toolchain",
  "aborting due to 1 previous error"
]
"#]]
    );
}

#[test]
fn invalid_native_plugins_fail_cleanly() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("test.evmir");
    fs::write(&source, EVM).unwrap();
    let mut failures = Vec::new();
    for mode in [
        "missing",
        "missing_abi",
        "wrong_abi",
        "bad_version",
        "panic",
        "duplicate_response",
        "no_response",
        "callback_error",
        "invalid_ir",
        "wrong_fork",
        "invalid_opcode",
        "invalid_block",
    ] {
        let library = broken(directory.path(), mode);
        let output = run(
            &source,
            &library,
            &[
                "-Ogas",
                "--evm-version=paris",
                "-Zevm-ir-pipeline=broken::fail",
                "-Zvalidate-ir=false",
                "--emit=bin",
            ],
        );
        assert!(!output.status.success(), "{mode} unexpectedly succeeded");
        // The loader reports canonical native paths, including `/private` on macOS.
        #[allow(clippy::disallowed_methods)]
        let canonical = library.canonicalize().unwrap();
        let diagnostics = diagnostic_messages(&output)
            .into_iter()
            .map(|message| {
                message
                    .replace(&canonical.display().to_string(), "$PLUGIN")
                    .replace(&library.display().to_string(), "$PLUGIN")
            })
            .collect::<Vec<_>>();
        // Loader suffixes are platform-specific; the stable primary diagnosis names the symbol.
        let diagnostics = if mode == "missing" {
            diagnostics
                .iter()
                .map(|message| message.split(": ").next().unwrap().to_owned())
                .collect()
        } else {
            diagnostics
        };
        failures.push((mode, diagnostics));
    }
    assert_data_eq!(
        serde_json::to_string_pretty(&failures).unwrap(),
        str![[r#"
[
  [
    "missing",
    [
      "pass plugin `$PLUGIN` has no `solar_rust_pass_plugin_v1` entry point",
      "aborting due to 1 previous error"
    ]
  ],
  [
    "missing_abi",
    [
      "pass plugin `$PLUGIN` is missing the Rust SDK fingerprint",
      "aborting due to 1 previous error"
    ]
  ],
  [
    "wrong_abi",
    [
      "pass plugin `$PLUGIN` was built with an incompatible Rust SDK or toolchain",
      "aborting due to 1 previous error"
    ]
  ],
  [
    "bad_version",
    [
      "plugin `broken` requires API 999, expected 1",
      "aborting due to 1 previous error"
    ]
  ],
  [
    "panic",
    [
      "plugin pass `broken::fail` failed: plugin panicked",
      "aborting due to 1 previous error"
    ]
  ],
  [
    "duplicate_response",
    [
      "plugin pass `broken::fail` failed: Rust plugin returned more than one response",
      "aborting due to 1 previous error"
    ]
  ],
  [
    "no_response",
    [
      "plugin pass `broken::fail` failed: Rust plugin did not return a response",
      "aborting due to 1 previous error"
    ]
  ],
  [
    "callback_error",
    [
      "plugin pass `broken::fail` failed: intentional failure",
      "aborting due to 1 previous error"
    ]
  ],
  [
    "invalid_ir",
    [
      "EVM IR verification failed: block 0: `pop` on an empty stack",
      "plugin pass `broken::fail` failed: produced invalid EVM IR",
      "aborting due to 2 previous errors"
    ]
  ],
  [
    "wrong_fork",
    [
      "EVM IR verification failed: block 0: opcode `tload` is unavailable for `paris` EVM",
      "plugin pass `broken::fail` failed: produced invalid EVM IR",
      "aborting due to 2 previous errors"
    ]
  ],
  [
    "invalid_opcode",
    [
      "EVM IR verification failed: block 0: `dupn` must use the logical stack-op form",
      "EVM IR verification failed: block 0: instruction `dupn` has no known stack effect",
      "plugin pass `broken::fail` failed: produced invalid EVM IR",
      "aborting due to 3 previous errors"
    ]
  ],
  [
    "invalid_block",
    [
      "plugin pass `broken::fail` failed: unknown EVM IR block",
      "aborting due to 1 previous error"
    ]
  ]
]
"#]]
    );
}

#[test]
fn invalid_mir_edits_and_duplicate_names_are_errors() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("test.mir");
    fs::write(
        &source,
        "@module WrongType\nfn @f(arg0: i256, arg1: i1) -> i256 {\n bb0:\n ret arg0\n}\n",
    )
    .unwrap();
    let library = broken(directory.path(), "wrong_type");
    let output = run(
        &source,
        &library,
        &["-Ogas", "-Zmir-pipeline=broken::fail", "-Zvalidate-ir=false", "--emit=bin"],
    );
    assert!(!output.status.success());
    assert_data_eq!(
        messages(&output),
        str![[r#"
[
  "plugin pass `broken::fail` failed: replacement SSA value has a different type",
  "aborting due to 1 previous error"
]
"#]]
    );
    let library = broken(directory.path(), "invalid_function");
    let output = run(
        &source,
        &library,
        &["-Ogas", "-Zmir-pipeline=broken::fail", "-Zvalidate-ir=false", "--emit=bin"],
    );
    assert!(!output.status.success());
    assert_data_eq!(
        messages(&output),
        str![[r#"
[
  "plugin pass `broken::fail` failed: unknown MIR function",
  "aborting due to 1 previous error"
]
"#]]
    );
    fs::write(&source, MIR).unwrap();
    let sample = sample();
    let repeated = format!("-Zload-pass-plugin={}", sample.display());
    let output = run(&source, &sample, &["-Ogas", &repeated]);
    assert!(!output.status.success());
    assert_data_eq!(
        messages(&output),
        str![[r#"
[
  "duplicate plugin namespace `example`",
  "aborting due to 1 previous error"
]
"#]]
    );
    let output = run(&source, &sample, &["-Ogas", "-Zmir-pipeline=example::simplify-add-zero"]);
    assert!(!output.status.success());
    assert_data_eq!(
        messages(&output),
        str![[r#"
[
  "plugin pass `example::simplify-add-zero` failed: requires semantic MIR, got lowered",
  "aborting due to 1 previous error"
]
"#]]
    );
}

#[test]
fn plugin_option_survives_standard_json_settings() {
    let sample = sample();
    let output = Command::new(SOLAR)
        .args(["--standard-json", "--allow=2264"])
        .arg(format!("-Zload-pass-plugin={}", sample.display()))
        .arg(format!("-Zload-pass-plugin={}", sample.display()))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut child = output;
    std::io::Write::write_all(
        child.stdin.as_mut().unwrap(),
        json!({
            "language":"Solidity", "sources":{"C.sol":{"content":"contract C {}"}},
            "settings":{"outputSelection":{"*":{"*":["evm.bytecode.object"]}}}
        })
        .to_string()
        .as_bytes(),
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    let errors = response["errors"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|error| error["severity"] == "error")
        .map(|error| error["message"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_data_eq!(
        serde_json::to_string_pretty(&errors).unwrap(),
        str![[r#"
[
  "duplicate plugin namespace `example`"
]
"#]]
    );
}

#[test]
fn standard_json_cannot_load_native_code() {
    let input = json!({
        "language": "Solidity", "sources": {"C.sol": {"content": "contract C {}"}},
        "settings": {
            "outputSelection": {"*": {"*": ["evm.bytecode.object"]}},
            "solar": {"unstable": ["load-pass-plugin=not-a-real-library"]}
        }
    });
    let mut child = Command::new(SOLAR)
        .args(["--standard-json", "--allow=2264"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    std::io::Write::write_all(child.stdin.as_mut().unwrap(), input.to_string().as_bytes()).unwrap();
    let output = child.wait_with_output().unwrap();
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    let errors = response["errors"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|error| error["severity"] == "error")
        .map(|error| error["message"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_data_eq!(
        serde_json::to_string_pretty(&errors).unwrap(),
        str![[r#"
[
  "pass plugins must be loaded through the command line"
]
"#]]
    );
}

#[test]
fn example_passes_keep_gas_observations() {
    let directory = tempfile::tempdir().unwrap();
    let mir = MIR.replace("calldataload 0", "gas");
    let evm = EVM.replace("calldatasize", "gas");
    let mut results = Vec::new();
    for (extension, source, pipeline) in [
        ("mir", mir, "-Zmir-pipeline=example::simplify-lowered"),
        ("evmir", evm, "-Zevm-ir-pipeline=example::remove-double-not"),
    ] {
        let path = directory.path().join(format!("test.{extension}"));
        fs::write(&path, source).unwrap();
        let output = run(&path, &sample(), &["-Ogas", pipeline, "-Zprint-after-each"]);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        results.push(
            String::from_utf8(output.stdout)
                .unwrap()
                .replace(&path.display().to_string(), "$INPUT"),
        );
    }
    assert_data_eq!(
        results.join("\n"),
        str![[r#"
; === $INPUT (after example::simplify-lowered) ===
@module PluginTest
@phase lowered
; runtime dispatch entry
fn @entry() [entry] {
  bb0:
    v0 = gas
    v1 = add v0, 0
    mstore 0, v1
    returndata 0, 32
}

; === $INPUT (after example::remove-double-not) ===
@module PluginTest
bb0:
  gas
  not
  not
  push 0
  mstore
  push 32
  push 0
  return

"#]]
    );
}
