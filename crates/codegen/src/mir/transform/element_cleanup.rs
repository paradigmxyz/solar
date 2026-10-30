//! Removal of element masks on arrays that only hold narrow words.
//!
//! A word loaded from a typed memory array is masked to its element type,
//! because inline assembly may have stored a dirty word there. When every
//! word an array can hold fits the mask, the mask is an identity. This pass
//! bounds the width of the words each array can hold and drops the masks
//! that cover the bound, which removes a mask and its constant from every
//! element read of the address and narrow-integer kernels.
//!
//! The bound is a least fixed point over the call graph. A function's store
//! bound is the widest word it stores into any array element, by the proved
//! width of the stored value, provided the array has a known word-aligned origin.
//! Other object stores, unknown array pointers, raw writes outside scratch memory,
//! external calls, and explicit frame addresses widen the bound to the full word,
//! since the address written is not tracked. A function's
//! transitive bound also covers the functions it calls. An array's origin
//! bound is zero for a zeroed allocation, the element width of an
//! externally callable function's array parameter whose ABI decoding
//! validates every element, the widest argument any call site passes for an
//! internal parameter (each argument bounded by its own origin and the
//! caller's transitive bound), the widest incoming array for a phi, the width
//! of a storage-array load's validated unsigned elements, and the full word
//! for any other object ABI decoding produces. A pointer nothing tracks, which inline
//! assembly may have offset into another array's words, gets a bound past the full word.
//! Storage-array loads initialize fresh private arrays.
//! A mask on an element load is dropped when its width covers both the array's
//! origin bound and the transitive bound
//! of the function reading it. Widths are tracked per function rather than
//! per array, so a store into any array of a function widens every array
//! that function reads. Provisional width summaries establish array origins first;
//! a second summary includes stores through unknown origins before any masks are removed.
//!
//! Only word-element arrays, contiguous low-bit masks, and `zext iN (trunc i256 x to iN) to
//! i256` round trips are rewritten. An unsigned comparison of such a narrowing compares the
//! element word instead, with its other operand zero-extended, which is free after integer
//! lowering. A narrowing result with other typed uses stays in place.
//! Runs early in the optimized phase, while `ptradd` keeps element addresses
//! explicit and the call graph is explicit; removed masks lose their debug
//! checkpoints rather than lending them to the loads.

use super::egraph::max_bits_with_args;
use crate::mir::{
    AbiParamType, AbiWordValidator, AllocationInitialization, ArgIdx, BlockId, EffectKind,
    Function, FunctionId, Immediate, InstId, InstKind, Instruction, MemoryRegion, MirType, Module,
    PtrLocation, Terminator, Value, ValueId,
    analysis::direct_callees,
    memory::EvmMemoryLayout,
    pass::{MirPass, ModuleAnalyses},
};
use alloy_primitives::U256;
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::{IndexVec, index_vec},
    map::FxHashMap,
};

/// Removes element masks that cover the width bound of their array.
pub(crate) struct ElementCleanup;

/// SSA definitions followed when bounding a stored word's width.
const MAX_VALUE_DEPTH: u32 = 8;
/// The width of a word nothing bounds.
const FULL_WIDTH: u32 = 256;
/// The origin bound of a pointer nothing tracks: past every width, as it may point into the middle
/// of another array's words, unlike an object's own start.
const UNKNOWN_ORIGIN: u32 = FULL_WIDTH + 1;
/// Fixed-point rounds after which every bound is given up as full.
const MAX_ROUNDS: usize = 16;

impl MirPass for ElementCleanup {
    fn name(&self) -> &'static str {
        "element-cleanup"
    }

    fn is_contract_wide(&self) -> bool {
        true
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        _analyses: &mut ModuleAnalyses,
    ) -> bool {
        let initial = transitive_bounds(module, None);
        let mut params = param_bounds(module, &initial);
        // Narrow origins also prove that array stores use ordinary word boundaries.
        // Cast-derived objects can instead write across another array's elements.
        let transitive = transitive_bounds(module, Some(&params));
        if transitive != initial {
            params = param_bounds(module, &transitive);
        }
        if tracing::enabled!(tracing::Level::TRACE) {
            for (id, func) in module.functions.iter_enumerated() {
                tracing::trace!(
                    function = %func.name,
                    store = store_bound(func, Some(&object_bounds(func, id, &params))),
                    transitive = transitive[id],
                    params = ?func
                        .params
                        .indices()
                        .filter_map(|index| params.get(&(id, index)).map(|bound| (index, *bound)))
                        .collect::<Vec<_>>(),
                    "element bounds"
                );
            }
        }
        let mut changed = false;
        let mut replacements = FxHashMap::default();
        let mut dead = DenseBitSet::new_empty(0);
        for (id, func) in module.functions.iter_mut_enumerated() {
            let reading = transitive[id];
            if reading >= FULL_WIDTH {
                continue;
            }
            // The ABI return proofs run after the masks are gone: leave them
            // the bound of every array parameter this function reads.
            for (index, _) in func.params.iter_enumerated() {
                if let Some(&origin) = params.get(&(id, index))
                    && origin.max(reading) < FULL_WIDTH
                {
                    func.attributes.array_element_bits.insert(index, origin.max(reading));
                }
            }
            let objects = object_bounds(func, id, &params);
            // Publish the width of an array this function returns, so its caller can
            // re-encode the payload in one copy. A fresh array holds what this function
            // and its callees store; a returned parameter holds that and what it arrived
            // with. Several return blocks must all stay within the bound.
            if func.return_components().len() == 1 {
                let returned = func
                    .blocks
                    .iter()
                    .filter_map(|block| match &block.terminator {
                        Some(Terminator::Return { values }) => Some(values.first().copied()),
                        _ => None,
                    })
                    .map(|value| {
                        value
                            .and_then(|value| objects.get(&value).copied())
                            .unwrap_or(FULL_WIDTH)
                            .max(reading)
                    })
                    .max();
                tracing::trace!(function = %func.name, ?returned, reading, "array return bits");
                if let Some(bound) = returned.filter(|bound| *bound < FULL_WIDTH) {
                    func.attributes.array_return_element_bits = Some(bound);
                }
            }
            let uses = super::egraph::use_counts(func);
            replacements.clear();
            dead.clear_to(func.num_insts());
            for inst in func.instructions() {
                if let Some((element, bits)) = masked_element(func, inst)
                    && let Value::Inst(load) = func.value(element)
                    && let Some(object) = word_element_object(func, *load)
                    && covers(&objects, object, reading, bits)
                    && let Some(result) = func.inst_result_value(inst)
                {
                    // zext iN (trunc i256 element to iN) to i256 -> element
                    if let InstKind::Zext(narrow) = func.inst(inst).kind
                        && uses[narrow] == 1
                        && let Value::Inst(trunc) = func.value(narrow)
                    {
                        dead.insert(*trunc);
                    }
                    // and element, mask -> element
                    replacements.insert(result, element);
                    dead.insert(inst);
                }
            }
            if !replacements.is_empty() {
                // result = and element, mask; use result -> use element
                // NOTE: Removed cleanup instructions lose their debug checkpoints;
                // their source locations must not be assigned to the load.
                func.replace_uses_canonicalized(&replacements);
                for block in &mut func.blocks {
                    block.instructions.retain(|&inst| !dead.contains(inst));
                }
                changed = true;
            }
            changed |= widen_comparisons(func, &objects, reading);
        }
        changed
    }
}

/// Compares the words of narrowed elements instead of the narrowed values. An unsigned comparison
/// of `trunc i256 x to iN`, where `x` is an element of an array that holds no word wider than N
/// bits, has the result the same comparison of `x` has with the other operand zero-extended, so
/// the narrowing and the mask it lowers to go once nothing else uses them. Signed comparisons
/// read bit N - 1 as the sign and stay.
fn widen_comparisons(func: &mut Function, objects: &FxHashMap<ValueId, u32>, reading: u32) -> bool {
    let mut changed = false;
    for block in func.blocks.indices() {
        let mut position = 0;
        while position < func.blocks[block].instructions.len() {
            let inst = func.blocks[block].instructions[position];
            let kind = &func.inst(inst).kind;
            let (InstKind::Eq(a, b) | InstKind::Ne(a, b) | InstKind::Lt(a, b) | InstKind::Gt(a, b)) =
                *kind
            else {
                position += 1;
                continue;
            };
            let (wide_a, wide_b) = (
                clean_element(func, a, objects, reading),
                clean_element(func, b, objects, reading),
            );
            if wide_a.is_none() && wide_b.is_none() {
                position += 1;
                continue;
            }
            let mut widen = |func: &mut Function, value: ValueId, wide: Option<ValueId>| {
                if let Some(wide) = wide {
                    return wide;
                }
                if let Some(constant) = func.value_u256(value) {
                    let immediate = Immediate::for_type(Some(MirType::I256), constant);
                    return func.alloc_value(Value::Immediate(immediate));
                }
                // zext iN value to i256
                let mut zext = Instruction::new(InstKind::Zext(value), Some(MirType::I256));
                zext.metadata = func.inst(inst).metadata.debug_context();
                let (zext, result) = func.alloc_value_inst(zext);
                func.blocks[block].instructions.insert(position, zext);
                position += 1;
                result
            };
            let (a, b) = (widen(func, a, wide_a), widen(func, b, wide_b));
            // cmp iN (trunc i256 x to iN), y -> cmp i256 x, (zext iN y to i256)
            let kind = match func.inst(inst).kind {
                InstKind::Eq(..) => InstKind::Eq(a, b),
                InstKind::Ne(..) => InstKind::Ne(a, b),
                InstKind::Lt(..) => InstKind::Lt(a, b),
                _ => InstKind::Gt(a, b),
            };
            func.inst_mut(inst).replace_kind(kind);
            changed = true;
            position += 1;
        }
    }
    changed
}

/// The word `value` narrows when it truncates an element of an array whose words all fit the
/// narrowed width.
fn clean_element(
    func: &Function,
    value: ValueId,
    objects: &FxHashMap<ValueId, u32>,
    reading: u32,
) -> Option<ValueId> {
    let Value::Inst(trunc) = *func.value(value) else { return None };
    let InstKind::Trunc(element, bits) = func.inst(trunc).kind else { return None };
    let Value::Inst(load) = *func.value(element) else { return None };
    let object = word_element_object(func, load)?;
    covers(objects, object, reading, bits).then_some(element)
}

/// Whether every word of the array `object` fits `bits`, given the width `reading` of what the
/// function's callees store.
fn covers(objects: &FxHashMap<ValueId, u32>, object: ValueId, reading: u32, bits: u32) -> bool {
    objects.get(&object).is_some_and(|&origin| origin.max(reading) <= bits)
}

/// The array a word element load or store accesses: a `ptr<memory>` object at a word-aligned
/// offset. The word at a dynamic object's pointer is its length, not an element, so a constant
/// offset of zero counts only in a constant-size allocation, whose first word, length or not,
/// is tracked like any element. Every fixed array and struct has a constant size, so only a
/// runtime-sized allocation is known to start with a length word.
fn word_element_object(func: &Function, inst: InstId) -> Option<ValueId> {
    let (InstKind::Load(address, PtrLocation::Memory)
    | InstKind::Store(address, _, PtrLocation::Memory)) = func.inst(inst).kind
    else {
        return None;
    };
    let (object, offset, index) = func.word_access(address)?;
    let length = index.is_none() && offset == 0 && func.alloc_size(object).is_none();
    (!length && func.value_ty(object) == Some(MirType::MEMPTR)).then_some(object)
}

/// Whether storing `length` into the header of `object` keeps every element read inside the
/// allocation: `object` is a direct runtime-sized allocation, and `length` is the length its
/// checked size scales from, as in `new T[](length)`. Any other length, such as one assembly
/// stores, may let an index pass the bounds check past the allocation, or wrap around to the
/// header, and read words the width bounds never saw.
fn is_header_length(func: &Function, object: ValueId, length: ValueId) -> bool {
    let Value::Inst(alloc) = *func.value(object) else { return false };
    let InstKind::Alloc { size, .. } = func.inst(alloc).kind else { return false };
    func.value_u64(size).is_none() && scaled_root(func, size) == length
}

/// The value `value` scales from through additions, multiplications, and shifts by constants.
fn scaled_root(func: &Function, mut value: ValueId) -> ValueId {
    for _ in 0..MAX_VALUE_DEPTH {
        let Value::Inst(inst) = *func.value(value) else { break };
        value = match func.inst(inst).kind {
            InstKind::Add(a, b) | InstKind::Mul(a, b) if func.value_u256(b).is_some() => a,
            InstKind::Add(a, b) | InstKind::Mul(a, b) if func.value_u256(a).is_some() => b,
            InstKind::Shl(shift, x) if func.value_u256(shift).is_some() => x,
            _ => break,
        };
    }
    value
}

/// The value a contiguous low-bit mask keeps, and the mask's width in bits.
fn masked_element(func: &Function, inst: InstId) -> Option<(ValueId, u32)> {
    if let InstKind::Zext(narrow) = func.inst(inst).kind
        && func.inst(inst).result_ty == Some(MirType::I256)
        && let Value::Inst(trunc) = func.value(narrow)
        && let InstKind::Trunc(element, bits) = func.inst(*trunc).kind
    {
        return Some((element, bits));
    }
    let InstKind::And(a, b) = func.inst(inst).kind else { return None };
    let (element, mask) = match (func.value_u256(a), func.value_u256(b)) {
        (None, Some(mask)) => (a, mask),
        (Some(mask), None) => (b, mask),
        _ => return None,
    };
    (mask.wrapping_add(U256::ONE) & mask == U256::ZERO).then(|| (element, mask.bit_len() as u32))
}

/// The widest word the function's own instructions may store into an array.
fn store_bound(func: &Function, objects: Option<&FxHashMap<ValueId, u32>>) -> u32 {
    let mut bound = 0;
    let instructions = func
        .blocks
        .iter_enumerated()
        .flat_map(|(block, data)| data.instructions.iter().map(move |&inst| (block, inst)));
    for (block, inst) in instructions {
        let instruction = func.inst(inst);
        let width = match &instruction.kind {
            InstKind::Store(.., PtrLocation::Memory)
                if func
                    .memory_length_store(inst)
                    .is_some_and(|(object, length)| is_header_length(func, object, length)) =>
            {
                0
            }
            // The replaced word must be one of an object's own words, such as an array's header or
            // element: a store one byte past an element moves its low byte into the next
            // element's top byte.
            InstKind::Store(address, value, PtrLocation::Memory)
                if func.word_access(*address).is_some_and(|(object, ..)| {
                    func.value_ty(object) == Some(MirType::MEMPTR) && known_origin(objects, object)
                }) && replaces_no_narrower_word(func, block, inst, *address, *value) =>
            {
                0
            }
            InstKind::Store(_, value, PtrLocation::Memory)
                if word_element_object(func, inst)
                    .is_some_and(|object| narrow_origin(objects, object)) =>
            {
                max_bits_with_args(func, *value, MAX_VALUE_DEPTH, &|_| FULL_WIDTH)
            }
            // Only compiler-owned words and zeroing are harmless without an alias proof.
            InstKind::Alloc { .. }
            | InstKind::FrameStore { .. }
            | InstKind::MemoryZero(..)
            | InstKind::StorageArrayLoad { .. } => 0,
            InstKind::Store(.., PtrLocation::Memory) | InstKind::Store8(..) => {
                if instruction.metadata.memory_region() == Some(MemoryRegion::Scratch) {
                    0
                } else {
                    FULL_WIDTH
                }
            }
            InstKind::InternalFrameAddr(_)
            | InstKind::Call { .. }
            | InstKind::CallCode { .. }
            | InstKind::StaticCall { .. }
            | InstKind::DelegateCall { .. } => FULL_WIDTH,
            kind if kind.effect_kind() == EffectKind::MemoryWrite => FULL_WIDTH,
            _ => 0,
        };
        bound = bound.max(width);
        if bound >= FULL_WIDTH {
            break;
        }
    }
    bound
}

/// Whether `object` is an array of a known origin that holds words narrower than a word. Without
/// `objects`, the provisional summary takes every object as one.
fn narrow_origin(objects: Option<&FxHashMap<ValueId, u32>>, object: ValueId) -> bool {
    objects.is_none_or(|objects| objects.get(&object).is_some_and(|&bound| bound < FULL_WIDTH))
}

/// Whether `object` is the start of an object of a known origin, built or decoded here or passed
/// by every call site as one, rather than a pointer assembly may have offset into another array's
/// words. Without `objects`, the provisional summary takes every object as one.
fn known_origin(objects: Option<&FxHashMap<ValueId, u32>>, object: ValueId) -> bool {
    objects.is_none_or(|objects| objects.get(&object).is_some_and(|&bound| bound <= FULL_WIDTH))
}

/// Whether the store `inst` of `value` at `address` leaves a word no wider than the one it
/// replaces: on the only path to the store, a branch found `value` at most the word a load read
/// from `address`, and nothing writes memory between that load and the store. Whatever array
/// holds the word keeps its bound. A guarded truncation, `if (n > a.length) fail(); a.length = n`,
/// stores such a word into an array parameter's header.
fn replaces_no_narrower_word(
    func: &Function,
    mut block: BlockId,
    inst: InstId,
    address: ValueId,
    value: ValueId,
) -> bool {
    let reads_only = |inst: InstId| {
        matches!(func.inst(inst).kind.effect_kind(), EffectKind::Pure | EffectKind::MemoryRead)
    };
    let before = &func.blocks[block].instructions;
    if !before.iter().take_while(|&&other| other != inst).all(|&other| reads_only(other)) {
        return false;
    }
    let value = through_single_phis(func, value);
    for _ in 0..MAX_VALUE_DEPTH {
        let [predecessor] = *func.blocks[block].predecessors.as_slice() else { return false };
        let data = &func.blocks[predecessor];
        match data.terminator {
            Some(Terminator::Jump(_))
                if data.instructions.iter().all(|&other| reads_only(other)) =>
            {
                block = predecessor;
            }
            Some(Terminator::Branch { condition, then_block, else_block })
                if then_block != else_block =>
            {
                let Value::Inst(test) = *func.value(condition) else { return false };
                // On the else edge `a > b` and `a < b` fail; on the then edge they hold.
                let (small, large) = match (&func.inst(test).kind, block == else_block) {
                    (&InstKind::Gt(a, b), true) | (&InstKind::Lt(a, b), false) => (a, b),
                    (&InstKind::Lt(a, b), true) | (&InstKind::Gt(a, b), false) => (b, a),
                    _ => return false,
                };
                if through_single_phis(func, small) != value {
                    return false;
                }
                let Value::Inst(load) = *func.value(large) else { return false };
                let InstKind::Load(loaded, PtrLocation::Memory) = func.inst(load).kind else {
                    return false;
                };
                let same_address = loaded == address
                    || func
                        .object_address(loaded)
                        .is_some_and(|loaded| func.object_address(address) == Some(loaded));
                let Some(position) = data.instructions.iter().position(|&other| other == load)
                else {
                    return false;
                };
                return same_address
                    && data.instructions[position + 1..].iter().all(|&other| reads_only(other));
            }
            _ => return false,
        }
    }
    false
}

/// The value `value` stands for through phis whose incoming values are all one value.
fn through_single_phis(func: &Function, mut value: ValueId) -> ValueId {
    for _ in 0..MAX_VALUE_DEPTH {
        let Value::Inst(inst) = *func.value(value) else { break };
        let InstKind::Phi(incoming) = &func.inst(inst).kind else { break };
        let Some(&(_, first)) = incoming.first() else { break };
        if incoming.iter().any(|&(_, other)| other != first) {
            break;
        }
        value = first;
    }
    value
}

/// Each function's store bound including the functions it calls.
fn transitive_bounds(
    module: &Module,
    params: Option<&FxHashMap<(FunctionId, ArgIdx), u32>>,
) -> IndexVec<FunctionId, u32> {
    let mut bounds: IndexVec<FunctionId, u32> = module
        .functions
        .iter_enumerated()
        .map(|(id, func)| {
            let objects = params.map(|params| object_bounds(func, id, params));
            store_bound(func, objects.as_ref())
        })
        .collect();
    for _ in 0..MAX_ROUNDS {
        let mut changed = false;
        for (id, func) in module.functions.iter_enumerated() {
            let widened =
                direct_callees(func).map(|callee| bounds[callee]).fold(bounds[id], u32::max);
            if widened != bounds[id] {
                bounds[id] = widened;
                changed = true;
            }
        }
        if !changed {
            return bounds;
        }
    }
    index_vec![FULL_WIDTH; module.functions.len()]
}

/// The element width an externally callable function's ABI decoding
/// validates for an array parameter, or the full word.
fn validated_width(func: &Function, index: ArgIdx) -> u32 {
    let element = match func.abi_params.as_ref().and_then(|layout| layout.types.get(index.index()))
    {
        Some(AbiParamType::DynamicArray(element) | AbiParamType::FixedArray { element, .. }) => {
            element
        }
        _ => return FULL_WIDTH,
    };
    // A full-width element has no mask to remove.
    if let AbiParamType::Scalar(ty) = **element
        && let Some(AbiWordValidator::Unsigned(bits)) = AbiWordValidator::from_layout(ty)
    {
        u32::from(bits)
    } else {
        FULL_WIDTH
    }
}

/// The widest word each pointer parameter's array elements may hold on entry:
/// what ABI decoding admits, or the widest argument any call site passes.
fn param_bounds(
    module: &Module,
    transitive: &IndexVec<FunctionId, u32>,
) -> FxHashMap<(FunctionId, ArgIdx), u32> {
    let mut bounds = FxHashMap::default();
    for (id, func) in module.functions.iter_enumerated() {
        for (index, &ty) in func.params.iter_enumerated() {
            if ty != MirType::MEMPTR {
                continue;
            }
            let entry = if func.selector.is_some() {
                validated_width(func, index)
            } else if func.is_external_entry() || func.is_public() {
                UNKNOWN_ORIGIN
            } else {
                0
            };
            bounds.insert((id, index), entry);
        }
    }
    for _ in 0..MAX_ROUNDS {
        let mut changed = false;
        for (caller, func) in module.functions.iter_enumerated() {
            let objects = object_bounds(func, caller, &bounds);
            let reading = transitive[caller];
            let mut visit = |callee: FunctionId, args: &[ValueId]| {
                for (index, &arg) in args.iter().enumerate() {
                    let key = (callee, ArgIdx::new(index));
                    let Some(bound) = bounds.get(&key).copied() else { continue };
                    let passed = objects.get(&arg).copied().unwrap_or(UNKNOWN_ORIGIN).max(reading);
                    if passed > bound {
                        bounds.insert(key, passed);
                        changed = true;
                    }
                }
            };
            for inst in func.instructions() {
                if let InstKind::ICall {
                    function: crate::mir::Callee::Function(function),
                    args,
                    ..
                } = &func.inst(inst).kind
                {
                    visit(*function, args);
                }
            }
            for block in &func.blocks {
                if let Some(Terminator::TailCall { function, args }) = &block.terminator {
                    visit(*function, args);
                }
            }
        }
        if !changed {
            return bounds;
        }
    }
    bounds.values_mut().for_each(|bound| *bound = UNKNOWN_ORIGIN);
    bounds
}

/// The origin bound of every array value in a function: parameters,
/// zeroed allocations, and phis over those.
fn object_bounds(
    func: &Function,
    id: FunctionId,
    params: &FxHashMap<(FunctionId, ArgIdx), u32>,
) -> FxHashMap<ValueId, u32> {
    let mut bounds = FxHashMap::default();
    for index in 0..func.num_values() {
        let value = ValueId::new(index);
        match func.value(value) {
            Value::Arg(arg) => {
                if let Some(&bound) = params.get(&(id, *arg)) {
                    bounds.insert(value, bound);
                }
            }
            // The canonical empty array lives at the zero slot and holds no elements.
            Value::Immediate(_) if func.value_u64(value) == Some(EvmMemoryLayout::ZERO_SLOT) => {
                bounds.insert(value, 0);
            }
            _ => {}
        }
    }
    let mut phis = Vec::new();
    for inst in func.instructions() {
        let Some(result) = func.inst_result_value(inst) else { continue };
        match &func.inst(inst).kind {
            InstKind::Alloc { semantics, .. }
                if semantics.initialization == AllocationInitialization::Zeroed =>
            {
                bounds.insert(result, 0);
            }
            InstKind::StorageArrayLoad { element, .. } => {
                if let Some(AbiWordValidator::Unsigned(bits)) =
                    AbiWordValidator::from_layout(*element)
                {
                    bounds.insert(result, u32::from(bits));
                }
            }
            // Every phi joins, not only the array-typed ones: lowering types a returned
            // array as the raw pointer it is, and an object reaches its uses through
            // those. A phi whose incoming values are not tracked still joins to the full
            // width, so this only adds entries the join can prove.
            InstKind::Phi(incoming) => {
                bounds.insert(result, 0);
                phis.push((result, incoming.iter().map(|&(_, value)| value).collect::<Vec<_>>()));
            }
            _ => {}
        }
    }
    // A phi holds whatever its widest incoming array holds.
    for _ in 0..MAX_ROUNDS {
        let mut changed = false;
        for (result, incoming) in &phis {
            let widest = incoming
                .iter()
                .map(|value| bounds.get(value).copied().unwrap_or(UNKNOWN_ORIGIN))
                .fold(0, u32::max);
            if widest > bounds[result] {
                bounds.insert(*result, widest);
                changed = true;
            }
        }
        if !changed {
            return bounds;
        }
    }
    for (result, _) in &phis {
        bounds.insert(*result, UNKNOWN_ORIGIN);
    }
    bounds
}
