//! Standard JSON compiler orchestration and output generation.

use super::{
    data::{
        BytecodeOutput, CompilerInput, CompilerOutput, ContractOutput, DebugInfoComponent,
        DebugSettings, EthdebugCodePointer, EthdebugCompilation, EthdebugCompiler, EthdebugContext,
        EthdebugContract, EthdebugEnvironment, EthdebugFunctionExit, EthdebugFunctionInvoke,
        EthdebugId, EthdebugInstruction, EthdebugInvocationTarget, EthdebugOperation,
        EthdebugOutput, EthdebugProgram, EthdebugRange, EthdebugReference, EthdebugResources,
        EthdebugSource, EthdebugSourceRange, EvmOutput, FxIndexMap, MetadataHash, OffsetLength,
        OutputSelection, OutputSelectionFlags, ReadCallbackResult, Settings, SourceOutput,
        StandardJsonReadCallback, optimizer_settings, print_standard_json_stats,
        strip_json_comments,
    },
    metadata::Metadata,
};
use crate::bytecode::MaybeHexBytecode;
use serde_json::json;
use solar_codegen::{
    ContractArtifact, ContractSelection, RuntimeDataFn,
    backend::evm::{DebugFunction, DebugFunctionExit, DebugInstruction},
};
use solar_config::{
    CompileOpts, CompilerStage, EvmVersion, ImportRemapping, Language, LibraryAddress,
    OptimizationMode, RevertStrings,
};
use solar_data_structures::map::FxHashMap;
use solar_interface::{
    SourceMap,
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
    out: &mut (dyn Write + Send),
) -> io::Result<()> {
    let source_map = Arc::new(SourceMap::empty());
    source_map.set_file_loader(StandardJsonFileLoader { read_callback });
    let (emitter, diagnostics) = InMemoryEmitter::new();
    let dcx = DiagCtxt::new(Box::new(emitter))
        .with_flags(|flags| flags.update_from_opts(&opts))
        .with_allowed_diagnostic_codes(opts.allow.iter().cloned());

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
            return compile(
                compiler_input,
                &mut opts,
                Arc::clone(&source_map),
                dcx,
                &diagnostics,
                out,
            );
        }
        Err(e) => {
            dcx.err(format!("JSON parse error: {e}")).emit();
        }
    }

    write_empty_standard_json_output(Arc::clone(&source_map), &opts, &diagnostics, out)
}

pub(crate) fn run(opts: CompileOpts) -> io::Result<()> {
    let mut stdout = io::BufWriter::new(io::stdout());
    let mut input = String::new();
    let result = match opts.input.as_slice() {
        [] => io::stdin().read_to_string(&mut input),
        [arg] if arg == "-" => io::stdin().read_to_string(&mut input),
        [path] => File::open(path).and_then(|mut file| file.read_to_string(&mut input)),
        _ => unreachable!("standard JSON input count is validated during argument parsing"),
    };
    match result {
        Ok(_) => compile_standard_json(&input, opts, None, &mut stdout)?,
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

fn write_empty_standard_json_output(
    source_map: Arc<SourceMap>,
    opts: &CompileOpts,
    diagnostics: &solar_data_structures::sync::RwLock<Vec<solar_interface::diagnostics::Diag>>,
    out: &mut (dyn Write + Send),
) -> io::Result<()> {
    let mut output = CompilerOutput::default();
    let diagnostics = diagnostics.read();
    finish_standard_json_output(&mut output, source_map, opts, &diagnostics, out)
}

fn finish_standard_json_output<'a>(
    output: &mut CompilerOutput<'a>,
    source_map: Arc<SourceMap>,
    opts: &CompileOpts,
    diagnostics: &'a [solar_interface::diagnostics::Diag],
    out: &mut (dyn Write + Send),
) -> io::Result<()> {
    let mut emitter = JsonEmitter::new(Box::new(io::sink()), source_map, opts.color)
        .ui_testing(opts.unstable.ui_testing)
        .human_kind(opts.error_format_human)
        .terminal_width(opts.diagnostic_width);
    output.errors =
        diagnostics.iter().map(|diagnostic| emitter.solc_diagnostic(diagnostic)).collect();

    if output.errors.iter().any(SolcDiagnostic::is_error) {
        output.contracts.clear();
    }

    crate::emit::to_json(out, &output, opts.pretty_json).map_err(Into::into)
}

/// Applies `settings.debug`, enforcing solc's selection rules.
///
/// `debugInfo` components only affect Yul IR output, which we do not emit, so only the rules
/// that solc rejects inputs for are checked: `snippet` requires `location`, and an explicit
/// selection has to include `ethdebug` when ethdebug bytecode output is requested.
fn apply_debug_settings(
    dcx: &DiagCtxt,
    opts: &mut CompileOpts,
    debug: &DebugSettings,
    output_selection: &OutputSelection<'_>,
) {
    match debug.revert_strings {
        Some(RevertStrings::VerboseDebug) => {
            dcx.err(
                "only `default`, `strip` and `debug` are implemented for `settings.debug.revertStrings` for now",
            )
            .emit();
        }
        Some(revert_strings) => opts.revert_strings = revert_strings,
        None => {}
    }
    if debug.debug_info.is_none() {
        return;
    }
    if debug.selects_debug_info(DebugInfoComponent::Snippet)
        && !debug.selects_debug_info(DebugInfoComponent::Location)
    {
        dcx.err("to use `snippet` with `settings.debug.debugInfo` you must also select `location`")
            .emit();
    }
    let ethdebug_outputs =
        OutputSelectionFlags::BYTECODE_ETHDEBUG | OutputSelectionFlags::DEPLOYED_BYTECODE_ETHDEBUG;
    if output_selection.union().intersects(ethdebug_outputs)
        && !debug.selects_debug_info(DebugInfoComponent::Ethdebug)
    {
        dcx.err(
            "`ethdebug` needs to be enabled in `settings.debug.debugInfo` if `evm.bytecode.ethdebug` or `evm.deployedBytecode.ethdebug` was selected as output",
        )
        .emit();
    }
}

fn compile(
    input: CompilerInput<'_>,
    opts: &mut CompileOpts,
    source_map: Arc<SourceMap>,
    dcx: DiagCtxt,
    diagnostics: &solar_data_structures::sync::RwLock<Vec<solar_interface::diagnostics::Diag>>,
    out: &mut (dyn Write + Send),
) -> io::Result<()> {
    let CompilerInput { language, sources, settings } = input;
    // Destructure `Settings` so every recognized field is handled explicitly;
    // fields we don't act on yet are bound with a leading underscore and a note.
    // Adding a field to `Settings` then forces a decision here instead of it
    // being silently ignored.
    let Settings {
        remappings,
        output_selection,
        stop_after,
        evm_version,
        optimizer,
        metadata,
        libraries,
        debug,
    } = &settings;

    if !metadata.append_cbor
        && metadata.bytecode_hash.is_explicit
        && metadata.bytecode_hash.value != MetadataHash::None
    {
        dcx.err("when `settings.metadata.appendCBOR` is false, `bytecodeHash` must be `none`")
            .emit();
        return write_empty_standard_json_output(source_map, opts, diagnostics, out);
    }

    let mut parsed_remappings = Vec::with_capacity(remappings.len());
    for remapping in remappings {
        match remapping.parse::<ImportRemapping>() {
            Ok(remapping) => parsed_remappings.push(remapping),
            Err(e) => {
                dcx.err(format!("invalid remapping `{remapping}`: {e}")).emit();
            }
        }
    }
    if dcx.has_errors().is_err() {
        return write_empty_standard_json_output(source_map, opts, diagnostics, out);
    }

    opts.import_remappings = parsed_remappings;
    if let Some(version) = evm_version.as_deref() {
        match EvmVersion::from_str(version) {
            Ok(version) => opts.evm_version = version,
            Err(_) => {
                dcx.err(format!("invalid EVM version `{version}`")).emit();
            }
        }
    }
    opts.language = match language.as_ref() {
        "Solidity" | "solidity" => Language::Solidity,
        "Yul" | "yul" => Language::Yul,
        language => {
            dcx.err(format!("unsupported language `{language}`")).emit();
            return write_empty_standard_json_output(source_map, opts, diagnostics, out);
        }
    };
    if let Some(stage) = stop_after.as_deref() {
        match CompilerStage::from_str(stage) {
            Ok(stage) => opts.stop_after = Some(stage),
            Err(_) => {
                dcx.err(format!("invalid compiler stage `{stage}`")).emit();
            }
        }
    }
    // Like solc, Standard JSON never inherits the command line's `--revert-strings`; only
    // `settings.debug.revertStrings` selects a non-default mode.
    opts.revert_strings = RevertStrings::Default;
    if let Some(debug) = debug {
        apply_debug_settings(&dcx, opts, debug, output_selection);
    }
    if dcx.has_errors().is_err() {
        return write_empty_standard_json_output(source_map, opts, diagnostics, out);
    }

    let (optimizer_enabled, optimizer_runs) = optimizer_settings(optimizer.as_ref());
    // Treat lower run counts as size optimization.
    opts.optimization = if optimizer_enabled {
        if optimizer_runs >= 200 { OptimizationMode::Gas } else { OptimizationMode::Size }
    } else {
        OptimizationMode::None
    };
    opts.optimizer_runs = optimizer_enabled.then_some(optimizer_runs);

    opts.libraries = Vec::with_capacity(libraries.len());
    for (source, libraries) in &libraries.0 {
        opts.libraries.extend(libraries.iter().map(|(name, &address)| LibraryAddress {
            source: (!source.is_empty()).then(|| source.to_string()),
            name: name.to_string(),
            address,
        }));
    }
    opts.input = sources.keys().map(ToString::to_string).collect();

    let sess = solar_interface::Session::builder()
        .source_map(Arc::clone(&source_map))
        .dcx(dcx)
        .opts(opts.clone())
        .build();

    let mut output_result = None;
    let _ = crate::commands::compile::run_compiler_session_with(
        sess,
        |compiler| {
            let mut output = CompilerOutput::default();
            let result = (|| {
                let control_flow = crate::commands::compile::run_pipeline(
                    compiler,
                    |pcx| {
                        let mut files = Vec::<(PathBuf, String)>::with_capacity(sources.len());
                        for (name, source) in sources {
                            let Some(content) = source.content else {
                                let message = if source.urls.is_empty() {
                                    format!("source `{name}` is missing `content`")
                                } else {
                                    format!("source URLs are not supported for `{name}`")
                                };
                                return Err(pcx.dcx().err(message).emit());
                            };
                            files.push((PathBuf::from(name.as_ref()), content.into()));
                        }
                        pcx.par_load_files_with_contents(files)
                    },
                    |compiler| output.sources = source_outputs_from_compiler(compiler),
                )?;
                if control_flow.is_break() {
                    return Ok(());
                }

                let gcx = compiler.gcx();
                let bytecode_contracts = requested_bytecode_contracts(gcx, output_selection);
                let debug_info_contracts = requested_debug_info_contracts(gcx, output_selection);
                let needs_metadata = output_selection.requests_metadata()
                    || metadata.append_cbor && !bytecode_contracts.is_empty();
                let contract_metadata = needs_metadata.then(|| Metadata::new(gcx, &settings));

                let source_map_outputs = OutputSelectionFlags::BYTECODE_SOURCE_MAP
                    | OutputSelectionFlags::DEPLOYED_BYTECODE_SOURCE_MAP;
                let source_map_encoder =
                    contract_output_requested(gcx, output_selection, source_map_outputs)
                        .then(|| crate::source_map::SourceMapEncoder::new(gcx));
                crate::commands::compile::warn_experimental_codegen(
                    gcx.sess,
                    !bytecode_contracts.is_empty(),
                );
                let runtime_data = contract_metadata
                    .as_ref()
                    .map(|metadata| |contract_id| metadata.runtime_data(contract_id));
                let runtime_data = runtime_data.as_ref().map(|data| data as &RuntimeDataFn<'_>);
                let bytecodes = crate::emit::emit_requested(
                    compiler,
                    bytecode_contracts,
                    runtime_data,
                    debug_info_contracts,
                )?;

                gcx.dcx().has_errors()?;

                let global_ethdebug = output_selection.global();
                let ethdebug_outputs = OutputSelectionFlags::BYTECODE_ETHDEBUG
                    | OutputSelectionFlags::DEPLOYED_BYTECODE_ETHDEBUG;
                let compilation_requested = !global_ethdebug.is_empty()
                    || contract_output_requested(gcx, output_selection, ethdebug_outputs);
                let compilation = compilation_requested.then(|| make_ethdebug_compilation(gcx));
                let compilation_id = compilation.as_ref().map(ethdebug_compilation_id);

                for (contract_id, contract) in gcx.hir.contracts_enumerated() {
                    let source = gcx.hir.source(contract.source);
                    let source_name = standard_json_source_name(&source.file.name);
                    let contract_name = contract.name.as_str();
                    let contract_selection = output_selection.contract(&source_name, contract_name);
                    let contract_output = make_contract_output(
                        gcx,
                        contract_id,
                        contract_selection,
                        bytecodes.as_ref(),
                        contract_metadata.as_ref(),
                        compilation_id,
                        source_map_encoder.as_ref(),
                    );
                    if !contract_output.is_empty() {
                        output
                            .contracts
                            .entry(source_name)
                            .or_default()
                            .insert(contract_name.to_string(), contract_output);
                    }
                }

                if let Some(compilation) = compilation {
                    let mut ethdebug = EthdebugOutput::default();
                    if global_ethdebug.contains(OutputSelectionFlags::ETHDEBUG_RESOURCES) {
                        ethdebug.resources = Some(EthdebugResources {
                            compilation: compilation.clone(),
                            types: Default::default(),
                            pointers: Default::default(),
                        });
                    }
                    // A program always references its compilation resource. Emit it
                    // even when the caller selected only a per-contract program.
                    if compilation_requested {
                        ethdebug.compilation = Some(compilation);
                    }
                    if ethdebug.resources.is_some() || ethdebug.compilation.is_some() {
                        output.ethdebug = Some(ethdebug);
                    }
                }

                Ok(())
            })();

            let diagnostics = diagnostics.read();
            output_result = Some(finish_standard_json_output(
                &mut output,
                Arc::clone(&source_map),
                opts,
                &diagnostics,
                &mut *out,
            ));
            result
        },
        false,
    );

    output_result
        .unwrap_or_else(|| write_empty_standard_json_output(source_map, opts, diagnostics, out))
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

pub(super) fn standard_json_source_name(name: &solar_interface::source_map::FileName) -> String {
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

fn make_contract_output<'gcx>(
    gcx: Gcx<'gcx>,
    contract_id: solar_sema::hir::ContractId,
    output_selection: OutputSelectionFlags,
    bytecodes: Option<&FxHashMap<ContractId, ContractArtifact>>,
    metadata: Option<&Metadata<'_, '_, 'gcx>>,
    compilation_id: Option<&str>,
    source_map_encoder: Option<&crate::source_map::SourceMapEncoder>,
) -> ContractOutput<'gcx> {
    let mut output = ContractOutput::default();

    if output_selection.contains(OutputSelectionFlags::ABI) {
        output.abi = Some(gcx.contract_abi(contract_id));
    }
    if output_selection.contains(OutputSelectionFlags::METADATA) {
        output.metadata = Some(
            metadata
                .expect("metadata cache must exist when metadata is requested")
                .json(contract_id)
                .to_string(),
        );
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
    let artifact = bytecodes.and_then(|bytecodes| bytecodes.get(&contract_id));
    let bytecode_outputs = OutputSelectionFlags::BYTECODE_OBJECT
        | OutputSelectionFlags::BYTECODE_OPCODES
        | OutputSelectionFlags::BYTECODE_SOURCE_MAP
        | OutputSelectionFlags::BYTECODE_LINK_REFERENCES
        | OutputSelectionFlags::BYTECODE_ETHDEBUG;
    if output_selection.intersects(bytecode_outputs) {
        evm.bytecode = Some(make_bytecode_output(
            gcx,
            contract_id,
            artifact,
            output_selection,
            compilation_id,
            source_map_encoder,
            false,
        ));
    }
    let deployed_bytecode_outputs = OutputSelectionFlags::DEPLOYED_BYTECODE_OBJECT
        | OutputSelectionFlags::DEPLOYED_BYTECODE_OPCODES
        | OutputSelectionFlags::DEPLOYED_BYTECODE_SOURCE_MAP
        | OutputSelectionFlags::DEPLOYED_BYTECODE_LINK_REFERENCES
        | OutputSelectionFlags::DEPLOYED_BYTECODE_IMMUTABLE_REFERENCES
        | OutputSelectionFlags::DEPLOYED_BYTECODE_ETHDEBUG;
    if output_selection.intersects(deployed_bytecode_outputs) {
        evm.deployed_bytecode = Some(make_bytecode_output(
            gcx,
            contract_id,
            artifact,
            output_selection,
            compilation_id,
            source_map_encoder,
            true,
        ));
    }
    if !evm.is_empty() {
        output.evm = Some(evm);
    }

    output
}

fn make_bytecode_output(
    gcx: Gcx<'_>,
    contract_id: ContractId,
    artifact: Option<&ContractArtifact>,
    output_selection: OutputSelectionFlags,
    compilation_id: Option<&str>,
    source_map_encoder: Option<&crate::source_map::SourceMapEncoder>,
    deployed: bool,
) -> BytecodeOutput {
    let object_flag = if deployed {
        OutputSelectionFlags::DEPLOYED_BYTECODE_OBJECT
    } else {
        OutputSelectionFlags::BYTECODE_OBJECT
    };
    let opcodes_flag = if deployed {
        OutputSelectionFlags::DEPLOYED_BYTECODE_OPCODES
    } else {
        OutputSelectionFlags::BYTECODE_OPCODES
    };
    let link_references_flag = if deployed {
        OutputSelectionFlags::DEPLOYED_BYTECODE_LINK_REFERENCES
    } else {
        OutputSelectionFlags::BYTECODE_LINK_REFERENCES
    };
    let source_map_flag = if deployed {
        OutputSelectionFlags::DEPLOYED_BYTECODE_SOURCE_MAP
    } else {
        OutputSelectionFlags::BYTECODE_SOURCE_MAP
    };
    let ethdebug_flag = if deployed {
        OutputSelectionFlags::DEPLOYED_BYTECODE_ETHDEBUG
    } else {
        OutputSelectionFlags::BYTECODE_ETHDEBUG
    };
    let bytecode =
        artifact.map(|artifact| if deployed { &artifact.runtime } else { &artifact.deployment });

    let mut output = BytecodeOutput::default();
    if output_selection.contains(object_flag) {
        let references = artifact.map_or(&[][..], |artifact| {
            if deployed {
                &artifact.runtime_link_references
            } else {
                &artifact.deployment_link_references
            }
        });
        output.object =
            Some(MaybeHexBytecode::new(bytecode.cloned().unwrap_or_default(), references));
    }
    if output_selection.contains(opcodes_flag) {
        output.opcodes = Some(solar_codegen::backend::evm::disassemble_standard_json(
            bytecode.map_or(&[], |bytecode| bytecode.as_ref()),
            gcx.sess.opts.evm_version,
        ));
    }
    if output_selection.contains(source_map_flag) {
        let debug_info = artifact.and_then(|artifact| {
            if deployed {
                artifact.runtime_debug_info.as_deref()
            } else {
                artifact.deployment_debug_info.as_deref()
            }
        });
        output.source_map = Some(match (source_map_encoder, debug_info) {
            (Some(encoder), Some(info)) => {
                encoder.encode(gcx, bytecode.map_or(&[], |bytecode| bytecode.as_ref()), info)
            }
            _ => String::new(),
        });
    }
    if output_selection.contains(link_references_flag) {
        let references = artifact.into_iter().flat_map(|artifact| {
            if deployed {
                artifact.runtime_link_references.iter()
            } else {
                artifact.deployment_link_references.iter()
            }
        });
        let mut by_source = FxIndexMap::<String, FxIndexMap<String, Vec<OffsetLength>>>::default();
        for reference in references {
            by_source
                .entry(reference.source.clone())
                .or_default()
                .entry(reference.name.clone())
                .or_default()
                .push(OffsetLength { start: reference.start, length: 20 });
        }
        output.link_references = Some(by_source);
    }
    if output_selection.contains(ethdebug_flag)
        && let (Some(artifact), Some(compilation_id)) = (artifact, compilation_id)
    {
        output.ethdebug =
            make_ethdebug_program(gcx, contract_id, artifact, compilation_id, deployed);
    }
    if deployed
        && output_selection.contains(OutputSelectionFlags::DEPLOYED_BYTECODE_IMMUTABLE_REFERENCES)
    {
        let mut references = artifact
            .into_iter()
            .flat_map(|artifact| artifact.immutable_references.iter())
            .map(|reference| (gcx.hir.global_item_id(reference.variable_id) as u64, reference))
            .collect::<Vec<_>>();
        references.sort_unstable_by_key(|(ast_id, reference)| (*ast_id, reference.start));

        let mut by_ast_id = FxIndexMap::<String, Vec<OffsetLength>>::default();
        for (ast_id, reference) in references {
            by_ast_id.entry(ast_id.to_string()).or_default().push(OffsetLength {
                start: reference.start,
                length: usize::from(reference.type_size.bytes()),
            });
        }
        output.immutable_references = Some(by_ast_id);
    }
    output
}

fn make_ethdebug_compilation(gcx: Gcx<'_>) -> EthdebugCompilation {
    let language = if gcx.sess.opts.language.is_yul() { "Yul" } else { "Solidity" };
    let sources = gcx
        .hir
        .source_ids()
        .map(|source_id| {
            let source = gcx.hir.source(source_id);
            EthdebugSource {
                id: EthdebugId::Number(source_id.index() as u32),
                path: standard_json_source_name(&source.file.name),
                contents: source.file.src.as_ref().clone(),
                language: language.to_owned(),
            }
        })
        .collect::<Vec<_>>();

    let version = solar_config::version::SEMVER_VERSION.to_owned();
    let mut identity = String::from("ethdebug-solar-compilation-v1");
    // A compilation ID names the complete source-to-bytecode context, not only
    // the source files. Include every code-generation setting that can change
    // instruction offsets or operations so programs cannot cross-reference a
    // different artifact accidentally.
    append_length_prefixed(&mut identity, solar_config::version::SHORT_VERSION);
    append_length_prefixed(&mut identity, &format!("{:?}", gcx.sess.opts.language));
    append_length_prefixed(&mut identity, &format!("{:?}", gcx.sess.opts.evm_version));
    append_length_prefixed(&mut identity, &format!("{:?}", gcx.sess.opts.optimization));
    append_length_prefixed(
        &mut identity,
        &gcx.sess.opts.optimizer_runs.map_or_else(|| "none".to_owned(), |runs| runs.to_string()),
    );
    let mut remappings =
        gcx.sess.opts.import_remappings.iter().map(ToString::to_string).collect::<Vec<_>>();
    remappings.sort_unstable();
    append_length_prefixed(&mut identity, &remappings.len().to_string());
    for remapping in remappings {
        append_length_prefixed(&mut identity, &remapping);
    }
    let mut libraries = gcx.sess.opts.libraries.iter().map(ToString::to_string).collect::<Vec<_>>();
    libraries.sort_unstable();
    append_length_prefixed(&mut identity, &libraries.len().to_string());
    for library in libraries {
        append_length_prefixed(&mut identity, &library);
    }
    append_length_prefixed(&mut identity, &format!("{:?}", gcx.sess.opts.unstable.mir_pipeline));
    append_length_prefixed(&mut identity, &format!("{:?}", gcx.sess.opts.unstable.evm_ir_pipeline));
    append_length_prefixed(&mut identity, &format!("{:?}", gcx.sess.opts.unstable.switch_lowering));
    append_length_prefixed(
        &mut identity,
        &format!(
            "{:?}:{:?}:{:?}",
            gcx.sess.opts.unstable.switch_max_gas_code_growth,
            gcx.sess.opts.unstable.switch_max_bit_slice_gas_code_growth,
            gcx.sess.opts.unstable.codegen_all_functions,
        ),
    );
    append_length_prefixed(&mut identity, &version);
    append_length_prefixed(&mut identity, &sources.len().to_string());
    for source in &sources {
        let EthdebugId::Number(id) = source.id else { unreachable!() };
        append_length_prefixed(&mut identity, &id.to_string());
        append_length_prefixed(&mut identity, &source.path);
        append_length_prefixed(&mut identity, &source.contents);
        append_length_prefixed(&mut identity, &source.language);
    }
    let digest = alloy_primitives::keccak256(identity.as_bytes());
    let id = format!("solar-{}", alloy_primitives::hex::encode(digest.as_slice()));

    EthdebugCompilation {
        id: EthdebugId::Text(id),
        compiler: EthdebugCompiler { name: "solar".to_owned(), version },
        sources,
    }
}

fn append_length_prefixed(output: &mut String, value: &str) {
    output.push_str(&value.len().to_string());
    output.push(':');
    output.push_str(value);
}

fn ethdebug_compilation_id(compilation: &EthdebugCompilation) -> &str {
    let EthdebugId::Text(id) = &compilation.id else { unreachable!() };
    id
}

fn make_ethdebug_program(
    gcx: Gcx<'_>,
    contract_id: ContractId,
    artifact: &ContractArtifact,
    compilation_id: &str,
    deployed: bool,
) -> Option<EthdebugProgram> {
    let debug_info = if deployed {
        artifact.runtime_debug_info.as_ref()?
    } else {
        artifact.deployment_debug_info.as_ref()?
    };
    let bytecode = if deployed { artifact.runtime.as_ref() } else { artifact.deployment.as_ref() };
    let contract = gcx.hir.contract(contract_id);
    let source_ids = gcx
        .hir
        .source_ids()
        .map(|source_id| (gcx.hir.source(source_id).file.start_pos.0, source_id.index() as u32))
        .collect::<FxHashMap<_, _>>();
    let definition_range = gcx
        .sess
        .source_map()
        .span_to_range(contract.span)
        .ok()
        .map(|range| EthdebugRange { offset: range.start, length: range.end - range.start });

    let instructions = debug_info
        .iter()
        .enumerate()
        .map(|(index, instruction)| {
            let mnemonic = solar_codegen::backend::evm::opcode_mnemonic(instruction.opcode)
                .expect("assembled opcode should have a mnemonic")
                .to_ascii_uppercase();
            let arguments = push_argument(bytecode, instruction)
                .map(|argument| format!("0x{}", alloy_primitives::hex::encode(argument)))
                .into_iter()
                .collect();
            EthdebugInstruction {
                offset: instruction.offset as usize,
                operation: EthdebugOperation { mnemonic, arguments },
                context: make_ethdebug_context(
                    gcx,
                    &source_ids,
                    bytecode,
                    debug_info.get(index.wrapping_sub(1)),
                    instruction,
                ),
            }
        })
        .collect();

    Some(EthdebugProgram {
        compilation: EthdebugReference { id: EthdebugId::Text(compilation_id.to_owned()) },
        contract: EthdebugContract {
            name: contract.name.to_string(),
            definition: EthdebugSourceRange {
                source: EthdebugReference {
                    id: EthdebugId::Number(contract.source.index() as u32),
                },
                range: definition_range,
            },
        },
        environment: if deployed { EthdebugEnvironment::Call } else { EthdebugEnvironment::Create },
        instructions,
    })
}

fn push_argument<'a>(bytecode: &'a [u8], instruction: &DebugInstruction) -> Option<&'a [u8]> {
    let width = instruction.opcode.checked_sub(0x5f)? as usize;
    if !(1..=32).contains(&width) {
        return None;
    }
    let start = instruction.offset as usize + 1;
    let end = start.checked_add(width)?;
    bytecode.get(start..end)
}

fn make_ethdebug_context(
    gcx: Gcx<'_>,
    source_ids: &FxHashMap<u32, u32>,
    bytecode: &[u8],
    previous: Option<&DebugInstruction>,
    instruction: &DebugInstruction,
) -> Option<EthdebugContext> {
    let mut contexts = instruction
        .source_spans
        .iter()
        .filter_map(|&span| make_ethdebug_source_range(gcx, source_ids, span))
        .map(|code| EthdebugContext {
            code: Some(code),
            pick: Vec::new(),
            invoke: None,
            r#return: None,
            revert: None,
        })
        .collect::<Vec<_>>();
    let (code, pick) = match contexts.len() {
        0 => (None, Vec::new()),
        1 => (contexts.pop().and_then(|context| context.code), Vec::new()),
        _ => (None, contexts),
    };
    let invoke = instruction.function_invoke.and_then(|function| {
        make_ethdebug_function_invoke(gcx, source_ids, bytecode, previous, function, instruction)
    });
    let (r#return, revert) = match instruction.function_exit {
        Some(DebugFunctionExit::Return) => (Some(EthdebugFunctionExit {}), None),
        Some(DebugFunctionExit::Revert) => (None, Some(EthdebugFunctionExit {})),
        None => (None, None),
    };
    if code.is_none()
        && pick.is_empty()
        && invoke.is_none()
        && r#return.is_none()
        && revert.is_none()
    {
        None
    } else {
        Some(EthdebugContext { code, pick, invoke, r#return, revert })
    }
}

fn make_ethdebug_function_invoke(
    gcx: Gcx<'_>,
    source_ids: &FxHashMap<u32, u32>,
    bytecode: &[u8],
    previous: Option<&DebugInstruction>,
    function: DebugFunction,
    instruction: &DebugInstruction,
) -> Option<EthdebugFunctionInvoke> {
    let target = crate::source_map::static_jump_target(bytecode, previous, instruction)
        .or_else(|| (instruction.opcode == 0x5b).then_some(instruction.offset as usize))
        .map(|target| EthdebugInvocationTarget {
            pointer: EthdebugCodePointer { location: "code", offset: target, length: 1 },
        });
    Some(EthdebugFunctionInvoke {
        identifier: (function.identifier != solar_interface::sym::_anonymous)
            .then(|| function.identifier.to_string()),
        declaration: make_ethdebug_source_range(gcx, source_ids, function.declaration)?,
        jump: true,
        target,
    })
}

fn make_ethdebug_source_range(
    gcx: Gcx<'_>,
    source_ids: &FxHashMap<u32, u32>,
    span: solar_interface::Span,
) -> Option<EthdebugSourceRange> {
    let source = gcx.sess.source_map().span_to_source(span).ok()?;
    let source_id = *source_ids.get(&source.file.start_pos.0)?;
    Some(EthdebugSourceRange {
        source: EthdebugReference { id: EthdebugId::Number(source_id) },
        range: Some(EthdebugRange {
            offset: source.data.start,
            length: source.data.end - source.data.start,
        }),
    })
}

fn requested_bytecode_contracts(
    gcx: solar_sema::Gcx<'_>,
    output_selection: &OutputSelection<'_>,
) -> ContractSelection {
    let bytecode_outputs = OutputSelectionFlags::BYTECODE
        | OutputSelectionFlags::DEPLOYED_BYTECODE
        | OutputSelectionFlags::BYTECODE_ETHDEBUG
        | OutputSelectionFlags::DEPLOYED_BYTECODE_ETHDEBUG;
    if output_selection.all().intersects(bytecode_outputs) {
        return ContractSelection::All;
    }

    let mut contracts = ContractSelection::empty(gcx);
    for (contract_id, contract) in gcx.hir.contracts_enumerated() {
        if contract.kind.is_interface() || contract.kind.is_abstract_contract() {
            continue;
        }

        let source = gcx.hir.source(contract.source);
        let source_name = standard_json_source_name(&source.file.name);
        let contract_name = contract.name.as_str();
        if output_selection.contract(&source_name, contract_name).intersects(bytecode_outputs) {
            contracts.insert(contract_id);
        }
    }
    contracts
}

fn requested_debug_info_contracts(
    gcx: solar_sema::Gcx<'_>,
    output_selection: &OutputSelection<'_>,
) -> ContractSelection {
    let debug_outputs = OutputSelectionFlags::BYTECODE_ETHDEBUG
        | OutputSelectionFlags::DEPLOYED_BYTECODE_ETHDEBUG
        | OutputSelectionFlags::BYTECODE_SOURCE_MAP
        | OutputSelectionFlags::DEPLOYED_BYTECODE_SOURCE_MAP;
    if output_selection.all().intersects(debug_outputs) {
        return ContractSelection::All;
    }

    let mut contracts = ContractSelection::empty(gcx);
    for (contract_id, contract) in gcx.hir.contracts_enumerated() {
        if contract.kind.is_interface() || contract.kind.is_abstract_contract() {
            continue;
        }

        let source = gcx.hir.source(contract.source);
        let source_name = standard_json_source_name(&source.file.name);
        if output_selection.contract(&source_name, contract.name.as_str()).intersects(debug_outputs)
        {
            contracts.insert(contract_id);
        }
    }
    contracts
}

fn contract_output_requested(
    gcx: solar_sema::Gcx<'_>,
    output_selection: &OutputSelection<'_>,
    outputs: OutputSelectionFlags,
) -> bool {
    output_selection.all().intersects(outputs)
        || gcx.hir.contracts().any(|contract| {
            let source = gcx.hir.source(contract.source);
            let source_name = standard_json_source_name(&source.file.name);
            output_selection.contract(&source_name, contract.name.as_str()).intersects(outputs)
        })
}
