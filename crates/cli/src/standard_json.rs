use alloy_primitives::Bytes;
use indexmap::IndexMap;
use serde::{
    Deserialize, Serialize,
    de::{self, SeqAccess, Visitor},
};
use serde_json::{Map, Value, json};
use solar_codegen::{EvmCodegen, lower};
use solar_config::{
    CompileOpts, CompilerStage, EvmVersion, ImportRemapping, Language, OptimizationMode,
};
use solar_data_structures::map::{FxBuildHasher, FxHashMap, FxHashSet};
use solar_interface::{
    Result, SourceMap,
    diagnostics::{DiagCtxt, InMemoryEmitter, JsonEmitter, SolcDiagnostic},
    source_map::FileLoader,
};
use solar_sema::{
    Gcx,
    hir::ContractId,
    output::{Documentation, StorageLayoutOutput},
};
use std::{
    borrow::{Borrow, Cow},
    fmt,
    fs::File,
    io::{self, Read, Write},
    ops::Deref,
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
};

type FxIndexMap<K, V> = IndexMap<K, V, FxBuildHasher>;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompilerInput<'a> {
    #[serde(default = "default_language")]
    language: CowStr<'a>,
    #[serde(borrow, default)]
    sources: FxIndexMap<CowStr<'a>, SourceInput<'a>>,
    #[serde(borrow, default)]
    settings: Settings<'a>,
    // `auxiliaryInput` is only used by solc's SMT checker, which we do not support.
    // #[serde(borrow, default)]
    // auxiliary_input: Option<CowValue<'a>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SourceInput<'a> {
    #[serde(borrow)]
    content: Option<CowStr<'a>>,
    #[serde(borrow, default)]
    urls: Vec<CowStr<'a>>,
    // `keccak256` validation is not supported yet.
    // #[serde(borrow)]
    // keccak256: Option<CowValue<'a>>,
    // AST inputs are not supported yet.
    // #[serde(borrow)]
    // ast: Option<CowValue<'a>>,
    // EVM assembly inputs are not supported yet.
    // #[serde(borrow)]
    // assembly_json: Option<CowValue<'a>>,
}

// The supported subset of solc's Standard JSON `settings` object.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Settings<'a> {
    #[serde(borrow, default)]
    remappings: Vec<CowStr<'a>>,
    #[serde(borrow, default)]
    output_selection: OutputSelection<'a>,
    #[serde(borrow)]
    stop_after: Option<CowStr<'a>>,
    #[serde(borrow)]
    evm_version: Option<CowStr<'a>>,
    /// Optimizer settings. Only `enabled` is currently honored.
    #[serde(default)]
    optimizer: Option<Optimizer>,
    // Metadata settings are ignored because bytecode metadata is not emitted.
    // #[serde(borrow, default)]
    // metadata: Option<CowValue<'a>>,
    // Library addresses are ignored because linking is not supported.
    // #[serde(borrow, default)]
    // libraries: Option<CowValue<'a>>,
    // Debug settings are ignored because we do not emit debug output.
    // #[serde(borrow, default)]
    // debug: Option<CowValue<'a>>,
    // Experimental features are ignored because we do not support solc's experimental mode.
    // #[serde(borrow, default)]
    // experimental: Option<CowValue<'a>>,
    // Model checker settings are ignored because we do not run an SMT checker.
    // #[serde(borrow, default)]
    // model_checker: Option<CowValue<'a>>,
    // The IR pipeline is ignored because we have a single compilation pipeline.
    // #[serde(default)]
    // via_ir: Option<bool>,
    // The SSA CFG pipeline is ignored because we have a single compilation pipeline.
    // #[serde(default)]
    // via_ssa_cfg: Option<bool>,
}

/// The solc Standard JSON `settings.optimizer` object.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Optimizer {
    /// Whether the optimizer is enabled. Mapped onto [`OptimizationMode::None`]
    /// when disabled.
    #[serde(default)]
    enabled: bool,
    /// Number of optimizer runs. The MIR optimizer has no runs parameter yet.
    #[serde(default)]
    runs: Option<u64>,
    // Fine-grained optimizer settings are not supported yet.
    // #[serde(borrow, default)]
    // details: Option<CowValue<'a>>,
}

#[derive(Debug, Default, Deserialize)]
struct OutputSelection<'a>(
    #[serde(borrow)] FxIndexMap<CowStr<'a>, FxIndexMap<CowStr<'a>, OutputSelectionFlags>>,
);

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    struct OutputSelectionFlags: u64 {
        const AST = 1 << 0;
        const ABI = 1 << 1;
        const METADATA = 1 << 2;
        const USERDOC = 1 << 3;
        const DEVDOC = 1 << 4;
        const STORAGE_LAYOUT = 1 << 5;
        const TRANSIENT_STORAGE_LAYOUT = 1 << 6;
        const IR = 1 << 7;
        const IR_AST = 1 << 8;
        const IR_OPTIMIZED = 1 << 9;
        const IR_OPTIMIZED_AST = 1 << 10;
        const YUL_CFG_JSON = 1 << 11;
        const ASSEMBLY = 1 << 12;
        const LEGACY_ASSEMBLY = 1 << 13;
        const METHOD_IDENTIFIERS = 1 << 14;
        const GAS_ESTIMATES = 1 << 15;
        const BYTECODE_OBJECT = 1 << 16;
        const BYTECODE_OPCODES = 1 << 17;
        const BYTECODE_SOURCE_MAP = 1 << 18;
        const BYTECODE_FUNCTION_DEBUG_DATA = 1 << 19;
        const BYTECODE_GENERATED_SOURCES = 1 << 20;
        const BYTECODE_LINK_REFERENCES = 1 << 21;
        const BYTECODE_ETHDEBUG = 1 << 22;
        const DEPLOYED_BYTECODE_OBJECT = 1 << 23;
        const DEPLOYED_BYTECODE_OPCODES = 1 << 24;
        const DEPLOYED_BYTECODE_SOURCE_MAP = 1 << 25;
        const DEPLOYED_BYTECODE_FUNCTION_DEBUG_DATA = 1 << 26;
        const DEPLOYED_BYTECODE_GENERATED_SOURCES = 1 << 27;
        const DEPLOYED_BYTECODE_LINK_REFERENCES = 1 << 28;
        const DEPLOYED_BYTECODE_IMMUTABLE_REFERENCES = 1 << 29;
        const DEPLOYED_BYTECODE_ETHDEBUG = 1 << 30;
        const ETHDEBUG_RESOURCES = 1 << 31;
        const ETHDEBUG_COMPILATION = 1 << 32;

        const YUL = Self::IR.bits()
            | Self::IR_AST.bits()
            | Self::IR_OPTIMIZED.bits()
            | Self::IR_OPTIMIZED_AST.bits()
            | Self::YUL_CFG_JSON.bits();
        const BYTECODE = Self::BYTECODE_OBJECT.bits()
            | Self::BYTECODE_OPCODES.bits()
            | Self::BYTECODE_SOURCE_MAP.bits()
            | Self::BYTECODE_FUNCTION_DEBUG_DATA.bits()
            | Self::BYTECODE_GENERATED_SOURCES.bits()
            | Self::BYTECODE_LINK_REFERENCES.bits();
        const DEPLOYED_BYTECODE = Self::DEPLOYED_BYTECODE_OBJECT.bits()
            | Self::DEPLOYED_BYTECODE_OPCODES.bits()
            | Self::DEPLOYED_BYTECODE_SOURCE_MAP.bits()
            | Self::DEPLOYED_BYTECODE_FUNCTION_DEBUG_DATA.bits()
            | Self::DEPLOYED_BYTECODE_GENERATED_SOURCES.bits()
            | Self::DEPLOYED_BYTECODE_LINK_REFERENCES.bits()
            | Self::DEPLOYED_BYTECODE_IMMUTABLE_REFERENCES.bits();
        const EVM = Self::ASSEMBLY.bits()
            | Self::LEGACY_ASSEMBLY.bits()
            | Self::METHOD_IDENTIFIERS.bits()
            | Self::GAS_ESTIMATES.bits()
            | Self::BYTECODE.bits()
            | Self::DEPLOYED_BYTECODE.bits();
        const ETHDEBUG = Self::BYTECODE_ETHDEBUG.bits()
            | Self::DEPLOYED_BYTECODE_ETHDEBUG.bits()
            | Self::ETHDEBUG_RESOURCES.bits()
            | Self::ETHDEBUG_COMPILATION.bits();
        const SOURCE = Self::AST.bits();
        const CONTRACT = Self::ABI.bits()
            | Self::METADATA.bits()
            | Self::USERDOC.bits()
            | Self::DEVDOC.bits()
            | Self::STORAGE_LAYOUT.bits()
            | Self::TRANSIENT_STORAGE_LAYOUT.bits()
            | Self::YUL.bits()
            | Self::EVM.bits()
            | Self::BYTECODE_ETHDEBUG.bits()
            | Self::DEPLOYED_BYTECODE_ETHDEBUG.bits();
        const GLOBAL = Self::ETHDEBUG_RESOURCES.bits() | Self::ETHDEBUG_COMPILATION.bits();
        const WILDCARD = Self::SOURCE.bits()
            | Self::ABI.bits()
            | Self::METADATA.bits()
            | Self::USERDOC.bits()
            | Self::DEVDOC.bits()
            | Self::STORAGE_LAYOUT.bits()
            | Self::TRANSIENT_STORAGE_LAYOUT.bits()
            | Self::EVM.bits();
    }
}

#[derive(Debug, Default, Serialize)]
struct CompilerOutput<'a> {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    errors: Vec<SolcDiagnostic<'a>>,
    #[serde(default, skip_serializing_if = "FxIndexMap::is_empty")]
    sources: FxIndexMap<String, SourceOutput>,
    #[serde(default, skip_serializing_if = "FxIndexMap::is_empty")]
    contracts: FxIndexMap<String, FxIndexMap<String, ContractOutput>>,
    // Global `ethdebug` output is not supported yet.
    // #[serde(skip_serializing_if = "Option::is_none")]
    // ethdebug: Option<CowValue<'static>>,
}

/// Result returned by a Standard JSON read callback.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadCallbackResult {
    /// The requested data was found.
    Success(String),
    /// The callback handled the request and returned an error.
    Error(String),
    /// The callback does not support this request kind.
    Unsupported,
}

/// Callback used by Standard JSON compilation to retrieve extra input.
pub trait StandardJsonReadCallback: Send + Sync + 'static {
    /// Reads data for `kind`.
    ///
    /// The modern soljson API currently uses `source` for import resolution.
    fn read(&self, kind: &str, data: &str) -> ReadCallbackResult;
}

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

    let mut emitter = JsonEmitter::new(Box::new(io::sink()), Arc::clone(&source_map))
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
            let compile_result = crate::commands::compile::run_pipeline(
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
            );

            if compile_result.is_ok() && compiler.dcx().has_errors().is_ok() {
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

                for (contract_id, contract) in gcx.hir.contracts_enumerated() {
                    let source = gcx.hir.source(contract.source);
                    let source_name = standard_json_source_name(&source.file.name);
                    let contract_name = contract.name.to_string();
                    let contract_selection =
                        output_selection.contract(&source_name, &contract_name);
                    let contract_output = make_contract_output(
                        gcx,
                        contract_id,
                        contract_selection,
                        bytecodes.as_ref(),
                    );
                    if !contract_output.is_empty() {
                        output
                            .contracts
                            .entry(source_name)
                            .or_default()
                            .insert(contract_name, contract_output);
                    }
                }
            }

            compile_result.map(|_| ())
        },
        false,
    );
}

/// JSON string wrapper that borrows from the standard-json input when possible.
///
/// Serde's generic `Cow<'de, str>` implementation deserializes through the
/// owned representation, so direct `Cow<'de, str>` fields allocate even when
/// the JSON backend can provide `visit_borrowed_str`. `#[serde(borrow)]` on the
/// containing fields is still needed to thread the input lifetime to this type,
/// and this visitor is what selects `Cow::Borrowed` for unescaped strings and
/// `Cow::Owned` when the deserializer has to materialize an escaped string.
///
/// See <https://github.com/serde-rs/serde/issues/1852> and
/// <https://github.com/serde-rs/serde/issues/914>.
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct CowStr<'a>(Cow<'a, str>);

impl CowStr<'_> {
    fn as_cow(&self) -> &Cow<'_, str> {
        &self.0
    }
}

impl AsRef<str> for CowStr<'_> {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Deref for CowStr<'_> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Borrow<str> for CowStr<'_> {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CowStr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<CowStr<'_>> for String {
    fn from(value: CowStr<'_>) -> Self {
        value.0.into_owned()
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for CowStr<'a> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct CowStrVisitor;

        impl<'de> Visitor<'de> for CowStrVisitor {
            type Value = CowStr<'de>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON string")
            }

            fn visit_borrowed_str<E>(self, value: &'de str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(CowStr(Cow::Borrowed(value)))
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(CowStr(Cow::Owned(value.to_string())))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(CowStr(Cow::Owned(value)))
            }
        }

        deserializer.deserialize_str(CowStrVisitor)
    }
}

#[derive(Debug, Serialize)]
struct SourceOutput {
    id: u32,
    // AST output is not supported yet.
    // #[serde(skip_serializing_if = "Option::is_none")]
    // ast: Option<CowValue<'static>>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct ContractOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    abi: Option<Vec<alloy_json_abi::AbiItem<'static>>>,
    // Metadata output is not supported yet.
    // #[serde(skip_serializing_if = "Option::is_none")]
    // metadata: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    userdoc: Option<Documentation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    devdoc: Option<Documentation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    storage_layout: Option<StorageLayoutOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    transient_storage_layout: Option<StorageLayoutOutput>,
    // Yul IR output is not supported yet.
    // #[serde(skip_serializing_if = "Option::is_none")]
    // ir: Option<CowValue<'static>>,
    // Yul IR AST output is not supported yet.
    // #[serde(skip_serializing_if = "Option::is_none")]
    // ir_ast: Option<CowValue<'static>>,
    // Optimized Yul IR output is not supported yet.
    // #[serde(skip_serializing_if = "Option::is_none")]
    // ir_optimized: Option<CowValue<'static>>,
    // Optimized Yul IR AST output is not supported yet.
    // #[serde(skip_serializing_if = "Option::is_none")]
    // ir_optimized_ast: Option<CowValue<'static>>,
    // Yul CFG output is not supported yet.
    // #[serde(rename = "yulCFGJson", skip_serializing_if = "Option::is_none")]
    // yul_cfg_json: Option<CowValue<'static>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    evm: Option<EvmOutput>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct EvmOutput {
    // Assembly output is not supported yet.
    // #[serde(skip_serializing_if = "Option::is_none")]
    // assembly: Option<CowValue<'static>>,
    // Legacy assembly output is not supported yet.
    // #[serde(skip_serializing_if = "Option::is_none")]
    // legacy_assembly: Option<CowValue<'static>>,
    #[serde(default, skip_serializing_if = "FxIndexMap::is_empty")]
    method_identifiers: FxIndexMap<String, String>,
    // Gas estimates are not supported yet.
    // #[serde(skip_serializing_if = "Option::is_none")]
    // gas_estimates: Option<CowValue<'static>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bytecode: Option<BytecodeOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    deployed_bytecode: Option<BytecodeOutput>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct BytecodeOutput {
    #[serde(serialize_with = "serialize_hex_bytes")]
    object: Bytes,
    // Ethdebug output is not supported yet.
    // #[serde(skip_serializing_if = "Option::is_none")]
    // ethdebug: Option<CowValue<'static>>,
    // Function debug data is not supported yet.
    // #[serde(skip_serializing_if = "Option::is_none")]
    // function_debug_data: Option<CowValue<'static>>,
    // Opcode output is not supported yet.
    // #[serde(default, skip_serializing_if = "String::is_empty")]
    // opcodes: String,
    // Source map output is not supported yet.
    // #[serde(default, skip_serializing_if = "String::is_empty")]
    // source_map: String,
    // Link references are not supported yet.
    // #[serde(default, skip_serializing_if = "FxIndexMap::is_empty")]
    // link_references: FxIndexMap<String, FxIndexMap<String, Vec<OffsetLength>>>,
    // Immutable references are not supported yet.
    // #[serde(default, skip_serializing_if = "FxIndexMap::is_empty")]
    // immutable_references: FxIndexMap<String, Vec<OffsetLength>>,
    // Generated sources are not supported yet.
    // #[serde(skip_serializing_if = "Option::is_none")]
    // generated_sources: Option<CowValue<'static>>,
}

// Link and immutable reference offsets are not supported yet.
// #[derive(Debug, Serialize)]
// struct OffsetLength {
//     start: u32,
//     length: u32,
// }

struct GeneratedBytecodes {
    deployment: Bytes,
    runtime: Bytes,
}

impl BytecodeOutput {
    fn empty() -> Self {
        Self::default()
    }

    fn new(object: Bytes) -> Self {
        Self { object }
    }
}

fn serialize_hex_bytes<S>(bytes: &Bytes, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&alloy_primitives::hex::encode(bytes))
}

impl<'input> OutputSelection<'input> {
    fn contract(&self, source: &str, contract: &str) -> OutputSelectionFlags {
        let source_contracts = self.0.get(source);
        let wildcard_contracts = self.0.get("*");
        (contract_flags(source_contracts, contract)
            | contract_flags(source_contracts, "*")
            | contract_flags(wildcard_contracts, contract)
            | contract_flags(wildcard_contracts, "*"))
            & OutputSelectionFlags::CONTRACT
    }
}

impl OutputSelectionFlags {
    fn from_key(key: &str) -> Self {
        match key {
            "*" => Self::WILDCARD,
            "ast" => Self::AST,
            "abi" => Self::ABI,
            "metadata" => Self::METADATA,
            "userdoc" => Self::USERDOC,
            "devdoc" => Self::DEVDOC,
            "storageLayout" => Self::STORAGE_LAYOUT,
            "transientStorageLayout" => Self::TRANSIENT_STORAGE_LAYOUT,
            "ir" => Self::IR,
            "irAst" => Self::IR_AST,
            "irOptimized" => Self::IR_OPTIMIZED,
            "irOptimizedAst" => Self::IR_OPTIMIZED_AST,
            "yulCFGJson" => Self::YUL_CFG_JSON,
            "evm" => Self::EVM,
            "evm.assembly" => Self::ASSEMBLY,
            "evm.legacyAssembly" => Self::LEGACY_ASSEMBLY,
            "evm.methodIdentifiers" => Self::METHOD_IDENTIFIERS,
            "evm.gasEstimates" => Self::GAS_ESTIMATES,
            "evm.bytecode" => Self::BYTECODE,
            "evm.bytecode.object" => Self::BYTECODE_OBJECT,
            "evm.bytecode.opcodes" => Self::BYTECODE_OPCODES,
            "evm.bytecode.sourceMap" => Self::BYTECODE_SOURCE_MAP,
            "evm.bytecode.functionDebugData" => Self::BYTECODE_FUNCTION_DEBUG_DATA,
            "evm.bytecode.generatedSources" => Self::BYTECODE_GENERATED_SOURCES,
            "evm.bytecode.linkReferences" => Self::BYTECODE_LINK_REFERENCES,
            "evm.bytecode.ethdebug" => Self::BYTECODE_ETHDEBUG,
            "evm.deployedBytecode" => Self::DEPLOYED_BYTECODE,
            "evm.deployedBytecode.object" => Self::DEPLOYED_BYTECODE_OBJECT,
            "evm.deployedBytecode.opcodes" => Self::DEPLOYED_BYTECODE_OPCODES,
            "evm.deployedBytecode.sourceMap" => Self::DEPLOYED_BYTECODE_SOURCE_MAP,
            "evm.deployedBytecode.functionDebugData" => Self::DEPLOYED_BYTECODE_FUNCTION_DEBUG_DATA,
            "evm.deployedBytecode.generatedSources" => Self::DEPLOYED_BYTECODE_GENERATED_SOURCES,
            "evm.deployedBytecode.linkReferences" => Self::DEPLOYED_BYTECODE_LINK_REFERENCES,
            "evm.deployedBytecode.immutableReferences" => {
                Self::DEPLOYED_BYTECODE_IMMUTABLE_REFERENCES
            }
            "evm.deployedBytecode.ethdebug" => Self::DEPLOYED_BYTECODE_ETHDEBUG,
            "ethdebug.resources" => Self::ETHDEBUG_RESOURCES,
            "ethdebug.compilation" => Self::ETHDEBUG_COMPILATION,
            _ => Self::empty(),
        }
    }
}

impl<'de> Deserialize<'de> for OutputSelectionFlags {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct OutputSelectionFlagsVisitor;

        impl<'de> Visitor<'de> for OutputSelectionFlagsVisitor {
            type Value = OutputSelectionFlags;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an array of output selection strings")
            }

            fn visit_seq<A>(self, mut seq: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut flags = OutputSelectionFlags::empty();
                while let Some(key) = seq.next_element::<CowStr<'de>>()? {
                    flags |= OutputSelectionFlags::from_key(&key);
                    if flags.is_all() {
                        while seq.next_element::<CowStr<'de>>()?.is_some() {}
                        break;
                    }
                }
                Ok(flags)
            }
        }

        deserializer.deserialize_seq(OutputSelectionFlagsVisitor)
    }
}

fn contract_flags(
    contracts: Option<&FxIndexMap<CowStr<'_>, OutputSelectionFlags>>,
    contract: &str,
) -> OutputSelectionFlags {
    contracts.and_then(|contracts| contracts.get(contract)).copied().unwrap_or_default()
}

fn default_language<'a>() -> CowStr<'a> {
    CowStr(Cow::Borrowed("Solidity"))
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

#[derive(Default)]
struct JsonTreeStats {
    nodes: usize,
    objects: usize,
    arrays: usize,
    strings: usize,
    numbers: usize,
    bools: usize,
    nulls: usize,
    object_entries: usize,
    array_elements: usize,
    object_key_bytes: usize,
    string_bytes: usize,
}

impl JsonTreeStats {
    fn add_value(&mut self, value: &Value) {
        self.nodes += 1;
        match value {
            Value::Null => self.nulls += 1,
            Value::Bool(_) => self.bools += 1,
            Value::Number(_) => self.numbers += 1,
            Value::String(value) => {
                self.strings += 1;
                self.string_bytes += value.len();
            }
            Value::Array(values) => {
                self.arrays += 1;
                self.array_elements += values.len();
                for value in values {
                    self.add_value(value);
                }
            }
            Value::Object(values) => self.add_object(values),
        }
    }

    fn add_object(&mut self, values: &Map<String, Value>) {
        self.objects += 1;
        self.object_entries += values.len();
        for (key, value) in values {
            self.object_key_bytes += key.len();
            self.add_value(value);
        }
    }
}

#[derive(Default)]
struct InputCowStats {
    borrowed: usize,
    borrowed_bytes: usize,
    owned: usize,
    owned_bytes: usize,
}

impl InputCowStats {
    fn add(&mut self, value: &CowStr<'_>) {
        match value.as_cow() {
            Cow::Borrowed(value) => {
                self.borrowed += 1;
                self.borrowed_bytes += value.len();
            }
            Cow::Owned(value) => {
                self.owned += 1;
                self.owned_bytes += value.len();
            }
        }
    }
}

fn print_standard_json_stats(raw_input: &str, input: &CompilerInput<'_>) {
    let mut tree = JsonTreeStats::default();
    match serde_json::from_str::<Value>(raw_input) {
        Ok(value) => tree.add_value(&value),
        Err(error) => {
            eprintln!("standard-json-stats: failed to parse JSON tree: {error}");
            return;
        }
    }

    let mut cows = InputCowStats::default();
    count_input_cows(input, &mut cows);

    let source_content_count =
        input.sources.values().filter(|source| source.content.is_some()).count();
    let source_content_bytes = input
        .sources
        .values()
        .filter_map(|source| source.content.as_ref())
        .map(|content| content.len())
        .sum::<usize>();
    let source_url_count = input.sources.values().map(|source| source.urls.len()).sum::<usize>();

    eprintln!(
        "standard-json-stats: input_bytes={} nodes={} objects={} arrays={} strings={} numbers={} bools={} nulls={} object_entries={} array_elements={} object_key_bytes={} string_bytes={}",
        raw_input.len(),
        tree.nodes,
        tree.objects,
        tree.arrays,
        tree.strings,
        tree.numbers,
        tree.bools,
        tree.nulls,
        tree.object_entries,
        tree.array_elements,
        tree.object_key_bytes,
        tree.string_bytes,
    );
    eprintln!(
        "standard-json-stats: sources={} source_content_count={} source_content_bytes={} source_url_count={} remappings={} output_selection_sources={}",
        input.sources.len(),
        source_content_count,
        source_content_bytes,
        source_url_count,
        input.settings.remappings.len(),
        input.settings.output_selection.0.len(),
    );
    eprintln!(
        "standard-json-stats: cow_borrowed={} cow_borrowed_bytes={} cow_owned={} cow_owned_bytes={}",
        cows.borrowed, cows.borrowed_bytes, cows.owned, cows.owned_bytes,
    );
}

fn count_input_cows(input: &CompilerInput<'_>, stats: &mut InputCowStats) {
    stats.add(&input.language);
    for (name, source) in &input.sources {
        stats.add(name);
        if let Some(content) = &source.content {
            stats.add(content);
        }
        for url in &source.urls {
            stats.add(url);
        }
    }
    for remapping in &input.settings.remappings {
        stats.add(remapping);
    }
    if let Some(stop_after) = &input.settings.stop_after {
        stats.add(stop_after);
    }
    if let Some(evm_version) = &input.settings.evm_version {
        stats.add(evm_version);
    }
    for (source, contracts) in &input.settings.output_selection.0 {
        stats.add(source);
        for contract in contracts.keys() {
            stats.add(contract);
        }
    }
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
        let contract_name = contract.name.to_string();
        output_selection.contract(&source_name, &contract_name).intersects(
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

    for dep in lower::contract_bytecode_dependencies(gcx, contract_id) {
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

impl ContractOutput {
    fn is_empty(&self) -> bool {
        self.abi.is_none()
            && self.userdoc.is_none()
            && self.devdoc.is_none()
            && self.storage_layout.is_none()
            && self.transient_storage_layout.is_none()
            && self.evm.is_none()
    }
}

impl EvmOutput {
    fn is_empty(&self) -> bool {
        self.method_identifiers.is_empty()
            && self.bytecode.is_none()
            && self.deployed_bytecode.is_none()
    }
}

fn strip_json_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;

    while let Some(ch) = chars.next() {
        if in_string {
            out.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        match ch {
            '"' => {
                in_string = true;
                out.push(ch);
            }
            '/' if chars.peek() == Some(&'/') => {
                chars.next();
                for ch in chars.by_ref() {
                    if ch == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut prev = '\0';
                for ch in chars.by_ref() {
                    if ch == '\n' {
                        out.push('\n');
                    }
                    if prev == '*' && ch == '/' {
                        break;
                    }
                    prev = ch;
                }
            }
            _ => out.push(ch),
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selection_flags(input: &str) -> OutputSelectionFlags {
        serde_json::from_str(input).unwrap()
    }

    #[test]
    fn output_selection_exact_keys() {
        let flags = selection_flags(
            r#"[
                "ast",
                "abi",
                "metadata",
                "userdoc",
                "devdoc",
                "storageLayout",
                "transientStorageLayout",
                "ir",
                "irAst",
                "irOptimized",
                "irOptimizedAst",
                "yulCFGJson",
                "evm.assembly",
                "evm.legacyAssembly",
                "evm.methodIdentifiers",
                "evm.gasEstimates",
                "evm.bytecode.object",
                "evm.bytecode.opcodes",
                "evm.bytecode.sourceMap",
                "evm.bytecode.functionDebugData",
                "evm.bytecode.generatedSources",
                "evm.bytecode.linkReferences",
                "evm.bytecode.ethdebug",
                "evm.deployedBytecode.object",
                "evm.deployedBytecode.opcodes",
                "evm.deployedBytecode.sourceMap",
                "evm.deployedBytecode.functionDebugData",
                "evm.deployedBytecode.generatedSources",
                "evm.deployedBytecode.linkReferences",
                "evm.deployedBytecode.immutableReferences",
                "evm.deployedBytecode.ethdebug",
                "ethdebug.resources",
                "ethdebug.compilation"
            ]"#,
        );

        assert_eq!(flags, OutputSelectionFlags::all());
    }

    #[test]
    fn output_selection_parent_keys() {
        assert_eq!(selection_flags(r#"["evm"]"#), OutputSelectionFlags::EVM);
        assert_eq!(
            selection_flags(r#"["evm.bytecode", "evm.deployedBytecode"]"#),
            OutputSelectionFlags::BYTECODE | OutputSelectionFlags::DEPLOYED_BYTECODE
        );
        assert!(!OutputSelectionFlags::EVM.intersects(OutputSelectionFlags::ETHDEBUG));
    }

    #[test]
    fn output_selection_wildcard_and_unknown_keys() {
        assert_eq!(selection_flags(r#"["unknown", "*"]"#), OutputSelectionFlags::WILDCARD);
        assert_eq!(
            selection_flags(r#"["*", "ir", "evm.bytecode.ethdebug"]"#),
            OutputSelectionFlags::WILDCARD
                | OutputSelectionFlags::IR
                | OutputSelectionFlags::BYTECODE_ETHDEBUG
        );
        assert!(selection_flags(r#"["unknown", "evm.bytecode.unknown"]"#).is_empty());
    }

    #[test]
    fn output_selection_merges_source_and_contract_wildcards() {
        let selection = serde_json::from_str::<OutputSelection<'_>>(
            r#"{
                "A.sol": {
                    "A": ["abi"],
                    "*": ["userdoc"]
                },
                "*": {
                    "A": ["devdoc"],
                    "*": ["storageLayout"]
                }
            }"#,
        )
        .unwrap();

        assert_eq!(
            selection.contract("A.sol", "A"),
            OutputSelectionFlags::ABI
                | OutputSelectionFlags::USERDOC
                | OutputSelectionFlags::DEVDOC
                | OutputSelectionFlags::STORAGE_LAYOUT
        );
        assert_eq!(
            selection.contract("A.sol", "B"),
            OutputSelectionFlags::USERDOC | OutputSelectionFlags::STORAGE_LAYOUT
        );
        assert_eq!(
            selection.contract("B.sol", "A"),
            OutputSelectionFlags::DEVDOC | OutputSelectionFlags::STORAGE_LAYOUT
        );
        assert_eq!(selection.contract("B.sol", "B"), OutputSelectionFlags::STORAGE_LAYOUT);
    }

    #[test]
    fn output_selection_contract_scope() {
        let selection = serde_json::from_str::<OutputSelection<'_>>(
            r#"{
                "A.sol": {
                    "A": ["ast", "abi", "ethdebug.compilation"]
                },
                "*": {
                    "A": ["devdoc", "ethdebug.compilation"],
                    "*": ["storageLayout", "ethdebug.resources"]
                }
            }"#,
        )
        .unwrap();

        assert_eq!(
            selection.contract("A.sol", "A"),
            OutputSelectionFlags::ABI
                | OutputSelectionFlags::DEVDOC
                | OutputSelectionFlags::STORAGE_LAYOUT
        );
    }
}
