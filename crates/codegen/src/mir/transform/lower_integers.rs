//! Legalize integer bit patterns to the EVM word representation inside MIR.
//!
//! Semantic arithmetic wraps at its declared width, and signed operations use
//! that width's sign bit. This pass widens integer carriers after ABI, aggregate,
//! and memory conversion, materializing masks and sign extension as ordinary MIR
//! operations. The following scalar passes can combine and eliminate them before
//! stack scheduling. Booleans retain i1 so branches still require a condition;
//! pointers retain their distinct types and explicit pointer conversions.
//!
//! Sign extension uses SIGNEXTEND for byte widths and a left/arithmetic-right
//! shift pair for other widths. All signatures and value types change together,
//! preserving SSA identities across calls and cyclic phis. No ABI layout changes:
//! narrow argument bit patterns are already clean at this internal boundary.

use crate::mir::{
    Function, FunctionBuilder, Immediate, InstKind, MirType, Module, ResultKind, Value, ValueId,
    pass::{MirPass, ModuleAnalyses},
};
use alloy_primitives::U256;

pub(crate) struct LowerIntegers;

impl MirPass for LowerIntegers {
    fn name(&self) -> &'static str {
        "lower-integers"
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut ModuleAnalyses,
    ) -> bool {
        let unsupported = |ty: MirType| ty.integer_bits().is_some_and(|bits| bits > 256);
        if module.struct_types.iter().any(|ty| ty.fields.iter().copied().any(unsupported))
            || module.functions.iter().any(|func| {
                unsupported(func.return_type())
                    || func.return_components().iter().copied().any(unsupported)
                    || (0..func.num_values()).any(|index| {
                        func.value_ty(ValueId::from_usize(index)).is_some_and(unsupported)
                    })
            })
        {
            analyses.fail(
                gcx.dcx().err("integer lowering supports widths from i1 through i256").emit(),
            );
            return false;
        }
        let mut changed = false;
        for ty in &mut module.struct_types {
            for field in &mut ty.fields {
                let lowered = lower_type(*field);
                changed |= lowered != *field;
                *field = lowered;
            }
        }
        for func in &mut module.functions {
            changed |= lower_function(func);
        }
        changed
    }
}

fn lower_type(ty: MirType) -> MirType {
    match ty {
        MirType::Int(bits) if (2..256).contains(&bits.get()) => MirType::I256,
        _ => ty,
    }
}

fn lower_function(func: &mut Function) -> bool {
    let types = (0..func.num_values())
        .map(|index| func.value_ty(ValueId::from_usize(index)))
        .collect::<Vec<_>>();
    let instructions = func.instructions().collect::<Vec<_>>();
    let mut changed = false;
    let returns = func.return_components().iter().copied().map(lower_type).collect::<Vec<_>>();
    let result = lower_type(func.return_type());
    changed |= result != func.return_type() || returns != func.return_components();
    func.set_return_type(result);
    func.set_return_abi(returns.into_boxed_slice());
    for index in func.arg_indices() {
        let ty = lower_type(func.arg_ty(index));
        changed |= ty != func.arg_ty(index);
        func.set_arg_ty(index, ty);
    }
    for (index, ty) in types.iter().enumerate() {
        if let Some(ty) = *ty
            && lower_type(ty) != ty
        {
            changed = true;
            let value = func.value_mut(ValueId::from_usize(index));
            match value {
                Value::Immediate(immediate) => {
                    *immediate =
                        Immediate::for_type(Some(MirType::I256), immediate.as_u256().unwrap());
                }
                Value::Undef(ty) => *ty = MirType::I256,
                _ => {}
            }
        }
    }
    for &id in &instructions {
        let inst = func.inst_mut(id);
        inst.result_ty = inst.result_ty.map(lower_type);
    }
    // Keep original types while rewriting; earlier producers now have word results.
    for block in func.blocks.indices() {
        let instructions = std::mem::take(&mut func.blocks[block].instructions);
        let mut builder = FunctionBuilder::new(func);
        builder.switch_to_block(block);
        for id in instructions {
            let bits = |value: ValueId| {
                types[value.index()].and_then(MirType::integer_bits).unwrap_or(256)
            };
            let inst = builder.func().inst(id);
            let conversion = matches!(
                inst.kind,
                InstKind::Trunc(..)
                    | InstKind::Sext(..)
                    | InstKind::Zext(..)
                    | InstKind::PtrToInt(..)
            );
            let narrow_scalar = (inst.kind.op_def().result == ResultKind::Integer
                || matches!(inst.kind, InstKind::SLt(..) | InstKind::SGt(..)))
                && inst.kind.operands().first().is_some_and(|&value| bits(value) < 256);
            if !conversion && !narrow_scalar {
                builder.func_mut().blocks[block].instructions.push(id);
                continue;
            }
            let inst = inst.clone();
            builder.set_debug_context(&inst.metadata.debug_context());
            let value = match inst.kind {
                InstKind::Trunc(value, width) => Some(clean(&mut builder, value, width)),
                InstKind::Sext(value, from, to) => {
                    let value = signed(&mut builder, value, from);
                    Some(clean(&mut builder, value, to))
                }
                InstKind::Zext(value)
                    if builder.func().value_ty(value) == builder.func().inst(id).result_ty =>
                {
                    Some(value)
                }
                InstKind::PtrToInt(value, width) if width < 256 => {
                    let value = builder.cast_word(value);
                    Some(clean(&mut builder, value, width))
                }
                InstKind::And(..) | InstKind::Or(..) | InstKind::Xor(..)
                    if inst.result_ty == Some(MirType::I1) =>
                {
                    None
                }
                ref kind
                    if kind.op_def().result == ResultKind::Integer
                        && kind.operands().first().is_some_and(|&value| bits(value) < 256) =>
                {
                    let operands = kind.operands();
                    let width = bits(operands[0]);
                    let mut kind = kind.clone();
                    let signed_op =
                        matches!(kind, InstKind::SDiv(..) | InstKind::SMod(..) | InstKind::Sar(..));
                    let arithmetic_shift = matches!(kind, InstKind::Sar(..));
                    let mut index = 0;
                    kind.visit_operands_mut(|value| {
                        *value = if signed_op && (!arithmetic_shift || index != 0) {
                            signed(&mut builder, *value, width)
                        } else {
                            builder.cast_word(*value)
                        };
                        index += 1;
                    });
                    let clz = matches!(kind, InstKind::Clz(..));
                    let mut value = builder.emit_inst(kind, Some(MirType::I256));
                    if clz {
                        let padding = builder.imm(256 - width);
                        value = builder.sub(value, padding);
                    }
                    Some(clean(&mut builder, value, width))
                }
                InstKind::SLt(a, b) | InstKind::SGt(a, b) if bits(a) < 256 => {
                    let a = signed(&mut builder, a, bits(a));
                    let b = signed(&mut builder, b, bits(b));
                    Some(if matches!(inst.kind, InstKind::SLt(..)) {
                        builder.slt(a, b)
                    } else {
                        builder.sgt(a, b)
                    })
                }
                _ => None,
            };
            if let Some(value) = value {
                // Preserve the result identity, including uses in backedge phis.
                builder.func_mut().inst_mut(id).replace_kind(InstKind::Bitcast(value));
                changed = true;
            }
            builder.func_mut().blocks[block].instructions.push(id);
        }
    }
    changed
}

fn clean(builder: &mut FunctionBuilder<'_>, value: ValueId, bits: u32) -> ValueId {
    let value = builder.cast_word(value);
    if bits == 256 {
        return value;
    }
    let mask = builder.imm(U256::MAX >> (256 - bits));
    let value = builder.and(value, mask);
    if bits == 1 {
        let zero = builder.imm(0);
        builder.ne(value, zero)
    } else {
        value
    }
}

fn signed(builder: &mut FunctionBuilder<'_>, value: ValueId, bits: u32) -> ValueId {
    let value = builder.cast_word(value);
    if bits == 256 {
        value
    } else if bits == 1 {
        let zero = builder.imm(0);
        builder.sub(zero, value)
    } else if bits.is_multiple_of(8) {
        let byte = builder.imm(bits / 8 - 1);
        builder.signextend(byte, value)
    } else {
        let shift = builder.imm(256 - bits);
        let value = builder.shl(shift, value);
        builder.sar(shift, value)
    }
}
