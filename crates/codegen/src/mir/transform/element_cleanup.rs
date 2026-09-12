//! Removal of element masks on arrays that only hold narrow words.
//!
//! A word loaded from a typed memory array is masked to its element type,
//! because inline assembly may have stored a dirty word there and solc masks
//! such reads the same way. When every word an array can hold fits the mask,
//! the mask is an identity. This pass bounds the width of the words each
//! array can hold and drops the masks that cover the bound, which removes a
//! mask and its constant from every element read of the address and
//! narrow-integer kernels.
//!
//! The bound is a least fixed point over the call graph. A function's store
//! bound is the widest word it stores into any array element, by the proved
//! width of the stored value; a raw memory write outside the compiler's
//! scratch region, an external call, or an explicit frame address makes it
//! the full word, since the address written is not tracked. A function's
//! transitive bound also covers the functions it calls. An array's origin
//! bound is zero for a zeroed allocation, the element width of an
//! externally callable function's array parameter whose ABI decoding
//! validates every element, the widest argument any call site passes for an
//! internal parameter (each argument bounded by its own origin and the
//! caller's transitive bound), the widest incoming array for a phi, and the
//! full word for anything else. A mask on an element load is dropped when
//! its width covers both the array's origin bound and the transitive bound
//! of the function reading it. Widths are tracked per function rather than
//! per array, so a store into any array of a function widens every array
//! that function reads; aliasing between arrays needs no separate proof.
//!
//! Only word-element arrays and contiguous low-bit masks are rewritten.
//! Runs early in the optimized phase, while element accesses are still
//! semantic and the call graph is explicit; removed masks lose their debug
//! checkpoints rather than lending them to the loads.

use super::egraph::max_bits_with_args;
use crate::mir::{
    AbiParamType, AbiWordValidator, AllocationInitialization, AllocationKind, ArgIdx, EffectKind,
    Function, FunctionId, InstId, InstKind, MemoryObjectKind, MemoryObjectLayout, MemoryRegion,
    MirType, Module, Terminator, Value, ValueId,
    pass::{MirPass, ModuleAnalyses},
    utils,
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
/// Fixed-point rounds after which every bound is given up as full.
const MAX_ROUNDS: usize = 16;

impl MirPass for ElementCleanup {
    fn name(&self) -> &'static str {
        "element-cleanup"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        _analyses: &mut ModuleAnalyses,
    ) -> bool {
        let transitive = transitive_bounds(module);
        let params = param_bounds(module, &transitive);
        let mut changed = false;
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
            let mut replacements = FxHashMap::default();
            let mut dead = DenseBitSet::new_empty(func.num_insts());
            for inst in func.instructions() {
                if let Some((element, bits)) = masked_element(func, inst)
                    && let Value::Inst(load) = func.value(element)
                    && let InstKind::MemoryObjectLoadElement { object, layout, .. } =
                        func.inst(*load).kind
                    && is_word_array(layout)
                    && objects.get(&object).is_some_and(|&origin| origin.max(reading) <= bits)
                    && let Some(result) = func.inst_result_value(inst)
                {
                    replacements.insert(result, element);
                    dead.insert(inst);
                }
            }
            if replacements.is_empty() {
                continue;
            }
            // result = and element, mask; use result -> use element
            // NOTE: Removed cleanup instructions lose their debug checkpoints;
            // their source locations must not be assigned to the load.
            func.for_each_instruction_mut(|_, inst| {
                inst.rewrite_operands(|value| {
                    *value = utils::resolve_replacement(*value, &replacements);
                });
            });
            for block in &mut func.blocks {
                block.instructions.retain(|&inst| !dead.contains(inst));
                if let Some(term) = &mut block.terminator {
                    utils::replace_terminator_uses_canonicalized(term, &replacements);
                }
            }
            changed = true;
        }
        changed
    }
}

fn is_word_array(layout: MemoryObjectLayout) -> bool {
    matches!(
        layout,
        MemoryObjectLayout::DynamicArray { element_words: 1 }
            | MemoryObjectLayout::FixedArray { element_words: 1, .. }
    )
}

fn is_array(ty: MirType) -> bool {
    matches!(
        ty,
        MirType::MemoryObject(MemoryObjectKind::DynamicArray | MemoryObjectKind::FixedArray)
    )
}

/// The value a contiguous low-bit mask keeps, and the mask's width in bits.
fn masked_element(func: &Function, inst: InstId) -> Option<(ValueId, u32)> {
    let InstKind::And(a, b) = func.inst(inst).kind else { return None };
    let (element, mask) = match (func.value_u256(a), func.value_u256(b)) {
        (None, Some(mask)) => (a, mask),
        (Some(mask), None) => (b, mask),
        _ => return None,
    };
    (mask.wrapping_add(U256::ONE) & mask == U256::ZERO).then(|| (element, mask.bit_len() as u32))
}

/// The widest word the function's own instructions may store into an array.
fn store_bound(func: &Function) -> u32 {
    let mut bound = 0;
    for inst in func.instructions() {
        let instruction = func.inst(inst);
        let width = match &instruction.kind {
            InstKind::MemoryObjectStoreElement { value, .. } => {
                max_bits_with_args(func, *value, MAX_VALUE_DEPTH, &|_| FULL_WIDTH)
            }
            // Other object kinds and compiler-owned words never overlap an
            // array element; zeroing keeps elements narrow.
            InstKind::MemoryObjectStoreField { .. }
            | InstKind::MemoryObjectStoreByte { .. }
            | InstKind::MemoryObjectStoreWord { .. }
            | InstKind::Alloc { .. }
            | InstKind::SetMemoryObjectLen(..)
            | InstKind::SetFmp(_)
            | InstKind::FrameStore { .. }
            | InstKind::MemoryZero(..) => 0,
            InstKind::MStore(..) | InstKind::MStore8(..) => {
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

fn callees(func: &Function) -> impl Iterator<Item = FunctionId> + '_ {
    let calls = func.instructions().filter_map(|inst| match func.inst(inst).kind {
        InstKind::ICall { function, .. } => Some(function),
        _ => None,
    });
    let tail_calls = func.blocks.iter().filter_map(|block| match block.terminator {
        Some(Terminator::TailCall { function, .. }) => Some(function),
        _ => None,
    });
    calls.chain(tail_calls)
}

/// Each function's store bound including the functions it calls.
fn transitive_bounds(module: &Module) -> IndexVec<FunctionId, u32> {
    let mut bounds: IndexVec<FunctionId, u32> = module.functions.iter().map(store_bound).collect();
    for _ in 0..MAX_ROUNDS {
        let mut changed = false;
        for (id, func) in module.functions.iter_enumerated() {
            let widened = callees(func).map(|callee| bounds[callee]).fold(bounds[id], u32::max);
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
        Some(AbiParamType::DynamicArray(element)) => element,
        Some(AbiParamType::FixedArray { element, .. }) => element,
        _ => return FULL_WIDTH,
    };
    match **element {
        AbiParamType::Scalar(ty) => match AbiWordValidator::from_mir_type(ty) {
            Some(AbiWordValidator::Unsigned(bits)) => u32::from(bits),
            // A full-width element has no mask to remove.
            None => FULL_WIDTH,
            _ => FULL_WIDTH,
        },
        _ => FULL_WIDTH,
    }
}

fn externally_callable(func: &Function) -> bool {
    func.selector.is_some()
        || func.is_public()
        || func.attributes.is_constructor
        || func.attributes.is_receive
        || func.attributes.is_fallback
}

/// The widest word each array parameter may hold on entry: what ABI
/// decoding admits, or the widest argument any call site passes.
fn param_bounds(
    module: &Module,
    transitive: &IndexVec<FunctionId, u32>,
) -> FxHashMap<(FunctionId, ArgIdx), u32> {
    let mut bounds = FxHashMap::default();
    for (id, func) in module.functions.iter_enumerated() {
        for (index, &ty) in func.params.iter_enumerated() {
            if !is_array(ty) {
                continue;
            }
            let entry = if !externally_callable(func) {
                0
            } else if func.selector.is_some() {
                validated_width(func, index)
            } else {
                FULL_WIDTH
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
                    let passed = objects.get(&arg).copied().unwrap_or(FULL_WIDTH).max(reading);
                    if passed > bound {
                        bounds.insert(key, passed);
                        changed = true;
                    }
                }
            };
            for inst in func.instructions() {
                if let InstKind::ICall { function, args, .. } = &func.inst(inst).kind {
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
    bounds.values_mut().for_each(|bound| *bound = FULL_WIDTH);
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
        if let Value::Arg(arg) = func.value(value)
            && let Some(&bound) = params.get(&(id, *arg))
        {
            bounds.insert(value, bound);
        }
    }
    let mut phis = Vec::new();
    for inst in func.instructions() {
        let Some(result) = func.inst_result_value(inst) else { continue };
        match &func.inst(inst).kind {
            InstKind::Alloc { kind: AllocationKind::Object(layout), semantics, .. }
                if is_word_array(*layout)
                    && semantics.initialization == AllocationInitialization::Zeroed =>
            {
                bounds.insert(result, 0);
            }
            InstKind::Phi(incoming) if func.value_ty(result).is_some_and(is_array) => {
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
                .map(|value| bounds.get(value).copied().unwrap_or(FULL_WIDTH))
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
        bounds.insert(*result, FULL_WIDTH);
    }
    bounds
}
