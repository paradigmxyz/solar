//! CLI debug-output selection, cross-interface parity, and bytecode neutrality.

use serde_json::{Value, json};
use solar::interface::source_map::{FileLoader, RealFileLoader};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

const SOLAR: &str = env!("CARGO_BIN_EXE_solar");
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/ui/cli/auxiliary");
const SOURCE: &str = "debug_output.sol";
const CONTRACT: &str = "debug_output.sol:C";
const DEBUG_OUTPUTS: &[&str] = &["ethdebug", "ethdebug-runtime", "srcmap", "srcmap-runtime"];

fn compile(args: &[&str]) -> Output {
    let output = Command::new(SOLAR)
        .current_dir(FIXTURES)
        .args(["--allow", "2264", "--threads", "1", "--evm-version", "cancun"])
        .args(args)
        .output()
        .expect("run compiler");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    output
}

fn compile_json(args: &[&str]) -> Value {
    serde_json::from_slice(&compile(args).stdout).expect("parse compiler JSON")
}

#[test]
fn debug_output_selection_and_bytecode_neutrality() {
    for mode in ["none", "gas", "size"] {
        let baseline = compile_json(&[SOURCE, "-O", mode, "--emit=bin,bin-runtime"]);
        assert!(baseline["contracts"][CONTRACT]["bin"].as_str().is_some_and(|s| !s.is_empty()));
        assert!(baseline.get("ethdebug").is_none());
        assert!(baseline.get("sourceList").is_none());
        for selection in
            DEBUG_OUTPUTS.iter().copied().chain(["ethdebug,ethdebug-runtime,srcmap,srcmap-runtime"])
        {
            let emit = format!("--emit=bin,bin-runtime,{selection}");
            let mut output = compile_json(&[SOURCE, "-O", mode, &emit]);
            for contract in output["contracts"].as_object_mut().unwrap().values_mut() {
                for name in DEBUG_OUTPUTS {
                    let artifact = contract.as_object_mut().unwrap().remove(*name);
                    assert_eq!(artifact.is_some(), selection.split(',').any(|s| s == *name));
                }
            }
            assert_eq!(output["contracts"], baseline["contracts"], "{mode}: {selection}");
            assert_eq!(output.get("ethdebug").is_some(), selection.contains("ethdebug"));
            assert_eq!(output.get("sourceList").is_some(), selection.contains("srcmap"));
        }
    }
}

#[test]
fn debug_outputs_match_standard_json() {
    for (mode, runs) in [("none", 200), ("gas", 200), ("size", 1)] {
        let output = compile_json(&[
            SOURCE,
            "-O",
            mode,
            "--emit=ethdebug,ethdebug-runtime,srcmap,srcmap-runtime",
        ]);
        let dir = tempfile::tempdir().unwrap();
        let sources = [SOURCE, "debug_output_helper.sol"]
            .into_iter()
            .map(|name| {
                (
                    name.to_owned(),
                    json!({"content": RealFileLoader.load_file(&Path::new(FIXTURES).join(name)).unwrap()}),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        let input = json!({
            "language": "Solidity",
            "sources": sources,
            "settings": {
                "evmVersion": "cancun",
                "metadata": {"appendCBOR": false, "bytecodeHash": "none"},
                "optimizer": {"enabled": mode != "none", "runs": runs},
                "outputSelection": {"*": {"*": [
                    "evm.bytecode.ethdebug", "evm.deployedBytecode.ethdebug",
                    "evm.bytecode.sourceMap", "evm.deployedBytecode.sourceMap",
                    "ethdebug.resources"
                ]}}
            }
        });
        let path = dir.path().join("input.json");
        fs::write(&path, serde_json::to_vec(&input).unwrap()).unwrap();
        let mut standard = compile_json(&["--standard-json", path.to_str().unwrap()]);
        assert!(
            standard["errors"]
                .as_array()
                .is_none_or(|errors| { errors.iter().all(|error| error["severity"] != "error") }),
            "{standard}"
        );
        // Standard JSON records an explicit optimizer run count, whereas the
        // CLI uses the objective's default. Check each reference before removing
        // this expected compilation-identity difference from the comparison.
        let cli_id = &output["ethdebug"]["compilation"]["id"];
        let standard_id = standard["ethdebug"]["resources"]["compilation"]["id"].clone();
        assert!(cli_id.as_str().is_some_and(|id| !id.is_empty()));
        assert!(standard_id.as_str().is_some_and(|id| !id.is_empty()));
        for (name, field) in [("ethdebug", "bytecode"), ("ethdebug-runtime", "deployedBytecode")] {
            assert_eq!(output["contracts"][CONTRACT][name]["compilation"]["id"], *cli_id);
            let program = &mut standard["contracts"][SOURCE]["C"]["evm"][field]["ethdebug"];
            assert_eq!(program["compilation"]["id"], standard_id);
            program["compilation"]["id"] = cli_id.clone();
        }
        standard["ethdebug"]["resources"]["compilation"]["id"] = cli_id.clone();
        let cli = &output["contracts"][CONTRACT];
        let evm = &standard["contracts"][SOURCE]["C"]["evm"];
        assert_eq!(cli["ethdebug"], evm["bytecode"]["ethdebug"], "{mode}");
        assert_eq!(cli["ethdebug-runtime"], evm["deployedBytecode"]["ethdebug"], "{mode}");
        assert_eq!(cli["srcmap"], evm["bytecode"]["sourceMap"], "{mode}");
        assert_eq!(cli["srcmap-runtime"], evm["deployedBytecode"]["sourceMap"], "{mode}");
        assert_eq!(output["ethdebug"], standard["ethdebug"]["resources"], "{mode}");
        let sources = output["sourceList"].as_array().unwrap();
        assert_eq!(sources.len(), 2);
        for (id, source) in sources.iter().enumerate() {
            assert_eq!(standard["sources"][source.as_str().unwrap()]["id"], id);
            assert_eq!(output["ethdebug"]["compilation"]["sources"][id]["path"], *source);
        }
    }
}

#[test]
fn debug_output_directory_and_resources_only() {
    let dir = tempfile::tempdir().unwrap();
    let args = [SOURCE, "--emit=ethdebug-runtime,srcmap-runtime", "--pretty-json"];
    let stdout = compile(&args);
    let mut args = args.to_vec();
    args.extend(["--out-dir", dir.path().to_str().unwrap()]);
    assert!(compile(&args).stdout.is_empty());
    assert_eq!(
        RealFileLoader.load_binary_file(&dir.path().join("combined.json")).unwrap(),
        stdout.stdout
    );

    let resources = compile_json(&[SOURCE, "--emit=ethdebug-resources"]);
    assert!(resources.get("contracts").is_none());
    assert!(resources.get("sourceList").is_none());
    let resources = &resources["ethdebug"];
    assert_eq!(resources["compilation"]["sources"].as_array().unwrap().len(), 2);
    assert!(resources["compilation"]["sources"].as_array().unwrap().iter().all(|source| {
        let path = Path::new(FIXTURES).join(source["path"].as_str().unwrap());
        source["contents"] == RealFileLoader.load_file(&path).unwrap()
    }));
    assert!(resources["types"].as_object().unwrap().is_empty());
    assert!(resources["pointers"].as_object().unwrap().is_empty());
}
