//! Standard JSON compiler orchestration and output generation.

use super::data::{
    BytecodeOutput, CompilerInput, CompilerOutput, ContractOutput, EvmOutput, FxIndexMap,
    Optimizer, OutputSelection, OutputSelectionFlags, ReadCallbackResult, Settings, SourceOutput,
    StandardJsonReadCallback, print_standard_json_stats, strip_json_comments,
};
use alloy_primitives::Bytes;
use serde_json::json;
use solar_codegen::{EvmCodegen, lower};
use solar_config::{
    CompileOpts, CompilerStage, EvmVersion, ImportRemapping, Language, OptimizationMode,
};
use solar_data_structures::map::{FxHashMap, FxHashSet};
use solar_interface::{
    Result, SourceMap,
    diagnostics::{DiagCtxt, InMemoryEmitter, JsonEmitter, SolcDiagnostic},
    source_map::FileLoader,
};
use solar_sema::{Gcx, hir::ContractId};
use std::{
    borrow::Cow,
    fs::File,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
};

/// Compiles Standard JSON input and returns Standard JSON output.
pub fn compile_standard_json(
    input: &str,
    mut opts: CompileOpts,
    read_callback: Option<Arc<dyn StandardJsonReadCallback>>,
    out: &mut dyn Write,
) {
    let source_map = Arc::new(SourceMap::empty());
    source_map.set_file_loader(StandardJsonFileLoader { read_callback });
    let (emitter, diagnostics) = InMemoryEmitter::new();
    let dcx = DiagCtxt::new(Box::new(emitter))
        .with_flags(|flags| flags.update_from_opts(&opts))
        .with_allowed_diagnostic_codes(opts.allow.iter().cloned());

    let mut output = CompilerOutput::default();
    let input = if opts.unstable.ui_testing {
        Cow::Owned(strip_json_comments(input))
    } else {
        Cow::Borrowed(input)
    };
    match serde_json::from_str::<CompilerInput<'_>>(&input) {
        Ok(compiler_input) => {
            if opts.unstable.standard_json_stats {
                print_standard_json_stats(&input, &compiler_input);
            }
            compile(compiler_input, &mut opts, Arc::clone(&source_map), dcx, &mut output);
        }
        Err(e) => {
            dcx.err(format!("JSON parse error: {e}")).emit();
        }
    }

    let mut emitter = JsonEmitter::new(Box::new(io::sink()), Arc::clone(&source_map), opts.color)
        .ui_testing(opts.unstable.ui_testing)
        .human_kind(opts.error_format_human)
        .terminal_width(opts.diagnostic_width);
    let diagnostics = diagnostics.read();
    output.errors =
        diagnostics.iter().map(|diagnostic| emitter.solc_diagnostic(diagnostic)).collect();

    if output.errors.iter().any(SolcDiagnostic::is_error) {
        output.contracts.clear();
    }

    let result = if opts.pretty_json {
        serde_json::to_writer_pretty(out, &output)
    } else {
        serde_json::to_writer(out, &output)
    };
    let _ = result;
}

pub(crate) fn run(opts: CompileOpts) -> io::Result<()> {
    let stdout = io::stdout();
    let mut stdout = io::BufWriter::new(stdout.lock());
    let mut input = String::new();
    let result = match opts.input.as_slice() {
        [] => io::stdin().read_to_string(&mut input),
        [arg] if arg == "-" => io::stdin().read_to_string(&mut input),
        [path] => File::open(path).and_then(|mut file| file.read_to_string(&mut input)),
        _ => unreachable!("standard JSON input count is validated during argument parsing"),
    };
    match result {
        Ok(_) => compile_standard_json(&input, opts, None, &mut stdout),
        Err(e) => standard_json_error_output(
            format!("failed to read standard JSON input: {e}"),
            &mut stdout,
        )?,
    }
    stdout.write_all(b"\n")?;
    stdout.flush()
}

fn standard_json_error_output(message: String, out: &mut dyn Write) -> io::Result<()> {
    let output = json!({
        "errors": [{
            "severity": "error",
            "type": "IOError",
            "message": message,
        }],
    });
    serde_json::to_writer(out, &output).map_err(io::Error::other)
}

fn compile(
    input: CompilerInput<'_>,
    opts: &mut CompileOpts,
    source_map: Arc<SourceMap>,
    dcx: DiagCtxt,
    output: &mut CompilerOutput<'_>,
) {
    let CompilerInput { language, sources, settings } = input;
    // Destructure `Settings` so every recognized field is handled explicitly;
    // fields we don't act on yet are bound with a leading underscore and a note.
    // Adding a field to `Settings` then forces a decision here instead of it
    // being silently ignored.
    let Settings { remappings, output_selection, stop_after, evm_version, optimizer } = settings;

    let mut parsed_remappings = Vec::with_capacity(remappings.len());
    for remapping in &remappings {
        match remapping.parse::<ImportRemapping>() {
            Ok(remapping) => parsed_remappings.push(remapping),
            Err(e) => {
                dcx.err(format!("invalid remapping `{remapping}`: {e}")).emit();
            }
        }
    }
    if dcx.has_errors().is_err() {
        return;
    }

    opts.import_remappings = parsed_remappings;
    opts.evm_version = evm_version
        .as_deref()
        .and_then(|version| EvmVersion::from_str(version).ok())
        .unwrap_or(opts.evm_version);
    opts.language = match language.as_ref() {
        "Solidity" | "solidity" => Language::Solidity,
        "Yul" | "yul" => Language::Yul,
        language => {
            dcx.err(format!("unsupported language `{language}`")).emit();
            return;
        }
    };
    opts.stop_after = stop_after.as_deref().and_then(|stage| CompilerStage::from_str(stage).ok());

    // Map the solc optimizer toggle onto our MIR optimization objective. We only
    // override when the input explicitly disables the optimizer, leaving the
    // CLI-driven default otherwise. `runs` and `details` have no analogue in the
    // MIR optimizer yet, so they're parsed but unused.
    if let Some(Optimizer { enabled: false, runs: _runs }) = optimizer {
        opts.optimization = OptimizationMode::None;
    }

    opts.input = sources.keys().map(ToString::to_string).collect();

    let sess = solar_interface::Session::builder()
        .source_map(Arc::clone(&source_map))
        .dcx(dcx)
        .opts(opts.clone())
        .build();

    let _ = crate::commands::compile::run_compiler_session_with(
        sess,
        |compiler| {
            let _control_flow = crate::commands::compile::run_pipeline(
                compiler,
                |pcx| {
                    let mut files = Vec::with_capacity(sources.len());
                    for (name, source) in sources {
                        let Some(content) = source.content else {
                            let message = if source.urls.is_empty() {
                                format!("source `{name}` is missing `content`")
                            } else {
                                format!("source URLs are not supported for `{name}`")
                            };
                            return Err(pcx.dcx().err(message).emit());
                        };
                        files.push((PathBuf::from(name.as_ref()), content));
                    }
                    pcx.par_load_files_with_contents(files)
                },
                |compiler| output.sources = source_outputs_from_compiler(compiler),
            )?;

            let gcx = compiler.gcx();

            // Code generation is experimental and gated behind `-Zcodegen`;
            // without it, no bytecode is produced even when requested.
            let bytecodes = if gcx.sess.opts.unstable.codegen
                && needs_bytecode_output(gcx, &output_selection)
            {
                Some(generate_contract_bytecodes(gcx)?)
            } else {
                None
            };

            gcx.dcx().has_errors()?;

            for (contract_id, contract) in gcx.hir.contracts_enumerated() {
                let source = gcx.hir.source(contract.source);
                let source_name = standard_json_source_name(&source.file.name);
                let contract_name = contract.name.as_str();
                let contract_selection = output_selection.contract(&source_name, contract_name);
                let contract_output =
                    make_contract_output(gcx, contract_id, contract_selection, bytecodes.as_ref());
                if !contract_output.is_empty() {
                    output
                        .contracts
                        .entry(source_name)
                        .or_default()
                        .insert(contract_name.to_string(), contract_output);
                }
            }

            Ok(())
        },
        false,
    );
}

struct GeneratedBytecodes {
    deployment: Bytes,
    runtime: Bytes,
}

struct StandardJsonFileLoader {
    read_callback: Option<Arc<dyn StandardJsonReadCallback>>,
}

impl FileLoader for StandardJsonFileLoader {
    fn canonicalize_path(&self, path: &Path) -> io::Result<PathBuf> {
        if path.is_absolute()
            && let Ok(cwd) = std::env::current_dir()
            && let Ok(path) = path.strip_prefix(cwd)
        {
            Ok(path.to_path_buf())
        } else {
            Ok(path.to_path_buf())
        }
    }

    fn load_stdin(&self) -> io::Result<String> {
        self.read_source(Path::new("stdin"))
    }

    fn load_file(&self, path: &Path) -> io::Result<String> {
        self.read_source(path)
    }

    fn load_binary_file(&self, path: &Path) -> io::Result<Vec<u8>> {
        Err(disallowed_io(path))
    }
}

impl StandardJsonFileLoader {
    fn read_source(&self, path: &Path) -> io::Result<String> {
        let Some(read_callback) = &self.read_callback else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "File import callback not supported",
            ));
        };
        let data = callback_path(path);
        match read_callback.read("source", &data) {
            ReadCallbackResult::Success(contents) => Ok(contents),
            ReadCallbackResult::Error(error) => Err(io::Error::other(error)),
            ReadCallbackResult::Unsupported => {
                Err(io::Error::new(io::ErrorKind::Unsupported, unsupported_callback_kind("source")))
            }
        }
    }
}

/// Returns the diagnostic message for an unsupported Standard JSON callback kind.
pub(crate) fn unsupported_callback_kind(kind: &str) -> String {
    format!("Callback kind `{kind}` is not supported")
}

fn callback_path(path: &Path) -> Cow<'_, str> {
    let path = if path.is_absolute()
        && let Ok(cwd) = std::env::current_dir()
        && let Ok(path) = path.strip_prefix(cwd)
    {
        path
    } else {
        path
    };
    path.to_string_lossy()
}

fn standard_json_source_name(name: &solar_interface::source_map::FileName) -> String {
    name.display().to_string().replace('\\', "/")
}

fn disallowed_io(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!("standard JSON mode cannot read `{}` from the filesystem", path.display()),
    )
}

fn source_outputs_from_compiler(
    compiler: &solar_sema::CompilerRef<'_>,
) -> FxIndexMap<String, SourceOutput> {
    compiler
        .gcx()
        .sources
        .iter_enumerated()
        .map(|(id, source)| {
            (standard_json_source_name(&source.file.name), SourceOutput { id: id.index() as u32 })
        })
        .collect()
}

fn make_contract_output(
    gcx: Gcx<'_>,
    contract_id: solar_sema::hir::ContractId,
    output_selection: OutputSelectionFlags,
    bytecodes: Option<&FxHashMap<ContractId, GeneratedBytecodes>>,
) -> ContractOutput {
    let mut output = ContractOutput::default();

    if output_selection.contains(OutputSelectionFlags::ABI) {
        output.abi = Some(gcx.contract_abi(contract_id));
    }
    if output_selection.contains(OutputSelectionFlags::USERDOC) {
        output.userdoc = Some(gcx.user_documentation(contract_id));
    }
    if output_selection.contains(OutputSelectionFlags::DEVDOC) {
        output.devdoc = Some(gcx.dev_documentation(contract_id));
    }
    if output_selection.contains(OutputSelectionFlags::STORAGE_LAYOUT) {
        output.storage_layout = Some(gcx.storage_layout(contract_id));
    }
    if output_selection.contains(OutputSelectionFlags::TRANSIENT_STORAGE_LAYOUT) {
        output.transient_storage_layout = Some(gcx.transient_storage_layout(contract_id));
    }

    let mut evm = EvmOutput::default();
    if output_selection.contains(OutputSelectionFlags::METHOD_IDENTIFIERS) {
        for function in gcx.interface_functions(contract_id) {
            evm.method_identifiers.insert(
                gcx.item_signature(function.id.into()).to_string(),
                alloy_primitives::hex::encode(function.selector),
            );
        }
    }
    // In solc's output selection `evm.bytecode` is the full bytecode object
    // (`object`, `opcodes`, `sourceMap`, `linkReferences`, ...) and
    // `evm.bytecode.object` selects only the `object` hex sub-field. We match
    // either selector and emit a `BytecodeOutput`; since we only populate
    // `object` for now, the two selectors currently produce identical output.
    // Honoring the finer-grained `.object`/`.opcodes`/`.sourceMap` selectors is
    // part of the larger effort to match solc's input->output key mapping.
    if output_selection.contains(OutputSelectionFlags::BYTECODE_OBJECT) {
        evm.bytecode = Some(
            bytecodes
                .and_then(|bytecodes| bytecodes.get(&contract_id))
                .map(|bytecodes| BytecodeOutput::new(bytecodes.deployment.clone()))
                .unwrap_or_else(BytecodeOutput::empty),
        );
    }
    if output_selection.contains(OutputSelectionFlags::DEPLOYED_BYTECODE_OBJECT) {
        evm.deployed_bytecode = Some(
            bytecodes
                .and_then(|bytecodes| bytecodes.get(&contract_id))
                .map(|bytecodes| BytecodeOutput::new(bytecodes.runtime.clone()))
                .unwrap_or_else(BytecodeOutput::empty),
        );
    }
    if !evm.is_empty() {
        output.evm = Some(evm);
    }

    output
}

fn needs_bytecode_output(gcx: solar_sema::Gcx<'_>, output_selection: &OutputSelection<'_>) -> bool {
    gcx.hir.contracts_enumerated().any(|(_, contract)| {
        let source = gcx.hir.source(contract.source);
        let source_name = source.file.name.display().to_string();
        let contract_name = contract.name.as_str();
        output_selection.contract(&source_name, contract_name).intersects(
            OutputSelectionFlags::BYTECODE_OBJECT | OutputSelectionFlags::DEPLOYED_BYTECODE_OBJECT,
        )
    })
}

fn generate_contract_bytecodes(
    gcx: solar_sema::Gcx<'_>,
) -> Result<FxHashMap<ContractId, GeneratedBytecodes>> {
    let mut all_bytecodes = FxHashMap::default();
    let mut visiting = FxHashSet::default();
    for contract_id in gcx.hir.contract_ids() {
        let contract = gcx.hir.contract(contract_id);
        if !contract.kind.is_interface() && !contract.kind.is_abstract_contract() {
            ensure_contract_bytecode(gcx, contract_id, &mut all_bytecodes, &mut visiting)?;
        }
    }

    let mut bytecodes = FxHashMap::default();
    for contract_id in gcx.hir.contract_ids() {
        let contract = gcx.hir.contract(contract_id);
        if !contract.kind.is_interface() && !contract.kind.is_abstract_contract() {
            let mut module = lower::lower_contract_with_bytecodes(gcx, contract_id, &all_bytecodes);
            gcx.dcx().has_errors()?;
            let mut codegen = EvmCodegen::new(gcx);
            let (deployment, runtime) = codegen.generate_deployment_bytecode(&mut module);
            bytecodes.insert(
                contract_id,
                GeneratedBytecodes { deployment: deployment.into(), runtime: runtime.into() },
            );
        }
    }

    Ok(bytecodes)
}

fn ensure_contract_bytecode(
    gcx: solar_sema::Gcx<'_>,
    contract_id: ContractId,
    all_bytecodes: &mut FxHashMap<ContractId, Vec<u8>>,
    visiting: &mut FxHashSet<ContractId>,
) -> Result {
    let contract = gcx.hir.contract(contract_id);

    if all_bytecodes.contains_key(&contract_id) {
        return Ok(());
    }

    if contract.kind.is_interface() || contract.kind.is_abstract_contract() {
        return Err(gcx
            .dcx()
            .err("cannot generate creation bytecode for non-deployable contract")
            .span(contract.span)
            .emit());
    }

    if !visiting.insert(contract_id) {
        return Err(gcx
            .dcx()
            .err("recursive contract creation bytecode dependency")
            .span(contract.span)
            .emit());
    }

    for dep in lower::contract_bytecode_dependencies(gcx, contract_id).iter() {
        ensure_contract_bytecode(gcx, dep, all_bytecodes, visiting)?;
    }

    let mut module = lower::lower_contract_with_bytecodes(gcx, contract_id, all_bytecodes);
    gcx.dcx().has_errors()?;
    let mut codegen = EvmCodegen::new(gcx);
    let (deployment, _) = codegen.generate_deployment_bytecode(&mut module);
    all_bytecodes.insert(contract_id, deployment);
    visiting.remove(&contract_id);

    Ok(())
}
