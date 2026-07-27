use alloy_json_abi::AbiItem;
use alloy_primitives::Bytes;
use solar_codegen::{
    ContractArtifact, ContractSelection, backend::evm::ir, generate_contract_bytecodes,
};
use solar_config::{CompilerOutput, Dump, DumpKind};
use solar_data_structures::map::FxHashMap;
use solar_interface::Result;
use solar_sema::{CompilerRef, Gcx, hir::ContractId};
use std::{
    collections::BTreeMap,
    io::{self, Write},
    path::Path,
    sync::Arc,
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

pub(crate) fn emit_requested(
    compiler: &mut CompilerRef<'_>,
    bytecode_contracts: ContractSelection,
) -> Result<Option<FxHashMap<ContractId, ContractArtifact>>> {
    let gcx = compiler.gcx();
    let dump_contracts = codegen_dump_contracts(gcx)?;
    let no_contracts = ContractSelection::empty(gcx);
    let capture_mir =
        dump_contracts.as_ref().filter(|_| has_mir_dump(gcx)).unwrap_or(&no_contracts);
    let capture_evm_ir =
        dump_contracts.as_ref().filter(|_| has_evm_ir_dump(gcx)).unwrap_or(&no_contracts);
    let generate_artifacts = !bytecode_contracts.is_empty() || dump_contracts.is_some();
    let artifacts = if generate_artifacts {
        Some(generate_contract_bytecodes(gcx, &bytecode_contracts, capture_mir, capture_evm_ir)?)
    } else {
        None
    };

    if let Some(contracts) = &dump_contracts
        && has_mir_dump(gcx)
    {
        dump_mir(gcx, contracts, artifacts.as_ref().expect("artifacts should be generated"))?;
    }
    emit_combined_json(gcx, artifacts.as_ref())?;
    if let Some(contracts) = &dump_contracts
        && has_evm_ir_dump(gcx)
    {
        dump_evm_ir(gcx, contracts, artifacts.as_ref().expect("artifacts should be generated"))?;
    }
    Ok(artifacts)
}

fn emit_combined_json(
    gcx: Gcx<'_>,
    artifacts: Option<&FxHashMap<ContractId, ContractArtifact>>,
) -> Result {
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

        if let Some(bytecode) = artifacts.and_then(|artifacts| artifacts.get(&id)) {
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

fn has_mir_dump(gcx: Gcx<'_>) -> bool {
    gcx.sess.opts.unstable.dump.as_ref().is_some_and(|dump| {
        dump.kinds.iter().any(|kind| matches!(kind, DumpKind::Mir | DumpKind::MirCfg))
    })
}

fn has_evm_ir_dump(gcx: Gcx<'_>) -> bool {
    gcx.sess.opts.unstable.dump.as_ref().is_some_and(|dump| {
        dump.kinds.iter().any(|kind| matches!(kind, DumpKind::EvmIr | DumpKind::EvmIrRuntime))
    })
}

fn codegen_dump_contracts(gcx: Gcx<'_>) -> Result<Option<ContractSelection>> {
    let Some(dump) = &gcx.sess.opts.unstable.dump else { return Ok(None) };
    if !dump.needs_codegen() {
        return Ok(None);
    }
    matching_dump_contracts(gcx, dump).map(Some)
}

fn dump_mir(
    gcx: Gcx<'_>,
    contracts: &ContractSelection,
    artifacts: &FxHashMap<ContractId, ContractArtifact>,
) -> Result {
    let sess = gcx.sess;
    let dump = sess.opts.unstable.dump.as_ref().expect("dump options should be present");
    let mut writer = out_writer(None)
        .map_err(|e| sess.dcx.err(format!("failed to write to output: {e}")).emit())?;
    for id in contracts.into_iter(gcx) {
        dump_mir_contract(&mut writer, gcx, dump, id, artifacts)?;
    }
    writer.flush().map_err(|e| sess.dcx.err(format!("failed to write to output: {e}")).emit())?;

    Ok(())
}

fn dump_mir_contract(
    writer: &mut impl Write,
    gcx: Gcx<'_>,
    dump: &Dump,
    id: ContractId,
    artifacts: &FxHashMap<ContractId, ContractArtifact>,
) -> Result {
    let Some(module) = artifacts.get(&id).and_then(|artifact| artifact.mir.as_ref()) else {
        return Ok(());
    };
    if dump.kinds.contains(&DumpKind::Mir) {
        write_mir_dump_contract(writer, gcx, id, module, DumpKind::Mir)?;
    }
    if dump.kinds.contains(&DumpKind::MirCfg) {
        write_mir_dump_contract(writer, gcx, id, module, DumpKind::MirCfg)?;
    }
    Ok(())
}

fn is_dumpable_contract(gcx: Gcx<'_>, id: ContractId) -> bool {
    let contract = gcx.hir.contract(id);
    !contract.kind.is_interface() && !contract.kind.is_abstract_contract()
}

fn contract_dump_path_matches(gcx: Gcx<'_>, id: ContractId, path: &str) -> bool {
    let contract = gcx.hir.contract(id);
    let source = gcx.hir.source(contract.source);
    if gcx.get_file(path.to_owned()).is_some_and(|file| Arc::ptr_eq(&file, &source.file)) {
        return true;
    }

    let path = path.replace('\\', "/");
    path == gcx.contract_fully_qualified_name(id).to_string().replace('\\', "/")
}

fn matching_dump_contracts(gcx: Gcx<'_>, dump: &Dump) -> Result<ContractSelection> {
    let Some(paths) = dump.paths.as_deref() else {
        return Ok(ContractSelection::All);
    };

    let mut contracts = ContractSelection::empty(gcx);
    for path in paths {
        let mut matched = false;
        for id in gcx.hir.contract_ids() {
            if !is_dumpable_contract(gcx, id) || !contract_dump_path_matches(gcx, id, path) {
                continue;
            }
            matched = true;
            contracts.insert(id);
        }
        if !matched {
            let kinds = dump.kinds.iter().map(ToString::to_string).collect::<Vec<_>>().join(",");
            let msg = format!("`-Zdump={kinds}={path}` did not match any contract");
            let note = format!("available contracts: {}", available_dump_contracts(gcx));
            return Err(gcx.sess.dcx.err(msg).note(note).emit());
        }
    }
    Ok(contracts)
}

fn write_mir_dump_contract(
    writer: &mut impl Write,
    gcx: Gcx<'_>,
    id: ContractId,
    module: &solar_codegen::mir::Module,
    kind: DumpKind,
) -> Result {
    let name = gcx.contract_fully_qualified_name(id);
    writeln!(writer, "// === {name} ===")
        .map_err(|e| gcx.sess.dcx.err(format!("failed to write to output: {e}")).emit())?;
    match kind {
        DumpKind::Mir | DumpKind::MirEvmShaped => writeln!(writer, "{module}"),
        DumpKind::MirCfg => writeln!(writer, "{}", module.to_dot()),
        _ => unreachable!("checked by caller"),
    }
    .map_err(|e| gcx.sess.dcx.err(format!("failed to write to output: {e}")).emit())?;
    Ok(())
}

fn available_dump_contracts(gcx: Gcx<'_>) -> String {
    gcx.hir
        .contract_ids()
        .filter(|&id| is_dumpable_contract(gcx, id))
        .map(|id| gcx.contract_fully_qualified_name(id).to_string().replace('\\', "/"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn dump_evm_ir(
    gcx: Gcx<'_>,
    contracts: &ContractSelection,
    artifacts: &FxHashMap<ContractId, ContractArtifact>,
) -> Result {
    let sess = gcx.sess;
    let dump = sess.opts.unstable.dump.as_ref().expect("dump options should be present");
    let mut writer = out_writer(None)
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
    for id in contracts.into_iter(gcx) {
        write_evm_ir_dump_contract(&mut writer, gcx, dump, id, artifacts)?;
    }
    writer.flush().map_err(|e| sess.dcx.err(format!("failed to write to output: {e}")).emit())?;
    Ok(())
}

fn write_evm_ir_dump_contract(
    writer: &mut impl Write,
    gcx: Gcx<'_>,
    dump: &Dump,
    id: ContractId,
    artifacts: &FxHashMap<ContractId, ContractArtifact>,
) -> Result {
    let Some(artifact) = artifacts.get(&id) else { return Ok(()) };
    let name = gcx.contract_fully_qualified_name(id);
    if dump.kinds.contains(&DumpKind::EvmIr) {
        writeln!(writer, "// === {name} (creation) ===")
            .map_err(|e| gcx.sess.dcx.err(format!("failed to write to output: {e}")).emit())?;
        write!(
            writer,
            "{}",
            format_deployment_evm_ir(
                artifact.deployment_evm_ir.as_ref(),
                artifact.runtime_evm_ir.as_ref()
            )
        )
        .map_err(|e| gcx.sess.dcx.err(format!("failed to write to output: {e}")).emit())?;
    }
    if dump.kinds.contains(&DumpKind::EvmIrRuntime) {
        writeln!(writer, "// === {name} (runtime) ===")
            .map_err(|e| gcx.sess.dcx.err(format!("failed to write to output: {e}")).emit())?;
        if let Some(runtime_evm_ir) = &artifact.runtime_evm_ir {
            write!(writer, "{}", runtime_evm_ir.to_text())
                .map_err(|e| gcx.sess.dcx.err(format!("failed to write to output: {e}")).emit())?;
        }
    }
    Ok(())
}

fn serialize_hex_bytes<S>(bytes: &Option<Bytes>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let Some(bytes) = bytes else { return serializer.serialize_none() };
    serializer.serialize_str(&alloy_primitives::hex::encode(bytes))
}

pub(crate) fn format_deployment_evm_ir(
    deployment: Option<&ir::Module>,
    runtime: Option<&ir::Module>,
) -> String {
    use std::fmt::Write;

    let mut output = String::new();
    for (index, module) in deployment.into_iter().chain(runtime).enumerate() {
        if index != 0 {
            output.push('\n');
        }
        writeln!(output, "// === {} ===", module.name()).unwrap();
        write!(output, "{}", module.to_text()).unwrap();
    }
    output
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
