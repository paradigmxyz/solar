use crate::bytecode::MaybeHexBytecode;
use alloy_json_abi::AbiItem;
use anstyle::{AnsiColor, Color, Style};
use solar_codegen::{
    ContractArtifact, ContractSelection, RuntimeDataFn,
    backend::evm::{self, ir},
    generate_contract_bytecodes,
    mir::{Module, validate},
    pass,
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
    abi: Option<&'a [AbiItem<'a>]>,
    /// Hex bytecode; unresolved library addresses print as solc's `__$<hash>$__` placeholders.
    #[serde(skip_serializing_if = "Option::is_none")]
    bin: Option<MaybeHexBytecode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bin_runtime: Option<MaybeHexBytecode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hashes: Option<Hashes>,
}

pub(crate) fn emit_requested(
    compiler: &mut CompilerRef<'_>,
    bytecode_contracts: ContractSelection,
    runtime_data: Option<&RuntimeDataFn<'_>>,
) -> Result<Option<FxHashMap<ContractId, ContractArtifact>>> {
    let gcx = compiler.gcx();
    if !gcx.sess.opts.language.is_source() {
        emit_ir_input(gcx)?;
        return Ok(None);
    }

    let dump_contracts = codegen_dump_contracts(gcx)?;
    let pipeline_mir_output = mir_pipeline_output_requested(gcx);
    let mut capture_mir = ContractSelection::empty(gcx);
    if let Some(contracts) = dump_contracts.as_ref().filter(|_| has_mir_dump(gcx)) {
        capture_mir.union_with(contracts);
    }
    if pipeline_mir_output {
        capture_mir.union_with(&ContractSelection::All);
    }
    let mut capture_evm_ir = ContractSelection::empty(gcx);
    if let Some(contracts) = dump_contracts.as_ref().filter(|_| has_evm_ir_dump(gcx)) {
        capture_evm_ir.union_with(contracts);
    }
    let mut generated_bytecode_contracts = bytecode_contracts;
    if has_disasm_dump(gcx)
        && let Some(contracts) = &dump_contracts
    {
        generated_bytecode_contracts.union_with(contracts);
    }
    let generate_artifacts = !generated_bytecode_contracts.is_empty()
        || !capture_mir.is_empty()
        || !capture_evm_ir.is_empty();
    let artifacts = if generate_artifacts {
        Some(generate_contract_bytecodes(
            gcx,
            &generated_bytecode_contracts,
            &capture_mir,
            &capture_evm_ir,
            runtime_data,
        )?)
    } else {
        None
    };

    if let Some(contracts) = &dump_contracts
        && has_mir_dump(gcx)
    {
        dump_mir(gcx, contracts, artifacts.as_ref().expect("artifacts should be generated"))?;
    }
    if pipeline_mir_output {
        emit_mir_pipeline_output(gcx, artifacts.as_ref().expect("artifacts should be generated"))?;
    }
    emit_combined_json(gcx, artifacts.as_ref())?;
    if let Some(contracts) = &dump_contracts
        && has_evm_ir_dump(gcx)
    {
        dump_evm_ir(gcx, contracts, artifacts.as_ref().expect("artifacts should be generated"))?;
    }
    if let Some(contracts) = &dump_contracts
        && has_disasm_dump(gcx)
    {
        dump_disassembly(
            gcx,
            contracts,
            artifacts.as_ref().expect("artifacts should be generated"),
        )?;
    }
    Ok(artifacts)
}

fn emit_ir_input(gcx: Gcx<'_>) -> Result {
    let source = &gcx.sources.first().expect("IR source should be loaded").file;
    if gcx.sess.opts.language.is_mir() {
        let mut module = Module::parse(gcx.sess, source)?;
        validate(&gcx.sess.dcx, &module);
        if gcx.dcx().has_errors().is_ok() {
            let name = source.name.display().to_string();
            let _changed = pass::run_pipeline(gcx, &mut module, Some(&name));
            validate(&gcx.sess.dcx, &module);
            gcx.dcx().has_errors()?;

            let value = gcx
                .sess
                .opts
                .unstable
                .mir_pipeline
                .as_deref()
                .expect("MIR pipeline should be configured");
            if should_print_pipeline_output(gcx, value) {
                write_pipeline_output(
                    gcx,
                    source.name.display(),
                    pass::pipeline_label(value),
                    module.to_text(),
                )?;
            }
        }
    } else {
        debug_assert!(gcx.sess.opts.language.is_evm_ir());
        let mut module = ir::Module::parse(gcx.sess, source)?;
        ir::validate(&gcx.sess.dcx, &module);
        if gcx.dcx().has_errors().is_ok() {
            if has_disasm_dump(gcx) {
                dump_evm_ir_input_disassembly(gcx, module)?;
                return Ok(());
            }

            let name = source.name.display().to_string();
            let _changed = ir::run_pipeline(gcx, &mut module, Some(&name));
            ir::validate(&gcx.sess.dcx, &module);
            gcx.dcx().has_errors()?;

            let value = gcx
                .sess
                .opts
                .unstable
                .evm_ir_pipeline
                .as_deref()
                .expect("EVM IR pipeline should be configured");
            if should_print_pipeline_output(gcx, value) {
                write_pipeline_output(
                    gcx,
                    source.name.display(),
                    ir::pipeline_label(value),
                    module.to_text(),
                )?;
            }
        }
    }
    Ok(())
}

fn dump_evm_ir_input_disassembly(gcx: Gcx<'_>, module: ir::Module) -> Result {
    let dump = gcx.sess.opts.unstable.dump.as_ref().expect("dump options should be present");
    let name = module.name();
    let bytecode = evm::generate_evm_ir_bytecode(gcx, module)?;
    let mut writer = console_writer(gcx.sess.opts.color);
    if dump.kinds.contains(&DumpKind::DisasmDeploy) {
        writeln!(writer, "// === {name} (deployment) ===")
            .and_then(|()| {
                write_highlighted(
                    &mut writer,
                    evm::disassemble(&bytecode, gcx.sess.opts.evm_version),
                    Syntax::Disasm,
                )
            })
            .map_err(|e| gcx.dcx().err(format!("failed to write to output: {e}")).emit())?;
    }
    if dump.kinds.contains(&DumpKind::DisasmRuntime) {
        writeln!(writer, "// === {name} (runtime) ===")
            .and_then(|()| {
                write_highlighted(
                    &mut writer,
                    evm::disassemble(&bytecode, gcx.sess.opts.evm_version),
                    Syntax::Disasm,
                )
            })
            .map_err(|e| gcx.dcx().err(format!("failed to write to output: {e}")).emit())?;
    }
    writer.flush().map_err(|e| gcx.dcx().err(format!("failed to write to output: {e}")).emit())
}

fn mir_pipeline_output_requested(gcx: Gcx<'_>) -> bool {
    !gcx.sess.opts.standard_json
        && gcx.sess.opts.unstable.mir_pipeline.is_some()
        && !gcx.sess.opts.emit.iter().any(|output| output.is_codegen())
        && !gcx.sess.opts.unstable.dump.as_ref().is_some_and(|dump| dump.needs_codegen())
}

fn emit_mir_pipeline_output(
    gcx: Gcx<'_>,
    artifacts: &FxHashMap<ContractId, ContractArtifact>,
) -> Result {
    let value =
        gcx.sess.opts.unstable.mir_pipeline.as_deref().expect("MIR pipeline should be configured");
    if !should_print_pipeline_output(gcx, value) {
        return Ok(());
    }

    let mut writer = console_writer(gcx.sess.opts.color);
    for id in ContractSelection::All.into_iter(gcx) {
        let module = artifacts
            .get(&id)
            .and_then(|artifact| artifact.mir.as_ref())
            .expect("requested MIR should be captured");
        writeln!(
            writer,
            "// === {} (after {}) ===",
            gcx.contract_fully_qualified_name(id),
            pass::pipeline_label(value)
        )
        .and_then(|()| write_highlighted(&mut writer, module.to_text().to_string(), Syntax::Ir))
        .map_err(|e| gcx.dcx().err(format!("failed to write to output: {e}")).emit())?;
    }
    writer.flush().map_err(|e| gcx.dcx().err(format!("failed to write to output: {e}")).emit())?;
    Ok(())
}

fn should_print_pipeline_output(gcx: Gcx<'_>, value: &str) -> bool {
    (!gcx.sess.opts.language.is_evm_ir() || !has_disasm_dump(gcx))
        && !gcx.sess.opts.unstable.print_after_each
        && (!gcx.sess.opts.unstable.pass_diff || value == "default")
}

fn write_pipeline_output(
    gcx: Gcx<'_>,
    name: impl std::fmt::Display,
    label: impl std::fmt::Display,
    text: impl std::fmt::Display,
) -> Result {
    let mut writer = console_writer(gcx.sess.opts.color);
    writeln!(writer, "// === {name} (after {label}) ===")
        .and_then(|()| write_highlighted(&mut writer, text.to_string(), Syntax::Ir))
        .and_then(|()| writer.flush())
        .map_err(|e| gcx.dcx().err(format!("failed to write to output: {e}")).emit())
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

        if let Some(artifact) = artifacts.and_then(|artifacts| artifacts.get(&id)) {
            if emit_bin {
                contract_output.bin = Some(MaybeHexBytecode::new(
                    artifact.deployment.clone(),
                    &artifact.deployment_link_references,
                ));
            }
            if emit_bin_runtime {
                contract_output.bin_runtime = Some(MaybeHexBytecode::new(
                    artifact.runtime.clone(),
                    &artifact.runtime_link_references,
                ));
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

fn has_disasm_dump(gcx: Gcx<'_>) -> bool {
    gcx.sess.opts.unstable.dump.as_ref().is_some_and(|dump| {
        dump.kinds
            .iter()
            .any(|kind| matches!(kind, DumpKind::DisasmDeploy | DumpKind::DisasmRuntime))
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
    let mut writer = console_writer(sess.opts.color);
    let mut first = true;
    for id in contracts.into_iter(gcx) {
        dump_mir_contract(&mut writer, gcx, dump, id, artifacts, &mut first)?;
    }
    writer.flush().map_err(|e| sess.dcx.err(format!("failed to write to output: {e}")).emit())?;

    Ok(())
}

fn dump_mir_contract(
    writer: &mut ConsoleWriter,
    gcx: Gcx<'_>,
    dump: &Dump,
    id: ContractId,
    artifacts: &FxHashMap<ContractId, ContractArtifact>,
    first: &mut bool,
) -> Result {
    let Some(module) = artifacts.get(&id).and_then(|artifact| artifact.mir.as_ref()) else {
        return Ok(());
    };
    if dump.kinds.contains(&DumpKind::Mir) {
        write_mir_dump_contract(writer, gcx, id, module, DumpKind::Mir, first)?;
    }
    if dump.kinds.contains(&DumpKind::MirCfg) {
        write_mir_dump_contract(writer, gcx, id, module, DumpKind::MirCfg, first)?;
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
    writer: &mut ConsoleWriter,
    gcx: Gcx<'_>,
    id: ContractId,
    module: &solar_codegen::mir::Module,
    kind: DumpKind,
    first: &mut bool,
) -> Result {
    if !std::mem::replace(first, false) {
        writeln!(writer)
            .map_err(|e| gcx.sess.dcx.err(format!("failed to write to output: {e}")).emit())?;
    }
    let name = gcx.contract_fully_qualified_name(id);
    writeln!(writer, "// === {name} ===")
        .map_err(|e| gcx.sess.dcx.err(format!("failed to write to output: {e}")).emit())?;
    match kind {
        DumpKind::Mir => write_highlighted(writer, module.to_string(), Syntax::Ir),
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
    let mut writer = console_writer(sess.opts.color);
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
    writer: &mut ConsoleWriter,
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
        write_highlighted(
            writer,
            format_deployment_evm_ir(
                artifact.deployment_evm_ir.as_ref(),
                artifact.runtime_evm_ir.as_ref(),
            ),
            Syntax::Ir,
        )
        .map_err(|e| gcx.sess.dcx.err(format!("failed to write to output: {e}")).emit())?;
    }
    if dump.kinds.contains(&DumpKind::EvmIrRuntime) {
        writeln!(writer, "// === {name} (runtime) ===")
            .map_err(|e| gcx.sess.dcx.err(format!("failed to write to output: {e}")).emit())?;
        if let Some(runtime_evm_ir) = &artifact.runtime_evm_ir {
            write_highlighted(writer, runtime_evm_ir.to_text().to_string(), Syntax::Ir)
                .map_err(|e| gcx.sess.dcx.err(format!("failed to write to output: {e}")).emit())?;
        }
    }
    Ok(())
}

fn dump_disassembly(
    gcx: Gcx<'_>,
    contracts: &ContractSelection,
    artifacts: &FxHashMap<ContractId, ContractArtifact>,
) -> Result {
    let sess = gcx.sess;
    let dump = sess.opts.unstable.dump.as_ref().expect("dump options should be present");
    let mut writer = console_writer(sess.opts.color);
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
        write_disassembly_dump_contract(&mut writer, gcx, dump, id, artifacts)?;
    }
    writer.flush().map_err(|e| sess.dcx.err(format!("failed to write to output: {e}")).emit())?;
    Ok(())
}

fn write_disassembly_dump_contract(
    writer: &mut ConsoleWriter,
    gcx: Gcx<'_>,
    dump: &Dump,
    id: ContractId,
    artifacts: &FxHashMap<ContractId, ContractArtifact>,
) -> Result {
    let Some(artifact) = artifacts.get(&id) else { return Ok(()) };
    let name = gcx.contract_fully_qualified_name(id);
    if dump.kinds.contains(&DumpKind::DisasmDeploy) {
        writeln!(writer, "// === {name} (deployment) ===")
            .map_err(|e| gcx.sess.dcx.err(format!("failed to write to output: {e}")).emit())?;
        let deployment_prefix = artifact
            .deployment
            .strip_suffix(artifact.runtime.as_ref())
            .expect("deployment bytecode should end with runtime bytecode");
        write_highlighted(
            writer,
            evm::disassemble(deployment_prefix, gcx.sess.opts.evm_version),
            Syntax::Disasm,
        )
        .map_err(|e| gcx.sess.dcx.err(format!("failed to write to output: {e}")).emit())?;
    }
    if dump.kinds.contains(&DumpKind::DisasmRuntime) {
        writeln!(writer, "// === {name} (runtime) ===")
            .map_err(|e| gcx.sess.dcx.err(format!("failed to write to output: {e}")).emit())?;
        write_highlighted(
            writer,
            evm::disassemble(&artifact.runtime, gcx.sess.opts.evm_version),
            Syntax::Disasm,
        )
        .map_err(|e| gcx.sess.dcx.err(format!("failed to write to output: {e}")).emit())?;
    }
    Ok(())
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

#[derive(Clone, Copy)]
enum Syntax {
    Ir,
    Disasm,
}

fn write_highlighted(writer: &mut ConsoleWriter, text: String, syntax: Syntax) -> io::Result<()> {
    if matches!(
        writer.get_ref().current_choice(),
        anstream::ColorChoice::Always | anstream::ColorChoice::AlwaysAnsi
    ) {
        for line in text.split_inclusive('\n') {
            let comment_start =
                if matches!(syntax, Syntax::Disasm) { line.find(';') } else { line.find("//") };
            let (code, comment) = comment_start.map_or((line, ""), |i| line.split_at(i));
            let (code, metadata) = code.find(" !").map_or((code, ""), |i| code.split_at(i + 1));
            let start = code.len() - code.trim_start().len();
            let trimmed = &code[start..];
            let token_start = if trimmed.starts_with('@')
                || trimmed.starts_with("fn ")
                || matches!(syntax, Syntax::Disasm)
                || trimmed.split_whitespace().next().is_some_and(|token| token.ends_with(':'))
            {
                start
            } else {
                code.find(" = ").map_or(start, |i| i + 3)
            };
            let token_end = code[token_start..]
                .find(char::is_whitespace)
                .map_or(code.len(), |i| token_start + i);
            let token = &code[token_start..token_end];
            let style = if token.ends_with(':') { LABEL_STYLE } else { OPCODE_STYLE };
            write_operands(writer, &code[..token_start])?;
            if token.starts_with(|c: char| c.is_ascii_alphanumeric() || matches!(c, '@' | '_')) {
                write!(writer, "{style}{token}{style:#}")?;
            } else {
                writer.write_all(token.as_bytes())?;
            }
            write_operands(writer, &code[token_end..])?;
            if !metadata.is_empty() {
                write!(writer, "{MUTED_STYLE}{metadata}{MUTED_STYLE:#}")?;
            }
            if !comment.is_empty() {
                write!(writer, "{MUTED_STYLE}{comment}{MUTED_STYLE:#}")?;
            }
        }
        Ok(())
    } else {
        writer.write_all(text.as_bytes())
    }
}

fn write_operands(writer: &mut ConsoleWriter, text: &str) -> io::Result<()> {
    for chunk in text.split_inclusive(|c| !is_operand_char(c)) {
        let token_end = chunk.find(|c| !is_operand_char(c)).unwrap_or(chunk.len());
        let (token, punctuation) = chunk.split_at(token_end);
        if !token.is_empty() {
            write_operand(writer, token)?;
        }
        writer.write_all(punctuation.as_bytes())?;
    }
    Ok(())
}

fn is_operand_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '@' | '_')
}

fn write_operand(writer: &mut ConsoleWriter, token: &str) -> io::Result<()> {
    let style = if is_indexed(token, "bb") {
        Some(LABEL_STYLE)
    } else if token.starts_with('@')
        || ["v", "arg", "fn"].into_iter().any(|prefix| is_indexed(token, prefix))
    {
        Some(VALUE_STYLE)
    } else if token == "true"
        || token == "false"
        || token == "undef"
        || token.chars().all(|c| c.is_ascii_digit())
        || token
            .strip_prefix("0x")
            .is_some_and(|value| value.chars().all(|c| c.is_ascii_hexdigit()))
    {
        Some(LITERAL_STYLE)
    } else {
        None
    };
    if let Some(style) = style {
        write!(writer, "{style}{token}{style:#}")
    } else {
        writer.write_all(token.as_bytes())
    }
}

fn is_indexed(token: &str, prefix: &str) -> bool {
    token
        .strip_prefix(prefix)
        .is_some_and(|index| !index.is_empty() && index.chars().all(|c| c.is_ascii_digit()))
}

type ConsoleWriter = io::BufWriter<anstream::AutoStream<io::Stdout>>;

const BLUE: Color =
    Color::Ansi(if cfg!(windows) { AnsiColor::BrightCyan } else { AnsiColor::BrightBlue });
const OPCODE_STYLE: Style = Style::new().fg_color(Some(BLUE)).bold();
const LABEL_STYLE: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::BrightCyan))).bold();
const MUTED_STYLE: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::BrightBlack)));
const VALUE_STYLE: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Magenta))).bold();
const LITERAL_STYLE: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::BrightGreen)));

fn console_writer(color: solar_config::ColorChoice) -> ConsoleWriter {
    io::BufWriter::new(anstream::AutoStream::new(io::stdout(), color))
}

fn out_writer(path: Option<&Path>) -> io::Result<impl io::Write> {
    let out: Box<dyn io::Write> = if let Some(path) = path {
        Box::new(std::fs::File::create(path)?)
    } else {
        Box::new(std::io::stdout())
    };
    Ok(io::BufWriter::new(out))
}

pub(crate) fn to_json<W: io::Write, T: serde::Serialize>(
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
