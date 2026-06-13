use indexmap::IndexMap;
use rustc_hash::FxBuildHasher;
use serde::{
    Deserialize, Serialize,
    de::{self, Visitor},
};
use serde_json::{Map, Value, json};
use solar_config::{CompilerStage, EvmVersion, ImportRemapping, Language, Opts};
use solar_interface::{
    SourceMap,
    diagnostics::{DiagCtxt, InMemoryEmitter, JsonEmitter, SolcDiagnostic},
    source_map::FileLoader,
};
use std::{
    borrow::{Borrow, Cow},
    collections::BTreeMap,
    fmt,
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
    #[serde(default)]
    #[serde(borrow)]
    sources: FxIndexMap<CowStr<'a>, SourceInput<'a>>,
    #[serde(default)]
    #[serde(borrow)]
    settings: Settings<'a>,
}

#[derive(Debug, Deserialize)]
struct SourceInput<'a> {
    #[serde(borrow)]
    content: Option<CowStr<'a>>,
    #[serde(default)]
    #[serde(borrow)]
    urls: Vec<CowStr<'a>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Settings<'a> {
    #[serde(default)]
    #[serde(borrow)]
    remappings: Vec<CowStr<'a>>,
    #[serde(default)]
    #[serde(borrow)]
    output_selection: OutputSelection<'a>,
    #[serde(borrow)]
    stop_after: Option<CowStr<'a>>,
    #[serde(borrow)]
    evm_version: Option<CowStr<'a>>,
}

#[derive(Debug, Default, Deserialize)]
struct OutputSelection<'a>(
    #[serde(borrow)] FxIndexMap<CowStr<'a>, FxIndexMap<CowStr<'a>, Vec<CowStr<'a>>>>,
);

#[derive(Debug, Default, Serialize)]
struct CompilerOutput<'a> {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    errors: Vec<SolcDiagnostic<'a>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    sources: BTreeMap<String, SourceOutput>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    contracts: BTreeMap<String, BTreeMap<String, ContractOutput>>,
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
    mut opts: Opts,
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

pub(crate) fn run(opts: Opts) -> io::Result<()> {
    let mut input = String::new();
    let stdout = io::stdout();
    let mut stdout = io::BufWriter::new(stdout.lock());
    match io::stdin().read_to_string(&mut input) {
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
    opts: &mut Opts,
    source_map: Arc<SourceMap>,
    dcx: DiagCtxt,
    output: &mut CompilerOutput<'_>,
) {
    let mut remappings = Vec::with_capacity(input.settings.remappings.len());
    for remapping in &input.settings.remappings {
        match remapping.parse::<ImportRemapping>() {
            Ok(remapping) => remappings.push(remapping),
            Err(e) => {
                dcx.err(format!("invalid remapping `{remapping}`: {e}")).emit();
            }
        }
    }
    if dcx.has_errors().is_err() {
        return;
    }

    opts.import_remappings = remappings;
    opts.evm_version = input
        .settings
        .evm_version
        .as_deref()
        .and_then(|version| EvmVersion::from_str(version).ok())
        .unwrap_or(opts.evm_version);
    opts.language = match input.language.as_ref() {
        "Solidity" | "solidity" => Language::Solidity,
        "Yul" | "yul" => Language::Yul,
        language => {
            dcx.err(format!("unsupported language `{language}`")).emit();
            return;
        }
    };
    opts.stop_after =
        input.settings.stop_after.as_deref().and_then(|stage| CompilerStage::from_str(stage).ok());
    opts.input = input.sources.keys().map(ToString::to_string).collect();

    let sess = solar_interface::Session::builder()
        .source_map(Arc::clone(&source_map))
        .dcx(dcx)
        .opts(opts.clone())
        .build();

    let output_selection = input.settings.output_selection;
    let sources = input.sources;
    let _ = crate::run_compiler_session_with(
        sess,
        |compiler| {
            let compile_result = crate::run_pipeline(
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
                for (contract_id, contract) in gcx.hir.contracts_enumerated() {
                    let source = gcx.hir.source(contract.source);
                    let source_name = source.file.name.display().to_string();
                    let contract_name = contract.name.to_string();
                    let contract_output = make_contract_output(
                        gcx,
                        contract_id,
                        &output_selection,
                        &source_name,
                        &contract_name,
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
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct ContractOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    abi: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    userdoc: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    devdoc: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    storage_layout: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    evm: Option<EvmOutput>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct EvmOutput {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    method_identifiers: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bytecode: Option<BytecodeOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    deployed_bytecode: Option<BytecodeOutput>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct BytecodeOutput {
    object: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    opcodes: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    source_map: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    link_references: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    immutable_references: BTreeMap<String, Value>,
}

impl BytecodeOutput {
    fn empty() -> Self {
        Self::default()
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
) -> BTreeMap<String, SourceOutput> {
    compiler
        .gcx()
        .sources
        .iter_enumerated()
        .map(|(id, source)| {
            (source.file.name.display().to_string(), SourceOutput { id: id.index() as u32 })
        })
        .collect()
}

fn make_contract_output(
    gcx: solar_sema::Gcx<'_>,
    contract_id: solar_sema::hir::ContractId,
    output_selection: &OutputSelection<'_>,
    source_name: &str,
    contract_name: &str,
) -> ContractOutput {
    let mut output = ContractOutput::default();

    if output_selection.selects(source_name, contract_name, &["abi"]) {
        output.abi = Some(serde_json::to_value(gcx.contract_abi(contract_id)).unwrap());
    }
    if output_selection.selects(source_name, contract_name, &["userdoc"]) {
        output.userdoc = Some(json!({ "kind": "user", "methods": {}, "version": 1 }));
    }
    if output_selection.selects(source_name, contract_name, &["devdoc"]) {
        output.devdoc = Some(json!({ "kind": "dev", "methods": {}, "version": 1 }));
    }
    if output_selection.selects(source_name, contract_name, &["storageLayout"]) {
        output.storage_layout = Some(json!({ "storage": [], "types": {} }));
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
    if output_selection.selects(
        source_name,
        contract_name,
        &["evm.bytecode", "evm.bytecode.object"],
    ) {
        evm.bytecode = Some(BytecodeOutput::empty());
    }
    if output_selection.selects(
        source_name,
        contract_name,
        &["evm.deployedBytecode", "evm.deployedBytecode.object"],
    ) {
        evm.deployed_bytecode = Some(BytecodeOutput::empty());
    }
    if !evm.is_empty() {
        output.evm = Some(evm);
    }

    output
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

#[cfg(test)]
mod tests {
    use super::*;
    use snapbox::{IntoData as _, assert_data_eq, str};
    use std::collections::BTreeMap;

    struct Sources(BTreeMap<String, String>);

    impl StandardJsonReadCallback for Sources {
        fn read(&self, kind: &str, data: &str) -> ReadCallbackResult {
            if kind != "source" {
                return ReadCallbackResult::Unsupported;
            }
            self.0.get(data).cloned().map_or_else(
                || ReadCallbackResult::Error(format!("source `{data}` not found")),
                ReadCallbackResult::Success,
            )
        }
    }

    fn compile(input: &str, callback: Option<Arc<dyn StandardJsonReadCallback>>) -> String {
        compile_with(input, callback, false)
    }

    fn compile_with_typeck(
        input: &str,
        callback: Option<Arc<dyn StandardJsonReadCallback>>,
    ) -> String {
        compile_with(input, callback, true)
    }

    fn compile_with(
        input: &str,
        callback: Option<Arc<dyn StandardJsonReadCallback>>,
        typeck: bool,
    ) -> String {
        let mut output = Vec::new();
        let opts = Opts {
            pretty_json: true,
            unstable: solar_config::UnstableOpts { ui_testing: true, typeck, ..Default::default() },
            ..Opts::default()
        };
        compile_standard_json(input, opts, callback, &mut output);
        normalize_manifest_dir(String::from_utf8(output).unwrap())
    }

    fn assert_json(actual: &str, expected: impl snapbox::IntoData) {
        assert_data_eq!(actual, expected.into_data().is_json());
    }

    fn normalize_manifest_dir(mut output: String) -> String {
        output = output.replace("\\/", "/");
        let native = env!("CARGO_MANIFEST_DIR");
        let slash = native.replace('\\', "/");
        let stripped = slash.strip_prefix("//?/").unwrap_or(&slash).to_string();
        let mut prefixes = vec![native.to_string(), slash, stripped.clone()];
        if let Some((drive, rest)) = stripped.split_once(':') {
            prefixes.push(format!("{}:{rest}", drive.to_ascii_uppercase()));
            prefixes.push(format!("{}:{rest}", drive.to_ascii_lowercase()));
        }
        prefixes.dedup();
        for prefix in prefixes {
            output = output.replace(&prefix, "ROOT");
        }
        while let Some(end) = output.find("/crates/cli") {
            let end = end + "/crates/cli".len();
            let start = output[..end].rfind('"').map_or(0, |i| i + 1);
            output.replace_range(start..end, "ROOT");
        }
        output
    }

    #[test]
    fn compile_without_imports() {
        assert_json(
            &compile(
                r#"{
                "language": "Solidity",
                "sources": {
                    "A.sol": {
                        "content": "contract A { function f() public pure returns (uint) { return 1; } }"
                    }
                },
                "settings": {
                    "outputSelection": { "*": { "*": ["abi"] } }
                }
            }"#,
                None,
            ),
            str![[r#"
{
  "sources": {
    "A.sol": {
      "id": 0
    }
  },
  "contracts": {
    "A.sol": {
      "A": {
        "abi": [
          {
            "inputs": [],
            "name": "f",
            "outputs": [
              {
                "internalType": "uint256",
                "name": "",
                "type": "uint256"
              }
            ],
            "stateMutability": "pure",
            "type": "function"
          }
        ]
      }
    }
  }
}
"#]],
        );
    }

    #[test]
    fn type_errors_are_reported() {
        assert_json(
            &compile_with_typeck(
                r#"{
                "language": "Solidity",
                "sources": {
                    "A.sol": {
                        "content": "contract A { function f() public pure { uint x = true; } }"
                    }
                },
                "settings": {
                    "outputSelection": { "*": { "*": ["abi"] } }
                }
            }"#,
                None,
            ),
            str![[r#"
{
  "errors": [
    {
      "component": "general",
      "errorCode": null,
      "formattedMessage": "error: mismatched types\n   ╭▸ A.sol:1:50\n   │\nLL │ contract A { function f() public pure { uint x = true; } }\n   ╰╴                                                 ━━━━ expected `uint256`, found `bool`\n\n",
      "message": "mismatched types",
      "secondarySourceLocations": [],
      "severity": "error",
      "sourceLocation": {
        "end": 53,
        "file": "A.sol",
        "start": 49
      },
      "type": "Exception"
    }
  ],
  "sources": {
    "A.sol": {
      "id": 0
    }
  }
}
"#]],
        );
    }

    #[test]
    fn import_callback_resolves_source() {
        let mut sources = BTreeMap::new();
        sources.insert(
            "B.sol".to_string(),
            "contract B { function g() public pure returns (uint) { return 2; } }".to_string(),
        );

        assert_json(
            &compile(
                r#"{
                "language": "Solidity",
                "sources": {
                    "A.sol": {
                        "content": "import \"B.sol\"; contract A is B {}"
                    }
                },
                "settings": {
                    "outputSelection": { "*": { "*": ["abi"] } }
                }
            }"#,
                Some(Arc::new(Sources(sources))),
            ),
            str![[r#"
{
  "contracts": {
    "A.sol": {
      "A": {
        "abi": [
          {
            "inputs": [],
            "name": "g",
            "outputs": [
              {
                "internalType": "uint256",
                "name": "",
                "type": "uint256"
              }
            ],
            "stateMutability": "pure",
            "type": "function"
          }
        ]
      }
    },
    "ROOT/B.sol": {
      "B": {
        "abi": [
          {
            "inputs": [],
            "name": "g",
            "outputs": [
              {
                "internalType": "uint256",
                "name": "",
                "type": "uint256"
              }
            ],
            "stateMutability": "pure",
            "type": "function"
          }
        ]
      }
    }
  },
  "sources": {
    "A.sol": {
      "id": 1
    },
    "ROOT/B.sol": {
      "id": 0
    }
  }
}
"#]],
        );
    }

    #[test]
    fn missing_import_callback_is_reported() {
        assert_json(
            &compile(
                r#"{
                "language": "Solidity",
                "sources": {
                    "A.sol": {
                        "content": "import \"Missing.sol\"; contract A {}"
                    }
                }
            }"#,
                None,
            ),
            str![[r#"
{
  "errors": [
    {
      "component": "general",
      "errorCode": null,
      "formattedMessage": "error: couldn't read Missing.sol: File import callback not supported\n   ╭▸ A.sol:1:8\n   │\nLL │ import \"Missing.sol\"; contract A {}\n   ╰╴       ━━━━━━━━━━━━━\n\n",
      "message": "couldn't read Missing.sol: File import callback not supported",
      "secondarySourceLocations": [],
      "severity": "error",
      "sourceLocation": {
        "end": 20,
        "file": "A.sol",
        "start": 7
      },
      "type": "Exception"
    }
  ],
  "sources": {
    "A.sol": {
      "id": 0
    }
  }
}
"#]],
        );
    }
}
