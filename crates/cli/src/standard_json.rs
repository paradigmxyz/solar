use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use solar_config::{CompilerStage, EvmVersion, ImportRemapping, Language, Opts};
use solar_interface::{
    SourceMap,
    diagnostics::{DiagCtxt, InMemoryEmitter, solc_diagnostics_to_json},
};
use std::{
    collections::BTreeMap,
    io::{self, Read, Write},
    path::PathBuf,
    str::FromStr,
    sync::Arc,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompilerInput {
    #[serde(default = "default_language")]
    language: String,
    #[serde(default)]
    sources: BTreeMap<String, SourceInput>,
    #[serde(default)]
    settings: Settings,
}

#[derive(Debug, Deserialize)]
struct SourceInput {
    content: Option<String>,
    #[serde(default)]
    urls: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    #[serde(default)]
    remappings: Vec<String>,
    #[serde(default)]
    output_selection: OutputSelection,
    stop_after: Option<String>,
    evm_version: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct OutputSelection(BTreeMap<String, BTreeMap<String, Vec<String>>>);

#[derive(Debug, Default, Serialize)]
struct CompilerOutput {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    errors: Vec<Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    sources: BTreeMap<String, SourceOutput>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    contracts: BTreeMap<String, BTreeMap<String, ContractOutput>>,
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

impl OutputSelection {
    fn selects(&self, source: &str, contract: &str, keys: &[&str]) -> bool {
        self.source_maps(source).any(|contracts| {
            contract_maps(contracts, contract).any(|items| {
                items.iter().any(|item| {
                    item == "*"
                        || keys.iter().any(|key| {
                            item == key
                                || key.strip_prefix(item).is_some_and(|rest| rest.starts_with('.'))
                        })
                })
            })
        })
    }

    fn source_maps(&self, source: &str) -> impl Iterator<Item = &BTreeMap<String, Vec<String>>> {
        [source, "*"].into_iter().filter_map(|source| self.0.get(source))
    }
}

fn contract_maps<'a>(
    contracts: &'a BTreeMap<String, Vec<String>>,
    contract: &'a str,
) -> impl Iterator<Item = &'a Vec<String>> {
    [contract, "*"].into_iter().filter_map(|contract| contracts.get(contract))
}

fn default_language() -> String {
    "Solidity".to_string()
}

pub(crate) fn run(mut opts: Opts) -> io::Result<()> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;

    let output = match serde_json::from_str::<CompilerInput>(&strip_json_comments(&input)) {
        Ok(input) => compile(input, &mut opts),
        Err(e) => CompilerOutput {
            errors: vec![json_error("JSONError", format!("JSON parse error: {e}"))],
            ..Default::default()
        },
    };

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

fn compile(input: CompilerInput, opts: &mut Opts) -> CompilerOutput {
    let mut output = CompilerOutput::default();

    let mut remappings = Vec::with_capacity(input.settings.remappings.len());
    for remapping in &input.settings.remappings {
        match remapping.parse::<ImportRemapping>() {
            Ok(remapping) => remappings.push(remapping),
            Err(e) => output
                .errors
                .push(json_error("JSONError", format!("invalid remapping `{remapping}`: {e}"))),
        }
    }
    if !output.errors.is_empty() {
        return output;
    }

    opts.import_remappings = remappings;
    opts.evm_version = input
        .settings
        .evm_version
        .as_deref()
        .and_then(|version| EvmVersion::from_str(version).ok())
        .unwrap_or(opts.evm_version);
    opts.language = match input.language.as_str() {
        "Solidity" | "solidity" => Language::Solidity,
        "Yul" | "yul" => Language::Yul,
        language => {
            output
                .errors
                .push(json_error("JSONError", format!("unsupported language `{language}`")));
            return output;
        }
    };
    opts.stop_after =
        input.settings.stop_after.as_deref().and_then(|stage| CompilerStage::from_str(stage).ok());
    opts.input = input.sources.keys().cloned().collect();

    let source_map = Arc::new(SourceMap::empty());
    let (emitter, diagnostics) = InMemoryEmitter::new();
    let dcx = DiagCtxt::new(Box::new(emitter)).with_flags(|flags| {
        flags.update_from_opts(opts);
        flags.track_diagnostics = opts.unstable.track_diagnostics;
    });
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
                for (name, source) in sources {
                    let Some(content) = source.content else {
                        let message = if source.urls.is_empty() {
                            format!("source `{name}` is missing `content`")
                        } else {
                            format!("source URLs are not supported for `{name}`")
                        };
                        return Err(pcx.dcx().err(message).emit());
                    };
                    let file =
                        match pcx.sess.source_map().new_source_file(PathBuf::from(name), content) {
                            Ok(file) => file,
                            Err(e) => {
                                return Err(pcx
                                    .dcx()
                                    .err(format!("failed to load source: {e}"))
                                    .emit());
                            }
                        };
                    pcx.add_file(file);
                }
                Ok(())
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

    let emitted = diagnostics.read().clone();
    output.errors = solc_diagnostics_to_json(
        &emitted,
        Arc::clone(&source_map),
        opts.unstable.ui_testing,
        opts.error_format_human,
        opts.diagnostic_width,
    );

    if has_error(&output.errors) {
        output.contracts.clear();
    }

    output
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
    output_selection: &OutputSelection,
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

fn has_error(errors: &[Value]) -> bool {
    errors.iter().any(|error| error.get("severity").and_then(Value::as_str) == Some("error"))
}

fn json_error(error_type: &str, message: String) -> Value {
    json!({
        "component": "general",
        "errorCode": null,
        "formattedMessage": message,
        "message": message,
        "severity": "error",
        "type": error_type,
    })
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
