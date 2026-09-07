//! Lower logical MIR slices back to their component words.
//!
//! Slices are deliberately a higher-level MIR abstraction. The EVM backend
//! remains word-based, so this pass expands slice parameters and call
//! arguments, resolves `slice_ptr`/`slice_len` projections, and erases the
//! corresponding constructors before machine lowering.

use crate::{
    memory::EvmMemoryLayout,
    mir::{
        ArgIdx, BlockId, Function, FunctionBuilder, FunctionId, InstId, InstKind, Instruction,
        MirType, Module, SliceLocation, Terminator, Value, ValueId,
    },
    pass::MirPass,
};
use solar_data_structures::{
    index::IndexVec,
    map::{FxHashMap, FxHashSet},
};
use solar_sema::Gcx;

/// Lowers logical slices to the word-based backend convention.
pub(crate) struct LowerSlices;

impl MirPass for LowerSlices {
    fn name(&self) -> &'static str {
        "lower-slices"
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(
        &self,
        _gcx: Gcx<'_>,
        module: &mut Module,
        _analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        Self::run(module)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ParamRepr {
    Word,
    CompactCalldata,
    Pair,
}

/// The pointer type for a slice parameter's leading word. Returndata has no
/// dedicated pointer type: like the result of `returndatasize`, its offset is
/// an ordinary EVM word whose address space remains encoded by the slice type.
fn slice_param_ptr_type(location: SliceLocation) -> MirType {
    match location {
        SliceLocation::Memory => MirType::MemPtr,
        SliceLocation::Calldata => MirType::CalldataPtr,
        SliceLocation::Returndata => MirType::uint256(),
    }
}

/// Allocates a word-typed instruction and its result value, returning both.
fn new_word_inst(func: &mut Function, kind: InstKind) -> (InstId, ValueId) {
    func.alloc_value_inst(
        Instruction::new(kind, Some(MirType::uint256())).with_debug_info_dropped(),
    )
}

/// Allocates a `make_slice` instruction and its slice-typed result value.
fn new_slice_inst(
    func: &mut Function,
    ptr: ValueId,
    len: ValueId,
    location: SliceLocation,
) -> (InstId, ValueId) {
    func.alloc_value_inst(
        Instruction::new(
            InstKind::MakeSlice { ptr, len, location },
            Some(MirType::Slice(location)),
        )
        .with_debug_info_dropped(),
    )
}

fn split_slice_phis(
    func: &mut Function,
    incoming: Vec<(BlockId, ValueId)>,
) -> ((InstId, ValueId), (InstId, ValueId)) {
    let mut ptr_incoming = Vec::with_capacity(incoming.len());
    let mut len_incoming = Vec::with_capacity(incoming.len());
    for (pred, value) in incoming {
        let (pointer, length) = if func.value_slice_location(value).is_some() {
            let (ptr_inst, pointer) = new_word_inst(func, InstKind::SlicePtr(value));
            let (len_inst, length) = new_word_inst(func, InstKind::SliceLen(value));
            func.blocks[pred].instructions.push(ptr_inst);
            func.blocks[pred].instructions.push(len_inst);
            (pointer, length)
        } else {
            let (len_inst, length) = new_word_inst(func, InstKind::MLoad(value));
            func.blocks[pred].instructions.push(len_inst);
            (value, length)
        };
        ptr_incoming.push((pred, pointer));
        len_incoming.push((pred, length));
    }
    (
        new_word_inst(func, InstKind::Phi(ptr_incoming)),
        new_word_inst(func, InstKind::Phi(len_incoming)),
    )
}

fn can_split_slice_incoming(
    func: &Function,
    incoming: &[(BlockId, ValueId)],
    allow_memory_pointer: bool,
) -> bool {
    let mut has_split = false;
    incoming.iter().all(|(_, value)| {
        let ty = func.value_ty(*value);
        let is_slice = matches!(ty, Some(MirType::Slice(_)));
        has_split |= is_slice || (allow_memory_pointer && matches!(ty, Some(MirType::MemPtr)));
        is_slice || matches!(ty, Some(MirType::MemPtr | MirType::UInt(_)))
    }) && has_split
}

impl LowerSlices {
    fn expand_physical_value(
        builder: &mut FunctionBuilder<'_>,
        value: ValueId,
        repr: ParamRepr,
        expanded: &mut Vec<ValueId>,
    ) {
        match repr {
            ParamRepr::Word | ParamRepr::CompactCalldata => expanded.push(value),
            ParamRepr::Pair => {
                expanded.push(builder.slice_ptr(value));
                expanded.push(builder.slice_len(value));
            }
        }
    }

    /// Splits a pointer phi whose incoming values still mix memory objects and
    /// logical slices. Inlining can erase the pointer type on one incoming
    /// memory object, so the physical word types may differ. Keep the pointer
    /// and length paths separate at the backend boundary.
    fn lower_mixed_slice_phis(func: &mut Function) -> bool {
        let mut replacements = FxHashMap::default();
        let mut removed = FxHashSet::default();
        let mut insertions = FxHashMap::default();
        let mut projection_users: FxHashMap<ValueId, Vec<(InstId, ValueId, bool)>> =
            FxHashMap::default();
        for user in func.instructions() {
            let Some(user_result) = func.inst_result_value(user) else { continue };
            match func.inst(user).kind {
                InstKind::SlicePtr(slice) => {
                    projection_users.entry(slice).or_default().push((user, user_result, true));
                }
                InstKind::SliceLen(slice) => {
                    projection_users.entry(slice).or_default().push((user, user_result, false));
                }
                _ => {}
            }
        }

        for block_id in func.blocks.indices() {
            let instructions = func.blocks[block_id].instructions.clone();
            for inst_id in instructions {
                let InstKind::Phi(incoming) = func.inst(inst_id).kind.clone() else { continue };
                let Some(result) = func.inst_result_value(inst_id) else { continue };
                let Some(users) = projection_users.get(&result) else { continue };
                if !can_split_slice_incoming(func, &incoming, true) {
                    continue;
                }

                let ((ptr_phi, pointer), (len_phi, length)) = split_slice_phis(func, incoming);
                insertions.insert(inst_id, (ptr_phi, len_phi));
                replacements.insert(result, pointer);
                for &(user, user_result, is_pointer) in users {
                    let replacement = if is_pointer { pointer } else { length };
                    replacements.insert(user_result, replacement);
                    removed.insert(user);
                }
                removed.insert(inst_id);
            }
        }

        if insertions.is_empty() {
            return false;
        }
        func.replace_uses_canonicalized(&replacements);
        for block in func.blocks.iter_mut() {
            let mut instructions = Vec::with_capacity(block.instructions.len());
            for inst_id in std::mem::take(&mut block.instructions) {
                if let Some(&(ptr_phi, len_phi)) = insertions.get(&inst_id) {
                    instructions.push(ptr_phi);
                    instructions.push(len_phi);
                } else if !removed.contains(&inst_id) {
                    instructions.push(inst_id);
                }
            }
            block.instructions = instructions;
        }
        true
    }
    /// Computes the shifted local-frame addresses for a signature expansion.
    ///
    /// Parsed MIR can contain arbitrary `u64` frame offsets. Collect every replacement before
    /// mutating the function so an offset near the address-space limit makes this transform bail
    /// atomically instead of panicking in debug builds or wrapping in release builds.
    fn shifted_frame_offsets(func: &Function, added_slots: usize) -> Option<Vec<(InstId, u64)>> {
        if added_slots == 0 {
            return Some(Vec::new());
        }
        let signature_slots = func.params.len().checked_add(func.returns.len())?;
        let signature_size =
            u64::try_from(signature_slots).ok()?.checked_mul(EvmMemoryLayout::WORD_SIZE)?;
        let old_local_start =
            EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE.checked_add(signature_size)?;
        let shift = u64::try_from(added_slots).ok()?.checked_mul(EvmMemoryLayout::WORD_SIZE)?;

        let mut shifted = Vec::new();
        for inst_id in func.instructions() {
            let InstKind::InternalFrameAddr(offset) = func.inst(inst_id).kind else { continue };
            // Lowering emits this instruction only for frame locals. Parsed MIR can address the
            // header or signature directly, but a raw signature offset does not identify which
            // parameter or result it belongs to after a slice expands. Bail instead of silently
            // retargeting it to a different slot.
            if offset < old_local_start {
                return None;
            }
            shifted.push((inst_id, offset.checked_add(shift)?));
        }
        Some(shifted)
    }

    /// Rewrites slice-typed `select` and `phi` into paired pointer/length
    /// operations over a `make_slice`, so no two-word slice value survives an
    /// aggregate use. Each operand slice is then consumed only by projections
    /// and folds away in `lower_projections`.
    fn split_slice_aggregates(func: &mut Function) -> bool {
        let mut replacements = FxHashMap::default();

        // Selects: rewrite in place within their block.
        for block_id in func.blocks.indices() {
            let insts = std::mem::take(&mut func.blocks[block_id].instructions);
            let mut out = Vec::with_capacity(insts.len());
            for inst_id in insts {
                if let InstKind::Select(cond, a, b) = func.inst(inst_id).kind
                    && let Some(location) = func.value_slice_location(a)
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
                    continue;
                }
                out.push(inst_id);
            }
            func.blocks[block_id].instructions = out;
        }

        // Phis: project each incoming slice in its predecessor, phi the
        // pointer and length words, and rebuild the slice after the phis.
        for block_id in func.blocks.indices() {
            // Collect the leading slice phis before mutating, since forming the
            // paired words allocates instructions and values.
            let mut slice_phis = Vec::new();
            for &inst_id in &func.blocks[block_id].instructions {
                let InstKind::Phi(incoming) = &func.inst(inst_id).kind else { continue };
                let Some(location) = func
                    .inst_result_value(inst_id)
                    .and_then(|result| func.value_slice_location(result))
                else {
                    continue;
                };
                if can_split_slice_incoming(func, incoming, false) {
                    slice_phis.push((inst_id, location));
                }
            }
            let mut split_map = FxHashMap::default();
            for (inst_id, location) in slice_phis {
                let incoming = match &mut func.inst_mut(inst_id).kind {
                    InstKind::Phi(incoming) => std::mem::take(incoming),
                    _ => unreachable!(),
                };
                let ((ptr_phi, sp), (len_phi, sl)) = split_slice_phis(func, incoming);
                let (make, new_slice) = new_slice_inst(func, sp, sl, location);
                let old = func.inst_result_value(inst_id).expect("phi has a result");
                replacements.insert(old, new_slice);
                split_map.insert(inst_id, (ptr_phi, len_phi, make));
            }
            if split_map.is_empty() {
                continue;
            }
            let mut phis = Vec::new();
            let mut makes = Vec::new();
            let mut rest = Vec::new();
            for &inst_id in &func.blocks[block_id].instructions {
                if matches!(func.inst(inst_id).kind, InstKind::Phi(_)) {
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

        let changed = !replacements.is_empty();
        func.replace_uses_canonicalized(&replacements);
        changed
    }

    fn expand_call_args(
        func: &mut Function,
        signatures: &FxHashMap<FunctionId, IndexVec<ArgIdx, ParamRepr>>,
    ) -> bool {
        let mut changed = false;
        let block_ids = func.blocks.indices();
        for block_id in block_ids {
            let instructions = std::mem::take(&mut func.blocks[block_id].instructions);
            let mut builder = FunctionBuilder::new(func);
            builder.switch_to_block(block_id);
            for inst_id in instructions {
                let call = match &builder.func().inst(inst_id).kind {
                    InstKind::ICall { function, args, .. } => Some((*function, args.to_vec())),
                    _ => None,
                };
                if let Some((callee, args)) = call
                    && let Some(signature) = signatures.get(&callee)
                    && signature.iter().any(|repr| *repr != ParamRepr::Word)
                {
                    let mut expanded = Vec::with_capacity(args.len() + 1);
                    for (index, arg) in args.into_iter().enumerate() {
                        let repr =
                            signature.get(ArgIdx::new(index)).copied().unwrap_or(ParamRepr::Word);
                        Self::expand_physical_value(&mut builder, arg, repr, &mut expanded);
                    }
                    let InstKind::ICall { args, .. } =
                        &mut builder.func_mut().inst_mut(inst_id).kind
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

    fn expand_call_returns(
        func: &mut Function,
        signatures: &FxHashMap<FunctionId, Vec<ParamRepr>>,
    ) -> bool {
        let mut changed = false;
        let mut replacements = FxHashMap::default();
        let mut constructors = Vec::new();
        let block_ids = func.blocks.indices();
        for block_id in block_ids {
            let instructions = std::mem::take(&mut func.blocks[block_id].instructions);
            let mut builder = FunctionBuilder::new(func);
            builder.switch_to_block(block_id);
            for inst_id in instructions {
                builder.func_mut().blocks[block_id].instructions.push(inst_id);
                let Some((signature, result)) = (match builder.func().inst(inst_id).kind {
                    InstKind::ICall { function, returns, .. } => {
                        signatures.get(&function).and_then(|signature| {
                            (usize::try_from(returns).ok() == Some(signature.len()))
                                .then(|| {
                                    builder
                                        .func()
                                        .inst_result_value(inst_id)
                                        .map(|result| (signature, result))
                                })
                                .flatten()
                        })
                    }
                    _ => None,
                }) else {
                    continue;
                };

                let returns = u32::try_from(
                    signature.len()
                        + signature.iter().filter(|&&repr| repr == ParamRepr::Pair).count(),
                )
                .expect("MIR return count fits in u32");

                let instruction = builder.func_mut().inst_mut(inst_id);
                let InstKind::ICall { returns: call_returns, .. } = &mut instruction.kind else {
                    unreachable!()
                };
                changed |= *call_returns != returns;
                *call_returns = returns;
                let Some(MirType::Slice(location)) = instruction.result_ty else {
                    continue;
                };
                instruction.result_ty = Some(slice_param_ptr_type(location));
                changed = true;

                if signature[0] != ParamRepr::Pair {
                    continue;
                }

                let already_rebuilt = builder.func().instructions().any(|user| {
                    matches!(builder.func().inst(user).kind, InstKind::MakeSlice { ptr, .. } if ptr == result)
                });
                if already_rebuilt {
                    changed = true;
                    continue;
                }

                let slot = builder.imm(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT);
                let base = builder.mload(slot);
                let length_address = builder.add_u64_offset(base, EvmMemoryLayout::WORD_SIZE);
                let length = builder.mload(length_address);
                let slice = builder.make_slice(result, length, location);
                let Value::Inst(constructor) = *builder.func().value(slice) else { unreachable!() };
                replacements.insert(result, slice);
                constructors.push((constructor, result, length, location));
                changed = true;
            }
        }
        func.replace_uses_canonicalized(&replacements);
        for (constructor, pointer, length, location) in constructors {
            func.inst_mut(constructor).kind =
                InstKind::MakeSlice { ptr: pointer, len: length, location };
        }
        changed
    }

    fn lower_returns(func: &mut Function, signature: &[ParamRepr]) -> bool {
        let added_slots = signature.iter().filter(|&&repr| repr == ParamRepr::Pair).count();
        let lowers_word_slice = func
            .returns
            .iter()
            .zip(signature)
            .any(|(ty, repr)| Self::is_slice(ty) && *repr != ParamRepr::Pair);
        if added_slots == 0 && !lowers_word_slice {
            return false;
        }
        let shifted = Self::shifted_frame_offsets(func, added_slots)
            .expect("slice return expansion was checked before lowering");
        let block_ids = func.blocks.indices();
        for block_id in block_ids {
            let Some(Terminator::Return { values }) = func.blocks[block_id].terminator.as_ref()
            else {
                continue;
            };
            if values.len() != signature.len() {
                continue;
            }
            let values = values.clone();
            let mut builder = FunctionBuilder::new(func);
            builder.switch_to_block(block_id);
            builder.inherit_terminator_debug_context(block_id);
            let mut expanded = Vec::with_capacity(values.len() + added_slots);
            for (value, repr) in values.into_iter().zip(signature) {
                Self::expand_physical_value(&mut builder, value, *repr, &mut expanded);
            }
            builder.func_mut().blocks[block_id].terminator =
                Some(Terminator::Return { values: expanded.into() });
        }
        for (inst_id, shifted) in shifted {
            let InstKind::InternalFrameAddr(offset) = &mut func.inst_mut(inst_id).kind else {
                unreachable!()
            };
            *offset = shifted;
        }
        let mut returns = Vec::with_capacity(func.returns.len() + added_slots);
        for (&ty, repr) in func.returns.iter().zip(signature) {
            match repr {
                ParamRepr::Word | ParamRepr::CompactCalldata => match ty {
                    MirType::Slice(location) => returns.push(slice_param_ptr_type(location)),
                    _ => returns.push(ty),
                },
                ParamRepr::Pair => {
                    let MirType::Slice(location) = ty else { unreachable!() };
                    returns.push(slice_param_ptr_type(location));
                    returns.push(MirType::uint256());
                }
            }
        }
        func.returns = returns;
        true
    }

    fn lower_params(func: &mut Function, signature: &IndexVec<ArgIdx, ParamRepr>) -> bool {
        if func.selector.is_some()
            || func.blocks.is_empty()
            || !func.params.iter().any(Self::is_slice)
        {
            return false;
        }

        let mut physical_indices = IndexVec::<ArgIdx, ArgIdx>::with_capacity(func.params.len());
        let mut new_params = IndexVec::<ArgIdx, MirType>::with_capacity(func.params.len() + 1);
        for (index, &ty) in func.params.iter_enumerated() {
            physical_indices.push(new_params.next_idx());
            match signature[index] {
                ParamRepr::Word => {
                    new_params.push(ty);
                }
                ParamRepr::CompactCalldata => {
                    new_params.push(MirType::uint256());
                }
                ParamRepr::Pair => {
                    let MirType::Slice(location) = ty else { unreachable!() };
                    new_params.push(slice_param_ptr_type(location));
                    new_params.push(MirType::uint256());
                }
            }
        }
        let added_slots = new_params.len() - func.params.len();
        let Some(shifted_frame_offsets) = Self::shifted_frame_offsets(func, added_slots) else {
            return false;
        };
        let argument_values: FxHashSet<_> = func
            .live_values()
            .filter(|&value| matches!(func.value(value), Value::Arg(_)))
            .collect();
        let mut slice_args = Vec::new();
        for &value in &argument_values {
            let Value::Arg(index) = func.value(value) else { unreachable!() };
            let index = *index;
            if matches!(func.arg_ty(index), MirType::Slice(_)) {
                slice_args.push((value, index));
            } else {
                let Value::Arg(value_index) = func.value_mut(value) else { unreachable!() };
                *value_index = physical_indices[index];
            }
        }

        func.set_params(new_params);
        // Frame-local addresses bake the signature prefix, so growing the
        // parameter list shifts the locals region up by one word per added
        // physical parameter. Rebase every baked offset or the backend —
        // which recomputes the frame layout from the widened signature —
        // would read the old addresses as parameter or return slots.
        for (inst_id, shifted) in shifted_frame_offsets {
            let InstKind::InternalFrameAddr(offset) = &mut func.inst_mut(inst_id).kind else {
                unreachable!()
            };
            *offset = shifted;
        }
        let mut components = FxHashMap::default();
        let mut compact_heads = FxHashMap::default();
        let mut builder = FunctionBuilder::new(func);
        for (slice_arg, logical_index) in slice_args {
            let physical_index = physical_indices[logical_index];
            match signature[logical_index] {
                ParamRepr::CompactCalldata => {
                    let head = builder.func_mut().alloc_arg(physical_index);
                    compact_heads.insert(slice_arg, head);
                }
                ParamRepr::Pair => {
                    let ptr = builder.func_mut().alloc_arg(physical_index);
                    let len = builder.func_mut().alloc_arg(ArgIdx::new(physical_index.index() + 1));
                    components.insert(slice_arg, (ptr, len));
                }
                ParamRepr::Word => unreachable!(),
            }
        }

        let mut replacements = FxHashMap::default();
        let mut removed = FxHashSet::default();
        for inst_id in builder.func().instructions() {
            let replacement = match builder.func().inst(inst_id).kind {
                InstKind::SlicePtr(slice) => components.get(&slice).map(|&(ptr, _)| ptr),
                InstKind::SliceLen(slice) => components.get(&slice).map(|&(_, len)| len),
                _ => None,
            };
            if let Some(replacement) = replacement
                && let Some(result) = builder.func().inst_result_value(inst_id)
            {
                replacements.insert(result, replacement);
                removed.insert(inst_id);
            }
        }
        builder.func_mut().replace_uses_canonicalized(&replacements);
        for block in builder.func_mut().blocks.iter_mut() {
            block.instructions.retain(|inst| !removed.contains(inst));
        }
        if !compact_heads.is_empty() {
            Self::lower_compact_values(builder.func_mut(), &compact_heads);
        }
        true
    }

    fn lower_compact_values(func: &mut Function, raw_heads: &FxHashMap<ValueId, ValueId>) {
        let mut replacements = raw_heads.clone();
        let block_ids = func.blocks.indices();
        for block_id in block_ids {
            let instructions = std::mem::take(&mut func.blocks[block_id].instructions);
            let mut builder = FunctionBuilder::new(func);
            builder.switch_to_block(block_id);
            let mut lengths = FxHashMap::default();
            let mut pointers = FxHashMap::default();
            for inst_id in instructions {
                let projection = match builder.func().inst(inst_id).kind {
                    InstKind::SlicePtr(slice) => raw_heads.get(&slice).map(|&head| (head, true)),
                    InstKind::SliceLen(slice) => raw_heads.get(&slice).map(|&head| (head, false)),
                    _ => None,
                };
                if let Some((head, is_ptr)) = projection {
                    let replacement = if is_ptr {
                        *pointers.entry(head).or_insert_with(|| builder.add_u64_offset(head, 36))
                    } else {
                        *lengths.entry(head).or_insert_with(|| {
                            let len_pos = builder.add_u64_offset(head, 4);
                            builder.calldataload(len_pos)
                        })
                    };
                    let result = builder
                        .func()
                        .inst_result_value(inst_id)
                        .expect("slice projection must produce a value");
                    replacements.insert(result, replacement);
                } else {
                    builder.func_mut().blocks[block_id].instructions.push(inst_id);
                }
            }
        }
        func.replace_uses_canonicalized(&replacements);
    }

    fn lower_external_args(func: &mut Function) -> bool {
        if func.selector.is_none() || func.blocks.is_empty() {
            return false;
        }
        let slice_args: FxHashMap<_, _> = func
            .arg_uses()
            .iter_enumerated()
            .filter(|(index, _)| func.arg_ty(*index) == MirType::Slice(SliceLocation::Calldata))
            .flat_map(|(index, uses)| uses.iter().map(move |&value| (value, index)))
            .collect();
        if slice_args.is_empty() {
            return false;
        }

        for &index in slice_args.values() {
            func.set_arg_ty(index, MirType::uint256());
        }
        let raw_heads: FxHashMap<_, _> = slice_args
            .iter()
            .map(|(&slice, &index)| {
                let head = func.alloc_arg(index);
                (slice, head)
            })
            .collect();
        Self::lower_compact_values(func, &raw_heads);
        true
    }

    fn infer_compact_params(module: &Module) -> FxHashSet<(FunctionId, ArgIdx)> {
        let mut compact: FxHashSet<_> = module
            .functions
            .iter_enumerated()
            .flat_map(|(function, func)| {
                func.params.iter_enumerated().filter_map(move |(index, ty)| {
                    matches!(ty, MirType::Slice(SliceLocation::Calldata))
                        .then_some((function, index))
                })
            })
            .collect();

        loop {
            let mut removed = FxHashSet::default();
            let mut seen = FxHashSet::default();
            for (caller_id, caller) in module.functions.iter_enumerated() {
                for inst_id in caller.instructions() {
                    let inst = caller.inst(inst_id);
                    let InstKind::ICall { function: callee, args, .. } = &inst.kind else {
                        continue;
                    };
                    for index in module.function(*callee).params.indices() {
                        let candidate = (*callee, index);
                        if !compact.contains(&candidate) {
                            continue;
                        }
                        seen.insert(candidate);
                        let Some(&arg) = args.get(index.index()) else {
                            removed.insert(candidate);
                            continue;
                        };
                        let is_compact = match caller.value(arg) {
                            Value::Arg(source)
                                if caller.arg_ty(*source)
                                    == MirType::Slice(SliceLocation::Calldata) =>
                            {
                                caller.selector.is_some() || compact.contains(&(caller_id, *source))
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

    fn lower_projections(func: &mut Function) -> bool {
        let live_insts: Vec<_> = func.instructions().collect();
        let mut components = FxHashMap::<ValueId, (ValueId, ValueId, InstId)>::default();
        let mut projections = FxHashMap::<ValueId, (ValueId, InstId, bool)>::default();
        let mut replacements = FxHashMap::default();
        let mut removed = FxHashSet::default();
        for &inst in &live_insts {
            let Some(result) = func.inst_result_value(inst) else { continue };
            match func.inst(inst).kind {
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

        // Inlining can substitute an already-materialized memory object for a
        // logical calldata slice parameter. Once memory-object lowering has
        // erased the nominal object type, the projections must use the
        // physical object representation.
        for &(slice, inst, is_ptr) in projections.values() {
            let Some(ty) = func.value_ty(slice) else { continue };
            let physical_object = matches!(ty, MirType::MemPtr);
            let physical_pointer = matches!(ty, MirType::MemPtr | MirType::CalldataPtr);
            let physical_word = matches!(ty, MirType::UInt(_))
                && matches!(func.value(slice), Value::Inst(def) if matches!(func.inst(*def).kind, InstKind::MLoad(_)));
            if physical_word || (physical_pointer && is_ptr) {
                let result = func.inst_result_value(inst).expect("slice projection has a result");
                replacements.insert(result, slice);
                removed.insert(inst);
            } else if physical_object {
                func.inst_mut(inst).kind = InstKind::MLoad(slice);
            }
        }
        if components.is_empty() {
            if replacements.is_empty() {
                return false;
            }
            func.replace_uses_canonicalized(&replacements);
            for block in func.blocks.iter_mut() {
                block.instructions.retain(|inst| !removed.contains(inst));
            }
            return true;
        }

        for (&slice, &(ptr, len, constructor)) in &components {
            replacements.insert(slice, ptr);
            removed.insert(constructor);
            for (&result, &(projected_slice, inst, is_ptr)) in &projections {
                if projected_slice == slice {
                    replacements.insert(result, if is_ptr { ptr } else { len });
                    removed.insert(inst);
                }
            }
        }

        func.replace_uses_canonicalized(&replacements);
        for block in func.blocks.iter_mut() {
            block.instructions.retain(|inst| !removed.contains(inst));
        }
        true
    }
    fn run(module: &mut Module) -> bool {
        let compact = Self::infer_compact_params(module);
        let return_signatures: FxHashMap<_, _> = module
            .functions
            .iter_enumerated()
            .filter(|(_, func)| func.selector.is_none())
            .map(|(id, func)| {
                let signature = func
                    .returns
                    .iter()
                    .enumerate()
                    .map(|(index, ty)| {
                        let has_slice_value = func.blocks.iter().any(|block| {
                            matches!(
                                &block.terminator,
                                Some(Terminator::Return { values })
                                    if values.get(index).is_some_and(|&value| {
                                        func.value_slice_location(value).is_some()
                                    })
                            )
                        });
                        if Self::is_slice(ty) && has_slice_value {
                            ParamRepr::Pair
                        } else {
                            ParamRepr::Word
                        }
                    })
                    .collect::<Vec<_>>();
                (id, signature)
            })
            .collect();
        let signatures: FxHashMap<_, _> = module
            .functions
            .iter_enumerated()
            .map(|(id, func)| {
                let signature = func
                    .params
                    .iter_enumerated()
                    .map(|(index, ty)| match ty {
                        MirType::Slice(SliceLocation::Calldata)
                            if compact.contains(&(id, index)) =>
                        {
                            ParamRepr::CompactCalldata
                        }
                        MirType::Slice(_) => ParamRepr::Pair,
                        _ => ParamRepr::Word,
                    })
                    .collect::<IndexVec<ArgIdx, _>>();
                (id, signature)
            })
            .collect();
        // Signature expansion rewrites call edges throughout the module. Prove every frame-address
        // shift first so an unrepresentable parsed-MIR offset leaves the complete module untouched.
        if module.functions.iter_enumerated().any(|(id, func)| {
            let added_slots = if func.selector.is_none()
                && !func.blocks.is_empty()
                && func.params.iter().any(Self::is_slice)
            {
                signatures[&id].iter().filter(|&&repr| repr == ParamRepr::Pair).count()
            } else {
                0
            };
            let added_slots = added_slots
                + return_signatures
                    .get(&id)
                    .into_iter()
                    .flatten()
                    .filter(|&&repr| repr == ParamRepr::Pair)
                    .count();
            Self::shifted_frame_offsets(func, added_slots).is_none()
        }) {
            return false;
        }
        let mut changed = false;
        for (id, func) in module.functions.iter_mut_enumerated() {
            // Eliminate slice-typed `select`/`phi` first, so every remaining
            // slice is a `make_slice` result or a projection that the later
            // stages can expand or fold.
            changed |= Self::split_slice_aggregates(func);
            changed |= Self::expand_call_args(func, &signatures);
            changed |= Self::expand_call_returns(func, &return_signatures);
            changed |= Self::lower_external_args(func);
            if let Some(signature) = return_signatures.get(&id) {
                changed |= Self::lower_returns(func, signature);
            }
            changed |= Self::lower_params(func, &signatures[&id]);
            changed |= Self::split_slice_aggregates(func);
            changed |= Self::lower_mixed_slice_phis(func);
            while Self::lower_projections(func) {
                changed = true;
            }
        }
        changed
    }
}
