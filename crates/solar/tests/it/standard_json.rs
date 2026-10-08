//! Standard JSON source unit names.

use serde_json::{Value, json};
use solar::{cli::standard_json::compile_standard_json, config::CompileOpts};

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
