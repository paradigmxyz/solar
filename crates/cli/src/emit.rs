use alloy_json_abi::AbiItem;
use alloy_primitives::Bytes;
use solar_codegen::{Backend, EvmCodegen, EvmCodegenConfig, backend::evm::ir, lower};
use solar_config::CompilerOutput;
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};
use solar_interface::Result;
use solar_sema::{CompilerRef, Gcx, hir::ContractId};
use std::{
    collections::BTreeMap,
    io::{self, Write},
    path::Path,
};

type Hashes = BTreeMap<String, String>;

#[derive(Default, serde::Serialize)]
struct CombinedJson<'a> {
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    contracts: BTreeMap<String, CombinedJsonContract<'a>>,
    version: &'static str,
}

#[derive(Default, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
struct CombinedJsonContract<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    abi: Option<Vec<AbiItem<'a>>>,
    #[serde(serialize_with = "serialize_hex_bytes", skip_serializing_if = "Option::is_none")]
    bin: Option<Bytes>,
    #[serde(serialize_with = "serialize_hex_bytes", skip_serializing_if = "Option::is_none")]
    bin_runtime: Option<Bytes>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hashes: Option<Hashes>,
}

pub(crate) fn emit_requested(compiler: &mut CompilerRef<'_>) -> Result {
    let gcx = compiler.gcx();
    emit_combined_json(gcx)?;
    emit_mir(gcx)?;
    emit_evm_ir(gcx)
}

fn emit_combined_json(gcx: Gcx<'_>) -> Result {
    let sess = gcx.sess;
    let (mut emit_abi, mut emit_hashes, mut emit_bin, mut emit_bin_runtime) =
        (false, false, false, false);
    for output in &sess.opts.emit {
        match output {
            CompilerOutput::Abi => emit_abi = true,
            CompilerOutput::Hashes => emit_hashes = true,
            CompilerOutput::Bin => emit_bin = true,
            CompilerOutput::BinRuntime => emit_bin_runtime = true,
            _ => {}
        }
    }

    if !emit_abi && !emit_hashes && !emit_bin && !emit_bin_runtime {
        return Ok(());
    }

    let bytecodes = if emit_bin || emit_bin_runtime {
        Some(generate_contract_bytecodes(gcx, false)?)
    } else {
        None
    };

    let mut output = CombinedJson {
        contracts: BTreeMap::default(),
        version: solar_config::version::SEMVER_VERSION,
    };

    for id in gcx.hir.contract_ids() {
        let name = contract_output_name(gcx, id);
        let contract_output = output.contracts.entry(name).or_default();

        if emit_abi {
            contract_output.abi = Some(gcx.contract_abi(id));
        }
        if emit_hashes {
            contract_output.hashes = Some(contract_hashes(gcx, id));
        }

        if let Some(bytecode) = bytecodes.as_ref().and_then(|bytecodes| bytecodes.get(&id)) {
            if emit_bin {
                contract_output.bin = Some(bytecode.deployment.clone());
            }
            if emit_bin_runtime {
                contract_output.bin_runtime = Some(bytecode.runtime.clone());
            }
        }
    }

    write_output_json(gcx, &output, emit_bin || emit_bin_runtime)
}

fn write_output_json<T: serde::Serialize>(
    gcx: Gcx<'_>,
    output: &T,
    trailing_newline: bool,
) -> Result {
    let sess = gcx.sess;
    let out_path = sess.opts.out_dir.as_deref().map(|dir| dir.join("combined.json"));
    let mut writer = out_writer(out_path.as_deref())
        .map_err(|e| sess.dcx.err(format!("failed to write to output: {e}")).emit())?;
    to_json(&mut writer, &output, sess.opts.pretty_json)
        .map_err(|e| sess.dcx.err(format!("failed to write to output: {e}")).emit())?;
    if trailing_newline {
        writer
            .write_all(b"\n")
            .map_err(|e| sess.dcx.err(format!("failed to write to output: {e}")).emit())?;
    }
    writer.flush().map_err(|e| sess.dcx.err(format!("failed to write to output: {e}")).emit())?;

    Ok(())
}

fn contract_output_name(gcx: Gcx<'_>, id: ContractId) -> String {
    let contract = gcx.hir.contract(id);
    let source = gcx.hir.source(contract.source);
    format!("{}:{}", source.file.name.display().to_string().replace('\\', "/"), contract.name)
}

fn emit_mir(gcx: Gcx<'_>) -> Result {
    let sess = gcx.sess;
    if !sess.opts.emit.contains(&CompilerOutput::Mir) {
        return Ok(());
    }

    let out_path = sess.opts.out_dir.as_deref().map(|dir| dir.join("combined.mir"));
    let mut writer = out_writer(out_path.as_deref())
        .map_err(|e| sess.dcx.err(format!("failed to write to output: {e}")).emit())?;
    for id in gcx.hir.contract_ids() {
        let contract = gcx.hir.contract(id);
        if contract.kind.is_interface() || contract.kind.is_abstract_contract() {
            continue;
        }
        let module = lower::lower_contract(gcx, id);
        gcx.dcx().has_errors()?;
        let name = gcx.contract_fully_qualified_name(id);
        writeln!(writer, "// === {name} ===")
            .map_err(|e| sess.dcx.err(format!("failed to write to output: {e}")).emit())?;
        writeln!(writer, "{}", module.to_text())
            .map_err(|e| sess.dcx.err(format!("failed to write to output: {e}")).emit())?;
    }
    writer.flush().map_err(|e| sess.dcx.err(format!("failed to write to output: {e}")).emit())?;

    Ok(())
}

fn emit_evm_ir(gcx: Gcx<'_>) -> Result {
    let sess = gcx.sess;
    let emit_deployment = sess.opts.emit.contains(&CompilerOutput::EvmIr);
    let emit_runtime = sess.opts.emit.contains(&CompilerOutput::EvmIrRuntime);
    if !emit_deployment && !emit_runtime {
        return Ok(());
    }

    let bytecodes = generate_contract_bytecodes(gcx, true)?;
    let out_path = sess.opts.out_dir.as_deref().map(|dir| dir.join("combined.evmir"));
    let mut writer = out_writer(out_path.as_deref())
        .map_err(|e| sess.dcx.err(format!("failed to write to output: {e}")).emit())?;
    if sess.opts.out_dir.is_none()
        && sess
            .opts
            .emit
            .iter()
            .any(|output| matches!(output, CompilerOutput::Abi | CompilerOutput::Hashes))
    {
        writeln!(writer)
            .map_err(|e| sess.dcx.err(format!("failed to write to output: {e}")).emit())?;
    }
    for id in gcx.hir.contract_ids() {
        let Some(bytecode) = bytecodes.get(&id) else { continue };
        let name = gcx.contract_fully_qualified_name(id);
        if emit_deployment {
            writeln!(writer, "// === {name} (creation) ===")
                .map_err(|e| sess.dcx.err(format!("failed to write to output: {e}")).emit())?;
            write!(writer, "{}", bytecode.deployment_evm_ir.as_deref().unwrap_or_default())
                .map_err(|e| sess.dcx.err(format!("failed to write to output: {e}")).emit())?;
        }
        if emit_runtime {
            writeln!(writer, "// === {name} (runtime) ===")
                .map_err(|e| sess.dcx.err(format!("failed to write to output: {e}")).emit())?;
            write!(writer, "{}", bytecode.runtime_evm_ir.as_deref().unwrap_or_default())
                .map_err(|e| sess.dcx.err(format!("failed to write to output: {e}")).emit())?;
        }
    }
    writer.flush().map_err(|e| sess.dcx.err(format!("failed to write to output: {e}")).emit())?;
    Ok(())
}

struct GeneratedBytecodes {
    deployment: Bytes,
    runtime: Bytes,
    deployment_evm_ir: Option<String>,
    runtime_evm_ir: Option<String>,
}

fn generate_contract_bytecodes(
    gcx: Gcx<'_>,
    capture_evm_ir: bool,
) -> Result<FxHashMap<ContractId, GeneratedBytecodes>> {
    let mut all_bytecodes = FxHashMap::default();
    let mut artifacts = FxHashMap::default();
    let mut visiting = DenseBitSet::new_empty(gcx.hir.contract_ids().len());
    for id in gcx.hir.contract_ids() {
        let contract = gcx.hir.contract(id);
        if !contract.kind.is_interface() && !contract.kind.is_abstract_contract() {
            ensure_contract_bytecode(
                gcx,
                id,
                capture_evm_ir,
                &mut all_bytecodes,
                &mut artifacts,
                &mut visiting,
            )?;
        }
    }
    Ok(artifacts)
}

fn serialize_hex_bytes<S>(bytes: &Option<Bytes>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let Some(bytes) = bytes else { return serializer.serialize_none() };
    serializer.serialize_str(&alloy_primitives::hex::encode(bytes))
}

pub(crate) fn format_deployment_evm_ir(modules: &[ir::Module]) -> String {
    use std::fmt::Write;

    let mut output = String::new();
    for (index, module) in modules.iter().enumerate() {
        if index != 0 {
            output.push('\n');
        }
        writeln!(output, "// === {} ===", module.name()).unwrap();
        write!(output, "{}", module.to_text()).unwrap();
    }
    output
}

fn ensure_contract_bytecode(
    gcx: Gcx<'_>,
    contract_id: ContractId,
    capture_evm_ir: bool,
    all_bytecodes: &mut FxHashMap<ContractId, Vec<u8>>,
    artifacts: &mut FxHashMap<ContractId, GeneratedBytecodes>,
    visiting: &mut DenseBitSet<ContractId>,
) -> Result {
    if artifacts.contains_key(&contract_id) {
        return Ok(());
    }

    let contract = gcx.hir.contract(contract_id);
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

    for dep in &lower::contract_bytecode_dependencies(gcx, contract_id) {
        ensure_contract_bytecode(gcx, dep, capture_evm_ir, all_bytecodes, artifacts, visiting)?;
    }

    let mut module = lower::lower_contract_with_bytecodes(gcx, contract_id, all_bytecodes);
    gcx.dcx().has_errors()?;
    let mut codegen =
        EvmCodegen::new(EvmCodegenConfig { capture_evm_ir, ..EvmCodegenConfig::from(gcx) });
    let artifact = codegen.lower_module(&mut module);
    all_bytecodes.insert(contract_id, artifact.deployment.clone());
    artifacts.insert(
        contract_id,
        GeneratedBytecodes {
            deployment: artifact.deployment.into(),
            runtime: artifact.runtime.into(),
            deployment_evm_ir: capture_evm_ir
                .then(|| format_deployment_evm_ir(&artifact.deployment_evm_ir)),
            runtime_evm_ir: artifact.runtime_evm_ir.map(|ir| ir.to_text().to_string()),
        },
    );
    visiting.remove(contract_id);

    Ok(())
}

fn contract_hashes(gcx: Gcx<'_>, id: ContractId) -> Hashes {
    let mut hashes = Hashes::default();
    for function in gcx.interface_functions(id) {
        hashes.insert(
            gcx.item_signature(function.id.into()).to_string(),
            alloy_primitives::hex::encode(function.selector),
        );
    }
    hashes
}

fn out_writer(path: Option<&Path>) -> io::Result<impl io::Write> {
    let out: Box<dyn io::Write> = if let Some(path) = path {
        Box::new(std::fs::File::create(path)?)
    } else {
        Box::new(std::io::stdout())
    };
    Ok(io::BufWriter::new(out))
}

fn to_json<W: io::Write, T: serde::Serialize>(
    writer: W,
    value: &T,
    pretty: bool,
) -> serde_json::Result<()> {
    if pretty {
        serde_json::to_writer_pretty(writer, value)
    } else {
        serde_json::to_writer(writer, value)
    }
}
