//! Lower logical MIR slices back to their component words.
//!
//! Slices are deliberately a higher-level MIR abstraction. The EVM backend
//! remains word-based, so this pass expands slice parameters and call
//! arguments, resolves `extract_value` projections of slices, and erases the
//! corresponding constructors before machine lowering. A slice's pointer crosses a call as a word
//! and becomes a `ptr<location>` again at its projection, so the casts cancel. SSA structs must
//! have been lowered first; otherwise expanding a slice would invalidate the struct field's
//! declared type.
//!
//! The pass is required and runs after `lower-memory-objects` and before `lower-alloc`; the
//! backend accepts no slice value.
//!
//! Slice-typed `select` and `phi` first split into pointer and length operations. An internal
//! slice parameter becomes a pointer and a length word, except a calldata slice whose every caller
//! forwards the calldata argument of an external entry or of another such parameter: that one
//! stays a single word, the ABI head offset, and its projections read the length and data from
//! calldata. External entries use the same head form. A returned slice whose producer is a real
//! slice returns its pointer as the result and its length in the multi-return buffer; other slice
//! results return one word. Each signature change shifts local frame offsets, which are checked
//! for the whole module first, so an unrepresentable offset leaves the module unchanged.
//!
//! Every function ends by folding projections in one scan: a `make_slice` is replaced by its
//! pointer, its `extract_value` projections by the pointer or length word, and a projection of an
//! undefined slice by an undefined component. In valid MIR the operands of `make_slice` are a
//! pointer and a length, not slices, so folding exposes no new projection and one scan is enough.

use crate::mir::{
    ArgIdx, BlockId, Callee, Function, FunctionBuilder, FunctionId, InstId, InstKind, Instruction,
    MirType, Module, PtrLocation, Terminator, Value, ValueId,
    memory::EvmMemoryLayout,
    pass::{MirPass, ModuleAnalyses, run_lowering_function_pass},
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

    fn run_pass(&self, _gcx: Gcx<'_>, module: &mut Module, analyses: &mut ModuleAnalyses) -> bool {
        // NOTE: Rewriting a projection of a lowered memory object into a load is not reported
        // as a change on its own.
        analyses.note_unreported_module_edit();
        Self::run(module, analyses)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ParamRepr {
    Word,
    CompactCalldata,
    Pair,
}

/// Allocates an instruction of type `ty` and its result value, returning both.
fn new_inst(func: &mut Function, kind: InstKind, ty: MirType) -> (InstId, ValueId) {
    func.alloc_value_inst(Instruction::new(kind, Some(ty)).with_debug_info_dropped())
}

/// Allocates a word-typed instruction and its result value, returning both.
fn new_word_inst(func: &mut Function, kind: InstKind) -> (InstId, ValueId) {
    new_inst(func, kind, MirType::I256)
}

/// Allocates a projection of field `index` of `slice`, typed by the slice's location.
fn new_projection(func: &mut Function, slice: ValueId, index: u32) -> (InstId, ValueId) {
    let (kind, ty) = InstKind::slice_field(func, slice, index);
    new_inst(func, kind, ty)
}

fn split_slice_phis(
    func: &mut Function,
    incoming: Vec<(BlockId, ValueId)>,
    location: PtrLocation,
) -> ((InstId, ValueId), (InstId, ValueId)) {
    let pointer_ty = MirType::Ptr(location);
    let mut ptr_incoming = Vec::with_capacity(incoming.len());
    let mut len_incoming = Vec::with_capacity(incoming.len());
    for (pred, value) in incoming {
        let (ptr_inst, pointer) = new_projection(func, value, MirType::SLICE_PTR_FIELD);
        let (len_inst, length) = new_projection(func, value, MirType::SLICE_LEN_FIELD);
        func.blocks[pred].instructions.push(ptr_inst);
        func.blocks[pred].instructions.push(len_inst);
        ptr_incoming.push((pred, pointer));
        len_incoming.push((pred, length));
    }
    (
        new_inst(func, InstKind::Phi(ptr_incoming), pointer_ty),
        new_word_inst(func, InstKind::Phi(len_incoming)),
    )
}

impl LowerSlices {
    fn expand_physical_value(
        builder: &mut FunctionBuilder<'_>,
        value: ValueId,
        repr: ParamRepr,
        expanded: &mut Vec<ValueId>,
    ) {
        match repr {
            ParamRepr::CompactCalldata => expanded.push(value),
            // pointer_word = ptrtoint (extract_value value, 0) to i256
            ParamRepr::Word if builder.func().value_slice_location(value).is_some() => {
                let pointer = builder.slice_ptr(value);
                expanded.push(builder.cast(pointer, MirType::I256));
            }
            ParamRepr::Word => expanded.push(value),
            ParamRepr::Pair => {
                let pointer = builder.slice_ptr(value);
                expanded.push(builder.cast(pointer, MirType::I256));
                expanded.push(builder.slice_len(value));
            }
        }
    }

    /// Computes the shifted local-frame addresses for a signature expansion.
    ///
    /// Parsed MIR can contain arbitrary `u64` frame offsets. Collect every replacement before
    /// mutating the function so an offset near the address-space limit makes this transform bail
    /// atomically instead of panicking in debug builds or wrapping in release builds.
    fn shifted_frame_offsets(func: &Function, added_slots: usize) -> Option<Vec<(InstId, u64)>> {
        let signature_slots = func
            .params
            .len()
            .checked_add(func.return_components().len())?
            .checked_add(added_slots)?;
        super::utils::rebase_frame_offsets(func, signature_slots)
    }

    /// Rewrites slice-typed `select` and `phi` into paired pointer/length
    /// operations over a `make_slice`, so no two-word slice value survives an
    /// aggregate use. Each operand slice is then consumed only by projections
    /// and folds away in `lower_projections`.
    fn split_slice_aggregates(func: &mut Function) -> bool {
        let mut replacements = FxHashMap::default();

        // Selects: rewrite in place within their block. `insts` and `out` swap roles per block, so
        // two vectors serve every block.
        let mut out = Vec::new();
        for block_id in func.blocks.indices() {
            let mut insts = std::mem::take(&mut func.blocks[block_id].instructions);
            for inst_id in insts.drain(..) {
                if let InstKind::Select(cond, a, b) = func.inst(inst_id).kind
                    && let Some(location) = func.value_slice_location(a)
                {
                    let old = func.inst_result_value(inst_id).expect("select has a result");
                    let (ia, pa) = new_projection(func, a, MirType::SLICE_PTR_FIELD);
                    let (ib, pb) = new_projection(func, b, MirType::SLICE_PTR_FIELD);
                    let (ila, la) = new_projection(func, a, MirType::SLICE_LEN_FIELD);
                    let (ilb, lb) = new_projection(func, b, MirType::SLICE_LEN_FIELD);
                    let (isp, sp) =
                        new_inst(func, InstKind::Select(cond, pa, pb), MirType::Ptr(location));
                    let (isl, sl) = new_word_inst(func, InstKind::Select(cond, la, lb));
                    let (ims, new_slice) = new_inst(
                        func,
                        InstKind::MakeSlice { ptr: sp, len: sl, location },
                        MirType::Slice(location),
                    );
                    out.extend([ia, ib, ila, ilb, isp, isl, ims]);
                    replacements.insert(old, new_slice);
                    continue;
                }
                out.push(inst_id);
            }
            func.blocks[block_id].instructions = std::mem::replace(&mut out, insts);
        }

        // Phis: project each incoming slice in its predecessor, phi the
        // pointer and the length, and rebuild the slice after the phis.
        let mut slice_phis = Vec::new();
        let mut split_map = FxHashMap::default();
        let mut phis = Vec::new();
        let mut makes = Vec::new();
        let mut rest = Vec::new();
        for block_id in func.blocks.indices() {
            // Collect the leading slice phis before mutating, since forming the
            // paired words allocates instructions and values.
            slice_phis.extend(
                func.blocks[block_id]
                    .instructions
                    .iter()
                    .filter(|&&inst_id| matches!(func.inst(inst_id).kind, InstKind::Phi(_)))
                    .filter_map(|&inst_id| {
                        let result = func.inst_result_value(inst_id)?;
                        Some((inst_id, func.value_slice_location(result)?))
                    }),
            );
            split_map.clear();
            for (inst_id, location) in slice_phis.drain(..) {
                let incoming = match &mut func.inst_mut(inst_id).kind {
                    InstKind::Phi(incoming) => std::mem::take(incoming),
                    _ => unreachable!(),
                };
                let ((ptr_phi, sp), (len_phi, sl)) = split_slice_phis(func, incoming, location);
                let (make, new_slice) = new_inst(
                    func,
                    InstKind::MakeSlice { ptr: sp, len: sl, location },
                    MirType::Slice(location),
                );
                let old = func.inst_result_value(inst_id).expect("phi has a result");
                replacements.insert(old, new_slice);
                split_map.insert(inst_id, (ptr_phi, len_phi, make));
            }
            if split_map.is_empty() {
                continue;
            }
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
            phis.append(&mut makes);
            phis.append(&mut rest);
            std::mem::swap(&mut func.blocks[block_id].instructions, &mut phis);
            phis.clear();
        }

        let changed = !replacements.is_empty();
        func.replace_uses_canonicalized(&replacements);
        changed
    }

    fn expand_call_args(
        func: &mut Function,
        signatures: &IndexVec<FunctionId, IndexVec<ArgIdx, ParamRepr>>,
    ) -> bool {
        let mut changed = false;
        let mut spare = Vec::new();
        let block_ids = func.blocks.indices();
        for block_id in block_ids {
            let mut instructions = std::mem::replace(
                &mut func.blocks[block_id].instructions,
                std::mem::take(&mut spare),
            );
            let mut builder = FunctionBuilder::new(func);
            builder.switch_to_block(block_id);
            for inst_id in instructions.drain(..) {
                if let InstKind::ICall { function: Callee::Function(callee), args, .. } =
                    &builder.func().inst(inst_id).kind
                    && let Some(signature) = signatures.get(*callee)
                    && signature.iter().any(|repr| *repr != ParamRepr::Word)
                {
                    let args = args.to_vec();
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
            spare = instructions;
        }
        changed
    }

    fn expand_call_returns(
        func: &mut Function,
        signatures: &IndexVec<FunctionId, Option<Vec<ParamRepr>>>,
    ) -> bool {
        let mut changed = false;
        let mut replacements = FxHashMap::default();
        let mut constructors = Vec::new();
        let mut spare = Vec::new();
        let block_ids = func.blocks.indices();
        for block_id in block_ids {
            let mut instructions = std::mem::replace(
                &mut func.blocks[block_id].instructions,
                std::mem::take(&mut spare),
            );
            let mut builder = FunctionBuilder::new(func);
            builder.switch_to_block(block_id);
            for inst_id in instructions.drain(..) {
                builder.func_mut().blocks[block_id].instructions.push(inst_id);
                let Some((signature, result)) = (match builder.func().inst(inst_id).kind {
                    InstKind::ICall { function: Callee::Function(function), .. } => signatures
                        .get(function)
                        .and_then(Option::as_deref)
                        .zip(builder.func().inst_result_value(inst_id)),
                    _ => None,
                }) else {
                    continue;
                };

                let instruction = builder.func_mut().inst_mut(inst_id);
                let Some(MirType::Slice(location)) = instruction.result_ty else {
                    continue;
                };
                instruction.result_ty = Some(MirType::I256);
                changed = true;

                if signature[0] != ParamRepr::Pair {
                    continue;
                }

                let slot = builder.imm(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT);
                let base = builder.load(slot, PtrLocation::Memory);
                let length_address = builder.add_offset(base, EvmMemoryLayout::WORD_SIZE);
                let length = builder.load(length_address, PtrLocation::Memory);
                // pointer = inttoptr result to ptr<location>
                // slice = make_slice pointer, length
                let pointer = builder.cast(result, MirType::Ptr(location));
                let slice = builder.make_slice(pointer, length, location);
                let Value::Inst(cast) = *builder.func().value(pointer) else { unreachable!() };
                replacements.insert(result, slice);
                constructors.push((cast, result));
                changed = true;
            }
            spare = instructions;
        }
        func.replace_uses_canonicalized(&replacements);
        for (cast, word) in constructors {
            func.inst_mut(cast).kind = InstKind::IntToPtr(word);
        }
        changed
    }

    fn lower_returns(func: &mut Function, signature: &[ParamRepr]) -> bool {
        let added_slots = signature.iter().filter(|&&repr| repr == ParamRepr::Pair).count();
        let lowers_word_slice = func
            .return_components()
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
        for (inst_id, offset) in shifted {
            func.inst_mut(inst_id).kind = InstKind::InternalFrameAddr(offset);
        }
        let mut returns = Vec::with_capacity(func.return_components().len() + added_slots);
        for (&ty, repr) in func.return_components().iter().zip(signature) {
            match repr {
                ParamRepr::Word | ParamRepr::CompactCalldata => match ty {
                    MirType::Slice(_) => returns.push(MirType::I256),
                    _ => returns.push(ty),
                },
                ParamRepr::Pair => {
                    debug_assert!(matches!(ty, MirType::Slice(_)));
                    returns.extend([MirType::I256; 2]);
                }
            }
        }
        // result: slice -> result: slice [return_abi = pointer, length]
        func.set_return_abi(returns);
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
                    new_params.push(MirType::I256);
                }
                ParamRepr::Pair => {
                    debug_assert!(matches!(ty, MirType::Slice(_)));
                    new_params.extend([MirType::I256; 2]);
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
        for (inst_id, offset) in shifted_frame_offsets {
            func.inst_mut(inst_id).kind = InstKind::InternalFrameAddr(offset);
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
        for inst_id in builder.func().instructions().collect::<Vec<_>>() {
            let Some((slice, index)) = builder.func().inst(inst_id).kind.slice_projection() else {
                continue;
            };
            let Some(&(ptr, len)) = components.get(&slice) else { continue };
            if index == MirType::SLICE_PTR_FIELD {
                // extract_value slice, 0 -> inttoptr pointer_word to ptr<location>
                builder.func_mut().inst_mut(inst_id).kind = InstKind::IntToPtr(ptr);
                continue;
            }
            if let Some(result) = builder.func().inst_result_value(inst_id) {
                replacements.insert(result, len);
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
        let mut spare = Vec::new();
        // Values built in one block do not dominate the next, so the caches reset per block.
        let mut lengths = FxHashMap::default();
        let mut pointers = FxHashMap::default();
        let block_ids = func.blocks.indices();
        for block_id in block_ids {
            let mut instructions = std::mem::replace(
                &mut func.blocks[block_id].instructions,
                std::mem::take(&mut spare),
            );
            let mut builder = FunctionBuilder::new(func);
            builder.switch_to_block(block_id);
            lengths.clear();
            pointers.clear();
            for inst_id in instructions.drain(..) {
                let projection = builder.func().inst(inst_id).kind.slice_projection().and_then(
                    |(slice, index)| {
                        raw_heads.get(&slice).map(|&head| (head, index == MirType::SLICE_PTR_FIELD))
                    },
                );
                if let Some((head, is_ptr)) = projection {
                    let replacement = if is_ptr {
                        // pointer = inttoptr (add head, 36) to ptr<calldata>
                        *pointers.entry(head).or_insert_with(|| {
                            let data = builder.add_offset(head, 36);
                            builder.cast(data, MirType::Ptr(PtrLocation::Calldata))
                        })
                    } else {
                        *lengths.entry(head).or_insert_with(|| {
                            let len_pos = builder.add_offset(head, 4);
                            builder.load(len_pos, PtrLocation::Calldata)
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
            spare = instructions;
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
            .filter(|(index, _)| func.arg_ty(*index) == MirType::Slice(PtrLocation::Calldata))
            .flat_map(|(index, uses)| uses.iter().map(move |&value| (value, index)))
            .collect();
        if slice_args.is_empty() {
            return false;
        }

        for &index in slice_args.values() {
            func.set_arg_ty(index, MirType::I256);
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
                    matches!(ty, MirType::Slice(PtrLocation::Calldata)).then_some((function, index))
                })
            })
            .collect();

        let mut removed = FxHashSet::default();
        let mut seen = FxHashSet::default();
        loop {
            removed.clear();
            seen.clear();
            for (caller_id, caller) in module.functions.iter_enumerated() {
                for inst_id in caller.instructions() {
                    let inst = caller.inst(inst_id);
                    let InstKind::ICall { function: Callee::Function(callee), args, .. } =
                        &inst.kind
                    else {
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
                                    == MirType::Slice(PtrLocation::Calldata) =>
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
        let mut components = FxHashMap::<ValueId, (ValueId, ValueId, InstId)>::default();
        let mut projections = Vec::new();
        for inst in func.instructions() {
            let Some(result) = func.inst_result_value(inst) else { continue };
            let kind = &func.inst(inst).kind;
            if let InstKind::MakeSlice { ptr, len, .. } = *kind {
                components.insert(result, (ptr, len, inst));
            } else if let Some((slice, index)) = kind.slice_projection() {
                projections.push((result, slice, inst, index == MirType::SLICE_PTR_FIELD));
            }
        }

        let mut replacements = FxHashMap::default();
        let mut removed = FxHashSet::default();
        for (result, slice, inst, is_ptr) in projections {
            if let Some(&(ptr, len, _)) = components.get(&slice) {
                replacements.insert(result, if is_ptr { ptr } else { len });
            } else if let Value::Undef(MirType::Slice(location)) = *func.value(slice) {
                // extract_value undef, 0 -> undef pointer
                // extract_value undef, 1 -> undef i256
                let ty = MirType::slice_fields(location)[usize::from(!is_ptr)];
                replacements.insert(result, func.alloc_value(Value::Undef(ty)));
            } else {
                continue;
            }
            removed.insert(inst);
        }
        for (slice, (ptr, _, constructor)) in components {
            replacements.insert(slice, ptr);
            removed.insert(constructor);
        }
        if replacements.is_empty() {
            return false;
        }

        func.replace_uses_canonicalized(&replacements);
        for block in func.blocks.iter_mut() {
            block.instructions.retain(|inst| !removed.contains(inst));
        }
        true
    }

    fn run(module: &mut Module, analyses: &mut ModuleAnalyses) -> bool {
        if module.has_struct_values() {
            return false;
        }
        let compact = Self::infer_compact_params(module);
        let return_signatures: IndexVec<FunctionId, _> = module
            .functions
            .iter()
            .map(|func| {
                if func.selector.is_some() {
                    return None;
                }
                let signature = func
                    .return_components()
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
                Some(signature)
            })
            .collect();
        let signatures: IndexVec<FunctionId, _> = module
            .functions
            .iter_enumerated()
            .map(|(id, func)| {
                func.params
                    .iter_enumerated()
                    .map(|(index, ty)| match ty {
                        MirType::Slice(PtrLocation::Calldata) if compact.contains(&(id, index)) => {
                            ParamRepr::CompactCalldata
                        }
                        MirType::Slice(_) => ParamRepr::Pair,
                        _ => ParamRepr::Word,
                    })
                    .collect::<IndexVec<ArgIdx, _>>()
            })
            .collect();
        // Signature expansion rewrites call edges throughout the module. Prove every frame-address
        // shift first so an unrepresentable parsed-MIR offset leaves the complete module untouched.
        if module.functions.iter_enumerated().any(|(id, func)| {
            let added_slots = if func.selector.is_none()
                && !func.blocks.is_empty()
                && func.params.iter().any(Self::is_slice)
            {
                signatures[id].iter().filter(|&&repr| repr == ParamRepr::Pair).count()
            } else {
                0
            };
            let added_slots = added_slots
                + return_signatures[id]
                    .iter()
                    .flatten()
                    .filter(|&&repr| repr == ParamRepr::Pair)
                    .count();
            Self::shifted_frame_offsets(func, added_slots).is_none()
        }) {
            return false;
        }
        run_lowering_function_pass(module, analyses, |func, function_analyses| {
            let id = function_analyses.function();
            let mut changed = false;
            // Eliminate slice-typed `select`/`phi` first, so every remaining
            // slice is a `make_slice` result or a projection that the later
            // stages can expand or fold.
            changed |= Self::split_slice_aggregates(func);
            changed |= Self::expand_call_args(func, &signatures);
            changed |= Self::expand_call_returns(func, &return_signatures);
            changed |= Self::lower_external_args(func);
            if let Some(signature) = &return_signatures[id] {
                changed |= Self::lower_returns(func, signature);
            }
            changed |= Self::lower_params(func, &signatures[id]);
            changed |= Self::lower_projections(func);
            changed
        })
    }
}
