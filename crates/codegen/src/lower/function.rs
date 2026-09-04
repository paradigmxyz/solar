//! Function-level HIR to MIR lowering.

use super::{
    ContractBytecodes, contract,
    storage::{StorageEncoding, StorageLayout, StorageLocation},
    types,
};
use crate::{
    memory::EvmMemoryLayout,
    mir::{
        AbiLayout, AbiParamLayout, AbiParamLocation, AbiParamType, AbiType, AbiWordValidator,
        AllocationSemantics, BlockId, ERROR_SELECTOR, FrameMode, FrameSlotKind, Function,
        FunctionBuilder, FunctionId, ImmutableId, InstKind, LibraryLink, MemoryObjectKind,
        MemoryObjectLayout, MirType, Module, PanicCode, RevertReason, SliceLocation, Value,
        ValueId,
    },
};
use alloy_primitives::{U256, keccak256};
use solar_ast::{BinOpKind, DataLocation, LitKind, StateMutability, StrKind, TypeSize, UnOpKind};
use solar_data_structures::map::{FxHashMap, FxHashSet, StdEntry};
use solar_interface::{ByteSymbol, Ident, Span, Symbol, kw, sym};
use solar_sema::{
    Gcx,
    builtins::Builtin,
    eval::ConstValue,
    hir::{self, ElementaryType, ExprKind, LoopSource, StmtKind, VariableId},
    ty::{CallableParamNames, CallableParamSource, Ty, TyFn, TyKind},
};
use std::{fmt::Display, sync::Arc};

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
mod statements;
mod storage_values;
mod values;

/// Shared inputs for one contract's function lowering.
pub(super) struct LoweringContext<'gcx, 'ctx> {
    pub(super) gcx: Gcx<'gcx>,
    pub(super) module: &'ctx mut Module,
    pub(super) storage: &'ctx StorageLayout<'gcx>,
    pub(super) contract_id: hir::ContractId,
    pub(super) function_ids: &'ctx FxHashMap<hir::FunctionId, FunctionId>,
    pub(super) immutable_ids: &'ctx FxHashMap<VariableId, ImmutableId>,
    pub(super) child_bytecodes: &'ctx FxHashMap<hir::ContractId, ContractBytecodes>,
    pub(super) state: &'ctx mut LoweringState,
    pub(super) shared_literals: &'ctx FxHashSet<ByteSymbol>,
    pub(super) shared_word_literals: &'ctx FxHashSet<ByteSymbol>,
    pub(super) share_storage_bytes: bool,
    /// Whether the compilation had already failed when the code generation
    /// phase started.
    pub(super) sema_errored: bool,
}

impl<'gcx, 'ctx> LoweringContext<'gcx, 'ctx> {
    pub(super) fn reborrow<'a>(&'a mut self) -> LoweringContext<'gcx, 'a> {
        LoweringContext {
            gcx: self.gcx,
            module: &mut *self.module,
            storage: self.storage,
            contract_id: self.contract_id,
            function_ids: self.function_ids,
            immutable_ids: self.immutable_ids,
            child_bytecodes: self.child_bytecodes,
            state: &mut *self.state,
            shared_literals: self.shared_literals,
            shared_word_literals: self.shared_word_literals,
            share_storage_bytes: self.share_storage_bytes,
            sema_errored: self.sema_errored,
        }
    }

    /// Reports a lowering bail-out and returns `None`.
    ///
    /// A bail-out is only worth reporting when the compilation would otherwise
    /// succeed. After a sema error the bytecode is withheld anyway, and the
    /// construct that lowering cannot handle is usually the rejected one, so
    /// reporting it adds a second, misleading error.
    pub(super) fn report_unsupported<T>(&self, span: Span, what: &str) -> Option<T> {
        if self.sema_errored {
            return None;
        }
        self.gcx
            .dcx()
            .err(format!("codegen rewrite does not support this {what} yet"))
            .span(span)
            .emit();
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
    pub(super) invalid_event_topics: FxHashSet<hir::EventId>,
    pub(super) pointer_registry: InternalFunctionPointerRegistry,
    pub(super) helpers: FxHashMap<Symbol, FunctionId>,
}

/// Lowers one HIR function into a typed MIR function.
pub(super) fn lower(
    mut context: LoweringContext<'_, '_>,
    id: hir::FunctionId,
    expose_selector: bool,
) -> Option<Function> {
    let gcx = context.gcx;
    let hir_function = gcx.hir.function(id);
    let mut mir = contract::declaration(gcx, id, hir_function);
    if !expose_selector {
        mir.selector = None;
    }
    let has_constructor_params =
        mir.attributes.is_constructor && !hir_function.parameters.is_empty();
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
            let has_calldata_aggregate_return = hir_function.returns.iter().any(|&ret| {
                types::TypeLowerer::mir_return_type(gcx.type_of_item(ret.into()))
                    == MirType::Slice(SliceLocation::Calldata)
            });
            if has_calldata_aggregate_return
                || output_param_shapes.iter().any(AbiParamType::needs_nested_return_cleanup)
            {
                mir.abi_return_params =
                    Some(AbiParamLayout::new(output_param_shapes.into_boxed_slice()));
            }
        }
    }

    let mut lowerer = FunctionLowerer::new(context.reborrow(), &mut mir);
    lowerer.is_getter = hir_function.is_getter();
    lowerer.bind_signature(hir_function);
    if hir_function.kind == hir::FunctionKind::Constructor {
        let Some(contract_id) = hir_function.contract else {
            return context.report_unsupported(hir_function.span, "free constructor");
        };
        lowerer.lower_implicit_base_constructors(contract_id)?;
        lowerer.lower_state_initializers(contract_id)?;
    }
    if let Some(body) = hir_function.body {
        lowerer.lower_function_body(hir_function.modifiers, body)?;
    }
    if !lowerer.is_terminated() {
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
    let mut mir =
        Function::new(solar_interface::Ident::with_dummy_span(solar_interface::kw::Constructor));
    mir.attributes.is_constructor = true;
    let mut lowerer = FunctionLowerer::new(context.reborrow(), &mut mir);
    lowerer.lower_implicit_base_constructors(contract_id)?;
    lowerer.lower_state_initializers(contract_id)?;
    if !lowerer.is_terminated() {
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
    dirty_values: FxHashSet<ValueId>,
    default_bindings: FxHashSet<VariableId>,
    deferred_bindings: FxHashSet<VariableId>,
    storage_refs: FxHashMap<VariableId, StorageAccess>,
    parameters: Vec<VariableId>,
    returns: Vec<VariableId>,
    prepared_constructors: FxHashSet<hir::FunctionId>,
    /// The base constructor argument list chosen for each base of the contract
    /// being constructed, in evaluation order.
    base_args: Vec<(hir::ContractId, hir::CallArgs<'gcx>)>,
    loops: Vec<LoopTargets>,
    modifiers: Vec<ModifierContext<'gcx>>,
    modifier_depth: u32,
    return_targets: Vec<ReturnTarget>,
    is_getter: bool,
    unchecked: bool,
    in_inline_assembly: bool,
    /// The expressions being lowered whose values nothing observes: a discarded expression
    /// statement, and the tuple declaration and assignment components that have no target.
    ///
    /// Lowering an expression that has to read storage to produce its value can skip the read
    /// here. Only the discarded expression itself and the tuple components and conditional
    /// branches that just hand their value up to it qualify: every other subexpression feeds the
    /// value it belongs to.
    discarded_exprs: Vec<hir::ExprId>,
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
    /// A pre-EIP-150 call reserves the value-transfer cost whenever the option is there, like
    /// solc's `valueSet`, because the amount is not known to be zero.
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
    parameters: BindingSnapshot,
    returns: BindingSnapshot,
    incoming_returns: BindingSnapshot,
}

struct ReturnTarget {
    block: BlockId,
    states: Vec<LoopState>,
}

enum PreparedRevertPayload {
    ShortString { length: ValueId, data: ValueId },
    EmptyString,
    ErrorString(ValueId),
    CustomError { selector: ValueId, layout: Arc<AbiLayout>, values: Box<[ValueId]> },
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
    MemoryField { object: ValueId, layout: MemoryObjectLayout, field: u64, ty: Ty<'gcx> },
    MemoryElement { object: ValueId, layout: MemoryObjectLayout, index: ValueId, ty: Ty<'gcx> },
    MemoryByte { object: ValueId, index: ValueId, ty: Ty<'gcx> },
}

#[derive(Clone, Copy)]
enum ArithmeticKind {
    Unsigned(u16),
    Signed(u16),
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
    params: Vec<MirType>,
    returns: Vec<MirType>,
}

impl InternalFunctionPointerShape {
    fn from_ty(function: &TyFn<'_>) -> Self {
        Self {
            params: function
                .parameters
                .iter()
                .map(|&ty| types::TypeLowerer::mir_type(ty))
                .collect(),
            returns: function
                .returns
                .iter()
                .map(|&ty| types::TypeLowerer::mir_return_type(ty))
                .collect(),
        }
    }

    fn from_function(function: &Function) -> Self {
        Self {
            params: function.params.iter().copied().skip(1).collect(),
            returns: function.returns.clone(),
        }
    }

    fn is_assembly_cast_compatible_with(&self, target: &Self) -> bool {
        // Assembly casts preserve these full-word argument representations. Keep return shapes
        // exact because internal calls can expose dirty return words.
        let canonicalize = |ty: MirType| {
            if ty == MirType::Address || ty.is_full_abi_word() { MirType::uint256() } else { ty }
        };
        self.params.iter().copied().map(canonicalize).eq(target
            .params
            .iter()
            .copied()
            .map(canonicalize))
            && self.returns == target.returns
    }

    fn helper_name(&self) -> Symbol {
        let params = self.params.iter().map(ToString::to_string).collect::<Vec<_>>().join("_");
        let returns = self.returns.iter().map(ToString::to_string).collect::<Vec<_>>().join("_");
        helper_name(
            sym::internal_dispatcher,
            format!(
                "p_{}_r_{}",
                if params.is_empty() { "none" } else { &params },
                if returns.is_empty() { "none" } else { &returns },
            ),
        )
    }
}

fn helper_name(prefix: Symbol, suffix: impl Display) -> Symbol {
    Symbol::intern(&format!("{prefix}_{suffix}"))
}

#[derive(Default)]
pub(super) struct InternalFunctionPointerRegistry {
    targets: FxHashSet<hir::FunctionId>,
}

fn internal_function_pointer_id(function_id: hir::FunctionId) -> u64 {
    function_id.index() as u64 + 1
}

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    fn new(cx: LoweringContext<'gcx, 'ctx>, function: &'ctx mut Function) -> Self {
        let gcx = cx.gcx;
        Self {
            cx,
            builder: FunctionBuilder::new(function)
                .with_revert_strings(gcx.sess.opts.revert_strings),
            types: types::TypeLowerer::new(gcx),
            values: FxHashMap::default(),
            dirty_values: FxHashSet::default(),
            default_bindings: FxHashSet::default(),
            deferred_bindings: FxHashSet::default(),
            storage_refs: FxHashMap::default(),
            parameters: Vec::new(),
            returns: Vec::new(),
            prepared_constructors: FxHashSet::default(),
            base_args: Vec::new(),
            loops: Vec::new(),
            modifiers: Vec::new(),
            modifier_depth: 0,
            return_targets: Vec::new(),
            is_getter: false,
            unchecked: false,
            in_inline_assembly: false,
            discarded_exprs: Vec::new(),
        }
    }

    fn lazy_helper(
        &mut self,
        name: Symbol,
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
            FunctionBuilder::new(&mut function).invalid();
            *self.cx.module.function_mut(id) = function;
            self.cx.state.helpers.remove(&name);
            return None;
        }
        function.name = self.cx.module.function(id).name;
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
        options: Option<&hir::CallOptions<'_>>,
        allow_value: bool,
        diagnostic: &'static str,
    ) -> Option<LoweredCallOptions> {
        // zero = 0
        // gas = gas() if can_overcharge_gas_for_call
        // value = zero
        // for option { gas/value = lower(option.value) }
        let zero = self.builder.imm(U256::ZERO);
        let evm_version = self.cx.gcx.sess.opts.evm_version;
        let mut gas = evm_version.can_overcharge_gas_for_call().then(|| self.builder.gas());
        let mut value = zero;
        let mut value_set = false;
        if let Some(options) = options {
            for option in options.args {
                let option_value =
                    self.lower_typed_expr(&option.value, self.cx.gcx.types.uint(256))?;
                match option.name.name {
                    kw::Gas => gas = Some(option_value),
                    sym::value if allow_value => {
                        value = option_value;
                        value_set = true;
                    }
                    _ => {
                        return self.cx.report_unsupported(option.name.span, diagnostic);
                    }
                }
            }
        }
        Some(LoweredCallOptions { gas, value, value_set, zero })
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
    fn assert_operand_ty_registered(&self, expr: &hir::Expr<'_>) {
        debug_assert!(
            self.cx.gcx.type_of_expr(expr.id).is_some() || self.cx.gcx.dcx().has_errors().is_err(),
            "operator expression has no registered type: {:?}",
            expr.span
        );
    }

    fn lower_expr(&mut self, expr: &hir::Expr<'_>) -> Option<ValueId> {
        let previous = self.builder.replace_source_span(expr.span);
        let previous_modifier_depth = self.builder.replace_modifier_depth(self.modifier_depth);
        let result = self.lower_expr_inner(expr);
        self.builder.replace_modifier_depth(previous_modifier_depth);
        self.builder.replace_source_span(previous);
        result
    }

    fn lower_expr_inner(&mut self, expr: &hir::Expr<'_>) -> Option<ValueId> {
        // value = const_eval(expr)
        if int_literal_expr_contains_wide(self.cx.gcx, expr).is_some_and(|wide| wide)
            && let Ok(value) = self.cx.gcx.try_eval_const(expr)
            && value.bit_len() <= 256
        {
            return Some(self.builder.imm(value.as_evm_word()));
        }
        match &expr.kind {
            ExprKind::Lit(lit) => self.lower_literal(lit.kind, expr.span),
            ExprKind::Array(elements) => self.lower_array(expr, elements),
            ExprKind::Ident(_) => {
                if let Some(builtin) = self.cx.gcx.resolved_builtin(expr) {
                    return self.lower_builtin_value(expr, builtin);
                }
                if let Some(value) = self.lower_internal_function_value(expr) {
                    return Some(value);
                }
                let id = self.cx.gcx.resolved_variable(expr)?;
                let value = self.load_variable(id, expr.span)?;
                if self.cx.gcx.type_of_expr(expr.id).is_some_and(|ty| ty.is_value_type())
                    && self.cx.gcx.type_of_item(id.into()).is_ref_at(DataLocation::Calldata)
                    && matches!(
                        self.builder.func().value_ty(value),
                        Some(MirType::Slice(SliceLocation::Calldata))
                    )
                {
                    Some(self.builder.slice_ptr(value))
                } else {
                    Some(value)
                }
            }
            ExprKind::Binary(lhs, op, rhs) => {
                self.assert_operand_ty_registered(expr);
                if matches!(op.kind, BinOpKind::And | BinOpKind::Or) {
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
                let mut lhs = self.lower_expr(lhs)?;
                if let Some(ty) = lhs_ty {
                    lhs = self.normalize_dirty_scalar(lhs, ty);
                }
                let rhs_ty = self.cx.gcx.type_of_expr(rhs.id);
                let mut rhs = self.lower_expr(rhs)?;
                if let Some(ty) = rhs_ty {
                    rhs = self.normalize_dirty_scalar(rhs, ty);
                }
                let (lhs, rhs) = match (lhs_ty, rhs_ty) {
                    (Some(lhs_ty), Some(rhs_ty))
                        if matches!(
                            lhs_ty.peel_refs().kind,
                            TyKind::Elementary(solar_sema::hir::ElementaryType::FixedBytes(_))
                        ) && matches!(
                            rhs_ty.peel_refs().kind,
                            TyKind::IntLiteral(..) | TyKind::StringLiteral(..)
                        ) && !matches!(
                            op.kind,
                            BinOpKind::Shl | BinOpKind::Shr | BinOpKind::Sar
                        ) =>
                    {
                        (lhs, self.coerce_value(rhs, rhs_ty, lhs_ty))
                    }
                    (Some(lhs_ty), Some(rhs_ty))
                        if matches!(lhs_ty.peel_refs().kind, TyKind::StringLiteral(..))
                            && matches!(
                                rhs_ty.peel_refs().kind,
                                TyKind::Elementary(solar_sema::hir::ElementaryType::FixedBytes(_))
                            ) =>
                    {
                        (self.coerce_value(lhs, lhs_ty, rhs_ty), rhs)
                    }
                    _ => (lhs, rhs),
                };
                let expr_ty = self.cx.gcx.type_of_expr(expr.id);
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
                        BinOpKind::Lt | BinOpKind::Gt | BinOpKind::Le | BinOpKind::Ge
                            if lhs_is_literal =>
                        {
                            rhs_ty
                        }
                        BinOpKind::Lt | BinOpKind::Gt | BinOpKind::Le | BinOpKind::Ge => lhs_ty,
                        BinOpKind::Shl | BinOpKind::Shr | BinOpKind::Sar if lhs_is_literal => {
                            expr_ty
                        }
                        BinOpKind::Shl | BinOpKind::Shr | BinOpKind::Sar => lhs_ty,
                        _ => expr_ty,
                    }
                };
                let result = self.binary(op.kind, lhs, rhs, ty);
                Some(if let Some(bytes) = expr_ty.and_then(operators::fixed_bytes_width) {
                    self.clean_fixed_bytes(result, bytes)
                } else {
                    result
                })
            }
            ExprKind::Call(callee, args, call_opts) => {
                self.lower_call(expr, callee, *args, *call_opts)
            }
            ExprKind::Delete(value) => {
                self.delete_lvalue(value)?;
                Some(self.builder.imm(U256::ZERO))
            }
            ExprKind::Unary(op, value) => {
                self.assert_operand_ty_registered(expr);
                if matches!(
                    op.kind,
                    UnOpKind::PreInc | UnOpKind::PostInc | UnOpKind::PreDec | UnOpKind::PostDec
                ) {
                    let place = self.resolve_lvalue_place(value)?;
                    let ty = self.cx.gcx.type_of_expr(value.id);
                    let old = self.load_lvalue_place(&place)?;
                    let old = ty.map_or(old, |ty| self.normalize_dirty_scalar(old, ty));
                    let one = self.builder.imm(1);
                    let kind = if matches!(op.kind, UnOpKind::PreInc | UnOpKind::PostInc) {
                        BinOpKind::Add
                    } else {
                        BinOpKind::Sub
                    };
                    let new = self.binary(kind, old, one, ty);
                    self.store_lvalue_place(&place, new)?;
                    return Some(if matches!(op.kind, UnOpKind::PreInc | UnOpKind::PreDec) {
                        new
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
                let ty = self.cx.gcx.type_of_expr(value.id);
                let value = self.lower_expr(value)?;
                let value = ty.map_or(value, |ty| self.normalize_dirty_scalar(value, ty));
                Some(self.unary(op.kind, value, self.cx.gcx.type_of_expr(expr.id)))
            }
            ExprKind::Assign(lhs, op, rhs) => {
                let compound_op = op.map(|op| op.kind);
                if compound_op.is_some() && self.cx.gcx.unsupported_udvt_operator(expr.id) {
                    return self.report_unsupported_udvt_operator(expr.span);
                }
                if op.is_none()
                    && let ExprKind::Tuple(elements) = &lhs.peel_parens().kind
                {
                    self.lower_tuple_assignment(elements, rhs)?;
                    return Some(self.builder.imm(U256::ZERO));
                }
                if op.is_none() && self.is_storage_reference_binding(lhs) {
                    let Some(access) = self.storage_access(rhs) else {
                        return self.cx.report_unsupported(rhs.span, "storage access");
                    };
                    let Some(id) = self.cx.gcx.resolved_variable(lhs) else {
                        return self.cx.report_unsupported(lhs.span, "storage reference target");
                    };
                    self.storage_refs.insert(id, access);
                    return Some(self.builder.imm(U256::ZERO));
                }
                let lhs_ty = self.type_of_expr_or_variable(lhs)?;
                let fixed_bytes = operators::fixed_bytes_width(lhs_ty);
                let rhs_ty = self.cx.gcx.type_of_expr(rhs.id).unwrap_or(lhs_ty);
                let memory_rhs_ty = rhs_ty.with_loc_if_ref(self.cx.gcx, DataLocation::Memory);
                let rhs_value = if self.in_inline_assembly {
                    self.lower_yul_word_expr(rhs)?
                } else if self.types.memory_layout(memory_rhs_ty).is_some()
                    && rhs_ty.is_ref_at(DataLocation::Storage)
                {
                    self.lower_typed_expr(rhs, memory_rhs_ty)?
                } else if fixed_bytes.is_some()
                    && compound_op.is_some_and(|op| {
                        !matches!(op, BinOpKind::Shl | BinOpKind::Shr | BinOpKind::Sar)
                    })
                {
                    self.lower_typed_expr(rhs, lhs_ty)?
                } else {
                    self.lower_expr(rhs)?
                };
                if let Some(kind) = compound_op {
                    let place = self.resolve_lvalue_place(lhs)?;
                    let lhs_value = self.load_lvalue_place(&place)?;
                    let lhs_value = self.normalize_dirty_scalar(lhs_value, lhs_ty);
                    let rhs_value = self.normalize_dirty_scalar(rhs_value, rhs_ty);
                    let value = self.binary(kind, lhs_value, rhs_value, Some(lhs_ty));
                    let value = if let Some(bytes) = fixed_bytes {
                        self.clean_fixed_bytes(value, bytes)
                    } else {
                        self.coerce_value(value, rhs_ty, lhs_ty)
                    };
                    self.store_lvalue_place(&place, value)?;
                    return Some(value);
                }
                let preserve_calldata_slice = lhs_ty.is_ref_at(DataLocation::Calldata)
                    && matches!(
                        self.builder.func().value_ty(rhs_value),
                        Some(MirType::Slice(SliceLocation::Calldata))
                    );
                let value = if preserve_calldata_slice {
                    rhs_value
                } else {
                    let materialize_ty = if lhs_ty.is_ref_at(DataLocation::Storage) {
                        memory_rhs_ty
                    } else {
                        lhs_ty
                    };
                    self.materialize_memory_argument(materialize_ty, rhs_value, rhs.span)?
                };
                let value = self.coerce_value(value, rhs_ty, lhs_ty);
                self.store_lvalue_with_source(lhs, value, Some(rhs_ty))?;
                Some(value)
            }
            ExprKind::Ternary(cond, then_expr, else_expr) => {
                self.lower_ternary(cond, then_expr, else_expr)
            }
            ExprKind::Tuple([Some(inner)]) => self.lower_expr(inner),
            ExprKind::Tuple(values) => self.lower_tuple(expr, values),
            ExprKind::Member(receiver, name) => self.lower_member(expr, receiver, *name),
            ExprKind::YulMember(receiver, name) => self.lower_yul_member(expr, receiver, *name),
            ExprKind::Index(receiver, index) => self.lower_index(expr, receiver, *index),
            ExprKind::Slice(receiver, start, end) => self.lower_slice(expr, receiver, *start, *end),
            ExprKind::Payable(value) => self.lower_expr(value),
            _ if self.cx.gcx.dcx().has_errors().is_err() => Some(self.builder.imm(U256::ZERO)),
            _ => self.cx.report_unsupported(expr.span, "expression"),
        }
    }

    fn report_unsupported_udvt_operator(&self, span: Span) -> Option<ValueId> {
        self.cx
            .gcx
            .dcx()
            .err("user-defined operators are not supported in this codegen path")
            .span(span)
            .help("unwrap the user-defined value type before using this operator")
            .emit();
        None
    }
}

fn int_literal_expr_contains_wide(gcx: Gcx<'_>, expr: &hir::Expr<'_>) -> Option<bool> {
    let is_wide = |expr| gcx.try_eval_const(expr).is_ok_and(|value| value.bit_len() > 256);
    match &expr.kind {
        ExprKind::Lit(lit) if matches!(lit.kind, LitKind::Number(_)) => Some(false),
        ExprKind::Unary(op, inner) if matches!(op.kind, UnOpKind::Neg | UnOpKind::BitNot) => {
            Some(is_wide(expr) || int_literal_expr_contains_wide(gcx, inner)?)
        }
        ExprKind::Binary(lhs, op, rhs)
            if !op.kind.is_cmp() && !matches!(op.kind, BinOpKind::Or | BinOpKind::And) =>
        {
            Some(
                is_wide(expr)
                    || int_literal_expr_contains_wide(gcx, lhs)?
                    || int_literal_expr_contains_wide(gcx, rhs)?,
            )
        }
        ExprKind::Tuple([Some(inner)]) => int_literal_expr_contains_wide(gcx, inner),
        _ => None,
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
        .map(|(id, function)| (InternalFunctionPointerShape::from_function(function), id))
        .collect::<Vec<_>>();
    for (shape, dispatcher) in dispatchers {
        let mut candidates = state
            .pointer_registry
            .targets
            .iter()
            .filter_map(|&function_id| {
                let TyKind::Fn(function) = gcx.type_of_item(function_id.into()).kind else {
                    return None;
                };
                let candidate_shape = InternalFunctionPointerShape::from_ty(function);
                shape
                    .is_assembly_cast_compatible_with(&candidate_shape)
                    .then_some((function_id, candidate_shape))
            })
            .filter_map(|(function_id, candidate_shape)| {
                function_ids
                    .get(&function_id)
                    .copied()
                    .map(|mir_id| (function_id, mir_id, candidate_shape))
            })
            .collect::<Vec<_>>();
        candidates.sort_by_key(|(function_id, _, _)| function_id.index());

        let reserved = module.function(dispatcher);
        let mut function = Function::new(Ident::new(reserved.name.symbol, reserved.name_span));
        function.name = reserved.name;
        function.attributes.is_function_pointer_dispatcher = true;
        {
            let mut builder = FunctionBuilder::new(&mut function);
            let function_value = builder.add_param(MirType::Function);
            let arguments =
                shape.params.iter().copied().map(|ty| builder.add_param(ty)).collect::<Vec<_>>();
            for ty in shape.returns.iter().copied() {
                builder.add_return(ty);
            }

            // for target {
            //     if function_id == target {
            //         results = icall(target, arguments)
            //         return results
            //     }
            // }
            for (function_id, mir_id, candidate_shape) in candidates {
                let case_block = builder.create_block();
                let next_block = builder.create_block();
                let id = builder.imm(internal_function_pointer_id(function_id));
                let is_match = builder.eq(function_value, id);
                builder.branch(is_match, case_block, next_block);

                builder.switch_to_block(case_block);
                let call_arguments = arguments
                    .iter()
                    .copied()
                    .zip(&shape.params)
                    .zip(&candidate_shape.params)
                    .map(|((argument, &source), &target)| {
                        if source == target {
                            argument
                        } else {
                            AbiWordValidator::from_mir_type(target).map_or(argument, |validator| {
                                validator.cleanup(&mut builder, argument)
                            })
                        }
                    })
                    .collect::<Vec<_>>();
                if shape.returns.is_empty() {
                    builder.icall_void(mir_id, call_arguments, 0);
                    builder.ret([]);
                } else {
                    let result = builder.icall(
                        mir_id,
                        call_arguments,
                        shape.returns[0],
                        shape.returns.len(),
                    );
                    let mut values = Vec::with_capacity(shape.returns.len());
                    values.push(result);
                    if shape.returns.len() > 1 {
                        let base =
                            builder.frame_load(0, FrameMode::MultiReturn, FrameSlotKind::Word);
                        let mut word_index =
                            if matches!(shape.returns[0], MirType::Slice(_)) { 2 } else { 1 };
                        for &ty in &shape.returns[1..] {
                            let offset = u64::try_from(word_index)
                                .unwrap_or(u64::MAX)
                                .saturating_mul(EvmMemoryLayout::WORD_SIZE);
                            let position = builder.add_u64_offset(base, offset);
                            let first_word = builder.mload(position);
                            let value = if let MirType::Slice(location) = ty {
                                let length_position =
                                    builder.add_u64_offset(position, EvmMemoryLayout::WORD_SIZE);
                                let length = builder.mload(length_position);
                                word_index += 2;
                                builder.make_slice(first_word, length, location)
                            } else {
                                word_index += 1;
                                first_word
                            };
                            values.push(value);
                        }
                    }
                    builder.ret(values);
                }
                builder.switch_to_block(next_block);
            }

            // panic(InvalidInternalFunction)
            builder.panic(PanicCode::InvalidInternalFunction);
        }
        *module.function_mut(dispatcher) = function;
    }
}

fn arithmetic_kind(ty: Ty<'_>) -> Option<ArithmeticKind> {
    match ty.peel_refs().kind {
        TyKind::Udvt(inner, _) => arithmetic_kind(inner),
        TyKind::Elementary(elementary) => match elementary {
            solar_sema::hir::ElementaryType::UInt(size)
            | solar_sema::hir::ElementaryType::UFixed(size, _) => {
                Some(ArithmeticKind::Unsigned(size.bits()))
            }
            solar_sema::hir::ElementaryType::Int(size)
            | solar_sema::hir::ElementaryType::Fixed(size, _) => {
                Some(ArithmeticKind::Signed(size.bits()))
            }
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

fn signed_bounds(bits: u16, builder: &mut FunctionBuilder<'_>) -> (ValueId, ValueId) {
    let magnitude = U256::from(1) << (bits - 1);
    let min = builder.imm(U256::MAX - magnitude + U256::ONE);
    let max = builder.imm(magnitude - U256::ONE);
    (min, max)
}

fn report_error<T>(gcx: Gcx<'_>, span: Span, message: &'static str) -> Option<T> {
    gcx.dcx().err(message).span(span).emit();
    None
}
