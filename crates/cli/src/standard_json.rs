//! Standard JSON input/output mode compatible with solc.
//!
//! This module handles the `--standard-json` flag, allowing Solar to be used
//! as a drop-in replacement for solc in tools like Foundry.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use solar_codegen::{EvmCodegen, FxHashMap, lower};
use solar_config::{ImportRemapping, Opts};
use solar_interface::Session;
use solar_sema::{Compiler, hir::ContractId};
use std::{
    collections::BTreeMap,
    io::{self, Read, Write},
    ops::ControlFlow,
};

/// Type alias for internal contract data: (ABI, deployment_hex, runtime_hex)
type ContractData = (Vec<Value>, String, String);
/// Type alias for the contracts map: source_name -> contract_name -> ContractData
type ContractsMap = BTreeMap<String, BTreeMap<String, ContractData>>;

/// Standard JSON input format (subset of solc's format).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StandardJsonInput {
    /// Source language (should be "Solidity").
    pub language: String,
    /// Source files.
    pub sources: BTreeMap<String, SourceInput>,
    /// Compiler settings.
    #[serde(default)]
    pub settings: Settings,
}

/// A source file in the standard JSON input.
#[derive(Debug, Deserialize)]
pub struct SourceInput {
    /// Source code content.
    pub content: Option<String>,
    /// URLs to fetch source from (not yet supported).
    pub urls: Option<Vec<String>>,
}

/// Compiler settings.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Remappings.
    #[serde(default)]
    pub remappings: Vec<String>,
    /// Output selection.
    #[serde(default)]
    pub output_selection: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    /// Optimizer settings.
    #[serde(default)]
    pub optimizer: OptimizerSettings,
    /// EVM version.
    pub evm_version: Option<String>,
}

/// Optimizer settings.
#[derive(Debug, Default, Deserialize)]
pub struct OptimizerSettings {
    /// Whether optimization is enabled.
    #[serde(default)]
    pub enabled: bool,
    /// Number of optimization runs.
    #[serde(default)]
    pub runs: Option<u32>,
}

/// Standard JSON output format.
#[derive(Debug, Serialize)]
pub struct StandardJsonOutput {
    /// Compilation errors and warnings.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<OutputError>,
    /// Compiled contracts.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub contracts: BTreeMap<String, BTreeMap<String, ContractOutput>>,
    /// Source information.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub sources: BTreeMap<String, SourceOutput>,
}

/// An error or warning in the output.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputError {
    /// Component that generated the error.
    pub component: String,
    /// Error code.
    pub error_code: String,
    /// Formatted message for display.
    pub formatted_message: String,
    /// Error message.
    pub message: String,
    /// Severity: "error" or "warning".
    pub severity: String,
    /// Error type.
    #[serde(rename = "type")]
    pub error_type: String,
}

/// Contract compilation output.
#[derive(Debug, Serialize)]
pub struct ContractOutput {
    /// Contract ABI.
    pub abi: Vec<Value>,
    /// EVM-related outputs.
    pub evm: EvmOutput,
}

/// EVM outputs.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvmOutput {
    /// Creation bytecode.
    pub bytecode: BytecodeOutput,
    /// Deployed bytecode.
    pub deployed_bytecode: BytecodeOutput,
}

/// Bytecode output.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BytecodeOutput {
    /// The bytecode as a hex string.
    pub object: String,
    /// Opcodes (not yet implemented).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub opcodes: String,
    /// Source map (not yet implemented).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub source_map: String,
    /// Link references.
    pub link_references: BTreeMap<String, Value>,
    /// Generated sources.
    pub generated_sources: Vec<Value>,
    /// Function debug data.
    pub function_debug_data: BTreeMap<String, Value>,
}

impl BytecodeOutput {
    fn new(bytecode: String) -> Self {
        Self {
            object: bytecode,
            opcodes: String::new(),
            source_map: String::new(),
            link_references: BTreeMap::new(),
            generated_sources: Vec::new(),
            function_debug_data: BTreeMap::new(),
        }
    }
}

/// Source file output.
#[derive(Debug, Serialize)]
pub struct SourceOutput {
    /// Source ID.
    pub id: u32,
}

/// Runs the compiler in standard JSON mode.
pub fn run_standard_json() -> io::Result<()> {
    eprintln!("DEBUG: run_standard_json called");
    std::io::stderr().flush().unwrap();
    // Read input from stdin
    let mut input_str = String::new();
    io::stdin().read_to_string(&mut input_str)?;

    // Parse input
    let input: StandardJsonInput = match serde_json::from_str(&input_str) {
        Ok(input) => input,
        Err(e) => {
            let output = StandardJsonOutput {
                errors: vec![OutputError {
                    component: "general".to_string(),
                    error_code: "1".to_string(),
                    formatted_message: format!("JSON parse error: {e}"),
                    message: e.to_string(),
                    severity: "error".to_string(),
                    error_type: "JSONError".to_string(),
                }],
                contracts: BTreeMap::new(),
                sources: BTreeMap::new(),
            };
            let json = serde_json::to_string(&output)?;
            io::stdout().write_all(json.as_bytes())?;
            return Ok(());
        }
    };

    // Compile and produce output
    let output = compile_standard_json(input);
    let json = serde_json::to_string(&output)?;
    io::stdout().write_all(json.as_bytes())?;

    Ok(())
}

/// Compiles sources from standard JSON input.
fn compile_standard_json(input: StandardJsonInput) -> StandardJsonOutput {
    let mut output = StandardJsonOutput {
        errors: Vec::new(),
        contracts: BTreeMap::new(),
        sources: BTreeMap::new(),
    };

    // Create a temporary directory for source files
    let temp_dir = match tempfile::tempdir() {
        Ok(dir) => dir,
        Err(e) => {
            output.errors.push(OutputError {
                component: "general".to_string(),
                error_code: "1".to_string(),
                formatted_message: format!("Failed to create temp directory: {e}"),
                message: e.to_string(),
                severity: "error".to_string(),
                error_type: "InternalError".to_string(),
            });
            return output;
        }
    };

    // Write source files to temp directory
    let mut source_paths = Vec::new();
    for (name, source) in &input.sources {
        if let Some(content) = &source.content {
            let path = temp_dir.path().join(name);
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Err(e) = std::fs::write(&path, content) {
                output.errors.push(OutputError {
                    component: "general".to_string(),
                    error_code: "1".to_string(),
                    formatted_message: format!("Failed to write source file {name}: {e}"),
                    message: e.to_string(),
                    severity: "error".to_string(),
                    error_type: "InternalError".to_string(),
                });
                return output;
            }
            source_paths.push((name.clone(), path));
        }
    }

    // Add source IDs
    for (idx, (name, _)) in source_paths.iter().enumerate() {
        output.sources.insert(name.clone(), SourceOutput { id: idx as u32 });
    }

    // Parse import remappings from JSON input
    let import_remappings: Vec<ImportRemapping> =
        input.settings.remappings.iter().filter_map(|s| s.parse().ok()).collect();

    // Build opts with remappings and base_path
    let opts = Opts {
        import_remappings,
        base_path: Some(temp_dir.path().to_path_buf()),
        ..Default::default()
    };

    // Create session with buffer emitter (to capture errors as JSON)
    let sess = Session::builder()
        .with_buffer_emitter(solar_interface::ColorChoice::Never)
        .opts(opts)
        .build();

    let mut compiler = Compiler::new(sess);

    eprintln!("DEBUG: entering compiler.enter_mut");
    // Parse and compile
    let compile_result = compiler.enter_mut(|compiler| -> solar_interface::Result<ContractsMap> {
        eprintln!("DEBUG: inside enter_mut, parsing files");
        // Parse files
        let mut pcx = compiler.parse();
        let paths: Vec<&std::path::Path> = source_paths.iter().map(|(_, p)| p.as_path()).collect();
        pcx.load_files(paths)?;
        pcx.parse();

        // Lower ASTs
        eprintln!("DEBUG: lowering ASTs");
        let ControlFlow::Continue(()) = compiler.lower_asts()? else {
            eprintln!("DEBUG: lower_asts() returned early");
            return Ok(BTreeMap::new());
        };

        // Analysis
        eprintln!("DEBUG: running analysis");
        let ControlFlow::Continue(()) = compiler.analysis()? else {
            eprintln!("DEBUG: analysis() returned early");
            return Ok(BTreeMap::new());
        };

        eprintln!("DEBUG: analysis complete, starting codegen");
        let gcx = compiler.gcx();

        // Two-pass compilation to support `new Contract()` expressions:
        // Pass 1: Compile all contracts to get their bytecodes
        let mut all_bytecodes: FxHashMap<ContractId, Vec<u8>> = FxHashMap::default();

        for (contract_id, _contract) in gcx.hir.contracts_enumerated() {
            // Lower to MIR (without child bytecodes for first pass)
            let mut module = lower::lower_contract(gcx, contract_id);

            // Generate deployment bytecode
            let mut codegen = EvmCodegen::new();
            let (deployment_bytecode, _runtime_bytecode) =
                codegen.generate_deployment_bytecode(&mut module);

            all_bytecodes.insert(contract_id, deployment_bytecode);
        }

        // Pass 2: Recompile with bytecodes available for `new` expressions
        let mut contracts_output: ContractsMap = BTreeMap::new();

        for (contract_id, contract) in gcx.hir.contracts_enumerated() {
            // Find source file name for this contract by matching the file path
            let source = gcx.hir.source(contract.source);
            let source_name = source_paths
                .iter()
                .find(|(_, path)| source.file.name == **path)
                .map(|(name, _)| name.clone())
                .unwrap_or_else(|| "Unknown.sol".to_string());

            // Lower to MIR with all bytecodes available
            let mut module = lower::lower_contract_with_bytecodes(gcx, contract_id, &all_bytecodes);

            // Generate bytecode (deployment and runtime)
            eprintln!("DEBUG: Generating bytecode for {:?}", contract.name);
            let mut codegen = EvmCodegen::new();
            let (deployment_bytecode, runtime_bytecode) =
                codegen.generate_deployment_bytecode(&mut module);
            eprintln!("DEBUG: Done generating bytecode, {} bytes", deployment_bytecode.len());
            let deployment_hex = alloy_primitives::hex::encode(&deployment_bytecode);
            let runtime_hex = alloy_primitives::hex::encode(&runtime_bytecode);

            // Generate ABI using proper method that includes inherited functions
            let abi = gcx
                .contract_abi(contract_id)
                .into_iter()
                .map(|item| serde_json::to_value(&item).unwrap())
                .collect();

            let contract_name = contract.name.to_string();
            contracts_output
                .entry(source_name)
                .or_default()
                .insert(contract_name, (abi, deployment_hex, runtime_hex));
        }

        Ok(contracts_output)
    });

    // Handle compilation result
    match compile_result {
        Ok(contracts) => {
            for (source_name, source_contracts) in contracts {
                for (contract_name, (abi, deployment_bytecode, runtime_bytecode)) in
                    source_contracts
                {
                    let contract_output = ContractOutput {
                        abi,
                        evm: EvmOutput {
                            bytecode: BytecodeOutput::new(deployment_bytecode),
                            deployed_bytecode: BytecodeOutput::new(runtime_bytecode),
                        },
                    };
                    output
                        .contracts
                        .entry(source_name.clone())
                        .or_default()
                        .insert(contract_name, contract_output);
                }
            }
        }
        Err(_) => {
            // Collect diagnostics from the session
            if let Err(errs) = compiler.sess().emitted_errors().unwrap() {
                output.errors.push(OutputError {
                    component: "general".to_string(),
                    error_code: "1".to_string(),
                    formatted_message: errs.to_string(),
                    message: errs.to_string(),
                    severity: "error".to_string(),
                    error_type: "CompilerError".to_string(),
                });
            }
        }
    }

    output
}
