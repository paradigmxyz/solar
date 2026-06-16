use solar_codegen::{Backend, EvmCodegen, lower, mir::module_to_text};
use solar_config::CompilerOutput;
use solar_data_structures::map::{FxHashMap, FxHashSet};
use solar_interface::Result;
use solar_sema::{CompilerRef, Gcx, hir::ContractId};
use std::{
    collections::BTreeMap,
    io::{self, Write},
    path::Path,
};

type Hashes = BTreeMap<String, String>;

#[derive(Default, serde::Serialize)]
struct SemaCombinedJson<Abi> {
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    contracts: BTreeMap<String, SemaCombinedJsonContract<Abi>>,
    version: &'static str,
}

#[derive(Default, serde::Serialize)]
struct SemaCombinedJsonContract<Abi> {
    #[serde(skip_serializing_if = "Option::is_none")]
    abi: Option<Abi>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hashes: Option<Hashes>,
}

#[derive(Default, serde::Serialize)]
struct CodegenCombinedJson {
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    contracts: BTreeMap<String, CodegenCombinedJsonContract>,
    version: &'static str,
}

#[derive(Default, serde::Serialize)]
struct CodegenCombinedJsonContract {
    #[serde(skip_serializing_if = "Option::is_none")]
    abi: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bin: Option<String>,
    #[serde(rename = "bin-runtime", skip_serializing_if = "Option::is_none")]
    bin_runtime: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hashes: Option<Hashes>,
}

pub(crate) fn emit_requested(compiler: &mut CompilerRef<'_>) -> Result {
    let gcx = compiler.gcx();
    emit_sema_json(gcx)?;
    emit_mir(gcx)?;
    emit_codegen_json(gcx)
}

fn emit_sema_json(gcx: Gcx<'_>) -> Result {
    let sess = gcx.sess;
    let emit_abi = sess.opts.emit.contains(&CompilerOutput::Abi);
    let emit_hashes = sess.opts.emit.contains(&CompilerOutput::Hashes);

    if !emit_abi && !emit_hashes {
        return Ok(());
    }

    let mut output = SemaCombinedJson {
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
    }

    write_output_json(gcx, &output, false)
}

fn emit_codegen_json(gcx: Gcx<'_>) -> Result {
    let sess = gcx.sess;
    let emit_abi = sess.opts.emit.contains(&CompilerOutput::Abi);
    let emit_hashes = sess.opts.emit.contains(&CompilerOutput::Hashes);
    let emit_bin = sess.opts.emit.contains(&CompilerOutput::Bin);
    let emit_bin_runtime = sess.opts.emit.contains(&CompilerOutput::BinRuntime);

    if !emit_bin && !emit_bin_runtime {
        return Ok(());
    }

    let bytecodes =
        if emit_bin || emit_bin_runtime { Some(generate_contract_bytecodes(gcx)?) } else { None };

    let mut output = CodegenCombinedJson {
        contracts: BTreeMap::default(),
        version: solar_config::version::SEMVER_VERSION,
    };

    for id in gcx.hir.contract_ids() {
        let name = contract_output_name(gcx, id);
        let contract_output = output.contracts.entry(name).or_default();

        if emit_abi {
            contract_output.abi = Some(serde_json::to_value(gcx.contract_abi(id)).unwrap());
        }
        if emit_hashes {
            contract_output.hashes = Some(contract_hashes(gcx, id));
        }

        if let Some(bytecodes) = &bytecodes
            && let Some(bytecode) = bytecodes.get(&id)
        {
            if emit_bin {
                contract_output.bin = Some(bytecode.deployment.clone());
            }
            if emit_bin_runtime {
                contract_output.bin_runtime = Some(bytecode.runtime.clone());
            }
        }
    }

    write_output_json(gcx, &output, true)
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
    if !gcx.sess.opts.emit.contains(&CompilerOutput::Mir) {
        return Ok(());
    }

    for id in gcx.hir.contract_ids() {
        let contract = gcx.hir.contract(id);
        if contract.kind.is_interface() || contract.kind.is_abstract_contract() {
            continue;
        }
        let module = lower::lower_contract(gcx, id);
        gcx.dcx().has_errors()?;
        let name = gcx.contract_fully_qualified_name(id);
        println!("// === {name} ===");
        println!("{}", module_to_text(&module));
    }

    Ok(())
}

#[derive(Clone)]
struct GeneratedBytecodes {
    deployment: String,
    runtime: String,
}

fn generate_contract_bytecodes(gcx: Gcx<'_>) -> Result<FxHashMap<ContractId, GeneratedBytecodes>> {
    let mut all_bytecodes = FxHashMap::default();
    let mut visiting = FxHashSet::default();
    for id in gcx.hir.contract_ids() {
        let contract = gcx.hir.contract(id);
        if !contract.kind.is_interface() && !contract.kind.is_abstract_contract() {
            ensure_contract_bytecode(gcx, id, &mut all_bytecodes, &mut visiting)?;
        }
    }

    let mut bytecodes = FxHashMap::default();
    for id in gcx.hir.contract_ids() {
        let contract = gcx.hir.contract(id);
        if contract.kind.is_interface() || contract.kind.is_abstract_contract() {
            continue;
        }

        let mut module = lower::lower_contract_with_bytecodes(gcx, id, &all_bytecodes);
        gcx.dcx().has_errors()?;
        let mut codegen = EvmCodegen::new(gcx);
        let artifact = codegen.lower_module(&mut module);
        bytecodes.insert(
            id,
            GeneratedBytecodes {
                deployment: alloy_primitives::hex::encode(artifact.deployment),
                runtime: alloy_primitives::hex::encode(artifact.runtime),
            },
        );
    }

    Ok(bytecodes)
}

fn ensure_contract_bytecode(
    gcx: Gcx<'_>,
    contract_id: ContractId,
    all_bytecodes: &mut FxHashMap<ContractId, Vec<u8>>,
    visiting: &mut FxHashSet<ContractId>,
) -> Result {
    if all_bytecodes.contains_key(&contract_id) {
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

    for dep in lower::contract_bytecode_dependencies(gcx, contract_id) {
        ensure_contract_bytecode(gcx, dep, all_bytecodes, visiting)?;
    }

    let mut module = lower::lower_contract_with_bytecodes(gcx, contract_id, all_bytecodes);
    gcx.dcx().has_errors()?;
    let mut codegen = EvmCodegen::new(gcx);
    let artifact = codegen.lower_module(&mut module);
    all_bytecodes.insert(contract_id, artifact.deployment);
    visiting.remove(&contract_id);

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
