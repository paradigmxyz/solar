//! Lower semantic ABI encoding operations to memory and slice operations.
//!
//! After the main optimization pipeline, expand each `abi_encode` from its ABI
//! layout into head stores and dynamic-tail copies, reusing outlined helpers when
//! supplied by the lowering plan. Literal objects are kept until their projections
//! can be folded. Generated stores inherit the encoding operation's source context;
//! any block split moves the original terminator and its metadata together.
//! Constructor-reachable encoders stay inline because their output is not reserved
//! until encoding finishes, and a dynamic call frame would overlap that output.
//! Dynamic tuples are written at the current heap frontier before their size is
//! known. Their final reservation must retain that exact address even if later
//! simplification makes the size constant; static placement would detach the
//! result from the stores that already initialized it.
//! Tuples with known literal byte tails can instead reserve their full output
//! before encoding, keeping stores tied to the allocated base. This is limited
//! to nonempty payloads: empty tails already have a short encoder, and moving
//! their reservation earlier can increase stack setup around external calls.
//! Word-cleanup loops use a guarded do-while with a destination cursor and fixed
//! source displacement, returning the final cursor instead of retaining a
//! separate tail. The original count controls iteration even if addresses wrap;
//! loads, cleanup checks, and stores remain in their original order. Small
//! constant allocations retain the counted loop because rotation's extra entry
//! guard and exit phi can grow short encodings in callers with live arguments.
//! Byte tails zero their final padded word before writing the length header.
//! For an empty tail these addresses coincide, and the header store restores its
//! length. This removes a branch without touching memory beyond the encoded tail.
//! Size mode retains the guarded padding store: its encoder tails share better
//! in the machine outlining pipeline on large ABI-heavy contracts.
//! A terminal external return of a freshly allocated full-word array or byte
//! string is encoded in place. Moving its payload forward by one word makes
//! room for the ABI tuple offset and reuses the object's length word, avoiding
//! a second allocation. A module-wide plan selects those sites before shared
//! tuple helpers are built, so locally cheaper terminal encodings do not leave
//! an unused shared body.

use crate::{
    mir::{
        AbiEncodeMode, AbiLayout, AbiType, AbiWordValidator, AllocationKind, BlockId, EffectKind,
        Function, FunctionBuilder, FunctionId, InstId, InstKind, InstructionMetadata,
        MemoryObjectKind, MemoryObjectLayout, MirType, Module, RevertReason, SliceLocation,
        Terminator, Value, ValueId, analysis::CallGraphInfo, memory::EvmMemoryLayout,
        pass::MirPass, transform::utils::redirect_successor_predecessors,
        utils::resolve_replacement,
    },
    target::{Cost, Target},
};

use alloy_primitives::U256;
use solar_config::RevertStrings;
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::IndexVec,
    map::{FxHashMap, FxHashSet},
};
use solar_interface::{Ident, sym};
use solar_sema::Gcx;

/// Lowers `abi_encode` after the main optimization pipeline.
pub(crate) struct LowerAbiEncode;

impl MirPass for LowerAbiEncode {
    fn name(&self) -> &'static str {
        "lower-abi-encode"
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(
        &self,
        gcx: Gcx<'_>,
        module: &mut Module,
        _analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        let revert_strings = gcx.sess.opts.revert_strings;
        let target = Target::new(gcx);
        let (fresh_object_returns, mut destructive_returns) =
            plan_destructive_terminal_returns(target, module);
        let mut helpers = synthesize_array_helpers(target, module, revert_strings);
        synthesize_tuple_helpers(
            target,
            module,
            &mut helpers,
            revert_strings,
            &destructive_returns,
        );
        destructive_returns.resize_with(module.functions.len(), FxHashSet::default);
        let call_graph = CallGraphInfo::new(module);
        let mut constructors = call_graph.reachable_callees_from(
            module
                .functions
                .iter_enumerated()
                .filter_map(|(id, func)| func.attributes.is_constructor.then_some(id)),
        );
        for (id, func) in module.functions.iter_enumerated() {
            if func.attributes.is_constructor {
                constructors.insert(id);
            }
        }
        let mut inline_helpers = EncodeHelpers {
            branchless_byte_tails: !target.optimization().is_size(),
            ..EncodeHelpers::default()
        };
        let mut changed = !helpers.arrays.is_empty() || !helpers.tuples.is_empty();
        for (func_id, func) in module.functions.iter_mut_enumerated() {
            // NOTE: Constructor calls allocate frames at the free-memory pointer. Encoding
            // writes there before reserving its output, so an outlined call would overwrite it.
            // abi_encode(args) => inline head stores and tail copies
            let helpers =
                if constructors.contains(func_id) { &mut inline_helpers } else { &mut helpers };
            changed |= lower_function(
                func,
                helpers,
                revert_strings,
                &fresh_object_returns,
                &destructive_returns[func_id],
            );
        }
        changed
    }
}

/// A dynamic memory array layout whose element-wise encoding loop several sites share as one
/// helper function, like solc's per-type `abi_encode_t_array` functions. The key carries the
/// MIR type the sites pass so the helper's parameter matches it.
#[derive(Clone, PartialEq, Eq, Hash)]
struct ArrayHelperKey {
    element: AbiType,
    value_ty: MirType,
}

/// A whole encoding several sites share as one helper function, like solc's per-signature
/// `abi_encode_tuple` functions: the same layout, storage policy, selector presence, and MIR
/// types of the values passed.
#[derive(Clone, PartialEq, Eq, Hash)]
struct TupleHelperKey {
    mode: AbiEncodeMode,
    selector: bool,
    types: Box<[AbiType]>,
    arg_types: Box<[MirType]>,
}

impl TupleHelperKey {
    fn of(
        func: &Function,
        mode: AbiEncodeMode,
        selector: Option<ValueId>,
        args: &[ValueId],
        layout: &AbiLayout,
    ) -> Self {
        Self {
            mode,
            selector: selector.is_some(),
            types: layout.types.clone(),
            arg_types: args
                .iter()
                .map(|&arg| func.value_ty(arg).unwrap_or_else(MirType::uint256))
                .collect(),
        }
    }
}

/// Shared encoder helpers available to every site in the module.
#[derive(Default)]
struct EncodeHelpers {
    arrays: FxHashMap<ArrayHelperKey, FunctionId>,
    tuples: FxHashMap<TupleHelperKey, FunctionId>,
    /// Proven on the original function, before encoding emits raw memory operations.
    literal_objects: FxHashSet<ValueId>,
    branchless_byte_tails: bool,
}

/// Builds `encode_abi_tuple(args.., [selector]) -> encoded` for every encoding shape at least
/// two sites share, when the objective ranks the calls, with the protocol gas they run, above
/// the expanded copies: under the size objective the sites call one body, under the gas
/// objective each keeps its own.
fn synthesize_tuple_helpers(
    target: Target,
    module: &mut Module,
    helpers: &mut EncodeHelpers,
    revert_strings: RevertStrings,
    destructive_returns: &IndexVec<FunctionId, FxHashSet<InstId>>,
) {
    let mut counts = FxHashMap::<TupleHelperKey, (usize, usize)>::default();
    for (func_id, func) in module.functions.iter_enumerated() {
        for inst in func.instructions() {
            let InstKind::AbiEncode { mode, selector, args, layout } = &func.inst(inst).kind else {
                continue;
            };
            if destructive_returns[func_id].contains(&inst) {
                continue;
            }
            let next = counts.len();
            let count = counts
                .entry(TupleHelperKey::of(func, *mode, *selector, args, layout))
                .or_insert((0, next));
            count.0 += 1;
        }
    }
    let mut keys = counts
        .into_iter()
        .filter(|(_, (count, _))| *count >= 2)
        .map(|(key, (count, first))| (first, count, key))
        .collect::<Vec<_>>();
    keys.sort_by_key(|(first, _, _)| *first);

    let mut shared = Vec::new();
    for (_, sites, key) in keys {
        let mut function = Function::new(Ident::with_dummy_span(sym::encode_abi_tuple));
        {
            let mut builder =
                FunctionBuilder::new(&mut function).with_revert_strings(revert_strings);
            let args = key.arg_types.iter().map(|ty| builder.add_param(*ty)).collect::<Vec<_>>();
            let selector = key.selector.then(|| builder.add_param(MirType::uint256()));
            let layout = AbiLayout::new(key.types.clone());
            let encoded = lower_encode(&mut builder, &layout, selector, &args, key.mode, helpers);
            let result_ty = builder.func().value_ty(encoded).unwrap_or_else(MirType::uint256);
            builder.add_return(result_ty);
            builder.ret([encoded]);
        }
        let params = function.params.len();
        let body = target.code_estimate(&function);
        let sites = u32::try_from(sites).unwrap_or(u32::MAX);
        // The encoding runs at every site in both shapes; the call protocol is the price of
        // sharing it, the copies of the body the price of expanding it.
        let frame_words = EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE / EvmMemoryLayout::WORD_SIZE
            + params as u64
            + 1;
        let call = target.icall(params, 1, frame_words);
        let ret = target.internal_return(params, 1);
        let with_helper = Cost::new(0, body.bytes).plus(ret).plus(call.times(sites));
        let expanded = Cost::new(0, body.bytes.saturating_mul(sites));
        if target.cmp(with_helper, expanded).is_lt() {
            shared.push((key, function));
        }
    }
    for (key, function) in shared {
        let helper = module.add_function(function);
        helpers.tuples.insert(key, helper);
    }
}

/// Builds `encode_abi_array(value, dest) -> tail` for every memory array layout whose
/// element-wise loop at least two sites would otherwise expand inline. Inner layouts are built
/// first so an outer helper's element encoding calls the inner helper.
fn synthesize_array_helpers(
    target: Target,
    module: &mut Module,
    revert_strings: RevertStrings,
) -> EncodeHelpers {
    fn count_sites(
        func: &Function,
        ty: &AbiType,
        value: Option<ValueId>,
        occurrences: usize,
        counts: &mut FxHashMap<ArrayHelperKey, (usize, usize)>,
    ) {
        match ty {
            AbiType::DynamicArray { element, location } => {
                let location = value
                    .map_or(*location, |value| effective_slice_location(func, value, *location));
                if location == SliceLocation::Memory && array_loop_element(element).is_some() {
                    let value_ty = value
                        .and_then(|value| func.value_ty(value))
                        .unwrap_or_else(MirType::uint256);
                    let next = counts.len();
                    let count = counts
                        .entry(ArrayHelperKey { element: element.as_ref().clone(), value_ty })
                        .or_insert((0, next));
                    count.0 = count.0.saturating_add(occurrences).min(2);
                }
                count_sites(func, element, None, occurrences, counts);
            }
            AbiType::FixedArray { element, len } => count_sites(
                func,
                element,
                None,
                occurrences.saturating_mul(usize::try_from(*len).unwrap_or(usize::MAX)).min(2),
                counts,
            ),
            AbiType::Tuple(fields) => {
                for field in fields {
                    count_sites(func, field, None, occurrences, counts);
                }
            }
            AbiType::Word(_) | AbiType::Function | AbiType::Bytes(_) => {}
        }
    }

    let mut counts = FxHashMap::<ArrayHelperKey, (usize, usize)>::default();
    for func in module.functions.iter() {
        for inst in func.instructions() {
            let InstKind::AbiEncode { args, layout, .. } = &func.inst(inst).kind else { continue };
            for (&arg, ty) in args.iter().zip(&layout.types) {
                count_sites(func, ty, Some(arg), 1, &mut counts);
            }
        }
    }
    let mut keys = counts
        .into_iter()
        .filter(|(_, (count, _))| *count >= 2)
        .map(|(key, (_, first))| (first, key))
        .collect::<Vec<_>>();
    keys.sort_by_key(|(first, key)| (array_depth(&key.element), *first));

    let mut helpers = EncodeHelpers {
        branchless_byte_tails: !target.optimization().is_size(),
        ..EncodeHelpers::default()
    };
    for (_, key) in keys {
        let mut function = Function::new(Ident::with_dummy_span(sym::encode_abi_array));
        {
            let mut builder =
                FunctionBuilder::new(&mut function).with_revert_strings(revert_strings);
            let value = builder.add_param(key.value_ty);
            // The destination is a heap pointer, and typing it so lets the backend's
            // provenance analysis see that the returned tail stays in the heap.
            let dest = builder.add_param(MirType::MemPtr);
            let tail = encode_memory_array(&mut builder, &key.element, value, dest, &helpers);
            builder.add_return(MirType::uint256());
            builder.ret([tail]);
        }
        let helper = module.add_function(function);
        helpers.arrays.insert(key, helper);
    }
    helpers
}

/// Returns the shared helper encoding a memory array of `element`s passed as `value`.
fn array_helper(
    func: &Function,
    helpers: &EncodeHelpers,
    element: &AbiType,
    value: ValueId,
) -> Option<FunctionId> {
    array_loop_element(element)?;
    let value_ty = func.value_ty(value).unwrap_or_else(MirType::uint256);
    helpers.arrays.get(&ArrayHelperKey { element: element.clone(), value_ty }).copied()
}

/// Classifies how a memory array's elements are encoded: `None` for full words, copied as one
/// block; `Some(Some(cleanup))` for words cleaned one at a time; `Some(None)` for composite
/// elements encoded one at a time.
fn array_loop_element(element: &AbiType) -> Option<Option<AbiWordValidator>> {
    match element {
        AbiType::Word(cleanup) => cleanup.map(Some),
        AbiType::Function => Some(AbiWordValidator::from_mir_type(MirType::Function)),
        _ => Some(None),
    }
}

/// Number of dynamic arrays nested inside an element layout.
fn array_depth(ty: &AbiType) -> usize {
    match ty {
        AbiType::DynamicArray { element, .. } => 1 + array_depth(element),
        AbiType::FixedArray { element, .. } => array_depth(element),
        AbiType::Tuple(fields) => fields.iter().map(array_depth).max().unwrap_or(0),
        AbiType::Word(_) | AbiType::Function | AbiType::Bytes(_) => 0,
    }
}

#[derive(Clone, Copy)]
struct AbiValueDest {
    head_addr: ValueId,
    tuple_base: ValueId,
    tail: ValueId,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AbiValueSource {
    Scalar,
    Memory,
    // A zeroed composite array slot uses null to represent a default object;
    // following it as memory address zero would read unrelated scratch memory.
    NullableMemory,
}

impl AbiValueSource {
    fn descend_memory(
        self,
        builder: &mut FunctionBuilder<'_>,
        object: ValueId,
    ) -> (Self, Option<ValueId>) {
        match self {
            Self::NullableMemory => {
                (Self::NullableMemory, Some(memory_object_non_null(builder, object)))
            }
            Self::Scalar | Self::Memory => (Self::Memory, None),
        }
    }
}

fn lower_function(
    func: &mut Function,
    helpers: &mut EncodeHelpers,
    revert_strings: RevertStrings,
    fresh_object_returns: &DenseBitSet<FunctionId>,
    destructive_returns: &FxHashSet<InstId>,
) -> bool {
    let has_encodes =
        func.instructions().any(|inst| matches!(func.inst(inst).kind, InstKind::AbiEncode { .. }));
    if !has_encodes {
        return false;
    }
    helpers.literal_objects = literal_objects_at_encodes(func);

    let mut replacements = FxHashMap::default();
    let mut literal_objects = FxHashSet::default();
    let blocks = func.blocks.indices();
    for block in blocks {
        let instructions = std::mem::take(&mut func.blocks[block].instructions);
        let (original_terminator, terminator_metadata) = func.blocks[block].take_terminator();
        let mut builder = FunctionBuilder::new(func).with_revert_strings(revert_strings);
        builder.switch_to_block(block);
        for inst in instructions {
            let InstKind::AbiEncode { mode, selector, args, layout } =
                &builder.func().inst(inst).kind
            else {
                let current = builder.current_block();
                builder.func_mut().blocks[current].instructions.push(inst);
                continue;
            };
            let selector = selector.map(|value| resolve_replacement(value, &replacements));
            let args = args
                .iter()
                .map(|&value| resolve_replacement(value, &replacements))
                .collect::<Vec<_>>();
            let layout = std::sync::Arc::clone(layout);
            let mode = *mode;
            // abi_encode(args) !metadata(span) => icall or stores/copies !metadata(span)
            let metadata = builder.func().inst(inst).metadata.clone();
            builder.set_debug_context(&metadata);
            let key = TupleHelperKey::of(builder.func(), mode, selector, &args, &layout);
            let result =
                builder.func().inst_result_value(inst).expect("ABI encode must produce a value");
            let replacement = if destructive_returns.contains(&inst)
                && let Some(encoded) = encode_dynamic_return_in_place(
                    &mut builder,
                    &layout,
                    &args,
                    fresh_object_returns,
                ) {
                encoded
            } else {
                match helpers.tuples.get(&key) {
                    Some(&helper) => {
                        // encoded = icall @encode_abi_tuple, 1, args.., [selector]
                        let result_ty =
                            builder.func().value_ty(result).unwrap_or_else(MirType::uint256);
                        let call_args = args.iter().copied().chain(selector).collect();
                        builder.icall(helper, call_args, result_ty, 1)
                    }
                    None => lower_encode(&mut builder, &layout, selector, &args, mode, helpers),
                }
            };
            literal_objects.extend(args.iter().copied());
            replacements.insert(result, replacement);
        }
        move_terminator(&mut builder, block, original_terminator, terminator_metadata);
    }
    fold_slice_projections(func, &mut replacements);
    func.replace_uses_canonicalized(&replacements);
    remove_literal_objects(
        func,
        &literal_objects
            .into_iter()
            .filter(|object| helpers.literal_objects.contains(object))
            .collect::<Vec<_>>(),
    );
    let repaired = crate::mir::utils::repair_reachability_phis(func);
    !replacements.is_empty() || repaired
}

/// Selects destructive terminal encodings before module-wide helper sharing is
/// costed. Unoptimized lowering retains the direct semantic expansion.
fn plan_destructive_terminal_returns(
    target: Target,
    module: &Module,
) -> (DenseBitSet<FunctionId>, IndexVec<FunctionId, FxHashSet<InstId>>) {
    let mut fresh = DenseBitSet::new_empty(module.functions.len());
    let mut plan = IndexVec::from_vec(vec![FxHashSet::default(); module.functions.len()]);
    if !(target.optimization().is_gas() || target.optimization().is_size()) {
        return (fresh, plan);
    }
    let candidates = module.functions.iter().map(terminal_return_encodes).collect::<Vec<_>>();
    if candidates.iter().all(FxHashSet::is_empty) {
        return (fresh, plan);
    }
    fresh = fresh_object_returning_functions(module);
    for (func_id, candidates) in IndexVec::<FunctionId, _>::from_vec(candidates).iter_enumerated() {
        let func = &module.functions[func_id];
        let literal_objects = literal_objects_at_encodes(func);
        for &inst in candidates {
            let InstKind::AbiEncode { args, layout, .. } = &func.inst(inst).kind else {
                unreachable!()
            };
            if args.iter().all(|arg| !literal_objects.contains(arg))
                && can_encode_dynamic_return_in_place(func, layout, args, &fresh)
            {
                plan[func_id].insert(inst);
            }
        }
    }
    (fresh, plan)
}

/// Finds encodes used only by the two projections of an immediate
/// `returndata`. Destructive encoding is safe only in this terminal shape.
fn terminal_return_encodes(func: &Function) -> FxHashSet<InstId> {
    if !func.instructions().any(|inst| matches!(func.inst(inst).kind, InstKind::AbiEncode { .. })) {
        return FxHashSet::default();
    }
    let mut uses = FxHashMap::<ValueId, usize>::default();
    for inst_id in func.instructions() {
        for operand in func.inst(inst_id).operands() {
            *uses.entry(operand).or_default() += 1;
        }
    }
    for block in &func.blocks {
        if let Some(term) = &block.terminator {
            for operand in term.operands() {
                *uses.entry(operand).or_default() += 1;
            }
        }
    }

    let mut terminal = FxHashSet::default();
    for block in &func.blocks {
        let Some(Terminator::ReturnData { offset, size }) = &block.terminator else { continue };
        for (position, &encode) in block.instructions.iter().enumerate() {
            if !matches!(func.inst(encode).kind, InstKind::AbiEncode { .. }) {
                continue;
            }
            let Some(encoded) = func.inst_result_value(encode) else { continue };
            let suffix = &block.instructions[position + 1..];
            if suffix.len() != 2 || uses.get(&encoded).copied() != Some(2) {
                continue;
            }
            let mut projected_offset = None;
            let mut projected_size = None;
            for &projection in suffix {
                let Some(result) = func.inst_result_value(projection) else { continue };
                match func.inst(projection).kind {
                    InstKind::SlicePtr(value) if value == encoded => {
                        projected_offset = Some(result)
                    }
                    InstKind::SliceLen(value) if value == encoded => projected_size = Some(result),
                    _ => {}
                }
            }
            if projected_offset == Some(*offset)
                && projected_size == Some(*size)
                && uses.get(offset).copied() == Some(1)
                && uses.get(size).copied() == Some(1)
            {
                terminal.insert(encode);
            }
        }
    }
    terminal
}

/// Returns functions whose successful exits return a freshly allocated object.
/// Internal-call results propagate freshness to thin wrappers. Reverting paths
/// do not affect the proof.
pub(super) fn fresh_object_returning_functions(module: &Module) -> DenseBitSet<FunctionId> {
    let mut fresh = DenseBitSet::new_empty(module.functions.len());
    loop {
        let mut changed = false;
        for (func_id, func) in module.functions.iter_enumerated() {
            if fresh.contains(func_id) {
                continue;
            }
            let mut returns = func.blocks.iter().filter_map(|block| match &block.terminator {
                Some(Terminator::Return { values }) => Some(values.as_slice()),
                _ => None,
            });
            let Some(first) = returns.next() else { continue };
            if return_values_are_fresh(func, first, &fresh)
                && returns.all(|values| return_values_are_fresh(func, values, &fresh))
            {
                fresh.insert(func_id);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    fresh
}

/// Proves that a successful exit returns one allocation owned by its caller.
fn return_values_are_fresh(
    func: &Function,
    values: &[ValueId],
    fresh: &DenseBitSet<FunctionId>,
) -> bool {
    let [value] = values else { return false };
    let Value::Inst(inst) = func.value(*value) else { return false };
    match func.inst(*inst).kind {
        InstKind::Alloc { .. } => true,
        InstKind::MLoad(address)
            if func.value_u64(address) == Some(EvmMemoryLayout::FMP_SLOT)
                && func.inst(*inst).metadata.effect() == Some(EffectKind::MemoryWrite) =>
        {
            true
        }
        InstKind::ICall { function, returns: 1, .. } => fresh.contains(function),
        _ => false,
    }
}

fn can_encode_dynamic_return_in_place(
    func: &Function,
    layout: &AbiLayout,
    args: &[ValueId],
    fresh_object_returns: &DenseBitSet<FunctionId>,
) -> bool {
    let [object] = args else { return false };
    let kind = match &*layout.types {
        [AbiType::DynamicArray { element, location: SliceLocation::Memory }]
            if matches!(element.as_ref(), AbiType::Word(None)) =>
        {
            MemoryObjectKind::DynamicArray
        }
        [AbiType::Bytes(SliceLocation::Memory)] => MemoryObjectKind::Bytes,
        _ => return false,
    };
    func.value_ty(*object) == Some(MirType::MemoryObject(kind))
        && fresh_memory_object(func, *object, fresh_object_returns)
}

/// Encodes a one-element dynamic tuple over the returned object's storage.
fn encode_dynamic_return_in_place(
    builder: &mut FunctionBuilder<'_>,
    layout: &AbiLayout,
    args: &[ValueId],
    fresh_object_returns: &DenseBitSet<FunctionId>,
) -> Option<ValueId> {
    if !can_encode_dynamic_return_in_place(builder.func(), layout, args, fresh_object_returns) {
        return None;
    }
    let [object] = args else { unreachable!() };

    let kind = match &*layout.types {
        [AbiType::DynamicArray { .. }] => MemoryObjectKind::DynamicArray,
        [AbiType::Bytes(_)] => MemoryObjectKind::Bytes,
        _ => unreachable!(),
    };
    let length = memory_object_len(builder, *object, kind);
    let source = builder.memory_object_data(*object, kind);
    let bytes = if kind == MemoryObjectKind::DynamicArray {
        let five = builder.imm(5);
        builder.shl(five, length)
    } else {
        let thirty_one = builder.imm(31);
        let rounded = builder.add(length, thirty_one);
        let mask = builder.not(thirty_one);
        let padded = builder.and(rounded, mask);
        // object: [length, bytes..., padding]
        // => [32, length, bytes..., zero padding]
        let last = builder.add(source, padded);
        let zero = builder.imm(0);
        builder.mstore(last, zero);
        padded
    };

    // object: [length, payload...]
    // => [32, length, payload...]
    let destination = builder.add_u64_offset(source, 32);
    let copy_size = if kind == MemoryObjectKind::Bytes { length } else { bytes };
    builder.mcopy(destination, source, copy_size);
    let offset = builder.imm(32);
    builder.mstore(*object, offset);
    builder.mstore(source, length);
    let total = builder.add_u64_offset(bytes, 64);
    Some(builder.make_slice(*object, total, SliceLocation::Memory))
}

fn fresh_memory_object(
    func: &Function,
    value: ValueId,
    fresh_object_returns: &DenseBitSet<FunctionId>,
) -> bool {
    let Value::Inst(inst) = func.value(value) else { return false };
    match func.inst(*inst).kind {
        InstKind::Alloc { kind: AllocationKind::Object(_), .. } => true,
        InstKind::ICall { function, returns: 1, .. } => fresh_object_returns.contains(function),
        _ => false,
    }
}

fn fold_slice_projections(func: &Function, replacements: &mut FxHashMap<ValueId, ValueId>) {
    for inst_id in func.instructions() {
        let Some(result) = func.inst_result_value(inst_id) else { continue };
        let (slice, is_pointer) = match func.inst(inst_id).kind {
            InstKind::SlicePtr(slice) => (resolve_replacement(slice, replacements), true),
            InstKind::SliceLen(slice) => (resolve_replacement(slice, replacements), false),
            _ => continue,
        };
        let Value::Inst(make_slice) = func.value(slice) else { continue };
        let InstKind::MakeSlice { ptr, len, .. } = &func.inst(*make_slice).kind else { continue };
        let replacement = if is_pointer { *ptr } else { *len };
        replacements.insert(result, resolve_replacement(replacement, replacements));
    }
}

pub(crate) fn move_terminator(
    builder: &mut FunctionBuilder<'_>,
    original_block: BlockId,
    terminator: Option<Terminator>,
    metadata: InstructionMetadata,
) {
    let final_block = builder.current_block();
    let Some(terminator) = terminator else { return };
    // final_block: lowered_ops; original_terminator !metadata(original block)
    builder.func_mut().blocks[final_block].terminator = Some(terminator);
    builder.func_mut().blocks[final_block].terminator_metadata = metadata;
    if final_block != original_block {
        redirect_successor_predecessors(builder.func_mut(), original_block, final_block);
    }
}

fn lower_encode(
    builder: &mut FunctionBuilder<'_>,
    layout: &AbiLayout,
    selector: Option<ValueId>,
    args: &[ValueId],
    mode: AbiEncodeMode,
    helpers: &EncodeHelpers,
) -> ValueId {
    debug_assert_eq!(layout.types.len(), args.len());
    let selector_size = if selector.is_some() { 4 } else { 0 };
    if !layout.types.iter().any(AbiType::is_dynamic) {
        let total_size = selector_size + layout.head_size();
        let aligned_size = total_size.next_multiple_of(32);
        if mode == AbiEncodeMode::Bytes {
            let allocation_size = builder.imm(aligned_size.saturating_add(32));
            let object = builder.alloc_object(
                allocation_size,
                MemoryObjectLayout::Bytes,
                crate::mir::AllocationSemantics::INTERNAL,
            );
            let total = builder.imm(total_size);
            builder.set_memory_object_len(object, total, MemoryObjectKind::Bytes);
            let buffer = builder.memory_object_data(object, MemoryObjectKind::Bytes);
            if let Some(selector) = selector {
                builder.mstore(buffer, selector);
            }
            let dest = offset_ptr(builder, buffer, selector_size);
            encode_tuple(builder, args, &layout.types, dest, helpers);
            return object;
        }
        if mode == AbiEncodeMode::Slice && selector.is_none() {
            // Constructor arguments do not need a raw slice, and keeping their
            // object header lets the memory lowering preserve the established
            // allocation path for creation code.
            let allocation_size = builder.imm(aligned_size.saturating_add(32));
            let object = builder.alloc_object(
                allocation_size,
                MemoryObjectLayout::Bytes,
                crate::mir::AllocationSemantics::INTERNAL,
            );
            let total = builder.imm(total_size);
            builder.set_memory_object_len(object, total, MemoryObjectKind::Bytes);
            let data = builder.memory_object_data(object, MemoryObjectKind::Bytes);
            encode_tuple(builder, args, &layout.types, data, helpers);
            return builder.make_slice(data, total, SliceLocation::Memory);
        }
        let buffer = if mode == AbiEncodeMode::Scratch {
            builder.fmp()
        } else {
            let allocation_size = builder.imm(aligned_size);
            builder.alloc_raw(allocation_size, crate::mir::AllocationSemantics::INTERNAL)
        };
        if let Some(selector) = selector {
            builder.mstore(buffer, selector);
        }
        let dest = offset_ptr(builder, buffer, selector_size);
        encode_tuple(builder, args, &layout.types, dest, helpers);
        let total = builder.imm(total_size);
        return builder.make_slice(buffer, total, SliceLocation::Memory);
    }

    if mode == AbiEncodeMode::Slice
        && let Some(total_size) = literal_tuple_size(
            builder.func(),
            builder.current_block(),
            layout,
            args,
            selector_size,
            &helpers.literal_objects,
        )
    {
        // buffer = alloc raw, padded_size
        // mstore buffer, selector (when present)
        // encode_tuple buffer + selector_size, args
        // make_slice buffer, total_size
        let size = builder.imm(total_size.next_multiple_of(32));
        let buffer = builder.alloc_raw(size, crate::mir::AllocationSemantics::INTERNAL);
        if let Some(selector) = selector {
            builder.mstore(buffer, selector);
        }
        let dest = offset_ptr(builder, buffer, selector_size);
        encode_tuple(builder, args, &layout.types, dest, helpers);
        let total = builder.imm(total_size);
        return builder.make_slice(buffer, total, SliceLocation::Memory);
    }

    // Source objects already live below the free-memory pointer. Encode into
    // the untouched range, then reserve the exact output in one pass.
    let allocation_base = builder.fmp();
    let buffer = if mode == AbiEncodeMode::Bytes {
        offset_ptr(builder, allocation_base, 32)
    } else {
        allocation_base
    };
    if let Some(selector) = selector {
        builder.mstore(buffer, selector);
    }
    let dest = offset_ptr(builder, buffer, selector_size);
    let encoded_size = encode_tuple(builder, args, &layout.types, dest, helpers);
    let selector_size = builder.imm(selector_size);
    let total = builder.add(encoded_size, selector_size);
    if mode == AbiEncodeMode::Bytes {
        // object = alloc bytes allocation_size !metadata(preserves_fmp)
        // memory_object_store_len object, total
        let allocation_size = builder.checked_padded_size(total);
        let object = builder.alloc_object(
            allocation_size,
            MemoryObjectLayout::Bytes,
            crate::mir::AllocationSemantics::INTERNAL,
        );
        preserve_encoding_address(builder, object);
        builder.set_memory_object_len(object, total, MemoryObjectKind::Bytes);
        return object;
    }
    if mode == AbiEncodeMode::Scratch {
        return builder.make_slice(allocation_base, total, SliceLocation::Memory);
    }
    let thirty_one = builder.imm(31);
    let rounded = builder.add(total, thirty_one);
    let mask = builder.not(thirty_one);
    let aligned = builder.and(rounded, mask);
    // allocated = alloc raw aligned !metadata(preserves_fmp)
    // make_slice allocated, total
    let allocated = builder.alloc_raw(aligned, crate::mir::AllocationSemantics::INTERNAL);
    preserve_encoding_address(builder, allocated);
    builder.make_slice(allocated, total, SliceLocation::Memory)
}

/// Literal byte tails have known lengths even though their ABI types are
/// dynamic. Reserve the output before writing it so its base is explicit and
/// ordinary allocation analysis can place it. Other dynamic shapes retain the
/// reserve-after-encoding path and its address-preservation obligation.
fn literal_tuple_size(
    func: &Function,
    block: BlockId,
    layout: &AbiLayout,
    args: &[ValueId],
    selector_size: u64,
    literal_objects: &FxHashSet<ValueId>,
) -> Option<u64> {
    let mut size = selector_size;
    let mut has_payload = false;
    for (ty, &value) in layout.types.iter().zip(args) {
        size = size.checked_add(ty.head_size())?;
        if ty.is_dynamic() {
            if *ty != AbiType::Bytes(SliceLocation::Memory) || !literal_objects.contains(&value) {
                return None;
            }
            let length = u64::try_from(literal_bytes(func, value, block)?.len()).ok()?;
            has_payload |= length != 0;
            size = size.checked_add(32)?.checked_add(length.checked_next_multiple_of(32)?)?;
        }
    }
    size.checked_next_multiple_of(32)?;
    has_payload.then_some(size)
}

/// The reservation commits bytes already written through a separate FMP read.
fn preserve_encoding_address(builder: &mut FunctionBuilder<'_>, allocation: ValueId) {
    let Value::Inst(inst) = *builder.func().value(allocation) else {
        unreachable!("allocation result must reference its instruction")
    };
    // allocation !metadata(preserves_fmp)
    builder.func_mut().inst_mut(inst).metadata.set_preserves_fmp(true);
}

/// Encodes a statically shaped tuple into an existing physical return buffer.
///
/// Static ABI returns use the fixed return buffer owned by the ABI boundary;
/// source-level `abi.encode` uses the typed object encoder below.
pub(crate) fn encode_static_tuple(
    builder: &mut FunctionBuilder<'_>,
    values: &[ValueId],
    types: &[AbiType],
    dest: ValueId,
) -> ValueId {
    let mut head = dest;
    for (&value, ty) in values.iter().zip(types) {
        encode_static_impl(builder, ty, value, head, false, AbiValueSource::Scalar);
        head = offset_ptr(builder, head, ty.head_size());
    }
    builder.sub(head, dest)
}

fn encode_static_impl(
    builder: &mut FunctionBuilder<'_>,
    ty: &AbiType,
    value: ValueId,
    head_addr: ValueId,
    allow_slice_fast_path: bool,
    source: AbiValueSource,
) {
    if source != AbiValueSource::NullableMemory
        && allow_slice_fast_path
        && let Some(location @ (SliceLocation::Calldata | SliceLocation::Memory)) =
            builder.func().value_slice_location(value)
    {
        let source = builder.slice_ptr(value);
        encode_static_slice(builder, ty, source, head_addr, location);
        return;
    }
    match ty {
        AbiType::Tuple(fields) => {
            let (child_source, non_null) = source.descend_memory(builder, value);
            let mut field_head = head_addr;
            for (index, field) in fields.iter().enumerate() {
                let mut field_value = builder.memory_object_load_field(
                    value,
                    MemoryObjectLayout::structure(fields.len() as u64),
                    index as u64,
                );
                if let Some(non_null) = non_null {
                    field_value = builder.mul(field_value, non_null);
                }
                encode_static_impl(
                    builder,
                    field,
                    field_value,
                    field_head,
                    allow_slice_fast_path,
                    child_source,
                );
                field_head = offset_ptr(builder, field_head, field.head_size());
            }
        }
        AbiType::FixedArray { element, len } => {
            encode_static_array(
                builder,
                element,
                *len,
                value,
                head_addr,
                allow_slice_fast_path,
                source,
            );
        }
        AbiType::Function => {
            let value = if source == AbiValueSource::Scalar {
                let shift = builder.imm(64);
                builder.shl(shift, value)
            } else {
                AbiWordValidator::from_mir_type(MirType::Function)
                    .expect("function words always require cleanup")
                    .cleanup(builder, value)
            };
            builder.mstore(head_addr, value);
        }
        AbiType::Word(cleanup) => {
            let value = match (source, cleanup) {
                (AbiValueSource::Memory | AbiValueSource::NullableMemory, Some(cleanup)) => {
                    clean_word(builder, *cleanup, value)
                }
                _ => value,
            };
            builder.mstore(head_addr, value);
        }
        AbiType::Bytes(_) | AbiType::DynamicArray { .. } => {
            unreachable!("dynamic ABI values are not static")
        }
    }
}

fn encode_static_array(
    builder: &mut FunctionBuilder<'_>,
    element: &AbiType,
    len: u64,
    value: ValueId,
    head_addr: ValueId,
    allow_slice_fast_path: bool,
    source: AbiValueSource,
) {
    let (child_source, non_null) = source.descend_memory(builder, value);
    let done = non_null.map(|non_null| {
        let encode = builder.create_block();
        let encode_zero = builder.create_block();
        let done = builder.create_block();
        builder.branch(non_null, encode, encode_zero);

        builder.switch_to_block(encode_zero);
        let size = builder.imm(element.head_size() * len);
        builder.memory_zero(head_addr, size);
        builder.jump(done);

        builder.switch_to_block(encode);
        done
    });

    let length = builder.imm(len);
    let stride = builder.imm(element.head_size());
    builder.counted_loop(length, |builder, index| {
        let element_value = builder.memory_object_load_element(
            value,
            MemoryObjectLayout::word_fixed_array(len),
            index,
        );
        let offset = builder.mul(index, stride);
        let element_head = builder.add(head_addr, offset);
        encode_static_impl(
            builder,
            element,
            element_value,
            element_head,
            allow_slice_fast_path,
            child_source,
        );
    });
    if let Some(done) = done {
        builder.jump(done);
        builder.switch_to_block(done);
    }
}

fn encode_tuple(
    builder: &mut FunctionBuilder<'_>,
    values: &[ValueId],
    types: &[AbiType],
    dest: ValueId,
    helpers: &EncodeHelpers,
) -> ValueId {
    encode_tuple_impl(builder, values, types, dest, AbiValueSource::Scalar, helpers)
}

fn encode_tuple_impl(
    builder: &mut FunctionBuilder<'_>,
    values: &[ValueId],
    types: &[AbiType],
    dest: ValueId,
    source: AbiValueSource,
    helpers: &EncodeHelpers,
) -> ValueId {
    let head_size: u64 = types.iter().map(AbiType::head_size).sum();
    if !types.iter().any(AbiType::is_dynamic) {
        let mut head_offset = 0;
        for (&value, ty) in values.iter().zip(types) {
            let head = offset_ptr(builder, dest, head_offset);
            encode_static_impl(builder, ty, value, head, true, source);
            head_offset += ty.head_size();
        }
        return builder.imm(head_size);
    }

    let head_size_value = builder.imm(head_size);
    let mut tail = builder.add(dest, head_size_value);
    let mut head_offset = 0;
    for (&value, ty) in values.iter().zip(types) {
        let head_addr = offset_ptr(builder, dest, head_offset);
        tail = encode_value(
            builder,
            ty,
            value,
            AbiValueDest { head_addr, tuple_base: dest, tail },
            source,
            helpers,
        );
        head_offset += ty.head_size();
    }
    builder.sub(tail, dest)
}

fn encode_value(
    builder: &mut FunctionBuilder<'_>,
    ty: &AbiType,
    value: ValueId,
    dest: AbiValueDest,
    source: AbiValueSource,
    helpers: &EncodeHelpers,
) -> ValueId {
    if ty.is_dynamic() {
        let relative = builder.sub(dest.tail, dest.tuple_base);
        builder.mstore(dest.head_addr, relative);
        encode_dynamic_body(builder, ty, value, dest.tail, source, helpers)
    } else {
        encode_static_impl(builder, ty, value, dest.head_addr, true, source);
        dest.tail
    }
}

fn encode_static_slice(
    builder: &mut FunctionBuilder<'_>,
    ty: &AbiType,
    source: ValueId,
    head_addr: ValueId,
    location: SliceLocation,
) {
    match ty {
        AbiType::Tuple(fields) => {
            let mut source_offset = 0;
            let mut head = head_addr;
            for field in fields {
                let source_word = offset_ptr(builder, source, source_offset);
                encode_static_slice(builder, field, source_word, head, location);
                source_offset += field.head_size();
                head = offset_ptr(builder, head, field.head_size());
            }
        }
        AbiType::FixedArray { element, len } => {
            let length = builder.imm(*len);
            let stride = builder.imm(element.head_size());
            builder.counted_loop(length, |builder, index| {
                let offset = builder.mul(index, stride);
                let source_word = builder.add(source, offset);
                let head = builder.add(head_addr, offset);
                encode_static_slice(builder, element, source_word, head, location);
            });
        }
        AbiType::Function => {
            let value = load_slice_word(builder, source, location);
            let value = if location == SliceLocation::Memory {
                AbiWordValidator::from_mir_type(MirType::Function)
                    .expect("function words always require cleanup")
                    .cleanup(builder, value)
            } else {
                value
            };
            builder.mstore(head_addr, value);
        }
        AbiType::Word(cleanup) => {
            let value = load_slice_word(builder, source, location);
            let value = match cleanup {
                Some(cleanup) if location == SliceLocation::Memory => {
                    clean_word(builder, *cleanup, value)
                }
                _ => value,
            };
            builder.mstore(head_addr, value);
        }
        AbiType::Bytes(_) | AbiType::DynamicArray { .. } => {
            unreachable!("dynamic ABI values are not static")
        }
    }
}

/// Canonicalizes a word read from memory before it is encoded, like solc's per-type
/// `cleanup` and `validator_assert` helpers: narrow values are masked or sign-extended and an
/// out-of-range enum panics. Scalars arriving on the stack were already cleaned by the
/// lowering, and calldata words were validated when decoded.
fn clean_word(
    builder: &mut FunctionBuilder<'_>,
    cleanup: AbiWordValidator,
    value: ValueId,
) -> ValueId {
    match cleanup {
        AbiWordValidator::EnumRange(variants) => {
            builder.validate_enum_value(variants, value);
            value
        }
        _ => cleanup.cleanup(builder, value),
    }
}

fn load_slice_word(
    builder: &mut FunctionBuilder<'_>,
    source: ValueId,
    location: SliceLocation,
) -> ValueId {
    match location {
        SliceLocation::Calldata => builder.calldataload(source),
        SliceLocation::Memory => builder.mload(source),
        SliceLocation::Returndata => unreachable!("returndata slices are not static ABI inputs"),
    }
}

fn encode_dynamic_body(
    builder: &mut FunctionBuilder<'_>,
    ty: &AbiType,
    value: ValueId,
    dest: ValueId,
    source: AbiValueSource,
    helpers: &EncodeHelpers,
) -> ValueId {
    match ty {
        AbiType::Bytes(location) => {
            let location = effective_slice_location(builder.func(), value, *location);
            encode_bytes(
                builder,
                value,
                dest,
                location,
                helpers.literal_objects.contains(&value),
                helpers.branchless_byte_tails,
            )
        }
        AbiType::DynamicArray { element, location } => {
            let location = effective_slice_location(builder.func(), value, *location);
            if location == SliceLocation::Memory {
                if let Some(helper) = array_helper(builder.func(), helpers, element, value) {
                    return builder.icall(helper, vec![value, dest], MirType::uint256(), 1);
                }
                return encode_memory_array(builder, element, value, dest, helpers);
            }
            let word_cleanup = match element.as_ref() {
                AbiType::Word(cleanup) => Some(*cleanup),
                AbiType::Function => Some(AbiWordValidator::from_mir_type(MirType::Function)),
                _ => None,
            };
            if let Some(cleanup) = word_cleanup {
                return encode_word_array(builder, value, dest, location, cleanup);
            }
            match location {
                SliceLocation::Calldata => {
                    if matches!(element.as_ref(), AbiType::Bytes(_)) {
                        encode_calldata_bytes_array(builder, element, value, dest, helpers)
                    } else {
                        unreachable!(
                            "non-word calldata arrays are materialized before ABI encoding"
                        )
                    }
                }
                SliceLocation::Memory | SliceLocation::Returndata => {
                    unreachable!("returndata arrays are not ABI inputs")
                }
            }
        }
        AbiType::FixedArray { element, len } => {
            let (_, non_null) = source.descend_memory(builder, value);
            encode_memory_array_elements(
                builder,
                element,
                value,
                dest,
                MemoryObjectLayout::word_fixed_array(*len),
                non_null,
                helpers,
            )
        }
        AbiType::Tuple(fields) => {
            let (child_source, non_null) = source.descend_memory(builder, value);
            let mut values = Vec::with_capacity(fields.len());
            for index in 0..fields.len() {
                let mut field_value = builder.memory_object_load_field(
                    value,
                    crate::mir::MemoryObjectLayout::structure(fields.len() as u64),
                    index as u64,
                );
                if let Some(non_null) = non_null {
                    field_value = builder.mul(field_value, non_null);
                }
                values.push(field_value);
            }
            let size = encode_tuple_impl(builder, &values, fields, dest, child_source, helpers);
            builder.add(dest, size)
        }
        AbiType::Word(_) | AbiType::Function => unreachable!("word ABI values are static"),
    }
}

fn effective_slice_location(
    func: &Function,
    value: ValueId,
    declared: SliceLocation,
) -> SliceLocation {
    match func.value_ty(value) {
        Some(MirType::Slice(location)) => location,
        Some(MirType::MemPtr | MirType::MemoryObject(_)) => SliceLocation::Memory,
        _ if matches!(func.value(value), Value::Inst(inst) if matches!(
            func.inst(*inst).kind,
            InstKind::MemoryObjectLoadField { .. } | InstKind::MemoryObjectLoadElement { .. }
        )) =>
        {
            SliceLocation::Memory
        }
        _ => declared,
    }
}

/// Encodes a memory array's elements: cleaned words and composite elements one at a time, full
/// words as one copy.
fn encode_memory_array(
    builder: &mut FunctionBuilder<'_>,
    element: &AbiType,
    value: ValueId,
    dest: ValueId,
    helpers: &EncodeHelpers,
) -> ValueId {
    match array_loop_element(element) {
        Some(None) => encode_memory_array_elements(
            builder,
            element,
            value,
            dest,
            MemoryObjectLayout::WORD_ARRAY,
            None,
            helpers,
        ),
        cleanup => {
            encode_word_array(builder, value, dest, SliceLocation::Memory, cleanup.flatten())
        }
    }
}

fn encode_memory_array_elements(
    builder: &mut FunctionBuilder<'_>,
    element: &AbiType,
    value: ValueId,
    dest: ValueId,
    layout: MemoryObjectLayout,
    non_null: Option<ValueId>,
    helpers: &EncodeHelpers,
) -> ValueId {
    let (len, element_area) = match layout {
        MemoryObjectLayout::DynamicArray { .. } => {
            let len = memory_object_len(builder, value, MemoryObjectKind::DynamicArray);
            builder.mstore(dest, len);
            let word = builder.imm(32);
            (len, builder.add(dest, word))
        }
        MemoryObjectLayout::FixedArray { len, .. } => (builder.imm(len), dest),
        MemoryObjectLayout::Bytes | MemoryObjectLayout::Struct { .. } => {
            unreachable!("ABI array encoding requires an array memory layout")
        }
    };
    let element_head_size = builder.imm(element.head_size());
    let head_bytes = builder.mul(len, element_head_size);
    let initial_tail = builder.add(element_area, head_bytes);
    let source_cursor = builder.memory_object_data(value, layout.kind());
    let word = builder.imm(32);

    let preheader = builder.current_block();
    let cond = builder.create_block();
    let body = builder.create_block();
    let done = builder.create_block();
    builder.jump(cond);

    builder.switch_to_block(cond);
    let remaining = builder.phi(vec![(preheader, len)]);
    let current_tail = builder.phi(vec![(preheader, initial_tail)]);
    let element_head = builder.phi(vec![(preheader, element_area)]);
    let source = builder.phi(vec![(preheader, source_cursor)]);
    let zero = builder.imm(0);
    let has_next = builder.gt(remaining, zero);
    builder.branch(has_next, body, done);

    builder.switch_to_block(body);
    let mut element_value = builder.mload(source);
    if let Some(non_null) = non_null {
        element_value = builder.mul(element_value, non_null);
    }
    let new_tail = encode_value(
        builder,
        element,
        element_value,
        AbiValueDest { head_addr: element_head, tuple_base: element_area, tail: current_tail },
        AbiValueSource::NullableMemory,
        helpers,
    );

    let one = builder.imm(1);
    let next_remaining = builder.sub(remaining, one);
    let next_source = builder.add(source, word);
    let next_head = builder.add(element_head, element_head_size);
    let backedge = builder.current_block();
    builder.jump(cond);
    builder.add_phi_incoming(remaining, backedge, next_remaining);
    builder.add_phi_incoming(current_tail, backedge, new_tail);
    builder.add_phi_incoming(element_head, backedge, next_head);
    builder.add_phi_incoming(source, backedge, next_source);

    builder.switch_to_block(done);
    current_tail
}

fn encode_calldata_bytes_array(
    builder: &mut FunctionBuilder<'_>,
    element: &AbiType,
    value: ValueId,
    dest: ValueId,
    helpers: &EncodeHelpers,
) -> ValueId {
    let len = builder.slice_len(value);
    builder.mstore(dest, len);

    let word = builder.imm(32);
    let element_area = builder.add(dest, word);
    let head_bytes = builder.mul(len, word);
    let initial_tail = builder.add(element_area, head_bytes);
    let source_base = builder.slice_ptr(value);

    let preheader = builder.current_block();
    let cond = builder.create_block();
    let body = builder.create_block();
    let done = builder.create_block();
    builder.jump(cond);

    builder.switch_to_block(cond);
    let remaining = builder.phi(vec![(preheader, len)]);
    let current_tail = builder.phi(vec![(preheader, initial_tail)]);
    let element_head = builder.phi(vec![(preheader, element_area)]);
    let source_head = builder.phi(vec![(preheader, source_base)]);
    let zero = builder.imm(0);
    let has_next = builder.gt(remaining, zero);
    builder.branch(has_next, body, done);

    builder.switch_to_block(body);
    // Mirror solc's calldata tail access: the offset bound is signed, so a negative
    // offset that wraps to a valid load is accepted and the tail checks decide whether
    // the element is valid. Bytes past `calldatasize` read as zero.
    let offset = builder.calldataload(source_head);
    let calldata_size = builder.calldatasize();
    let available = builder.sub(calldata_size, source_base);
    let thirty_one = builder.imm(31);
    let bound = builder.sub(available, thirty_one);
    let valid_offset = builder.slt(offset, bound);
    let invalid_offset = builder.iszero(valid_offset);
    builder.revert_if(invalid_offset, RevertReason::InvalidCalldataAccessOffset);
    let element_base = builder.add(source_base, offset);
    let length = builder.calldataload(element_base);
    let max_length = builder.imm(u64::MAX);
    let invalid_length = builder.gt(length, max_length);
    builder.revert_if(invalid_length, RevertReason::InvalidCalldataAccessLength);
    let data = builder.add(element_base, word);
    let limit = builder.sub(calldata_size, length);
    let short_tail = builder.sgt(data, limit);
    builder.revert_if(short_tail, RevertReason::InvalidCalldataAccessStride);
    let element_value = builder.make_slice(data, length, SliceLocation::Calldata);
    let new_tail = encode_value(
        builder,
        element,
        element_value,
        AbiValueDest { head_addr: element_head, tuple_base: element_area, tail: current_tail },
        AbiValueSource::Scalar,
        helpers,
    );

    let one = builder.imm(1);
    let next_remaining = builder.sub(remaining, one);
    let next_source = builder.add(source_head, word);
    let next_head = builder.add(element_head, word);
    let backedge = builder.current_block();
    builder.jump(cond);
    builder.add_phi_incoming(remaining, backedge, next_remaining);
    builder.add_phi_incoming(current_tail, backedge, new_tail);
    builder.add_phi_incoming(element_head, backedge, next_head);
    builder.add_phi_incoming(source_head, backedge, next_source);

    builder.switch_to_block(done);
    current_tail
}

fn encode_word_array(
    builder: &mut FunctionBuilder<'_>,
    value: ValueId,
    dest: ValueId,
    location: SliceLocation,
    cleanup: Option<AbiWordValidator>,
) -> ValueId {
    let len = match location {
        SliceLocation::Memory => memory_object_len(builder, value, MemoryObjectKind::DynamicArray),
        SliceLocation::Calldata | SliceLocation::Returndata => builder.slice_len(value),
    };
    builder.mstore(dest, len);
    let word = builder.imm(32);
    let bytes = builder.mul(len, word);
    let data_dest = builder.add(dest, word);
    let data_source = match location {
        SliceLocation::Memory => builder.memory_object_data(value, MemoryObjectKind::DynamicArray),
        SliceLocation::Calldata | SliceLocation::Returndata => builder.slice_ptr(value),
    };
    let tail = builder.add(data_dest, bytes);
    // Memory elements that need cleanup are copied one word at a time, like solc's per-element
    // array encoder; full words and calldata elements copy as one block.
    if let Some(cleanup) = cleanup
        && location == SliceLocation::Memory
    {
        // This is a profitability hint, not a length fact: assembly may have
        // overwritten the length since allocation. Both loops use the loaded
        // length and preserve the same addresses and memory-operation order.
        let small_allocation = matches!(builder.func().value(value), Value::Inst(inst)
            if matches!(builder.func().inst(*inst).kind, InstKind::Alloc { size, .. }
                if builder.func().value_u64(size).is_some_and(|size| size <= 64)));
        if small_allocation {
            // for index in 0..len:
            //   mstore data_dest + index * 32, clean(mload data_source + index * 32)
            builder.counted_loop(len, |builder, index| {
                let offset = builder.mul(index, word);
                let source = builder.add(data_source, offset);
                let destination = builder.add(data_dest, offset);
                let value = builder.mload(source);
                let value = clean_word(builder, cleanup, value);
                builder.mstore(destination, value);
            });
            return tail;
        }
        // delta = data_source - data_dest
        // remaining = len; destination = data_dest
        // if remaining != 0: do:
        //   mstore destination, clean(mload destination + delta)
        //   remaining -= 1; destination += 32
        // while remaining != 0
        let delta = builder.sub(data_source, data_dest);
        let preheader = builder.current_block();
        let body = builder.create_block();
        let done = builder.create_block();
        builder.branch(len, body, done);
        builder.switch_to_block(body);
        let remaining = builder.phi(vec![(preheader, len)]);
        let destination = builder.phi(vec![(preheader, data_dest)]);
        let source = builder.add(destination, delta);
        let value = builder.mload(source);
        let value = clean_word(builder, cleanup, value);
        builder.mstore(destination, value);
        let one = builder.imm(1);
        let next_remaining = builder.sub(remaining, one);
        let next_destination = builder.add(destination, word);
        let backedge = builder.current_block();
        builder.branch(next_remaining, body, done);
        builder.add_phi_incoming(remaining, backedge, next_remaining);
        builder.add_phi_incoming(destination, backedge, next_destination);
        builder.switch_to_block(done);
        return builder.phi(vec![(preheader, data_dest), (backedge, next_destination)]);
    }
    builder.copy_slice_data(location, data_dest, data_source, bytes);
    tail
}

fn encode_bytes(
    builder: &mut FunctionBuilder<'_>,
    value: ValueId,
    dest: ValueId,
    location: SliceLocation,
    literal_folding: bool,
    branchless_padding: bool,
) -> ValueId {
    if literal_folding
        && location == SliceLocation::Memory
        && let Some(bytes) = literal_bytes(builder.func(), value, builder.current_block())
    {
        let length = builder.imm(bytes.len() as u64);
        builder.mstore(dest, length);
        let data = builder.add_u64_offset(dest, 32);
        for (index, chunk) in bytes.chunks(32).enumerate() {
            let mut padded = [0_u8; 32];
            padded[..chunk.len()].copy_from_slice(chunk);
            let word = builder.imm(U256::from_be_bytes(padded));
            let offset = builder.imm(index as u64 * 32);
            let address = builder.add(data, offset);
            builder.mstore(address, word);
        }
        let size = builder.imm(bytes.len().next_multiple_of(32) as u64);
        return builder.add(data, size);
    }

    let len = match location {
        SliceLocation::Memory => memory_object_len(builder, value, MemoryObjectKind::Bytes),
        SliceLocation::Calldata | SliceLocation::Returndata => builder.slice_len(value),
    };
    if !branchless_padding {
        // mstore dest, len
        builder.mstore(dest, len);
    }

    let word = builder.imm(32);
    let thirty_one = builder.imm(31);
    let mask = builder.not(thirty_one);
    let rounded = builder.add(len, thirty_one);
    let padded = builder.and(rounded, mask);
    let data_dest = builder.add(dest, word);

    if branchless_padding {
        // last = dest + padded
        // mstore last, 0
        // mstore dest, len
        // For an empty tail, last == dest; write the header after zeroing it.
        let last = builder.add(dest, padded);
        let zero = builder.imm(0);
        builder.mstore(last, zero);
        builder.mstore(dest, len);
    } else {
        // if padded != 0: mstore data_dest + padded - 32, 0
        let zero_block = builder.create_block();
        let copy_block = builder.create_block();
        let empty = builder.iszero(padded);
        builder.branch(empty, copy_block, zero_block);
        builder.switch_to_block(zero_block);
        let last_offset = builder.sub(padded, word);
        let last = builder.add(data_dest, last_offset);
        let zero = builder.imm(0);
        builder.mstore(last, zero);
        builder.jump(copy_block);
        builder.switch_to_block(copy_block);
    }
    let data_source = match location {
        SliceLocation::Memory => builder.memory_object_data(value, MemoryObjectKind::Bytes),
        SliceLocation::Calldata | SliceLocation::Returndata => builder.slice_ptr(value),
    };
    let tail = builder.add(data_dest, padded);
    builder.copy_slice_data(location, data_dest, data_source, len);
    tail
}

fn memory_object_len(
    builder: &mut FunctionBuilder<'_>,
    value: ValueId,
    kind: MemoryObjectKind,
) -> ValueId {
    let len = builder.memory_object_len(value, kind);
    let non_null = memory_object_non_null(builder, value);
    builder.mul(len, non_null)
}

fn memory_object_non_null(builder: &mut FunctionBuilder<'_>, object: ValueId) -> ValueId {
    let non_null = builder.iszero(object);
    builder.iszero(non_null)
}

/// Finds literal candidates whose allocation and encoding share a block, with
/// no intervening opaque memory access or physical address observation. Capture
/// this before lowering emits its own raw accesses. If any encoding use fails
/// the check, reject the object for the whole function. The separate direct-use
/// check rejects escaping pointers and validates complete initialization; after
/// the last encoding use the unescaped source object is dead.
fn literal_objects_at_encodes(func: &Function) -> FxHashSet<ValueId> {
    let mut objects = FxHashSet::default();
    let mut rejected = FxHashSet::default();
    let mut available = FxHashSet::default();
    for block in &func.blocks {
        available.clear();
        for &inst in &block.instructions {
            let kind = &func.inst(inst).kind;
            if literal_opaque_instruction(func, kind) {
                available.clear();
            }
            if matches!(
                kind,
                InstKind::Alloc { kind: AllocationKind::Object(MemoryObjectLayout::Bytes), .. }
            ) && let Some(object) = func.inst_result_value(inst)
            {
                available.insert(object);
            }
            if let InstKind::AbiEncode { args, .. } = kind {
                for &object in args.iter() {
                    if func.value_ty(object) == Some(MirType::MemoryObject(MemoryObjectKind::Bytes))
                    {
                        if available.contains(&object) {
                            objects.insert(object);
                        } else {
                            rejected.insert(object);
                        }
                    }
                }
            }
        }
    }
    objects.retain(|object| !rejected.contains(object));
    objects
}

fn literal_opaque_instruction(func: &Function, kind: &InstKind) -> bool {
    match kind {
        InstKind::Alloc { kind: AllocationKind::Object(_), .. } | InstKind::AbiEncode { .. } => {
            false
        }
        InstKind::SetMemoryObjectLen(object, ..) => {
            !literal_store_in_bounds(func, *object, Some(32))
        }
        InstKind::MemoryObjectStoreWord { object, offset, .. } => !literal_store_in_bounds(
            func,
            *object,
            func.value_u64(*offset).and_then(|offset| offset.checked_add(64)),
        ),
        InstKind::MemoryObjectData(..)
        | InstKind::MemoryObjectFieldAddr { .. }
        | InstKind::MemoryObjectElementAddr { .. }
        | InstKind::MSize => true,
        _ => matches!(
            kind.effect_kind(),
            EffectKind::MemoryRead
                | EffectKind::MemoryWrite
                | EffectKind::ICall
                | EffectKind::ExternalCall
                | EffectKind::Create
                | EffectKind::Log
        ),
    }
}

/// A semantic store to a different object is disjoint only inside its fresh
/// allocation. An unknown or wrapping offset can overwrite a preceding object.
fn literal_store_in_bounds(func: &Function, object: ValueId, end: Option<u64>) -> bool {
    matches!((func.value(object), end), (Value::Inst(alloc), Some(end))
        if matches!(func.inst(*alloc).kind, InstKind::Alloc { kind: AllocationKind::Object(_), size, .. }
            if func.value_u64(size).is_some_and(|size| end <= size)))
}

/// Returns the bytes represented by an immutable literal object when all active
/// uses are initialization operations preceding this encoding in the same block.
/// The caller must also exclude opaque observers using the original IR.
fn literal_bytes(func: &Function, object: ValueId, block: BlockId) -> Option<Vec<u8>> {
    if func.value_ty(object) != Some(MirType::MemoryObject(MemoryObjectKind::Bytes)) {
        return None;
    }
    let Value::Inst(defining_inst) = func.value(object) else { return None };
    if !func.blocks[block].instructions.contains(defining_inst) {
        return None;
    }
    if !matches!(func.inst(*defining_inst).kind, InstKind::Alloc { .. })
        || func.inst(*defining_inst).metadata.preserves_fmp()
    {
        return None;
    }

    let mut length = None;
    let mut words = FxHashMap::default();
    for inst in func.instructions() {
        if inst == *defining_inst {
            continue;
        }
        let instruction = func.inst(inst);
        if !instruction.operands().contains(&object) {
            continue;
        }
        if !func.blocks[block].instructions.contains(&inst) {
            return None;
        }
        match &instruction.kind {
            InstKind::SetMemoryObjectLen(value, len, MemoryObjectKind::Bytes)
                if *value == object =>
            {
                if length.replace(func.value_u64(*len)?).is_some() {
                    return None;
                }
            }
            InstKind::MemoryObjectStoreWord { object: value, offset, value: word }
                if *value == object =>
            {
                let offset = func.value_u64(*offset)?;
                if !offset.is_multiple_of(32)
                    || words.insert(offset, func.value_u256(*word)?).is_some()
                {
                    return None;
                }
            }
            _ => return None,
        }
    }

    if func
        .blocks
        .iter()
        .filter_map(|block| block.terminator.as_ref())
        .any(|terminator| terminator.operands().contains(&object))
    {
        return None;
    }

    let length = length?;
    let word_count = length.div_ceil(32);
    if words.len() != usize::try_from(word_count).ok()? {
        return None;
    }
    let mut bytes = Vec::with_capacity(usize::try_from(length).ok()?);
    for index in 0..word_count {
        let offset = index.checked_mul(32)?;
        let word = words.remove(&offset)?.to_be_bytes::<32>();
        let remaining = length.saturating_sub(offset).min(32) as usize;
        bytes.extend_from_slice(&word[..remaining]);
    }
    Some(bytes)
}

/// Removes a literal object's initialization after its encoding has embedded
/// the bytes directly in the output. The object must have no active use other
/// than its allocation and literal stores; otherwise the object remains live.
fn remove_literal_objects(func: &mut Function, values: &[ValueId]) {
    let mut removed = FxHashSet::default();
    for &object in values {
        let Value::Inst(defining_inst) = func.value(object) else { continue };
        let Some(block) = func
            .blocks
            .iter_enumerated()
            .find_map(|(id, block)| block.instructions.contains(defining_inst).then_some(id))
        else {
            continue;
        };
        if literal_bytes(func, object, block).is_none() {
            continue;
        }
        removed.insert(*defining_inst);
        for inst_id in func.instructions() {
            if inst_id != *defining_inst && func.inst(inst_id).operands().contains(&object) {
                removed.insert(inst_id);
            }
        }
    }
    for block in &mut func.blocks {
        block.instructions.retain(|inst| !removed.contains(inst));
    }
}

fn offset_ptr(builder: &mut FunctionBuilder<'_>, base: ValueId, offset: u64) -> ValueId {
    if offset != 0 && builder.func().value_u256(base).is_some_and(|base| base.is_zero()) {
        builder.imm(offset)
    } else {
        builder.add_u64_offset(base, offset)
    }
}
