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
//! cannot supply a pair and make this pass bail without changing the module.

use crate::{
    memory::EvmMemoryLayout,
    mir::{
        ArgIdx, FrameMode, FrameSlotKind, Function, FunctionBuilder, FunctionId, InstKind, MirType,
        Module, StructId, StructType, Terminator, Value, ValueId,
    },
    pass::{MirPass, ModuleAnalyses},
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

    fn run_pass(&self, _gcx: Gcx<'_>, module: &mut Module, _analyses: &mut ModuleAnalyses) -> bool {
        lower_structs(module)
    }
}

struct Layouts {
    types: IndexVec<StructId, StructType>,
    leaves: IndexVec<StructId, Box<[MirType]>>,
}

impl Layouts {
    fn new(module: &Module) -> Option<Self> {
        let mut layouts = Self { types: module.struct_types.clone(), leaves: IndexVec::new() };
        for (id, ty) in module.struct_types.iter_enumerated() {
            let mut leaves = Vec::new();
            for &field in &ty.fields {
                match field {
                    MirType::Struct(nested) if nested < id => {
                        leaves.extend_from_slice(&layouts.leaves[nested])
                    }
                    MirType::Slice(location) => {
                        leaves.push(super::lower_slices::slice_param_ptr_type(location));
                        leaves.push(MirType::uint256());
                    }
                    MirType::Struct(_) | MirType::Void => return None,
                    _ => leaves.push(field),
                }
            }
            layouts.leaves.push(leaves.into_boxed_slice());
        }
        Some(layouts)
    }

    fn flatten(&self, ty: MirType) -> Vec<MirType> {
        match ty {
            MirType::Struct(id) => self.leaves[id].to_vec(),
            _ => vec![ty],
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

fn lower_structs(module: &mut Module) -> bool {
    if !module.has_struct_values() {
        return false;
    }
    if module.functions.iter().any(|func| {
        func.arg_indices().any(|index| {
            index.index() >= func.params.len() && matches!(func.arg_ty(index), MirType::Struct(_))
        })
    }) {
        return false;
    }
    if module.functions.iter().any(|func| {
        func.returns.len() > 1 && func.returns.iter().any(|ty| matches!(ty, MirType::Struct(_)))
    }) {
        return false;
    }
    if module
        .struct_types
        .iter()
        .any(|ty| ty.fields.iter().any(|ty| matches!(ty, MirType::Slice(_))))
        && !slice_values_are_pairs(module)
    {
        return false;
    }
    let Some(layouts) = Layouts::new(module) else { return false };
    let mut shifts = IndexVec::new();
    for func in &module.functions {
        let slots =
            func.params.iter().chain(&func.returns).map(|&ty| layouts.flatten(ty).len()).sum();
        let Some(offsets) = super::utils::rebase_frame_offsets(func, slots) else { return false };
        shifts.push(offsets);
    }
    for (id, func) in module.functions.iter_mut_enumerated() {
        // unreachable aggregate definitions -> removed blocks and phi inputs
        let _ = super::cfg_simplify::remove_unreachable_blocks(func);
        lower_function(func, &layouts);
        // frame_addr(old_local_base + offset) -> frame_addr(new_local_base + offset)
        for &(inst, offset) in &shifts[id] {
            func.inst_mut(inst).kind = InstKind::InternalFrameAddr(offset);
        }
    }
    true
}

/// Checks the slice representation contract before exposing a field's two words.
fn slice_values_are_pairs(module: &Module) -> bool {
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
            InstKind::ICall { function, args, .. } => args_match(*function, args),
            _ => true,
        });
        let returns_match = func.blocks.iter().all(|block| match &block.terminator {
            Some(Terminator::Return { values }) => {
                func.returns.iter().enumerate().all(|(index, &ty)| {
                    !matches!(ty, MirType::Slice(_))
                        || values.get(index).is_some_and(|&value| func.value_ty(value) == Some(ty))
                })
            }
            Some(Terminator::TailCall { function, args }) => {
                args_match(*function, args)
                    && module.functions.get(*function).is_some_and(|callee| {
                        !callee.blocks.iter().any(|block| {
                            matches!(block.terminator, Some(Terminator::Return { .. }))
                        }) || func.returns.iter().enumerate().all(|(index, &ty)| {
                            !matches!(ty, MirType::Slice(_))
                                || callee.returns.get(index) == Some(&ty)
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
                    InstKind::ExtractValue { ty: aggregate, index, .. } => {
                        module.struct_types[*aggregate].fields[*index as usize] == ty
                    }
                    InstKind::Phi(incoming) => {
                        incoming.iter().all(|&(_, value)| func.value_ty(value) == Some(ty))
                    }
                    InstKind::Select(_, a, b) => {
                        [*a, *b].iter().all(|&value| func.value_ty(value) == Some(ty))
                    }
                    InstKind::ICall { function, .. } => module
                        .functions
                        .get(*function)
                        .is_some_and(|callee| callee.returns.first() == Some(&ty)),
                    _ => false,
                }
            })
    })
}

fn components(value: ValueId, aggregates: &FxHashMap<ValueId, Box<[ValueId]>>) -> Vec<ValueId> {
    aggregates.get(&value).map_or_else(|| vec![value], |values| values.to_vec())
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
        params.extend(layouts.flatten(ty));
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
    func.returns = func.returns.iter().flat_map(|&ty| layouts.flatten(ty)).collect();

    for block in func.blocks.indices() {
        let instructions = std::mem::take(&mut func.blocks[block].instructions);
        let mut builder = FunctionBuilder::new(func);
        builder.switch_to_block(block);
        for id in instructions {
            let inst = builder.func().inst(id).clone();
            builder.replace_source_span(inst.metadata.source_span().unwrap_or_default());
            builder.replace_modifier_depth(inst.metadata.modifier_depth());
            let result = builder.func().inst_result_value(id);
            let fields = result.and_then(|value| aggregates.get(&value));
            let outputs = match inst.kind {
                // insert_value aggregate, index, value -> replace the selected leaf range
                InstKind::InsertValue { ty, aggregate, index, value } => {
                    let mut leaves = components(aggregate, &aggregates);
                    let inserted =
                        if matches!(layouts.types[ty].fields[index as usize], MirType::Slice(_)) {
                            // slice -> slice_ptr(slice), slice_len(slice)
                            vec![builder.slice_ptr(value), builder.slice_len(value)]
                        } else {
                            components(value, &aggregates)
                        };
                    leaves.splice(layouts.field_range(ty, index), inserted);
                    leaves
                }
                // extract_value aggregate, index -> selected leaf range
                InstKind::ExtractValue { ty, aggregate, index } => {
                    let leaves =
                        components(aggregate, &aggregates)[layouts.field_range(ty, index)].to_vec();
                    if let MirType::Slice(location) = layouts.types[ty].fields[index as usize] {
                        // slice = make_slice(field.pointer, field.length)
                        vec![builder.make_slice(leaves[0], leaves[1], location)]
                    } else {
                        leaves
                    }
                }
                // phi struct [pred: value] -> phi field0 [pred: value.field0], ...
                InstKind::Phi(incoming) if fields.is_some() => {
                    let ty = inst.result_ty.unwrap();
                    layouts
                        .flatten(ty)
                        .into_iter()
                        .enumerate()
                        .map(|(index, ty)| {
                            let incoming = incoming
                                .iter()
                                .map(|&(pred, value)| (pred, components(value, &aggregates)[index]))
                                .collect();
                            builder.emit_inst(InstKind::Phi(incoming), Some(ty))
                        })
                        .collect()
                }
                // select cond, a, b -> select cond, a.field0, b.field0; ...
                InstKind::Select(cond, a, b) if fields.is_some() => {
                    let a = components(a, &aggregates);
                    let b = components(b, &aggregates);
                    layouts
                        .flatten(inst.result_ty.unwrap())
                        .into_iter()
                        .enumerate()
                        .map(|(index, ty)| {
                            builder.emit_inst(InstKind::Select(cond, a[index], b[index]), Some(ty))
                        })
                        .collect()
                }
                // first = icall callee, args.fields
                // buffer = frame_load multi_return
                // rest = mload(buffer + field_offset)
                InstKind::ICall { function, args, .. } => {
                    let args =
                        args.iter().flat_map(|&value| components(value, &aggregates)).collect();
                    if let Some(fields) = fields {
                        let types = layouts.flatten(inst.result_ty.unwrap());
                        if types.is_empty() {
                            builder.icall_void(function, args);
                            Vec::new()
                        } else {
                            let first = builder.icall(function, args, types[0]);
                            let mut values = vec![first];
                            if fields.len() > 1 {
                                let base = builder.frame_load(
                                    0,
                                    FrameMode::MultiReturn,
                                    FrameSlotKind::Word,
                                );
                                for (index, &ty) in types.iter().enumerate().skip(1) {
                                    let offset = builder.add_u64_offset(
                                        base,
                                        index as u64 * EvmMemoryLayout::WORD_SIZE,
                                    );
                                    values
                                        .push(builder.emit_inst(InstKind::MLoad(offset), Some(ty)));
                                }
                            }
                            values
                        }
                    } else {
                        builder.func_mut().inst_mut(id).kind =
                            InstKind::ICall { function, args: args.into() };
                        builder.func_mut().blocks[block].instructions.push(id);
                        continue;
                    }
                }
                _ => {
                    builder.func_mut().blocks[block].instructions.push(id);
                    continue;
                }
            };
            if let Some(fields) = fields {
                replacements.extend(fields.iter().copied().zip(outputs));
            } else if let Some(result) = result {
                replacements.insert(result, outputs[0]);
            }
        }
        // return aggregate -> return field0, ..., fieldN
        // tail_call callee, aggregate -> tail_call callee, field0, ..., fieldN
        match builder.func_mut().blocks[block].terminator.as_mut() {
            Some(Terminator::Return { values }) => {
                *values = values.iter().flat_map(|&value| components(value, &aggregates)).collect();
            }
            Some(Terminator::TailCall { args, .. }) => {
                *args = args.iter().flat_map(|&value| components(value, &aggregates)).collect();
            }
            _ => {}
        }
    }
    func.replace_uses_canonicalized(&replacements);
}
