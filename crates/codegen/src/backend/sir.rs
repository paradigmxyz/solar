//! Lowers MIR to Sensei IR and statically links Plank's release backend.
//!
//! MIR values become function-scoped SIR locals. Dedicated edge blocks perform
//! parallel phi copies; the SIR SSA transform computes values carried between
//! blocks and splits critical edges before stack scheduling. Runtime code is
//! compiled first, then embedded as a data segment in the constructor program.
//! Frames, recursive calls, and relocations require further lowering support.

use super::evm::EvmArtifact;
use crate::mir::{BlockId, Function, InstKind, Module, Terminator, Value, ValueId};
use alloy_primitives::hex;
use sir_stack_scheduling::{ScheduleConfig, stack::StackOps};
use solar_config::OptimizationMode;
use std::fmt::Write;

pub(super) fn compile(
    module: &Module,
    optimization: OptimizationMode,
) -> Result<EvmArtifact, String> {
    if module.immutable_count() != 0 || module.data_count() != 0 || module.is_library {
        return Err(
            "SIR lowering does not yet support immutable, data, or library relocations".into()
        );
    }
    let entry = module.dispatch_entry().ok_or("missing runtime entry")?;
    let mut functions = String::new();
    for (id, f) in module.functions.iter_enumerated() {
        function(f, id.index(), &mut functions)?;
    }
    // runtime: mstore(64, 128); call dispatch; stop
    let runtime = assemble(
        &format!(
            "fn init:\nentry {{\nmstore256 64 128\nicall @f{}\nstop\n}}\n{functions}",
            entry.index()
        ),
        optimization,
    )?;
    // init: constructor(); codecopy(0, runtime_data, runtime_length); return(0, runtime_length)
    let mut init = String::from("fn init:\nentry {\nmstore256 64 128\n");
    if let Some((id, _)) =
        module.functions.iter_enumerated().find(|(_, f)| f.attributes.is_constructor)
    {
        writeln!(init, "icall @f{}", id.index()).unwrap();
    } else {
        // callvalue != 0 -> revert; otherwise continue deployment.
        init.push_str("value = callvalue\n=> value ? @reject : @deploy\n}\nreject {\nrevert 0 0\n}\ndeploy {\n");
    }
    writeln!(init,"offset = data_offset .runtime\ncodecopy 0 offset {}\nreturn 0 {}\n}}\n{functions}\ndata runtime 0x{}",runtime.len(),runtime.len(),hex::encode(&runtime)).unwrap();
    Ok(EvmArtifact {
        deployment: assemble(&init, optimization)?,
        runtime,
        backend_ir: Some(init),
        ..Default::default()
    })
}

fn assemble(text: &str, optimization: OptimizationMode) -> Result<Vec<u8>, String> {
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
    // NOTE: The upstream allocator starts at address zero. Reject spills rather
    // than overwrite Solidity memory; switch lowering below also avoids scratch.
    let (ops, _, _) =
        sir_stack_scheduling::schedule(&program, &analyses, ScheduleConfig::PRE_AMSTERDAM);
    if ops
        .enumerate_idx()
        .any(|(_, ops)| ops.iter().any(|op| matches!(op, StackOps::Store(_) | StackOps::Load(_))))
    {
        return Err("SIR stack spills require a separate memory layout".into());
    }
    let mut bytes = Vec::new();
    sir_release_backend::ir_to_bytecode(&program, &analyses, &mut bytes);
    Ok(bytes)
}

fn function(f: &Function, id: usize, out: &mut String) -> Result<(), String> {
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
                (0..values.len())
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
            let name = match &inst.kind {
                InstKind::Shl(..) | InstKind::Shr(..) | InstKind::Sar(..) => {
                    inst.kind.mnemonic().to_owned()
                }
                InstKind::MLoad(..) => "mload256".into(),
                InstKind::MStore(..) => "mstore256".into(),
                InstKind::Fmp => {
                    args.push("64".into());
                    "mload256".into()
                }
                InstKind::SetFmp(..) => {
                    args.insert(0, "64".into());
                    "mstore256".into()
                }
                InstKind::ICall { function, returns, .. } if *returns <= 1 => {
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
                | InstKind::Keccak256(..) => inst.kind.mnemonic().to_owned(),
                other => return Err(format!("unsupported SIR instruction `{}`", other.mnemonic())),
            };
            if let Some(result) = f.inst_result_value(iid) {
                write!(out, "v{} = ", result.index()).unwrap();
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
                for (i, &v) in values.iter().enumerate() {
                    writeln!(out, "r{}_{} = copy {}", bid.index(), i, value(f, v)?).unwrap();
                }
                out.push_str("iret\n");
            }
            Terminator::TailCall { function, args } => writeln!(
                out,
                "icall @f{} {}\nstop",
                function.index(),
                args.iter().map(|&v| value(f, v)).collect::<Result<Vec<_>, _>>()?.join(" ")
            )
            .unwrap(),
            Terminator::ReturnData { offset, size } => {
                writeln!(out, "return {} {}", value(f, *offset)?, value(f, *size)?).unwrap()
            }
            Terminator::Revert { offset, size } => {
                writeln!(out, "revert {} {}", value(f, *offset)?, value(f, *size)?).unwrap()
            }
            Terminator::Stop => out.push_str("iret\n"),
            Terminator::Invalid => out.push_str("invalid\n"),
            other => return Err(format!("unsupported SIR terminator: {other:?}")),
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
