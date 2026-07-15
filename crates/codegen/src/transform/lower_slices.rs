//! Lower logical MIR slices back to their component words.
//!
//! Slices are deliberately a higher-level MIR abstraction. The EVM backend
//! remains word-based, so this pass expands slice parameters and call
//! arguments, resolves `slice_ptr`/`slice_len` projections, and erases the
//! corresponding constructors before machine lowering.

use crate::{
    mir::{
        BlockId, Function, FunctionBuilder, FunctionId, InstId, InstKind, MirType, Module,
        SliceLocation, Value, ValueId,
    },
    pass::ModulePass,
};
use solar_data_structures::map::{FxHashMap, FxHashSet};
use solar_sema::Gcx;

/// Statistics from slice lowering.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LowerSlicesStats {
    /// Slice constructors erased.
    pub slices: usize,
    /// Pointer/length projections resolved.
    pub projections: usize,
    /// Logical slice parameters expanded into pointer/length words.
    pub params: usize,
    /// Logical call arguments expanded into pointer/length words.
    pub call_args: usize,
    /// External slice parameters projected back to ABI head reads.
    pub external_params: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ParamRepr {
    Word,
    CompactCalldata,
    Pair,
}

/// Lowers logical slices to the word-based backend convention.
#[derive(Debug, Default)]
pub struct LowerSlicesPass {
    stats: LowerSlicesStats,
}

impl LowerSlicesPass {
    /// Returns statistics for the most recent run.
    #[must_use]
    pub const fn stats(&self) -> &LowerSlicesStats {
        &self.stats
    }

    fn expand_call_args(
        &mut self,
        func: &mut Function,
        signatures: &FxHashMap<FunctionId, Vec<ParamRepr>>,
    ) -> bool {
        let mut changed = false;
        let block_ids: Vec<BlockId> = func.blocks.indices().collect();
        for block_id in block_ids {
            let instructions = std::mem::take(&mut func.blocks[block_id].instructions);
            let mut builder = FunctionBuilder::new(func);
            builder.switch_to_block(block_id);
            for inst_id in instructions {
                let call = match &builder.func().instructions[inst_id].kind {
                    InstKind::InternalCall { function, args, .. } => {
                        Some((*function, args.to_vec()))
                    }
                    _ => None,
                };
                if let Some((callee, args)) = call
                    && let Some(signature) = signatures.get(&callee)
                    && signature.iter().any(|repr| *repr != ParamRepr::Word)
                {
                    let mut expanded = Vec::with_capacity(args.len() + 1);
                    for (index, arg) in args.into_iter().enumerate() {
                        let repr = signature.get(index).copied().unwrap_or(ParamRepr::Word);
                        match repr {
                            ParamRepr::Word | ParamRepr::CompactCalldata => expanded.push(arg),
                            ParamRepr::Pair => {
                                expanded.push(builder.slice_ptr(arg));
                                expanded.push(builder.slice_len(arg));
                            }
                        }
                        self.stats.call_args += usize::from(repr != ParamRepr::Word);
                    }
                    let InstKind::InternalCall { args, .. } =
                        &mut builder.func_mut().instructions[inst_id].kind
                    else {
                        unreachable!()
                    };
                    *args = expanded.into();
                    changed = true;
                }
                builder.func_mut().blocks[block_id].instructions.push(inst_id);
            }
        }
        changed
    }

    fn lower_params(&mut self, func: &mut Function, signature: &[ParamRepr]) -> bool {
        if func.selector.is_some()
            || func.blocks.is_empty()
            || !func.params.iter().any(Self::is_slice)
        {
            return false;
        }

        let old_params = func.params.clone();
        let mut physical_indices = Vec::with_capacity(old_params.len());
        let mut next_index = 0u32;
        let mut new_params = Vec::with_capacity(old_params.len() + 1);
        for (index, &ty) in old_params.iter().enumerate() {
            physical_indices.push(next_index);
            match signature[index] {
                ParamRepr::Word => new_params.push(ty),
                ParamRepr::CompactCalldata => new_params.push(MirType::uint256()),
                ParamRepr::Pair => {
                    let MirType::Slice(location) = ty else { unreachable!() };
                    new_params.push(match location {
                        SliceLocation::Memory => MirType::MemPtr,
                        SliceLocation::Calldata => MirType::CalldataPtr,
                    });
                    new_params.push(MirType::uint256());
                    next_index += 1;
                }
            }
            next_index += 1;
        }

        for value in func.values.iter_mut() {
            if let Value::Arg { index, ty } = value
                && !matches!(ty, MirType::Slice(_))
            {
                *index = physical_indices[*index as usize];
            }
        }

        let slice_args: Vec<_> = func
            .values
            .iter_enumerated()
            .filter_map(|(value, kind)| match kind {
                Value::Arg { index, ty: MirType::Slice(location) } => {
                    Some((value, *index, *location))
                }
                _ => None,
            })
            .collect();
        let mut components = FxHashMap::default();
        let mut compact_heads = FxHashMap::default();
        let mut builder = FunctionBuilder::new(func);
        for (slice_arg, logical_index, location) in slice_args {
            let physical_index = physical_indices[logical_index as usize];
            match signature[logical_index as usize] {
                ParamRepr::CompactCalldata => {
                    let head = builder
                        .func_mut()
                        .alloc_value(Value::Arg { index: physical_index, ty: MirType::uint256() });
                    compact_heads.insert(slice_arg, head);
                }
                ParamRepr::Pair => {
                    let ptr_ty = match location {
                        SliceLocation::Memory => MirType::MemPtr,
                        SliceLocation::Calldata => MirType::CalldataPtr,
                    };
                    let ptr = builder
                        .func_mut()
                        .alloc_value(Value::Arg { index: physical_index, ty: ptr_ty });
                    let len = builder.func_mut().alloc_value(Value::Arg {
                        index: physical_index + 1,
                        ty: MirType::uint256(),
                    });
                    components.insert(slice_arg, (ptr, len));
                }
                ParamRepr::Word => unreachable!(),
            }
            self.stats.params += 1;
        }
        builder.func_mut().params = new_params;

        let inst_results = builder.func().inst_results();
        let mut replacements = FxHashMap::default();
        let mut removed = FxHashSet::default();
        for block in builder.func().blocks.iter() {
            for &inst_id in &block.instructions {
                let replacement = match builder.func().instructions[inst_id].kind {
                    InstKind::SlicePtr(slice) => components.get(&slice).map(|&(ptr, _)| ptr),
                    InstKind::SliceLen(slice) => components.get(&slice).map(|&(_, len)| len),
                    _ => None,
                };
                if let Some(replacement) = replacement
                    && let Some(&result) = inst_results.get(&inst_id)
                {
                    replacements.insert(result, replacement);
                    removed.insert(inst_id);
                    self.stats.projections += 1;
                }
            }
        }
        builder.func_mut().replace_uses_canonicalized(&replacements);
        for block in builder.func_mut().blocks.iter_mut() {
            block.instructions.retain(|inst| !removed.contains(inst));
        }
        if !compact_heads.is_empty() {
            self.lower_compact_values(builder.func_mut(), &compact_heads);
        }
        true
    }

    fn lower_compact_values(
        &mut self,
        func: &mut Function,
        raw_heads: &FxHashMap<ValueId, ValueId>,
    ) {
        let inst_results = func.inst_results();
        let mut replacements = raw_heads.clone();
        let block_ids: Vec<BlockId> = func.blocks.indices().collect();
        for block_id in block_ids {
            let instructions = std::mem::take(&mut func.blocks[block_id].instructions);
            let mut builder = FunctionBuilder::new(func);
            builder.switch_to_block(block_id);
            let mut lengths = FxHashMap::default();
            let mut pointers = FxHashMap::default();
            for inst_id in instructions {
                let projection = match builder.func().instructions[inst_id].kind {
                    InstKind::SlicePtr(slice) => raw_heads.get(&slice).map(|&head| (head, true)),
                    InstKind::SliceLen(slice) => raw_heads.get(&slice).map(|&head| (head, false)),
                    _ => None,
                };
                if let Some((head, is_ptr)) = projection {
                    let replacement = if is_ptr {
                        *pointers.entry(head).or_insert_with(|| {
                            let data_offset = builder.imm_u64(36);
                            builder.add(head, data_offset)
                        })
                    } else {
                        *lengths.entry(head).or_insert_with(|| {
                            let selector_size = builder.imm_u64(4);
                            let len_pos = builder.add(head, selector_size);
                            builder.calldataload(len_pos)
                        })
                    };
                    replacements.insert(inst_results[&inst_id], replacement);
                    self.stats.projections += 1;
                } else {
                    builder.func_mut().blocks[block_id].instructions.push(inst_id);
                }
            }
        }
        func.replace_uses_canonicalized(&replacements);
    }

    fn lower_external_args(&mut self, func: &mut Function) -> bool {
        if func.selector.is_none() || func.blocks.is_empty() {
            return false;
        }
        let slice_args: FxHashMap<_, _> = func
            .values
            .iter_enumerated()
            .filter_map(|(value, kind)| match kind {
                Value::Arg { index, ty: MirType::Slice(SliceLocation::Calldata) } => {
                    Some((value, *index))
                }
                _ => None,
            })
            .collect();
        if slice_args.is_empty() {
            return false;
        }

        let raw_heads: FxHashMap<_, _> = slice_args
            .iter()
            .map(|(&slice, &index)| {
                let head = func.alloc_value(Value::Arg { index, ty: MirType::uint256() });
                (slice, head)
            })
            .collect();
        self.lower_compact_values(func, &raw_heads);
        self.stats.external_params += slice_args.len();
        true
    }

    fn infer_compact_params(module: &Module) -> FxHashSet<(FunctionId, usize)> {
        let mut compact: FxHashSet<_> = module
            .functions
            .iter_enumerated()
            .flat_map(|(function, func)| {
                func.params.iter().enumerate().filter_map(move |(index, ty)| {
                    matches!(ty, MirType::Slice(SliceLocation::Calldata))
                        .then_some((function, index))
                })
            })
            .collect();

        loop {
            let mut removed = FxHashSet::default();
            let mut seen = FxHashSet::default();
            for (caller_id, caller) in module.functions.iter_enumerated() {
                for &inst_id in caller.blocks.iter().flat_map(|block| &block.instructions) {
                    let inst = &caller.instructions[inst_id];
                    let InstKind::InternalCall { function: callee, args, .. } = &inst.kind else {
                        continue;
                    };
                    for index in 0..module.function(*callee).params.len() {
                        let candidate = (*callee, index);
                        if !compact.contains(&candidate) {
                            continue;
                        }
                        seen.insert(candidate);
                        let Some(&arg) = args.get(index) else {
                            removed.insert(candidate);
                            continue;
                        };
                        let is_compact = match caller.value(arg) {
                            Value::Arg {
                                index: source,
                                ty: MirType::Slice(SliceLocation::Calldata),
                            } => {
                                caller.selector.is_some()
                                    || compact.contains(&(caller_id, *source as usize))
                            }
                            _ => false,
                        };
                        if !is_compact {
                            removed.insert(candidate);
                        }
                    }
                }
            }
            removed.extend(compact.iter().filter(|candidate| !seen.contains(candidate)).copied());
            if removed.is_empty() {
                return compact;
            }
            compact.retain(|candidate| !removed.contains(candidate));
        }
    }

    const fn is_slice(ty: &MirType) -> bool {
        matches!(ty, MirType::Slice(_))
    }

    fn lower_projections(&mut self, func: &mut Function) -> bool {
        let inst_results = func.inst_results();
        let live_insts: FxHashSet<_> =
            func.blocks.iter().flat_map(|block| block.instructions.iter().copied()).collect();
        let mut components = FxHashMap::<ValueId, (ValueId, ValueId, InstId)>::default();
        let mut projections = FxHashMap::<ValueId, (ValueId, InstId, bool)>::default();
        for (&inst, &result) in &inst_results {
            if !live_insts.contains(&inst) {
                continue;
            }
            match func.instructions[inst].kind {
                InstKind::MakeSlice { ptr, len, .. } => {
                    components.insert(result, (ptr, len, inst));
                }
                InstKind::SlicePtr(slice) => {
                    projections.insert(result, (slice, inst, true));
                }
                InstKind::SliceLen(slice) => {
                    projections.insert(result, (slice, inst, false));
                }
                _ => {}
            }
        }
        if components.is_empty() {
            return false;
        }

        // Aggregate uses need a future explicit lowering rule. Keep those
        // slices intact instead of guessing at a one-word representation.
        let mut removable: FxHashSet<ValueId> = components.keys().copied().collect();
        for inst_id in &live_insts {
            let inst = &func.instructions[*inst_id];
            for operand in inst.kind.operands() {
                if components.contains_key(&operand)
                    && !matches!(inst.kind, InstKind::SlicePtr(v) | InstKind::SliceLen(v) if v == operand)
                {
                    removable.remove(&operand);
                }
            }
        }
        for block in func.blocks.iter() {
            if let Some(term) = &block.terminator {
                for operand in term.operands() {
                    removable.remove(&operand);
                }
            }
        }
        if removable.is_empty() {
            return false;
        }

        let mut replacements = FxHashMap::default();
        let mut removed = FxHashSet::default();
        for (&slice, &(ptr, len, constructor)) in &components {
            if removable.contains(&slice) {
                removed.insert(constructor);
                self.stats.slices += 1;
                for (&result, &(projected_slice, inst, is_ptr)) in &projections {
                    if projected_slice == slice {
                        replacements.insert(result, if is_ptr { ptr } else { len });
                        removed.insert(inst);
                        self.stats.projections += 1;
                    }
                }
            }
        }

        func.replace_uses_canonicalized(&replacements);
        for block in func.blocks.iter_mut() {
            block.instructions.retain(|inst| !removed.contains(inst));
        }
        true
    }
}

impl ModulePass for LowerSlicesPass {
    fn run(&mut self, _gcx: Gcx<'_>, module: &mut Module) -> bool {
        self.stats = LowerSlicesStats::default();
        let compact = Self::infer_compact_params(module);
        let signatures: FxHashMap<_, _> = module
            .functions
            .iter_enumerated()
            .map(|(id, func)| {
                let signature = func
                    .params
                    .iter()
                    .enumerate()
                    .map(|(index, ty)| match ty {
                        MirType::Slice(SliceLocation::Calldata)
                            if compact.contains(&(id, index)) =>
                        {
                            ParamRepr::CompactCalldata
                        }
                        MirType::Slice(_) => ParamRepr::Pair,
                        _ => ParamRepr::Word,
                    })
                    .collect();
                (id, signature)
            })
            .collect();
        let mut changed = false;
        for func in module.functions.iter_mut() {
            changed |= self.expand_call_args(func, &signatures);
        }
        let function_ids: Vec<_> = module.functions.indices().collect();
        for id in function_ids {
            let func = module.function_mut(id);
            changed |= self.lower_external_args(func);
            changed |= self.lower_params(func, &signatures[&id]);
            changed |= self.lower_projections(func);
        }
        changed
    }
}
