//! Lowers MIR to Sensei IR and statically links Plank's release backend.
//!
//! MIR values become function-scoped SIR locals. Dedicated edge blocks perform
//! parallel phi copies; the SIR SSA transform computes values carried between
//! blocks and splits critical edges before stack scheduling. Runtime code is
//! compiled first, then embedded as a data segment in the constructor program.
//! Activation frames use the heap and tuple results use a reserved return area.
//! Immutable words follow native runtime code and are patched during deployment.
//! Native spills occupy a measured prefix below translated contract memory.
//! The upstream compiler still rejects recursion.

use super::evm::EvmArtifact;
use crate::mir::{
    BlockId, Function, InstKind, Module, Terminator, Value, ValueId, analysis::CallGraphInfo,
    immutable::immutable_staging_base,
};
use alloy_primitives::hex;
use sir_stack_scheduling::ScheduleConfig;
use sir_static_memory_allocator::BumpAllocateAll;
use solar_config::OptimizationMode;
use std::fmt::Write;

pub(super) fn compile(
    module: &Module,
    optimization: OptimizationMode,
) -> Result<EvmArtifact, String> {
    let mut base = 0;
    for _ in 0..16 {
        let translated = super::memory::translate(module, base);
        let mut required = base;
        let artifact = compile_with_memory(&translated, optimization, base, &mut required)?;
        if required <= base {
            return Ok(artifact);
        }
        base = required;
    }
    Err("SIR spill layout did not stabilize".into())
}

fn compile_with_memory(
    module: &Module,
    optimization: OptimizationMode,
    base: u64,
    required: &mut u64,
) -> Result<EvmArtifact, String> {
    let fmp = base + 64;
    let (return_base, heap) = super::alternative::memory_layout(module);
    let entry = module.dispatch_entry().ok_or("missing runtime entry")?;
    let mut functions = functions(module, false, return_base, heap, base)?;
    // runtime: mstore(64, heap); call dispatch; stop
    let runtime_text = format!(
        "fn init:\nentry {{\nmstore256 {fmp} {heap}\nicall @f{}\nstop\n}}\n{functions}",
        entry.index()
    );
    let mut runtime = assemble(&runtime_text, optimization, base, required)?;
    if *required > base {
        return Ok(EvmArtifact::default());
    }
    let immutable_references = super::alternative::append_immutable_data(module, &mut runtime);
    functions = self::functions(module, true, return_base, heap, base)?;
    // init: constructor(); codecopy(deploy, runtime_data, runtime_length); return(deploy, runtime_length)
    let mut init = format!("fn init:\nentry {{\nmstore256 {fmp} {heap}\n");
    if let Some(id) = module.library_deploy_address() {
        // address = address; mstore staging[id] address
        writeln!(
            init,
            "address = address\nmstore256 {} address",
            base + immutable_staging_base(module) + id.index() as u64 * 32
        )
        .unwrap();
    }
    if let Some((id, _)) =
        module.functions.iter_enumerated().find(|(_, f)| f.attributes.is_constructor)
    {
        // end = init_end_offset; size = codesize - end; codecopy ARGS_PHYSICAL end size; mstore 64 ceil32(ARGS_BASE + size)
        init.push_str("end = init_end_offset\ntotal = codesize\nsize = sub total end\ncodecopy ARGS_PHYSICAL end size\nrounded = add size ARGS_ROUND\nfree = and rounded 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0\nmstore256 FMP_SLOT free\n");
        for i in 0..module.functions[id].params.len() {
            writeln!(init, "arg{i} = mload256 {}", base + heap + i as u64 * 32).unwrap();
        }
        writeln!(
            init,
            "icall @f{} {}",
            id.index(),
            (0..module.functions[id].params.len())
                .map(|i| format!("arg{i}"))
                .collect::<Vec<_>>()
                .join(" ")
        )
        .unwrap();
    } else if !module.is_library {
        // callvalue != 0 -> revert; otherwise continue deployment.
        init.push_str("value = callvalue\n=> value ? @reject : @deploy\n}\nreject {\nrevert 0 0\n}\ndeploy {\n");
    }
    // deploy = mload256 64; codecopy deploy runtime size; patch each immutable word
    if base == 0 {
        init.push_str("deploy = mload256 64\n");
    } else {
        // deploy = logical + base; overflow -> invalid; otherwise copy runtime
        writeln!(init, "logical_deploy = mload256 {fmp}\ndeploy = add logical_deploy {base}\noverflow = lt deploy logical_deploy\n=> overflow ? @overflow : @copy_runtime\n}}\noverflow {{\ninvalid\n}}\ncopy_runtime {{").unwrap();
    }
    writeln!(init, "offset = data_offset .runtime\ncodecopy deploy offset {}", runtime.len())
        .unwrap();
    for reference in &immutable_references {
        let id = reference.id.index();
        writeln!(
            init,
            "imm{id} = mload256 {}\npatch{id} = add deploy {}\nmstore256 patch{id} imm{id}",
            base + immutable_staging_base(module) + id as u64 * 32,
            reference.code_offset + 1
        )
        .unwrap();
    }
    writeln!(
        init,
        "return deploy {}\n}}\n{functions}data runtime 0x{}",
        runtime.len(),
        hex::encode(&runtime)
    )
    .unwrap();
    let init = init
        .replace("ARGS_BASE", &heap.to_string())
        .replace("ARGS_PHYSICAL", &(base + heap).to_string())
        .replace("FMP_SLOT", &fmp.to_string())
        .replace("ARGS_ROUND", &(heap + 31).to_string());
    Ok(EvmArtifact {
        deployment: assemble(&init, optimization, base, required)?,
        runtime,
        immutable_references,
        backend_ir: Some(format!("// Runtime\n{runtime_text}\n// Deployment\n{init}")),
        ..Default::default()
    })
}

fn functions(
    module: &Module,
    constructor: bool,
    return_base: u64,
    heap: u64,
    base: u64,
) -> Result<String, String> {
    let root = if constructor {
        module
            .functions
            .iter_enumerated()
            .find(|(_, f)| f.attributes.is_constructor)
            .map(|(id, _)| id)
    } else {
        module.dispatch_entry()
    };
    let mut reachable = CallGraphInfo::new(module).reachable_callees_from(root);
    if let Some(id) = root {
        reachable.insert(id);
    }
    let mut text = String::new();
    for id in reachable.iter() {
        function(
            module,
            &module.functions[id],
            id.index(),
            constructor,
            return_base,
            base,
            &mut text,
        )?;
    }
    // data dN 0xbytes
    for (id, bytes) in module.iter_data() {
        writeln!(text, "data d{} 0x{}", id.index(), hex::encode(bytes)).unwrap();
    }
    Ok(text.replace("ARGS_BASE", &heap.to_string()))
}

fn assemble(
    text: &str,
    optimization: OptimizationMode,
    base: u64,
    required: &mut u64,
) -> Result<Vec<u8>, String> {
    let arena = bumpalo::Bump::new();
    let ast =
        sir_parser::parse(text, &arena).map_err(|e| format!("invalid generated SIR: {e:?}"))?;
    let mut config = sir_parser::EmitConfig::init_only();
    config.allow_duplicate_locals = true;
    let mut program =
        sir_parser::emit_ir(&arena, &ast, config).map_err(|e| e.reason.to_string())?;
    let mut passes = sir_passes::PassManager::new(&mut program);
    passes.run_ssa_transform();
    if !matches!(optimization, OptimizationMode::None) {
        passes.run_optimizations(sir_passes::OptimizationLevel::O2.passes().expect("O2 pipeline"));
    }
    passes.run_legalize().map_err(|e| format!("illegal generated SIR: {e}"))?;
    let analyses = passes.into_store();
    if program.next_static_alloc_id.const_get() != 0 {
        return Err("unexpected native allocation in SIR".into());
    }
    // All source allocations have become raw memory operations. Native static
    // allocations are word-sized spills; the layout also accounts for any
    // switch scratch introduced by native legalization.
    let (ops, _, last_alloc) =
        sir_stack_scheduling::schedule(&program, &analyses, ScheduleConfig::PRE_AMSTERDAM);
    let layout = BumpAllocateAll::generate(
        &program,
        program.init_entry,
        &ops,
        last_alloc.const_get() as usize,
    );
    if layout.dyn_free_pointer.is_some() {
        return Err("unexpected native dynamic allocation in SIR".into());
    }
    let end = layout
        .alloc_start
        .values()
        .map(|addr| u64::from(addr.get()) + 32)
        .chain(layout.switch_store.map(|addr| u64::from(addr.get()) + 32))
        .max()
        .unwrap_or(0);
    *required = (*required).max(end);
    if *required > base {
        return Ok(Vec::new());
    }
    let mut bytes = Vec::new();
    sir_release_backend::ir_to_bytecode(&program, &analyses, &mut bytes);
    Ok(bytes)
}

fn function(
    module: &Module,
    f: &Function,
    id: usize,
    constructor: bool,
    return_base: u64,
    base: u64,
    out: &mut String,
) -> Result<(), String> {
    let fmp = base + 64;
    let return_slot = base + 32;
    let mut lazy = String::new();
    if f.params.is_empty() {
        // aN = calldataload (4 + 32 * argument_index)
        for arg in f.arg_indices() {
            if f.selector.is_none() {
                return Err("unsupported lazy SIR argument".into());
            }
            writeln!(lazy, "a{} = calldataload {}", arg.index(), 4 + arg.index() * 32).unwrap();
        }
    }
    let frame_size = super::alternative::frame_size(f);
    if frame_size != 0 {
        // frame = mload256 {fmp}; free = add frame activation_size; mstore256 FMP_SLOT free
        writeln!(
            lazy,
            "frame = mload256 {fmp}\nframe_end = add frame {frame_size}\nmstore256 {fmp} frame_end"
        )
        .unwrap();
    }
    // fn fN: entry a0 ... { materialize lazy arguments; jump b0 }; bN { instructions; terminator }
    writeln!(
        out,
        "fn f{id}:\nentry {} {{\n{lazy}=> @b0\n}}",
        (0..f.params.len()).map(|i| format!("a{i}")).collect::<Vec<_>>().join(" ")
    )
    .unwrap();
    for (bid, block) in f.blocks.iter_enumerated() {
        write!(out, "b{}", bid.index()).unwrap();
        if let Some(Terminator::Return { values }) = &block.terminator
            && !values.is_empty()
        {
            write!(
                out,
                " -> {}",
                (0..values.len().min(1))
                    .map(|i| format!("r{}_{}", bid.index(), i))
                    .collect::<Vec<_>>()
                    .join(" ")
            )
            .unwrap();
        }
        out.push_str(" {\n");
        for &iid in &block.instructions {
            let inst = f.inst(iid);
            if matches!(inst.kind, InstKind::Phi(_)) {
                continue;
            }
            let mut args =
                inst.operands().iter().map(|&v| value(f, v)).collect::<Result<Vec<_>, _>>()?;
            if let InstKind::LoadImmutable(id) = inst.kind {
                let result = f.inst_result_value(iid).ok_or("immutable without result")?.index();
                if constructor {
                    // result = mload256 staging[id]
                    writeln!(
                        out,
                        "v{result} = mload256 {}",
                        base + immutable_staging_base(module) + id.index() as u64 * 32
                    )
                    .unwrap();
                } else {
                    // saved = mload256 0; codecopy 0 (codesize - tail_offset) 32; result = mload256 0; mstore256 0 saved
                    writeln!(out, "saved{result} = mload256 {base}\ntotal{result} = codesize\noffset{result} = sub total{result} {}\ncodecopy {base} offset{result} 32\nv{result} = mload256 {base}\nmstore256 {base} saved{result}", (module.immutable_count() - id.index()) * 33 - 1 + super::alternative::runtime_tail_size(module)).unwrap();
                }
                continue;
            }
            if matches!(inst.kind, InstKind::Select(..)) {
                // zero = iszero condition; mask = sub zero 1; delta = xor yes no; result = xor no (and delta mask)
                writeln!(out, "zero{} = iszero {}\nmask{} = sub zero{} 1\ndelta{} = xor {} {}\nmasked{} = and delta{} mask{}\nv{} = xor {} masked{}", iid.index(), args[0], iid.index(), iid.index(), iid.index(), args[1], args[2], iid.index(), iid.index(), iid.index(), f.inst_result_value(iid).ok_or("select without result")?.index(), args[2], iid.index()).unwrap();
                continue;
            }
            if matches!(inst.kind, InstKind::ConstructorArgsEnd) {
                // end = init_end_offset; size = codesize - end; result = 128 + size
                writeln!(out, "end{} = init_end_offset\ntotal{} = codesize\nsize{} = sub total{} end{}\nv{} = add ARGS_BASE size{}", iid.index(), iid.index(), iid.index(), iid.index(), iid.index(), f.inst_result_value(iid).ok_or("constructor boundary without result")?.index(), iid.index()).unwrap();
                continue;
            }
            if let InstKind::DataCopy(data, ..) = &inst.kind {
                // base = data_offset .dN; offset = add base data.offset; codecopy dest offset size
                writeln!(
                    out,
                    "data{} = data_offset .d{}\noffset{} = add data{} {}\ncodecopy {} offset{} {}",
                    iid.index(),
                    data.id.index(),
                    iid.index(),
                    iid.index(),
                    data.offset,
                    args[0],
                    iid.index(),
                    args[1]
                )
                .unwrap();
                continue;
            }
            let name = match &inst.kind {
                InstKind::Shl(..) | InstKind::Shr(..) | InstKind::Sar(..) => {
                    inst.kind.mnemonic().to_owned()
                }
                InstKind::InternalFrameAddr(offset) => {
                    args.extend(["frame".into(), offset.to_string()]);
                    "add".into()
                }
                InstKind::PrevRandao => "difficulty".into(),
                InstKind::Clz(..) => "clz".into(),
                InstKind::ConstructorArgsBase => {
                    args.push("ARGS_BASE".into());
                    "copy".into()
                }
                InstKind::MLoad(..) => "mload256".into(),
                InstKind::MStore(..) => "mstore256".into(),
                InstKind::Fmp => {
                    args.push(fmp.to_string());
                    "mload256".into()
                }
                InstKind::SetFmp(..) => {
                    args.insert(0, fmp.to_string());
                    "mstore256".into()
                }
                InstKind::ICall { function, .. } => {
                    args.insert(0, format!("@f{}", function.index()));
                    "icall".into()
                }
                InstKind::Add(..)
                | InstKind::Sub(..)
                | InstKind::Mul(..)
                | InstKind::Div(..)
                | InstKind::SDiv(..)
                | InstKind::Mod(..)
                | InstKind::SMod(..)
                | InstKind::And(..)
                | InstKind::Or(..)
                | InstKind::Xor(..)
                | InstKind::Not(..)
                | InstKind::Lt(..)
                | InstKind::Gt(..)
                | InstKind::SLt(..)
                | InstKind::SGt(..)
                | InstKind::Eq(..)
                | InstKind::IsZero(..)
                | InstKind::Exp(..)
                | InstKind::AddMod(..)
                | InstKind::MulMod(..)
                | InstKind::Byte(..)
                | InstKind::SignExtend(..)
                | InstKind::MStore8(..)
                | InstKind::MCopy(..)
                | InstKind::SLoad(..)
                | InstKind::SStore(..)
                | InstKind::TLoad(..)
                | InstKind::TStore(..)
                | InstKind::CalldataLoad(..)
                | InstKind::CalldataSize
                | InstKind::CalldataCopy(..)
                | InstKind::Caller
                | InstKind::CallValue
                | InstKind::Address
                | InstKind::CodeSize
                | InstKind::CodeCopy(..)
                | InstKind::ExtCodeSize(..)
                | InstKind::ExtCodeCopy(..)
                | InstKind::ExtCodeHash(..)
                | InstKind::ReturnDataSize
                | InstKind::ReturnDataCopy(..)
                | InstKind::Origin
                | InstKind::GasPrice
                | InstKind::BlockHash(..)
                | InstKind::Coinbase
                | InstKind::Timestamp
                | InstKind::BlockNumber
                | InstKind::GasLimit
                | InstKind::ChainId
                | InstKind::Balance(..)
                | InstKind::SelfBalance
                | InstKind::Gas
                | InstKind::BaseFee
                | InstKind::BlobBaseFee
                | InstKind::BlobHash(..)
                | InstKind::Create(..)
                | InstKind::Create2(..)
                | InstKind::Call { .. }
                | InstKind::CallCode { .. }
                | InstKind::DelegateCall { .. }
                | InstKind::StaticCall { .. }
                | InstKind::Log0(..)
                | InstKind::Log1(..)
                | InstKind::Log2(..)
                | InstKind::Log3(..)
                | InstKind::Log4(..)
                | InstKind::Keccak256(..) => inst.kind.mnemonic().to_owned(),
                other => return Err(format!("unsupported SIR instruction `{}`", other.mnemonic())),
            };
            if let Some(result) = f.inst_result_value(iid) {
                if let InstKind::ICall { function, .. } = inst.kind
                    && !has_return_value(&module.functions[function])
                {
                    // dead_result = 0; icall nonreturning_callee
                    // SIR infers zero returns from the callee's terminating blocks.
                    writeln!(out, "v{} = copy 0", result.index()).unwrap();
                } else {
                    write!(out, "v{} = ", result.index()).unwrap();
                }
            } else if let InstKind::ICall { function, .. } = inst.kind
                && has_return_value(&module.functions[function])
            {
                // discarded = icall callee(args)
                write!(out, "discarded{} = ", iid.index()).unwrap();
            }
            writeln!(out, "{name} {}", args.join(" ")).unwrap();
        }
        // Each outgoing edge gets its own block for parallel phi assignments.
        match block.terminator.as_ref().ok_or("missing terminator")? {
            Terminator::Jump(to) => writeln!(out, "=> @e{}_{}", bid.index(), to.index()).unwrap(),
            Terminator::Branch { condition, then_block, else_block } => writeln!(
                out,
                "=> {} ? @e{}_{} : @e{}_{}",
                value(f, *condition)?,
                bid.index(),
                then_block.index(),
                bid.index(),
                else_block.index()
            )
            .unwrap(),
            Terminator::Switch { value: v, default, cases } => {
                // c = eq value case; branch c -> edge, next_case; ...; jump default
                for (i, &(case, target)) in cases.iter().enumerate() {
                    writeln!(
                        out,
                        "c{}_{} = eq {} {}\n=> c{}_{} ? @e{}_{} : @s{}_{}\n}}\ns{}_{} {{",
                        bid.index(),
                        i,
                        value(f, *v)?,
                        value(f, case)?,
                        bid.index(),
                        i,
                        bid.index(),
                        target.index(),
                        bid.index(),
                        i,
                        bid.index(),
                        i
                    )
                    .unwrap();
                }
                writeln!(out, "=> @e{}_{}", bid.index(), default.index()).unwrap();
            }
            Terminator::Return { values } => {
                if values.len() > 1 {
                    // mstore256(return_base + 32 * i, result_i); mstore256(32, return_base)
                    for (i, &v) in values.iter().enumerate() {
                        writeln!(out, "mstore256 {} {}", base + return_base + i as u64 * 32, value(f, v)?)
                            .unwrap();
                    }
                    writeln!(out, "mstore256 {return_slot} {return_base}").unwrap();
                }
                for (i, &v) in values.iter().take(1).enumerate() {
                    writeln!(out, "r{}_{} = copy {}", bid.index(), i, value(f, v)?).unwrap();
                }
                out.push_str("iret\n");
            }
            Terminator::TailCall { function, args } => {
                // discarded = icall callee(args); stop
                if has_return_value(&module.functions[*function]) {
                    write!(out, "tail{} = ", bid.index()).unwrap();
                }
                writeln!(
                    out,
                    "icall @f{} {}\nstop",
                    function.index(),
                    args.iter().map(|&v| value(f, v)).collect::<Result<Vec<_>, _>>()?.join(" ")
                )
                .unwrap();
            }
            Terminator::ReturnData { offset, size } => {
                writeln!(out, "return {} {}", value(f, *offset)?, value(f, *size)?).unwrap()
            }
            Terminator::Revert { offset, size } => {
                writeln!(out, "revert {} {}", value(f, *offset)?, value(f, *size)?).unwrap()
            }
            Terminator::Stop => out.push_str("iret\n"),
            Terminator::Invalid => out.push_str("invalid\n"),
            Terminator::SelfDestruct { recipient } => {
                writeln!(out, "selfdestruct {}", value(f, *recipient)?).unwrap()
            }
            // size = returndatasize; returndatacopy 0 0 size; revert 0 size
            Terminator::RevertReturndata => writeln!(out,
                "returndata = returndatasize\nreturndatacopy {base} 0 returndata\nrevert {base} returndata"
            ).unwrap(),
        }
        out.push_str("}\n");
        let mut successors = Vec::new();
        block.terminator.as_ref().unwrap().for_each_successor(|b| {
            if !successors.contains(&b) {
                successors.push(b);
            }
        });
        for to in successors {
            edge(f, bid, to, out)?;
        }
    }
    Ok(())
}

fn edge(f: &Function, from: BlockId, to: BlockId, out: &mut String) -> Result<(), String> {
    // edge: t0 = copy incoming0; ...; phi0 = copy t0; ...; jump successor
    writeln!(out, "e{}_{} {{", from.index(), to.index()).unwrap();
    let mut copies = Vec::new();
    for &iid in &f.blocks[to].instructions {
        if let InstKind::Phi(inputs) = &f.inst(iid).kind {
            let input = inputs.iter().find(|(b, _)| *b == from).ok_or("missing phi input")?.1;
            let result = f.inst_result_value(iid).ok_or("phi without result")?;
            let tmp = format!("t{}_{}_{}", from.index(), to.index(), result.index());
            writeln!(out, "{tmp} = copy {}", value(f, input)?).unwrap();
            copies.push((result, tmp));
        }
    }
    for (result, tmp) in copies {
        writeln!(out, "v{} = copy {tmp}", result.index()).unwrap();
    }
    writeln!(out, "=> @b{}\n}}", to.index()).unwrap();
    Ok(())
}

fn has_return_value(f: &Function) -> bool {
    f.blocks.iter().any(|block| matches!(&block.terminator, Some(Terminator::Return { values }) if !values.is_empty()))
}

fn value(f: &Function, id: ValueId) -> Result<String, String> {
    Ok(match f.value(id) {
        Value::Immediate(imm) => format!("0x{:x}", imm.as_u256().ok_or("non-word immediate")?),
        Value::Arg(arg) => format!("a{}", arg.index()),
        Value::Inst(inst) => {
            format!("v{}", f.inst_result_value(*inst).ok_or("instruction without result")?.index())
        }
        _ => return Err("undefined value in SIR lowering".into()),
    })
}
