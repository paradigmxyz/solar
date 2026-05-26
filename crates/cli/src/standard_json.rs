use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use solar_config::{CompilerStage, EvmVersion, ImportRemapping, Language};
use solar_interface::{
    SourceMap,
    diagnostics::{InMemoryEmitter, solc_diagnostics_to_json},
};
use solar_sema::CompilerRef;
use std::{
    collections::BTreeMap,
    io::{self, Read as _, Write as _},
    path::PathBuf,
    str::FromStr,
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

pub(crate) fn run_in_default(compiler: &mut CompilerRef<'_>) -> solar_interface::Result {
    let (emitter, diagnostics) = InMemoryEmitter::new();
    let previous_emitter = compiler.dcx().set_emitter(Box::new(emitter));
    compiler.dcx().set_flags(|flags| {
        flags.update_from_opts(&compiler.sess().opts);
        flags.track_diagnostics = compiler.sess().opts.unstable.track_diagnostics;
    });

    let output = compile(compiler);
    let source_map = compiler.sess().clone_source_map();
    let emitted = diagnostics.read().clone();
    let errors = solc_diagnostics_to_json(
        &emitted,
        source_map,
        compiler.sess().opts.unstable.ui_testing,
        compiler.sess().opts.error_format_human,
        compiler.sess().opts.diagnostic_width,
    );
    compiler.dcx().set_emitter(previous_emitter);

    write_output(compiler, CompilerOutput { errors, ..output })
}

fn compile(compiler: &mut CompilerRef<'_>) -> CompilerOutput {
    let mut output = CompilerOutput::default();

    let mut input_json = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut input_json) {
        compiler.dcx().err(format!("failed to read standard JSON input: {e}")).emit();
        return output;
    }

    let input = match serde_json::from_str::<CompilerInput>(&strip_json_comments(&input_json)) {
        Ok(input) => input,
        Err(e) => {
            compiler.dcx().err(format!("JSON parse error: {e}")).emit();
            return output;
        }
    };

    let mut remappings = Vec::with_capacity(input.settings.remappings.len());
    for remapping in &input.settings.remappings {
        match remapping.parse::<ImportRemapping>() {
            Ok(remapping) => remappings.push(remapping),
            Err(e) => {
                compiler.dcx().err(format!("invalid remapping `{remapping}`: {e}")).emit();
            }
        }
    }
    if compiler.dcx().has_errors().is_err() {
        return output;
    }

    let _evm_version = input
        .settings
        .evm_version
        .as_deref()
        .and_then(|version| EvmVersion::from_str(version).ok());
    let language = match input.language.as_str() {
        "Solidity" | "solidity" => Language::Solidity,
        "Yul" | "yul" => Language::Yul,
        language => {
            compiler.dcx().err(format!("unsupported language `{language}`")).emit();
            return output;
        }
    };
    if language.is_yul() && !compiler.sess().opts.unstable.parse_yul {
        compiler.dcx().err("Yul is not supported yet").emit();
        return output;
    }
    let _stop_after =
        input.settings.stop_after.as_deref().and_then(|stage| CompilerStage::from_str(stage).ok());

    let output_selection = input.settings.output_selection;
    let sources = input.sources;

    let compile_result = crate::run_pipeline(compiler, |pcx| {
        pcx.file_resolver.add_import_remappings(remappings);
        for (name, source) in sources {
            let Some(content) = source.content else {
                let message = if source.urls.is_empty() {
                    format!("source `{name}` is missing `content`")
                } else {
                    format!("source URLs are not supported for `{name}`")
                };
                return Err(pcx.dcx().err(message).emit());
            };
            let file = match pcx.sess.source_map().new_source_file(PathBuf::from(name), content) {
                Ok(file) => file,
                Err(e) => return Err(pcx.dcx().err(format!("failed to load source: {e}")).emit()),
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

    output
}

fn write_output(compiler: &CompilerRef<'_>, mut output: CompilerOutput) -> solar_interface::Result {
    if has_error(&output.errors) {
        output.contracts.clear();
    }

    let stdout = io::stdout();
    let mut stdout = io::BufWriter::new(stdout.lock());
    let result = (|| {
        if compiler.sess().opts.pretty_json {
            serde_json::to_writer_pretty(&mut stdout, &output).map_err(|e| e.to_string())?;
        } else {
            serde_json::to_writer(&mut stdout, &output).map_err(|e| e.to_string())?;
        }
        stdout.write_all(b"\n").map_err(|e| e.to_string())?;
        stdout.flush().map_err(|e| e.to_string())
    })();
    result.map_err(|e| {
        compiler.dcx().err(format!("failed to write standard JSON output: {e}")).emit()
    })
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
