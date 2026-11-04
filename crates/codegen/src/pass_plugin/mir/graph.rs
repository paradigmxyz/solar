//! Construction of internal MIR functions and CFG fragments for native plugins.
//!
//! Signatures, blocks, and SSA results are declared before their bodies, allowing mutual calls,
//! forward branches, and loop phis. Response-local SSA names never alias existing value IDs.
//! Operands and indices are checked before calling IR constructors; the enclosing transaction
//! then verifies dominance, signatures, phi coverage, types, and phase legality. New code has
//! unknown source context unless it copies an existing instruction's debug information.

use crate::mir::{
    BlockId, Callee, Function, FunctionBuilder, FunctionId, Immediate, InstId, InstKind,
    Instruction, MirType, Module, PtrLocation, StructId, Terminator, Value, ValueId,
};
use alloy_primitives::U256;
use solar_data_structures::map::FxHashMap;
use solar_interface::{Ident, Symbol};
use solar_pass_plugin::{self as api, Edit};
use std::{collections::hash_map::Entry, num::NonZeroU32};

pub(super) type Locals = FxHashMap<(FunctionId, u32), ValueId>;

pub(super) fn apply_edit(
    module: &mut Module,
    edit: Edit,
    locals: &mut Locals,
) -> Result<(), String> {
    let types = module.struct_types.len();
    if let Edit::AddMirFunctions { functions } = edit {
        let first = module.functions.len();
        for definition in &functions {
            let name = &definition.name;
            if name.is_empty()
                || !name.bytes().enumerate().all(|(index, byte)| {
                    byte == b'_'
                        || byte.is_ascii_alphabetic()
                        || (index != 0 && byte.is_ascii_digit())
                })
                || module
                    .functions
                    .iter()
                    .any(|function| function.name.symbol.as_str() == name.as_str())
            {
                return Err("invalid or duplicate new MIR function name".into());
            }
            let mut function = Function::new(Ident::with_dummy_span(Symbol::intern(name)));
            for parameter in &definition.parameters {
                let ty = value_type(parameter, types)?;
                // @helper(argN: TYPE)
                FunctionBuilder::new_semantic(&mut function).add_param(ty);
            }
            function.set_return_type(convert_type(&definition.returns, types)?);
            // module.functions += @helper
            module.add_function(function);
        }
        let functions_len = module.functions.len();
        for (index, definition) in functions.into_iter().enumerate() {
            if definition.blocks.is_empty() {
                return Err("new MIR function requires an entry block".into());
            }
            let id = FunctionId::from_usize(first + index);
            let function = &mut module.functions[id];
            for _ in 1..definition.blocks.len() {
                // bbN:
                function.alloc_block();
            }
            build_blocks(function, id, 0, definition.blocks, locals, types, functions_len)?;
            crate::mir::utils::rebuild_predecessors(function);
        }
        return Ok(());
    }
    let (Edit::AddMirBlocks { function, .. }
    | Edit::SpliceMirInstructions { function, .. }
    | Edit::SetMirTerminator { function, .. }
    | Edit::SplitMirBlock { function, .. }
    | Edit::ReplaceMirValueWith { function, .. }) = &edit
    else {
        return Err("EVM IR edit returned by a MIR pass".into());
    };
    if *function as usize >= module.functions.len() {
        return Err("unknown MIR function".into());
    }
    let functions = module.functions.len();
    let id = FunctionId::from_usize(*function as usize);
    let func = &mut module.functions[id];
    let cfg_changed = matches!(
        &edit,
        Edit::AddMirBlocks { .. } | Edit::SetMirTerminator { .. } | Edit::SplitMirBlock { .. }
    );
    let result = match edit {
        Edit::AddMirBlocks { blocks, .. } => {
            let first = func.blocks.len();
            for _ in &blocks {
                // bbN:
                func.alloc_block();
            }
            build_blocks(func, id, first, blocks, locals, types, functions)
        }
        Edit::SpliceMirInstructions { block, start, remove, instructions, .. } => {
            let block = checked_block(func, block)?;
            let start = start as usize;
            let end = start.checked_add(remove as usize).ok_or("instruction window overflow")?;
            if end > func.blocks[block].instructions.len() {
                return Err("MIR instruction window is out of bounds".into());
            }
            reserve_results(func, id, instructions.iter(), locals, types)?;
            let replacement = instructions
                .into_iter()
                .map(|instruction| {
                    build_instruction(func, id, instruction, locals, types, functions)
                })
                .collect::<Result<Vec<_>, _>>()?;
            // bb.instructions[start..end] => replacement
            func.blocks[block].instructions.splice(start..end, replacement);
            Ok(())
        }
        Edit::SetMirTerminator { block, terminator, .. } => {
            let block = checked_block(func, block)?;
            let terminator = build_terminator(func, id, terminator, locals, types, functions)?;
            // bb.terminator => terminator
            func.blocks[block].set_generated_terminator(terminator);
            Ok(())
        }
        Edit::ReplaceMirValueWith { value, replacement, .. } => {
            let value = super::checked_value(func, value)?;
            let replacement = operand(func, id, replacement, locals, types)?;
            super::replace_value(func, value, replacement)
        }
        Edit::SplitMirBlock { block, at, .. } => {
            let block = checked_block(func, block)?;
            let at = at as usize;
            if at > func.blocks[block].instructions.len() {
                return Err("MIR split position is out of bounds".into());
            }
            if func.blocks[block].instructions[at..]
                .iter()
                .any(|&inst| matches!(func.inst(inst).kind, InstKind::Phi(_)))
            {
                return Err("MIR block split would move phi nodes".into());
            }
            let instructions = func.blocks[block].instructions.split_off(at);
            let (terminator, metadata) = func.blocks[block].take_terminator();
            let terminator = terminator.ok_or("cannot split an unterminated MIR block")?;
            let next = func.alloc_block();
            let mut successors = Vec::new();
            terminator.for_each_successor(|successor| {
                if !successors.contains(&successor) {
                    successors.push(successor);
                }
            });
            // bb: prefix; jump next
            // next: suffix; old_terminator
            func.blocks[next].instructions = instructions;
            func.blocks[next].set_terminator(terminator, metadata);
            func.blocks[block].set_generated_terminator(Terminator::Jump(next));
            for successor in successors {
                let phis = func.blocks[successor].instructions.clone();
                for inst in phis {
                    let InstKind::Phi(incoming) = &mut func.inst_mut(inst).kind else { continue };
                    // phi [bb: value] => phi [next: value]
                    for (predecessor, _) in incoming {
                        if *predecessor == block {
                            *predecessor = next;
                        }
                    }
                }
            }
            Ok(())
        }
        _ => unreachable!("checked graph edit above"),
    };
    result?;
    if cfg_changed {
        crate::mir::utils::rebuild_predecessors(func);
    }
    Ok(())
}

fn convert_type(ty: &api::Type, types: usize) -> Result<MirType, String> {
    Ok(match ty {
        api::Type::Integer { bits } => {
            if !matches!(*bits, 1 | 160 | 256) {
                return Err("unsupported MIR integer width".into());
            }
            MirType::Int(NonZeroU32::new(*bits).unwrap())
        }
        api::Type::Pointer { location } => MirType::Ptr(address_space(*location)),
        api::Type::Slice { location } => MirType::Slice(address_space(*location)),
        api::Type::Struct { id } => {
            if *id as usize >= types {
                return Err("unknown MIR struct type".into());
            }
            MirType::Struct(StructId::from_usize(*id as usize))
        }
        api::Type::Void => MirType::Void,
    })
}

fn value_type(ty: &api::Type, types: usize) -> Result<MirType, String> {
    let ty = convert_type(ty, types)?;
    if ty == MirType::Void {
        return Err("MIR value cannot have void type".into());
    }
    Ok(ty)
}

fn address_space(space: api::AddressSpace) -> PtrLocation {
    match space {
        api::AddressSpace::Memory => PtrLocation::Memory,
        api::AddressSpace::Calldata => PtrLocation::Calldata,
        api::AddressSpace::Returndata => PtrLocation::Returndata,
        api::AddressSpace::Storage => PtrLocation::Storage,
        api::AddressSpace::Transient => PtrLocation::Transient,
    }
}

fn checked_block(func: &Function, block: u32) -> Result<BlockId, String> {
    if block as usize >= func.blocks.len() {
        return Err("unknown MIR block".into());
    }
    Ok(BlockId::from_usize(block as usize))
}

fn operand(
    func: &mut Function,
    id: FunctionId,
    value: api::MirOperand,
    locals: &Locals,
    types: usize,
) -> Result<ValueId, String> {
    match value {
        api::MirOperand::Value { id } => super::checked_value(func, id),
        api::MirOperand::Local { id: local } => locals
            .get(&(id, local))
            .copied()
            .ok_or_else(|| "unknown response-local SSA value".into()),
        api::MirOperand::Argument { index } => {
            let argument = func.arg_indices().nth(index as usize).ok_or("unknown MIR argument")?;
            Ok(func.alloc_arg(argument))
        }
        api::MirOperand::Constant { ty, value } => {
            let ty = value_type(&ty, types)?;
            let value = value.parse::<U256>().map_err(|_| "invalid MIR constant")?;
            match ty {
                MirType::Int(bits) if value.bit_len() <= bits.get() as usize => {}
                MirType::Ptr(_) => {}
                _ => return Err("constant does not fit its scalar MIR type".into()),
            }
            Ok(func.alloc_value(Value::Immediate(Immediate::for_type(Some(ty), value))))
        }
    }
}

fn reserve_results<'a>(
    func: &mut Function,
    id: FunctionId,
    instructions: impl Iterator<Item = &'a api::NewMirInstruction>,
    locals: &mut Locals,
    types: usize,
) -> Result<(), String> {
    for instruction in instructions {
        match (instruction.result, &instruction.ty) {
            (None, None) => {}
            (Some(local), Some(ty)) => {
                let ty = value_type(ty, types)?;
                let Entry::Vacant(entry) = locals.entry((id, local)) else {
                    return Err("duplicate response-local SSA definition".into());
                };
                // %local = undef TYPE
                entry.insert(func.alloc_value(Value::Undef(ty)));
            }
            _ => return Err("MIR result requires both a local ID and a type".into()),
        }
    }
    Ok(())
}

fn build_blocks(
    func: &mut Function,
    id: FunctionId,
    first: usize,
    blocks: Vec<api::NewMirBlock>,
    locals: &mut Locals,
    types: usize,
    functions: usize,
) -> Result<(), String> {
    reserve_results(
        func,
        id,
        blocks.iter().flat_map(|block| block.instructions.iter()),
        locals,
        types,
    )?;
    for (index, block) in blocks.into_iter().enumerate() {
        let instructions = block
            .instructions
            .into_iter()
            .map(|instruction| build_instruction(func, id, instruction, locals, types, functions))
            .collect::<Result<Vec<_>, _>>()?;
        let terminator = build_terminator(func, id, block.terminator, locals, types, functions)?;
        let block = &mut func.blocks[BlockId::from_usize(first + index)];
        // bb.instructions = instructions
        // bb.terminator = terminator
        block.instructions = instructions;
        block.set_generated_terminator(terminator);
    }
    Ok(())
}

fn build_instruction(
    func: &mut Function,
    id: FunctionId,
    instruction: api::NewMirInstruction,
    locals: &Locals,
    types: usize,
    functions: usize,
) -> Result<InstId, String> {
    let ty = instruction.ty.as_ref().map(|ty| value_type(ty, types)).transpose()?;
    let mut debug = None;
    let kind = match instruction.instruction {
        api::MirInstructionKind::Operation { operation, operands } => {
            let operands = operands
                .into_iter()
                .map(|value| operand(func, id, value, locals, types))
                .collect::<Result<Vec<_>, _>>()?;
            InstKind::from_operands(&operation, &operands)
                .ok_or("unknown operand-only operation or invalid operand count")?
        }
        api::MirInstructionKind::Phi { incoming } => {
            if ty.is_none() {
                return Err("new MIR phi requires an SSA result".into());
            }
            InstKind::Phi(
                incoming
                    .into_iter()
                    .map(|(block, value)| {
                        Ok((checked_block(func, block)?, operand(func, id, value, locals, types)?))
                    })
                    .collect::<Result<_, String>>()?,
            )
        }
        api::MirInstructionKind::Call { function, args } => {
            if function as usize >= functions {
                return Err("unknown MIR callee".into());
            }
            let args = args
                .into_iter()
                .map(|value| operand(func, id, value, locals, types))
                .collect::<Result<_, _>>()?;
            InstKind::ICall {
                function: Callee::Function(FunctionId::from_usize(function as usize)),
                args,
            }
        }
        api::MirInstructionKind::Copy { instruction, operands } => {
            if instruction as usize >= func.num_insts() {
                return Err("unknown copied MIR instruction".into());
            }
            let instruction = InstId::from_usize(instruction as usize);
            if !func.instructions().any(|active| active == instruction) {
                return Err("inactive copied MIR instruction".into());
            }
            if func.inst(instruction).result_ty.is_some() != ty.is_some() {
                return Err("copied MIR result representation differs".into());
            }
            let mut kind = func.inst(instruction).kind.clone();
            debug = Some(func.inst(instruction).metadata.clone());
            let operands = operands
                .into_iter()
                .map(|value| operand(func, id, value, locals, types))
                .collect::<Result<Vec<_>, _>>()?;
            if operands.len() != kind.operands().len() {
                return Err("copied MIR operand count differs".into());
            }
            let mut operands = operands.into_iter();
            // old(operands) => old(replacement_operands)
            kind.visit_operands_mut(|value| *value = operands.next().unwrap());
            kind
        }
    };
    let mut inst = Instruction::new(kind, ty).with_debug_info_dropped();
    if !inst.can_replace_kind(&inst.kind) {
        return Err("invalid new MIR result representation".into());
    }
    if let Some(debug) = debug {
        inst.metadata.copy_debug_context(&debug);
    }
    // %local = operation(operands)
    Ok(match instruction.result {
        Some(local) => func.alloc_inst_with_result(inst, locals[&(id, local)]),
        None => func.alloc_inst(inst),
    })
}

fn build_terminator(
    func: &mut Function,
    id: FunctionId,
    term: api::MirTerminator,
    locals: &Locals,
    types: usize,
    functions: usize,
) -> Result<Terminator, String> {
    Ok(match term {
        api::MirTerminator::Jump { target } => Terminator::Jump(checked_block(func, target)?),
        api::MirTerminator::Branch { condition, then_block, else_block } => Terminator::Branch {
            condition: operand(func, id, condition, locals, types)?,
            then_block: checked_block(func, then_block)?,
            else_block: checked_block(func, else_block)?,
        },
        api::MirTerminator::Switch { value, default, cases } => Terminator::Switch {
            value: operand(func, id, value, locals, types)?,
            default: checked_block(func, default)?,
            cases: cases
                .into_iter()
                .map(|(value, block)| {
                    Ok((operand(func, id, value, locals, types)?, checked_block(func, block)?))
                })
                .collect::<Result<_, String>>()?,
        },
        api::MirTerminator::Return { values } => Terminator::Return {
            values: values
                .into_iter()
                .map(|value| operand(func, id, value, locals, types))
                .collect::<Result<_, _>>()?,
        },
        api::MirTerminator::ReturnData { offset, size } => Terminator::ReturnData {
            offset: operand(func, id, offset, locals, types)?,
            size: operand(func, id, size, locals, types)?,
        },
        api::MirTerminator::Revert { offset, size } => Terminator::Revert {
            offset: operand(func, id, offset, locals, types)?,
            size: operand(func, id, size, locals, types)?,
        },
        api::MirTerminator::TailCall { function, args } => {
            if function as usize >= functions {
                return Err("unknown MIR callee".into());
            }
            Terminator::TailCall {
                function: FunctionId::from_usize(function as usize),
                args: args
                    .into_iter()
                    .map(|value| operand(func, id, value, locals, types))
                    .collect::<Result<_, _>>()?,
            }
        }
        api::MirTerminator::Stop => Terminator::Stop,
        api::MirTerminator::Invalid => Terminator::Invalid,
    })
}
