//! Lowers word-based MIR to structured Yul.
//!
//! Each MIR function uses a block-number loop so arbitrary control flow stays
//! explicit. Edge-local parallel copies implement phis without clobbering loop
//! values. The Yul optimizer can simplify this structure before stack scheduling.
//! ABI, dispatch, and semantic memory lowering run before this backend. Data uses
//! object sections; constructor values patch native Yul immutable relocations.
//! Activation frames use the heap; multi-value returns publish a reserved buffer.

use crate::mir::{
    BlockId, Function, InstKind, Module, Terminator, Value, ValueId, analysis::CallGraphInfo,
};
use alloy_primitives::hex;
use std::fmt::Write;

pub(super) fn lower(module: &Module) -> Result<String, String> {
    let entry = module.dispatch_entry().ok_or("missing runtime entry")?;
    let staging = crate::mir::immutable::immutable_staging_base(module);
    let (return_base, args_base) = super::alternative::memory_layout(module);
    // codecopy(args_base, datasize(Contract), codesize() - datasize(Contract))
    // mstore(64, ceil32(args_base + argument_size)); constructor()
    let mut out = format!("object \"Contract\" {{ code {{\nmstore(64, {args_base})\n");
    let constructor = module
        .functions
        .iter_enumerated()
        .find(|(_, f)| f.attributes.is_constructor)
        .map(|(id, _)| id);
    if let Some(id) = module.library_deploy_address() {
        writeln!(out, "mstore({}, address())", staging + id.index() as u64 * 32).unwrap();
    }
    if let Some(id) = constructor {
        writeln!(out, "let argsize := sub(codesize(), datasize(\"Contract\"))\ncodecopy({args_base}, datasize(\"Contract\"), argsize)\nmstore(64, and(add(add({args_base}, argsize), 31), not(31)))\nf{}({})", id.index(), (0..module.functions[id].params.len()).map(|i| format!("mload({})", args_base + i as u64 * 32)).collect::<Vec<_>>().join(", ")).unwrap();
    } else if !module.is_library {
        out.push_str("if callvalue() { revert(0, 0) }\n");
    }
    // datacopy(deploy, Runtime, sizeof(Runtime)); setimmutable(deploy, id, staged_value)
    // return(deploy, sizeof(Runtime))
    out.push_str("let deploy := mload(64)\ndatacopy(deploy, dataoffset(\"Runtime\"), datasize(\"Runtime\"))\n");
    for (id, _) in module.iter_immutables() {
        writeln!(
            out,
            "setimmutable(deploy, \"i{}\", mload({}))",
            id.index(),
            staging + id.index() as u64 * 32
        )
        .unwrap();
    }
    // datacopy(deploy + runtime_size + tail_offset, data, length)
    let mut tail_offset = 0;
    for (id, bytes) in module.iter_data() {
        if module.data_is_emitted_in_runtime(id) {
            writeln!(out, "datacopy(add(deploy, add(datasize(\"Runtime\"), {tail_offset})), dataoffset(\"d{}\"), {})", id.index(), bytes.len()).unwrap();
            tail_offset += bytes.len();
        }
    }
    writeln!(out, "return(deploy, add(datasize(\"Runtime\"), {tail_offset}))").unwrap();
    functions(module, true, staging, args_base, return_base, &mut out)?;
    out.push_str("}\n");
    data(module, &mut out);
    writeln!(
        out,
        "object \"Runtime\" {{ code {{ mstore(64, {args_base}) f{}() stop()",
        entry.index()
    )
    .unwrap();
    functions(module, false, staging, args_base, return_base, &mut out)?;
    out.push_str("}\n");
    data(module, &mut out);
    out.push_str("} }\n");
    Ok(out)
}

fn data(module: &Module, out: &mut String) {
    // data dN hex"bytes"
    for (id, bytes) in module.iter_data() {
        writeln!(out, "data \"d{}\" hex\"{}\"", id.index(), hex::encode(bytes)).unwrap();
    }
}

fn functions(
    module: &Module,
    constructor: bool,
    staging: u64,
    args_base: u64,
    return_base: u64,
    out: &mut String,
) -> Result<(), String> {
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
    if let Some(root) = root {
        reachable.insert(root);
    }
    // select(cond, yes, no) -> switch cond { case 0: no; default: yes }
    out.push_str("function choose(c, a, b) -> r { r := b if c { r := a } }\n");
    for id in reachable.iter() {
        let f = &module.functions[id];
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
                (0..f.returns.len().min(1)).map(|i| format!("r{i}")).collect::<Vec<_>>().join(", ")
            )
            .unwrap();
        }
        out.push_str(" {\n");
        let frame_size = super::alternative::frame_size(f);
        if frame_size != 0 {
            // frame := mload(64); mstore(64, frame + activation_size)
            writeln!(out, "let frame := mload(64) mstore(64, add(frame, {frame_size}))").unwrap();
        }
        if constructor && f.params.is_empty() && f.attributes.is_constructor {
            // aN := mload(args_base + 32 * argument_index)
            for arg in f.arg_indices() {
                writeln!(
                    out,
                    "let a{} := mload({})",
                    arg.index(),
                    args_base + arg.index() as u64 * 32
                )
                .unwrap();
            }
        }
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
                let expression = expression(f, &inst.kind, constructor, staging, args_base)?;
                if let Some(result) = f.inst_result_value(iid) {
                    writeln!(out, "v{} := {expression}", result.index()).unwrap();
                } else if let InstKind::ICall { function, .. } = inst.kind
                    && !module.functions[function].returns.is_empty()
                {
                    writeln!(out, "pop({expression})").unwrap();
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
                    if values.len() > 1 {
                        // mstore(return_base + 32 * i, result_i); mstore(32, return_base)
                        for (i, &v) in values.iter().enumerate() {
                            writeln!(
                                out,
                                "mstore({}, {})",
                                return_base + i as u64 * 32,
                                value(f, v)?
                            )
                            .unwrap();
                        }
                        writeln!(out, "mstore(32, {return_base})").unwrap();
                    }
                    for (i, &v) in values.iter().take(1).enumerate() {
                        writeln!(out, "r{i} := {}", value(f, v)?).unwrap();
                    }
                    out.push_str("leave\n");
                }
                Terminator::TailCall { function, args } => {
                    // call callee(args); discard any declared result; stop
                    let call = format!(
                        "f{}({})",
                        function.index(),
                        args.iter()
                            .map(|&v| value(f, v))
                            .collect::<Result<Vec<_>, _>>()?
                            .join(", ")
                    );
                    if module.functions[*function].returns.is_empty() {
                        writeln!(out, "{call}").unwrap();
                    } else {
                        writeln!(out, "pop({call})").unwrap();
                    }
                    out.push_str("stop()\n");
                }
                Terminator::ReturnData { offset, size } => {
                    writeln!(out, "return({}, {})", value(f, *offset)?, value(f, *size)?).unwrap()
                }
                Terminator::Revert { offset, size } => {
                    writeln!(out, "revert({}, {})", value(f, *offset)?, value(f, *size)?).unwrap()
                }
                Terminator::RevertReturndata => out.push_str(
                    "returndatacopy(0, 0, returndatasize()) revert(0, returndatasize())\n",
                ),
                Terminator::Stop => out.push_str("leave\n"),
                Terminator::Invalid => out.push_str("invalid()\n"),
                Terminator::SelfDestruct { recipient } => {
                    writeln!(out, "selfdestruct({})", value(f, *recipient)?).unwrap()
                }
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
        Value::Arg(arg) if arg.index() < f.params.len() || f.attributes.is_constructor => {
            format!("a{}", arg.index())
        }
        Value::Arg(_) => return Err("unsupported lazy Yul argument".into()),
        Value::Inst(inst) => {
            format!("v{}", f.inst_result_value(*inst).ok_or("instruction without result")?.index())
        }
        Value::Undef(_) | Value::Error(_) => return Err("undefined value in Yul lowering".into()),
    })
}

fn expression(
    f: &Function,
    inst: &InstKind,
    constructor: bool,
    staging: u64,
    args_base: u64,
) -> Result<String, String> {
    // result := opcode(operands).
    let args = inst.operands().iter().map(|&v| value(f, v)).collect::<Result<Vec<_>, _>>()?;
    let name = match inst {
        InstKind::DataCopy(data, ..) => {
            return Ok(format!(
                "datacopy({}, add(dataoffset(\"d{}\"), {}), {})",
                args[0],
                data.id.index(),
                data.offset,
                args[1]
            ));
        }
        InstKind::ConstructorArgsBase if constructor => return Ok(args_base.to_string()),
        InstKind::ConstructorArgsEnd if constructor => {
            return Ok(format!("add({args_base}, sub(codesize(), datasize(\"Contract\")))"));
        }
        InstKind::StoreImmutable(id, _) if constructor => {
            return Ok(format!("mstore({}, {})", staging + id.index() as u64 * 32, args[0]));
        }
        InstKind::LoadImmutable(id) => {
            return Ok(if constructor {
                format!("mload({})", staging + id.index() as u64 * 32)
            } else {
                format!("loadimmutable(\"i{}\")", id.index())
            });
        }
        InstKind::InternalFrameAddr(offset) => return Ok(format!("add(frame, {offset})")),
        InstKind::Select(..) => "choose",
        InstKind::Shl(..) | InstKind::Shr(..) | InstKind::Sar(..) => inst.mnemonic(),
        InstKind::Fmp => return Ok("mload(64)".into()),
        InstKind::SetFmp(_) => return Ok(format!("mstore(64, {})", args[0])),
        InstKind::ICall { function, .. } => {
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
