//! Lowers MIR CFGs and word operations to Sonatina IR, then links its EVM backend.
//!
//! MIR phis remain SSA phis. Comparisons use Sonatina's boolean results and widen
//! them to MIR words; branches convert words back to booleans. EVM-specific
//! arithmetic preserves zero-divisor and oversized-shift behavior. Compilation
//! currently requires Osaka. Native data symbols address embedded contracts;
//! immutable words follow runtime code and are patched during deployment.
//! Activation frames use the heap and tuple results use a reserved return area.
//! Native spill storage occupies a measured prefix below translated contract memory.
//! Recursive native frames still need a dynamic allocation protocol.

use super::evm::EvmArtifact;
use crate::mir::{
    Function, InstKind, Module, Terminator, Value, ValueId, analysis::CallGraphInfo,
    immutable::immutable_staging_base,
};
use solar_config::OptimizationMode;
use sonatina_codegen::{
    isa::evm::{EvmBackend, ImmediateMaterializationMode, LateCleanupProfile},
    machinst::lower::SectionWorkModule,
    stackalloc::StackifySearchProfile,
};
use sonatina_ir::{
    AccessLoc, Type,
    inst::{
        Inst,
        arith::Add,
        data::{SymAddr, SymbolRef},
    },
    ir_writer::ModuleWriter,
    isa::evm::space::MEMORY,
};
use std::fmt::Write;

pub(super) fn compile(
    module: &Module,
    optimization: OptimizationMode,
) -> Result<EvmArtifact, String> {
    let mut base = 0;
    for _ in 0..16 {
        let translated = super::memory::translate(module, base);
        let mut runtime_text = lower(&translated, None, base)?;
        let mut required = base;
        let mut runtime = assemble(&mut runtime_text, optimization, base, &mut required)?;
        if required > base {
            base = required;
            continue;
        }
        let immutable_references = super::alternative::append_immutable_data(module, &mut runtime);
        let mut text = lower(&translated, Some(&runtime), base)?;
        let deployment = assemble(&mut text, optimization, base, &mut required)?;
        if required > base {
            base = required;
            continue;
        }
        return Ok(EvmArtifact {
            deployment,
            runtime,
            immutable_references,
            backend_ir: Some(format!("// Runtime\n{runtime_text}\n// Deployment\n{text}")),
            ..Default::default()
        });
    }
    Err("Sonatina spill layout did not stabilize".into())
}

fn assemble(
    text: &mut String,
    optimization: OptimizationMode,
    base: u64,
    required: &mut u64,
) -> Result<Vec<u8>, String> {
    let parsed = sonatina_parser::parse_module(text)
        .map_err(|e| format!("invalid generated Sonatina IR: {e:?}"))?;
    let level = match optimization {
        OptimizationMode::None => sonatina_codegen::OptLevel::O0,
        OptimizationMode::Size => sonatina_codegen::OptLevel::Os,
        _ => sonatina_codegen::OptLevel::O2,
    };
    let mut compiler = sonatina_codegen::EvmCompile::new(parsed.module).with_opt_level(level);
    let module = compiler.optimize();
    if base != 0 {
        defer_memory_addresses(module);
        let mut bytes = Vec::new();
        ModuleWriter::new(module).write(&mut bytes).map_err(|e| e.to_string())?;
        *text = String::from_utf8(bytes).map_err(|e| e.to_string())?;
    }
    let backend = EvmBackend::new(sonatina_ir::isa::evm::Evm::new(module.ctx.triple))
        .with_late_cleanup_profile(match level {
            sonatina_codegen::OptLevel::O0 => LateCleanupProfile::Off,
            sonatina_codegen::OptLevel::Os => LateCleanupProfile::Size,
            _ => LateCleanupProfile::Speed,
        })
        .with_stackify_search_profile(if matches!(level, sonatina_codegen::OptLevel::O0) {
            StackifySearchProfile::Fast
        } else {
            StackifySearchProfile::Exact
        })
        .with_immediate_materialization_mode(match level {
            sonatina_codegen::OptLevel::O0 => ImmediateMaterializationMode::Gas,
            sonatina_codegen::OptLevel::Os => ImmediateMaterializationMode::Size,
            _ => ImmediateMaterializationMode::Balanced,
        });
    let entry = module
        .funcs()
        .into_iter()
        .find(|&f| module.ctx.func_sig(f, |s| s.name() == "entry"))
        .ok_or("missing Sonatina entry")?;
    let prepared =
        backend.prepare_section(SectionWorkModule::from_roots(module, entry, &[], &[]))?;
    // The pinned public snapshot exposes the exact static bound; dynamic
    // recursive frames have no finite prefix and need a separate heap protocol.
    let plan = backend.snapshot_mem_plan_detail(&prepared);
    let bound = plan
        .strip_prefix("evm mem plan: global_dyn_base=0x")
        .and_then(|s| s.split_whitespace().next())
        .and_then(|s| u64::from_str_radix(s, 16).ok())
        .ok_or("unrecognized Sonatina memory plan")?;
    let arena = plan
        .lines()
        .next()
        .and_then(|line| {
            line.split_whitespace().find_map(|word| word.strip_prefix("arena_base=0x"))
        })
        .and_then(|s| u64::from_str_radix(s, 16).ok())
        .ok_or("unrecognized Sonatina arena bound")?;
    let spills = plan
        .lines()
        .map(str::trim_start)
        .any(|line| line.starts_with("spill ") || line.starts_with("scratch_spill "));
    if spills || bound > arena {
        if plan.lines().any(|line| line.contains("stable_mode=DynamicFrame")) {
            return Err("Sonatina recursive spills require dynamic native frames".into());
        }
        *required = (*required).max(bound);
    }
    if *required > base {
        return Ok(Vec::new());
    }
    let artifacts = compiler.compile().map_err(|e| format!("Sonatina codegen failed: {e:?}"))?;
    let object = artifacts.first().ok_or("Sonatina produced no object")?;
    object
        .sections
        .iter()
        .find(|(key, _)| key.0.as_str() == "code")
        .map(|(_, section)| section.bytes.clone())
        .ok_or("missing Sonatina code section".into())
}

// Each independently linked code section starts at zero. Keeping fixed memory
// addresses section-relative until linking prevents the native reservation scan
// from moving its arena above contract memory. The measured arena must still fit
// entirely below the translated contract prefix before emission is allowed.
fn defer_memory_addresses(module: &sonatina_ir::Module) {
    for fid in module.funcs() {
        module.func_store.modify(fid, |f| {
            let blocks = f.layout.iter_block().collect::<Vec<_>>();
            for block in blocks {
                let instructions = f.layout.iter_inst(block).collect::<Vec<_>>();
                for id in instructions {
                    let mut addresses = f
                        .dfg
                        .effects(id)
                        .accesses
                        .iter()
                        .filter(|access| access.space == MEMORY)
                        .filter_map(|access| match access.loc {
                            AccessLoc::LinearExact { addr, .. }
                            | AccessLoc::LinearRange { addr, .. }
                                if f.dfg.value_is_imm(addr) =>
                            {
                                Some(addr)
                            }
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    addresses.sort_unstable();
                    addresses.dedup();
                    for address in addresses {
                        // zero = sym_addr .; relocated = add zero address; memory_op relocated
                        let symbol =
                            SymAddr::new_unchecked(f.inst_set(), SymbolRef::CurrentSection);
                        let zero = insert_native_value(f, id, symbol);
                        let sum = Add::new_unchecked(f.inst_set(), zero, address);
                        let relocated = insert_native_value(f, id, sum);
                        let mut inst = f.dfg.clone_inst(id);
                        inst.for_each_value_mut(&mut |value| {
                            if *value == address {
                                *value = relocated;
                            }
                        });
                        f.dfg.replace_inst(id, inst);
                    }
                }
            }
        });
    }
}

fn insert_native_value(
    f: &mut sonatina_ir::Function,
    before: sonatina_ir::InstId,
    inst: impl Inst,
) -> sonatina_ir::ValueId {
    // value = instruction
    let inst = f.dfg.make_inst(inst);
    let value = f.dfg.make_value(sonatina_ir::Value::Inst { inst, result_idx: 0, ty: Type::I256 });
    f.dfg.attach_results(inst, &[value]);
    f.layout.insert_inst_before(inst, before);
    value
}

fn lower(module: &Module, runtime: Option<&[u8]>, base: u64) -> Result<String, String> {
    let constructor = runtime.is_some();
    let preserve_expansion = super::memory::observes_size(module);
    let fmp = base + 64;
    let return_slot = base + 32;
    let (return_base, heap) = super::alternative::memory_layout(module);
    let root = if constructor {
        module
            .functions
            .iter_enumerated()
            .find(|(_, f)| f.attributes.is_constructor)
            .map(|(id, _)| id)
    } else {
        Some(module.dispatch_entry().ok_or("missing runtime entry")?)
    };
    // entry: initialize heap; call constructor/dispatch; deploy runtime or stop
    let mut out = format!(
        "target = \"evm-ethereum-osaka\"\nfunc public %entry() {{\nblock0:\nevm_mstore {fmp}.i256 {heap}.i256;\n"
    );
    if constructor {
        if let Some(id) = module.library_deploy_address() {
            // address = evm_address; evm_mstore staging[id] address
            writeln!(
                out,
                "v25.i256 = evm_address;\nevm_mstore {}.i256 v25;",
                base + immutable_staging_base(module) + id.index() as u64 * 32
            )
            .unwrap();
        }
        if let Some(id) = root {
            // size = codesize - sym_size .; codecopy heap end size; mstore 64 ceil32(heap + size)
            writeln!(out, "v20.i256 = sym_size .;\nv21.i256 = evm_code_size;\nv22.i256 = sub v21 v20;\nevm_code_copy {}.i256 v20 v22;\nv23.i256 = add v22 {}.i256;\nv24.i256 = and v23 -32.i256;\nevm_mstore {fmp}.i256 v24;", base + heap, heap + 31).unwrap();
            for i in 0..module.functions[id].params.len() {
                writeln!(out, "v{}.i256 = evm_mload {}.i256;", 30 + i, base + heap + i as u64 * 32)
                    .unwrap();
            }
            writeln!(
                out,
                "call %f{} {};",
                id.index(),
                (0..module.functions[id].params.len())
                    .map(|i| format!("v{}", 30 + i))
                    .collect::<Vec<_>>()
                    .join(" ")
            )
            .unwrap();
        } else if !module.is_library {
            // callvalue != 0 -> revert; otherwise continue deployment.
            out.push_str("v10.i256 = evm_call_value;\nv11.i1 = eq v10 0.i256;\nbr v11 block1 block2;\nblock2:\nevm_revert 0.i256 0.i256;\nblock1:\n");
        }
    } else if let Some(id) = root {
        writeln!(out, "call %f{};", id.index()).unwrap();
    }
    if let Some(runtime) = runtime {
        // deploy = evm_mload 64; codecopy deploy $runtime size; patch immutable words; return deploy size
        out.push_str("v0.i256 = sym_addr $runtime;\n");
        if base == 0 {
            out.push_str("v1.i256 = evm_mload 64.i256;\n");
        } else {
            // deploy = logical + base; overflow -> invalid; otherwise copy runtime
            writeln!(out, "v2.i256 = evm_mload {fmp}.i256;\nv1.i256 = add v2 {base}.i256;\nv3.i1 = lt v1 v2;\nbr v3 block3 block4;\nblock3:\nevm_invalid;\nblock4:").unwrap();
        }
        writeln!(out, "evm_code_copy v1 v0 {}.i256;", runtime.len()).unwrap();
        for (id, _) in module.iter_immutables() {
            let tmp = 30
                + module.functions.iter().map(|f| f.params.len()).max().unwrap_or(0)
                + id.index() * 2;
            writeln!(out, "v{tmp}.i256 = evm_mload {}.i256;\nv{}.i256 = add v1 {}.i256;\nevm_mstore v{} v{tmp};", base + immutable_staging_base(module) + id.index() as u64 * 32, tmp + 1, runtime.len() - super::alternative::runtime_tail_size(module) - (module.immutable_count() - id.index()) * 33 + 1, tmp + 1).unwrap();
        }
        writeln!(out, "evm_return v1 {}.i256;\n}}", runtime.len()).unwrap();
    } else {
        out.push_str("evm_stop;\n}\n");
    }
    let mut reachable = CallGraphInfo::new(module).reachable_callees_from(root);
    if let Some(id) = root {
        reachable.insert(id);
    }
    for fid in reachable.iter() {
        let f = &module.functions[fid];
        let frame =
            f.num_values() + f.arg_indices().count() + 6 * f.num_insts() + f.blocks.len() + 4;
        // func fN(a0.i256, ...) -> i256 { block0: ... }
        write!(
            out,
            "func private %f{}({})",
            fid.index(),
            (0..f.params.len())
                .map(|i| format!("v{}.i256", f.num_values() + i))
                .collect::<Vec<_>>()
                .join(", ")
        )
        .unwrap();
        if !f.returns.is_empty() {
            out.push_str(" -> i256");
        }
        out.push_str(" {\n");
        for (bid, block) in f.blocks.iter_enumerated() {
            writeln!(out, "block{}:", bid.index()).unwrap();
            if bid.index() == 0 && super::alternative::frame_size(f) != 0 {
                // frame = evm_mload 64; end = add frame activation_size; evm_mstore 64 end
                writeln!(out, "v{frame}.i256 = evm_mload {fmp}.i256;\nv{}.i256 = add v{frame} {}.i256;\nevm_mstore {fmp}.i256 v{};", frame + 1, super::alternative::frame_size(f), frame + 1).unwrap();
            }
            if bid.index() == 0 && f.params.is_empty() {
                // vN = evm_calldata_load (4 + 32 * argument_index)
                for arg in f.arg_indices() {
                    if f.selector.is_none() {
                        return Err("unsupported lazy Sonatina argument".into());
                    }
                    writeln!(
                        out,
                        "v{}.i256 = evm_calldata_load {}.i256;",
                        f.num_values() + arg.index(),
                        4 + arg.index() * 32
                    )
                    .unwrap();
                }
            }
            for &iid in &block.instructions {
                let inst = f.inst(iid);
                let result = f.inst_result_value(iid).map(|v| format!("v{}", v.index()));
                let args =
                    inst.operands().iter().map(|&v| value(f, v)).collect::<Result<Vec<_>, _>>()?;
                if preserve_expansion
                    && matches!(inst.kind, InstKind::MLoad(..) | InstKind::Keccak256(..))
                {
                    // mcopy address address length: preserve expansion if the read becomes dead.
                    let length =
                        if matches!(inst.kind, InstKind::MLoad(..)) { "32.i256" } else { &args[1] };
                    writeln!(out, "evm_mcopy {} {} {length};", args[0], args[0]).unwrap();
                }
                if let InstKind::Phi(inputs) = &inst.kind {
                    writeln!(
                        out,
                        "{}.i256 = phi {};",
                        result.as_ref().ok_or("phi without result")?,
                        inputs
                            .iter()
                            .map(|&(b, v)| Ok(format!("({} block{})", value(f, v)?, b.index())))
                            .collect::<Result<Vec<_>, String>>()?
                            .join(" ")
                    )
                    .unwrap();
                    continue;
                }
                if let InstKind::LoadImmutable(id) = inst.kind {
                    let result = result.ok_or("immutable without result")?;
                    if constructor {
                        // result = evm_mload staging[id]
                        writeln!(
                            out,
                            "{result}.i256 = evm_mload {}.i256;",
                            base + immutable_staging_base(module) + id.index() as u64 * 32
                        )
                        .unwrap();
                    } else {
                        // saved = mload 0; codecopy 0 (codesize - tail_offset) 32; result = mload 0; mstore 0 saved
                        let tmp = f.num_values()
                            + f.arg_indices().count()
                            + f.num_insts()
                            + f.blocks.len()
                            + iid.index() * 5;
                        writeln!(out, "v{tmp}.i256 = evm_mload {base}.i256;\nv{}.i256 = evm_code_size;\nv{}.i256 = sub v{} {}.i256;\nevm_code_copy {base}.i256 v{} 32.i256;\n{result}.i256 = evm_mload {base}.i256;\nevm_mstore {base}.i256 v{tmp};", tmp+1, tmp+2, tmp+1, (module.immutable_count()-id.index())*33-1 + super::alternative::runtime_tail_size(module), tmp+2).unwrap();
                    }
                    continue;
                }
                if let InstKind::InternalFrameAddr(offset) = inst.kind {
                    writeln!(
                        out,
                        "{}.i256 = add v{frame} {offset}.i256;",
                        result.ok_or("frame without result")?
                    )
                    .unwrap();
                    continue;
                }
                if matches!(inst.kind, InstKind::Select(..)) {
                    // zero = eq condition 0; flag = zext zero; mask = flag - 1; delta = yes xor no; result = no xor (delta and mask)
                    let tmp = f.num_values()
                        + f.arg_indices().count()
                        + f.num_insts()
                        + f.blocks.len()
                        + iid.index() * 5;
                    writeln!(out, "v{tmp}.i1 = eq {} 0.i256;\nv{}.i256 = zext v{tmp} i256;\nv{}.i256 = sub v{} 1.i256;\nv{}.i256 = xor {} {};\nv{}.i256 = and v{} v{};\n{}.i256 = xor {} v{};", args[0], tmp+1, tmp+2, tmp+1, tmp+3, args[1], args[2], tmp+4, tmp+3, tmp+2, result.ok_or("select without result")?, args[2], tmp+4).unwrap();
                    continue;
                }
                if matches!(inst.kind, InstKind::ConstructorArgsBase | InstKind::ConstructorArgsEnd)
                {
                    // base = 128; end = 128 + codesize - sym_size .
                    let result = result.ok_or("constructor boundary without result")?;
                    if matches!(inst.kind, InstKind::ConstructorArgsBase) {
                        writeln!(out, "{result}.i256 = add {heap}.i256 0.i256;").unwrap();
                    } else {
                        let tmp = f.num_values()
                            + f.arg_indices().count()
                            + f.num_insts()
                            + f.blocks.len()
                            + iid.index() * 5;
                        writeln!(out, "v{tmp}.i256 = sym_size .;\nv{}.i256 = evm_code_size;\nv{}.i256 = sub v{} v{tmp};\n{result}.i256 = add {heap}.i256 v{};", tmp+1, tmp+2, tmp+1, tmp+2).unwrap();
                    }
                    continue;
                }
                if let InstKind::DataCopy(data, ..) = &inst.kind {
                    // address = sym_addr $dN; offset = add address data.offset; codecopy dest offset size
                    let tmp = f.num_values()
                        + f.arg_indices().count()
                        + f.num_insts()
                        + f.blocks.len()
                        + iid.index() * 5;
                    writeln!(out, "v{tmp}.i256 = sym_addr $d{};\nv{}.i256 = add v{tmp} {}.i256;\nevm_code_copy {} v{} {};", data.id.index(), tmp + 1, data.offset, args[0], tmp + 1, args[1]).unwrap();
                    continue;
                }
                let (name, args) = operation(&inst.kind, args, base)?;
                if matches!(
                    inst.kind,
                    InstKind::Lt(..)
                        | InstKind::Gt(..)
                        | InstKind::SLt(..)
                        | InstKind::SGt(..)
                        | InstKind::Eq(..)
                        | InstKind::IsZero(..)
                ) {
                    // c.i1 = compare lhs rhs; v.i256 = zext c i256
                    let result = result.ok_or("comparison without result")?;
                    let predicate = f.num_values() + f.arg_indices().count() + iid.index();
                    writeln!(
                        out,
                        "v{predicate}.i1 = {name} {};\n{result}.i256 = zext v{predicate} i256;",
                        args.join(" ")
                    )
                    .unwrap();
                } else {
                    if let Some(result) = result {
                        write!(out, "{result}.i256 = ").unwrap();
                    } else if let InstKind::ICall { function, .. } = inst.kind
                        && !module.functions[function].returns.is_empty()
                    {
                        // discarded.i256 = call callee(args)
                        let discarded = f.num_values()
                            + f.arg_indices().count()
                            + f.num_insts()
                            + f.blocks.len()
                            + iid.index() * 5;
                        write!(out, "v{discarded}.i256 = ").unwrap();
                    }
                    writeln!(out, "{name} {};", args.join(" ")).unwrap();
                }
            }
            // Branches preserve MIR CFG edges; external exits remain EVM terminators.
            match block.terminator.as_ref().ok_or("missing terminator")? {
                Terminator::Jump(b) => writeln!(out, "jump block{};", b.index()).unwrap(),
                Terminator::Branch { condition, then_block, else_block } => writeln!(
                    out,
                    "v{}.i1 = ne {} 0.i256;\nbr v{} block{} block{};",
                    f.num_values() + f.arg_indices().count() + f.num_insts() + bid.index(),
                    value(f, *condition)?,
                    f.num_values() + f.arg_indices().count() + f.num_insts() + bid.index(),
                    then_block.index(),
                    else_block.index()
                )
                .unwrap(),
                Terminator::Switch { value: v, default, cases } => writeln!(
                    out,
                    "br_table {} block{} {};",
                    value(f, *v)?,
                    default.index(),
                    cases
                        .iter()
                        .map(|&(v, b)| Ok(format!("({} block{})", value(f, v)?, b.index())))
                        .collect::<Result<Vec<_>, String>>()?
                        .join(" ")
                )
                .unwrap(),
                Terminator::Return { values } => {
                    if values.len() > 1 {
                        // mstore(return_base + 32 * i, result_i); mstore(32, return_base)
                        for (i, &v) in values.iter().enumerate() {
                            writeln!(
                                out,
                                "evm_mstore {}.i256 {};",
                                base + return_base + i as u64 * 32,
                                value(f, v)?
                            )
                            .unwrap();
                        }
                        writeln!(out, "evm_mstore {return_slot}.i256 {return_base}.i256;").unwrap();
                    }
                    writeln!(
                        out,
                        "return {};",
                        values
                            .iter()
                            .take(1)
                            .map(|&v| value(f, v))
                            .collect::<Result<Vec<_>, _>>()?
                            .join(" ")
                    )
                    .unwrap();
                }
                Terminator::TailCall { function, args } => {
                    // discarded = call callee(args); evm_stop
                    if !module.functions[*function].returns.is_empty() {
                        writeln!(out, "v{}.i256 =", frame + 2 + bid.index()).unwrap();
                    }
                    writeln!(
                        out,
                        "call %f{} {};\nevm_stop;",
                        function.index(),
                        args.iter().map(|&v| value(f, v)).collect::<Result<Vec<_>, _>>()?.join(" ")
                    )
                    .unwrap();
                }
                Terminator::ReturnData { offset, size } => {
                    writeln!(out, "evm_return {} {};", value(f, *offset)?, value(f, *size)?)
                        .unwrap()
                }
                Terminator::Revert { offset, size } => {
                    writeln!(out, "evm_revert {} {};", value(f, *offset)?, value(f, *size)?)
                        .unwrap()
                }
                Terminator::Stop => out.push_str("return;\n"),
                Terminator::Invalid => out.push_str("evm_invalid;\n"),
                Terminator::SelfDestruct { recipient } => {
                    writeln!(out, "evm_self_destruct {};", value(f, *recipient)?).unwrap()
                }
                Terminator::RevertReturndata => {
                    // size = returndatasize; returndatacopy 0 0 size; revert 0 size
                    let tmp =
                        f.num_values() + f.arg_indices().count() + f.num_insts() + bid.index();
                    writeln!(out, "v{tmp}.i256 = evm_return_data_size;\nevm_return_data_copy {base}.i256 0.i256 v{tmp};\nevm_revert {base}.i256 v{tmp};").unwrap();
                }
            }
        }
        out.push_str("}\n");
    }
    let mut data_sections = String::new();
    let mut globals = String::new();
    // global const [i8; N] $dN = [bytes]; section { data $dN; }
    for (id, bytes) in module.iter_data() {
        writeln!(
            globals,
            "global private const [i8; {}] $d{} = [{}];",
            bytes.len(),
            id.index(),
            bytes.iter().map(|b| (*b as i8).to_string()).collect::<Vec<_>>().join(", ")
        )
        .unwrap();
        writeln!(data_sections, "data $d{};", id.index()).unwrap();
    }
    if let Some(runtime) = runtime {
        // global const $runtime = [compiled runtime bytes]
        writeln!(
            globals,
            "global private const [i8; {}] $runtime = [{}];",
            runtime.len(),
            runtime.iter().map(|b| (*b as i8).to_string()).collect::<Vec<_>>().join(", ")
        )
        .unwrap();
        data_sections.push_str("data $runtime;\n");
    }
    out.insert_str(out.find('\n').unwrap() + 1, &globals);
    writeln!(out, "object @Contract {{ section code {{ entry %entry; {data_sections} }} }}")
        .unwrap();
    Ok(out)
}

fn value(f: &Function, id: ValueId) -> Result<String, String> {
    Ok(match f.value(id) {
        Value::Immediate(imm) => format!("{}.i256", imm.as_u256().ok_or("non-word immediate")?),
        Value::Arg(arg) => format!("v{}", f.num_values() + arg.index()),
        Value::Inst(inst) => {
            format!("v{}", f.inst_result_value(*inst).ok_or("instruction without result")?.index())
        }
        _ => return Err("undefined value in Sonatina lowering".into()),
    })
}

fn operation(
    inst: &InstKind,
    mut args: Vec<String>,
    base: u64,
) -> Result<(String, Vec<String>), String> {
    let name = match inst {
        InstKind::Add(..)
        | InstKind::Sub(..)
        | InstKind::Mul(..)
        | InstKind::And(..)
        | InstKind::Or(..)
        | InstKind::Xor(..)
        | InstKind::Not(..)
        | InstKind::Shl(..)
        | InstKind::Shr(..)
        | InstKind::Sar(..)
        | InstKind::Lt(..)
        | InstKind::Gt(..)
        | InstKind::Eq(..) => inst.mnemonic(),
        InstKind::SLt(..) => "slt",
        InstKind::SGt(..) => "sgt",
        InstKind::IsZero(..) => {
            args.push("0.i256".into());
            "eq"
        }
        InstKind::Div(..) => "evm_udiv",
        InstKind::SDiv(..) => "evm_sdiv",
        InstKind::Mod(..) => "evm_umod",
        InstKind::SMod(..) => "evm_smod",
        InstKind::Exp(..) => "evm_exp",
        InstKind::AddMod(..) => "evm_add_mod",
        InstKind::MulMod(..) => "evm_mul_mod",
        InstKind::Byte(..) => "evm_byte",
        InstKind::SignExtend(..) => "evm_signextend",
        InstKind::Clz(..) => "evm_clz",
        InstKind::MLoad(..) => "evm_mload",
        InstKind::MStore(..) => "evm_mstore",
        InstKind::MStore8(..) => "evm_mstore8",
        InstKind::Fmp => {
            args.push(format!("{}.i256", base + 64));
            "evm_mload"
        }
        InstKind::SetFmp(..) => {
            args.insert(0, format!("{}.i256", base + 64));
            "evm_mstore"
        }
        InstKind::SLoad(..) => "evm_sload",
        InstKind::SStore(..) => "evm_sstore",
        InstKind::TLoad(..) => "evm_tload",
        InstKind::TStore(..) => "evm_tstore",
        InstKind::CalldataLoad(..) => "evm_calldata_load",
        InstKind::CalldataSize => "evm_calldata_size",
        InstKind::CalldataCopy(..) => "evm_calldata_copy",
        InstKind::Caller => "evm_caller",
        InstKind::CallValue => "evm_call_value",
        InstKind::Address => "evm_address",
        InstKind::Keccak256(..) => "evm_keccak256",
        InstKind::CodeSize => "evm_code_size",
        InstKind::CodeCopy(..) => "evm_code_copy",
        InstKind::ExtCodeSize(..) => "evm_ext_code_size",
        InstKind::ExtCodeCopy(..) => "evm_ext_code_copy",
        InstKind::ExtCodeHash(..) => "evm_ext_code_hash",
        InstKind::ReturnDataSize => "evm_return_data_size",
        InstKind::ReturnDataCopy(..) => "evm_return_data_copy",
        InstKind::Origin => "evm_origin",
        InstKind::GasPrice => "evm_gas_price",
        InstKind::BlockHash(..) => "evm_block_hash",
        InstKind::Coinbase => "evm_coin_base",
        InstKind::Timestamp => "evm_timestamp",
        InstKind::BlockNumber => "evm_number",
        InstKind::PrevRandao => "evm_prev_randao",
        InstKind::GasLimit => "evm_gas_limit",
        InstKind::ChainId => "evm_chain_id",
        InstKind::Balance(..) => "evm_balance",
        InstKind::SelfBalance => "evm_self_balance",
        InstKind::Gas => "evm_gas",
        InstKind::BaseFee => "evm_base_fee",
        InstKind::BlobBaseFee => "evm_blob_base_fee",
        InstKind::BlobHash(..) => "evm_blob_hash",
        InstKind::MSize => "evm_msize",
        InstKind::MCopy(..) => "evm_mcopy",
        InstKind::Create(..) => "evm_create",
        InstKind::Create2(..) => "evm_create2",
        InstKind::Call { .. } => "evm_call",
        InstKind::CallCode { .. } => "evm_call_code",
        InstKind::DelegateCall { .. } => "evm_delegate_call",
        InstKind::StaticCall { .. } => "evm_static_call",
        InstKind::Log0(..) => "evm_log0",
        InstKind::Log1(..) => "evm_log1",
        InstKind::Log2(..) => "evm_log2",
        InstKind::Log3(..) => "evm_log3",
        InstKind::Log4(..) => "evm_log4",
        InstKind::ICall { function, .. } => {
            args.insert(0, format!("%f{}", function.index()));
            "call"
        }
        _ => return Err(format!("unsupported Sonatina instruction `{}`", inst.mnemonic())),
    };
    Ok((name.into(), args))
}
