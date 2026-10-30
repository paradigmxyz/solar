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
//!
//! Literal objects. A `bytes memory` argument allocated and initialized with
//! constant words in the encoding's block, with no intervening opaque memory
//! access, encodes as immediate stores, and its now unused allocation is
//! removed. The candidates are proven on the original IR, before encoding
//! emits its own raw accesses.
//! Literal bytes with observable allocations can still fold into encoder
//! stores, in slice and scratch encodings alike; their original allocations
//! and initialization remain for later observers. A revert or return of a
//! scratch encoding of words and one literal, with only pure instructions
//! between them, has no observer, so the literal's allocation goes too rather
//! than repeating the payload as stores beside the folded copy. The cost
//! model decides each preserved literal (`Target::folds_preserved_literal`):
//! gas mode folds before Cancun, where the copy is a loop; size mode folds
//! every unobserved literal and any other whose stores take fewer bytes than
//! the length computation and copy they replace, with or without native MCOPY.
//!
//! Element loops. Full-word memory arrays and calldata word arrays copy as one
//! block. Memory words that need cleanup use a guarded do-while over a
//! destination cursor with a fixed source displacement. The original count
//! controls iteration even if addresses wrap, and loads, cleanup checks, and
//! stores keep their order. Allocations of at most one element keep a counted
//! loop, whose rotation would add an entry guard and an exit phi. Byte tails
//! zero their last padded word before writing the length header, which needs
//! no branch even for an empty tail.
//!
//! Calldata aggregates. A struct or array in calldata, with dynamic members at any depth,
//! encodes straight from where it lies instead of through a memory copy. Offsets in calldata
//! need not be canonical, so each dynamic member is followed through its offset, encoded at the
//! next tail, and given a new offset in its head. The checks are those of solc's ABI encoder:
//! each tail offset must leave room for the member's head, each length must fit in 64 bits, each
//! array must end inside calldata, and a narrow word that is not canonical reverts, where
//! the same word read from memory is cleaned. Static elements that need no validation copy as
//! one block, as solc copies an array of `uint256` or `bytes32`; solc loads every other static
//! element one word at a time, so a range that wraps past 2^256 sends such arrays to an
//! element loop, which reads each word at its own wrapping address. Under the size objective
//! the loop is the only code, because the guard and the copy cost more bytes than they save.
//!
//! Zero-copy returns. Under the gas and size objectives, a terminal external
//! return of a full-word array or byte string that is not a literal object
//! encodes without copying the payload. The object already holds the length
//! word and the payload, so the encoding writes the tuple offset `0x20` into
//! the word just before the object, zeroes the padding after a byte string's
//! data, and returns from that word through the padded payload. `RETURN` ends
//! the frame, so the clobbered word before the object and the memory after
//! its data have no observer. A module-wide plan selects those sites before
//! helpers are counted. A fresh allocation lies at or above the heap floor, so
//! its preceding word exists and the form is unconditional. Any other object,
//! such as a string an internal call built in assembly, may sit below `32`,
//! where the preceding word wraps; the site then tests the pointer and sends
//! such an object to the copying encoder. The cost model decides whether that
//! guarded form beats the copy alone (`Target::guards_zero_copy_return`); it
//! keeps both encodings, so it pays for gas, not for size, and its site is left out of
//! the shared helpers.
//!
//! Shared helpers. Encodings of one shape at two or more sites can call one
//! `encode_abi_tuple` helper when the cost model ranks the call protocol below
//! the expanded copies, which in practice happens under the size objective.
//! Memory arrays whose element loop appears at two or more sites share an
//! `encode_abi_array` helper, built inner layouts first. Calldata arrays of composite elements,
//! fixed arrays of dynamic elements, and structs with dynamic members share an
//! `encode_abi_calldata` helper when the cost model ranks a call below the expansions, which
//! covers a layout nested in several larger ones as well as one encoded at several sites. Functions
//! reachable from a constructor always encode inline: a constructor call frame is
//! allocated at the free-memory pointer, where an outlined encoder would still
//! be writing its unreserved output.

use crate::{
    mir::{
        AbiEncodeMode, AbiLayout, AbiType, AbiWordValidator, BlockId, CalldataTail, EffectKind,
        Function, FunctionBuilder, FunctionId, InstId, InstKind, MemoryObjectLayout,
        MirModuleTarget, MirType, Module, PtrLocation, RevertReason, Terminator, Value, ValueId,
        memory::EvmMemoryLayout,
        pass::{MirPass, ModuleAnalyses, run_lowering_function_pass},
        transform::{egraph::use_counts, utils::expand_block},
        utils::resolve_replacement,
    },
    target::Target,
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
use std::sync::Arc;

/// Lowers `abi_encode` after the main optimization pipeline.
pub(crate) struct LowerAbiEncode;

impl MirPass for LowerAbiEncode {
    fn name(&self) -> &'static str {
        "lower-abi-encode"
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module, analyses: &mut ModuleAnalyses) -> bool {
        if !module.functions.iter().any(has_abi_encode) {
            return false;
        }
        let revert_strings = gcx.sess.opts.revert_strings;
        let target = module.target(gcx);
        let mut zero_copy_returns = plan_zero_copy_terminal_returns(&target, module);
        let mut helpers = synthesize_array_helpers(&target, module, revert_strings);
        synthesize_calldata_helpers(&target, module, &mut helpers, revert_strings);
        synthesize_tuple_helpers(&target, module, &mut helpers, revert_strings, &zero_copy_returns);
        zero_copy_returns.resize_with(module.functions.len(), FxHashMap::default);
        let has_helpers = !helpers.arrays.is_empty()
            || !helpers.calldata.is_empty()
            || !helpers.tuples.is_empty();
        let constructors = has_helpers.then(|| module.constructor_reachable_functions());
        let inline_helpers = EncodeHelpers::new(&target);
        let mut changed = has_helpers;
        changed |= run_lowering_function_pass(module, analyses, |func, function_analyses| {
            let func_id = function_analyses.function();
            // NOTE: Constructor calls allocate frames at the free-memory pointer. Encoding
            // writes there before reserving its output, so an outlined call would overwrite it.
            // abi_encode(args) => inline head stores and tail copies
            let helpers = if constructors.as_ref().is_some_and(|ids| ids.contains(func_id)) {
                &inline_helpers
            } else {
                &helpers
            };
            lower_function(func, helpers, revert_strings, &zero_copy_returns[func_id])
        });
        module.assert_runtime_helpers(
            helpers
                .arrays
                .values()
                .chain(helpers.calldata.values())
                .chain(helpers.tuples.values())
                .copied(),
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

/// Shared encoder helpers available to every site in the module. Each function lowers with its
/// own copy, which adds the literal objects it proves.
#[derive(Clone)]
struct EncodeHelpers {
    arrays: Arc<FxHashMap<ArrayHelperKey, FunctionId>>,
    /// Keyed by the layout with every location set to calldata.
    calldata: Arc<FxHashMap<AbiType, FunctionId>>,
    tuples: Arc<FxHashMap<TupleHelperKey, FunctionId>>,
    /// Proven on the original function, before encoding emits raw memory operations.
    literal_objects: FxHashSet<ValueId>,
    target: Target,
}

impl EncodeHelpers {
    fn new(target: &Target) -> Self {
        Self {
            arrays: Arc::default(),
            calldata: Arc::default(),
            tuples: Arc::default(),
            literal_objects: FxHashSet::default(),
            target: *target,
        }
    }
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
    zero_copy_returns: &IndexVec<FunctionId, FxHashMap<InstId, ZeroCopyReturn>>,
) {
    let mut counts = FxHashMap::<TupleHelperKey, (usize, usize, bool)>::default();
    for (func_id, func) in module.functions.iter_enumerated() {
        for block in func.blocks.iter() {
            for &inst in &block.instructions {
                let InstKind::AbiEncode { mode, selector, args, layout } = &func.inst(inst).kind
                else {
                    continue;
                };
                if zero_copy_returns[func_id].contains_key(&inst)
                    || (*mode == AbiEncodeMode::Scratch && ends_function(block.terminator.as_ref()))
                {
                    continue;
                }
                let next = counts.len();
                let count = counts
                    .entry(TupleHelperKey::of(func, *mode, *selector, args, layout))
                    .or_insert((0, next, true));
                count.0 += 1;
                count.2 &= block.hotness.is_cold();
            }
        }
    }
    let mut keys = counts
        .into_iter()
        .filter(|(_, (count, _, _))| *count >= 2)
        .map(|(key, (count, first, cold))| (first, count, key, cold))
        .collect::<Vec<_>>();
    keys.sort_by_key(|(first, _, _, _)| *first);

    let mut shared = Vec::new();
    for (_, sites, key, cold) in keys {
        // A shape that only cold sites encode is priced like a size build.
        let target = &target.priced_for_cold(cold);
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
        Arc::make_mut(&mut helpers.tuples).insert(key, helper);
    }
}

fn has_abi_encode(func: &Function) -> bool {
    func.instructions().any(|inst| matches!(func.inst(inst).kind, InstKind::AbiEncode { .. }))
}

/// Whether a block returns or reverts. The backend's terminal-block sharing already merges
/// identical copies of a scratch encoding in such a block, so those stay expanded.
fn ends_function(terminator: Option<&Terminator>) -> bool {
    matches!(terminator, Some(Terminator::Revert { .. } | Terminator::ReturnData { .. }))
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
        if value.is_some_and(|value| is_calldata_slice(func, value)) {
            return;
        }
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
            AbiType::FixedArray { element, len, .. } => count_sites(
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

    let mut helpers = EncodeHelpers::new(target);
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
        Arc::make_mut(&mut helpers.arrays).insert(key, helper);
    }
    helpers
}

/// Builds `encode_abi_calldata(value, dest) -> length` for every composite calldata layout, an
/// array of non-word elements, a fixed array of dynamic elements, or a struct with dynamic
/// members, whose expansion the cost model ranks below a call. Like solc's per-type
/// `abi_encode_*_calldata` functions, one helper serves every site that encodes the layout
/// as a value or as a member of a larger layout.
///
/// Layouts are decided from the outermost in. A layout that is shared is expanded once, in its
/// helper, so the layouts it contains count one expansion per occurrence in it rather than one
/// per site that calls it. Each candidate body is priced with its contents expanded, and the
/// helpers that are kept are built from the innermost out so that outer bodies call inner ones.
fn synthesize_calldata_helpers(
    target: &Target,
    module: &mut Module,
    helpers: &mut EncodeHelpers,
    revert_strings: RevertStrings,
) {
    fn collect(ty: &AbiType, layouts: &mut Vec<AbiType>) {
        if is_calldata_helper_layout(ty) && !layouts.contains(ty) {
            layouts.push(ty.clone());
            calldata_children(ty).for_each(|child| collect(child, layouts));
        }
    }

    // Layouts of the calldata values encoded, in order of first appearance, and the number of
    // sites encoding each.
    let mut layouts = Vec::new();
    let mut sites = FxHashMap::<AbiType, usize>::default();
    let mut hot = FxHashSet::<AbiType>::default();
    for func in module.functions.iter() {
        for block in func.blocks.iter() {
            for &inst in &block.instructions {
                let InstKind::AbiEncode { args, layout, .. } = &func.inst(inst).kind else {
                    continue;
                };
                for (&arg, ty) in args.iter().zip(&layout.types) {
                    if is_calldata_slice(func, arg) && is_calldata_helper_layout(ty) {
                        let ty = ty.clone().at_location(PtrLocation::Calldata);
                        collect(&ty, &mut layouts);
                        if !block.hotness.is_cold() {
                            hot.insert(ty.clone());
                        }
                        *sites.entry(ty).or_default() += 1;
                    }
                }
            }
        }
    }
    layouts.sort_by_key(|ty| std::cmp::Reverse(calldata_depth(ty)));

    // Every layout that contains this one has been decided, so its expansions are known.
    let mut shared = Vec::new();
    for layout in layouts {
        let expansions = sites.get(&layout).copied().unwrap_or(0);
        // A layout that no hot site or hot layout encodes is priced like a size build.
        let is_hot = hot.contains(&layout);
        if is_hot {
            hot.extend(calldata_children(&layout).cloned());
        }
        let target = &target.priced_for_cold(!is_hot);
        if expansions >= 2 {
            let function = calldata_helper(&layout, revert_strings, helpers);
            let body_bytes = target.code_estimate(&function).bytes;
            let expansions = u32::try_from(expansions).unwrap_or(u32::MAX);
            let params = function.params.len();
            let (with_helper, expanded) = target.helper_sharing(body_bytes, params, 1, expansions);
            if target.cmp(with_helper, expanded).is_lt() {
                // A shared layout expands once, in its helper.
                for child in calldata_children(&layout) {
                    *sites.entry(child.clone()).or_default() += 1;
                }
                shared.push(layout);
                continue;
            }
        }
        for child in calldata_children(&layout) {
            *sites.entry(child.clone()).or_default() += expansions;
        }
    }
    // Inner layouts first, so outer bodies call inner helpers.
    for layout in shared.into_iter().rev() {
        let function = calldata_helper(&layout, revert_strings, helpers);
        let helper = module.add_function(function);
        Arc::make_mut(&mut helpers.calldata).insert(layout, helper);
    }
}

/// Builds the helper encoding the calldata layout `ty`, calling the helpers `helpers` has for
/// the layouts inside it.
fn calldata_helper(
    ty: &AbiType,
    revert_strings: RevertStrings,
    helpers: &EncodeHelpers,
) -> Function {
    let mut function = Function::new(Ident::with_dummy_span(sym::encode_abi_calldata));
    {
        let mut builder = FunctionBuilder::new(&mut function).with_revert_strings(revert_strings);
        let value = builder.add_param(MirType::Slice(PtrLocation::Calldata));
        let dest = builder.add_param(MirType::MEMPTR);
        let dest = builder.cast(dest, MirType::I256);
        let tail = encode_calldata_body(&mut builder, ty, value, dest, helpers);
        let length = builder.sub(tail, dest);
        builder.set_return_type(MirType::I256);
        builder.ret([length]);
    }
    function
}

/// Whether `ty` is a calldata layout with a helper of its own: it has a loop or several members
/// to encode, unlike a byte string or an array of words, which copy as one block.
fn is_calldata_helper_layout(ty: &AbiType) -> bool {
    match ty {
        AbiType::DynamicArray { element, .. } => !matches!(element.as_ref(), AbiType::Word(_)),
        AbiType::FixedArray { .. } | AbiType::Tuple(_) => ty.is_dynamic(),
        AbiType::Word(_) | AbiType::Function | AbiType::Bytes(_) => false,
    }
}

/// The layouts directly inside `ty` that have helpers of their own.
fn calldata_children(ty: &AbiType) -> impl Iterator<Item = &AbiType> {
    let children: &[AbiType] = match ty {
        AbiType::DynamicArray { element, .. } | AbiType::FixedArray { element, .. } => {
            std::slice::from_ref(element)
        }
        AbiType::Tuple(fields) => fields,
        AbiType::Word(_) | AbiType::Function | AbiType::Bytes(_) => &[],
    };
    children.iter().filter(|child| is_calldata_helper_layout(child))
}

/// Number of calldata layouts nested inside `ty`, counting itself.
fn calldata_depth(ty: &AbiType) -> usize {
    1 + calldata_children(ty).map(calldata_depth).max().unwrap_or(0)
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
    func: &mut Function,
    helpers: &EncodeHelpers,
    revert_strings: RevertStrings,
    zero_copy_returns: &FxHashMap<InstId, ZeroCopyReturn>,
) -> bool {
    if !has_abi_encode(func) {
        return false;
    }
    let mut helpers = helpers.clone();
    let target = helpers.target;
    helpers.literal_objects = literal_objects_at_encodes(func);
    let unobserved = terminal_scratch_literals(func, &helpers.literal_objects);
    helpers.literal_objects.retain(|&object| {
        let Value::Inst(inst) = func.value(object) else { return true };
        !func.inst(*inst).metadata.preserves_fmp()
            || target.folds_preserved_literal(unobserved.contains(&object), || {
                literal_bytes(func, object, defining_block(func, *inst)?)
            })
    });
    let mut terminal_literals = unobserved;
    terminal_literals.retain(|object| helpers.literal_objects.contains(object));

    let mut replacements = FxHashMap::default();
    let mut literal_objects = FxHashSet::default();
    let blocks = func.blocks.indices();
    for block in blocks {
        if !func.blocks[block]
            .instructions
            .iter()
            .any(|&inst| matches!(func.inst(inst).kind, InstKind::AbiEncode { .. }))
        {
            continue;
        }
        let terminal = ends_function(func.blocks[block].terminator.as_ref());
        let mut builder = FunctionBuilder::new(func).with_revert_strings(revert_strings);
        expand_block(&mut builder, block, |builder, inst| {
            let InstKind::AbiEncode { mode, selector, args, layout } =
                &builder.func().inst(inst).kind
            else {
                return false;
            };
            let selector = selector.map(|value| resolve_replacement(value, &replacements));
            let args = args
                .iter()
                .map(|&value| resolve_replacement(value, &replacements))
                .collect::<Vec<_>>();
            let layout = Arc::clone(layout);
            let mode = *mode;
            // abi_encode(args) !metadata(span) => icall or stores/copies !metadata(span)
            let metadata = builder.func().inst(inst).metadata.clone();
            builder.set_debug_context(&metadata);
            let result =
                builder.func().inst_result_value(inst).expect("ABI encode must produce a value");
            let replacement = if let Some(&form) = zero_copy_returns.get(&inst) {
                encode_zero_copy_return(builder, &layout, &args, mode, form, &helpers)
            } else {
                let helper = (!helpers.tuples.is_empty()
                    && !(mode == AbiEncodeMode::Scratch && terminal))
                    .then(|| {
                        let key =
                            TupleHelperKey::of(builder.func(), mode, selector, &args, &layout);
                        helpers.tuples.get(&key)
                    })
                    .flatten();
                match helper {
                    Some(&helper) => {
                        // encoded = icall @encode_abi_tuple, 1, args.., [selector]
                        let result_ty = builder.func().value_ty(result).unwrap_or(MirType::I256);
                        let call_args = args.iter().copied().chain(selector).collect();
                        builder.icall(helper, call_args, result_ty)
                    }
                    None => lower_encode(builder, &layout, selector, &args, mode, &helpers),
                }
            };
            literal_objects.extend(args.iter().copied());
            replacements.insert(result, replacement);
            true
        });
    }
    fold_slice_projections(func, &mut replacements);
    func.replace_uses_canonicalized(&replacements);
    remove_literal_objects(
        func,
        &literal_objects
            .into_iter()
            .filter(|object| helpers.literal_objects.contains(object))
            .collect::<Vec<_>>(),
        &terminal_literals,
    );
    !replacements.is_empty()
}

/// How a terminal return writes its tuple offset before the returned object.
#[derive(Clone, Copy)]
enum ZeroCopyReturn {
    /// A fresh allocation always has a word before it.
    Unguarded,
    /// An arbitrary object takes the copying encoder when its pointer is below one word.
    Guarded,
}

/// Selects zero-copy terminal encodings before module-wide helper sharing is
/// costed. Unoptimized lowering retains the direct semantic expansion.
fn plan_zero_copy_terminal_returns(
    target: &Target,
    module: &Module,
) -> IndexVec<FunctionId, FxHashMap<InstId, ZeroCopyReturn>> {
    let mut plan = IndexVec::from_vec(vec![FxHashMap::default(); module.functions.len()]);
    if !(target.optimization().is_gas() || target.optimization().is_size()) {
        return plan;
    }
    let candidates = module.functions.iter().map(terminal_return_encodes).collect::<Vec<_>>();
    if candidates.iter().all(FxHashSet::is_empty) {
        return plan;
    }
    let fresh = fresh_object_returning_functions(module);
    for (func_id, candidates) in IndexVec::<FunctionId, _>::from_vec(candidates).iter_enumerated() {
        let func = &module.functions[func_id];
        let literal_objects = literal_objects_at_encodes(func);
        for &inst in candidates {
            let InstKind::AbiEncode { selector, args, layout, .. } = &func.inst(inst).kind else {
                unreachable!()
            };
            if selector.is_some() || args.iter().any(|arg| literal_objects.contains(arg)) {
                continue;
            }
            let Some(object) = zero_copy_return_object(func, layout, args) else { continue };
            if fresh_memory_object(func, object, &fresh) {
                plan[func_id].insert(inst, ZeroCopyReturn::Unguarded);
            } else if target.guards_zero_copy_return() {
                plan[func_id].insert(inst, ZeroCopyReturn::Guarded);
            }
        }
    }
    plan
}

/// Finds encodes used only by the two projections of an immediate
/// `returndata`. Writing before the object is safe only in this terminal shape.
fn terminal_return_encodes(func: &Function) -> FxHashSet<InstId> {
    if !has_abi_encode(func) {
        return FxHashSet::default();
    }
    let uses = use_counts(func);
    let mut terminal = FxHashSet::default();
    for block in &func.blocks {
        let Some(Terminator::ReturnData { offset, size }) = &block.terminator else { continue };
        for (position, &encode) in block.instructions.iter().enumerate() {
            if !matches!(func.inst(encode).kind, InstKind::AbiEncode { .. }) {
                continue;
            }
            let Some(encoded) = func.inst_result_value(encode) else { continue };
            let suffix = &block.instructions[position + 1..];
            if suffix.len() != 2 || uses[encoded] != 2 {
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
                && uses[*offset] == 1
                && uses[*size] == 1
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
    fresh_memory_object(func, *value, fresh)
}

/// The memory object of a one-element dynamic tuple of a full-word array or bytes.
fn zero_copy_return_object(
    func: &Function,
    layout: &AbiLayout,
    args: &[ValueId],
) -> Option<ValueId> {
    let [object] = args else { return None };
    let word_array_or_bytes = match &*layout.types {
        [AbiType::DynamicArray { element, location: PtrLocation::Memory }] => {
            matches!(element.as_ref(), AbiType::Word(None))
        }
        [AbiType::Bytes(PtrLocation::Memory)] => true,
        _ => false,
    };
    (word_array_or_bytes && func.value_ty(*object) == Some(MirType::MEMPTR)).then_some(*object)
}

/// Whether an accepted zero-copy layout is an array rather than a byte string.
fn is_word_array(layout: &AbiLayout) -> bool {
    matches!(&*layout.types, [AbiType::DynamicArray { .. }])
}

/// Encodes a one-element dynamic tuple over the returned object, which
/// [`plan_zero_copy_terminal_returns`] proved eligible.
fn encode_zero_copy_return(
    builder: &mut FunctionBuilder<'_>,
    layout: &AbiLayout,
    args: &[ValueId],
    mode: AbiEncodeMode,
    form: ZeroCopyReturn,
    helpers: &EncodeHelpers,
) -> ValueId {
    let [object] = args else { unreachable!() };
    if matches!(form, ZeroCopyReturn::Unguarded) {
        let (start, total) = write_head_before_object(builder, layout, *object);
        return builder.make_slice(start, total, PtrLocation::Memory);
    }

    // small = lt object, 32
    // branch small, copy, zero_copy
    let address = builder.cast_word(*object);
    let word = builder.imm(32);
    let small = builder.lt(address, word);
    let copy = builder.create_block();
    let zero_copy = builder.create_block();
    let done = builder.create_block();
    builder.branch(small, copy, zero_copy);

    builder.switch_to_block(copy);
    let copied = lower_encode(builder, layout, None, args, mode, helpers);
    let copied_start = builder.slice_ptr(copied);
    let copied_total = builder.slice_len(copied);
    let copy_end = builder.current_block();
    builder.jump(done);

    builder.switch_to_block(zero_copy);
    let (start, total) = write_head_before_object(builder, layout, *object);
    let zero_copy_end = builder.current_block();
    builder.jump(done);

    // start = phi [copy: copied_start, zero_copy: start]
    // total = phi [copy: copied_total, zero_copy: total]
    builder.switch_to_block(done);
    let start = builder.phi(vec![(copy_end, copied_start), (zero_copy_end, start)]);
    let total = builder.phi(vec![(copy_end, copied_total), (zero_copy_end, total)]);
    builder.make_slice(start, total, PtrLocation::Memory)
}

/// Writes the tuple offset into the word before `object` and zeroes the padding after a byte
/// string, returning the start and size of the encoding that now lies in place.
fn write_head_before_object(
    builder: &mut FunctionBuilder<'_>,
    layout: &AbiLayout,
    object: ValueId,
) -> (ValueId, ValueId) {
    let length = memory_object_len(builder, object);
    let extent = if is_word_array(layout) {
        let five = builder.imm(5);
        builder.shl(length, five)
    } else {
        let thirty_one = builder.imm(31);
        let rounded = builder.add(length, thirty_one);
        let mask = builder.not(thirty_one);
        let padded = builder.and(rounded, mask);
        // object: [length, bytes..., stale padding]
        // => [length, bytes..., zero padding]
        let source = builder.memory_data(object);
        let end = builder.add(source, length);
        let zero = builder.imm(0);
        builder.store(end, zero);
        padded
    };

    // before: [word, length, payload...]
    // => [32, length, payload...]
    let back = builder.imm(-32);
    let start = builder.ptradd(object, back);
    let offset = builder.imm(32);
    builder.store(start, offset);
    let total = builder.add_u64_offset(extent, 64);
    (start, total)
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
            encode_tuple(builder, args, &layout.types, dest, AbiValueSource::Scalar, helpers);
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
            encode_tuple(builder, args, &layout.types, data, AbiValueSource::Scalar, helpers);
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
        encode_tuple(builder, args, &layout.types, dest, AbiValueSource::Scalar, helpers);
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
        encode_tuple(builder, args, &layout.types, dest, AbiValueSource::Scalar, helpers);
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
    let encoded_size =
        encode_tuple(builder, args, &layout.types, dest, AbiValueSource::Scalar, helpers);
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
        encode_static_slice(builder, ty, source, head_addr, location, false);
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
        AbiType::FixedArray { element, len, .. } => {
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

/// Encodes the static value at `source` into `head_addr`. A `validate`
/// reverts on a calldata word that is not canonical for its type, as ABI decoding would have.
fn encode_static_slice(
    builder: &mut FunctionBuilder<'_>,
    ty: &AbiType,
    source: ValueId,
    head_addr: ValueId,
    location: PtrLocation,
    validate: bool,
) {
    match ty {
        AbiType::Tuple(fields) => {
            let mut source_offset = 0;
            let mut head = head_addr;
            for field in fields {
                let source_word = offset_ptr(builder, source, source_offset);
                encode_static_slice(builder, field, source_word, head, location, validate);
                source_offset += field.head_size();
                head = offset_ptr(builder, head, field.head_size());
            }
        }
        AbiType::FixedArray { block_copy: true, .. } if location == PtrLocation::Calldata => {
            // Like solc, copy the words as one block, which reads zeros from an address that
            // wraps past the end of the address space where per-word loads would wrap around.
            // calldatacopy head_addr, source, head_size
            let size = builder.imm(ty.head_size());
            copy_source_data(builder, location, head_addr, source, size);
        }
        AbiType::FixedArray { element, len, .. } => {
            let length = builder.imm(*len);
            let stride = builder.imm(element.head_size());
            builder.counted_loop(length, |builder, index| {
                let offset = builder.mul(index, stride);
                let source_word = builder.add(source, offset);
                let head = builder.add(head_addr, offset);
                encode_static_slice(builder, element, source_word, head, location, validate);
            });
        }
        AbiType::Function => {
            let value = builder.load(source, location);
            let function = AbiWordValidator::from_layout(crate::mir::ValueLayout::Function)
                .expect("function words always require cleanup");
            let value = if location == PtrLocation::Memory {
                function.cleanup(builder, value)
            } else {
                if validate {
                    validate_calldata_word(builder, function, value);
                }
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
                Some(cleanup) => {
                    if validate {
                        validate_calldata_word(builder, *cleanup, value);
                    }
                    value
                }
                None => value,
            };
            builder.store(head_addr, value);
        }
        AbiType::Bytes(_) | AbiType::DynamicArray { .. } => {
            unreachable!("dynamic ABI values are not static")
        }
    }
}

/// Reverts unless the calldata `word` is canonical for `validator`.
fn validate_calldata_word(
    builder: &mut FunctionBuilder<'_>,
    validator: AbiWordValidator,
    word: ValueId,
) {
    // if !valid(word) { revert(0, 0) }
    let valid = validator.condition(builder, word);
    builder.revert_if_zero(valid, RevertReason::Empty);
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
            encode_bytes(builder, value, dest, location, helpers.literal_objects.contains(&value))
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
            // Calldata function pointers are validated one at a time, like members of a struct.
            let word_cleanup = match element.as_ref() {
                AbiType::Word(cleanup) => Some(*cleanup),
                AbiType::Function if location != PtrLocation::Calldata => {
                    Some(AbiWordValidator::from_layout(crate::mir::ValueLayout::Function))
                }
                _ => None,
            };
            if let Some(cleanup) = word_cleanup {
                return encode_word_array(builder, value, dest, location, cleanup);
            }
            debug_assert_eq!(location, PtrLocation::Calldata, "only calldata arrays are slices");
            encode_calldata_body(builder, ty, value, dest, helpers)
        }
        AbiType::FixedArray { .. } | AbiType::Tuple(_)
            if is_calldata_slice(builder.func(), value) =>
        {
            encode_calldata_body(builder, ty, value, dest, helpers)
        }
        AbiType::FixedArray { element, len, .. } => encode_memory_array_elements(
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
                encode_tuple(builder, &values, fields, dest, AbiValueSource::Memory, helpers);
            builder.add(dest, size)
        }
        AbiType::Word(_) | AbiType::Function => unreachable!("word ABI values are static"),
    }
}

/// Encodes the calldata aggregate `value` of layout `ty` at `dest`, through the shared helper
/// for its layout if there is one.
fn encode_calldata_body(
    builder: &mut FunctionBuilder<'_>,
    ty: &AbiType,
    value: ValueId,
    dest: ValueId,
    helpers: &EncodeHelpers,
) -> ValueId {
    if !helpers.calldata.is_empty()
        && let Some(&helper) = helpers.calldata.get(&ty.clone().at_location(PtrLocation::Calldata))
    {
        // length = icall @encode_abi_calldata, value, dest
        let pointer = builder.cast(dest, MirType::MEMPTR);
        let length = builder.icall(helper, vec![value, pointer], MirType::I256);
        return builder.add(dest, length);
    }
    match ty {
        AbiType::DynamicArray { element, .. } => {
            // length = slice_len(value)
            // mstore dest, length
            // encode_calldata_elements(element, slice_ptr(value), length, dest + 32)
            let length = builder.slice_len(value);
            builder.store(dest, length);
            let area = builder.add_u64_offset(dest, 32);
            let base = builder.slice_ptr(value);
            encode_calldata_elements(builder, element, base, length, area, helpers)
        }
        AbiType::FixedArray { element, len, .. } => {
            // encode_calldata_elements(element, slice_ptr(value), len, dest)
            let length = builder.imm(*len);
            let base = builder.slice_ptr(value);
            encode_calldata_elements(builder, element, base, length, dest, helpers)
        }
        AbiType::Tuple(fields) => {
            let base = builder.slice_ptr(value);
            encode_calldata_tuple(builder, fields, base, dest, helpers)
        }
        AbiType::Word(_) | AbiType::Function | AbiType::Bytes(_) => {
            unreachable!("calldata aggregates are arrays and tuples")
        }
    }
}

fn is_calldata_slice(func: &Function, value: ValueId) -> bool {
    func.value_slice_location(value) == Some(PtrLocation::Calldata)
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

/// Encodes the members of a calldata struct at `base` into `dest`, re-encoding each dynamic
/// member's offset and validating what solc's ABI encoder validates: the tail offsets, lengths,
/// and ranges of dynamic members, and every narrow word.
fn encode_calldata_tuple(
    builder: &mut FunctionBuilder<'_>,
    fields: &[AbiType],
    base: ValueId,
    dest: ValueId,
    helpers: &EncodeHelpers,
) -> ValueId {
    let head_size = fields.iter().map(AbiType::head_size).sum();
    let mut tail = offset_ptr(builder, dest, head_size);
    let mut offset = 0;
    for field in fields {
        // tail = encode field at dest + offset from base + offset
        let source_head = offset_ptr(builder, base, offset);
        let head_addr = offset_ptr(builder, dest, offset);
        let field_dest = AbiValueDest { head_addr, tuple_base: dest, tail };
        tail = encode_calldata_element(builder, field, source_head, base, field_dest, helpers);
        offset += field.head_size();
    }
    tail
}

/// Encodes the element of `ty` whose head is at `source_head` in the calldata aggregate at
/// `base`, returning the end of the encoded tail.
fn encode_calldata_element(
    builder: &mut FunctionBuilder<'_>,
    ty: &AbiType,
    source_head: ValueId,
    base: ValueId,
    dest: AbiValueDest,
    helpers: &EncodeHelpers,
) -> ValueId {
    if !ty.is_dynamic() {
        encode_static_slice(builder, ty, source_head, dest.head_addr, PtrLocation::Calldata, true);
        return dest.tail;
    }
    let value = calldata_dynamic_value(builder, ty, source_head, base);
    if let AbiType::DynamicArray { element, .. } = ty
        && let AbiType::Word(Some(validator)) = element.as_ref()
    {
        // Neither lowering nor the dispatcher validates the words of a nested array, which
        // then copy as one block.
        // for index in 0..slice_len(value): validate(calldataload(slice_ptr(value) + index * 32))
        let length = builder.slice_len(value);
        let words = builder.slice_ptr(value);
        let word = builder.imm(32);
        builder.counted_loop(length, |builder, index| {
            let offset = builder.mul(index, word);
            let position = builder.add(words, offset);
            let value = builder.load(position, PtrLocation::Calldata);
            validate_calldata_word(builder, *validator, value);
        });
    }
    encode_value(builder, ty, value, dest, AbiValueSource::Scalar, helpers)
}

/// Follows the tail offset at `head` to the dynamic value of `ty` in the calldata aggregate at
/// `base`, with solc's `calldata_access` checks, and returns it as a calldata slice: the data
/// and length of a byte string or array, the head and member count of a fixed array, and the
/// head of a struct.
fn calldata_dynamic_value(
    builder: &mut FunctionBuilder<'_>,
    ty: &AbiType,
    head: ValueId,
    base: ValueId,
) -> ValueId {
    let tail = CalldataTail::Encode;
    // offset = calldataload(head)
    // check_calldata_tail_offset(base, offset, tail_size)
    // position = base + offset
    let offset = builder.load(head, PtrLocation::Calldata);
    let calldata_end = builder.calldatasize();
    builder.check_calldata_tail_offset(tail, base, offset, calldata_end, ty.tail_size());
    let position = builder.add(base, offset);
    let stride = match ty {
        AbiType::Bytes(_) => 1,
        AbiType::DynamicArray { element, .. } => element.head_size(),
        AbiType::FixedArray { len, .. } => {
            let length = builder.imm(*len);
            return builder.make_slice(position, length, PtrLocation::Calldata);
        }
        AbiType::Tuple(_) => {
            let size = builder.imm(ty.head_size());
            return builder.make_slice(position, size, PtrLocation::Calldata);
        }
        AbiType::Word(_) | AbiType::Function => unreachable!("word ABI values are static"),
    };
    // length = calldataload(position)
    // data = calldata_tail_data(position + 32, length, stride)
    let length = builder.load(position, PtrLocation::Calldata);
    let data = builder.calldata_tail_data(tail, position, 32, length, calldata_end, stride);
    builder.make_slice(data, length, PtrLocation::Calldata)
}

/// Encodes `length` calldata elements of one layout, whose heads start at `base`, into `area`.
/// Dynamic elements re-encode their offsets. Static elements that need no validation copy as
/// one block.
fn encode_calldata_elements(
    builder: &mut FunctionBuilder<'_>,
    element: &AbiType,
    base: ValueId,
    length: ValueId,
    area: ValueId,
    helpers: &EncodeHelpers,
) -> ValueId {
    let stride = builder.imm(element.head_size());
    let head_bytes = builder.mul(length, stride);
    let initial_tail = builder.add(area, head_bytes);
    if !element.is_dynamic() {
        // for index in 0..length: encode_static(element, base + index * stride)
        let encode_each = |builder: &mut FunctionBuilder<'_>| {
            let validate = needs_calldata_validation(element);
            builder.counted_loop(length, |builder, index| {
                let offset = builder.mul(index, stride);
                let source = builder.add(base, offset);
                let head = builder.add(area, offset);
                encode_static_slice(
                    builder,
                    element,
                    source,
                    head,
                    PtrLocation::Calldata,
                    validate,
                );
            });
        };
        if needs_calldata_validation(element)
            || !helpers.target.guards_block_copy(element.head_size() / 32)
        {
            encode_each(builder);
            return initial_tail;
        }
        // if base + head_bytes < base { encode_each } else { calldatacopy area, base, head_bytes }
        let end = builder.add(base, head_bytes);
        let wraps = builder.lt(end, base);
        let each = builder.create_block();
        let copy = builder.create_block();
        let done = builder.create_block();
        builder.branch(wraps, each, copy);
        builder.switch_to_block(each);
        encode_each(builder);
        builder.jump(done);
        builder.switch_to_block(copy);
        copy_source_data(builder, PtrLocation::Calldata, area, base, head_bytes);
        builder.jump(done);
        builder.switch_to_block(done);
        return initial_tail;
    }

    let preheader = builder.current_block();
    let cond = builder.create_block();
    let body = builder.create_block();
    let done = builder.create_block();
    builder.jump(cond);

    builder.switch_to_block(cond);
    let remaining = builder.phi(vec![(preheader, length)]);
    let current_tail = builder.phi(vec![(preheader, initial_tail)]);
    let element_head = builder.phi(vec![(preheader, area)]);
    let source_head = builder.phi(vec![(preheader, base)]);
    let zero = builder.imm(0);
    let has_next = builder.gt(remaining, zero);
    builder.branch(has_next, body, done);

    builder.switch_to_block(body);
    let new_tail = encode_calldata_element(
        builder,
        element,
        source_head,
        base,
        AbiValueDest { head_addr: element_head, tuple_base: area, tail: current_tail },
        helpers,
    );

    let one = builder.imm(1);
    let next_remaining = builder.sub(remaining, one);
    let next_source = builder.add(source_head, stride);
    let next_head = builder.add(element_head, stride);
    let backedge = builder.current_block();
    builder.jump(cond);
    builder.add_phi_incoming(remaining, backedge, next_remaining);
    builder.add_phi_incoming(current_tail, backedge, new_tail);
    builder.add_phi_incoming(element_head, backedge, next_head);
    builder.add_phi_incoming(source_head, backedge, next_source);

    builder.switch_to_block(done);
    current_tail
}

/// Whether a static calldata value holds a word that ABI decoding would validate.
fn needs_calldata_validation(ty: &AbiType) -> bool {
    match ty {
        AbiType::Word(cleanup) => cleanup.is_some(),
        AbiType::Function => true,
        AbiType::FixedArray { element, .. } => needs_calldata_validation(element),
        AbiType::Tuple(fields) => fields.iter().any(needs_calldata_validation),
        AbiType::Bytes(_) | AbiType::DynamicArray { .. } => false,
    }
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
    let word = builder.imm(32);
    let thirty_one = builder.imm(31);
    let mask = builder.not(thirty_one);
    let rounded = builder.add(len, thirty_one);
    let padded = builder.and(rounded, mask);
    let data_dest = builder.add(dest, word);

    // last = dest + padded
    // store last, 0
    // mstore dest, len
    // For an empty tail, last == dest; write the header after zeroing it.
    let last = builder.add(dest, padded);
    let zero = builder.imm(0);
    builder.store(last, zero);
    builder.store(dest, len);
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

/// Literal objects whose allocation nothing can observe once their scratch encode has run: the
/// block returns or reverts with exactly the encoded bytes after only pure instructions, so no
/// later access reads the allocation or the free-memory pointer it moved. The literal's allocation
/// is the last one before the encode and no other argument reads memory, so no address shifts and
/// no argument reaches into the literal. `literal_objects_at_encodes` already proved that no access
/// sits between the allocation and the encode.
fn terminal_scratch_literals(
    func: &Function,
    literal_objects: &FxHashSet<ValueId>,
) -> FxHashSet<ValueId> {
    let mut objects = FxHashSet::default();
    for block in &func.blocks {
        let Some(Terminator::Revert { offset, size } | Terminator::ReturnData { offset, size }) =
            &block.terminator
        else {
            continue;
        };
        // The last two allocations or encodes of the block, latest first.
        let mut reservers = block.instructions.iter().enumerate().rev().filter(|&(_, &inst)| {
            matches!(func.inst(inst).kind, InstKind::Alloc { .. } | InstKind::AbiEncode { .. })
        });
        if let Some((index, &encode)) = reservers.next()
            && let Some((_, &allocation)) = reservers.next()
            && let InstKind::AbiEncode { mode: AbiEncodeMode::Scratch, args, layout, .. } =
                &func.inst(encode).kind
            && matches!(func.inst(allocation).kind, InstKind::Alloc { .. })
            && let Some(object) = func.inst_result_value(allocation)
            && let Some(encoded) = func.inst_result_value(encode)
            && literal_objects.contains(&object)
            && args.contains(&object)
            && layout.types.iter().zip(args).all(|(ty, &arg)| {
                arg == object || matches!(ty, AbiType::Word(_) | AbiType::Function)
            })
            && [(*offset, MirType::SLICE_PTR_FIELD), (*size, MirType::SLICE_LEN_FIELD)].iter().all(
                |&(operand, field)| {
                    matches!(func.value(operand), Value::Inst(projection)
                        if func.inst(*projection).kind.slice_projection() == Some((encoded, field)))
                },
            )
            && block.instructions[index + 1..]
                .iter()
                .all(|&later| func.inst(later).kind.effect_kind() == EffectKind::Pure)
        {
            objects.insert(object);
        }
    }
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
/// uses, apart from encodings, are initialization operations preceding this encoding in the
/// same block. The caller must also exclude opaque observers using the original IR.
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
        if func.is_object_derivation(inst)
            || matches!(func.inst(inst).kind, InstKind::AbiEncode { .. })
        {
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

/// The block that holds `inst`.
fn defining_block(func: &Function, inst: InstId) -> Option<BlockId> {
    func.blocks
        .iter_enumerated()
        .find_map(|(id, block)| block.instructions.contains(&inst).then_some(id))
}

/// Removes a literal object's initialization after its encoding has embedded
/// the bytes directly in the output. The object must have no active use other
/// than its allocation and literal stores; otherwise the object remains live. An allocation
/// that preserves the free-memory pointer goes only if it is in `unobserved`.
fn remove_literal_objects(
    func: &mut Function,
    values: &[ValueId],
    unobserved: &FxHashSet<ValueId>,
) {
    let mut removed = FxHashSet::default();
    for &object in values {
        let Value::Inst(defining_inst) = func.value(object) else { continue };
        if func.inst(*defining_inst).metadata.preserves_fmp() && !unobserved.contains(&object) {
            continue;
        }
        if defining_block(func, *defining_inst)
            .and_then(|block| literal_bytes(func, object, block))
            .is_none()
        {
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
