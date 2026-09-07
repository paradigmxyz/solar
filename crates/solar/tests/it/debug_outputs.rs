//! CLI debug-output selection, cross-interface parity, and bytecode neutrality.

use serde_json::{Value, json};
use solar::{
    config::CompileOpts,
    interface::source_map::{FileLoader, RealFileLoader},
};
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
        // Standard JSON records an explicit optimizer run count and metadata
        // settings, whereas the CLI uses its defaults without a CBOR trailer.
        // Check references before removing this identity difference.
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

#[test]
fn standard_json_api_ignores_cli_outputs() {
    let input = json!({
        "language": "Solidity",
        "sources": {"C.sol": {"content": "contract C {}"}},
        "settings": {"outputSelection": {"*": {"*": ["abi"]}}}
    });
    for selection in DEBUG_OUTPUTS.iter().copied().chain(["abi", "ethdebug-resources"]) {
        let dir = tempfile::tempdir().unwrap();
        let opts = CompileOpts {
            emit: vec![selection.parse().unwrap()],
            out_dir: Some(dir.path().to_owned()),
            ..Default::default()
        };
        let mut output = Vec::new();
        solar::cli::standard_json::compile_standard_json(
            &input.to_string(),
            opts,
            None,
            &mut output,
        )
        .unwrap();
        let output = serde_json::from_slice::<Value>(&output).unwrap();
        assert_eq!(output["contracts"]["C.sol"]["C"], json!({"abi": []}));
        assert!(output.get("ethdebug").is_none());
        assert!(!dir.path().join("combined.json").exists(), "{selection}");
    }
}

#[test]
fn compilation_identity_covers_revert_strings_and_metadata() {
    let mut identities = Vec::new();
    let mut bytecodes = Vec::new();
    for revert_strings in ["default", "strip", "debug"] {
        let output = compile_json(&[
            "debug_transfers.sol",
            "--revert-strings",
            revert_strings,
            "--emit=bin-runtime,ethdebug-runtime",
        ]);
        identities.push(output["ethdebug"]["compilation"]["id"].clone());
        bytecodes.push(output["contracts"]["debug_transfers.sol:Transfers"]["bin-runtime"].clone());
    }
    for a in 0..identities.len() {
        for b in a + 1..identities.len() {
            assert_ne!(bytecodes[a], bytecodes[b]);
            assert_ne!(identities[a], identities[b]);
        }
    }

    // Resources-only requests must name the same effective compilation as
    // program requests, including settings outside CompileOpts.
    let mut identities = Vec::new();
    for metadata in [
        json!({"appendCBOR": false, "bytecodeHash": "none"}),
        json!({"bytecodeHash": "none"}),
        json!({"bytecodeHash": "ipfs"}),
        json!({"bytecodeHash": "ipfs", "useLiteralContent": true}),
    ] {
        let mut ids = Vec::new();
        for selectors in [json!(["ethdebug.resources"]), json!(["evm.bytecode.ethdebug"])] {
            let input = json!({
                "language": "Solidity",
                "sources": {"C.sol": {"content": "contract C {}"}},
                "settings": {"metadata": metadata, "outputSelection": {"*": {"*": selectors}}}
            });
            let mut bytes = Vec::new();
            solar::cli::standard_json::compile_standard_json(
                &input.to_string(),
                CompileOpts::default(),
                None,
                &mut bytes,
            )
            .unwrap();
            let output = serde_json::from_slice::<Value>(&bytes).unwrap();
            let id = &output["ethdebug"]["compilation"]["id"];
            assert!(id.as_str().is_some());
            ids.push(id.clone());
        }
        assert_eq!(ids[0], ids[1]);
        assert!(!identities.contains(&ids[0]));
        identities.push(ids.remove(0));
    }
}

#[test]
fn compilation_identity_preserves_library_precedence() {
    let a = "L=0x1111111111111111111111111111111111111111";
    let b = "L=0x2222222222222222222222222222222222222222";
    let outputs = [format!("{a},{b}"), format!("{b},{a}")].map(|libraries| {
        compile_json(&[
            "debug_library.sol",
            "--emit=bin-runtime,ethdebug-runtime",
            "--libraries",
            &libraries,
        ])
    });
    assert_ne!(
        outputs[0]["contracts"]["debug_library.sol:C"]["bin-runtime"],
        outputs[1]["contracts"]["debug_library.sol:C"]["bin-runtime"],
    );
    assert_ne!(
        outputs[0]["ethdebug"]["compilation"]["id"],
        outputs[1]["ethdebug"]["compilation"]["id"]
    );
}

#[test]
fn debug_activations_require_real_transfers() {
    for mode in ["none", "gas", "size"] {
        let source = "debug_transfers.sol";
        let baseline = compile_json(&[source, "-O", mode, "--emit=bin,bin-runtime"]);
        let output = compile_json(&[
            source,
            "-O",
            mode,
            "--emit=bin,bin-runtime,ethdebug,ethdebug-runtime,srcmap,srcmap-runtime",
        ]);
        let contract = &output["contracts"]["debug_transfers.sol:Transfers"];
        for (program, source_map, bytecode) in
            [("ethdebug", "srcmap", "bin"), ("ethdebug-runtime", "srcmap-runtime", "bin-runtime")]
        {
            assert_eq!(
                contract[bytecode],
                baseline["contracts"]["debug_transfers.sol:Transfers"][bytecode]
            );
            let instructions = contract[program]["instructions"].as_array().unwrap();
            let entries = contract[source_map].as_str().unwrap().split(';').collect::<Vec<_>>();
            assert_eq!(instructions.len(), entries.len());
            let mut jump = "-";
            let mut internal_returns = 0;
            let mut returns = 0;
            for (index, (instruction, entry)) in instructions.iter().zip(entries).enumerate() {
                let opcode = instruction["operation"]["mnemonic"].as_str().unwrap();
                if let Some(marker) = entry.split(':').nth(3).filter(|marker| !marker.is_empty()) {
                    jump = marker;
                }
                if !matches!(opcode, "JUMP" | "JUMPI") {
                    assert_eq!(jump, "-", "{mode} {program}: {instruction}");
                }
                internal_returns += usize::from(jump == "o");
                let context = &instruction["context"];
                if let Some(invoke) = context.get("invoke") {
                    assert!(matches!(opcode, "JUMP" | "JUMPI"), "{invoke}");
                    assert_eq!(invoke["jump"], true);
                    let pointer = &invoke["target"]["pointer"];
                    assert_eq!(pointer["location"], "code");
                    assert_eq!(pointer["length"], 1);
                    let target = pointer["offset"].as_u64().expect("required internal jump target");
                    assert!(instructions.iter().any(|inst| {
                        inst["offset"] == target && inst["operation"]["mnemonic"] == "JUMPDEST"
                    }));
                    let argument =
                        instructions[index - 1]["operation"]["arguments"][0].as_str().unwrap();
                    assert_eq!(
                        u64::from_str_radix(argument.strip_prefix("0x").unwrap(), 16).unwrap(),
                        target
                    );
                }
                if context.get("return").is_some() {
                    returns += 1;
                    assert!(
                        matches!(opcode, "STOP" | "JUMP" | "JUMPI" | "RETURN"),
                        "{instruction}"
                    );
                }
                if context.get("revert").is_some() {
                    assert_eq!(opcode, "REVERT");
                }
            }
            if program == "ethdebug-runtime" {
                assert!(returns > 0, "valid return events must survive");
                if mode == "none" {
                    assert!(internal_returns > 0, "internal return jumps must remain marked");
                }
            }
        }
    }
}

#[test]
fn ethdebug_omits_unresolved_library_operands() {
    let mut unresolved = 0;
    for linked in [false, true] {
        let mut args =
            vec!["debug_library.sol", "--emit=bin,bin-runtime,ethdebug,ethdebug-runtime"];
        if linked {
            args.push("--libraries=L=0x1111111111111111111111111111111111111111");
        }
        let output = compile_json(&args);
        for contract in output["contracts"].as_object().unwrap().values() {
            for (program, bytecode) in [("ethdebug", "bin"), ("ethdebug-runtime", "bin-runtime")] {
                let code = contract[bytecode].as_str().unwrap();
                for inst in contract[program]["instructions"].as_array().unwrap() {
                    let operation = &inst["operation"];
                    if let Some(width) =
                        operation["mnemonic"].as_str().unwrap().strip_prefix("PUSH")
                        && let Ok(width) = width.parse::<usize>()
                        && width > 0
                    {
                        let start = (inst["offset"].as_u64().unwrap() as usize + 1) * 2;
                        let operand = &code[start..start + width * 2];
                        if operand.contains('_') {
                            assert!(!linked);
                            assert!(operation.get("arguments").is_none());
                            unresolved += 1;
                        } else {
                            assert_eq!(operation["arguments"], json!([format!("0x{operand}")]));
                        }
                    }
                }
            }
        }
    }
    assert!(unresolved > 0);
}
