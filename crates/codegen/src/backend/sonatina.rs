//! Lowers MIR CFGs and word operations to Sonatina IR, then links its EVM backend.
//!
//! MIR phis remain SSA phis. Comparisons use Sonatina's boolean results and widen
//! them to MIR words; branches convert words back to booleans. EVM-specific
//! arithmetic preserves zero-divisor and oversized-shift behavior. Compilation
//! currently requires Osaka and rejects unsupported relocations and stack frames.

use super::evm::EvmArtifact;
use crate::mir::{Function, InstKind, Module, Terminator, Value, ValueId};
use solar_config::OptimizationMode;
use std::fmt::Write;

pub(super) fn compile(
    module: &Module,
    optimization: OptimizationMode,
) -> Result<EvmArtifact, String> {
    let text = lower(module)?;
    let parsed = sonatina_parser::parse_module(&text)
        .map_err(|e| format!("invalid generated Sonatina IR: {e:?}"))?;
    let level = match optimization {
        OptimizationMode::None => sonatina_codegen::OptLevel::O0,
        OptimizationMode::Size => sonatina_codegen::OptLevel::Os,
        _ => sonatina_codegen::OptLevel::O2,
    };
    let artifacts = sonatina_codegen::EvmCompile::new(parsed.module)
        .with_opt_level(level)
        .compile()
        .map_err(|e| format!("Sonatina codegen failed: {e:?}"))?;
    let object = artifacts.first().ok_or("Sonatina produced no object")?;
    let section = |name: &str| {
        object
            .sections
            .iter()
            .find(|(key, _)| key.0.as_str() == name)
            .map(|(_, s)| s.bytes.clone())
            .ok_or_else(|| format!("missing Sonatina {name} section"))
    };
    Ok(EvmArtifact {
        backend_ir: Some(text),
        deployment: section("init")?,
        runtime: section("runtime")?,
        ..Default::default()
    })
}

fn lower(module: &Module) -> Result<String, String> {
    if module.immutable_count() != 0 || module.data_count() != 0 || module.is_library {
        return Err(
            "Sonatina lowering does not yet support immutable, data, or library relocations".into(),
        );
    }
    let entry = module.dispatch_entry().ok_or("missing runtime entry")?;
    // init: constructor(); codecopy(0, runtime, sizeof(runtime)); return(0, sizeof(runtime))
    let mut out = String::from(
        "target = \"evm-ethereum-osaka\"\nfunc public %init() {\nblock0:\n evm_mstore 64.i256 128.i256;\n",
    );
    if let Some((id, _)) =
        module.functions.iter_enumerated().find(|(_, f)| f.attributes.is_constructor)
    {
        writeln!(out, "call %f{};", id.index()).unwrap();
    } else {
        // callvalue != 0 -> revert; otherwise continue deployment.
        out.push_str("v10.i256 = evm_call_value;\nv11.i1 = eq v10 0.i256;\nbr v11 block1 block2;\nblock2:\nevm_revert 0.i256 0.i256;\nblock1:\n");
    }
    out.push_str("v0.i256 = sym_addr &runtime;\nv1.i256 = sym_size &runtime;\nevm_code_copy 0.i256 v0 v1;\nevm_return 0.i256 v1;\n}\n");
    // runtime: initialize the Solidity heap pointer; call the MIR dispatch entry.
    writeln!(
        out,
        "func public %runtime() {{\nblock0:\nevm_mstore 64.i256 128.i256;\ncall %f{};\nevm_stop;\n}}",
        entry.index()
    )
    .unwrap();
    for (fid, f) in module.functions.iter_enumerated() {
        if f.returns.len() > 1 {
            return Err(format!(
                "unsupported Sonatina function frame or return tuple in `{}`",
                f.name
            ));
        }
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
                let (name, args) = operation(&inst.kind, args)?;
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
                Terminator::Return { values } => writeln!(
                    out,
                    "return {};",
                    values.iter().map(|&v| value(f, v)).collect::<Result<Vec<_>, _>>()?.join(" ")
                )
                .unwrap(),
                Terminator::TailCall { function, args } => writeln!(
                    out,
                    "call %f{} {};\nevm_stop;",
                    function.index(),
                    args.iter().map(|&v| value(f, v)).collect::<Result<Vec<_>, _>>()?.join(" ")
                )
                .unwrap(),
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
                other => return Err(format!("unsupported Sonatina terminator: {other:?}")),
            }
        }
        out.push_str("}\n");
    }
    out.push_str("object @Contract {\nsection init {\nentry %init;\nembed .runtime as &runtime;\n}\nsection runtime {\nentry %runtime;\n}\n}\n");
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

fn operation(inst: &InstKind, mut args: Vec<String>) -> Result<(String, Vec<String>), String> {
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
        InstKind::SignExtend(..) => "evm_sign_extend",
        InstKind::Clz(..) => "evm_clz",
        InstKind::MLoad(..) => "evm_mload",
        InstKind::MStore(..) => "evm_mstore",
        InstKind::MStore8(..) => "evm_mstore8",
        InstKind::Fmp => {
            args.push("64.i256".into());
            "evm_mload"
        }
        InstKind::SetFmp(..) => {
            args.insert(0, "64.i256".into());
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
        InstKind::ICall { function, returns, .. } if *returns <= 1 => {
            args.insert(0, format!("%f{}", function.index()));
            "call"
        }
        _ => return Err(format!("unsupported Sonatina instruction `{}`", inst.mnemonic())),
    };
    Ok((name.into(), args))
}
