//! Lower fixed SSA structs into their scalar fields at the calling-convention boundary.
//!
//! Fields retain declaration order through parameters, returns, calls, and control-flow merges.
//! Every aggregate value receives leaf placeholders before rewriting instructions, so cyclic
//! phis can refer to backedge definitions without depending on block traversal order. Unreachable
//! blocks are removed first: their definitions need not obey SSA and can form substitution cycles.
//! Insertions and projections become value substitutions; phis and selects become one instruction
//! per leaf. Internal calls publish their scalar results through the backend return convention only
//! here. All tail results are read immediately after their call, before another call can overwrite
//! them.
//!
//! This pass belongs before frame and memory-object lowering. Frame rebasing is checked for the
//! whole module before changing signatures. Slice fields expand directly to pointer/length words;
//! standalone slice signatures remain for slice lowering. Before expanding a slice field, check
//! that slice producers and returns agree with their types. Legacy pointer-only pseudo-slices
//! cannot supply a pair. Failed preflight checks report their reason without changing the module.

use super::utils::expand_block;
use crate::mir::{
    ArgIdx, Callee, FrameMode, FrameSlotKind, Function, FunctionBuilder, FunctionId, InstKind,
    MirType, Module, PtrLocation, StructId, StructType, Terminator, Value, ValueId,
    memory::EvmMemoryLayout,
    pass::{MirPass, ModuleAnalyses, run_lowering_function_pass},
};
use solar_data_structures::{index::IndexVec, map::FxHashMap};
use solar_sema::Gcx;

/// Expands fixed aggregate values into the scalar calling convention.
pub(crate) struct LowerStructs;

impl MirPass for LowerStructs {
    fn name(&self) -> &'static str {
        "lower-structs"
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module, analyses: &mut ModuleAnalyses) -> bool {
        match lower_structs(module, analyses) {
            Ok(changed) => changed,
            Err(reason) => {
                analyses.fail(
                    gcx.dcx()
                        .err(format!("`lower-structs` cannot lower this module: {reason}"))
                        .emit(),
                );
                false
            }
        }
    }
}

struct Layouts {
    types: IndexVec<StructId, StructType>,
    leaves: IndexVec<StructId, Box<[MirType]>>,
}

impl Layouts {
    fn new(module: &Module) -> Option<Self> {
        let mut layouts = Self { types: module.struct_types.clone(), leaves: IndexVec::new() };
        let mut leaves = Vec::new();
        for (id, ty) in module.struct_types.iter_enumerated() {
            leaves.clear();
            for &field in &ty.fields {
                match field {
                    MirType::Struct(nested) if nested < id => {
                        leaves.extend_from_slice(&layouts.leaves[nested])
                    }
                    MirType::Slice(_) => leaves.extend([MirType::I256; 2]),
                    MirType::Struct(_) | MirType::Void => return None,
                    _ => leaves.push(field),
                }
            }
            layouts.leaves.push(leaves.as_slice().into());
        }
        Some(layouts)
    }

    fn flatten<'a>(&'a self, ty: &'a MirType) -> &'a [MirType] {
        match *ty {
            MirType::Struct(id) => &self.leaves[id],
            _ => std::slice::from_ref(ty),
        }
    }

    fn field_range(&self, ty: StructId, index: u32) -> std::ops::Range<usize> {
        let fields = &self.types[ty].fields;
        let width = |ty| match ty {
            MirType::Struct(id) => self.leaves[id].len(),
            MirType::Slice(_) => 2,
            _ => 1,
        };
        let start = fields[..index as usize].iter().map(|&ty| width(ty)).sum::<usize>();
        start..start + width(fields[index as usize])
    }
}

fn lower_structs(module: &mut Module, analyses: &mut ModuleAnalyses) -> Result<bool, &'static str> {
    if !module.has_struct_values() {
        return Ok(false);
    }
    if module.functions.iter().any(|func| {
        func.arg_indices().any(|index| {
            index.index() >= func.params.len() && matches!(func.arg_ty(index), MirType::Struct(_))
        })
    }) {
        return Err("aggregate argument is absent from the function signature");
    }
    if module.functions.iter().any(|func| {
        func.return_components().len() > 1
            && func.return_components().iter().any(|ty| matches!(ty, MirType::Struct(_)))
    }) {
        return Err("aggregate result is mixed with an expanded return ABI");
    }
    if module
        .struct_types
        .iter()
        .any(|ty| ty.fields.iter().any(|ty| matches!(ty, MirType::Slice(_))))
        && !slice_values_are_pairs(module)
    {
        return Err("slice producers or returns do not provide pointer/length pairs");
    }
    let layouts =
        Layouts::new(module).ok_or("struct fields contain void or a forward type reference")?;
    let mut shifts = IndexVec::new();
    for func in &module.functions {
        let slots = func
            .params
            .iter()
            .chain(func.return_components())
            .map(|ty| layouts.flatten(ty).len())
            .sum();
        let offsets = super::utils::rebase_frame_offsets(func, slots)
            .ok_or("local frame offsets cannot be rebased for the expanded signature")?;
        shifts.push(offsets);
    }
    Ok(run_lowering_function_pass(module, analyses, |func, function_analyses| {
        // unreachable aggregate definitions -> removed blocks and phi inputs
        let _ = super::cfg_simplify::remove_unreachable_blocks(func);
        lower_function(func, &layouts);
        // frame_addr(old_local_base + offset) -> frame_addr(new_local_base + offset)
        for &(inst, offset) in &shifts[function_analyses.function()] {
            func.inst_mut(inst).kind = InstKind::InternalFrameAddr(offset);
        }
        true
    }))
}

/// Checks the slice representation contract before exposing a field's two words.
fn slice_values_are_pairs(module: &Module) -> bool {
    let returning = module.returning_functions();
    module.functions.iter().all(|func| {
        let args_match = |function: FunctionId, args: &[ValueId]| {
            module.functions.get(function).is_some_and(|callee| {
                callee.params.iter().enumerate().all(|(index, &ty)| {
                    !matches!(ty, MirType::Slice(_))
                        || args.get(index).is_some_and(|&value| func.value_ty(value) == Some(ty))
                })
            })
        };
        let calls_match = func.instructions().all(|id| match &func.inst(id).kind {
            InstKind::ICall { function: Callee::Function(function), args, .. } => {
                args_match(*function, args)
            }
            _ => true,
        });
        let returns_match = func.blocks.iter().all(|block| match &block.terminator {
            Some(Terminator::Return { values }) => {
                func.return_components().iter().enumerate().all(|(index, &ty)| {
                    !matches!(ty, MirType::Slice(_))
                        || values.get(index).is_some_and(|&value| func.value_ty(value) == Some(ty))
                })
            }
            Some(Terminator::TailCall { function, args }) => {
                args_match(*function, args)
                    && module.functions.get(*function).is_some_and(|callee| {
                        !returning.contains(*function)
                            || func.return_components().iter().enumerate().all(|(index, &ty)| {
                                !matches!(ty, MirType::Slice(_))
                                    || callee.return_components().get(index) == Some(&ty)
                            })
                    })
            }
            _ => true,
        });
        calls_match
            && returns_match
            && func.live_values().all(|value| {
                let Some(ty @ MirType::Slice(location)) = func.value_ty(value) else { return true };
                let Value::Inst(id) = *func.value(value) else { return true };
                match &func.inst(id).kind {
                    InstKind::MakeSlice { location: actual, .. } => *actual == location,
                    InstKind::FrameLoad { kind, .. } => kind.result_type() == ty,
                    InstKind::AbiEncode { mode, .. } => mode.result_type() == ty,
                    InstKind::ExtractValue { ty: MirType::Struct(aggregate), index, .. } => {
                        module.struct_types[*aggregate].fields[*index as usize] == ty
                    }
                    InstKind::Phi(incoming) => {
                        incoming.iter().all(|&(_, value)| func.value_ty(value) == Some(ty))
                    }
                    InstKind::Select(_, a, b) => {
                        [*a, *b].iter().all(|&value| func.value_ty(value) == Some(ty))
                    }
                    InstKind::ICall { function: Callee::Function(function), .. } => module
                        .functions
                        .get(*function)
                        .is_some_and(|callee| callee.return_components().first() == Some(&ty)),
                    _ => false,
                }
            })
    })
}

fn components<'a>(
    value: &'a ValueId,
    aggregates: &'a FxHashMap<ValueId, Box<[ValueId]>>,
) -> &'a [ValueId] {
    aggregates.get(value).map_or_else(|| std::slice::from_ref(value), |values| values)
}

fn flatten(values: &[ValueId], aggregates: &FxHashMap<ValueId, Box<[ValueId]>>) -> Vec<ValueId> {
    values.iter().flat_map(|value| components(value, aggregates).iter().copied()).collect()
}

fn lower_function(func: &mut Function, layouts: &Layouts) {
    let mut aggregates = FxHashMap::default();
    let mut replacements = FxHashMap::default();
    let mut values = func.live_values().collect::<Vec<_>>();
    values.sort_unstable();
    values.dedup();
    for &value in &values {
        if let Some(MirType::Struct(ty)) = func.value_ty(value) {
            let fields = layouts.leaves[ty]
                .iter()
                .map(|&ty| func.alloc_value(Value::Undef(ty)))
                .collect::<Box<[_]>>();
            aggregates.insert(value, fields);
        }
    }
    let mut params = IndexVec::new();
    let mut starts = IndexVec::<ArgIdx, ArgIdx>::new();
    for &ty in &func.params {
        starts.push(params.next_idx());
        params.extend(layouts.flatten(&ty).iter().copied());
    }
    if func.params.iter().any(|ty| matches!(ty, MirType::Struct(_))) {
        func.set_params(params);
    }
    // aggregate_arg -> (field_arg0, ..., field_argN)
    for value in values {
        if let Value::Arg(index) = *func.value(value)
            && let Some(&start) = starts.get(index)
        {
            if let Some(fields) = aggregates.get(&value) {
                for (offset, &field) in fields.iter().enumerate() {
                    let arg = func.alloc_arg(ArgIdx::new(start.index() + offset));
                    replacements.insert(field, arg);
                }
            } else {
                *func.value_mut(value) = Value::Arg(start);
            }
        }
    }
    // result: aggregate -> result: aggregate [return_abi = scalar leaves]
    func.set_return_abi(
        func.return_components()
            .iter()
            .flat_map(|ty| layouts.flatten(ty).iter().copied())
            .collect::<Vec<_>>(),
    );

    // The leaves each instruction expands to, reused across instructions.
    let mut outputs = Vec::new();
    for block in func.blocks.indices() {
        let mut builder = FunctionBuilder::new(func);
        expand_block(&mut builder, block, |builder, id| {
            let result = builder.func().inst_result_value(id);
            let fields = result.and_then(|value| aggregates.get(&value));
            let result_ty = builder.func().inst(id).result_ty;
            let metadata = builder.func().inst(id).metadata.debug_context();
            builder.set_debug_context(&metadata);
            outputs.clear();
            match &builder.func().inst(id).kind {
                // NOTE: Scalar insertions and projections emit no instruction. Their source
                // checkpoints disappear with them; do not attach them to unrelated definitions.
                // insert_value aggregate, index, value -> replace the selected leaf range
                &InstKind::InsertValue { ty: MirType::Struct(ty), aggregate, index, value } => {
                    outputs.extend_from_slice(components(&aggregate, &aggregates));
                    let range = layouts.field_range(ty, index);
                    if matches!(layouts.types[ty].fields[index as usize], MirType::Slice(_)) {
                        // slice -> ptrtoint (extract_value slice, 0), extract_value slice, 1
                        let pointer = builder.slice_ptr(value);
                        let pointer = builder.cast(pointer, MirType::I256);
                        outputs.splice(range, [pointer, builder.slice_len(value)]);
                    } else {
                        outputs.splice(range, components(&value, &aggregates).iter().copied());
                    }
                }
                // extract_value aggregate, index -> selected leaf range
                &InstKind::ExtractValue { ty: MirType::Struct(ty), aggregate, index } => {
                    let leaves =
                        &components(&aggregate, &aggregates)[layouts.field_range(ty, index)];
                    if let MirType::Slice(location) = layouts.types[ty].fields[index as usize] {
                        // slice = make_slice(field.pointer, field.length)
                        outputs.push(builder.make_slice(leaves[0], leaves[1], location));
                    } else {
                        outputs.extend_from_slice(leaves);
                    }
                }
                // phi struct [pred: value] -> phi field0 [pred: value.field0], ...
                InstKind::Phi(incoming) if fields.is_some() => {
                    let incoming = incoming.clone();
                    for (index, &ty) in layouts.flatten(&result_ty.unwrap()).iter().enumerate() {
                        let incoming = incoming
                            .iter()
                            .map(|(pred, value)| (*pred, components(value, &aggregates)[index]))
                            .collect();
                        outputs.push(builder.emit_inst(InstKind::Phi(incoming), Some(ty)));
                    }
                }
                // select cond, a, b -> select cond, a.field0, b.field0; ...
                &InstKind::Select(cond, a, b) if fields.is_some() => {
                    let a = components(&a, &aggregates);
                    let b = components(&b, &aggregates);
                    for (index, &ty) in layouts.flatten(&result_ty.unwrap()).iter().enumerate() {
                        outputs.push(
                            builder.emit_inst(InstKind::Select(cond, a[index], b[index]), Some(ty)),
                        );
                    }
                }
                // first = icall callee, args.fields
                // buffer = frame_load multi_return
                // rest = mload(buffer + field_offset)
                InstKind::ICall { function: Callee::Function(function), args, .. } => {
                    let function = *function;
                    if fields.is_none() && !args.iter().any(|arg| aggregates.contains_key(arg)) {
                        return false;
                    }
                    let args = flatten(args, &aggregates);
                    let Some(fields) = fields else {
                        builder.func_mut().inst_mut(id).kind = InstKind::ICall {
                            function: Callee::Function(function),
                            args: args.into(),
                        };
                        return false;
                    };
                    let result_ty = result_ty.unwrap();
                    let types = layouts.flatten(&result_ty);
                    if types.is_empty() {
                        builder.icall_void(function, args);
                    } else {
                        outputs.push(builder.icall(function, args, types[0]));
                        if fields.len() > 1 {
                            let base =
                                builder.frame_load(0, FrameMode::MultiReturn, FrameSlotKind::Word);
                            for (index, &ty) in types.iter().enumerate().skip(1) {
                                let offset = builder
                                    .add_offset(base, index as u64 * EvmMemoryLayout::WORD_SIZE);
                                outputs.push(builder.emit_inst(
                                    InstKind::Load(offset, PtrLocation::Memory),
                                    Some(ty),
                                ));
                            }
                        }
                    }
                }
                _ => return false,
            }
            if let Some(fields) = fields {
                replacements.extend(fields.iter().copied().zip(outputs.iter().copied()));
            } else if let Some(result) = result {
                replacements.insert(result, outputs[0]);
            }
            true
        });
        // return aggregate -> return field0, ..., fieldN
        // tail_call callee, aggregate -> tail_call callee, field0, ..., fieldN
        match func.blocks[block].terminator.as_mut() {
            Some(Terminator::Return { values }) => *values = flatten(values, &aggregates).into(),
            Some(Terminator::TailCall { args, .. }) => *args = flatten(args, &aggregates).into(),
            _ => {}
        }
    }
    func.replace_uses_canonicalized(&replacements);
}
