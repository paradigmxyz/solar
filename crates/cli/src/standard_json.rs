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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompilerInput<'a> {
    #[serde(default = "default_language")]
    language: CowStr<'a>,
    #[serde(default)]
    #[serde(borrow)]
    sources: BTreeMap<CowStr<'a>, SourceInput<'a>>,
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
    #[serde(borrow)] BTreeMap<CowStr<'a>, BTreeMap<CowStr<'a>, Vec<CowStr<'a>>>>,
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
    ) -> impl Iterator<Item = &BTreeMap<CowStr<'_>, Vec<CowStr<'_>>>> {
        [source, "*"].into_iter().filter_map(|source| self.0.get(source))
    }
}

fn contract_maps<'a, 'b>(
    contracts: &'a BTreeMap<CowStr<'b>, Vec<CowStr<'b>>>,
    contract: &'a str,
) -> impl Iterator<Item = &'a Vec<CowStr<'b>>> {
    [contract, "*"].into_iter().filter_map(|contract| contracts.get(contract))
}

fn default_language<'a>() -> CowStr<'a> {
    CowStr(Cow::Borrowed("Solidity"))
}

struct StandardJsonFileLoader;

impl FileLoader for StandardJsonFileLoader {
    fn canonicalize_path(&self, path: &Path) -> io::Result<PathBuf> {
        Err(disallowed_io(path))
    }

    fn load_stdin(&self) -> io::Result<String> {
        Err(disallowed_io(Path::new("stdin")))
    }

    fn load_file(&self, path: &Path) -> io::Result<String> {
        Err(disallowed_io(path))
    }

    fn load_binary_file(&self, path: &Path) -> io::Result<Vec<u8>> {
        Err(disallowed_io(path))
    }
}

fn disallowed_io(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!("standard JSON mode cannot read `{}` from the filesystem", path.display()),
    )
}

pub(crate) fn run(mut opts: Opts) -> io::Result<()> {
    let source_map = Arc::new(SourceMap::empty());
    source_map.set_file_loader(StandardJsonFileLoader);
    let (emitter, diagnostics) = InMemoryEmitter::new();
    let dcx = DiagCtxt::new(Box::new(emitter)).with_flags(|flags| flags.update_from_opts(&opts));

    let mut output = CompilerOutput::default();
    let mut input = String::new();
    match io::stdin().read_to_string(&mut input) {
        Ok(_) => {
            if opts.unstable.ui_testing {
                input = strip_json_comments(&input);
            }
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
        }
        Err(e) => {
            dcx.err(format!("failed to read standard JSON input: {e}")).emit();
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

    let stdout = io::stdout();
    let mut stdout = io::BufWriter::new(stdout.lock());
    if opts.pretty_json {
        serde_json::to_writer_pretty(&mut stdout, &output)?;
    } else {
        serde_json::to_writer(&mut stdout, &output)?;
    }
    stdout.write_all(b"\n")?;
    stdout.flush()
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
            let compile_result = crate::run_pipeline(compiler, |pcx| {
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
            });

            output.sources = source_outputs(compiler.sess().source_map());

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

    if output.sources.is_empty() {
        output.sources = source_outputs(&source_map);
    }
}

fn source_outputs(source_map: &SourceMap) -> BTreeMap<String, SourceOutput> {
    source_map
        .files()
        .iter()
        .enumerate()
        .map(|(id, file)| (file.name.display().to_string(), SourceOutput { id: id as u32 }))
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
