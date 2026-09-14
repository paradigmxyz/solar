//! Lowers word-based MIR to structured Yul.
//!
//! Each MIR function uses a block-number loop so arbitrary control flow stays
//! explicit. Edge-local parallel copies implement phis without clobbering loop
//! values. The Yul optimizer can simplify this structure before stack scheduling.
//! ABI, dispatch, and semantic memory lowering run before this backend. Unsupported
//! frame, immutable, and data relocations fail before invoking the target compiler.

use crate::mir::{BlockId, Function, InstKind, Module, Terminator, Value, ValueId};
use std::fmt::Write;

pub(super) fn lower(module: &Module) -> Result<String, String> {
    if module.immutable_count() != 0 || module.data_count() != 0 || module.is_library {
        return Err(
            "Yul lowering does not yet support immutable, data, or library relocations".into()
        );
    }
    let entry = module.dispatch_entry().ok_or("missing runtime entry")?;
    // object Contract { code { constructor(); return(runtime) } object Runtime { code { entry() } } }
    let mut out = String::from("object \"Contract\" { code { mstore(64, 128)\n");
    if let Some((id, _)) =
        module.functions.iter_enumerated().find(|(_, f)| f.attributes.is_constructor)
    {
        writeln!(out, "f{}()", id.index()).unwrap();
    } else {
        // if callvalue() { revert(0, 0) }
        out.push_str("if callvalue() { revert(0, 0) }\n");
    }
    out.push_str("datacopy(0, dataoffset(\"Runtime\"), datasize(\"Runtime\"))\nreturn(0, datasize(\"Runtime\"))\n");
    functions(module, &mut out)?;
    writeln!(out, "}} object \"Runtime\" {{ code {{ mstore(64, 128) f{}()", entry.index()).unwrap();
    functions(module, &mut out)?;
    out.push_str("} } }\n");
    Ok(out)
}

fn functions(module: &Module, out: &mut String) -> Result<(), String> {
    for (id, f) in module.functions.iter_enumerated() {
        // function fN(a0, ...) -> r0, ... { let block := 0; for {} 1 {} { switch block ... } }
        write!(
            out,
            "function f{}({})",
            id.index(),
            (0..f.params.len()).map(|i| format!("a{i}")).collect::<Vec<_>>().join(", ")
        )
        .unwrap();
        if !f.returns.is_empty() {
            write!(
                out,
                " -> {}",
                (0..f.returns.len()).map(|i| format!("r{i}")).collect::<Vec<_>>().join(", ")
            )
            .unwrap();
        }
        out.push_str(" {\n");
        for block in &f.blocks {
            for &inst in &block.instructions {
                if let Some(result) = f.inst_result_value(inst) {
                    writeln!(out, "let v{} := 0", result.index()).unwrap();
                }
            }
        }
        out.push_str("let block := 0\nfor {} 1 {} { switch block\n");
        for (bid, block) in f.blocks.iter_enumerated() {
            writeln!(out, "case {} {{", bid.index()).unwrap();
            for &iid in &block.instructions {
                let inst = f.inst(iid);
                if matches!(inst.kind, InstKind::Phi(_)) {
                    continue;
                }
                let expression = expression(f, &inst.kind)?;
                if let Some(result) = f.inst_result_value(iid) {
                    writeln!(out, "v{} := {expression}", result.index()).unwrap();
                } else {
                    writeln!(out, "{expression}").unwrap();
                }
            }
            // The selected edge copies phi inputs before changing the block number.
            match block.terminator.as_ref().ok_or("missing terminator")? {
                Terminator::Jump(target) => edge(f, bid, *target, out)?,
                Terminator::Branch { condition, then_block, else_block } => {
                    writeln!(out, "switch {} case 0 {{", value(f, *condition)?).unwrap();
                    edge(f, bid, *else_block, out)?;
                    out.push_str("} default {\n");
                    edge(f, bid, *then_block, out)?;
                    out.push_str("}\n");
                }
                Terminator::Switch { value: v, default, cases } => {
                    writeln!(out, "switch {}", value(f, *v)?).unwrap();
                    for &(v, target) in cases {
                        writeln!(out, "case {} {{", value(f, v)?).unwrap();
                        edge(f, bid, target, out)?;
                        out.push_str("}\n");
                    }
                    out.push_str("default {\n");
                    edge(f, bid, *default, out)?;
                    out.push_str("}\n");
                }
                Terminator::Return { values } => {
                    for (i, &v) in values.iter().enumerate() {
                        writeln!(out, "r{i} := {}", value(f, v)?).unwrap();
                    }
                    out.push_str("leave\n");
                }
                Terminator::TailCall { function, args } => {
                    writeln!(
                        out,
                        "f{}({})\nstop()",
                        function.index(),
                        args.iter()
                            .map(|&v| value(f, v))
                            .collect::<Result<Vec<_>, _>>()?
                            .join(", ")
                    )
                    .unwrap();
                }
                Terminator::ReturnData { offset, size } => {
                    writeln!(out, "return({}, {})", value(f, *offset)?, value(f, *size)?).unwrap()
                }
                Terminator::Revert { offset, size } => {
                    writeln!(out, "revert({}, {})", value(f, *offset)?, value(f, *size)?).unwrap()
                }
                Terminator::Stop => out.push_str("leave\n"),
                Terminator::Invalid => out.push_str("invalid()\n"),
                Terminator::SelfDestruct { recipient } => {
                    writeln!(out, "selfdestruct({})", value(f, *recipient)?).unwrap()
                }
                other => return Err(format!("unsupported Yul terminator: {other:?}")),
            }
            out.push_str("}\n");
        }
        out.push_str("default { invalid() }\n}\n}\n");
    }
    Ok(())
}

fn edge(f: &Function, from: BlockId, to: BlockId, out: &mut String) -> Result<(), String> {
    let mut copies = Vec::new();
    for &iid in &f.blocks[to].instructions {
        if let InstKind::Phi(inputs) = &f.inst(iid).kind {
            let input = inputs.iter().find(|(pred, _)| *pred == from).ok_or("missing phi input")?.1;
            copies.push((f.inst_result_value(iid).ok_or("phi without result")?, value(f, input)?));
        }
    }
    // let t0 := incoming0; ...; v0 := t0; ...; block := successor
    for (i, (_, input)) in copies.iter().enumerate() {
        writeln!(out, "let t{i} := {input}").unwrap();
    }
    for (i, (result, _)) in copies.iter().enumerate() {
        writeln!(out, "v{} := t{i}", result.index()).unwrap();
    }
    writeln!(out, "block := {}", to.index()).unwrap();
    Ok(())
}

fn value(f: &Function, id: ValueId) -> Result<String, String> {
    Ok(match f.value(id) {
        Value::Immediate(imm) => format!("0x{:x}", imm.as_u256().ok_or("non-word immediate")?),
        Value::Arg(arg) if f.params.is_empty() && f.selector.is_some() => {
            format!("calldataload({})", 4 + arg.index() * 32)
        }
        Value::Arg(arg) if arg.index() < f.params.len() => format!("a{}", arg.index()),
        Value::Arg(_) => return Err("unsupported lazy Yul argument".into()),
        Value::Inst(inst) => {
            format!("v{}", f.inst_result_value(*inst).ok_or("instruction without result")?.index())
        }
        Value::Undef(_) | Value::Error(_) => return Err("undefined value in Yul lowering".into()),
    })
}

fn expression(f: &Function, inst: &InstKind) -> Result<String, String> {
    // result := opcode(operands).
    let args = inst.operands().iter().map(|&v| value(f, v)).collect::<Result<Vec<_>, _>>()?;
    let name = match inst {
        InstKind::Shl(..) | InstKind::Shr(..) | InstKind::Sar(..) => inst.mnemonic(),
        InstKind::Fmp => return Ok("mload(64)".into()),
        InstKind::SetFmp(_) => return Ok(format!("mstore(64, {})", args[0])),
        InstKind::ICall { function, returns, .. } if *returns <= 1 => {
            return Ok(format!("f{}({})", function.index(), args.join(", ")));
        }
        InstKind::Add(..)
        | InstKind::Sub(..)
        | InstKind::Mul(..)
        | InstKind::Div(..)
        | InstKind::SDiv(..)
        | InstKind::Mod(..)
        | InstKind::SMod(..)
        | InstKind::Exp(..)
        | InstKind::AddMod(..)
        | InstKind::MulMod(..)
        | InstKind::And(..)
        | InstKind::Or(..)
        | InstKind::Xor(..)
        | InstKind::Not(..)
        | InstKind::Clz(..)
        | InstKind::Byte(..)
        | InstKind::Lt(..)
        | InstKind::Gt(..)
        | InstKind::SLt(..)
        | InstKind::SGt(..)
        | InstKind::Eq(..)
        | InstKind::IsZero(..)
        | InstKind::MLoad(..)
        | InstKind::MStore(..)
        | InstKind::MStore8(..)
        | InstKind::MSize
        | InstKind::MCopy(..)
        | InstKind::SLoad(..)
        | InstKind::SStore(..)
        | InstKind::TLoad(..)
        | InstKind::TStore(..)
        | InstKind::CalldataLoad(..)
        | InstKind::CalldataCopy(..)
        | InstKind::CalldataSize
        | InstKind::CodeSize
        | InstKind::CodeCopy(..)
        | InstKind::ExtCodeSize(..)
        | InstKind::ExtCodeCopy(..)
        | InstKind::ExtCodeHash(..)
        | InstKind::ReturnDataSize
        | InstKind::ReturnDataCopy(..)
        | InstKind::Caller
        | InstKind::CallValue
        | InstKind::Origin
        | InstKind::GasPrice
        | InstKind::BlockHash(..)
        | InstKind::Coinbase
        | InstKind::Timestamp
        | InstKind::BlockNumber
        | InstKind::PrevRandao
        | InstKind::GasLimit
        | InstKind::ChainId
        | InstKind::Address
        | InstKind::Balance(..)
        | InstKind::SelfBalance
        | InstKind::Gas
        | InstKind::BaseFee
        | InstKind::BlobBaseFee
        | InstKind::BlobHash(..)
        | InstKind::Keccak256(..)
        | InstKind::Call { .. }
        | InstKind::CallCode { .. }
        | InstKind::StaticCall { .. }
        | InstKind::DelegateCall { .. }
        | InstKind::Create(..)
        | InstKind::Create2(..)
        | InstKind::Log0(..)
        | InstKind::Log1(..)
        | InstKind::Log2(..)
        | InstKind::Log3(..)
        | InstKind::Log4(..)
        | InstKind::SignExtend(..) => inst.mnemonic(),
        other => return Err(format!("unsupported Yul instruction `{}`", other.mnemonic())),
    };
    Ok(format!("{name}({})", args.join(", ")))
}
