//! Function-level HIR to MIR lowering.

use super::{
    contract,
    storage::{StorageLayout, StorageLocation},
    types,
};
use crate::{
    memory::EvmMemoryLayout,
    mir::{
        AbiLayout, AbiParamLayout, AbiParamLocation, AbiParamType, AbiType, AllocationSemantics,
        BlockId, FrameMode, FrameSlotKind, Function, FunctionBuilder, FunctionId, ImmutableId,
        InstKind, MemoryObjectKind, MemoryObjectLayout, MirType, Module, SliceLocation, Value,
        ValueId,
    },
};
use alloy_primitives::{Bytes, U256, keccak256};
use solar_ast::{BinOpKind, DataLocation, LitKind, UnOpKind};
use solar_data_structures::map::{FxHashMap, FxHashSet, StdEntry};
use solar_interface::{Ident, Span, kw, sym};
use solar_sema::{
    Gcx,
    builtins::Builtin,
    eval::ConstValue,
    hir::{self, ExprKind, LoopSource, StmtKind, VariableId},
    ty::{CallableParamSource, Ty, TyKind},
};
use std::sync::Arc;

mod abi_calls;
mod abi_values;
mod builtins;
mod calls;
mod checks;
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
pub(super) struct LoweringContext<'gcx, 'mir, 'ids, 'bytes, 'events, 'module, 'pointers> {
    pub(super) gcx: Gcx<'gcx>,
    pub(super) module: &'module mut Module,
    pub(super) storage: &'mir StorageLayout<'gcx>,
    pub(super) contract_id: hir::ContractId,
    pub(super) function_ids: &'ids FxHashMap<hir::FunctionId, FunctionId>,
    pub(super) immutable_ids: &'ids FxHashMap<VariableId, ImmutableId>,
    pub(super) child_bytecodes: &'bytes FxHashMap<hir::ContractId, Bytes>,
    pub(super) child_runtime_bytecodes: &'bytes FxHashMap<hir::ContractId, Bytes>,
    pub(super) invalid_event_topics: &'events mut FxHashSet<hir::EventId>,
    pub(super) pointer_registry: &'pointers mut InternalFunctionPointerRegistry,
}

/// Lowers one HIR function into a typed MIR function.
pub(super) fn lower(
    context: LoweringContext<'_, '_, '_, '_, '_, '_, '_>,
    id: hir::FunctionId,
    expose_selector: bool,
) -> Option<Function> {
    let LoweringContext {
        gcx,
        module,
        storage,
        contract_id,
        function_ids,
        immutable_ids,
        child_bytecodes,
        child_runtime_bytecodes,
        invalid_event_topics,
        pointer_registry,
    } = context;
    let hir_function = gcx.hir.function(id);
    let mut mir = contract::declaration(gcx, id, hir_function);
    if !expose_selector {
        mir.selector = None;
    }
    let mut type_lowerer = types::TypeLowerer::new(gcx);

    let input_shapes = hir_function
        .parameters
        .iter()
        .map(|&param| type_lowerer.abi_param_type(gcx.type_of_item(param.into())))
        .collect::<Option<Vec<_>>>();
    let output_shapes = hir_function
        .returns
        .iter()
        .map(|&ret| type_lowerer.abi_return_type(gcx.type_of_item(ret.into())))
        .collect::<Option<Vec<_>>>();
    let output_param_shapes = hir_function
        .returns
        .iter()
        .map(|&ret| type_lowerer.abi_return_param_type(gcx.type_of_item(ret.into())))
        .collect::<Option<Vec<_>>>();

    let has_constructor_params = mir.attributes.is_constructor
        && input_shapes.as_ref().is_some_and(|shapes| !shapes.is_empty());
    if mir.selector.is_some() || has_constructor_params {
        let Some(input_shapes) = input_shapes else {
            return report_unsupported(gcx, hir_function.span, "function parameter shape");
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
        mir.abi_args_lazy = true;
        if mir.selector.is_some() {
            let Some(output_shapes) = output_shapes else {
                return report_unsupported(gcx, hir_function.span, "function return shape");
            };
            let Some(output_param_shapes) = output_param_shapes else {
                return report_unsupported(gcx, hir_function.span, "function return ABI shape");
            };
            mir.abi_returns =
                Some(module.intern_abi_layout(AbiLayout::new(output_shapes.into_boxed_slice())));
            if output_param_shapes.iter().any(AbiParamType::needs_nested_return_cleanup) {
                mir.abi_return_params =
                    Some(AbiParamLayout::new(output_param_shapes.into_boxed_slice()));
            }
        }
    }

    let mut lowerer = FunctionLowerer::new(
        LoweringContext {
            gcx,
            module,
            storage,
            contract_id,
            function_ids,
            immutable_ids,
            child_bytecodes,
            child_runtime_bytecodes,
            invalid_event_topics,
            pointer_registry,
        },
        &mut mir,
    );
    lowerer.bind_signature(hir_function);
    if hir_function.kind == hir::FunctionKind::Constructor {
        let Some(contract_id) = hir_function.contract else {
            return report_unsupported(gcx, hir_function.span, "free constructor");
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
    context: LoweringContext<'_, '_, '_, '_, '_, '_, '_>,
    contract_id: hir::ContractId,
) -> Option<Function> {
    let LoweringContext {
        gcx,
        module,
        storage,
        function_ids,
        immutable_ids,
        child_bytecodes,
        child_runtime_bytecodes,
        invalid_event_topics,
        pointer_registry,
        ..
    } = context;
    let mut mir =
        Function::new(solar_interface::Ident::with_dummy_span(solar_interface::kw::Constructor));
    mir.attributes.is_constructor = true;
    let mut lowerer = FunctionLowerer::new(
        LoweringContext {
            gcx,
            module,
            storage,
            contract_id,
            function_ids,
            immutable_ids,
            child_bytecodes,
            child_runtime_bytecodes,
            invalid_event_topics,
            pointer_registry,
        },
        &mut mir,
    );
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
struct FunctionLowerer<'gcx, 'mir, 'ids, 'bytes, 'events, 'module, 'pointers> {
    gcx: Gcx<'gcx>,
    module: &'module mut Module,
    storage: &'mir StorageLayout<'gcx>,
    contract_id: hir::ContractId,
    function_ids: &'ids FxHashMap<hir::FunctionId, FunctionId>,
    immutable_ids: &'ids FxHashMap<VariableId, ImmutableId>,
    child_bytecodes: &'bytes FxHashMap<hir::ContractId, Bytes>,
    child_runtime_bytecodes: &'bytes FxHashMap<hir::ContractId, Bytes>,
    invalid_event_topics: &'events mut FxHashSet<hir::EventId>,
    pointer_registry: &'pointers mut InternalFunctionPointerRegistry,
    builder: FunctionBuilder<'mir>,
    types: types::TypeLowerer<'gcx>,
    values: FxHashMap<VariableId, ValueId>,
    storage_refs: FxHashMap<VariableId, StorageAccess>,
    returns: Vec<VariableId>,
    loops: Vec<LoopTargets>,
    modifiers: Vec<ModifierContext<'gcx>>,
    return_targets: Vec<ReturnTarget>,
    unchecked: bool,
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

#[derive(Clone, Copy)]
struct ModifierContext<'gcx> {
    modifiers: &'gcx [hir::Modifier<'gcx>],
    body: hir::Block<'gcx>,
    next: usize,
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
    MemoryField { object: ValueId, layout: MemoryObjectLayout, field: u64, ty: Ty<'gcx> },
    MemoryElement { object: ValueId, layout: MemoryObjectLayout, index: ValueId, ty: Ty<'gcx> },
    MemoryByte { object: ValueId, index: ValueId, ty: Ty<'gcx> },
    StorageByte { slot: ValueId, object: ValueId, index: ValueId, ty: Ty<'gcx> },
}

enum PackedPiece<'gcx> {
    Bytes(Vec<u8>),
    Static {
        value: ValueId,
        length: u64,
        fixed_bytes: bool,
    },
    Dynamic {
        source: ValueId,
        length: ValueId,
    },
    Array {
        value: ValueId,
        length: ValueId,
        element: PackedArrayElement<'gcx>,
        source: PackedArraySource,
    },
}

struct PackedArrayElement<'gcx> {
    abi: AbiType,
    ty: Ty<'gcx>,
}

#[derive(Clone, Copy)]
enum PackedArraySource {
    Memory { layout: MemoryObjectLayout },
    Slice(SliceLocation),
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

#[derive(Default)]
pub(super) struct InternalFunctionPointerRegistry {
    targets: FxHashSet<hir::FunctionId>,
    dispatchers: FxHashMap<InternalFunctionPointerShape, FunctionId>,
}

fn internal_function_pointer_id(function_id: hir::FunctionId) -> u64 {
    function_id.index() as u64 + 1
}

impl<'gcx, 'mir, 'ids, 'bytes, 'events, 'module, 'pointers>
    FunctionLowerer<'gcx, 'mir, 'ids, 'bytes, 'events, 'module, 'pointers>
{
    fn new(
        context: LoweringContext<'gcx, 'mir, 'ids, 'bytes, 'events, 'module, 'pointers>,
        function: &'mir mut Function,
    ) -> Self {
        let LoweringContext {
            gcx,
            module,
            storage,
            contract_id,
            function_ids,
            immutable_ids,
            child_bytecodes,
            child_runtime_bytecodes,
            invalid_event_topics,
            pointer_registry,
        } = context;
        Self {
            gcx,
            module,
            storage,
            contract_id,
            function_ids,
            immutable_ids,
            child_bytecodes,
            child_runtime_bytecodes,
            invalid_event_topics,
            pointer_registry,
            builder: FunctionBuilder::new(function),
            types: types::TypeLowerer::new(gcx),
            values: FxHashMap::default(),
            storage_refs: FxHashMap::default(),
            returns: Vec::new(),
            loops: Vec::new(),
            modifiers: Vec::new(),
            return_targets: Vec::new(),
            unchecked: false,
        }
    }

    fn lower_word_value(&mut self, ty: Ty<'gcx>, expr: &hir::Expr<'_>, value: ValueId) -> ValueId {
        let expr = expr.peel_parens();
        if let TyKind::Fn(function) = ty.peel_refs().kind
            && function.is_external()
        {
            let shift = self.builder.imm_u64(64);
            return self.builder.shl(shift, value);
        }
        if !matches!(
            ty.peel_refs().kind,
            TyKind::Elementary(solar_sema::hir::ElementaryType::FixedBytes(_))
        ) {
            return value;
        }
        if let ExprKind::Lit(lit) = &expr.kind
            && let LitKind::Str(_, bytes, _) = &lit.kind
        {
            let bytes = bytes.as_byte_str();
            return self.builder.imm_u256(U256::from_be_slice(bytes) << ((32 - bytes.len()) * 8));
        }
        if matches!(
            self.builder.func().value_ty(value),
            Some(MirType::MemoryObject(MemoryObjectKind::Bytes))
        ) {
            let zero = self.builder.imm_u64(0);
            return self.builder.memory_object_load_element(value, MemoryObjectLayout::Bytes, zero);
        }
        value
    }

    fn is_low_level_call_expr(&self, expr: &hir::Expr<'_>) -> bool {
        let ExprKind::Call(callee, ..) = &expr.kind else { return false };
        matches!(
            self.gcx.resolved_builtin(callee),
            Some(Builtin::AddressCall | Builtin::AddressStaticcall | Builtin::AddressDelegatecall)
        ) && matches!(callee.kind, ExprKind::Member(..))
    }

    fn lower_low_level_call_values(
        &mut self,
        expr: &hir::Expr<'_>,
        count: usize,
        first_is_omitted: bool,
    ) -> Option<Vec<ValueId>> {
        let ExprKind::Call(callee, args, call_opts) = &expr.kind else { return None };
        let builtin = self.gcx.resolved_builtin(callee)?;
        if !matches!(
            builtin,
            Builtin::AddressCall | Builtin::AddressStaticcall | Builtin::AddressDelegatecall
        ) {
            return None;
        }
        let ExprKind::Member(receiver, _) = callee.kind else { return None };
        let capture_returndata = count > 1 || first_is_omitted;
        let (success, returndata) = self.lower_address_call_result(
            callee.span,
            receiver,
            builtin,
            *args,
            *call_opts,
            capture_returndata,
        )?;
        if count <= 1 && !first_is_omitted {
            return Some(vec![success]);
        }
        if count != 2 {
            let Some(returndata) = returndata else {
                return report_unsupported(self.gcx, expr.span, "low-level call return values");
            };
            return Some(vec![returndata]);
        }
        let Some(returndata) = returndata else {
            return report_unsupported(self.gcx, expr.span, "low-level call return values");
        };
        Some(vec![success, returndata])
    }

    fn lower_expr(&mut self, expr: &hir::Expr<'_>) -> Option<ValueId> {
        match &expr.kind {
            ExprKind::Lit(lit) => self.lower_literal(lit.kind, expr.span),
            ExprKind::Array(elements) => self.lower_array(expr, elements),
            ExprKind::Ident(_) => {
                if let Some(builtin) = self.gcx.resolved_builtin(expr) {
                    return self.lower_environment_builtin(expr, builtin);
                }
                if let Some(value) = self.lower_internal_function_value(expr) {
                    return Some(value);
                }
                let id = self.gcx.resolved_variable(expr)?;
                self.load_variable(id, expr.span)
            }
            ExprKind::Binary(lhs, op, rhs) => {
                if matches!(op.kind, BinOpKind::And | BinOpKind::Or) {
                    return self.lower_logical(lhs, op.kind, rhs);
                }
                let lhs_ty = self.gcx.type_of_expr(lhs.id);
                let lhs = self.lower_expr(lhs)?;
                let rhs_ty = self.gcx.type_of_expr(rhs.id);
                let rhs = self.lower_expr(rhs)?;
                let (lhs, rhs) = match (lhs_ty, rhs_ty) {
                    (Some(lhs_ty), Some(rhs_ty))
                        if matches!(
                            lhs_ty.peel_refs().kind,
                            TyKind::Elementary(solar_sema::hir::ElementaryType::FixedBytes(_))
                        ) && matches!(rhs_ty.peel_refs().kind, TyKind::StringLiteral(..)) =>
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
                let ty = match op.kind {
                    BinOpKind::Lt
                    | BinOpKind::Gt
                    | BinOpKind::Le
                    | BinOpKind::Ge
                    | BinOpKind::Shl
                    | BinOpKind::Shr
                    | BinOpKind::Sar => lhs_ty,
                    _ => self.gcx.type_of_expr(expr.id),
                };
                Some(self.binary(op.kind, lhs, rhs, ty))
            }
            ExprKind::Call(callee, args, call_opts) => {
                self.lower_call(expr, callee, *args, *call_opts)
            }
            ExprKind::Delete(value) => {
                self.delete_lvalue(value)?;
                Some(self.builder.imm_u256(U256::ZERO))
            }
            ExprKind::Unary(op, value) => {
                if matches!(
                    op.kind,
                    UnOpKind::PreInc | UnOpKind::PostInc | UnOpKind::PreDec | UnOpKind::PostDec
                ) {
                    let place = self.resolve_lvalue_place(value)?;
                    let old = self.load_lvalue_place(&place)?;
                    let one = self.builder.imm_u256(U256::from(1));
                    let kind = if matches!(op.kind, UnOpKind::PreInc | UnOpKind::PostInc) {
                        BinOpKind::Add
                    } else {
                        BinOpKind::Sub
                    };
                    let new = self.binary(kind, old, one, self.gcx.type_of_expr(value.id));
                    self.store_lvalue_place(&place, new)?;
                    return Some(if matches!(op.kind, UnOpKind::PreInc | UnOpKind::PreDec) {
                        new
                    } else {
                        old
                    });
                }
                let value = self.lower_expr(value)?;
                self.unary(op.kind, value, expr.span, self.gcx.type_of_expr(expr.id))
            }
            ExprKind::Assign(lhs, op, rhs) => {
                if op.is_none()
                    && let ExprKind::Tuple(elements) = &lhs.peel_parens().kind
                {
                    self.lower_tuple_assignment(elements, rhs)?;
                    return Some(self.builder.imm_u256(U256::ZERO));
                }
                if op.is_none() && self.is_storage_reference_binding(lhs) {
                    let access = self.storage_access(rhs)?;
                    let Some(id) = self.gcx.resolved_variable(lhs) else {
                        return report_unsupported(self.gcx, lhs.span, "storage reference target");
                    };
                    self.storage_refs.insert(id, access);
                    return Some(self.builder.imm_u256(U256::ZERO));
                }
                let lhs_ty = self.type_of_expr_or_variable(lhs)?;
                let rhs_ty = self.gcx.type_of_expr(rhs.id).unwrap_or(lhs_ty);
                let memory_rhs_ty = rhs_ty.with_loc_if_ref(self.gcx, DataLocation::Memory);
                let rhs_value = if self.types.memory_layout(memory_rhs_ty).is_some()
                    && rhs_ty.is_ref_at(DataLocation::Storage)
                {
                    self.lower_typed_expr(rhs, memory_rhs_ty)?
                } else {
                    self.lower_expr(rhs)?
                };
                if let Some(kind) = op.map(|op| op.kind) {
                    let place = self.resolve_lvalue_place(lhs)?;
                    let lhs_value = self.load_lvalue_place(&place)?;
                    let value = self.binary(kind, lhs_value, rhs_value, Some(lhs_ty));
                    let value = self.coerce_value(value, rhs_ty, lhs_ty);
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
                    self.materialize_memory_argument(lhs_ty, rhs_value, rhs.span)?
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
            _ if self.gcx.dcx().has_errors().is_err() => Some(self.builder.imm_u256(U256::ZERO)),
            _ => report_unsupported(self.gcx, expr.span, "expression"),
        }
    }
}

pub(super) fn generate_internal_function_pointer_dispatchers(
    gcx: Gcx<'_>,
    module: &mut Module,
    function_ids: &FxHashMap<hir::FunctionId, FunctionId>,
    registry: &InternalFunctionPointerRegistry,
) {
    let dispatchers = registry
        .dispatchers
        .iter()
        .map(|(shape, &dispatcher)| (shape.clone(), dispatcher))
        .collect::<Vec<_>>();
    for (shape, dispatcher) in dispatchers {
        let mut candidates = registry
            .targets
            .iter()
            .filter_map(|&function_id| {
                let TyKind::Fn(function) = gcx.type_of_item(function_id.into()).kind else {
                    return None;
                };
                let candidate_shape = InternalFunctionPointerShape {
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
                };
                (candidate_shape == shape).then_some(function_id)
            })
            .filter_map(|function_id| {
                function_ids.get(&function_id).copied().map(|mir_id| (function_id, mir_id))
            })
            .collect::<Vec<_>>();
        candidates.sort_by_key(|(function_id, _)| function_id.index());

        let reserved = module.function(dispatcher);
        let name = reserved.name;
        let mut function = Function::new(Ident::new(name.symbol, reserved.name_span));
        function.name = name;
        function.attributes.is_function_pointer_dispatcher = true;
        {
            let mut builder = FunctionBuilder::new(&mut function);
            let function_value = builder.add_param(MirType::Function);
            let arguments =
                shape.params.iter().copied().map(|ty| builder.add_param(ty)).collect::<Vec<_>>();
            for ty in shape.returns.iter().copied() {
                builder.add_return(ty);
            }

            for (function_id, mir_id) in candidates {
                let case_block = builder.create_block();
                let next_block = builder.create_block();
                let id = builder.imm_u64(internal_function_pointer_id(function_id));
                let is_match = builder.eq(function_value, id);
                builder.branch(is_match, case_block, next_block);

                builder.switch_to_block(case_block);
                if shape.returns.is_empty() {
                    builder.internal_call_void(mir_id, arguments.clone(), 0);
                    builder.ret([]);
                } else {
                    let result = builder.internal_call(
                        mir_id,
                        arguments.clone(),
                        shape.returns[0],
                        shape.returns.len(),
                    );
                    let mut values = Vec::with_capacity(shape.returns.len());
                    values.push(result);
                    if shape.returns.len() > 1 {
                        let base =
                            builder.frame_load(0, FrameMode::MultiReturn, FrameSlotKind::Word);
                        let size = builder.imm_u64(
                            u64::try_from(shape.returns.len())
                                .unwrap_or(u64::MAX)
                                .saturating_mul(EvmMemoryLayout::WORD_SIZE),
                        );
                        let slice = builder.make_slice(base, size, SliceLocation::Memory);
                        for index in 1..shape.returns.len() {
                            let offset = builder.imm_u64(
                                u64::try_from(index).unwrap_or(u64::MAX).saturating_mul(32),
                            );
                            values.push(builder.memory_slice_load_word(slice, offset));
                        }
                    }
                    builder.ret(values);
                }
                builder.switch_to_block(next_block);
            }

            let zero = builder.imm_u256(U256::ZERO);
            let selector = builder.imm_u256(U256::from(0x4e48_7b71_u64) << 224);
            builder.mstore(zero, selector);
            let four = builder.imm_u256(U256::from(4));
            let code = builder.imm_u256(U256::from(0x51));
            builder.mstore(four, code);
            let size = builder.imm_u256(U256::from(36));
            builder.revert(zero, size);
        }
        *module.function_mut(dispatcher) = function;
    }
}

fn report_unsupported<T>(gcx: Gcx<'_>, span: Span, what: &str) -> Option<T> {
    gcx.dcx().err(format!("codegen rewrite does not support this {what} yet")).span(span).emit();
    None
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

fn signed_bounds(bits: u16, builder: &mut FunctionBuilder<'_>) -> (ValueId, ValueId) {
    let magnitude = U256::from(1) << (bits - 1);
    let min = builder.imm_u256(U256::MAX - magnitude + U256::ONE);
    let max = builder.imm_u256(magnitude - U256::ONE);
    (min, max)
}

fn report_error<T>(gcx: Gcx<'_>, span: Span, message: &'static str) -> Option<T> {
    gcx.dcx().err(message).span(span).emit();
    None
}
