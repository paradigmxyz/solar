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
fn debug_output_selection_preserves_shared_tail_bytecode() {
    for source in [
        "../../codegen/lowering/run-call/external_call_returndata_size.sol",
        "../../codegen/lowering/empty_code_external_call.sol",
        "../../codegen/lowering/storage_checked_arithmetic.sol",
        "../../codegen/lowering/library_address.sol",
        "../../../foundry/libraries/test/OverloadLib.t.sol",
    ] {
        for mode in ["none", "gas", "size"] {
            let baseline = compile_json(&[source, "-O", mode, "--emit=bin,bin-runtime"]);
            for selection in DEBUG_OUTPUTS {
                let output = compile_json(&[
                    source,
                    "-O",
                    mode,
                    &format!("--emit=bin,bin-runtime,{selection}"),
                ]);
                for (name, contract) in baseline["contracts"].as_object().unwrap() {
                    for bytecode in ["bin", "bin-runtime"] {
                        assert_eq!(
                            contract[bytecode], output["contracts"][name][bytecode],
                            "{mode}: {selection}: {name}: {bytecode}"
                        );
                    }
                }
            }
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
fn inlined_dispatch_debug_outputs_are_bytecode_neutral() {
    let source = "../../codegen/lowering/run-call/scalar_dispatch.sol";
    for mode in ["gas", "size"] {
        let baseline = compile_json(&[source, "-O", mode, "--emit=bin,bin-runtime"]);
        let debug = compile_json(&[
            source,
            "-O",
            mode,
            "--emit=bin,bin-runtime,ethdebug,ethdebug-runtime,srcmap,srcmap-runtime",
        ]);
        for (name, contract) in baseline["contracts"].as_object().unwrap() {
            for field in ["bin", "bin-runtime"] {
                assert_eq!(contract[field], debug["contracts"][name][field], "{mode}: {field}");
            }
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
    assert_eq!(resources["types"]["t_uint256"], json!({"kind": "uint", "bits": 256}));
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

#[test]
fn debug_types_storage_templates_and_variable_observations() {
    let source = "debug_variables.sol";
    let output =
        compile_json(&[source, "-O", "none", "--emit=bin,bin-runtime,ethdebug,ethdebug-runtime"]);
    let resources = &output["ethdebug"];
    assert_eq!(resources["ethdebug"]["version"], "0.1.0-draft.2");
    let mut documents = json!({"types": resources["types"], "pointers": resources["pointers"]});
    // Declaration coordinates are checked below; the resource snapshot focuses on composition.
    for document in documents["types"].as_object_mut().unwrap().values_mut() {
        if let Some(definition) = document.get_mut("definition") {
            definition.as_object_mut().unwrap().remove("location");
        }
    }
    snapbox::assert_data_eq!(
        serde_json::to_string_pretty(&documents).unwrap(),
        snapbox::file!["snapshots/debug_types.json"]
    );
    let program = &output["contracts"]["debug_variables.sol:DebugVariables"]["ethdebug-runtime"];
    let variables = program["context"]["variables"].as_array().unwrap();
    let small = variables.iter().find(|variable| variable["identifier"] == "small").unwrap();
    assert_eq!(
        small["pointer"],
        json!({"location": "storage", "slot": "0x00", "offset": 31, "length": 1})
    );
    let signed = variables.iter().find(|variable| variable["identifier"] == "signedValue").unwrap();
    assert_eq!(signed["pointer"]["offset"], 28);
    let temporary =
        variables.iter().find(|variable| variable["identifier"] == "temporary").unwrap();
    assert_eq!(temporary["pointer"]["location"], "transient");
    assert_eq!(temporary["pointer"]["slot"], "0x00");
    let text = variables.iter().find(|variable| variable["identifier"] == "text").unwrap();
    assert_eq!(text["pointer"]["in"], json!({"template": "t_string_storage"}));
    for variable in variables {
        let range = &variable["declaration"]["range"];
        let start = range["offset"].as_u64().unwrap() as usize;
        let end = start + range["length"].as_u64().unwrap() as usize;
        let contents = resources["compilation"]["sources"][0]["contents"].as_str().unwrap();
        assert!(!contents[start..end].is_empty());
        assert!(resources["types"].get(variable["type"]["id"].as_str().unwrap()).is_some());
    }
    let observations = program["instructions"].as_array().unwrap().iter().filter_map(|instruction| {
        let variables = context_variables(&instruction["context"]);
        if variables.is_empty() { return None; }
        Some(json!({"operation": instruction["operation"]["mnemonic"], "variables": variables.iter().map(|variable| json!({"identifier": variable["identifier"], "type": variable["type"], "pointer": variable["pointer"]})).collect::<Vec<_>>()}))
    }).collect::<Vec<_>>();
    assert!(!observations.is_empty());
    snapbox::assert_data_eq!(
        serde_json::to_string_pretty(&observations).unwrap(),
        snapbox::file!["snapshots/debug_variables.json"]
    );
}

#[test]
fn variable_debug_records_preserve_optimized_bytecode() {
    for mode in ["none", "gas", "size"] {
        let baseline = compile_json(&["debug_variables.sol", "-O", mode, "--emit=bin,bin-runtime"]);
        let mut debug = compile_json(&[
            "debug_variables.sol",
            "-O",
            mode,
            "--emit=bin,bin-runtime,ethdebug,ethdebug-runtime",
        ]);
        for contract in debug["contracts"].as_object_mut().unwrap().values_mut() {
            contract.as_object_mut().unwrap().remove("ethdebug");
            contract.as_object_mut().unwrap().remove("ethdebug-runtime");
        }
        assert_eq!(debug["contracts"], baseline["contracts"], "{mode}");
    }
}

#[test]
fn shared_variable_origins_are_unavailable() {
    let source = "debug_variables.sol";
    for mode in ["gas", "size"] {
        let output = compile_json(&[source, "-O", mode, "--emit=ethdebug-runtime"]);
        for contract in output["contracts"].as_object().unwrap().values() {
            for instruction in contract["ethdebug-runtime"]["instructions"].as_array().unwrap() {
                if let Some(origins) = instruction["context"]["pick"].as_array() {
                    assert!(origins.len() > 1);
                    assert!(context_variables(&instruction["context"]).is_empty());
                }
            }
        }
    }
}

#[test]
fn immutable_debug_locations_follow_final_relocations() {
    for mode in ["none", "gas", "size"] {
        let output = compile_json(&[
            "debug_variables.sol",
            "-O",
            mode,
            "--emit=bin,bin-runtime,ethdebug,ethdebug-runtime",
        ]);
        let contract = &output["contracts"]["debug_variables.sol:DebugImmutables"];
        let runtime = contract["bin-runtime"].as_str().unwrap();
        let variables = contract["ethdebug-runtime"]["context"]["variables"].as_array().unwrap();
        for (name, length) in [("value", 8), ("prefix", 4)] {
            let variable =
                variables.iter().find(|variable| variable["identifier"] == name).unwrap();
            let pointer = &variable["pointer"];
            assert_eq!(pointer["location"], "code");
            assert_eq!(pointer["length"], length);
            let start = pointer["offset"].as_u64().unwrap() as usize;
            assert!(start + length as usize <= runtime.len() / 2);
            // Unlinked immutable operands are zero placeholders, not a constant recovery recipe.
            assert_eq!(
                &runtime[2 * start..2 * (start + length as usize)],
                "0".repeat(length as usize * 2)
            );
        }
        for variable in contract["ethdebug"]["context"]["variables"].as_array().unwrap() {
            assert!(variable.get("pointer").is_none());
        }
        let constant = variables.iter().find(|variable| variable["identifier"] == "SCALE").unwrap();
        assert!(constant.get("pointer").is_none());
    }
}

#[test]
fn standard_json_programs_include_their_type_resources() {
    let input = json!({
        "language": "Solidity",
        "sources": {"C.sol": {"content": "contract C { uint8 x; function read() external view returns (uint8) { return x; } }"}},
        "settings": {"outputSelection": {"*": {"*": ["evm.deployedBytecode.ethdebug"]}}}
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
    let program = &output["contracts"]["C.sol"]["C"]["evm"]["deployedBytecode"]["ethdebug"];
    assert_eq!(program["context"]["variables"][0]["type"]["id"], "t_uint8");
    assert_eq!(
        output["ethdebug"]["resources"]["types"]["t_uint8"],
        json!({"kind": "uint", "bits": 8})
    );
    assert_eq!(program["compilation"]["id"], output["ethdebug"]["resources"]["compilation"]["id"]);
}

#[test]
fn assignment_inside_an_expression_invalidates_old_variable_reads() {
    let output = compile_json(&["debug_variables.sol", "-O", "none", "--emit=ethdebug-runtime"]);
    let source = output["ethdebug"]["compilation"]["sources"][0]["contents"].as_str().unwrap();
    let begin = source.find("function mutate(").unwrap();
    let declaration = begin + source[begin..].find("uint256 x").unwrap();
    let program = &output["contracts"]["debug_variables.sol:DebugVariables"]["ethdebug-runtime"];
    for instruction in program["instructions"].as_array().unwrap() {
        for variable in context_variables(&instruction["context"]) {
            assert_ne!(variable["declaration"]["range"]["offset"], declaration);
        }
    }
    // A read after an assignment with no intervening opcode is a different epoch.
    let begin = source.find("function afterWrite(").unwrap();
    let declaration = begin + source[begin..].find("uint256 x").unwrap();
    assert!(program["instructions"].as_array().unwrap().iter().any(|instruction| {
        {
            let variables = context_variables(&instruction["context"]);
            variables
                .iter()
                .any(|variable| variable["declaration"]["range"]["offset"] == declaration)
        }
    }));
}

fn context_variables(context: &Value) -> Vec<&Value> {
    let mut variables = context["variables"].as_array().into_iter().flatten().collect::<Vec<_>>();
    for child in context["gather"].as_array().into_iter().flatten() {
        variables.extend(context_variables(child));
    }
    variables
}

#[test]
fn optimized_debug_tracking_preserves_scopes_spills_and_bytecode() {
    for mode in ["none", "gas", "size"] {
        let baseline = compile_json(&["debug_tracking.sol", "-O", mode, "--emit=bin,bin-runtime"]);
        let output = compile_json(&[
            "debug_tracking.sol",
            "-O",
            mode,
            "--emit=bin,bin-runtime,ethdebug,ethdebug-runtime",
        ]);
        let key = "debug_tracking.sol:DebugTracking";
        for field in ["bin", "bin-runtime"] {
            assert_eq!(
                baseline["contracts"][key][field], output["contracts"][key][field],
                "{mode}: {field}"
            );
        }
        let program = &output["contracts"][key]["ethdebug-runtime"];
        let contexts = program["instructions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|inst| &inst["context"])
            .collect::<Vec<_>>();
        for context in &contexts {
            check_context_gathers(context);
        }
        let variables =
            contexts.iter().flat_map(|context| context_variables(context)).collect::<Vec<_>>();
        for variable in &variables {
            assert!(
                output["ethdebug"]["types"].get(variable["type"]["id"].as_str().unwrap()).is_some()
            );
        }
        assert!(
            variables
                .iter()
                .any(|var| var["identifier"] == "object" && var.get("pointer").is_some()),
            "{mode}: composite object"
        );
        assert!(
            variables
                .iter()
                .any(|var| var["identifier"] == "input" && var.get("pointer").is_some()),
            "{mode}: calldata bytes"
        );
        assert!(
            variables
                .iter()
                .any(|var| var["identifier"] == "items" && var.get("pointer").is_some()),
            "{mode}: multiword array"
        );
        assert!(
            variables
                .iter()
                .any(|var| var["identifier"].as_str().is_some_and(|name| name.starts_with('a'))
                    && var["pointer"]["location"] == "memory"),
            "{mode}: spill"
        );
        if mode != "none" {
            let inline = contexts
                .iter()
                .flat_map(|context| context_names(context, "inline:"))
                .collect::<std::collections::BTreeSet<_>>();
            assert!(inline.len() >= 2, "{mode}: separate inlined instances: {inline:?}");
        }
        let declarations = variables
            .iter()
            .filter(|var| var["identifier"] == "value")
            .filter_map(|var| var["declaration"]["range"]["offset"].as_u64())
            .collect::<std::collections::BTreeSet<_>>();
        assert!(declarations.len() >= 2, "{mode}: shadowed declarations");
        // A modified memory object must not be reconstructed from its original calldata.
        let source = output["ethdebug"]["compilation"]["sources"][0]["contents"].as_str().unwrap();
        let begin = source.find("function mutateObject(").unwrap();
        let declaration = begin + source[begin..].find("Tree memory object").unwrap();
        for variable in
            variables.iter().filter(|var| var["declaration"]["range"]["offset"] == declaration)
        {
            assert!(
                !pointer_uses_location(&variable["pointer"], "calldata"),
                "{mode}: stale ABI origin"
            );
        }
    }
}

fn context_names<'a>(context: &'a Value, prefix: &str) -> Vec<&'a str> {
    let mut names = context["name"]
        .as_str()
        .filter(|name| name.starts_with(prefix))
        .into_iter()
        .collect::<Vec<_>>();
    for child in context["gather"].as_array().into_iter().flatten() {
        names.extend(context_names(child, prefix));
    }
    names
}

fn pointer_uses_location(pointer: &Value, location: &str) -> bool {
    match pointer {
        Value::Object(object) => {
            object.get("location").is_some_and(|value| value == location)
                || object.values().any(|value| pointer_uses_location(value, location))
        }
        Value::Array(values) => values.iter().any(|value| pointer_uses_location(value, location)),
        _ => false,
    }
}

fn check_context_gathers(context: &Value) {
    if let Some(children) = context["gather"].as_array() {
        assert!(children.len() >= 2, "ETHDebug gather requires two contexts: {context}");
        for child in children {
            check_context_gathers(child);
        }
    }
    for child in context["pick"].as_array().into_iter().flatten() {
        check_context_gathers(child);
    }
}
