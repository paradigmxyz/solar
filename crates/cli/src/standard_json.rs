use alloy_primitives::U256;
use indexmap::IndexMap;
use serde::{
    Deserialize, Serialize,
    de::{self, Visitor},
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
    Gcx, Ty,
    ast::{DataLocation, ElementaryType},
    hir::{self, ContractId},
    ty::TyKind,
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
struct SourceInput<'a> {
    #[serde(borrow)]
    content: Option<CowStr<'a>>,
    #[serde(borrow, default)]
    urls: Vec<CowStr<'a>>,
    // `keccak256` validation and `assemblyJson` inputs are not supported yet.
    // #[serde(borrow)]
    // keccak256: Option<CowValue<'a>>,
    // #[serde(borrow, rename = "assemblyJson")]
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
    /// Whether to compile via the Yul IR pipeline. We have a single pipeline, so
    /// there is nothing to switch.
    #[serde(default, rename = "viaIR")]
    via_ir: Option<bool>,
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
    // The SSA CFG pipeline is ignored because we have a single compilation pipeline.
    // #[serde(borrow, default, rename = "viaSSACFG")]
    // via_ssa_cfg: Option<CowValue<'a>>,
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
    #[serde(borrow)] FxIndexMap<CowStr<'a>, FxIndexMap<CowStr<'a>, Vec<CowStr<'a>>>>,
);

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
    let Settings {
        remappings,
        output_selection,
        stop_after,
        evm_version,
        optimizer,
        via_ir: _via_ir,
    } = settings;

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
                    let contract_output = make_contract_output(
                        gcx,
                        contract_id,
                        &output_selection,
                        &source_name,
                        &contract_name,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    userdoc: Option<UserDocumentation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    devdoc: Option<DevDocumentation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    storage_layout: Option<StorageLayoutOutput>,
    // Transient storage layouts and Yul IR output are not supported yet.
    // #[serde(skip_serializing_if = "Option::is_none")]
    // transient_storage_layout: Option<CowValue<'static>>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // ir: Option<CowValue<'static>>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // ir_ast: Option<CowValue<'static>>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // ir_optimized: Option<CowValue<'static>>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // ir_optimized_ast: Option<CowValue<'static>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    evm: Option<EvmOutput>,
}

#[derive(Debug, Serialize)]
struct UserDocumentation {
    kind: DocumentationKind,
    methods: FxIndexMap<String, UserDocNotice>,
    #[serde(default, skip_serializing_if = "FxIndexMap::is_empty")]
    events: FxIndexMap<String, UserDocNotice>,
    #[serde(default, skip_serializing_if = "FxIndexMap::is_empty")]
    errors: FxIndexMap<String, Vec<UserDocNotice>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    notice: Option<String>,
    version: u8,
}

#[derive(Debug, Default, Serialize)]
struct UserDocNotice {
    notice: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DevDocumentation {
    kind: DocumentationKind,
    methods: FxIndexMap<String, DevDocItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<String>,
    #[serde(default, skip_serializing_if = "FxIndexMap::is_empty")]
    events: FxIndexMap<String, DevDocItem>,
    #[serde(default, skip_serializing_if = "FxIndexMap::is_empty")]
    errors: FxIndexMap<String, Vec<DevDocItem>>,
    #[serde(default, skip_serializing_if = "FxIndexMap::is_empty")]
    state_variables: FxIndexMap<String, StateVariableDoc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(flatten)]
    custom: FxIndexMap<String, String>,
    version: u8,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum DocumentationKind {
    User,
    Dev,
}

#[derive(Debug, Default, Serialize)]
struct DevDocItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<String>,
    #[serde(default, skip_serializing_if = "FxIndexMap::is_empty")]
    params: FxIndexMap<String, String>,
    #[serde(default, skip_serializing_if = "FxIndexMap::is_empty")]
    returns: FxIndexMap<String, String>,
    #[serde(flatten)]
    custom: FxIndexMap<String, String>,
}

#[derive(Debug, Default, Serialize)]
struct StateVariableDoc {
    #[serde(skip_serializing_if = "Option::is_none")]
    author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<String>,
    #[serde(default, skip_serializing_if = "FxIndexMap::is_empty")]
    params: FxIndexMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "return")]
    return_doc: Option<String>,
    #[serde(default, skip_serializing_if = "FxIndexMap::is_empty")]
    returns: FxIndexMap<String, String>,
    #[serde(flatten)]
    custom: FxIndexMap<String, String>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct StorageLayoutOutput {
    storage: Vec<StorageLayoutEntry>,
    types: FxIndexMap<String, StorageLayoutType>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StorageLayoutEntry {
    ast_id: u64,
    contract: String,
    label: String,
    offset: u64,
    slot: String,
    #[serde(rename = "type")]
    ty: String,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct StorageLayoutType {
    encoding: StorageEncoding,
    label: String,
    number_of_bytes: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    base: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    members: Vec<StorageLayoutMember>,
}

#[derive(Debug, Default, Serialize)]
enum StorageEncoding {
    #[serde(rename = "inplace")]
    #[default]
    Inplace,
    #[serde(rename = "mapping")]
    Mapping,
    #[serde(rename = "dynamic_array")]
    DynamicArray,
    #[serde(rename = "bytes")]
    Bytes,
}

type StorageLayoutMember = StorageLayoutEntry;

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct EvmOutput {
    // Assembly, gas estimates, and Yul CFG output are not supported yet.
    // #[serde(skip_serializing_if = "Option::is_none")]
    // assembly: Option<CowValue<'static>>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // legacy_assembly: Option<CowValue<'static>>,
    #[serde(default, skip_serializing_if = "FxIndexMap::is_empty")]
    method_identifiers: FxIndexMap<String, String>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // gas_estimates: Option<CowValue<'static>>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // yul_cfg_json: Option<CowValue<'static>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bytecode: Option<BytecodeOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    deployed_bytecode: Option<BytecodeOutput>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct BytecodeOutput {
    object: String,
    // Ethdebug, function debug data, and generated sources are not supported yet.
    // #[serde(skip_serializing_if = "Option::is_none")]
    // ethdebug: Option<CowValue<'static>>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // function_debug_data: Option<CowValue<'static>>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    opcodes: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    source_map: String,
    #[serde(default, skip_serializing_if = "FxIndexMap::is_empty")]
    link_references: LinkReferences,
    #[serde(default, skip_serializing_if = "FxIndexMap::is_empty")]
    immutable_references: ImmutableReferences,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // generated_sources: Option<CowValue<'static>>,
}

type LinkReferences = FxIndexMap<String, FxIndexMap<String, Vec<OffsetLength>>>;
type ImmutableReferences = FxIndexMap<String, Vec<OffsetLength>>;

#[derive(Debug, Serialize)]
struct OffsetLength {
    start: u32,
    length: u32,
}

struct GeneratedBytecodes {
    deployment: String,
    runtime: String,
}

impl BytecodeOutput {
    fn empty() -> Self {
        Self::default()
    }

    fn new(object: String) -> Self {
        Self { object, ..Self::default() }
    }
}

impl OutputSelection<'_> {
    fn selects(&self, source: &str, contract: &str, keys: &[&str]) -> bool {
        self.source_maps(source).any(|contracts| {
            contract_maps(contracts, contract).any(|items| {
                items.iter().any(|item| {
                    item.as_ref() == "*"
                        || keys.iter().any(|key| {
                            item.as_ref() == *key
                                || key
                                    .strip_prefix(item.as_ref())
                                    .is_some_and(|rest| rest.starts_with('.'))
                        })
                })
            })
        })
    }

    fn source_maps(
        &self,
        source: &str,
    ) -> impl Iterator<Item = &FxIndexMap<CowStr<'_>, Vec<CowStr<'_>>>> {
        [source, "*"].into_iter().filter_map(|source| self.0.get(source))
    }
}

fn contract_maps<'a, 'b>(
    contracts: &'a FxIndexMap<CowStr<'b>, Vec<CowStr<'b>>>,
    contract: &'a str,
) -> impl Iterator<Item = &'a Vec<CowStr<'b>>> {
    [contract, "*"].into_iter().filter_map(|contract| contracts.get(contract))
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
        for (contract, items) in contracts {
            stats.add(contract);
            for item in items {
                stats.add(item);
            }
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

fn user_documentation(gcx: Gcx<'_>, contract_id: ContractId) -> UserDocumentation {
    let contract = gcx.hir.contract(contract_id);
    let mut documentation = UserDocumentation {
        kind: DocumentationKind::User,
        methods: FxIndexMap::default(),
        events: FxIndexMap::default(),
        errors: FxIndexMap::default(),
        notice: natspec_text(gcx, contract.doc, |kind| matches!(kind, hir::NatSpecKind::Notice)),
        version: 1,
    };

    if let Some(constructor) = contract.ctor
        && let Some(notice) = natspec_text(gcx, gcx.hir.function(constructor).doc, |kind| {
            matches!(kind, hir::NatSpecKind::Notice)
        })
    {
        documentation.methods.insert("constructor".into(), UserDocNotice { notice });
    }

    for interface_function in gcx.interface_functions(contract_id) {
        let function_id = interface_function.id;
        let function = gcx.hir.function(function_id);
        let doc = function.gettee.map_or(function.doc, |gettee| gcx.hir.variable(gettee).doc);
        if let Some(notice) =
            natspec_text(gcx, doc, |kind| matches!(kind, hir::NatSpecKind::Notice))
        {
            documentation.methods.insert(
                gcx.item_signature(function_id.into()).to_string(),
                UserDocNotice { notice },
            );
        }
    }

    let mut event_signatures = FxHashSet::default();
    for item in gcx.hir.contract_item_ids(contract_id) {
        match item {
            hir::ItemId::Event(event_id) => {
                let event = gcx.hir.event(event_id);
                let signature = gcx.item_signature(event_id.into()).to_string();
                if !event_signatures.insert(signature.clone()) {
                    continue;
                }
                if let Some(notice) =
                    natspec_text(gcx, event.doc, |kind| matches!(kind, hir::NatSpecKind::Notice))
                {
                    documentation.events.insert(signature, UserDocNotice { notice });
                }
            }
            hir::ItemId::Error(error_id) => {
                let error = gcx.hir.error(error_id);
                if let Some(notice) =
                    natspec_text(gcx, error.doc, |kind| matches!(kind, hir::NatSpecKind::Notice))
                {
                    documentation
                        .errors
                        .entry(gcx.item_signature(error_id.into()).to_string())
                        .or_default()
                        .push(UserDocNotice { notice });
                }
            }
            _ => {}
        }
    }

    documentation
}

fn dev_documentation(gcx: Gcx<'_>, contract_id: ContractId) -> DevDocumentation {
    let contract = gcx.hir.contract(contract_id);
    let contract_doc = dev_doc_item(gcx, contract.doc);
    let mut documentation = DevDocumentation {
        kind: DocumentationKind::Dev,
        methods: FxIndexMap::default(),
        author: contract_doc.author,
        details: contract_doc.details,
        events: FxIndexMap::default(),
        errors: FxIndexMap::default(),
        state_variables: FxIndexMap::default(),
        title: natspec_text(gcx, contract.doc, |kind| matches!(kind, hir::NatSpecKind::Title)),
        custom: contract_doc.custom,
        version: 1,
    };

    if let Some(constructor) = contract.ctor {
        let documentation_item = dev_doc_item(gcx, gcx.hir.function(constructor).doc);
        if !documentation_item.is_empty() {
            documentation.methods.insert("constructor".into(), documentation_item);
        }
    }

    for interface_function in gcx.interface_functions(contract_id) {
        let function_id = interface_function.id;
        let function = gcx.hir.function(function_id);
        if function.is_getter() {
            continue;
        }

        let mut documentation_item = dev_doc_item(gcx, function.doc);
        documentation_item.returns = return_docs(gcx, function.doc, function.returns);
        if !documentation_item.is_empty() {
            documentation
                .methods
                .insert(gcx.item_signature(function_id.into()).to_string(), documentation_item);
        }
    }

    for variable_id in contract.variables() {
        let variable = gcx.hir.variable(variable_id);
        let mut documentation_item =
            StateVariableDoc::from_dev_doc_item(dev_doc_item(gcx, variable.doc));
        let return_text = natspec_items(gcx, variable.doc)
            .filter(|item| matches!(item.kind, hir::NatSpecKind::Return { .. }))
            .map(|item| item.content().to_string())
            .collect::<String>();
        if !return_text.is_empty() {
            documentation_item.return_doc = Some(return_text);
        }
        if let Some(getter) = variable.getter {
            documentation_item.returns =
                return_docs(gcx, variable.doc, gcx.hir.function(getter).returns);
        }
        if !documentation_item.is_empty() {
            documentation
                .state_variables
                .insert(variable.name.unwrap().to_string(), documentation_item);
        }
    }

    let mut event_signatures = FxHashSet::default();
    for item in gcx.hir.contract_item_ids(contract_id) {
        match item {
            hir::ItemId::Event(event_id) => {
                let event = gcx.hir.event(event_id);
                let signature = gcx.item_signature(event_id.into()).to_string();
                if !event_signatures.insert(signature.clone()) {
                    continue;
                }
                let documentation_item = dev_doc_item(gcx, event.doc);
                if !documentation_item.is_empty() {
                    documentation.events.insert(signature, documentation_item);
                }
            }
            hir::ItemId::Error(error_id) => {
                let error = gcx.hir.error(error_id);
                let documentation_item = dev_doc_item(gcx, error.doc);
                if !documentation_item.is_empty() {
                    documentation
                        .errors
                        .entry(gcx.item_signature(error_id.into()).to_string())
                        .or_default()
                        .push(documentation_item);
                }
            }
            _ => {}
        }
    }

    documentation
}

fn natspec_items(gcx: Gcx<'_>, doc_id: hir::DocId) -> impl Iterator<Item = hir::NatSpecItem> + '_ {
    gcx.natspec_doc_comments(doc_id).iter().copied()
}

fn natspec_text(
    gcx: Gcx<'_>,
    doc_id: hir::DocId,
    mut matches: impl FnMut(hir::NatSpecKind) -> bool,
) -> Option<String> {
    let text = natspec_items(gcx, doc_id)
        .filter(|item| matches(item.kind))
        .map(|item| item.content().to_string())
        .collect::<String>();
    (!text.is_empty()).then_some(text)
}

fn dev_doc_item(gcx: Gcx<'_>, doc_id: hir::DocId) -> DevDocItem {
    let mut documentation = DevDocItem::default();
    for item in natspec_items(gcx, doc_id) {
        let content = item.content();
        match item.kind {
            hir::NatSpecKind::Author => append_doc(&mut documentation.author, content),
            hir::NatSpecKind::Dev => append_doc(&mut documentation.details, content),
            hir::NatSpecKind::Param { name } => {
                documentation.params.entry(name.name.to_string()).or_default().push_str(content);
            }
            hir::NatSpecKind::Custom { name } => {
                documentation
                    .custom
                    .entry(format!("custom:{}", name.name))
                    .or_default()
                    .push_str(content);
            }
            hir::NatSpecKind::Title
            | hir::NatSpecKind::Notice
            | hir::NatSpecKind::Return { .. }
            | hir::NatSpecKind::Inheritdoc { .. }
            | hir::NatSpecKind::Internal { .. } => {}
        }
    }
    documentation
}

fn append_doc(target: &mut Option<String>, content: &str) {
    target.get_or_insert_default().push_str(content);
}

fn return_docs(
    gcx: Gcx<'_>,
    doc_id: hir::DocId,
    returns: &[hir::VariableId],
) -> FxIndexMap<String, String> {
    natspec_items(gcx, doc_id)
        .filter(|item| matches!(item.kind, hir::NatSpecKind::Return { .. }))
        .enumerate()
        .filter_map(|(index, item)| {
            let variable = gcx.hir.variable(*returns.get(index)?);
            let name = variable.name.map_or_else(|| format!("_{index}"), |name| name.to_string());
            (!item.content().is_empty()).then_some((name, item.content().to_string()))
        })
        .collect()
}

impl DevDocItem {
    fn is_empty(&self) -> bool {
        self.author.is_none()
            && self.details.is_none()
            && self.params.is_empty()
            && self.returns.is_empty()
            && self.custom.is_empty()
    }
}

impl StateVariableDoc {
    fn from_dev_doc_item(documentation: DevDocItem) -> Self {
        Self {
            author: documentation.author,
            details: documentation.details,
            params: documentation.params,
            return_doc: None,
            returns: documentation.returns,
            custom: documentation.custom,
        }
    }

    fn is_empty(&self) -> bool {
        self.author.is_none()
            && self.details.is_none()
            && self.params.is_empty()
            && self.return_doc.is_none()
            && self.returns.is_empty()
            && self.custom.is_empty()
    }
}

fn storage_layout(gcx: Gcx<'_>, contract_id: ContractId) -> StorageLayoutOutput {
    StorageLayoutBuilder::new(gcx, contract_id).build()
}

struct StorageLayoutBuilder<'gcx> {
    gcx: Gcx<'gcx>,
    contract_id: ContractId,
    contract_name: String,
    types: FxIndexMap<String, StorageLayoutType>,
}

impl<'gcx> StorageLayoutBuilder<'gcx> {
    fn new(gcx: Gcx<'gcx>, contract_id: ContractId) -> Self {
        Self {
            gcx,
            contract_id,
            contract_name: gcx.contract_fully_qualified_name(contract_id).to_string(),
            types: FxIndexMap::default(),
        }
    }

    fn build(mut self) -> StorageLayoutOutput {
        let contract = self.gcx.hir.contract(self.contract_id);
        let base_slot = contract.layout.map_or(U256::ZERO, |layout| {
            solar_sema::eval::ConstantEvaluator::new(self.gcx)
                .eval(layout)
                .ok()
                .and_then(|value| value.as_u256())
                .unwrap_or_default()
        });
        let bases = if contract.linearized_bases.is_empty() {
            std::slice::from_ref(&self.contract_id)
        } else {
            contract.linearized_bases
        }
        .iter()
        .rev()
        .copied()
        .collect::<Vec<_>>();
        let mut cursor = StorageCursor::new(base_slot);
        let mut storage = Vec::new();

        for base in bases {
            for variable_id in self.gcx.hir.contract(base).variables() {
                let variable = self.gcx.hir.variable(variable_id);
                if variable.is_constant()
                    || variable.is_immutable()
                    || variable.data_location == Some(DataLocation::Transient)
                {
                    continue;
                }

                let ty = self.gcx.type_of_item(variable_id.into());
                let ty_name = self.generate_type(ty);
                let (slot, offset) = self.place_type(ty, &mut cursor);
                storage.push(self.storage_entry(variable_id, slot, offset, ty_name));
            }
        }

        StorageLayoutOutput { storage, types: self.types }
    }

    fn layout_members(&mut self, fields: &[hir::VariableId]) -> (Vec<StorageLayoutEntry>, U256) {
        let mut cursor = StorageCursor::new(U256::ZERO);
        let mut members = Vec::with_capacity(fields.len());
        for &field in fields {
            let ty = self.gcx.type_of_item(field.into());
            let ty_name = self.generate_type(ty);
            let (slot, offset) = self.place_type(ty, &mut cursor);
            members.push(self.storage_entry(field, slot, offset, ty_name));
        }
        (members, cursor.size())
    }

    fn storage_entry(
        &self,
        variable_id: hir::VariableId,
        slot: U256,
        offset: u64,
        ty: String,
    ) -> StorageLayoutEntry {
        StorageLayoutEntry {
            ast_id: variable_id.index() as u64,
            contract: self.contract_name.clone(),
            label: self.gcx.hir.variable(variable_id).name.unwrap().to_string(),
            offset,
            slot: slot.to_string(),
            ty,
        }
    }

    fn place_type(&mut self, ty: Ty<'gcx>, cursor: &mut StorageCursor) -> (U256, u64) {
        let bytes = self.storage_bytes(ty);
        if !self.is_packable(ty) {
            cursor.align();
            let slot = cursor.slot;
            cursor.advance(slots_for(bytes));
            return (slot, 0);
        }

        let bytes = bytes.to::<u64>();
        if cursor.offset + bytes > 32 {
            cursor.align();
        }
        let (slot, offset) = (cursor.slot, cursor.offset);
        cursor.offset += bytes;
        if cursor.offset == 32 {
            cursor.advance(U256::from(1));
        }
        (slot, offset)
    }

    fn generate_type(&mut self, ty: Ty<'gcx>) -> String {
        let key = self.storage_type_key(ty);
        if self.types.contains_key(&key) {
            return key;
        }
        self.types.insert(key.clone(), StorageLayoutType::default());

        let ty = ty.peel_refs();
        let mut info = StorageLayoutType {
            encoding: StorageEncoding::Inplace,
            label: self.storage_type_label(ty),
            number_of_bytes: self.storage_bytes(ty).to_string(),
            ..Default::default()
        };
        match ty.kind {
            TyKind::Struct(struct_id) => {
                let (members, _) = self.layout_members(self.gcx.hir.strukt(struct_id).fields);
                info.members = members;
            }
            TyKind::Mapping(key_ty, value_ty) => {
                info.encoding = StorageEncoding::Mapping;
                info.key = Some(self.generate_type(key_ty));
                info.value = Some(self.generate_type(value_ty));
            }
            TyKind::Array(base, _) => {
                info.base = Some(self.generate_type(base));
            }
            TyKind::DynArray(base) => {
                info.encoding = StorageEncoding::DynamicArray;
                info.base = Some(self.generate_type(base));
            }
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                info.encoding = StorageEncoding::Bytes;
            }
            TyKind::Elementary(_)
            | TyKind::Contract(_)
            | TyKind::Enum(_)
            | TyKind::Fn(_)
            | TyKind::Udvt(..) => {}
            _ => unreachable!("invalid storage type: {ty:?}"),
        }
        self.types.insert(key.clone(), info);
        key
    }

    fn storage_type_key(&self, ty: Ty<'gcx>) -> String {
        match ty.kind {
            TyKind::Ref(inner, location) => {
                let key = self.storage_type_key(inner);
                if matches!(inner.peel_refs().kind, TyKind::Mapping(..)) {
                    key
                } else {
                    format!("{key}_{location}")
                }
            }
            TyKind::Elementary(ty) => format!("t_{}", ty.to_string().replace(' ', "_")),
            TyKind::Array(base, length) => {
                format!("t_array({}){length}", self.storage_type_key(base))
            }
            TyKind::DynArray(base) => format!("t_array({})dyn", self.storage_type_key(base)),
            TyKind::Mapping(key, value) => format!(
                "t_mapping({},{})",
                self.storage_type_key(key),
                self.storage_type_key(value)
            ),
            TyKind::Contract(id) => format!("t_contract({}){}", self.gcx.item_name(id), id.index()),
            TyKind::Struct(id) => format!("t_struct({}){}", self.gcx.item_name(id), id.index()),
            TyKind::Enum(id) => format!("t_enum({}){}", self.gcx.item_name(id), id.index()),
            TyKind::Udvt(_, id) => {
                format!("t_userDefinedValueType({}){}", self.gcx.item_name(id), id.index())
            }
            TyKind::Fn(function) => {
                let kind = if function.is_external() { "external" } else { "internal" };
                let params = function
                    .parameters
                    .iter()
                    .map(|ty| self.storage_type_key(*ty))
                    .collect::<Vec<_>>()
                    .join(",");
                let returns = function
                    .returns
                    .iter()
                    .map(|ty| self.storage_type_key(*ty))
                    .collect::<Vec<_>>()
                    .join(",");
                format!(
                    "t_function_{kind}_{}({params})returns({returns})",
                    function.state_mutability
                )
            }
            _ => unreachable!("invalid storage type: {ty:?}"),
        }
    }

    fn storage_type_label(&self, ty: Ty<'gcx>) -> String {
        match ty.kind {
            TyKind::Ref(inner, _) => self.storage_type_label(inner),
            TyKind::Elementary(ty) => ty.to_string(),
            TyKind::Array(base, length) => format!("{}[{length}]", self.storage_type_label(base)),
            TyKind::DynArray(base) => format!("{}[]", self.storage_type_label(base)),
            TyKind::Mapping(key, value) => format!(
                "mapping({} => {})",
                self.storage_type_label(key),
                self.storage_type_label(value)
            ),
            TyKind::Contract(id) => format!("contract {}", self.gcx.item_name(id)),
            TyKind::Struct(id) => format!("struct {}", self.gcx.item_canonical_name(id)),
            TyKind::Enum(id) => format!("enum {}", self.gcx.item_name(id)),
            TyKind::Udvt(_, id) => self.gcx.item_name(id).to_string(),
            TyKind::Fn(function) => {
                let params = function
                    .parameters
                    .iter()
                    .map(|ty| self.storage_type_label(*ty))
                    .collect::<Vec<_>>()
                    .join(", ");
                let kind = if function.is_external() { "external" } else { "internal" };
                let mut label = format!("function ({params}) {kind}");
                if function.state_mutability != hir::StateMutability::NonPayable {
                    label.push(' ');
                    label.push_str(&function.state_mutability.to_string());
                }
                if !function.returns.is_empty() {
                    let returns = function
                        .returns
                        .iter()
                        .map(|ty| self.storage_type_label(*ty))
                        .collect::<Vec<_>>()
                        .join(", ");
                    label.push_str(&format!(" returns ({returns})"));
                }
                label
            }
            _ => unreachable!("invalid storage type: {ty:?}"),
        }
    }

    fn storage_bytes(&mut self, ty: Ty<'gcx>) -> U256 {
        match ty.kind {
            TyKind::Ref(inner, _) => self.storage_bytes(inner),
            TyKind::Elementary(ty) => match ty {
                ElementaryType::Address(_) => U256::from(20),
                ElementaryType::Bool => U256::from(1),
                ElementaryType::String | ElementaryType::Bytes => U256::from(32),
                ElementaryType::Fixed(size, _)
                | ElementaryType::UFixed(size, _)
                | ElementaryType::Int(size)
                | ElementaryType::UInt(size)
                | ElementaryType::FixedBytes(size) => U256::from(size.bytes()),
            },
            TyKind::Array(base, length) => {
                slots_for(self.storage_bytes(base) * length) * U256::from(32)
            }
            TyKind::DynArray(_) | TyKind::Mapping(..) => U256::from(32),
            TyKind::Struct(struct_id) => {
                self.layout_members(self.gcx.hir.strukt(struct_id).fields).1
            }
            TyKind::Contract(_) => U256::from(20),
            TyKind::Enum(_) => U256::from(1),
            TyKind::Udvt(inner, _) => self.storage_bytes(inner),
            TyKind::Fn(function) if function.is_external() => U256::from(24),
            TyKind::Fn(_) => U256::from(8),
            _ => unreachable!("invalid storage type: {ty:?}"),
        }
    }

    fn is_packable(&self, ty: Ty<'gcx>) -> bool {
        matches!(
            ty.peel_refs().kind,
            TyKind::Elementary(
                ElementaryType::Address(_)
                    | ElementaryType::Bool
                    | ElementaryType::Fixed(..)
                    | ElementaryType::UFixed(..)
                    | ElementaryType::Int(_)
                    | ElementaryType::UInt(_)
                    | ElementaryType::FixedBytes(_)
            ) | TyKind::Contract(_)
                | TyKind::Enum(_)
                | TyKind::Udvt(..)
                | TyKind::Fn(_)
        )
    }
}

#[derive(Clone, Copy)]
struct StorageCursor {
    slot: U256,
    offset: u64,
}

impl StorageCursor {
    fn new(slot: U256) -> Self {
        Self { slot, offset: 0 }
    }

    fn align(&mut self) {
        if self.offset != 0 {
            self.slot += U256::from(1);
            self.offset = 0;
        }
    }

    fn advance(&mut self, slots: U256) {
        self.slot += slots;
        self.offset = 0;
    }

    fn size(self) -> U256 {
        (self.slot + U256::from(u8::from(self.offset != 0))) * U256::from(32)
    }
}

fn slots_for(bytes: U256) -> U256 {
    (bytes + U256::from(31)) / U256::from(32)
}

fn make_contract_output(
    gcx: Gcx<'_>,
    contract_id: solar_sema::hir::ContractId,
    output_selection: &OutputSelection<'_>,
    source_name: &str,
    contract_name: &str,
    bytecodes: Option<&FxHashMap<ContractId, GeneratedBytecodes>>,
) -> ContractOutput {
    let mut output = ContractOutput::default();

    if output_selection.selects(source_name, contract_name, &["abi"]) {
        output.abi = Some(gcx.contract_abi(contract_id));
    }
    if output_selection.selects(source_name, contract_name, &["userdoc"]) {
        output.userdoc = Some(user_documentation(gcx, contract_id));
    }
    if output_selection.selects(source_name, contract_name, &["devdoc"]) {
        output.devdoc = Some(dev_documentation(gcx, contract_id));
    }
    if output_selection.selects(source_name, contract_name, &["storageLayout"]) {
        output.storage_layout = Some(storage_layout(gcx, contract_id));
    }

    let mut evm = EvmOutput::default();
    if output_selection.selects(source_name, contract_name, &["evm.methodIdentifiers"]) {
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
    // `object` for now (the other sub-fields are left empty and skipped during
    // serialization), the two selectors currently produce identical output.
    // Honoring the finer-grained `.object`/`.opcodes`/`.sourceMap` selectors is
    // part of the larger effort to match solc's input->output key mapping.
    if output_selection.selects(
        source_name,
        contract_name,
        &["evm.bytecode", "evm.bytecode.object"],
    ) {
        evm.bytecode = Some(
            bytecodes
                .and_then(|bytecodes| bytecodes.get(&contract_id))
                .map(|bytecodes| BytecodeOutput::new(bytecodes.deployment.clone()))
                .unwrap_or_else(BytecodeOutput::empty),
        );
    }
    if output_selection.selects(
        source_name,
        contract_name,
        &["evm.deployedBytecode", "evm.deployedBytecode.object"],
    ) {
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
        output_selection.selects(
            &source_name,
            &contract_name,
            &[
                "evm.bytecode",
                "evm.bytecode.object",
                "evm.deployedBytecode",
                "evm.deployedBytecode.object",
            ],
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
                GeneratedBytecodes {
                    deployment: alloy_primitives::hex::encode(deployment),
                    runtime: alloy_primitives::hex::encode(runtime),
                },
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
            && self.metadata.is_none()
            && self.userdoc.is_none()
            && self.devdoc.is_none()
            && self.storage_layout.is_none()
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
