//! Standard JSON source unit names.

use serde_json::{Value, json};
use solar::{
    cli::standard_json::{ReadCallbackResult, StandardJsonReadCallback, compile_standard_json},
    config::CompileOpts,
};
use std::sync::Arc;

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
/// as in solc, not by a path joined with the current directory.
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
