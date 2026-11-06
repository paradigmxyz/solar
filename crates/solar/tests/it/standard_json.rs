//! Standard JSON source unit names.

use serde_json::{Value, json};
use solar::{
    cli::standard_json::{ReadCallbackResult, StandardJsonReadCallback, compile_standard_json},
    config::CompileOpts,
};
use solar_interface::source_map::{FileLoader, RealFileLoader};
use std::{
    path::Path,
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

/// Source keys are opaque source unit names: `--base-path` must not shorten them, or the output
/// selection for the exact key matches nothing.
#[test]
fn base_path_keeps_source_keys() {
    let outside = std::env::temp_dir();
    let inside = std::env::current_dir().unwrap().join("src");
    let absolute_key = outside.join("Y.sol").display().to_string().replace('\\', "/");
    for (base_path, key) in [(outside, absolute_key.as_str()), (inside, "src/Y.sol")] {
        let input = json!({
            "language": "Solidity",
            "sources": {key: {"content": "contract Y {}"}},
            "settings": {"outputSelection": {key: {"Y": ["abi"]}}}
        });
        let opts = CompileOpts { base_path: Some(base_path), ..Default::default() };
        let mut output = Vec::new();
        compile_standard_json(&input.to_string(), opts, None, &mut output).unwrap();
        let output = serde_json::from_slice::<Value>(&output).unwrap();
        assert_eq!(
            output,
            json!({
                "sources": {key: {"id": 0}},
                "contracts": {key: {"Y": {"abi": []}}}
            })
        );
    }
}

/// Files loaded through the read callback are named by the source unit name the callback receives,
/// not by a path joined with the current directory.
#[test]
fn callback_imports_keep_source_unit_names() {
    struct Callback;

    impl StandardJsonReadCallback for Callback {
        fn read(&self, kind: &str, data: &str) -> ReadCallbackResult {
            assert_eq!(kind, "source");
            ReadCallbackResult::Success(match data {
                "src/dep.sol" => "import \"../lib/Lib.sol\"; contract D {}".to_string(),
                "lib/Lib.sol" => "contract L {}".to_string(),
                _ => panic!("unexpected callback path `{data}`"),
            })
        }
    }

    let input = json!({
        "language": "Solidity",
        "sources": {"src/Main.sol": {"content": "import \"./dep.sol\"; import \"lib/Lib.sol\"; contract M {}"}},
        "settings": {"outputSelection": {"src/Main.sol": {"M": ["metadata"]}}}
    });
    let mut output = Vec::new();
    compile_standard_json(
        &input.to_string(),
        CompileOpts::default(),
        Some(Arc::new(Callback)),
        &mut output,
    )
    .unwrap();
    let output = serde_json::from_slice::<Value>(&output).unwrap();
    let mut names = output["sources"].as_object().unwrap().keys().collect::<Vec<_>>();
    names.sort();
    assert_eq!(names, ["lib/Lib.sol", "src/Main.sol", "src/dep.sol"]);
    let metadata = output["contracts"]["src/Main.sol"]["M"]["metadata"].as_str().unwrap();
    let metadata = serde_json::from_str::<Value>(metadata).unwrap();
    let names = metadata["sources"].as_object().unwrap().keys().collect::<Vec<_>>();
    assert_eq!(names, ["lib/Lib.sol", "src/Main.sol", "src/dep.sol"]);
}

/// On Windows, input keys keep their backslashes, while imports the resolver builds from native
/// paths use the `/` name the read callback receives.
#[cfg(windows)]
#[test]
fn windows_backslash_keys_and_callback_imports() {
    struct Callback;

    impl StandardJsonReadCallback for Callback {
        fn read(&self, kind: &str, data: &str) -> ReadCallbackResult {
            assert_eq!(kind, "source");
            assert_eq!(data, "contracts/B.sol");
            ReadCallbackResult::Success("contract B {}".to_string())
        }
    }

    let input = json!({
        "language": "Solidity",
        "sources": {"contracts\\A.sol": {"content": "import \"./B.sol\"; contract A is B {}"}},
        "settings": {"outputSelection": {"contracts\\A.sol": {"A": ["abi"]}}}
    });
    let mut output = Vec::new();
    compile_standard_json(
        &input.to_string(),
        CompileOpts::default(),
        Some(Arc::new(Callback)),
        &mut output,
    )
    .unwrap();
    let output = serde_json::from_slice::<Value>(&output).unwrap();
    assert_eq!(
        output,
        json!({
            "sources": {"contracts\\A.sol": {"id": 1}, "contracts/B.sol": {"id": 0}},
            "contracts": {"contracts\\A.sol": {"A": {"abi": []}}}
        })
    );
}

/// A leading `..` is part of a source unit name, so `../shared/X.sol` is not the source
/// `shared/X.sol`.
#[test]
fn leading_parent_segments_keep_source_unit_names() {
    let input = json!({
        "language": "Solidity",
        "sources": {
            "src/A.sol": {"content": "import \"shared/X.sol\"; contract A {}"},
            "shared/X.sol": {"content": "contract X {}"}
        },
        "settings": {"remappings": ["shared/=../shared/"], "outputSelection": {}}
    });
    let mut output = Vec::new();
    compile_standard_json(&input.to_string(), CompileOpts::default(), None, &mut output).unwrap();
    let output = serde_json::from_slice::<Value>(&output).unwrap();
    assert_eq!(
        output["errors"][0]["message"],
        "couldn't read ../shared/X.sol: File import callback not supported"
    );
}

/// The read callback receives a path in the base path, and the file keeps its source unit name.
#[test]
fn callback_imports_with_base_path() {
    struct Callback;

    impl StandardJsonReadCallback for Callback {
        fn read(&self, kind: &str, data: &str) -> ReadCallbackResult {
            assert_eq!((kind, data), ("source", "lib/X.sol"));
            ReadCallbackResult::Success("contract X {}".to_string())
        }
    }

    let input = json!({
        "language": "Solidity",
        "sources": {"A.sol": {"content": "import \"X.sol\"; contract A {}"}},
        "settings": {"outputSelection": {}}
    });
    let opts = CompileOpts { base_path: Some("lib".into()), ..Default::default() };
    let mut output = Vec::new();
    compile_standard_json(&input.to_string(), opts, Some(Arc::new(Callback)), &mut output).unwrap();
    let output = serde_json::from_slice::<Value>(&output).unwrap();
    assert_eq!(output, json!({"sources": {"A.sol": {"id": 1}, "X.sol": {"id": 0}}}));
}

/// The read callback gets a `file://` import with the prefix when there is no base path or include
/// path, like libsolc, and without it otherwise, like solc's file reader. The name keeps it.
#[test]
fn callback_imports_file_urls() {
    struct Callback(&'static str);

    impl StandardJsonReadCallback for Callback {
        fn read(&self, kind: &str, data: &str) -> ReadCallbackResult {
            assert_eq!(kind, "source");
            if data == self.0 {
                ReadCallbackResult::Success("contract X {}".to_string())
            } else {
                ReadCallbackResult::Error(format!("`{data}` not found"))
            }
        }
    }

    for (import, base_path, include_paths, data) in [
        ("file://X.sol", None, vec![], "file://X.sol"),
        ("file://X.sol", Some("lib".into()), vec![], "lib/X.sol"),
        ("file://X.sol", None, vec!["inc".into()], "inc/X.sol"),
        ("file:///abs/X.sol", None, vec!["inc".into()], "/abs/X.sol"),
    ] {
        let input = json!({
            "language": "Solidity",
            "sources": {"A.sol": {"content": format!("import \"{import}\"; contract A {{}}")}},
            "settings": {"outputSelection": {}}
        });
        let opts = CompileOpts { base_path, include_paths, ..Default::default() };
        let mut output = Vec::new();
        let callback = Arc::new(Callback(data));
        compile_standard_json(&input.to_string(), opts, Some(callback), &mut output).unwrap();
        let output = serde_json::from_slice::<Value>(&output).unwrap();
        assert_eq!(output, json!({"sources": {"A.sol": {"id": 1}, import: {"id": 0}}}), "{import}");
    }
}

/// The read callback is asked for every root: a file that only an include path serves resolves,
/// and one that several roots serve is ambiguous.
#[test]
fn callback_imports_search_include_paths() {
    struct Callback(&'static [&'static str]);

    impl StandardJsonReadCallback for Callback {
        fn read(&self, _kind: &str, data: &str) -> ReadCallbackResult {
            if self.0.contains(&data) {
                ReadCallbackResult::Success("contract X {}".to_string())
            } else {
                ReadCallbackResult::Error(format!("`{data}` not found"))
            }
        }
    }

    let input = json!({
        "language": "Solidity",
        "sources": {"A.sol": {"content": "import \"X.sol\"; contract A {}"}},
        "settings": {"outputSelection": {}}
    })
    .to_string();
    let compile = |served| {
        let opts = CompileOpts {
            base_path: Some("lib".into()),
            include_paths: vec!["inc".into()],
            ..Default::default()
        };
        let mut output = Vec::new();
        compile_standard_json(&input, opts, Some(Arc::new(Callback(served))), &mut output).unwrap();
        serde_json::from_slice::<Value>(&output).unwrap()
    };
    assert_eq!(
        compile(&["inc/X.sol"]),
        json!({"sources": {"A.sol": {"id": 1}, "X.sol": {"id": 0}}})
    );
    let output = compile(&["lib/X.sol", "inc/X.sol"]);
    let message = output["errors"][0]["message"].as_str().unwrap();
    assert!(message.starts_with("multiple files match X.sol: "), "{message}");
}

/// Files read through the callback keep their exact source unit names, a repeated include path is
/// not ambiguous, and sources named like a root-relative path are not mistaken for matches.
#[test]
fn callback_imports_keep_exact_names() {
    struct Callback;

    impl StandardJsonReadCallback for Callback {
        fn read(&self, _kind: &str, data: &str) -> ReadCallbackResult {
            match data {
                "https://example.com/B.sol" => ReadCallbackResult::Success("contract B {}".into()),
                "inc/X.sol" => ReadCallbackResult::Success("contract X {}".into()),
                _ => ReadCallbackResult::Error(format!("`{data}` not found")),
            }
        }
    }

    let input = json!({
        "language": "Solidity",
        "sources": {
            "A.sol": {"content": "import \"https://example.com/B.sol\"; import \"X.sol\"; contract A {}"},
            "sub/X.sol": {"content": "contract Y {}"}
        },
        "settings": {"outputSelection": {}}
    });
    let opts = CompileOpts {
        include_paths: vec!["inc".into(), "inc".into(), "sub".into()],
        ..Default::default()
    };
    let mut output = Vec::new();
    compile_standard_json(&input.to_string(), opts, Some(Arc::new(Callback)), &mut output).unwrap();
    let output = serde_json::from_slice::<Value>(&output).unwrap();
    let mut names = output["sources"].as_object().unwrap().keys().collect::<Vec<_>>();
    names.sort();
    assert_eq!(names, ["A.sol", "X.sol", "https://example.com/B.sol", "sub/X.sol"], "{output}");
}

/// Spellings of a callback source unit name that differ only in `.` and `..` segments or repeated
/// separators name one source with a normalized name, and the callback reads it once.
#[test]
fn callback_imports_merge_spellings() {
    #[derive(Default)]
    struct Callback(AtomicUsize);

    impl StandardJsonReadCallback for Callback {
        fn read(&self, _kind: &str, data: &str) -> ReadCallbackResult {
            self.0.fetch_add(1, Ordering::Relaxed);
            ReadCallbackResult::Success(format!("// {data}\ncontract X {{}}"))
        }
    }

    let sources = [
        "lib//X.sol",
        "lib/X.sol",
        "lib/./X.sol",
        "lib/sub/../X.sol",
        "/abs/./x//X.sol",
        "/abs/y/../x/X.sol",
    ]
    .iter()
    .enumerate()
    .map(|(i, import)| {
        (
            format!("S{i}.sol"),
            json!({"content": format!("import \"{import}\"; contract S{i} {{}}")}),
        )
    })
    .collect::<serde_json::Map<_, _>>();
    let input =
        json!({"language": "Solidity", "sources": sources, "settings": {"outputSelection": {}}});
    let callback = Arc::new(Callback::default());
    let mut output = Vec::new();
    compile_standard_json(
        &input.to_string(),
        CompileOpts::default(),
        Some(callback.clone()),
        &mut output,
    )
    .unwrap();
    let output = serde_json::from_slice::<Value>(&output).unwrap();
    let mut names = output["sources"].as_object().unwrap().keys().collect::<Vec<_>>();
    names.sort();
    assert_eq!(
        names,
        ["/abs/x/X.sol", "S0.sol", "S1.sol", "S2.sol", "S3.sol", "S4.sol", "S5.sol", "lib/X.sol"],
        "{output}"
    );
    assert_eq!(callback.0.load(Ordering::Relaxed), 2);
}

/// Relative imports join the importing source unit name, which the base path does not shorten.
#[test]
fn relative_imports_with_base_path() {
    let input = json!({
        "language": "Solidity",
        "sources": {
            "/project/A.sol": {"content": "import \"./B.sol\"; contract A is B {}"},
            "/project/B.sol": {"content": "contract B {}"}
        },
        "settings": {"outputSelection": {}}
    });
    let opts = CompileOpts { base_path: Some("/project".into()), ..Default::default() };
    let mut output = Vec::new();
    compile_standard_json(&input.to_string(), opts, None, &mut output).unwrap();
    let output = serde_json::from_slice::<Value>(&output).unwrap();
    assert_eq!(
        output,
        json!({"sources": {"/project/A.sol": {"id": 1}, "/project/B.sol": {"id": 0}}})
    );
}

/// Compact output serializes contracts in parallel; it must keep the source and contract order of
/// the output.
#[test]
fn compact_output_keeps_contract_order() {
    let input = json!({
        "language": "Solidity",
        "sources": {
            "B.sol": {"content": "contract Z { function z() external {} } contract A { function a() external {} }"},
            "A.sol": {"content": "contract M { function m() external {} }"}
        },
        "settings": {"outputSelection": {"*": {"*": ["abi"]}}}
    });
    let mut output = Vec::new();
    compile_standard_json(&input.to_string(), CompileOpts::default(), None, &mut output).unwrap();
    assert_eq!(
        String::from_utf8(output).unwrap(),
        concat!(
            r#"{"sources":{"A.sol":{"id":0},"B.sol":{"id":1}},"contracts":{"#,
            r#""A.sol":{"M":{"abi":[{"type":"function","name":"m","inputs":[],"outputs":[],"stateMutability":"nonpayable"}]}},"#,
            r#""B.sol":{"Z":{"abi":[{"type":"function","name":"z","inputs":[],"outputs":[],"stateMutability":"nonpayable"}]},"#,
            r#""A":{"abi":[{"type":"function","name":"a","inputs":[],"outputs":[],"stateMutability":"nonpayable"}]}}}}"#,
        )
    );
}

/// Reads extra sources through the same filesystem callback the CLI installs.
struct Disk;

impl StandardJsonReadCallback for Disk {
    fn read(&self, kind: &str, data: &str) -> ReadCallbackResult {
        assert_eq!(kind, "source");
        match RealFileLoader.load_file(Path::new(data)) {
            Ok(contents) => ReadCallbackResult::Success(contents),
            Err(error) => ReadCallbackResult::Error(error.to_string()),
        }
    }
}

/// A Foundry monorepo whose `lib/` sits above the package: only the package source is in the
/// Standard JSON, remappings point at `../../lib`, and a dependency uses a relative import.
/// https://github.com/paradigmxyz/solar/issues/2455
fn monorepo_outside_package_root(relative_remap: bool) -> (tempfile::TempDir, String, String) {
    let dir = tempfile::tempdir().unwrap();
    let lib = dir.path().join("lib/oz/contracts");
    std::fs::create_dir_all(lib.join("access")).unwrap();
    std::fs::create_dir_all(lib.join("utils")).unwrap();
    std::fs::write(lib.join("utils/Context.sol"), "contract Context {}").unwrap();
    std::fs::write(
        lib.join("access/Ownable.sol"),
        "import \"../utils/Context.sol\"; abstract contract Ownable is Context {}",
    )
    .unwrap();
    let package = dir.path().join("packages/app");
    std::fs::create_dir_all(package.join("src")).unwrap();
    let app = "import \"@openzeppelin/contracts/access/Ownable.sol\"; contract App is Ownable {}";
    std::fs::write(package.join("src/App.sol"), app).unwrap();
    let remapping = if relative_remap {
        // Foundry writes remappings relative to the package, as in issue 2455.
        "@openzeppelin/contracts/=../../lib/oz/contracts/".to_string()
    } else {
        format!("@openzeppelin/contracts/={}/", lib.display().to_string().replace('\\', "/"))
    };
    let input = json!({
        "language": "Solidity",
        "sources": {"src/App.sol": {"content": app}},
        "settings": {
            "remappings": [remapping],
            "outputSelection": {"*": {"*": ["abi"]}}
        }
    })
    .to_string();
    (dir, package.display().to_string(), input)
}

/// Library callers without a callback stay sandboxed. The CLI installs a filesystem
/// callback so the same input compiles, like solc.
#[test]
fn disk_imports_need_a_filesystem_callback() {
    let (_dir, _package, input) = monorepo_outside_package_root(false);
    let mut sandboxed = Vec::new();
    compile_standard_json(&input, CompileOpts::default(), None, &mut sandboxed).unwrap();
    let sandboxed = serde_json::from_slice::<Value>(&sandboxed).unwrap();
    let message = sandboxed["errors"][0]["message"].as_str().unwrap();
    assert!(message.contains("File import callback not supported"), "{message}");

    let mut loaded = Vec::new();
    compile_standard_json(&input, CompileOpts::default(), Some(Arc::new(Disk)), &mut loaded)
        .unwrap();
    let loaded = serde_json::from_slice::<Value>(&loaded).unwrap();
    assert!(loaded["errors"].is_null(), "{loaded}");
    assert!(loaded["contracts"]["src/App.sol"]["App"]["abi"].is_array(), "{loaded}");
}

/// `solar --standard-json` reads dependency files that sit outside the package root.
#[test]
fn cli_standard_json_reads_imports_outside_the_package() {
    let (_dir, package, input) = monorepo_outside_package_root(true);
    let json_path = Path::new(&package).join("input.json");
    std::fs::write(&json_path, &input).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_solar"))
        .current_dir(&package)
        .args(["--standard-json", "input.json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let output = serde_json::from_slice::<Value>(&output.stdout).unwrap();
    assert!(output["errors"].is_null(), "{output}");
    assert!(output["contracts"]["src/App.sol"]["App"]["abi"].is_array(), "{output}");
}
