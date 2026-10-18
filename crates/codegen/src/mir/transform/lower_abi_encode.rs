//! ABI encode lowering: expand each semantic `abi_encode` into memory stores and copies.
//!
//! The pass runs after the main optimization pipeline, once `lower-abi` has
//! turned external returns into `abi_encode` operations, and before aggregate,
//! memory-object, and allocation lowering. It writes each layout's heads and
//! dynamic tails with typed memory and slice operations. Generated
//! instructions inherit the encoding's source context, and a block split moves
//! the original terminator with its metadata.
//!
//! Placement. A static layout reserves its output first and stores into it. A
//! dynamic layout writes at the free-memory pointer, then reserves the encoded
//! extent with `preserves_fmp`, so that reservation keeps its address even
//! after later folding makes its size constant. That extent is the encoding, a
//! whole number of words, plus the padded selector, with no rounding at
//! runtime. `Scratch` encodings stage at
//! the free-memory pointer without reserving. `Bytes` results keep their
//! allocation point because source code can observe the pointer bump. A tuple
//! whose only dynamic values are literal byte strings has a known size, so it
//! reserves its output before encoding; empty literal tails keep the general
//! path, whose short encoder needs less stack setup around external calls.
//! A `bytes` argument that only holds a copy of a calldata slice, as passing
//! `bytes calldata` to a `bytes memory` parameter makes, encodes from the slice;
//! the copy's length and bytes go, while its allocation keeps its pointer bump.
//!
//! Literal objects. A `bytes memory` argument allocated and initialized with
//! constant words in the encoding's block, with no intervening opaque memory
//! access, encodes as immediate stores, and its now unused allocation is
//! removed. The candidates are proven on the original IR, before encoding
//! emits its own raw accesses.
//! Literal bytes with observable allocations can still fold into encoder stores;
//! their original allocations and initialization remain for later observers.
//! This is limited to gas-mode encodings before Cancun, excluding scratch results.
//! NOTE: Duplicating preserved payloads can grow native-MCOPY and scratch encoders;
//! keep their copies, and size-mode copies, until their full code shape is priced.
//!
//! Element loops. Full-word memory arrays and calldata word arrays copy as one
//! block. Memory words that need cleanup use a guarded do-while over a
//! destination cursor with a fixed source displacement. The original count
//! controls iteration even if addresses wrap, and loads, cleanup checks, and
//! stores keep their order. Allocations of at most one element keep a counted
//! loop, whose rotation would add an entry guard and an exit phi. Byte tails
//! zero their last padded word before writing the length header, which needs
//! no branch even for an empty tail; size mode keeps the guarded store,
//! because those tails share better under machine outlining.
//!
//! In-place returns. Under the gas and size objectives, a terminal external
//! return of a fresh full-word array or byte string that is not a literal
//! object encodes over the object itself: the payload moves forward one word
//! to make room for the tuple offset and the length word is reused, so no
//! second allocation is made. A module-wide plan selects those sites before
//! helpers are counted.
//!
//! Shared helpers. Encodings of one shape at two or more sites can call one
//! `encode_abi_tuple` helper when the cost model ranks the call protocol below
//! the expanded copies, which in practice happens under the size objective.
//! Memory arrays whose element loop appears at two or more sites share an
//! `encode_abi_array` helper, built inner layouts first. Functions reachable
//! from a constructor always encode inline: a constructor call frame is
//! allocated at the free-memory pointer, where an outlined encoder would still
//! be writing its unreserved output.

use crate::{
    mir::{
        AbiEncodeMode, AbiLayout, AbiType, AbiWordValidator, BlockId, CalldataTail, EffectKind,
        Function, FunctionBuilder, FunctionId, InstId, InstKind, InstructionMetadata,
        MemoryObjectLayout, MirType, Module, PtrLocation, Terminator, Value, ValueId,
        analysis::CallGraphInfo, memory::EvmMemoryLayout, pass::MirPass,
        transform::utils::redirect_successor_predecessors, utils::resolve_replacement,
    },
    target::Target,
};

use alloy_primitives::U256;
use smallvec::SmallVec;
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
            plan_destructive_terminal_returns(&target, module);
        let mut helpers = synthesize_array_helpers(&target, module, revert_strings);
        synthesize_tuple_helpers(
            &target,
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
                &target,
                func,
                helpers,
                revert_strings,
                &fresh_object_returns,
                &destructive_returns[func_id],
            );
        }
        CallGraphInfo::assert_runtime_helpers(
            module,
            helpers.arrays.values().chain(helpers.tuples.values()).copied(),
        );
        changed
    }
}

/// A dynamic memory array layout whose element-wise encoding loop several sites share as one
/// helper function. The key carries the MIR type the sites pass so the helper's parameter
/// matches it.
#[derive(Clone, PartialEq, Eq, Hash)]
struct ArrayHelperKey {
    element: AbiType,
    value_ty: MirType,
}

/// A whole encoding several sites share as one helper function: the same layout, storage policy,
/// selector presence, and MIR types of the values passed.
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
                .map(|&arg| func.value_ty(arg).unwrap_or(MirType::I256))
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
    target: &Target,
    module: &mut Module,
    helpers: &mut EncodeHelpers,
    revert_strings: RevertStrings,
    destructive_returns: &IndexVec<FunctionId, FxHashSet<InstId>>,
) {
    let mut counts = FxHashMap::<TupleHelperKey, (usize, usize)>::default();
    for (func_id, func) in module.functions.iter_enumerated() {
        for block in func.blocks.iter() {
            for &inst in &block.instructions {
                let InstKind::AbiEncode { mode, selector, args, layout } = &func.inst(inst).kind
                else {
                    continue;
                };
                if destructive_returns[func_id].contains(&inst)
                    || is_terminal_scratch(*mode, block.terminator.as_ref())
                {
                    continue;
                }
                let next = counts.len();
                let count = counts
                    .entry(TupleHelperKey::of(func, *mode, *selector, args, layout))
                    .or_insert((0, next));
                count.0 += 1;
            }
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
            let selector = key.selector.then(|| builder.add_param(MirType::I256));
            let layout = AbiLayout::new(key.types.clone());
            let encoded = lower_encode(&mut builder, &layout, selector, &args, key.mode, helpers);
            let result_ty = builder.func().value_ty(encoded).unwrap_or(MirType::I256);
            builder.set_return_type(result_ty);
            builder.ret([encoded]);
        }
        // A slice result returns its pointer and length.
        let returns = if matches!(function.return_type(), MirType::Slice(_)) { 2 } else { 1 };
        let body_bytes = target.code_estimate(&function).bytes;
        let sites = u32::try_from(sites).unwrap_or(u32::MAX);
        let (with_helper, expanded) =
            target.helper_sharing(body_bytes, function.params.len(), returns, sites);
        if target.cmp(with_helper, expanded).is_lt() {
            shared.push((key, function));
        }
    }
    for (key, function) in shared {
        let helper = module.add_function(function);
        helpers.tuples.insert(key, helper);
    }
}

/// Whether a scratch encoding's block returns or reverts with it: the backend's terminal-block
/// sharing already merges identical copies of such tails, so they stay expanded.
fn is_terminal_scratch(mode: AbiEncodeMode, terminator: Option<&Terminator>) -> bool {
    mode == AbiEncodeMode::Scratch
        && matches!(terminator, Some(Terminator::Revert { .. } | Terminator::ReturnData { .. }))
}

/// Builds `encode_abi_array(value, dest) -> length` for every memory array layout whose
/// element-wise loop at least two sites would otherwise expand inline. Inner layouts are built
/// first so an outer helper's element encoding calls the inner helper.
fn synthesize_array_helpers(
    target: &Target,
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
                if location == PtrLocation::Memory && array_loop_element(element).is_some() {
                    let value_ty =
                        value.and_then(|value| func.value_ty(value)).unwrap_or(MirType::I256);
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
            // Keep the destination's heap provenance inside the helper. Return its
            // length so the caller derives the tail from its own destination.
            let dest = builder.add_param(MirType::MEMPTR);
            let dest = builder.cast(dest, MirType::I256);
            let tail = encode_memory_array(&mut builder, &key.element, value, dest, &helpers);
            let length = builder.sub(tail, dest);
            builder.set_return_type(MirType::I256);
            builder.ret([length]);
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
    let value_ty = func.value_ty(value).unwrap_or(MirType::I256);
    helpers.arrays.get(&ArrayHelperKey { element: element.clone(), value_ty }).copied()
}

/// Classifies how a memory array's elements are encoded: `None` for full words, copied as one
/// block; `Some(Some(cleanup))` for words cleaned one at a time; `Some(None)` for composite
/// elements encoded one at a time.
fn array_loop_element(element: &AbiType) -> Option<Option<AbiWordValidator>> {
    match element {
        AbiType::Word(cleanup) => cleanup.map(Some),
        AbiType::Function => Some(AbiWordValidator::from_layout(crate::mir::ValueLayout::Function)),
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
}

fn lower_function(
    target: &Target,
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
    let fold_preserved = target.optimization().is_gas() && !target.evm_version().has_mcopy();
    helpers.literal_objects.retain(|&object| {
        fold_preserved
            || !matches!(func.value(object), Value::Inst(inst) if func.inst(*inst).metadata.preserves_fmp())
    });
    for inst in func.instructions() {
        if let InstKind::AbiEncode { mode: AbiEncodeMode::Scratch, args, .. } =
            &func.inst(inst).kind
        {
            for &object in args {
                if let Value::Inst(inst) = func.value(object)
                    && func.inst(*inst).metadata.preserves_fmp()
                {
                    helpers.literal_objects.remove(&object);
                }
            }
        }
    }
    let calldata_copies = calldata_copy_objects(func);

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
            // A `bytes` argument that only holds a copy of a calldata slice encodes from the
            // slice itself, so its bytes are copied once.
            let args = args
                .iter()
                .map(|&value| resolve_replacement(value, &replacements))
                .map(|value| calldata_copies.get(&value).map_or(value, |&(slice, _)| slice))
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
                let helper = (!is_terminal_scratch(mode, original_terminator.as_ref()))
                    .then(|| helpers.tuples.get(&key))
                    .flatten();
                match helper {
                    Some(&helper) => {
                        // encoded = icall @encode_abi_tuple, 1, args.., [selector]
                        let result_ty = builder.func().value_ty(result).unwrap_or(MirType::I256);
                        let call_args = args.iter().copied().chain(selector).collect();
                        builder.icall(helper, call_args, result_ty)
                    }
                    None => lower_encode(&mut builder, &layout, selector, &args, mode, helpers),
                }
            };
            literal_objects.extend(args.iter().copied());
            replacements.insert(result, replacement);
        }
        move_terminator(&mut builder, block, original_terminator, terminator_metadata);
    }
    // Every reader of such a copy now reads the slice, so its length and bytes are dead; the
    // allocation keeps its free-memory-pointer bump.
    let dead_copies =
        calldata_copies.values().flat_map(|&(_, writes)| writes).collect::<FxHashSet<_>>();
    if !dead_copies.is_empty() {
        for block in func.blocks.iter_mut() {
            block.instructions.retain(|inst| !dead_copies.contains(inst));
        }
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
    !replacements.is_empty()
}

/// Selects destructive terminal encodings before module-wide helper sharing is
/// costed. Unoptimized lowering retains the direct semantic expansion.
fn plan_destructive_terminal_returns(
    target: &Target,
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
        func.inst(inst_id).visit_operands(|operand| {
            *uses.entry(operand).or_default() += 1;
        });
    }
    for block in &func.blocks {
        if let Some(term) = &block.terminator {
            term.visit_operands(|operand| {
                *uses.entry(operand).or_default() += 1;
            });
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
                match func.inst(projection).kind.slice_projection() {
                    Some((value, MirType::SLICE_PTR_FIELD)) if value == encoded => {
                        projected_offset = Some(result)
                    }
                    Some((value, MirType::SLICE_LEN_FIELD)) if value == encoded => {
                        projected_size = Some(result)
                    }
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
fn fresh_object_returning_functions(module: &Module) -> DenseBitSet<FunctionId> {
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
        InstKind::ICall { function: crate::mir::Callee::Function(function), .. } => {
            fresh.contains(function)
        }
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
    let word_array_or_bytes = match &*layout.types {
        [AbiType::DynamicArray { element, location: PtrLocation::Memory }] => {
            matches!(element.as_ref(), AbiType::Word(None))
        }
        [AbiType::Bytes(PtrLocation::Memory)] => true,
        _ => false,
    };
    word_array_or_bytes
        && func.value_ty(*object) == Some(MirType::MEMPTR)
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

    let is_array = matches!(&*layout.types, [AbiType::DynamicArray { .. }]);
    let length = memory_object_len(builder, *object);
    let source = builder.memory_data(*object);
    let bytes = if is_array {
        let five = builder.imm(5);
        builder.shl(length, five)
    } else {
        let thirty_one = builder.imm(31);
        let rounded = builder.add(length, thirty_one);
        let mask = builder.not(thirty_one);
        let padded = builder.and(rounded, mask);
        // object: [length, bytes..., padding]
        // => [32, length, bytes..., zero padding]
        let last = builder.add(source, padded);
        let zero = builder.imm(0);
        builder.store(last, zero);
        padded
    };

    // object: [length, payload...]
    // => [32, length, payload...]
    let destination = builder.memory_data_at(*object, 32);
    let copy_size = if is_array { bytes } else { length };
    builder.copy(destination, source, copy_size, PtrLocation::Memory);
    let offset = builder.imm(32);
    builder.store(*object, offset);
    builder.store(source, length);
    let total = builder.add_u64_offset(bytes, 64);
    Some(builder.make_slice(*object, total, PtrLocation::Memory))
}

fn fresh_memory_object(
    func: &Function,
    value: ValueId,
    fresh_object_returns: &DenseBitSet<FunctionId>,
) -> bool {
    let Value::Inst(inst) = func.value(value) else { return false };
    match func.inst(*inst).kind {
        InstKind::Alloc { .. } => true,
        InstKind::ICall { function: crate::mir::Callee::Function(function), .. } => {
            fresh_object_returns.contains(function)
        }
        _ => false,
    }
}

fn fold_slice_projections(func: &Function, replacements: &mut FxHashMap<ValueId, ValueId>) {
    for inst_id in func.instructions() {
        let Some(result) = func.inst_result_value(inst_id) else { continue };
        let Some((slice, index)) = func.inst(inst_id).kind.slice_projection() else { continue };
        let slice = resolve_replacement(slice, replacements);
        let Value::Inst(make_slice) = func.value(slice) else { continue };
        let InstKind::MakeSlice { ptr, len, .. } = &func.inst(*make_slice).kind else { continue };
        let replacement = if index == MirType::SLICE_PTR_FIELD { *ptr } else { *len };
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
            // object = alloc aligned_size + 32 !preserves_fmp
            let allocation_size = builder.imm(aligned_size.saturating_add(32));
            let object = builder.alloc(allocation_size, crate::mir::AllocationSemantics::INTERNAL);
            if let Value::Inst(alloc) = *builder.func().value(object) {
                builder.func_mut().inst_mut(alloc).metadata.set_preserves_fmp(true);
            }
            let total = builder.imm(total_size);
            builder.set_memory_len(object, total);
            if let Some(selector) = selector {
                let buffer = builder.memory_data(object);
                builder.store(buffer, selector);
            }
            let dest = builder.memory_data_at(object, selector_size);
            encode_tuple(builder, args, &layout.types, dest, helpers);
            return object;
        }
        if mode == AbiEncodeMode::Slice && selector.is_none() {
            // Constructor arguments do not need a raw slice, and keeping their
            // object header lets the memory lowering preserve the established
            // allocation path for creation code.
            let allocation_size = builder.imm(aligned_size.saturating_add(32));
            let object = builder.alloc(allocation_size, crate::mir::AllocationSemantics::INTERNAL);
            let total = builder.imm(total_size);
            builder.set_memory_len(object, total);
            let data = builder.memory_data(object);
            encode_tuple(builder, args, &layout.types, data, helpers);
            return builder.make_slice(data, total, PtrLocation::Memory);
        }
        let buffer = if mode == AbiEncodeMode::Scratch {
            builder.fmp()
        } else {
            let allocation_size = builder.imm(aligned_size);
            builder.alloc(allocation_size, crate::mir::AllocationSemantics::INTERNAL)
        };
        if let Some(selector) = selector {
            builder.store(buffer, selector);
        }
        let dest = offset_ptr(builder, buffer, selector_size);
        encode_tuple(builder, args, &layout.types, dest, helpers);
        let total = builder.imm(total_size);
        return builder.make_slice(buffer, total, PtrLocation::Memory);
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
        // buffer = alloc padded_size
        // mstore buffer, selector (when present)
        // encode_tuple buffer + selector_size, args
        // make_slice buffer, total_size
        let size = builder.imm(total_size.next_multiple_of(32));
        let buffer = builder.alloc(size, crate::mir::AllocationSemantics::INTERNAL);
        if let Some(selector) = selector {
            builder.store(buffer, selector);
        }
        let dest = offset_ptr(builder, buffer, selector_size);
        encode_tuple(builder, args, &layout.types, dest, helpers);
        let total = builder.imm(total_size);
        return builder.make_slice(buffer, total, PtrLocation::Memory);
    }

    // Source objects already live below the free-memory pointer. Encode into
    // the untouched range, then reserve the exact output in one pass.
    let allocation_base = builder.fmp();
    let header_size = if mode == AbiEncodeMode::Bytes { 32 } else { 0 };
    if let Some(selector) = selector {
        let buffer = offset_ptr(builder, allocation_base, header_size);
        builder.store(buffer, selector);
    }
    let dest = offset_ptr(builder, allocation_base, header_size + selector_size);
    let encoded_size = encode_tuple(builder, args, &layout.types, dest, helpers);
    // The encoding is a whole number of words, so only the selector needs padding.
    let padded_prefix = selector_size.next_multiple_of(32);
    let selector_size = builder.imm(selector_size);
    let total = builder.add(encoded_size, selector_size);
    if mode == AbiEncodeMode::Bytes {
        // object = alloc allocation_size !metadata(preserves_fmp)
        // mstore object, total
        let allocation_size = builder.checked_padded_size(total);
        let object = builder.alloc(allocation_size, crate::mir::AllocationSemantics::INTERNAL);
        preserve_encoding_address(builder, object);
        builder.set_memory_len(object, total);
        return object;
    }
    if mode == AbiEncodeMode::Scratch {
        return builder.make_slice(allocation_base, total, PtrLocation::Memory);
    }
    // aligned = encoded_size + align32(selector_size)
    let padded_prefix = builder.imm(padded_prefix);
    let aligned = builder.add(encoded_size, padded_prefix);
    // allocated = alloc aligned !metadata(preserves_fmp)
    // make_slice allocated, total
    let allocated = builder.alloc(aligned, crate::mir::AllocationSemantics::INTERNAL);
    preserve_encoding_address(builder, allocated);
    builder.make_slice(allocated, total, PtrLocation::Memory)
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
            if *ty != AbiType::Bytes(PtrLocation::Memory) || !literal_objects.contains(&value) {
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
    if allow_slice_fast_path
        && let Some(location @ (PtrLocation::Calldata | PtrLocation::Memory)) =
            builder.func().value_slice_location(value)
    {
        let source = builder.slice_ptr(value);
        encode_static_slice(builder, ty, source, head_addr, location);
        return;
    }
    match ty {
        AbiType::Tuple(fields) => {
            let mut field_head = head_addr;
            for (index, field) in fields.iter().enumerate() {
                let field_value = builder.memory_load_field(value, index as u64);
                encode_static_impl(
                    builder,
                    field,
                    field_value,
                    field_head,
                    allow_slice_fast_path,
                    AbiValueSource::Memory,
                );
                field_head = offset_ptr(builder, field_head, field.head_size());
            }
        }
        AbiType::FixedArray { element, len } => {
            encode_static_array(builder, element, *len, value, head_addr, allow_slice_fast_path);
        }
        AbiType::Function => {
            let value = if source == AbiValueSource::Scalar {
                let shift = builder.imm(64);
                builder.shl(value, shift)
            } else {
                AbiWordValidator::from_layout(crate::mir::ValueLayout::Function)
                    .expect("function words always require cleanup")
                    .cleanup(builder, value)
            };
            builder.store(head_addr, value);
        }
        AbiType::Word(cleanup) => {
            let value = match (source, cleanup) {
                (AbiValueSource::Memory, Some(cleanup)) => clean_word(builder, *cleanup, value),
                _ => value,
            };
            builder.store(head_addr, value);
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
) {
    let length = builder.imm(len);
    let stride = builder.imm(element.head_size());
    let layout = MemoryObjectLayout::word_fixed_array(len);
    builder.counted_loop(length, |builder, index| {
        let element_value = builder.memory_load_element(value, layout, index);
        let offset = builder.mul(index, stride);
        let element_head = builder.add(head_addr, offset);
        encode_static_impl(
            builder,
            element,
            element_value,
            element_head,
            allow_slice_fast_path,
            AbiValueSource::Memory,
        );
    });
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
        builder.store(dest.head_addr, relative);
        encode_dynamic_body(builder, ty, value, dest.tail, helpers)
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
    location: PtrLocation,
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
            let value = builder.load(source, location);
            let value = if location == PtrLocation::Memory {
                AbiWordValidator::from_layout(crate::mir::ValueLayout::Function)
                    .expect("function words always require cleanup")
                    .cleanup(builder, value)
            } else {
                value
            };
            builder.store(head_addr, value);
        }
        AbiType::Word(cleanup) => {
            let value = builder.load(source, location);
            let value = match cleanup {
                Some(cleanup) if location == PtrLocation::Memory => {
                    clean_word(builder, *cleanup, value)
                }
                _ => value,
            };
            builder.store(head_addr, value);
        }
        AbiType::Bytes(_) | AbiType::DynamicArray { .. } => {
            unreachable!("dynamic ABI values are not static")
        }
    }
}

/// Canonicalizes a word read from memory before it is encoded: narrow values are masked or
/// sign-extended and an out-of-range enum panics. Scalars arriving on the stack were already
/// cleaned by the lowering, and calldata words were validated when decoded.
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

fn encode_dynamic_body(
    builder: &mut FunctionBuilder<'_>,
    ty: &AbiType,
    value: ValueId,
    dest: ValueId,
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
            if location == PtrLocation::Memory {
                if let Some(helper) = array_helper(builder.func(), helpers, element, value) {
                    let pointer = builder.cast(dest, MirType::MEMPTR);
                    let length = builder.icall(helper, vec![value, pointer], MirType::I256);
                    return builder.add(dest, length);
                }
                return encode_memory_array(builder, element, value, dest, helpers);
            }
            let word_cleanup = match element.as_ref() {
                AbiType::Word(cleanup) => Some(*cleanup),
                AbiType::Function => {
                    Some(AbiWordValidator::from_layout(crate::mir::ValueLayout::Function))
                }
                _ => None,
            };
            if let Some(cleanup) = word_cleanup {
                return encode_word_array(builder, value, dest, location, cleanup);
            }
            match location {
                PtrLocation::Calldata => {
                    if matches!(element.as_ref(), AbiType::Bytes(_)) {
                        encode_calldata_bytes_array(builder, element, value, dest, helpers)
                    } else {
                        unreachable!(
                            "non-word calldata arrays are materialized before ABI encoding"
                        )
                    }
                }
                _ => unreachable!("only calldata arrays reach slice encoding"),
            }
        }
        AbiType::FixedArray { element, len } => encode_memory_array_elements(
            builder,
            element,
            value,
            dest,
            MemoryObjectLayout::word_fixed_array(*len),
            helpers,
        ),
        AbiType::Tuple(fields) => {
            let values = (0..fields.len())
                .map(|index| builder.memory_load_field(value, index as u64))
                .collect::<Vec<_>>();
            let size =
                encode_tuple_impl(builder, &values, fields, dest, AbiValueSource::Memory, helpers);
            builder.add(dest, size)
        }
        AbiType::Word(_) | AbiType::Function => unreachable!("word ABI values are static"),
    }
}

fn effective_slice_location(func: &Function, value: ValueId, declared: PtrLocation) -> PtrLocation {
    match func.value_ty(value) {
        Some(MirType::Slice(location)) => location,
        Some(MirType::I256 | MirType::MEMPTR) => PtrLocation::Memory,
        _ if func.memory_object_load(value) => PtrLocation::Memory,
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
            helpers,
        ),
        cleanup => encode_word_array(builder, value, dest, PtrLocation::Memory, cleanup.flatten()),
    }
}

fn encode_memory_array_elements(
    builder: &mut FunctionBuilder<'_>,
    element: &AbiType,
    value: ValueId,
    dest: ValueId,
    layout: MemoryObjectLayout,
    helpers: &EncodeHelpers,
) -> ValueId {
    let (len, element_area) = match layout {
        MemoryObjectLayout::DynamicArray { .. } => {
            let len = memory_object_len(builder, value);
            builder.store(dest, len);
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
    let source_cursor = match layout {
        MemoryObjectLayout::FixedArray { .. } => builder.cast(value, MirType::I256),
        _ => builder.memory_data(value),
    };
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
    let element_value = builder.load(source, PtrLocation::Memory);
    let new_tail = encode_value(
        builder,
        element,
        element_value,
        AbiValueDest { head_addr: element_head, tuple_base: element_area, tail: current_tail },
        AbiValueSource::Memory,
        helpers,
    );

    let one = builder.imm(1);
    let next_remaining = builder.sub(remaining, one);
    let next_head = builder.add(element_head, element_head_size);
    let next_source = builder.add(source, word);
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
    builder.store(dest, len);

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
    // offset = calldataload(source_head)
    // check_calldata_tail_offset(source_base, offset)
    // element_base = source_base + offset
    // length = calldataload(element_base)
    // data = calldata_tail_data(element_base + 32, length, 1)
    let tail = CalldataTail::Encode;
    let offset = builder.load(source_head, PtrLocation::Calldata);
    let calldata_end = builder.calldatasize();
    builder.check_calldata_tail_offset(tail, source_base, offset, calldata_end, 32);
    let element_base = builder.add(source_base, offset);
    let length = builder.load(element_base, PtrLocation::Calldata);
    let data = builder.calldata_tail_data(tail, element_base, 32, length, calldata_end, 1);
    let element_value = builder.make_slice(data, length, PtrLocation::Calldata);
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
    location: PtrLocation,
    cleanup: Option<AbiWordValidator>,
) -> ValueId {
    let len = match location {
        PtrLocation::Memory => memory_object_len(builder, value),
        _ => builder.slice_len(value),
    };
    builder.store(dest, len);
    let word = builder.imm(32);
    let bytes = builder.mul(len, word);
    let data_dest = builder.add(dest, word);
    let data_source = match location {
        PtrLocation::Memory => builder.memory_data(value),
        _ => builder.slice_ptr(value),
    };
    let tail = builder.add(data_dest, bytes);
    // Memory elements that need cleanup are copied one word at a time; full words and calldata
    // elements copy as one block.
    if let Some(cleanup) = cleanup
        && location == PtrLocation::Memory
    {
        // This is a profitability hint, not a length fact: assembly may have
        // overwritten the length since allocation. Both loops use the loaded
        // length and preserve the same addresses and memory-operation order.
        let small_allocation = matches!(builder.func().value(value), Value::Inst(inst)
            if matches!(builder.func().inst(*inst).kind, InstKind::Alloc { size, .. }
                if builder.func().value_u64(size).is_some_and(|size| size <= 64)));
        if small_allocation {
            // for index in 0..len:
            //   store data_dest + index * 32, clean(load data_source + index * 32)
            builder.counted_loop(len, |builder, index| {
                let offset = builder.mul(index, word);
                let source = builder.add(data_source, offset);
                let destination = builder.add(data_dest, offset);
                let value = builder.load(source, PtrLocation::Memory);
                let value = clean_word(builder, cleanup, value);
                builder.store(destination, value);
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
        let value = builder.load(source, PtrLocation::Memory);
        let value = clean_word(builder, cleanup, value);
        builder.store(destination, value);
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
    copy_source_data(builder, location, data_dest, data_source, bytes);
    tail
}

fn encode_bytes(
    builder: &mut FunctionBuilder<'_>,
    value: ValueId,
    dest: ValueId,
    location: PtrLocation,
    literal_folding: bool,
    branchless_padding: bool,
) -> ValueId {
    if literal_folding
        && location == PtrLocation::Memory
        && let Some(bytes) = literal_bytes(builder.func(), value, builder.current_block())
    {
        let length = builder.imm(bytes.len() as u64);
        builder.store(dest, length);
        let data = builder.add_u64_offset(dest, 32);
        for (index, chunk) in bytes.chunks(32).enumerate() {
            let mut padded = [0_u8; 32];
            padded[..chunk.len()].copy_from_slice(chunk);
            let word = builder.imm(U256::from_be_bytes(padded));
            let offset = builder.imm(index as u64 * 32);
            let address = builder.add(data, offset);
            builder.store(address, word);
        }
        let size = builder.imm(bytes.len().next_multiple_of(32) as u64);
        return builder.add(data, size);
    }

    let len = match location {
        PtrLocation::Memory => memory_object_len(builder, value),
        _ => builder.slice_len(value),
    };
    if !branchless_padding {
        // mstore dest, len
        builder.store(dest, len);
    }

    let word = builder.imm(32);
    let thirty_one = builder.imm(31);
    let mask = builder.not(thirty_one);
    let rounded = builder.add(len, thirty_one);
    let padded = builder.and(rounded, mask);
    let data_dest = builder.add(dest, word);

    if branchless_padding {
        // last = dest + padded
        // store last, 0
        // mstore dest, len
        // For an empty tail, last == dest; write the header after zeroing it.
        let last = builder.add(dest, padded);
        let zero = builder.imm(0);
        builder.store(last, zero);
        builder.store(dest, len);
    } else {
        // if padded != 0: mstore dest + padded, 0
        let zero_block = builder.create_block();
        let copy_block = builder.create_block();
        let empty = builder.eq_zero(padded);
        builder.branch(empty, copy_block, zero_block);
        builder.switch_to_block(zero_block);
        let last = builder.add(dest, padded);
        let zero = builder.imm(0);
        builder.store(last, zero);
        builder.jump(copy_block);
        builder.switch_to_block(copy_block);
    }
    let data_source = match location {
        PtrLocation::Memory => builder.memory_data(value),
        _ => builder.slice_ptr(value),
    };
    let tail = builder.add(data_dest, padded);
    copy_source_data(builder, location, data_dest, data_source, len);
    tail
}

/// Copies a source value's data into the output buffer.
///
/// Source objects live below the free-memory pointer, and the output is either untouched memory
/// above it or a fresh allocation, so memory copies run forward.
fn copy_source_data(
    builder: &mut FunctionBuilder<'_>,
    location: PtrLocation,
    dest: ValueId,
    source: ValueId,
    size: ValueId,
) {
    // mcopy|calldatacopy|returndatacopy dest, source, size [!metadata(disjoint) for memory]
    let copy = builder.copy_heap(dest, source, size, location);
    if location == PtrLocation::Memory {
        builder.func_mut().inst_mut(copy).metadata.set_disjoint(true);
    }
}

fn memory_object_len(builder: &mut FunctionBuilder<'_>, value: ValueId) -> ValueId {
    if builder.func().value_slice_location(value).is_some() {
        // len = extract_value value, 1
        return builder.slice_len(value);
    }
    builder.memory_len(value)
}

/// Finds literal bytes candidates whose allocation and encoding share a block, with
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
            if literal_opaque_instruction(func, kind, &available) {
                available.clear();
            }
            if matches!(kind, InstKind::Alloc { .. })
                && let Some(object) = func.inst_result_value(inst)
            {
                available.insert(object);
            }
            if let InstKind::AbiEncode { args, layout, .. } = kind {
                for (&object, ty) in args.iter().zip(layout.types.iter()) {
                    if *ty == AbiType::Bytes(PtrLocation::Memory) {
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

fn literal_opaque_instruction(
    func: &Function,
    kind: &InstKind,
    available: &FxHashSet<ValueId>,
) -> bool {
    match kind {
        InstKind::Alloc { .. } | InstKind::AbiEncode { .. } => false,
        InstKind::Store(address, _, PtrLocation::Memory) => {
            !literal_word_in_bounds(func, *address, available)
        }
        InstKind::MSize => true,
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

/// A store is disjoint only within an allocation made since the last opaque access.
/// Older allocations may alias new ones after a free-memory-pointer reset.
fn literal_word_in_bounds(
    func: &Function,
    address: ValueId,
    available: &FxHashSet<ValueId>,
) -> bool {
    let Some((object, base)) = func.object_address(address) else { return false };
    available.contains(&object)
        && base
            .checked_add(32)
            .is_some_and(|end| func.alloc_size(object).is_some_and(|size| end <= size))
}

/// Instructions other than its allocation that use a literal object or an address into it.
fn literal_object_uses(func: &Function, object: ValueId) -> impl Iterator<Item = InstId> + '_ {
    func.instructions().filter(move |&inst| {
        func.inst_result_value(inst) != Some(object)
            && func.inst(inst).operands().iter().any(|&operand| {
                func.object_address(operand).is_some_and(|(base, _)| base == object)
            })
    })
}

/// Returns the bytes represented by an immutable literal object when all active
/// uses are initialization operations preceding this encoding in the same block.
/// The caller must also exclude opaque observers using the original IR.
fn literal_bytes(func: &Function, object: ValueId, block: BlockId) -> Option<Vec<u8>> {
    if func.value_ty(object) != Some(MirType::MEMPTR) {
        return None;
    }
    let Value::Inst(defining_inst) = func.value(object) else { return None };
    if !func.blocks[block].instructions.contains(defining_inst) {
        return None;
    }
    if !matches!(func.inst(*defining_inst).kind, InstKind::Alloc { .. }) {
        return None;
    }

    let mut length = None;
    let mut words = FxHashMap::default();
    for inst in literal_object_uses(func, object) {
        if !func.blocks[block].instructions.contains(&inst) {
            return None;
        }
        if func.is_object_derivation(inst) {
            continue;
        }
        // Payload offset and stored word.
        let (offset, word) = match func.inst(inst).kind {
            InstKind::Store(address, word, PtrLocation::Memory) => {
                let (base, offset) = func.object_address(address)?;
                (base == object).then_some(())?;
                if offset == 0 {
                    if length.replace(func.value_u64(word)?).is_some() {
                        return None;
                    }
                    continue;
                }
                (offset.checked_sub(EvmMemoryLayout::DYNAMIC_HEADER_SIZE)?, word)
            }
            _ => return None,
        };
        if !offset.is_multiple_of(32) || words.insert(offset, func.value_u256(word)?).is_some() {
            return None;
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
        if func.inst(*defining_inst).metadata.preserves_fmp() {
            continue;
        }
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
        removed.extend(literal_object_uses(func, object));
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

/// Memory objects that hold nothing but an unmodified copy of a calldata slice and are read only
/// as top-level `bytes` arguments of encodings, mapped to that slice. Materializing such an
/// argument for a `bytes memory` parameter copies it once from calldata; encoding the copy then
/// copies it again.
fn calldata_copy_objects(func: &Function) -> FxHashMap<ValueId, (ValueId, [InstId; 2])> {
    #[derive(Default)]
    struct Uses {
        fill: Option<(ValueId, BlockId, usize)>,
        lengths: Vec<(ValueId, InstId)>,
        encodes: Vec<(BlockId, usize)>,
        escapes: bool,
    }
    let is_object = |value: ValueId| {
        func.value_ty(value) == Some(MirType::MEMPTR)
            && matches!(func.value(value), Value::Inst(inst)
                if matches!(func.inst(*inst).kind, InstKind::Alloc { .. }))
    };
    let mut uses = FxHashMap::<ValueId, Uses>::default();
    for (block, body) in func.blocks.iter_enumerated() {
        for (position, &inst) in body.instructions.iter().enumerate() {
            if func.is_object_derivation(inst) {
                continue;
            }
            let mut objects = SmallVec::<[ValueId; 2]>::new();
            for operand in func.inst(inst).operands() {
                if let Some((object, _)) = func.object_address(operand)
                    && is_object(object)
                    && !objects.contains(&object)
                {
                    objects.push(object);
                }
            }
            for object in objects {
                let entry = uses.entry(object).or_default();
                if let Some((base, length)) = func.memory_length_store(inst)
                    && base == object
                {
                    entry.lengths.push((length, inst));
                    continue;
                }
                match func.inst(inst).kind {
                    InstKind::Copy(destination, data, len, PtrLocation::Calldata)
                        if func.object_address(destination)
                            == Some((object, EvmMemoryLayout::DYNAMIC_HEADER_SIZE))
                            && entry.fill.is_none()
                            && let Some(slice) =
                                super::lower_abi::calldata_slice_copy(func, data, len) =>
                    {
                        entry.fill = Some((slice, block, position));
                    }
                    InstKind::AbiEncode { ref args, ref layout, selector, .. }
                        if selector != Some(object)
                            && args.iter().zip(&layout.types).all(|(&arg, ty)| {
                                func.object_address(arg).is_none_or(|(base, _)| base != object)
                                    || (arg == object && matches!(ty, AbiType::Bytes(_)))
                            }) =>
                    {
                        entry.encodes.push((block, position));
                    }
                    _ => entry.escapes = true,
                }
            }
        }
        if let Some(terminator) = &body.terminator {
            for operand in terminator.operands() {
                if let Some((object, _)) = func.object_address(operand)
                    && let Some(entry) = uses.get_mut(&object)
                {
                    entry.escapes = true;
                }
            }
        }
    }
    uses.into_iter()
        .filter_map(|(object, uses)| {
            let (slice, fill_block, fill_position) = uses.fill?;
            let [(length, length_store)] = uses.lengths[..] else { return None };
            let fill = func.blocks[fill_block].instructions[fill_position];
            let Value::Inst(length) = func.value(length) else { return None };
            let Value::Inst(alloc) = func.value(object) else { return None };
            // The copy fills the object in its allocating block, before any encoding there reads
            // it, and nothing else writes, measures, or passes it on.
            (!uses.escapes
                && !uses.encodes.is_empty()
                && func.inst(*length).kind.slice_projection()
                    == Some((slice, MirType::SLICE_LEN_FIELD))
                && func.blocks[fill_block].instructions.contains(alloc)
                && uses
                    .encodes
                    .iter()
                    .all(|&(block, position)| block != fill_block || position > fill_position))
            .then_some((object, (slice, [length_store, fill])))
        })
        .collect()
}
