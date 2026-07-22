//! Lower logical MIR slices back to their component words.
//!
//! Slices are deliberately a higher-level MIR abstraction. The EVM backend
//! remains word-based, so this pass expands slice parameters and call
//! arguments, resolves `slice_ptr`/`slice_len` projections, and erases the
//! corresponding constructors before machine lowering.

use crate::{
    mir::{
        BlockId, Function, FunctionBuilder, FunctionId, InstId, InstKind, Instruction, MirType,
        Module, SliceLocation, Value, ValueId,
    },
    pass::ModulePass,
};
use solar_data_structures::map::{FxHashMap, FxHashSet};
use solar_sema::Gcx;

/// Statistics from slice lowering.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LowerSlicesStats {
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
pub(crate) struct LowerSlicesPass {
    stats: LowerSlicesStats,
}

/// The slice location of a value, if it is slice-typed. A `select`/`phi`
/// result is always word-typed by construction, so slice aggregate uses are
/// recognized from their operands rather than the result type.
fn value_slice_location(func: &Function, value: ValueId) -> Option<SliceLocation> {
    let ty = match func.value(value) {
        Value::Arg { ty, .. } | Value::Undef(ty) => Some(*ty),
        Value::Inst(inst) => func.instructions[*inst].result_ty,
        Value::Immediate(_) | Value::Error(_) => None,
    };
    match ty {
        Some(MirType::Slice(location)) => Some(location),
        _ => None,
    }
}

/// The pointer type for a slice parameter's leading word. Only calldata and
/// memory slices are ever parameters; returndata is a volatile in-body buffer
/// and never reaches a function signature.
fn slice_param_ptr_type(location: SliceLocation) -> MirType {
    match location {
        SliceLocation::Memory => MirType::MemPtr,
        SliceLocation::Calldata => MirType::CalldataPtr,
        SliceLocation::Returndata => unreachable!("returndata slices are never parameters"),
    }
}

/// Allocates a word-typed instruction and its result value, returning both.
fn new_word_inst(func: &mut Function, kind: InstKind) -> (InstId, ValueId) {
    let inst = func.alloc_inst(Instruction::new(kind, Some(MirType::uint256())));
    let value = func.alloc_value(Value::Inst(inst));
    (inst, value)
}

/// Allocates a `make_slice` instruction and its slice-typed result value.
fn new_slice_inst(
    func: &mut Function,
    ptr: ValueId,
    len: ValueId,
    location: SliceLocation,
) -> (InstId, ValueId) {
    let inst = func.alloc_inst(Instruction::new(
        InstKind::MakeSlice { ptr, len, location },
        Some(MirType::Slice(location)),
    ));
    let value = func.alloc_value(Value::Inst(inst));
    (inst, value)
}

impl LowerSlicesPass {
    /// Rewrites slice-typed `select` and `phi` into paired pointer/length
    /// operations over a `make_slice`, so no two-word slice value survives an
    /// aggregate use. Each operand slice is then consumed only by projections
    /// and folds away in `lower_projections`.
    fn split_slice_aggregates(&mut self, func: &mut Function) -> bool {
        let mut changed = false;
        let mut replacements = FxHashMap::default();

        // Selects: rewrite in place within their block.
        let block_ids: Vec<BlockId> = func.blocks.indices().collect();
        for block_id in &block_ids {
            let insts = std::mem::take(&mut func.blocks[*block_id].instructions);
            let mut out = Vec::with_capacity(insts.len());
            for inst_id in insts {
                if let InstKind::Select(cond, a, b) = func.instructions[inst_id].kind
                    && let Some(location) = value_slice_location(func, a)
                {
                    let old = func.inst_result_value(inst_id).expect("select has a result");
                    let (ia, pa) = new_word_inst(func, InstKind::SlicePtr(a));
                    let (ib, pb) = new_word_inst(func, InstKind::SlicePtr(b));
                    let (ila, la) = new_word_inst(func, InstKind::SliceLen(a));
                    let (ilb, lb) = new_word_inst(func, InstKind::SliceLen(b));
                    let (isp, sp) = new_word_inst(func, InstKind::Select(cond, pa, pb));
                    let (isl, sl) = new_word_inst(func, InstKind::Select(cond, la, lb));
                    let (ims, new_slice) = new_slice_inst(func, sp, sl, location);
                    out.extend([ia, ib, ila, ilb, isp, isl, ims]);
                    replacements.insert(old, new_slice);
                    self.stats.slices += 1;
                    changed = true;
                    continue;
                }
                out.push(inst_id);
            }
            func.blocks[*block_id].instructions = out;
        }

        // Phis: project each incoming slice in its predecessor, phi the
        // pointer and length words, and rebuild the slice after the phis.
        for block_id in &block_ids {
            let block_id = *block_id;
            // Collect the leading slice phis before mutating, since forming the
            // paired words allocates instructions and values.
            type SlicePhi = (InstId, Vec<(BlockId, ValueId)>, SliceLocation);
            let mut slice_phis: Vec<SlicePhi> = Vec::new();
            for &inst_id in &func.blocks[block_id].instructions {
                match &func.instructions[inst_id].kind {
                    InstKind::Phi(incoming) => {
                        if let Some(location) = incoming
                            .first()
                            .and_then(|(_, value)| value_slice_location(func, *value))
                        {
                            slice_phis.push((inst_id, incoming.clone(), location));
                        }
                    }
                    _ => break,
                }
            }
            let mut splits: Vec<(InstId, InstId, InstId, InstId)> = Vec::new();
            for (inst_id, incoming, location) in slice_phis {
                let mut ptr_incoming = Vec::with_capacity(incoming.len());
                let mut len_incoming = Vec::with_capacity(incoming.len());
                for (pred, value) in incoming {
                    let (pi, pv) = new_word_inst(func, InstKind::SlicePtr(value));
                    let (li, lv) = new_word_inst(func, InstKind::SliceLen(value));
                    func.blocks[pred].instructions.push(pi);
                    func.blocks[pred].instructions.push(li);
                    ptr_incoming.push((pred, pv));
                    len_incoming.push((pred, lv));
                }
                let (ptr_phi, sp) = new_word_inst(func, InstKind::Phi(ptr_incoming));
                let (len_phi, sl) = new_word_inst(func, InstKind::Phi(len_incoming));
                let (make, new_slice) = new_slice_inst(func, sp, sl, location);
                let old = func.inst_result_value(inst_id).expect("phi has a result");
                replacements.insert(old, new_slice);
                splits.push((inst_id, ptr_phi, len_phi, make));
                self.stats.slices += 1;
                changed = true;
            }
            if splits.is_empty() {
                continue;
            }
            let split_map: FxHashMap<InstId, (InstId, InstId, InstId)> =
                splits.iter().map(|&(old, sp, sl, ms)| (old, (sp, sl, ms))).collect();
            let mut phis = Vec::new();
            let mut makes = Vec::new();
            let mut rest = Vec::new();
            for &inst_id in &func.blocks[block_id].instructions {
                if matches!(func.instructions[inst_id].kind, InstKind::Phi(_)) {
                    if let Some(&(sp, sl, ms)) = split_map.get(&inst_id) {
                        phis.push(sp);
                        phis.push(sl);
                        makes.push(ms);
                    } else {
                        phis.push(inst_id);
                    }
                } else {
                    rest.push(inst_id);
                }
            }
            phis.extend(makes);
            phis.extend(rest);
            func.blocks[block_id].instructions = phis;
        }

        if changed {
            func.replace_uses_canonicalized(&replacements);
        }
        changed
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
                    new_params.push(slice_param_ptr_type(location));
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
                    let ptr_ty = slice_param_ptr_type(location);
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
            // Eliminate slice-typed `select`/`phi` first, so every remaining
            // slice is a `make_slice` result or a projection that the later
            // stages can expand or fold.
            changed |= self.split_slice_aggregates(func);
        }
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
