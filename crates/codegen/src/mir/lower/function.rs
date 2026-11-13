//! Function-level HIR to MIR lowering.

use super::{
    contract,
    storage::{StorageEncoding, StorageLayout, StorageLocation},
    types,
};
use crate::mir::{
    AbiLayout, AbiParamLayout, AbiParamLocation, AbiParamType, AbiType, AbiWordValidator,
    AllocationSemantics, ArithmeticKind, BlockId, CallOutput, CallPolicy, CalldataTail, CheckedOp,
    DataId, DataRef, ExtCallKind, Function, FunctionBuilder, FunctionId, ImmutableId, InstKind,
    LogTopics, MemoryObjectKind, MemoryObjectLayout, MirModuleTarget, MirType, Module,
    PackedArraySource, PackedPart, PanicCode, PtrLocation, RevertPayload, RevertReason, Value,
    ValueId, memory::EvmMemoryLayout,
};
use alloy_primitives::{U256, keccak256};
use solar_ast::{BinOpKind, DataLocation, LitKind, StateMutability, TypeSize, UnOpKind};
use solar_data_structures::{
    bit_set::DenseBitSet,
    map::{FxHashMap, FxHashSet, FxIndexSet, StdEntry},
};
use solar_interface::{ByteSymbol, Ident, Span, Symbol, kw, sym};
use solar_sema::{
    Gcx,
    builtins::Builtin,
    eval::ConstValue,
    hir::{self, ElementaryType, ExprKind, LoopSource, StmtKind, VariableId},
    ty::{CallableParamNames, CallableParamSource, Ty, TyFn, TyKind},
};
use std::{convert::Infallible, fmt::Display, sync::Arc};

mod abi_calls;
mod abi_values;
mod builtins;
mod calls;
mod control_flow;
mod entry;
mod expressions;
mod indexing;
mod lvalues;
mod memory_values;
mod modifiers;
mod operators;
mod raw_scalars;
mod spec;
mod statements;
mod storage_values;
mod values;
mod word;

use word::Word;

/// Shared inputs for one contract's function lowering.
pub(super) struct LoweringContext<'gcx, 'ctx> {
    pub(super) gcx: Gcx<'gcx>,
    pub(super) module: &'ctx mut Module,
    pub(super) storage: &'ctx StorageLayout<'gcx>,
    pub(super) contract_id: hir::ContractId,
    /// Contracts whose bytecode this contract embeds.
    pub(super) bytecode_dependencies: &'ctx DenseBitSet<hir::ContractId>,
    pub(super) function_ids: &'ctx FxHashMap<hir::FunctionId, FunctionId>,
    pub(super) immutable_ids: &'ctx FxHashMap<VariableId, ImmutableId>,
    pub(super) state: &'ctx mut LoweringState,
    pub(super) shared_literals: &'ctx FxIndexSet<ByteSymbol>,
    pub(super) shared_word_literals: &'ctx FxHashSet<ByteSymbol>,
    pub(super) share_storage_bytes: bool,
}

impl<'gcx, 'ctx> LoweringContext<'gcx, 'ctx> {
    pub(super) fn reborrow<'a>(&'a mut self) -> LoweringContext<'gcx, 'a> {
        LoweringContext {
            gcx: self.gcx,
            module: &mut *self.module,
            storage: self.storage,
            contract_id: self.contract_id,
            bytecode_dependencies: self.bytecode_dependencies,
            function_ids: self.function_ids,
            immutable_ids: self.immutable_ids,
            state: &mut *self.state,
            shared_literals: self.shared_literals,
            shared_word_literals: self.shared_word_literals,
            share_storage_bytes: self.share_storage_bytes,
        }
    }

    /// Reports a lowering bail-out and returns `None`.
    pub(super) fn report_unsupported<T>(&self, span: Span, what: &str) -> Option<T> {
        self.gcx.dcx().emit_err(span, format!("codegen rewrite does not support this {what} yet"));
        None
    }
}

/// Mutable registries shared by all functions lowered for one contract.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum RecursiveStorageHelper {
    Store { target: hir::StructId, source: hir::StructId },
    Clear { target: hir::StructId },
}

#[derive(Default)]
pub(super) struct LoweringState {
    pub(super) pointer_registry: InternalFunctionPointerRegistry,
    /// DO NOT ADD OTHER HELPER MAPS. USE THIS ONE ONLY.
    pub(super) helpers: FxHashMap<Symbol, FunctionId>,
    raw_scalars: FxHashSet<VariableId>,
    raw_pointer_types: FxHashSet<MirType>,
}

/// Lowers one HIR function into a typed MIR function.
pub(super) fn lower(
    mut context: LoweringContext<'_, '_>,
    id: hir::FunctionId,
    expose_selector: bool,
) -> Option<Function> {
    let gcx = context.gcx;
    let hir_function = gcx.hir.function(id);
    let mut mir = contract::declaration(gcx, id, hir_function, context.module.optimization);
    let declaration_span = mir.declaration_span;
    if !expose_selector {
        mir.selector = None;
    }
    let has_constructor_params =
        hir_function.kind == hir::FunctionKind::Constructor && !hir_function.parameters.is_empty();
    if mir.selector.is_some() || has_constructor_params {
        let mut type_lowerer = types::TypeLowerer::new(gcx);
        let input_shapes = hir_function
            .parameters
            .iter()
            .map(|&param| type_lowerer.abi_param_type(gcx.type_of_item(param.into())))
            .collect::<Option<Vec<_>>>();
        let Some(input_shapes) = input_shapes else {
            return context.report_unsupported(hir_function.span, "function parameter shape");
        };
        mir.abi_params = Some(AbiParamLayout::new(input_shapes.into_boxed_slice()));
        mir.abi_param_locations = Some(
            hir_function
                .parameters
                .iter()
                .map(|&param| {
                    if gcx.type_of_item(param.into()).is_ref_at(DataLocation::Calldata) {
                        AbiParamLocation::Calldata
                    } else {
                        AbiParamLocation::Memory
                    }
                })
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        );
        if mir.selector.is_some() {
            let mut output_shapes = Vec::with_capacity(hir_function.returns.len());
            let mut output_param_shapes = Vec::with_capacity(hir_function.returns.len());
            for &ret in hir_function.returns {
                let Some((output_shape, output_param_shape)) =
                    type_lowerer.abi_return_shapes(gcx.type_of_item(ret.into()))
                else {
                    return context.report_unsupported(hir_function.span, "function return shape");
                };
                output_shapes.push(output_shape);
                output_param_shapes.push(output_param_shape);
            }
            mir.abi_returns = Some(
                context.module.intern_abi_layout(AbiLayout::new(output_shapes.into_boxed_slice())),
            );
            mir.abi_return_params =
                Some(AbiParamLayout::new(output_param_shapes.into_boxed_slice()));
        }
    }

    let mut lowerer = FunctionLowerer::new(context.reborrow(), &mut mir);
    // solc tags a function's entry under the function definition.
    lowerer.builder.set_entry_label_span(declaration_span);
    lowerer.is_getter = hir_function.is_getter();
    lowerer.bind_signature(hir_function);
    if hir_function.kind == hir::FunctionKind::Function {
        lowerer.lower_spec_requires(hir_function);
    }
    if hir_function.kind == hir::FunctionKind::Constructor {
        let Some(contract_id) = hir_function.contract else {
            return context.report_unsupported(hir_function.span, "free constructor");
        };
        lowerer.lower_implicit_base_constructors(contract_id)?;
        lowerer.lower_state_initializers(contract_id)?;
    }
    if let Some(body) = hir_function.body {
        lowerer.lower_function_body(hir_function.modifiers, body, hir_function.span)?;
    }
    if !lowerer.is_terminated() {
        // ret named_returns !metadata(function definition)
        lowerer.builder.replace_source_span(declaration_span);
        lowerer.finish(hir_function.returns)?;
    }
    Some(mir)
}

/// Lowers the synthetic constructor used when state initializers exist without
/// an explicit constructor body.
pub(super) fn lower_synthetic_constructor(
    mut context: LoweringContext<'_, '_>,
    contract_id: hir::ContractId,
) -> Option<Function> {
    let span = context.gcx.hir.contract(contract_id).span;
    let mut mir = Function::new(Ident::with_dummy_span(kw::Constructor));
    mir.declaration_span = span;
    let mut lowerer = FunctionLowerer::new(context.reborrow(), &mut mir);
    lowerer.builder.set_entry_label_span(span);
    lowerer.lower_implicit_base_constructors(contract_id)?;
    lowerer.lower_state_initializers(contract_id)?;
    if !lowerer.is_terminated() {
        // ret !metadata(contract definition)
        lowerer.builder.replace_source_span(span);
        lowerer.finish(&[])?;
    }
    Some(mir)
}

/// The mutable state for one function lowering.
///
/// Keeping the HIR context, variable environment, loop targets, and builder in
/// one object makes scope changes explicit. Child lowering methods do not need
/// to pass a growing collection of loosely related maps and flags.
struct FunctionLowerer<'gcx, 'ctx> {
    cx: LoweringContext<'gcx, 'ctx>,
    types: types::TypeLowerer<'gcx>,
    builder: FunctionBuilder<'ctx>,
    values: FxHashMap<VariableId, ValueId>,
    /// Calldata slices that inline assembly may have built or changed, which ABI decoding did not
    /// range check.
    unchecked_calldata_slices: FxHashSet<ValueId>,
    default_bindings: FxHashSet<VariableId>,
    deferred_bindings: FxHashSet<VariableId>,
    storage_refs: FxHashMap<VariableId, StorageAccess>,
    returns: Vec<VariableId>,
    prepared_constructors: FxHashSet<hir::FunctionId>,
    /// The pure struct constructors being inlined.
    inlined_struct_constructors: FxHashSet<hir::FunctionId>,
    /// The base constructor argument list chosen for each base of the contract
    /// being constructed, in evaluation order.
    base_args: Vec<(hir::ContractId, hir::CallArgs<'gcx>)>,
    loops: Vec<LoopTargets>,
    modifiers: Vec<ModifierContext<'gcx>>,
    modifier_depth: u32,
    return_targets: Vec<ReturnTarget>,
    /// The source specification, when recording lowers one.
    spec: Option<spec::FunctionSpec>,
    is_getter: bool,
    unchecked: bool,
    in_inline_assembly: bool,
    /// Whether the inline assembly lowered so far may write memory: a store, a copy, a call with
    /// a return buffer, or an assignment to a memory reference.
    assembly_writes_memory: bool,
}

/// The lowered `{gas: ..., value: ...}` options of an external call.
#[derive(Clone, Copy)]
struct LoweredCallOptions {
    /// The `gas` operand, or `None` when the call forwards the gas left on a target that predates
    /// EIP-150.
    ///
    /// Such a target aborts a call whose gas argument exceeds the gas left, so the operand has to
    /// withhold the call's own costs and only [`FunctionLowerer::call_gas`] materializes it,
    /// immediately before the call. Every other case is materialized here: an explicit
    /// `{gas: ...}` because its expression must run in source order, and `gas()` because
    /// EIP-150 caps the forwarded gas anyway.
    gas: Option<ValueId>,
    /// The `value` operand, zero when the call sends no value.
    value: ValueId,
    /// Whether the call has an explicit `{value: ...}` option.
    ///
    /// A pre-EIP-150 call reserves the value-transfer cost whenever the option is there, because
    /// the amount is not known to be zero.
    ///
    /// Reference: <https://github.com/argotorg/solidity/blob/f401782df49be312ea4ef52a2d467cf5183b5906/libsolidity/codegen/ExpressionCompiler.cpp#L2923-L2924>
    value_set: bool,
    /// A zero immediate, reusable by the caller.
    zero: ValueId,
}

#[derive(Clone, Copy)]
struct CallArgumentParams<'a> {
    count: usize,
    names: Option<&'a [Option<Symbol>]>,
    reverse: bool,
}

/// An operand lowered at the type of its own expression, before its consumer converts it.
///
/// A storage aggregate is its slot, so a consumer that converts after its later operands ran, as
/// via-IR does, copies it then.
struct Lowered<'gcx> {
    value: LoweredValue,
    /// The type of the expression, or `None` for a literal that was built at its target type.
    source: Option<Ty<'gcx>>,
    /// The type the consumer converts it to.
    target: Ty<'gcx>,
    span: Span,
}

enum LoweredValue {
    Word(Word),
    /// A `bytes` or `string` literal bound to a memory target, which the consumer allocates when
    /// it converts the operand, as via-IR's `convert` does.
    BytesLiteral(ByteSymbol),
}

impl Lowered<'_> {
    /// Whether the operand is a storage reference, which a consumer that converts late copies
    /// only after its later operands ran.
    fn waits(&self) -> bool {
        self.source.is_some_and(|ty| ty.is_ref_at(DataLocation::Storage))
    }

    /// Whether converting the operand allocates a memory copy of it.
    fn copies(&self) -> bool {
        self.waits()
            || matches!(self.value, LoweredValue::BytesLiteral(_))
            || (self.source.is_some_and(|ty| ty.is_ref_at(DataLocation::Calldata))
                && self.target.is_ref_at(DataLocation::Memory))
    }
}

/// An operand that was converted, or that waits for its later operands. `L` is a third kind of
/// operand that only some consumers produce.
enum Step<'gcx, T, L = Infallible> {
    Done(T),
    Wait(Lowered<'gcx>),
    Literal(L),
}

/// Constant call input, which needs no source-level memory object: the encoder materializes it.
struct CallInputLiteral {
    bytes: ByteSymbol,
    span: Span,
}

/// An operand of an ABI encoding after lowering: the value to encode with its ABI type, a storage
/// reference that the encoder reads after every operand ran, or constant call input.
type AbiOperand<'gcx> = Step<'gcx, (ValueId, AbiType), CallInputLiteral>;

/// The signature of `abi.encodeWithSignature` after lowering.
enum SignatureOperand {
    /// A selector known at once.
    Selector(ValueId),
    /// A `bytes` or `string` value, hashed after the other arguments ran.
    Bytes(ValueId, PtrLocation),
}

struct LoopTargets {
    break_block: BlockId,
    continue_block: BlockId,
    break_states: Vec<LoopState>,
    continue_states: Vec<LoopState>,
}

#[derive(Clone)]
struct LoopState {
    block: BlockId,
    values: FxHashMap<VariableId, ValueId>,
    storage_refs: FxHashMap<VariableId, StorageAccess>,
}

struct MergeBranch<T> {
    block: BlockId,
    values: FxHashMap<VariableId, T>,
    terminated: bool,
}

struct TernaryBranch<T> {
    block: BlockId,
    value: T,
    terminated: bool,
}

type BindingSnapshot = Vec<(VariableId, Option<ValueId>, Option<StorageAccess>)>;

struct ModifierContext<'gcx> {
    modifiers: &'gcx [hir::Modifier<'gcx>],
    body: hir::Block<'gcx>,
    next: usize,
    returns: BindingSnapshot,
    incoming_returns: BindingSnapshot,
}

struct ReturnTarget {
    block: BlockId,
    states: Vec<LoopState>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct StorageAccess {
    slot: ValueId,
    location: StorageLocation,
    offset: Option<ValueId>,
}

enum LValuePlace<'gcx> {
    Variable { id: VariableId, span: Span },
    Storage { ty: Ty<'gcx>, access: StorageAccess, span: Span },
    MemoryField { object: ValueId, field: u64, ty: Ty<'gcx> },
    MemoryElement { object: ValueId, layout: MemoryObjectLayout, index: ValueId, ty: Ty<'gcx> },
    MemoryByte { object: ValueId, index: ValueId, ty: Ty<'gcx> },
    YulSuffix { id: VariableId, suffix: YulSuffix, span: Span },
}

/// A Yul suffix on a variable, such as `x.offset` or `x.slot`.
///
/// Loads and stores read the variable when they run, so later writes see
/// earlier ones in a multi-assignment.
#[derive(Clone, Copy)]
enum YulSuffix {
    CalldataOffset,
    CalldataLength,
    FunctionAddress,
    FunctionSelector,
    StorageSlot,
    StorageOffset,
}

#[derive(Clone, Copy)]
enum BuiltinArgCount {
    Exact(usize),
    AtLeast(usize),
    Between(usize, usize),
}

impl BuiltinArgCount {
    fn description(self) -> String {
        match self {
            Self::Exact(count) => count.to_string(),
            Self::AtLeast(count) => format!("at least {count}"),
            Self::Between(min, max) => format!("{min} to {max}"),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub(super) struct InternalFunctionPointerShape {
    params: Vec<crate::mir::ValueLayout>,
    returns: Vec<crate::mir::ValueLayout>,
}

impl InternalFunctionPointerShape {
    fn from_ty(function: &TyFn<'_>) -> Self {
        Self {
            params: function
                .parameters
                .iter()
                .map(|&ty| types::TypeLowerer::value_layout(ty))
                .collect(),
            returns: function
                .returns
                .iter()
                .map(|&ty| types::TypeLowerer::value_layout(ty))
                .collect(),
        }
    }

    fn is_assembly_cast_compatible_with(&self, target: &Self) -> bool {
        // Assembly casts preserve these full-word argument representations. Keep return shapes
        // exact because internal calls can expose dirty return words.
        let canonicalize = |ty: crate::mir::ValueLayout| {
            if ty == crate::mir::ValueLayout::Address || ty.is_full_abi_word() {
                crate::mir::ValueLayout::uint256()
            } else {
                ty
            }
        };
        self.params.iter().copied().map(canonicalize).eq(target
            .params
            .iter()
            .copied()
            .map(canonicalize))
            && self.returns == target.returns
    }

    /// Returns whether a pointer of this shape can hold a `target` function retyped by inline
    /// assembly: every parameter and return either keeps its layout or stays one word, which
    /// the dispatcher reinterprets.
    ///
    /// NOTE: Calldata references travel as `(offset, length)` slices, so a one-slot calldata
    /// struct retyped to a word, which solc dispatches by stack arity, reaches no target.
    fn is_word_compatible_with(&self, target: &Self) -> bool {
        let compatible = |lhs: &[crate::mir::ValueLayout], rhs: &[crate::mir::ValueLayout]| {
            lhs.len() == rhs.len()
                && lhs
                    .iter()
                    .zip(rhs)
                    .all(|(&lhs, &rhs)| lhs == rhs || (lhs.is_word() && rhs.is_word()))
        };
        compatible(&self.params, &target.params) && compatible(&self.returns, &target.returns)
    }

    fn helper_name(&self, from_assembly: bool) -> Symbol {
        let params = self.params.iter().map(ToString::to_string).collect::<Vec<_>>().join("_");
        let returns = self.returns.iter().map(ToString::to_string).collect::<Vec<_>>().join("_");
        let suffix = format!(
            "{}p_{}_r_{}",
            if from_assembly { "asm_" } else { "" },
            if params.is_empty() { "none" } else { &params },
            if returns.is_empty() { "none" } else { &returns },
        );
        // `slice<calldata>` -> `slice_calldata`, keeping the name an identifier.
        helper_name(sym::internal_dispatcher, suffix.replace('<', "_").replace('>', ""))
    }
}

fn helper_name(prefix: Symbol, suffix: impl Display) -> Symbol {
    Symbol::intern(&format!("{prefix}_{suffix}"))
}

#[derive(Default)]
pub(super) struct InternalFunctionPointerRegistry {
    /// Pointer targets in first-use order; a pointer value is the target's index plus one.
    targets: FxIndexSet<hir::FunctionId>,
    dispatchers: FxHashMap<FunctionId, InternalFunctionPointerDispatch>,
    /// Functions whose pointers may reach inline assembly, which can retype them.
    assembly_exposed: FxHashSet<hir::FunctionId>,
    /// Pointer calls whose callee may hold a pointer retyped by inline assembly.
    assembly_calls: FxHashSet<hir::ExprId>,
}

impl InternalFunctionPointerRegistry {
    /// Returns whether a pointer of this shape that inline assembly retyped can hold an exposed
    /// function that the per-type dispatcher does not select.
    fn assembly_widens(&self, gcx: Gcx<'_>, shape: &InternalFunctionPointerShape) -> bool {
        self.assembly_exposed.iter().any(|&function_id| {
            let TyKind::Fn(function) = gcx.type_of_item(function_id.into()).kind else {
                return false;
            };
            let candidate = InternalFunctionPointerShape::from_ty(function);
            !shape.is_assembly_cast_compatible_with(&candidate)
                && shape.is_word_compatible_with(&candidate)
        })
    }
}

/// The pointers one internal function pointer dispatcher selects a callee for.
#[derive(Clone)]
pub(super) struct InternalFunctionPointerDispatch {
    shape: InternalFunctionPointerShape,
    /// Whether the pointers may come from inline assembly, which also makes the functions
    /// exposed to assembly with a word-compatible shape reachable.
    from_assembly: bool,
}

fn internal_function_pointer_id(index: usize) -> u64 {
    index as u64 + 1
}

enum CallResult {
    Void,
    /// A canonical value.
    Value(ValueId),
    /// The result of an internal call, which can carry raw bits.
    Word(Word),
}

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    fn new(cx: LoweringContext<'gcx, 'ctx>, function: &'ctx mut Function) -> Self {
        let gcx = cx.gcx;
        Self {
            cx,
            builder: FunctionBuilder::new_semantic(function)
                .with_revert_strings(gcx.sess.opts.revert_strings)
                .with_block_labels(),
            types: types::TypeLowerer::new(gcx),
            values: FxHashMap::default(),
            unchecked_calldata_slices: FxHashSet::default(),
            default_bindings: FxHashSet::default(),
            deferred_bindings: FxHashSet::default(),
            storage_refs: FxHashMap::default(),
            returns: Vec::new(),
            prepared_constructors: FxHashSet::default(),
            inlined_struct_constructors: FxHashSet::default(),
            base_args: Vec::new(),
            loops: Vec::new(),
            modifiers: Vec::new(),
            modifier_depth: 0,
            return_targets: Vec::new(),
            spec: None,
            is_getter: false,
            unchecked: false,
            in_inline_assembly: false,
            assembly_writes_memory: false,
        }
    }

    fn lazy_helper(
        &mut self,
        name: Symbol,
        build: impl FnOnce(&mut Self, &mut Function) -> Option<()>,
    ) -> Option<FunctionId> {
        self.lazy_helper_with(name, true, build)
    }

    /// Like [`Self::lazy_helper`], for a helper that no source construct lacks: `generated` is
    /// false when `build` lowers user code, which keeps its own origins.
    fn lazy_helper_with(
        &mut self,
        name: Symbol,
        generated: bool,
        build: impl FnOnce(&mut Self, &mut Function) -> Option<()>,
    ) -> Option<FunctionId> {
        // if name in helpers { return helpers[name] }
        // id = add_function(name)
        // helpers[name] = id
        // build(id)
        // if build fails { invalid(id); remove helpers[name] }
        // return id
        if let Some(&id) = self.cx.state.helpers.get(&name) {
            return Some(id);
        }

        let ident = Ident::with_dummy_span(name);
        let id = self.cx.module.add_function(Function::new(ident));
        self.cx.state.helpers.insert(name, id);

        let mut function = Function::new(ident);
        if build(self, &mut function).is_none() {
            FunctionBuilder::new_semantic(&mut function).invalid();
            *self.cx.module.function_mut(id) = function;
            self.cx.state.helpers.remove(&name);
            return None;
        }
        function.name = self.cx.module.function(id).name;
        if generated {
            function.mark_unlocated_compiler_generated();
        }
        *self.cx.module.function_mut(id) = function;
        Some(id)
    }

    fn counted_loop<R>(
        &mut self,
        length: ValueId,
        body: impl FnOnce(&mut Self, ValueId) -> R,
    ) -> R {
        let loop_ = self.builder.begin_counted_loop(length);
        let result = body(self, loop_.index());
        self.builder.finish_counted_loop(loop_);
        result
    }

    fn lower_call_arguments<'a, T>(
        &mut self,
        args: hir::CallArgs<'a>,
        params: CallArgumentParams<'_>,
        span: Span,
        error: &'static str,
        lower: impl FnMut(&mut Self, usize, &'a hir::Expr<'a>) -> Option<T>,
    ) -> Option<Vec<T>> {
        match args.kind {
            hir::CallArgsKind::Unnamed(args) => {
                self.lower_argument_exprs(params, args.iter().enumerate(), lower)
            }
            hir::CallArgsKind::Named(args) => {
                let Some(parameter_names) = params.names else {
                    return self.cx.report_unsupported(span, error);
                };
                let arguments = args
                    .iter()
                    .map(|arg| {
                        parameter_names
                            .iter()
                            .position(|&name| name == Some(arg.name.name))
                            .map(|index| (index, &arg.value))
                    })
                    .collect::<Option<Vec<_>>>();
                let Some(arguments) = arguments else {
                    return self.cx.report_unsupported(span, error);
                };
                self.lower_argument_exprs(params, arguments.into_iter(), lower)
            }
        }
    }

    /// Lowers `args` and converts them with `finish`. A storage reference whose `waits` entry, by
    /// evaluation order, is set waits until every argument is lowered, as via-IR converts a
    /// call's arguments; any other operand is converted as soon as it is lowered.
    fn lower_converted_arguments<T>(
        &mut self,
        args: hir::CallArgs<'gcx>,
        params: CallArgumentParams<'_>,
        span: Span,
        error: &'static str,
        mut lower: impl FnMut(&mut Self, usize, &'gcx hir::Expr<'gcx>) -> Option<Lowered<'gcx>>,
        mut finish: impl FnMut(&mut Self, usize, Lowered<'gcx>) -> Option<T>,
    ) -> Option<Vec<T>> {
        let waits = self.operands_wait(args.exprs());
        let mut position = 0;
        let mut waited = false;
        let steps =
            self.lower_call_arguments(args, params, span, error, |this, index, argument| {
                let operand = lower(this, index, argument)?;
                position += 1;
                // Once an operand waits, every later copy waits behind it, so the copies allocate
                // in argument order.
                if (waited && operand.copies()) || (waits[position - 1] && operand.waits()) {
                    waited = true;
                    Some(Step::Wait(operand))
                } else {
                    finish(this, index, operand).map(Step::Done)
                }
            })?;
        self.finish_steps(steps, finish)
    }

    /// Lowers `count` operands and converts them with `finish`, as
    /// [`Self::lower_converted_arguments`] does.
    fn lower_converted_operands<T>(
        &mut self,
        waits: &[bool],
        count: usize,
        mut lower: impl FnMut(&mut Self, usize) -> Option<Lowered<'gcx>>,
        mut finish: impl FnMut(&mut Self, usize, Lowered<'gcx>) -> Option<T>,
    ) -> Option<Vec<T>> {
        let mut waited = false;
        let steps = (0..count)
            .map(|index| {
                let operand = lower(self, index)?;
                if (waited && operand.copies()) || (waits[index] && operand.waits()) {
                    waited = true;
                    Some(Step::Wait(operand))
                } else {
                    finish(self, index, operand).map(Step::Done)
                }
            })
            .collect::<Option<Vec<_>>>()?;
        self.finish_steps(steps, finish)
    }

    /// Converts the operands that waited, in order.
    fn finish_steps<T>(
        &mut self,
        steps: Vec<Step<'gcx, T>>,
        mut finish: impl FnMut(&mut Self, usize, Lowered<'gcx>) -> Option<T>,
    ) -> Option<Vec<T>> {
        steps
            .into_iter()
            .enumerate()
            .map(|(index, step)| match step {
                Step::Done(value) => Some(value),
                Step::Wait(operand) => finish(self, index, operand),
                Step::Literal(never) => match never {},
            })
            .collect()
    }

    fn lower_argument_exprs<'a, T, I>(
        &mut self,
        params: CallArgumentParams<'_>,
        source_args: I,
        mut lower: impl FnMut(&mut Self, usize, &'a hir::Expr<'a>) -> Option<T>,
    ) -> Option<Vec<T>>
    where
        I: DoubleEndedIterator<Item = (usize, &'a hir::Expr<'a>)>,
    {
        let mut values = Vec::with_capacity(params.count);
        values.resize_with(params.count, || None);
        if params.reverse {
            for (index, argument) in source_args.rev() {
                values[index] = Some(lower(self, index, argument)?);
            }
        } else {
            for (index, argument) in source_args {
                values[index] = Some(lower(self, index, argument)?);
            }
        }
        // A slot stays unfilled only when the argument list does not bind every
        // parameter, which sema reports; codegen still runs after a sema error,
        // so bail out instead of lowering a partially bound call.
        values.into_iter().collect()
    }

    fn lower_call_options(
        &mut self,
        options: Option<&hir::CallOptions<'gcx>>,
        allow_value: bool,
        diagnostic: &'static str,
    ) -> Option<LoweredCallOptions> {
        // for option { gas/value = lower(option.value) }
        let mut lowered = self.default_call_options();
        let uint256 = self.cx.gcx.types.uint(256);
        if let Some(options) = options {
            for option in options.args {
                let option_value =
                    self.lower_typed_expr(&option.value, uint256)?.clean(self, uint256);
                match option.name.name {
                    kw::Gas => lowered.gas = Some(option_value),
                    sym::value if allow_value => {
                        lowered.value = option_value;
                        lowered.value_set = true;
                    }
                    _ => {
                        return self.cx.report_unsupported(option.name.span, diagnostic);
                    }
                }
            }
        }
        Some(lowered)
    }

    /// Lowers the options of a call that sets none.
    fn default_call_options(&mut self) -> LoweredCallOptions {
        // zero = 0
        // gas = gas() if can_overcharge_gas_for_call
        // value = zero
        let zero = self.builder.imm(U256::ZERO);
        let evm_version = self.cx.gcx.sess.opts.evm_version;
        let gas = evm_version.can_overcharge_gas_for_call().then(|| self.builder.gas());
        LoweredCallOptions { gas, value: zero, value_set: false, zero }
    }

    fn validate_enum(&mut self, ty: Ty<'gcx>, value: ValueId) {
        let TyKind::Enum(id) = ty.peel_refs().kind else { return };
        self.builder.validate_enum_value(self.cx.gcx.hir.enumm(id).variants.len() as u64, value);
    }

    /// Asserts that type checking registered a type for an operator expression.
    ///
    /// `binary` and `unary` derive the [`ArithmeticKind`] from this type, and a missing type
    /// silently drops the overflow check instead of wrapping or panicking, so the type must be
    /// present whenever the expression is well-formed.
    #[track_caller]
    fn assert_operand_ty_registered(&self, expr: &'gcx hir::Expr<'gcx>) {
        debug_assert!(
            self.cx.gcx.type_of_expr(expr.id).is_some() || self.cx.gcx.dcx().has_errors().is_err(),
            "operator expression has no registered type: {:?}",
            expr.span
        );
    }

    /// Lowers `expr` to its value at its carrier type, and whether it may hold bits above its type.
    ///
    /// This is the only function that lowers an expression. Take the value with
    /// [`Self::operand`], or hand the word to a sink that copies it, [`Self::bind_word`] or
    /// [`Self::store_word`].
    pub(super) fn lower_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> Option<Word> {
        let previous = self.builder.replace_source_span(expr.span);
        let previous_modifier_depth = self.builder.replace_modifier_depth(self.modifier_depth);
        let result = self.lower_expr_inner(expr).map(|word| word.into_native(self, expr));
        self.builder.replace_modifier_depth(previous_modifier_depth);
        self.builder.replace_source_span(previous);
        result
    }

    /// Returns the operand of `expr` when it converts a value to its own type.
    pub(super) fn equal_type_conversion<'a>(
        &self,
        expr: &'a hir::Expr<'a>,
    ) -> Option<&'a hir::Expr<'a>> {
        let ExprKind::Call(callee, args) = &expr.kind else { return None };
        let hir::CallArgsKind::Unnamed([argument]) = args.kind else { return None };
        let ty = self.cx.gcx.type_of_expr(expr.id)?;
        (expressions::is_type_conversion(self.cx.gcx, callee)
            && ty.is_value_type()
            && Some(ty) == self.cx.gcx.type_of_expr(argument.id))
        .then_some(argument)
    }

    fn lower_expr_inner(&mut self, expr: &'gcx hir::Expr<'gcx>) -> Option<Word> {
        // Fold pure value expressions. Literal arithmetic is exact, so its intermediate values may
        // be fractions or may not fit any integer type. Constant names fold in
        // `lower_constant_variable`.
        let computes = matches!(
            expr.kind,
            ExprKind::Unary(..)
                | ExprKind::Binary(..)
                | ExprKind::Ternary(..)
                | ExprKind::Call(..)
                | ExprKind::Member(..)
                | ExprKind::Payable(_)
                | ExprKind::Index(..)
                | ExprKind::Tuple(_)
        );
        // Inline assembly reads raw words, so only literal arithmetic folds there.
        let solidity_value = || {
            !self.in_inline_assembly
                && self.cx.gcx.type_of_expr(expr.id).is_some_and(|ty| ty.is_value_type())
        };
        // value = const_eval(expr)
        if computes
            && (solidity_value() || is_literal_operation(self.cx.gcx, expr))
            && let Ok(value) = self.cx.gcx.const_eval().eval_expr(expr)
            && !matches!(value, ConstValue::Bytes(..))
        {
            // Fractions have no runtime value.
            if let ConstValue::Rational(_) = value {
                return self.cx.report_unsupported(expr.span, "fractional value");
            }
            if let Some(value) = self.lower_const_value(value, expr.span) {
                return Some(Word::from_clean(value));
            }
        }
        // A storage aggregate is its slot, whatever expression names it; its consumer converts it.
        if !self.in_inline_assembly
            && (matches!(
                expr.kind,
                ExprKind::Ident(_)
                    | ExprKind::Member(..)
                    | ExprKind::Index(..)
                    | ExprKind::Ternary(..)
            ) || matches!(&expr.kind, ExprKind::Call(callee, arguments)
                if arguments.is_empty()
                    && self.cx.gcx.resolved_builtin(callee) == Some(Builtin::ArrayPush0))
                || matches!(expr.kind, ExprKind::Assign(_, None, rhs)
                if self.cx.gcx.type_of_expr(rhs.id).is_some_and(|ty| ty.is_ref_at(DataLocation::Storage))))
            && self
                .cx
                .gcx
                .type_of_expr(expr.id)
                .is_some_and(|ty| ty.is_ref_at(DataLocation::Storage))
        {
            return Some(Word::from_clean(self.require_storage_access(expr)?.slot));
        }
        match &expr.kind {
            ExprKind::Lit(lit) if self.in_inline_assembly => {
                Some(Word::from_clean(self.builder.imm(lit.kind.yul_word()?)))
            }
            ExprKind::Lit(lit) => self.lower_literal(lit.kind, expr.span).map(Word::from_clean),
            ExprKind::Array(elements) => self.lower_array(expr, elements).map(Word::from_clean),
            ExprKind::Ident(_) => {
                if let Some(builtin) = self.cx.gcx.resolved_builtin(expr) {
                    return self.lower_builtin_value(expr, builtin).map(Word::from_clean);
                }
                if let Some(value) = self.lower_internal_function_value(expr) {
                    return Some(Word::from_clean(value));
                }
                let id = self.cx.gcx.resolved_variable(expr)?;
                let word = self.load_binding_word(id, expr.span)?;
                if word.is_raw() {
                    return Some(word);
                }
                let value = word.clean(self, self.cx.gcx.type_of_item(id.into()));
                if self.cx.gcx.type_of_expr(expr.id).is_some_and(|ty| ty.is_value_type())
                    && self.cx.gcx.type_of_item(id.into()).is_ref_at(DataLocation::Calldata)
                    && matches!(
                        self.builder.func().value_ty(value),
                        Some(MirType::Slice(PtrLocation::Calldata))
                    )
                {
                    Some(Word::from_clean(self.builder.slice_ptr(value)))
                } else {
                    Some(word)
                }
            }
            ExprKind::Binary(lhs, op, rhs) => {
                self.assert_operand_ty_registered(expr);
                if op.kind.is_logical() {
                    return self.lower_logical(lhs, op.kind, rhs);
                }
                if let Some(function_id) = self.cx.gcx.user_operator(expr.id) {
                    let lhs = self.lower_expr(lhs)?;
                    let rhs = self.lower_expr(rhs)?;
                    return self.lower_user_operator(expr.span, function_id, &[lhs, rhs]);
                }
                if self.cx.gcx.unsupported_udvt_operator(expr.id) {
                    return self.report_unsupported_udvt_operator(expr.span);
                }
                let lhs_ty = self.cx.gcx.type_of_expr(lhs.id);
                let rhs_ty = self.cx.gcx.type_of_expr(rhs.id);
                let fixed_bytes =
                    |ty: Option<Ty<'gcx>>| ty.is_some_and(|ty| ty.peel_refs().is_fixed_bytes());
                let rhs_literal =
                    rhs_ty.is_some_and(|ty| {
                        matches!(
                            ty.peel_refs().kind,
                            TyKind::IntLiteral(..) | TyKind::StringLiteral(..)
                        )
                    }) && !matches!(op.kind, BinOpKind::Shl | BinOpKind::Shr | BinOpKind::Sar);
                let lhs_literal = lhs_ty
                    .is_some_and(|ty| matches!(ty.peel_refs().kind, TyKind::StringLiteral(..)));
                // A literal operand of a fixed-bytes operation is that word.
                // Build it directly instead of allocating a memory literal and
                // reading its first word back through a helper call.
                let rhs_word = if fixed_bytes(lhs_ty) && rhs_literal {
                    lhs_ty.and_then(|ty| self.lower_fixed_bytes_literal(ty, rhs))
                } else {
                    None
                };
                let lhs_word = if fixed_bytes(rhs_ty) && lhs_literal {
                    rhs_ty.and_then(|ty| self.lower_fixed_bytes_literal(ty, lhs))
                } else {
                    None
                };
                let lhs_value = match lhs_word {
                    Some(word) => Word::from_clean(word),
                    None => self.lower_expr(lhs)?,
                };
                let rhs_value = match rhs_word {
                    Some(word) => Word::from_clean(word),
                    None => self.lower_expr(rhs)?,
                };
                // A bitwise operation of same-type operands keeps raw bits, as via-IR does.
                let expr_ty = self.cx.gcx.type_of_expr(expr.id);
                if let Some(ty) = expr_ty
                    && let Some(word) =
                        self.raw_bitwise(op.kind, (lhs, lhs_value), (rhs, rhs_value), ty)
                {
                    return Some(word);
                }
                let lhs = lhs_value.clean(self, lhs_ty?);
                let rhs = rhs_value.clean(self, rhs_ty?);
                let (lhs, rhs) = match (lhs_ty, rhs_ty) {
                    (Some(lhs_ty), Some(rhs_ty))
                        if rhs_word.is_none() && fixed_bytes(Some(lhs_ty)) && rhs_literal =>
                    {
                        (lhs, self.coerce_value(rhs, rhs_ty, lhs_ty))
                    }
                    (Some(lhs_ty), Some(rhs_ty))
                        if lhs_word.is_none() && lhs_literal && fixed_bytes(Some(rhs_ty)) =>
                    {
                        (self.coerce_value(lhs, lhs_ty, rhs_ty), rhs)
                    }
                    _ => (lhs, rhs),
                };
                let lhs_is_literal =
                    lhs_ty.is_some_and(|ty| matches!(ty.peel_refs().kind, TyKind::IntLiteral(..)));
                let rhs_is_literal =
                    rhs_ty.is_some_and(|ty| matches!(ty.peel_refs().kind, TyKind::IntLiteral(..)));
                let signed_literal_arithmetic = lhs_is_literal
                    && rhs_is_literal
                    && lhs_ty
                        .zip(rhs_ty)
                        .is_some_and(|(lhs, rhs)| lhs.is_signed() || rhs.is_signed());
                let ty = if signed_literal_arithmetic {
                    Some(self.cx.gcx.types.int(256))
                } else {
                    match op.kind {
                        BinOpKind::Eq
                        | BinOpKind::Ne
                        | BinOpKind::Lt
                        | BinOpKind::Gt
                        | BinOpKind::Le
                        | BinOpKind::Ge => {
                            if lhs_is_literal && rhs_is_literal {
                                rhs_ty
                            } else {
                                lhs_ty
                                    .zip(rhs_ty)
                                    .and_then(|(lhs, rhs)| lhs.common_type(rhs, self.cx.gcx))
                            }
                        }
                        BinOpKind::Shl | BinOpKind::Shr | BinOpKind::Sar if lhs_is_literal => {
                            expr_ty
                        }
                        BinOpKind::Shl | BinOpKind::Shr | BinOpKind::Sar => lhs_ty,
                        _ => expr_ty,
                    }
                };
                let (lhs, rhs) = if let Some(ty) = ty
                    && arithmetic_kind(ty).is_some()
                {
                    let lhs = lhs_ty.map_or(lhs, |source| self.coerce_value(lhs, source, ty));
                    let rhs = if matches!(
                        op.kind,
                        BinOpKind::Shl | BinOpKind::Shr | BinOpKind::Sar | BinOpKind::Pow
                    ) {
                        rhs
                    } else {
                        rhs_ty.map_or(rhs, |source| self.coerce_value(rhs, source, ty))
                    };
                    (lhs, rhs)
                } else {
                    (lhs, rhs)
                };
                let result = self.binary(op.kind, lhs, rhs, ty);
                Some(Word::from_clean(
                    if let Some(bytes) = expr_ty.and_then(operators::fixed_bytes_width) {
                        self.clean_fixed_bytes(result, bytes)
                    } else {
                        result
                    },
                ))
            }
            // An equal-type conversion passes its operand through.
            ExprKind::Call(..) if let Some(argument) = self.equal_type_conversion(expr) => {
                self.lower_expr(argument)
            }
            ExprKind::Call(callee, args) => {
                let (callee, call_opts) = callee.split_call_options();
                match self.lower_call(expr, callee, *args, call_opts, true)? {
                    CallResult::Void => None,
                    CallResult::Value(value) => Some(Word::from_clean(value)),
                    CallResult::Word(word) => Some(word),
                }
            }
            ExprKind::CallOptions(callee, options) => {
                let value = self.operand(callee)?.id();
                for option in options.args {
                    self.lower_discarded_expr(&option.value)?;
                }
                Some(Word::from_clean(value))
            }
            ExprKind::Unary(op, value) => {
                self.assert_operand_ty_registered(expr);
                if matches!(
                    op.kind,
                    UnOpKind::PreInc | UnOpKind::PostInc | UnOpKind::PreDec | UnOpKind::PostDec
                ) {
                    let place = self.resolve_lvalue_place(value)?;
                    let ty = self.cx.gcx.type_of_expr(value.id)?;
                    let old = self.load_place_word(&place)?;
                    let old_value = old.clean(self, ty);
                    let one = self.builder.imm(1);
                    let kind = if matches!(op.kind, UnOpKind::PreInc | UnOpKind::PostInc) {
                        BinOpKind::Add
                    } else {
                        BinOpKind::Sub
                    };
                    let new = self.binary(kind, old_value, one, Some(ty));
                    self.store_lvalue_place(&place, new)?;
                    // The expression's value is the new one for a prefix operator and the old
                    // one, raw bits included, for a postfix operator.
                    return Some(if matches!(op.kind, UnOpKind::PreInc | UnOpKind::PreDec) {
                        Word::from_clean(new)
                    } else {
                        old
                    });
                }
                if let Some(function_id) = self.cx.gcx.user_operator(expr.id) {
                    let value = self.lower_expr(value)?;
                    return self.lower_user_operator(expr.span, function_id, &[value]);
                }
                if self.cx.gcx.unsupported_udvt_operator(expr.id) {
                    return self.report_unsupported_udvt_operator(expr.span);
                }
                let value = self.operand(value)?.id();
                Some(Word::from_clean(self.unary(
                    op.kind,
                    value,
                    self.cx.gcx.type_of_expr(expr.id),
                )))
            }
            ExprKind::Assign(lhs, op, rhs) => {
                let compound_op = op.map(|op| op.kind);
                if compound_op.is_some() && self.cx.gcx.unsupported_udvt_operator(expr.id) {
                    return self.report_unsupported_udvt_operator(expr.span).map(Word::from_clean);
                }
                if op.is_none() && self.is_storage_reference_binding(lhs) {
                    let access = self.require_storage_access(rhs)?;
                    let Some(id) = self.cx.gcx.resolved_variable(lhs) else {
                        return self.cx.report_unsupported(lhs.span, "storage reference target");
                    };
                    self.storage_refs.insert(id, access);
                    return Some(Word::from_clean(access.slot));
                }
                let lhs_ty = self.type_of_expr_or_variable(lhs)?;
                let rhs_ty = self.cx.gcx.type_of_expr(rhs.id).unwrap_or(lhs_ty);
                let fixed_bytes = operators::fixed_bytes_width(lhs_ty);
                let memory_rhs_ty = rhs_ty.with_loc_if_ref(self.cx.gcx, DataLocation::Memory);
                // A literal assigned to a fixed-bytes place is that word.
                // Building it directly avoids allocating a memory literal whose
                // only use is the first word read back out of it; the
                // allocation writes memory, so nothing later removes it.
                let fixed_bytes_literal = (fixed_bytes.is_some() && compound_op.is_none())
                    .then(|| self.lower_fixed_bytes_literal(lhs_ty, rhs))
                    .flatten();
                let mut place = None;
                let rhs_value = if let Some(word) = fixed_bytes_literal {
                    Word::from_clean(word)
                } else if self.types.memory_layout(memory_rhs_ty).is_some()
                    && rhs_ty.is_ref_at(DataLocation::Storage)
                {
                    if self.may_write_state(lhs) {
                        // The place, and any index it evaluates, runs before the copy.
                        let operand = self.lower_operand(rhs, memory_rhs_ty)?;
                        place = Some(self.resolve_lvalue_place(lhs)?);
                        Word::from_clean(self.convert_operand(operand)?.clean(self, memory_rhs_ty))
                    } else {
                        Word::from_clean(
                            self.lower_typed_expr(rhs, memory_rhs_ty)?.clean(self, memory_rhs_ty),
                        )
                    }
                } else if compound_op.is_some_and(|op| {
                    !matches!(op, BinOpKind::Shl | BinOpKind::Shr | BinOpKind::Sar)
                }) {
                    self.lower_typed_expr(rhs, lhs_ty)?
                } else if compound_op.is_none() && rhs_ty.is_value_type() {
                    self.lower_typed_expr(rhs, rhs_ty)?
                } else {
                    Word::from_clean(self.operand(rhs)?.id())
                };
                // The literal word above is already the place's type, so the
                // conversion below must not widen or shift it a second time.
                let rhs_ty = if fixed_bytes_literal.is_some() { lhs_ty } else { rhs_ty };
                if let Some(kind) = compound_op {
                    let place = self.resolve_lvalue_place(lhs)?;
                    let lhs_word = self.load_place_word(&place)?;
                    // A bitwise operation of same-type operands keeps raw bits, as via-IR does.
                    if self.cx.gcx.user_operator(expr.id).is_none()
                        && let Some(word) =
                            self.raw_bitwise(kind, (lhs, lhs_word), (rhs, rhs_value), lhs_ty)
                    {
                        self.store_word(&place, word, Some(lhs_ty), lhs_ty)?;
                        return Some(word);
                    }
                    let rhs_value = rhs_value.clean(self, lhs_ty);
                    let lhs_value = lhs_word.clean(self, lhs_ty);
                    let value = self.binary(kind, lhs_value, rhs_value, Some(lhs_ty));
                    let value = if let Some(bytes) = fixed_bytes {
                        self.clean_fixed_bytes(value, bytes)
                    } else {
                        self.coerce_value(value, lhs_ty, lhs_ty)
                    };
                    self.store_lvalue_place(&place, value)?;
                    return Some(Word::from_clean(value));
                }
                if lhs_ty.is_value_type() && rhs_ty.is_value_type() {
                    // The assigned value keeps its raw bits when the target is a raw carrier.
                    let carried = if rhs_ty == lhs_ty {
                        rhs_value
                    } else {
                        let value = rhs_value.clean(self, rhs_ty);
                        Word::from_clean(self.coerce_value(value, rhs_ty, lhs_ty))
                    };
                    let place = self.resolve_lvalue_place(lhs)?;
                    self.store_word(&place, carried, Some(rhs_ty), lhs_ty)?;
                    return Some(carried);
                }
                let rhs_value = rhs_value.clean(self, rhs_ty);
                let preserve_calldata_slice = lhs_ty.is_ref_at(DataLocation::Calldata)
                    && matches!(
                        self.builder.func().value_ty(rhs_value),
                        Some(MirType::Slice(PtrLocation::Calldata))
                    );
                let value = if preserve_calldata_slice {
                    rhs_value
                } else {
                    if lhs_ty.is_ref_at(DataLocation::Storage) {
                        self.materialize_memory_argument(memory_rhs_ty, rhs_value, rhs.span)?
                    } else {
                        self.materialize_memory_copy(lhs_ty, rhs_value, rhs.span)?
                    }
                };
                let value = self.coerce_value(value, rhs_ty, lhs_ty);
                let place = match place {
                    Some(place) => place,
                    None => self.resolve_lvalue_place(lhs)?,
                };
                self.store_lvalue_place_with_source(&place, value, Some(rhs_ty))?;
                // The value of an assignment to a storage aggregate is its storage reference.
                if let LValuePlace::Storage { access, .. } = place
                    && self.cx.gcx.type_of_expr(expr.id).is_some_and(|ty| {
                        ty.is_ref_at(DataLocation::Storage)
                            && self.types.memory_layout(ty).is_some()
                    })
                {
                    return Some(Word::from_clean(access.slot));
                }
                Some(Word::from_clean(value))
            }
            ExprKind::Ternary(cond, then_expr, else_expr) => {
                let ty = self.cx.gcx.type_of_expr(expr.id)?;
                self.lower_ternary(cond, then_expr, else_expr, ty)
            }
            ExprKind::Tuple([Some(inner)]) => self.lower_expr(inner),
            ExprKind::Tuple(values) => self.lower_tuple(expr, values).map(Word::from_clean),
            ExprKind::Member(receiver, name) => {
                self.lower_member(expr, receiver, *name).map(Word::from_clean)
            }
            ExprKind::YulMember(..) => {
                let place = self.resolve_lvalue_place(expr)?;
                self.load_lvalue_place(&place).map(Word::from_clean)
            }
            ExprKind::Index(receiver, index) => {
                self.lower_index(expr, receiver, *index).map(Word::from_clean)
            }
            ExprKind::Slice(receiver, start, end) => {
                self.lower_slice(expr, receiver, *start, *end).map(Word::from_clean)
            }
            // `payable(x)` of an `address payable` passes `x` through.
            ExprKind::Payable(value)
                if self.cx.gcx.type_of_expr(value.id) == self.cx.gcx.type_of_expr(expr.id) =>
            {
                self.lower_expr(value)
            }
            ExprKind::Payable(value) => {
                self.operand(value).map(|value| Word::from_clean(value.id()))
            }
            _ if self.cx.gcx.dcx().has_errors().is_err() => None,
            _ => self.cx.report_unsupported(expr.span, "expression"),
        }
    }

    fn report_unsupported_udvt_operator<T>(&self, span: Span) -> Option<T> {
        self.cx
            .gcx
            .dcx()
            .err_at(span, "user-defined operators are not supported in this codegen path")
            .help("unwrap the user-defined value type before using this operator")
            .emit();
        None
    }
}

pub(super) fn generate_internal_function_pointer_dispatchers(
    gcx: Gcx<'_>,
    module: &mut Module,
    function_ids: &FxHashMap<hir::FunctionId, FunctionId>,
    state: &LoweringState,
) {
    let dispatchers = module
        .iter_functions()
        .filter(|(_, function)| function.attributes.is_function_pointer_dispatcher)
        .map(|(id, _)| (state.pointer_registry.dispatchers[&id].clone(), id))
        .collect::<Vec<_>>();
    for (InternalFunctionPointerDispatch { shape, from_assembly }, dispatcher) in dispatchers {
        let candidates = state
            .pointer_registry
            .targets
            .iter()
            .enumerate()
            .filter_map(|(index, &function_id)| {
                let TyKind::Fn(function) = gcx.type_of_item(function_id.into()).kind else {
                    return None;
                };
                let candidate_shape = InternalFunctionPointerShape::from_ty(function);
                (shape.is_assembly_cast_compatible_with(&candidate_shape)
                    || (from_assembly
                        && state.pointer_registry.assembly_exposed.contains(&function_id)
                        && shape.is_word_compatible_with(&candidate_shape)))
                .then_some((index, function_id, candidate_shape))
            })
            .filter_map(|(index, function_id, candidate_shape)| {
                function_ids
                    .get(&function_id)
                    .copied()
                    .map(|mir_id| (index, function_id, mir_id, candidate_shape))
            })
            .collect::<Vec<_>>();

        let reserved = module.function(dispatcher);
        let mut function = Function::new(Ident::new(reserved.name.symbol, reserved.name_span));
        function.name = reserved.name;
        function.attributes.is_function_pointer_dispatcher = true;
        {
            let mut builder = FunctionBuilder::new_semantic(&mut function);
            let function_value = builder.add_param(MirType::I256);
            let arguments = shape
                .params
                .iter()
                .map(|ty| builder.add_param(state.pointer_carrier(ty.mir_type())))
                .collect::<Vec<_>>();
            if let Some(ty) = module.intern_return_type(
                shape.returns.iter().map(|ty| state.pointer_carrier(ty.mir_type())).collect(),
            ) {
                builder.set_return_type(ty);
            }

            // for target {
            //     if function_id == target {
            //         results = icall(target, reinterpret(arguments))
            //         return reinterpret(results)
            //     }
            // }
            for (index, function_id, mir_id, candidate_shape) in candidates {
                let case_block = builder.create_block();
                let next_block = builder.create_block();
                let id = builder.imm(internal_function_pointer_id(index));
                let is_match = builder.eq(function_value, id);
                builder.branch(is_match, case_block, next_block);

                builder.switch_to_block(case_block);
                let previous_span = builder.replace_source_span(gcx.hir.function(function_id).span);
                let call_arguments = arguments
                    .iter()
                    .copied()
                    .zip(&shape.params)
                    .zip(&candidate_shape.params)
                    .zip(gcx.hir.function(function_id).parameters)
                    .map(|(((argument, &source), &target), &parameter)| {
                        let carrier = state.scalar_carrier(gcx, parameter);
                        // A parameter that is not a raw carrier is read as a clean value, so
                        // the word is cleaned even when it travels as a full word. Equal layouts
                        // need not mean equal types: a user-defined value type shares the layout
                        // of its underlying type, and the raw-flow analysis links only equal
                        // types.
                        let raw = state.is_raw_scalar(gcx, parameter);
                        if source == target && (raw || carrier != MirType::I256) {
                            raw_scalars::cast_carrier(&mut builder, argument, source, carrier)
                        } else {
                            reinterpret_word(&mut builder, argument, source, target, carrier, raw)
                        }
                    })
                    .collect::<Vec<_>>();
                if shape.returns.is_empty() {
                    builder.icall_void(mir_id, call_arguments);
                    builder.ret([]);
                } else {
                    // result = icall target(arguments)
                    // ret result
                    let result_ty = module.function(mir_id).return_type();
                    let result = builder.icall(mir_id, call_arguments, result_ty);
                    let cast_return = |builder: &mut FunctionBuilder<'_>, value, index, ty| {
                        let (source, target) =
                            (candidate_shape.returns[index], shape.returns[index]);
                        if source == target {
                            raw_scalars::cast_carrier(builder, value, target, ty)
                        } else {
                            reinterpret_word(
                                builder,
                                value,
                                source,
                                target,
                                ty,
                                ty == MirType::I256,
                            )
                        }
                    };
                    let result = if let (MirType::Struct(source), MirType::Struct(target)) =
                        (result_ty, builder.func().return_type())
                        && source != target
                    {
                        let values = module.struct_types[source]
                            .fields
                            .iter()
                            .copied()
                            .zip(module.struct_types[target].fields.iter().copied())
                            .enumerate()
                            .map(|(index, (from, to))| {
                                let value =
                                    builder.extract_value(source, result, index as u32, from);
                                cast_return(&mut builder, value, index, to)
                            })
                            .collect::<Vec<_>>();
                        builder.make_struct(target, values)
                    } else if shape.returns.len() == 1 {
                        let ty = builder.func().return_type();
                        cast_return(&mut builder, result, 0, ty)
                    } else {
                        result
                    };
                    builder.ret([result]);
                }
                builder.replace_source_span(previous_span);
                builder.switch_to_block(next_block);
            }

            // panic(InvalidInternalFunction)
            builder.panic(PanicCode::InvalidInternalFunction);
        }
        function.mark_unlocated_compiler_generated();
        *module.function_mut(dispatcher) = function;
    }
}

fn arithmetic_kind(ty: Ty<'_>) -> Option<ArithmeticKind> {
    match ty.peel_refs().kind {
        TyKind::Udvt(inner, _) => arithmetic_kind(inner),
        TyKind::Elementary(elementary) => match elementary {
            solar_sema::hir::ElementaryType::UInt(size) => {
                Some(ArithmeticKind::Unsigned(size.bits()))
            }
            solar_sema::hir::ElementaryType::Int(size) => Some(ArithmeticKind::Signed(size.bits())),
            _ => None,
        },
        _ => None,
    }
}

fn is_signed_packed_scalar(ty: Ty<'_>) -> bool {
    match ty.peel_refs().kind {
        TyKind::Udvt(inner, _) => is_signed_packed_scalar(inner),
        TyKind::Elementary(solar_sema::hir::ElementaryType::Int(_)) => true,
        _ => false,
    }
}

fn report_error<T>(gcx: Gcx<'_>, span: Span, message: &'static str) -> Option<T> {
    gcx.dcx().emit_err(span, message);
    None
}

/// Reinterprets a one-word value that inline assembly retyped from `source` to `target`.
fn reinterpret_word(
    builder: &mut FunctionBuilder<'_>,
    value: ValueId,
    source: crate::mir::ValueLayout,
    target: crate::mir::ValueLayout,
    carrier: MirType,
    raw: bool,
) -> ValueId {
    // word = zext | sext | ptrtoint value to i256
    // word = canonical(target, word), unless `raw`
    // value = trunc | ne 0 | inttoptr word to carrier
    let word = raw_scalars::cast_carrier(builder, value, source, MirType::I256);
    // A raw word passes the bits on as a direct call would: the receiver cleans them where
    // Solidity reads the value, and its assembly sees them as they are. Internal function pointers
    // are small identifiers: the ABI's left-aligned cleanup of external function words does not
    // apply to them.
    let word = if raw || target == crate::mir::ValueLayout::Function {
        word
    } else {
        AbiWordValidator::from_layout(target)
            .map_or(word, |validator| validator.cleanup(builder, word))
    };
    builder.cast(word, carrier)
}

/// Returns `true` if `expr` is a number literal, an operation on them, or a comparison of them.
///
/// Only literal arithmetic has literal types, so this needs no walk of the operands.
fn is_literal_operation(gcx: Gcx<'_>, expr: &hir::Expr<'_>) -> bool {
    let is_literal =
        |expr: &hir::Expr<'_>| gcx.type_of_expr(expr.id).is_some_and(Ty::is_number_literal);
    match &expr.kind {
        ExprKind::Binary(lhs, op, rhs) if op.kind.is_cmp() => is_literal(lhs) && is_literal(rhs),
        _ => is_literal(expr),
    }
}
