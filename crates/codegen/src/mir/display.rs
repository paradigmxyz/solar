//! Display implementations for MIR.
//!
//! Includes DOT format CFG generation for visualization.

use super::{Function, InstKind, Module, Terminator, Value, ValueId};
use std::fmt::Write;

/// Generates a DOT format CFG for a function.
pub fn function_to_dot(func: &Function) -> String {
    let mut dot = String::new();

    writeln!(dot, "digraph \"{}\" {{", func.name).unwrap();
    writeln!(dot, "    node [shape=box, fontname=\"Courier\", fontsize=10];").unwrap();
    writeln!(dot, "    edge [fontname=\"Courier\", fontsize=9];").unwrap();
    writeln!(dot).unwrap();

    // Generate nodes for each basic block
    for (block_id, block) in func.blocks.iter_enumerated() {
        let block_idx = block_id.index();
        let is_entry = block_id == func.entry_block;

        // Build the label with instructions
        let mut label = format!("bb{block_idx}");
        if is_entry {
            label.push_str(" (entry)");
        }
        label.push_str(":\\l");

        // Add instructions
        for &inst_id in &block.instructions {
            let inst = &func.instructions[inst_id];
            // Find which value this instruction defines
            let def_value = func
                .values
                .iter_enumerated()
                .find(|(_, v)| matches!(v, Value::Inst(id) if *id == inst_id))
                .map(|(vid, _)| vid);

            if let Some(vid) = def_value {
                write!(label, "  v{} = ", vid.index()).unwrap();
            } else {
                label.push_str("  ");
            }

            write!(label, "{}", format_inst_kind(&inst.kind, func)).unwrap();
            label.push_str("\\l");
        }

        // Add terminator
        if let Some(term) = &block.terminator {
            write!(label, "  {}", format_terminator(term)).unwrap();
            label.push_str("\\l");
        }

        // Node attributes
        let color = if is_entry { ", fillcolor=\"#e0ffe0\", style=filled" } else { "" };
        writeln!(dot, "    bb{block_idx} [label=\"{label}\"{color}];").unwrap();
    }

    writeln!(dot).unwrap();

    // Generate edges
    for (block_id, block) in func.blocks.iter_enumerated() {
        let block_idx = block_id.index();

        if let Some(term) = &block.terminator {
            match term {
                Terminator::Jump(target) => {
                    writeln!(dot, "    bb{} -> bb{};", block_idx, target.index()).unwrap();
                }
                Terminator::Branch { condition, then_block, else_block } => {
                    writeln!(
                        dot,
                        "    bb{} -> bb{} [label=\"v{} == true\", color=\"green\"];",
                        block_idx,
                        then_block.index(),
                        condition.index()
                    )
                    .unwrap();
                    writeln!(
                        dot,
                        "    bb{} -> bb{} [label=\"false\", color=\"red\"];",
                        block_idx,
                        else_block.index()
                    )
                    .unwrap();
                }
                Terminator::Switch { value: _, default, cases } => {
                    writeln!(
                        dot,
                        "    bb{} -> bb{} [label=\"default\"];",
                        block_idx,
                        default.index()
                    )
                    .unwrap();
                    for (case_val, target) in cases {
                        writeln!(
                            dot,
                            "    bb{} -> bb{} [label=\"v{}\"];",
                            block_idx,
                            target.index(),
                            case_val.index()
                        )
                        .unwrap();
                    }
                }
                Terminator::Return { .. }
                | Terminator::Revert { .. }
                | Terminator::Stop
                | Terminator::SelfDestruct { .. }
                | Terminator::Invalid => {
                    // No outgoing edges
                }
            }
        }
    }

    writeln!(dot, "}}").unwrap();
    dot
}

/// Generates DOT format for an entire module.
pub fn module_to_dot(module: &Module) -> String {
    let mut result = String::new();

    for (func_id, func) in module.functions.iter_enumerated() {
        if func_id.index() > 0 {
            result.push_str("\n\n");
        }
        result.push_str(&function_to_dot(func));
    }

    result
}

/// Formats an instruction kind for display.
fn format_inst_kind(kind: &InstKind, func: &Function) -> String {
    match kind {
        // Arithmetic
        InstKind::Add(a, b) => format!("add {}, {}", fmt_val(*a, func), fmt_val(*b, func)),
        InstKind::Sub(a, b) => format!("sub {}, {}", fmt_val(*a, func), fmt_val(*b, func)),
        InstKind::Mul(a, b) => format!("mul {}, {}", fmt_val(*a, func), fmt_val(*b, func)),
        InstKind::Div(a, b) => format!("div {}, {}", fmt_val(*a, func), fmt_val(*b, func)),
        InstKind::SDiv(a, b) => format!("sdiv {}, {}", fmt_val(*a, func), fmt_val(*b, func)),
        InstKind::Mod(a, b) => format!("mod {}, {}", fmt_val(*a, func), fmt_val(*b, func)),
        InstKind::SMod(a, b) => format!("smod {}, {}", fmt_val(*a, func), fmt_val(*b, func)),
        InstKind::Exp(a, b) => format!("exp {}, {}", fmt_val(*a, func), fmt_val(*b, func)),
        InstKind::AddMod(a, b, n) => {
            format!("addmod {}, {}, {}", fmt_val(*a, func), fmt_val(*b, func), fmt_val(*n, func))
        }
        InstKind::MulMod(a, b, n) => {
            format!("mulmod {}, {}, {}", fmt_val(*a, func), fmt_val(*b, func), fmt_val(*n, func))
        }

        // Bitwise
        InstKind::And(a, b) => format!("and {}, {}", fmt_val(*a, func), fmt_val(*b, func)),
        InstKind::Or(a, b) => format!("or {}, {}", fmt_val(*a, func), fmt_val(*b, func)),
        InstKind::Xor(a, b) => format!("xor {}, {}", fmt_val(*a, func), fmt_val(*b, func)),
        InstKind::Not(a) => format!("not {}", fmt_val(*a, func)),
        InstKind::Shl(a, b) => format!("shl {}, {}", fmt_val(*a, func), fmt_val(*b, func)),
        InstKind::Shr(a, b) => format!("shr {}, {}", fmt_val(*a, func), fmt_val(*b, func)),
        InstKind::Sar(a, b) => format!("sar {}, {}", fmt_val(*a, func), fmt_val(*b, func)),
        InstKind::Byte(a, b) => format!("byte {}, {}", fmt_val(*a, func), fmt_val(*b, func)),

        // Comparison
        InstKind::Lt(a, b) => format!("lt {}, {}", fmt_val(*a, func), fmt_val(*b, func)),
        InstKind::Gt(a, b) => format!("gt {}, {}", fmt_val(*a, func), fmt_val(*b, func)),
        InstKind::SLt(a, b) => format!("slt {}, {}", fmt_val(*a, func), fmt_val(*b, func)),
        InstKind::SGt(a, b) => format!("sgt {}, {}", fmt_val(*a, func), fmt_val(*b, func)),
        InstKind::Eq(a, b) => format!("eq {}, {}", fmt_val(*a, func), fmt_val(*b, func)),
        InstKind::IsZero(a) => format!("iszero {}", fmt_val(*a, func)),

        // Memory
        InstKind::MLoad(offset) => format!("mload {}", fmt_val(*offset, func)),
        InstKind::MStore(offset, val) => {
            format!("mstore {}, {}", fmt_val(*offset, func), fmt_val(*val, func))
        }
        InstKind::MStore8(offset, val) => {
            format!("mstore8 {}, {}", fmt_val(*offset, func), fmt_val(*val, func))
        }
        InstKind::MSize => "msize".to_string(),
        InstKind::MCopy(dest, src, len) => format!(
            "mcopy {}, {}, {}",
            fmt_val(*dest, func),
            fmt_val(*src, func),
            fmt_val(*len, func)
        ),

        // Storage
        InstKind::SLoad(slot) => format!("sload {}", fmt_val(*slot, func)),
        InstKind::SStore(slot, val) => {
            format!("sstore {}, {}", fmt_val(*slot, func), fmt_val(*val, func))
        }
        InstKind::TLoad(slot) => format!("tload {}", fmt_val(*slot, func)),
        InstKind::TStore(slot, val) => {
            format!("tstore {}, {}", fmt_val(*slot, func), fmt_val(*val, func))
        }

        // Calldata
        InstKind::CalldataLoad(offset) => format!("calldataload {}", fmt_val(*offset, func)),
        InstKind::CalldataSize => "calldatasize".to_string(),
        InstKind::CalldataCopy(dest, offset, size) => {
            format!(
                "calldatacopy {}, {}, {}",
                fmt_val(*dest, func),
                fmt_val(*offset, func),
                fmt_val(*size, func)
            )
        }

        // Code
        InstKind::CodeSize => "codesize".to_string(),
        InstKind::CodeCopy(dest, offset, size) => {
            format!(
                "codecopy {}, {}, {}",
                fmt_val(*dest, func),
                fmt_val(*offset, func),
                fmt_val(*size, func)
            )
        }
        InstKind::ExtCodeSize(addr) => format!("extcodesize {}", fmt_val(*addr, func)),
        InstKind::ExtCodeCopy(addr, dest, offset, size) => {
            format!(
                "extcodecopy {}, {}, {}, {}",
                fmt_val(*addr, func),
                fmt_val(*dest, func),
                fmt_val(*offset, func),
                fmt_val(*size, func)
            )
        }
        InstKind::ExtCodeHash(addr) => format!("extcodehash {}", fmt_val(*addr, func)),

        // Return data
        InstKind::ReturnDataSize => "returndatasize".to_string(),
        InstKind::ReturnDataCopy(dest, offset, size) => {
            format!(
                "returndatacopy {}, {}, {}",
                fmt_val(*dest, func),
                fmt_val(*offset, func),
                fmt_val(*size, func)
            )
        }

        // Environment
        InstKind::Caller => "caller".to_string(),
        InstKind::CallValue => "callvalue".to_string(),
        InstKind::Origin => "origin".to_string(),
        InstKind::GasPrice => "gasprice".to_string(),
        InstKind::BlockHash(num) => format!("blockhash {}", fmt_val(*num, func)),
        InstKind::Coinbase => "coinbase".to_string(),
        InstKind::Timestamp => "timestamp".to_string(),
        InstKind::BlockNumber => "number".to_string(),
        InstKind::PrevRandao => "prevrandao".to_string(),
        InstKind::GasLimit => "gaslimit".to_string(),
        InstKind::ChainId => "chainid".to_string(),
        InstKind::Address => "address".to_string(),
        InstKind::Balance(addr) => format!("balance {}", fmt_val(*addr, func)),
        InstKind::SelfBalance => "selfbalance".to_string(),
        InstKind::Gas => "gas".to_string(),
        InstKind::BaseFee => "basefee".to_string(),
        InstKind::BlobBaseFee => "blobbasefee".to_string(),
        InstKind::BlobHash(idx) => format!("blobhash {}", fmt_val(*idx, func)),

        // Hashing
        InstKind::Keccak256(offset, size) => {
            format!("keccak256 {}, {}", fmt_val(*offset, func), fmt_val(*size, func))
        }

        // Calls
        InstKind::Call { gas, addr, value, args_offset, args_size, ret_offset, ret_size } => {
            format!(
                "call {}, {}, {}, {}, {}, {}, {}",
                fmt_val(*gas, func),
                fmt_val(*addr, func),
                fmt_val(*value, func),
                fmt_val(*args_offset, func),
                fmt_val(*args_size, func),
                fmt_val(*ret_offset, func),
                fmt_val(*ret_size, func)
            )
        }
        InstKind::StaticCall { gas, addr, args_offset, args_size, ret_offset, ret_size } => {
            format!(
                "staticcall {}, {}, {}, {}, {}, {}",
                fmt_val(*gas, func),
                fmt_val(*addr, func),
                fmt_val(*args_offset, func),
                fmt_val(*args_size, func),
                fmt_val(*ret_offset, func),
                fmt_val(*ret_size, func)
            )
        }
        InstKind::DelegateCall { gas, addr, args_offset, args_size, ret_offset, ret_size } => {
            format!(
                "delegatecall {}, {}, {}, {}, {}, {}",
                fmt_val(*gas, func),
                fmt_val(*addr, func),
                fmt_val(*args_offset, func),
                fmt_val(*args_size, func),
                fmt_val(*ret_offset, func),
                fmt_val(*ret_size, func)
            )
        }

        // Contract creation
        InstKind::Create(value, offset, size) => {
            format!(
                "create {}, {}, {}",
                fmt_val(*value, func),
                fmt_val(*offset, func),
                fmt_val(*size, func)
            )
        }
        InstKind::Create2(value, offset, size, salt) => {
            format!(
                "create2 {}, {}, {}, {}",
                fmt_val(*value, func),
                fmt_val(*offset, func),
                fmt_val(*size, func),
                fmt_val(*salt, func)
            )
        }

        // Logs
        InstKind::Log0(offset, size) => {
            format!("log0 {}, {}", fmt_val(*offset, func), fmt_val(*size, func))
        }
        InstKind::Log1(offset, size, t1) => format!(
            "log1 {}, {}, {}",
            fmt_val(*offset, func),
            fmt_val(*size, func),
            fmt_val(*t1, func)
        ),
        InstKind::Log2(offset, size, t1, t2) => format!(
            "log2 {}, {}, {}, {}",
            fmt_val(*offset, func),
            fmt_val(*size, func),
            fmt_val(*t1, func),
            fmt_val(*t2, func)
        ),
        InstKind::Log3(offset, size, t1, t2, t3) => format!(
            "log3 {}, {}, {}, {}, {}",
            fmt_val(*offset, func),
            fmt_val(*size, func),
            fmt_val(*t1, func),
            fmt_val(*t2, func),
            fmt_val(*t3, func)
        ),
        InstKind::Log4(offset, size, t1, t2, t3, t4) => format!(
            "log4 {}, {}, {}, {}, {}, {}",
            fmt_val(*offset, func),
            fmt_val(*size, func),
            fmt_val(*t1, func),
            fmt_val(*t2, func),
            fmt_val(*t3, func),
            fmt_val(*t4, func)
        ),

        // SSA
        InstKind::Phi(args) => {
            let args_str: Vec<_> = args
                .iter()
                .map(|(block, val)| format!("[bb{}: {}]", block.index(), fmt_val(*val, func)))
                .collect();
            format!("phi {}", args_str.join(", "))
        }
        InstKind::Select(cond, t, f) => {
            format!("select {}, {}, {}", fmt_val(*cond, func), fmt_val(*t, func), fmt_val(*f, func))
        }

        // Sign extension
        InstKind::SignExtend(size, val) => {
            format!("signextend {}, {}", fmt_val(*size, func), fmt_val(*val, func))
        }
    }
}

/// Format a value reference.
fn fmt_val(vid: ValueId, func: &Function) -> String {
    match &func.values[vid] {
        Value::Immediate(imm) => {
            if let Some(u256) = imm.as_u256() {
                if u256 < alloy_primitives::U256::from(1000u64) {
                    format!("{u256}")
                } else {
                    format!("0x{u256:x}")
                }
            } else {
                format!("v{}", vid.index())
            }
        }
        Value::Arg { index, .. } => format!("arg{index}"),
        _ => format!("v{}", vid.index()),
    }
}

/// Format a terminator for display.
fn format_terminator(term: &Terminator) -> String {
    match term {
        Terminator::Jump(target) => format!("jump bb{}", target.index()),
        Terminator::Branch { condition, then_block, else_block } => {
            format!("br v{}, bb{}, bb{}", condition.index(), then_block.index(), else_block.index())
        }
        Terminator::Switch { value, default, cases } => {
            let cases_str: Vec<_> = cases
                .iter()
                .map(|(val, block)| format!("v{} => bb{}", val.index(), block.index()))
                .collect();
            format!(
                "switch v{}, default bb{}, [{}]",
                value.index(),
                default.index(),
                cases_str.join(", ")
            )
        }
        Terminator::Return { values } => {
            if values.is_empty() {
                "ret".to_string()
            } else {
                let vals: Vec<_> = values.iter().map(|v| format!("v{}", v.index())).collect();
                format!("ret {}", vals.join(", "))
            }
        }
        Terminator::Revert { offset, size } => {
            format!("revert v{}, v{}", offset.index(), size.index())
        }
        Terminator::Stop => "stop".to_string(),
        Terminator::SelfDestruct { recipient } => format!("selfdestruct v{}", recipient.index()),
        Terminator::Invalid => "invalid".to_string(),
    }
}
