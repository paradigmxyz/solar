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
mod indexing;
mod lvalues;
mod memory_values;
mod modifiers;
mod statements;
mod storage_values;

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

    fn bind_signature(&mut self, function: &hir::Function<'_>) {
        for &param in function.parameters {
            let value = self
                .builder
                .add_param(types::TypeLowerer::mir_type(self.gcx.type_of_item(param.into())));
            let ty = self.gcx.type_of_item(param.into());
            if ty.is_ref_at(DataLocation::Storage) {
                self.storage_refs.insert(
                    param,
                    StorageAccess {
                        slot: value,
                        location: StorageLocation::word(U256::ZERO),
                        offset: None,
                    },
                );
            } else {
                self.values.insert(param, value);
            }
        }
        for &ret in function.returns {
            self.builder
                .add_return(types::TypeLowerer::mir_return_type(self.gcx.type_of_item(ret.into())));
            let ty = self.gcx.type_of_item(ret.into());
            if ty.is_ref_at(DataLocation::Storage) {
                let zero = self.builder.imm_u256(U256::ZERO);
                self.storage_refs.insert(
                    ret,
                    StorageAccess {
                        slot: zero,
                        location: StorageLocation::word(U256::ZERO),
                        offset: None,
                    },
                );
            } else {
                let value = self.default_binding_value(ty);
                self.values.insert(ret, value);
            }
        }
        self.returns.extend_from_slice(function.returns);
    }

    /// Lowers only one contract's own state initializers.
    fn lower_state_initializers(&mut self, contract_id: hir::ContractId) -> Option<()> {
        let contract = self.gcx.hir.contract(contract_id);
        for id in contract.variables() {
            let variable = self.gcx.hir.variable(id);
            if !variable.is_state_variable() || variable.is_constant() {
                continue;
            }
            let Some(initializer) = variable.initializer else { continue };
            let ty = self.gcx.type_of_item(id.into());
            let source_ty = self.gcx.type_of_expr(initializer.id)?;
            let value = self.lower_typed_expr(initializer, ty)?;
            let value = self.coerce_value(value, source_ty, ty);
            if let Some(&immutable_id) = self.immutable_ids.get(&id) {
                self.builder.store_immutable(immutable_id, value);
            } else {
                self.store_state_variable(id, value, source_ty, initializer.span)?;
            }
        }
        Some(())
    }

    fn lower_implicit_base_constructors(&mut self, contract_id: hir::ContractId) -> Option<()> {
        let mut lowered = FxHashSet::default();
        self.lower_implicit_base_constructors_inner(contract_id, &mut lowered)
    }

    fn lower_implicit_base_constructors_inner(
        &mut self,
        contract_id: hir::ContractId,
        lowered: &mut FxHashSet<hir::ContractId>,
    ) -> Option<()> {
        let bases = self.gcx.hir.contract(contract_id).linearized_bases;
        for (index, &base_id) in bases.iter().skip(1).enumerate() {
            if lowered.contains(&base_id) {
                continue;
            }
            let Some(constructor_id) = self.gcx.hir.contract(base_id).ctor else {
                self.lower_implicit_base_constructors_inner(base_id, lowered)?;
                self.lower_state_initializers(base_id)?;
                lowered.insert(base_id);
                continue;
            };
            let constructor = self.gcx.hir.function(constructor_id);
            let Some(args) = self
                .base_constructor_args(contract_id, base_id, index)
                .or_else(|| constructor.parameters.is_empty().then(hir::CallArgs::default))
            else {
                continue;
            };
            let modifier = hir::Modifier {
                span: constructor.span,
                name_span: constructor.span,
                id: hir::ItemId::Contract(base_id),
                args,
            };
            lowered.insert(base_id);
            self.lower_base_constructor(&modifier, constructor_id, constructor, lowered)?;
        }
        Some(())
    }

    fn base_constructor_args(
        &self,
        contract_id: hir::ContractId,
        base_id: hir::ContractId,
        index: usize,
    ) -> Option<hir::CallArgs<'gcx>> {
        let contract = self.gcx.hir.contract(contract_id);
        if let Some(modifier) = contract.linearized_bases_args.get(index).copied().flatten() {
            return Some(modifier.args);
        }

        for &ancestor_id in contract.linearized_bases.iter().skip(1) {
            let ancestor = self.gcx.hir.contract(ancestor_id);
            let Some(ancestor_index) =
                ancestor.linearized_bases.iter().skip(1).position(|&id| id == base_id)
            else {
                continue;
            };
            if let Some(modifier) =
                ancestor.linearized_bases_args.get(ancestor_index).copied().flatten()
            {
                return Some(modifier.args);
            }
        }
        None
    }

    fn finish(&mut self, returns: &[VariableId]) -> Option<()> {
        if returns.is_empty() {
            self.builder.stop();
        } else {
            let mut values = Vec::with_capacity(returns.len());
            for &id in returns {
                let ty = self.gcx.type_of_item(id.into());
                let value = if ty.is_ref_at(DataLocation::Storage) {
                    self.storage_refs.get(&id).copied()?.slot
                } else {
                    self.values.get(&id).copied()?
                };
                values.push(self.materialize_memory_argument(
                    ty,
                    value,
                    self.gcx.hir.variable(id).span,
                )?);
            }
            self.builder.ret(values);
        }
        Some(())
    }

    fn is_terminated(&self) -> bool {
        self.builder.func().block(self.builder.current_block()).terminator.is_some()
    }

    fn push_return_target(&mut self, block: BlockId) {
        self.return_targets.push(ReturnTarget { block, states: Vec::new() });
    }

    fn record_return_state(&mut self) {
        let state = LoopState {
            block: self.builder.current_block(),
            values: self.values.clone(),
            storage_refs: self.storage_refs.clone(),
        };
        if let Some(target) = self.return_targets.last_mut() {
            target.states.push(state);
        }
    }

    fn finish_return_target(
        &mut self,
        before_values: FxHashMap<VariableId, ValueId>,
        before_storage_refs: FxHashMap<VariableId, StorageAccess>,
    ) {
        let target = self.return_targets.pop().expect("return target exists");
        self.builder.switch_to_block(target.block);
        self.values = self.merge_many_values(before_values, &target.states);
        self.storage_refs = self.merge_many_storage_refs(before_storage_refs, &target.states);
    }

    fn lower_function_body(
        &mut self,
        modifiers: &'gcx [hir::Modifier<'gcx>],
        body: hir::Block<'gcx>,
    ) -> Option<()> {
        if modifiers.is_empty() {
            self.lower_block(body)
        } else {
            let return_block = self.builder.create_block();
            let before_values = self.values.clone();
            let before_storage_refs = self.storage_refs.clone();
            self.push_return_target(return_block);
            let result = self.lower_modifier_chain(modifiers, body);
            result?;
            if !self.is_terminated() {
                self.record_return_state();
                self.builder.jump(return_block);
            }
            self.finish_return_target(before_values, before_storage_refs);
            Some(())
        }
    }

    fn lower_block(&mut self, block: hir::Block<'_>) -> Option<()> {
        for stmt in block.stmts {
            if self.is_terminated() {
                break;
            }
            self.lower_stmt(stmt)?;
        }
        Some(())
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

    fn lower_values(&mut self, expr: &hir::Expr<'_>) -> Option<Vec<ValueId>> {
        if let ExprKind::Ternary(condition, then_expr, else_expr) = &expr.kind {
            return self.lower_ternary_values(condition, then_expr, else_expr);
        }
        if let ExprKind::Call(callee, args, call_opts) = &expr.kind {
            if self.gcx.resolved_builtin(callee) == Some(Builtin::AbiDecode)
                && let ExprKind::Call(_, args, _) = &expr.kind
                && let Some(types) = args.exprs().nth(1)
                && let ExprKind::Tuple(elements) = types.kind
                && elements.len() > 1
            {
                let first = self.lower_expr(expr)?;
                let base = self.multi_return_buffer_base();
                let mut values = Vec::with_capacity(elements.len());
                values.push(first);
                for index in 1..elements.len() {
                    values.push(self.load_multi_return_value(base, index, elements.len()));
                }
                return Some(values);
            }
            let returns = self
                .gcx
                .resolved_function(callee)
                .map(|function_id| self.gcx.hir.function(function_id).returns.len())
                .or_else(|| {
                    self.gcx.type_of_expr(callee.id).and_then(|ty| match ty.kind {
                        TyKind::Fn(function)
                            if function.function_id.is_none()
                                && (function.is_internal() || function.is_external()) =>
                        {
                            Some(function.returns.len())
                        }
                        _ => None,
                    })
                });
            if let Some(TyKind::Fn(function)) = self.gcx.type_of_expr(callee.id).map(|ty| ty.kind)
                && function.is_external()
                && function.function_id.is_none()
            {
                return self.lower_external_function_pointer_call_values(
                    callee, function, *args, *call_opts,
                );
            }
            if let Some(returns) = returns
                && returns > 1
            {
                let first = self.lower_expr(expr)?;
                let base = self.multi_return_buffer_base();
                let mut values = Vec::with_capacity(returns);
                values.push(first);
                for index in 1..returns {
                    values.push(self.load_multi_return_value(base, index, returns));
                }
                return Some(values);
            }
            let returns_empty = returns.is_some_and(|returns| returns == 0)
                || self.gcx.resolved_builtin(callee).is_some_and(|builtin| {
                    matches!(builtin, Builtin::Assert | Builtin::Revert | Builtin::RevertMsg)
                });
            if returns_empty {
                self.lower_expr(expr)?;
                return Some(Vec::new());
            }
        }
        match &expr.kind {
            ExprKind::Tuple(values) => {
                values.iter().flatten().map(|expr| self.lower_expr(expr)).collect()
            }
            _ => Some(vec![self.lower_expr(expr)?]),
        }
    }

    fn lower_return_values(&mut self, expr: &hir::Expr<'_>) -> Option<Vec<ValueId>> {
        if self.returns.len() > 1
            && let ExprKind::Tuple(values) = &expr.peel_parens().kind
            && values.len() == self.returns.len()
        {
            let returns = self.returns.clone();
            return values
                .iter()
                .zip(returns)
                .map(|(value, id)| {
                    let value = (*value)?;
                    let ty = self.gcx.type_of_item(id.into());
                    if ty.is_ref_at(DataLocation::Storage) {
                        self.storage_access(value).map(|access| access.slot)
                    } else {
                        self.lower_typed_expr(value, ty)
                    }
                })
                .collect();
        }

        self.lower_values(expr)
    }

    fn lower_tuple_assignment(
        &mut self,
        elements: &[Option<&hir::Expr<'_>>],
        rhs: &hir::Expr<'_>,
    ) -> Option<()> {
        let rhs = rhs.peel_parens();
        if elements.iter().flatten().any(|element| self.is_storage_reference_binding(element))
            && let Some(values) = self.lower_storage_reference_call(rhs)
        {
            if values.len() != elements.len() {
                return report_unsupported(self.gcx, rhs.span, "storage reference tuple");
            }
            for (element, (value, access)) in elements.iter().zip(values) {
                let Some(element) = element else { continue };
                if let Some(access) = access {
                    if !self.is_storage_reference_binding(element) {
                        return report_unsupported(self.gcx, element.span, "mixed storage tuple");
                    }
                    let Some(id) = self.gcx.resolved_variable(element) else {
                        return report_unsupported(
                            self.gcx,
                            element.span,
                            "storage reference target",
                        );
                    };
                    self.storage_refs.insert(id, access);
                } else {
                    if self.is_storage_reference_binding(element) {
                        return report_unsupported(self.gcx, element.span, "mixed storage tuple");
                    }
                    self.store_lvalue(element, value)?;
                }
            }
            return Some(());
        }
        if let ExprKind::Tuple(rhs_elements) = &rhs.peel_parens().kind
            && rhs_elements.len() == elements.len()
            && elements.iter().flatten().any(|element| self.is_storage_reference_binding(element))
        {
            let tuple_span = rhs.span;
            let mut bindings = Vec::with_capacity(elements.len());
            for (lhs, rhs) in elements.iter().zip(rhs_elements.iter()) {
                let Some(rhs) = rhs else {
                    if lhs.is_some() {
                        return report_unsupported(self.gcx, tuple_span, "storage reference tuple");
                    }
                    continue;
                };
                let Some(lhs) = lhs else {
                    self.lower_expr(rhs)?;
                    continue;
                };
                if !self.is_storage_reference_binding(lhs) {
                    return report_unsupported(self.gcx, lhs.span, "mixed storage tuple");
                }
                let access = self.storage_access(rhs)?;
                let Some(id) = self.gcx.resolved_variable(lhs) else {
                    return report_unsupported(self.gcx, lhs.span, "storage reference target");
                };
                bindings.push((id, access));
            }
            for (id, access) in bindings {
                self.storage_refs.insert(id, access);
            }
            return Some(());
        }
        if self.is_low_level_call_expr(rhs) {
            let values = self.lower_low_level_call_values(
                rhs,
                elements.iter().flatten().count(),
                elements.first().is_some_and(Option::is_none),
            )?;
            if values.len() != elements.iter().flatten().count() {
                return report_unsupported(self.gcx, rhs.span, "tuple assignment arity");
            }
            for (element, value) in elements.iter().flatten().zip(values) {
                self.store_lvalue(element, value)?;
            }
            return Some(());
        }
        if let ExprKind::Tuple(rhs_elements) = &rhs.peel_parens().kind {
            if rhs_elements.len() < elements.len() {
                return report_unsupported(self.gcx, rhs.span, "tuple assignment arity");
            }
            let mut values = Vec::with_capacity(rhs_elements.len());
            for (index, value) in rhs_elements.iter().enumerate() {
                let Some(value) = value else {
                    if elements.get(index).is_some_and(Option::is_some) {
                        return report_unsupported(self.gcx, rhs.span, "tuple assignment value");
                    }
                    continue;
                };
                values.push((index, self.lower_expr(value)?));
            }
            for (index, value) in values {
                let Some(Some(element)) = elements.get(index) else { continue };
                self.store_lvalue(element, value)?;
            }
            return Some(());
        }
        let values = self.lower_values(rhs)?;
        if values.len() < elements.len() {
            return report_unsupported(self.gcx, rhs.span, "tuple assignment arity");
        }
        for (element, value) in elements.iter().zip(values) {
            if let Some(element) = element {
                self.store_lvalue(element, value)?;
            }
        }
        Some(())
    }

    fn lower_storage_reference_call(
        &mut self,
        expr: &hir::Expr<'_>,
    ) -> Option<Vec<(ValueId, Option<StorageAccess>)>> {
        let ExprKind::Call(callee, ..) = &expr.kind else { return None };
        let function_id = self.gcx.resolved_function(callee)?;
        let returns = self.gcx.hir.function(function_id).returns;
        if returns.is_empty() {
            return None;
        }
        let has_storage_return = returns
            .iter()
            .any(|&id| self.gcx.type_of_item(id.into()).is_ref_at(DataLocation::Storage));
        if !has_storage_return {
            return None;
        }
        let values = self.lower_values(expr)?;
        (values.len() == returns.len()).then(|| {
            values
                .into_iter()
                .zip(returns)
                .map(|(value, id)| {
                    let access =
                        self.gcx.type_of_item((*id).into()).is_ref_at(DataLocation::Storage).then(
                            || StorageAccess {
                                slot: value,
                                location: StorageLocation::word(U256::ZERO),
                                offset: None,
                            },
                        );
                    (value, access)
                })
                .collect()
        })
    }

    fn multi_return_buffer_base(&mut self) -> ValueId {
        self.builder.frame_load(0, FrameMode::MultiReturn, FrameSlotKind::Word)
    }

    fn ensure_multi_return_buffer(
        &mut self,
        words: usize,
    ) -> (ValueId, ValueId, MemoryObjectLayout) {
        debug_assert!(words > 1);
        // The published pointer has no capacity, so each producer gets a fresh object.
        let words = u64::try_from(words).unwrap_or(u64::MAX);
        let (object, layout) = self.builder.alloc_word_array(words, AllocationSemantics::INTERNAL);
        let base = self.builder.memory_object_data(object, MemoryObjectKind::FixedArray);
        self.builder.frame_store(0, FrameMode::MultiReturn, FrameSlotKind::Word, base);
        (object, base, layout)
    }

    fn load_multi_return_value(&mut self, base: ValueId, index: usize, words: usize) -> ValueId {
        let offset = self.builder.imm_u64(u64::try_from(index).unwrap_or(u64::MAX) * 32);
        let size =
            self.builder.imm_u64(u64::try_from(words).unwrap_or(u64::MAX).saturating_mul(32));
        let slice = self.builder.make_slice(base, size, SliceLocation::Memory);
        self.builder.memory_slice_load_word(slice, offset)
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

    fn lower_literal(&mut self, kind: LitKind<'_>, span: Span) -> Option<ValueId> {
        match kind {
            LitKind::Str(_, value, _) => self.lower_bytes_literal(value.as_byte_str(), span),
            LitKind::Number(value) => Some(self.builder.imm_u256(value)),
            LitKind::Bool(value) => Some(self.builder.imm_bool(value)),
            LitKind::Address(value) => {
                Some(self.builder.imm_u256(U256::from_be_slice(value.as_slice())))
            }
            LitKind::Rational(value) if *value.denom() == U256::from(1) => {
                Some(self.builder.imm_u256(*value.numer()))
            }
            _ => report_unsupported(self.gcx, span, "literal"),
        }
    }

    fn lower_member(
        &mut self,
        expr: &hir::Expr<'_>,
        receiver: &hir::Expr<'_>,
        name: solar_interface::Ident,
    ) -> Option<ValueId> {
        if let Some(builtin) = self.gcx.resolved_builtin(expr) {
            if builtin == Builtin::AddressBalance {
                let receiver = self.lower_expr(receiver)?;
                return Some(self.builder.balance(receiver));
            }
            return self.lower_environment_builtin(expr, builtin);
        }
        if let Some(value) = self.lower_internal_function_value(expr) {
            return Some(value);
        }
        if let Some(TyKind::Fn(function)) = self.gcx.type_of_expr(expr.id).map(|ty| ty.kind)
            && function.is_external()
            && let Some(function_id) = self.gcx.resolved_function(expr)
        {
            let address = self.lower_expr(receiver)?;
            let address_shift = self.builder.imm_u64(32);
            let address = self.builder.shl(address_shift, address);
            let selector = self.gcx.function_selector(function_id).0;
            let selector = self.builder.imm_u256(U256::from_be_slice(&selector));
            return Some(self.builder.or(address, selector));
        }
        if name.name == sym::offset
            && self
                .type_of_expr_or_variable(receiver)
                .is_some_and(|ty| ty.is_ref_at(DataLocation::Calldata))
        {
            return self.lower_yul_member(expr, receiver, name);
        }
        if let Some(access) = self.storage_access(expr) {
            return self.load_storage_access(expr, access);
        }
        if name.name == sym::length {
            let receiver_ty = self.gcx.type_of_expr(receiver.id)?;
            if let TyKind::Array(_, len) = receiver_ty.peel_refs().kind {
                if !matches!(receiver.peel_parens().kind, ExprKind::Ident(_)) {
                    self.lower_expr(receiver)?;
                }
                return Some(self.builder.imm_u64(u64::try_from(len).ok()?));
            }
            if receiver_ty.is_ref_at(DataLocation::Storage) {
                if let Some(access) = self.storage_access(receiver) {
                    return match receiver_ty.peel_refs().kind {
                        TyKind::DynArray(_) => Some(self.builder.sload(access.slot)),
                        TyKind::Elementary(
                            solar_sema::hir::ElementaryType::Bytes
                            | solar_sema::hir::ElementaryType::String,
                        ) => {
                            let object = self.load_storage_bytes(access.slot)?;
                            Some(self.builder.memory_object_len(object, MemoryObjectKind::Bytes))
                        }
                        _ => report_unsupported(self.gcx, expr.span, "length member"),
                    };
                }
                let object = self.lower_expr(receiver)?;
                return match self.builder.func().value_ty(object) {
                    Some(MirType::MemoryObject(MemoryObjectKind::Bytes)) => {
                        Some(self.builder.memory_object_len(object, MemoryObjectKind::Bytes))
                    }
                    _ => report_unsupported(self.gcx, expr.span, "length member"),
                };
            }
            let object = self.lower_expr(receiver)?;
            let layout = self.types.memory_layout(receiver_ty)?;
            return match layout.kind() {
                MemoryObjectKind::Bytes | MemoryObjectKind::DynamicArray => {
                    Some(self.builder.memory_object_len(object, layout.kind()))
                }
                _ => report_unsupported(self.gcx, expr.span, "length member"),
            };
        }

        let id = self.gcx.resolved_variable(expr)?;
        let variable = self.gcx.hir.variable(id);
        if let Some(hir::ItemId::Enum(enum_id)) = variable.parent {
            let Some(index) =
                self.gcx.hir.enumm(enum_id).variants.iter().position(|&variant| variant == id)
            else {
                return report_unsupported(self.gcx, expr.span, "enum member");
            };
            return Some(self.builder.imm_u256(U256::from(index)));
        }
        if variable.is_state_variable() {
            return self.load_variable(id, expr.span);
        }
        let Some(hir::ItemId::Struct(struct_id)) = variable.parent else {
            return report_unsupported(self.gcx, expr.span, "member");
        };
        let Some(field) =
            self.gcx.hir.strukt(struct_id).fields.iter().position(|&field| field == id)
        else {
            return report_unsupported(self.gcx, expr.span, "struct field");
        };
        let receiver_ty = self.type_of_expr_or_variable(receiver)?;
        let object = self.lower_expr(receiver)?;
        if receiver_ty.is_ref_at(DataLocation::Calldata)
            && matches!(
                self.builder.func().value_ty(object),
                Some(MirType::Slice(SliceLocation::Calldata))
            )
        {
            let AbiType::Tuple(fields) = self.types.abi_type(receiver_ty)? else {
                return report_unsupported(self.gcx, expr.span, "calldata struct field");
            };
            let offset = fields[..field].iter().map(AbiType::head_size).sum();
            let offset = self.builder.imm_u64(offset);
            let base = self.builder.slice_ptr(object);
            let head = self.builder.add(base, offset);
            let field_ty =
                self.gcx.type_of_item(id.into()).with_loc_if_ref(self.gcx, DataLocation::Calldata);
            return self.materialize_calldata_value_at_inner(field_ty, head, base, expr.span, true);
        }
        let layout = self.types.memory_layout(receiver_ty)?;
        let value = self.builder.memory_object_load_field(object, layout, field as u64);
        let field_ty = self.gcx.type_of_item(id.into());
        if receiver_ty.is_ref_at(DataLocation::Calldata)
            && let TyKind::Fn(function) = field_ty.peel_refs().kind
            && function.is_external()
        {
            let inst = match self.builder.func().value(value) {
                Value::Inst(inst) => Some(*inst),
                _ => None,
            };
            if let Some(inst) = inst {
                self.builder.func_mut().inst_mut(inst).metadata.set_abi_validation(true);
            }
        }
        Some(self.normalize_memory_scalar(field_ty, value))
    }

    fn lower_yul_member(
        &mut self,
        expr: &hir::Expr<'_>,
        receiver: &hir::Expr<'_>,
        name: solar_interface::Ident,
    ) -> Option<ValueId> {
        let receiver_ty = self.type_of_expr_or_variable(receiver)?;
        if receiver_ty.is_ref_at(DataLocation::Calldata) {
            let value = self.lower_expr(receiver)?;
            return match name.name {
                sym::offset => Some(self.builder.slice_ptr(value)),
                sym::length => Some(self.builder.slice_len(value)),
                _ => report_unsupported(self.gcx, expr.span, "Yul calldata member"),
            };
        }

        if let TyKind::Fn(function) = receiver_ty.peel_refs().kind
            && function.is_external()
        {
            let value = self.lower_expr(receiver)?;
            return match name.name {
                kw::Address => {
                    let shift = self.builder.imm_u64(32);
                    let address = self.builder.shr(shift, value);
                    let mask = self.builder.imm_u256(U256::MAX >> 96);
                    Some(self.builder.and(address, mask))
                }
                sym::selector => {
                    let mask = self.builder.imm_u256(U256::from(u32::MAX));
                    Some(self.builder.and(value, mask))
                }
                _ => report_unsupported(self.gcx, expr.span, "Yul function member"),
            };
        }

        let Some(access) = self.storage_access(receiver) else {
            return report_unsupported(self.gcx, expr.span, "Yul storage member");
        };
        match name.name {
            sym::slot => Some(access.slot),
            sym::offset => Some(
                access
                    .offset
                    .unwrap_or_else(|| self.builder.imm_u64(u64::from(access.location.offset))),
            ),
            _ => report_unsupported(self.gcx, expr.span, "Yul storage member"),
        }
    }

    fn type_of_expr_or_variable(&self, expr: &hir::Expr<'_>) -> Option<Ty<'gcx>> {
        self.gcx
            .type_of_expr(expr.id)
            .or_else(|| self.gcx.resolved_variable(expr).map(|id| self.gcx.type_of_item(id.into())))
    }

    fn lower_selector_receiver_effects(&mut self, receiver: &hir::Expr<'_>) -> Option<()> {
        let receiver = receiver.peel_parens();
        match receiver.kind {
            ExprKind::Ident(_) | ExprKind::Type(_) => Some(()),
            ExprKind::Member(base, _)
                if matches!(base.peel_parens().kind, ExprKind::Ident(_) | ExprKind::Type(_)) =>
            {
                Some(())
            }
            ExprKind::Member(base, _) => self.lower_expr(base).map(|_| ()),
            _ => self.lower_expr(receiver).map(|_| ()),
        }
    }

    fn lower_environment_builtin(
        &mut self,
        expr: &hir::Expr<'_>,
        builtin: Builtin,
    ) -> Option<ValueId> {
        if matches!(builtin, Builtin::ContractCreationCode | Builtin::ContractRuntimeCode) {
            let ExprKind::Member(receiver, _) = &expr.kind else {
                return report_unsupported(self.gcx, expr.span, "environment builtin");
            };
            let TyKind::Meta(ty) = self.gcx.type_of_expr(receiver.id)?.kind else {
                return report_unsupported(self.gcx, expr.span, "creation code target");
            };
            let TyKind::Contract(contract_id) = ty.peel_refs().kind else {
                return report_unsupported(self.gcx, expr.span, "creation code target");
            };
            let bytecodes = if builtin == Builtin::ContractCreationCode {
                self.child_bytecodes
            } else {
                self.child_runtime_bytecodes
            };
            let Some(bytecode) = bytecodes.get(&contract_id) else {
                let (kind, name) = if builtin == Builtin::ContractCreationCode {
                    ("creation", "creationCode")
                } else {
                    ("runtime", "runtimeCode")
                };
                self.gcx
                    .dcx()
                    .err(format!("codegen is missing {kind} bytecode for `{name}`"))
                    .span(expr.span)
                    .note("the referenced contract did not compile or was not lowered first")
                    .emit();
                return None;
            };
            return self.lower_bytes_literal(bytecode, expr.span);
        }
        if matches!(builtin, Builtin::AddressCode | Builtin::AddressCodehash) {
            let ExprKind::Member(receiver, _) = &expr.kind else {
                return report_unsupported(self.gcx, expr.span, "environment builtin");
            };
            let address = self.lower_expr(receiver)?;
            if builtin == Builtin::AddressCodehash {
                return Some(self.builder.extcodehash(address));
            }

            let length = self.builder.extcodesize(address);
            let thirty_one = self.builder.imm_u64(31);
            let rounded = self.checked_add(length, thirty_one);
            let word_size = self.builder.imm_u64(32);
            let words = self.builder.div(rounded, word_size);
            let one = self.builder.imm_u64(1);
            let words = self.checked_add(words, one);
            let size = self.checked_mul(words, word_size);
            let object = self.builder.alloc_object(
                size,
                MemoryObjectLayout::Bytes,
                AllocationSemantics::SOLIDITY_ZEROED,
            );
            self.builder.set_memory_object_len(object, length, MemoryObjectKind::Bytes);
            let data = self.builder.memory_object_data(object, MemoryObjectKind::Bytes);
            let zero = self.builder.imm_u256(U256::ZERO);
            self.builder.extcodecopy(address, data, zero, length);
            return Some(object);
        }
        if builtin == Builtin::FunctionAddress {
            let ExprKind::Member(receiver, _) = &expr.kind else {
                return report_unsupported(self.gcx, expr.span, "function address");
            };
            let Some(TyKind::Fn(function)) =
                self.type_of_expr_or_variable(receiver).map(|ty| ty.kind)
            else {
                return report_unsupported(self.gcx, expr.span, "function address");
            };
            if !function.is_external() {
                return report_unsupported(self.gcx, expr.span, "function address");
            }
            let value = self.lower_expr(receiver)?;
            let shift = self.builder.imm_u64(32);
            let address = self.builder.shr(shift, value);
            let mask = self.builder.imm_u256(U256::MAX >> 96);
            return Some(self.builder.and(address, mask));
        }
        if builtin == Builtin::FunctionSelector {
            let selector = match self.gcx.resolved_expr(expr).and_then(|res| match res {
                hir::Res::Item(item @ (hir::ItemId::Function(_) | hir::ItemId::Error(_))) => {
                    Some(self.gcx.function_selector(item).0)
                }
                _ => None,
            }) {
                Some(selector) => {
                    let ExprKind::Member(receiver, _) = &expr.kind else {
                        return report_unsupported(self.gcx, expr.span, "function selector");
                    };
                    self.lower_selector_receiver_effects(receiver)?;
                    selector
                }
                None => {
                    let hir::ExprKind::Member(receiver, _) = &expr.kind else {
                        return report_unsupported(self.gcx, expr.span, "function selector");
                    };
                    if let Some(item) = self.gcx.resolved_expr(receiver).and_then(|res| match res {
                        hir::Res::Item(
                            item @ (hir::ItemId::Function(_) | hir::ItemId::Error(_)),
                        ) => Some(item),
                        _ => None,
                    }) {
                        self.lower_selector_receiver_effects(receiver)?;
                        self.gcx.function_selector(item).0
                    } else {
                        let Some(TyKind::Fn(function)) =
                            self.type_of_expr_or_variable(receiver).map(|ty| ty.kind)
                        else {
                            return report_unsupported(self.gcx, expr.span, "function selector");
                        };
                        if !function.is_external() {
                            return report_unsupported(self.gcx, expr.span, "function selector");
                        }
                        let value = self.lower_expr(receiver)?;
                        let mask = self.builder.imm_u256(U256::from(u32::MAX));
                        let selector = self.builder.and(value, mask);
                        let shift = self.builder.imm_u64(224);
                        return Some(self.builder.shl(shift, selector));
                    }
                }
            };
            return Some(self.builder.imm_u256(U256::from_be_slice(&selector) << 224));
        }
        if builtin == Builtin::EventSelector {
            let event_id = self.gcx.resolved_expr(expr).and_then(|res| match res {
                hir::Res::Item(hir::ItemId::Event(id)) => Some(id),
                _ => None,
            });
            let event_id = event_id.or_else(|| {
                let ExprKind::Member(receiver, _) = &expr.kind else { return None };
                self.gcx.resolved_expr(receiver).and_then(|res| match res {
                    hir::Res::Item(hir::ItemId::Event(id)) => Some(id),
                    _ => None,
                })
            });
            let Some(event_id) = event_id else {
                return report_unsupported(self.gcx, expr.span, "event selector");
            };
            return Some(
                self.builder
                    .imm_u256(U256::from_be_slice(self.gcx.event_selector(event_id).as_slice())),
            );
        }
        if builtin == Builtin::ArrayLength {
            let ExprKind::Member(receiver, _) = &expr.kind else {
                return report_unsupported(self.gcx, expr.span, "array length");
            };
            if self.gcx.resolved_builtin(receiver) == Some(Builtin::AddressCode)
                && let ExprKind::Member(address, _) = &receiver.kind
            {
                let address = self.lower_expr(address)?;
                return Some(self.builder.extcodesize(address));
            }
            let receiver_ty = self.gcx.type_of_expr(receiver.id)?;
            if let TyKind::Array(_, len) = receiver_ty.peel_refs().kind {
                if !matches!(receiver.peel_parens().kind, ExprKind::Ident(_)) {
                    self.lower_expr(receiver)?;
                }
                return Some(self.builder.imm_u64(u64::try_from(len).ok()?));
            }
            if receiver_ty.is_ref_at(DataLocation::Storage) {
                if let Some(access) = self.storage_access(receiver) {
                    return match receiver_ty.peel_refs().kind {
                        TyKind::DynArray(_) => Some(self.builder.sload(access.slot)),
                        TyKind::Elementary(
                            solar_sema::hir::ElementaryType::Bytes
                            | solar_sema::hir::ElementaryType::String,
                        ) => {
                            let object = self.load_storage_bytes(access.slot)?;
                            Some(self.builder.memory_object_len(object, MemoryObjectKind::Bytes))
                        }
                        _ => report_unsupported(self.gcx, expr.span, "array length"),
                    };
                }
                let object = self.lower_expr(receiver)?;
                return match self.builder.func().value_ty(object) {
                    Some(MirType::MemoryObject(MemoryObjectKind::Bytes)) => {
                        Some(self.builder.memory_object_len(object, MemoryObjectKind::Bytes))
                    }
                    _ => report_unsupported(self.gcx, expr.span, "array length"),
                };
            }
            let object = self.lower_expr(receiver)?;
            if matches!(self.builder.func().value_ty(object), Some(MirType::Slice(_))) {
                return Some(self.builder.slice_len(object));
            }
            let layout = self.types.memory_layout(receiver_ty)?;
            return match layout.kind() {
                MemoryObjectKind::Bytes | MemoryObjectKind::DynamicArray => {
                    Some(self.builder.memory_object_len(object, layout.kind()))
                }
                _ => report_unsupported(self.gcx, expr.span, "array length"),
            };
        }
        if matches!(builtin, Builtin::TypeMin | Builtin::TypeMax | Builtin::InterfaceId) {
            let ExprKind::Member(receiver, _) = &expr.kind else {
                return report_unsupported(self.gcx, expr.span, "type member");
            };
            if builtin == Builtin::InterfaceId {
                let TyKind::Meta(ty) = self.gcx.type_of_expr(receiver.id)?.kind else {
                    return report_unsupported(self.gcx, expr.span, "interface id");
                };
                let TyKind::Contract(id) = ty.peel_refs().kind else {
                    return report_unsupported(self.gcx, expr.span, "interface id");
                };
                let value = self
                    .gcx
                    .interface_functions(id)
                    .own()
                    .iter()
                    .fold(U256::ZERO, |value, function| {
                        value ^ U256::from_be_slice(function.selector.as_slice())
                    })
                    << 224;
                return Some(self.builder.imm_u256(value));
            }
            let value = self.type_limit(receiver, expr.span, builtin == Builtin::TypeMax)?;
            return Some(self.builder.imm_u256(value));
        }
        Some(match builtin {
            Builtin::This => self.builder.address(),
            Builtin::BlockCoinbase => self.builder.coinbase(),
            Builtin::BlockTimestamp => self.builder.timestamp(),
            Builtin::BlockDifficulty | Builtin::BlockPrevrandao => self.builder.prevrandao(),
            Builtin::BlockNumber => self.builder.number(),
            Builtin::BlockGaslimit => self.builder.gaslimit(),
            Builtin::BlockChainid => self.builder.chainid(),
            Builtin::BlockBasefee => self.builder.basefee(),
            Builtin::BlockBlobbasefee => self.builder.blobbasefee(),
            Builtin::MsgSender => self.builder.caller(),
            Builtin::MsgGas => self.builder.gas(),
            Builtin::MsgValue => self.builder.callvalue(),
            Builtin::MsgSig => {
                let offset = self.builder.imm_u64(0);
                let value = self.calldata_load_word(offset);
                let mask = self.builder.imm_u256(U256::MAX << 224);
                self.builder.and(value, mask)
            }
            Builtin::MsgData => {
                let offset = self.builder.imm_u64(0);
                let length = self.builder.calldatasize();
                self.builder.make_slice(offset, length, SliceLocation::Calldata)
            }
            Builtin::TxOrigin => self.builder.origin(),
            Builtin::TxGasPrice => self.builder.gasprice(),
            _ => return report_unsupported(self.gcx, expr.span, "environment builtin"),
        })
    }

    fn type_limit(&self, receiver: &hir::Expr<'_>, span: Span, maximum: bool) -> Option<U256> {
        let TyKind::Meta(ty) = self.gcx.type_of_expr(receiver.id)?.kind else {
            return report_unsupported(self.gcx, span, "type limit");
        };
        match ty.peel_refs().kind {
            TyKind::Enum(id) => {
                let max = self.gcx.hir.enumm(id).variants.len().saturating_sub(1);
                Some(U256::from(if maximum { max } else { 0 }))
            }
            TyKind::Elementary(solar_sema::hir::ElementaryType::UInt(size)) => {
                let max = (U256::from(1) << size.bits()) - U256::from(1);
                Some(if maximum { max } else { U256::ZERO })
            }
            TyKind::Elementary(solar_sema::hir::ElementaryType::Int(size)) => {
                let magnitude = U256::from(1) << (size.bits() - 1);
                Some(if maximum {
                    magnitude - U256::from(1)
                } else {
                    U256::MAX - magnitude + U256::from(1)
                })
            }
            _ => report_unsupported(self.gcx, span, "type limit"),
        }
    }

    fn normalize_byte_value(&mut self, expr: &hir::Expr<'_>, value: ValueId) -> ValueId {
        let Some(ty) = self.gcx.type_of_expr(expr.id) else { return value };
        self.normalize_byte_type(ty, value)
    }

    fn normalize_byte_type(&mut self, ty: Ty<'gcx>, value: ValueId) -> ValueId {
        let TyKind::Elementary(solar_sema::hir::ElementaryType::FixedBytes(size)) =
            ty.peel_refs().kind
        else {
            return value;
        };
        let shift = self.builder.imm_u64(u64::from(32 - size.bytes()) * 8);
        self.builder.shl(shift, value)
    }

    fn peel_bytes_conversion<'b>(&self, expr: &'b hir::Expr<'b>) -> &'b hir::Expr<'b> {
        if let ExprKind::Call(callee, args, _) = &expr.kind
            && let ExprKind::Type(ty) = &callee.kind
            && matches!(
                ty.kind,
                hir::TypeKind::Elementary(
                    solar_sema::hir::ElementaryType::Bytes
                        | solar_sema::hir::ElementaryType::String
                )
            )
            && let hir::CallArgsKind::Unnamed([inner]) = args.kind
        {
            return inner;
        }
        expr
    }

    fn lower_constant(
        &mut self,
        initializer: Option<&hir::Expr<'_>>,
        span: Span,
    ) -> Option<ValueId> {
        let Some(initializer) = initializer else {
            return report_unsupported(self.gcx, span, "constant initializer");
        };
        if let Ok(value) = self.gcx.try_eval_const_value(initializer) {
            return match value {
                ConstValue::Bool(value) => Some(self.builder.imm_bool(*value)),
                ConstValue::Integer(value) => Some(self.builder.imm_u256(value.as_u256()?)),
                ConstValue::String(value) => {
                    self.lower_bytes_literal(value.as_byte_str_in(self.gcx.sess), span)
                }
            };
        }
        self.lower_expr(initializer)
    }

    fn signed_add_overflow(
        &mut self,
        lhs: ValueId,
        rhs: ValueId,
        result: ValueId,
        bits: u16,
    ) -> ValueId {
        let zero = self.builder.imm_u256(U256::ZERO);
        let lhs_negative = self.builder.slt(lhs, zero);
        let rhs_negative = self.builder.slt(rhs, zero);
        let result_negative = self.builder.slt(result, zero);
        let signs_differ = self.builder.xor(lhs_negative, rhs_negative);
        let result_changed_sign = self.builder.xor(result_negative, lhs_negative);
        let same_sign = self.builder.iszero(signs_differ);
        let mut overflow = self.builder.and(same_sign, result_changed_sign);
        if bits < 256 {
            let (min, max) = signed_bounds(bits, &mut self.builder);
            let below = self.builder.slt(result, min);
            let above = self.builder.sgt(result, max);
            let out_of_range = self.builder.or(below, above);
            overflow = self.builder.or(overflow, out_of_range);
        }
        overflow
    }

    fn signed_sub_overflow(
        &mut self,
        lhs: ValueId,
        rhs: ValueId,
        result: ValueId,
        bits: u16,
    ) -> ValueId {
        let zero = self.builder.imm_u256(U256::ZERO);
        let lhs_negative = self.builder.slt(lhs, zero);
        let rhs_negative = self.builder.slt(rhs, zero);
        let result_negative = self.builder.slt(result, zero);
        let signs_differ = self.builder.xor(lhs_negative, rhs_negative);
        let result_changed_sign = self.builder.xor(result_negative, lhs_negative);
        let mut overflow = self.builder.and(signs_differ, result_changed_sign);
        if bits < 256 {
            let (min, max) = signed_bounds(bits, &mut self.builder);
            let below = self.builder.slt(result, min);
            let above = self.builder.sgt(result, max);
            let out_of_range = self.builder.or(below, above);
            overflow = self.builder.or(overflow, out_of_range);
        }
        overflow
    }

    fn mul_overflow(
        &mut self,
        lhs: ValueId,
        rhs: ValueId,
        result: ValueId,
        kind: ArithmeticKind,
    ) -> ValueId {
        let rhs_zero = self.builder.iszero(rhs);
        let quotient = match kind {
            ArithmeticKind::Unsigned(_) => self.builder.div(result, rhs),
            ArithmeticKind::Signed(_) => self.builder.sdiv(result, rhs),
        };
        let exact = self.builder.eq(quotient, lhs);
        let valid = self.builder.or(rhs_zero, exact);
        let mut overflow = self.builder.iszero(valid);
        if let ArithmeticKind::Signed(bits) = kind {
            let (min, max) = signed_bounds(bits, &mut self.builder);
            let below = self.builder.slt(result, min);
            let above = self.builder.sgt(result, max);
            let out_of_range = self.builder.or(below, above);
            overflow = self.builder.or(overflow, out_of_range);
            let minus_one = self.builder.imm_u256(U256::MAX);
            let lhs_is_min = self.builder.eq(lhs, min);
            let rhs_is_minus_one = self.builder.eq(rhs, minus_one);
            let special = self.builder.and(lhs_is_min, rhs_is_minus_one);
            overflow = self.builder.or(overflow, special);
        } else if let ArithmeticKind::Unsigned(bits) = kind
            && bits < 256
        {
            let max = self.builder.imm_u256((U256::from(1) << bits) - U256::ONE);
            let too_wide = self.builder.gt(result, max);
            overflow = self.builder.or(overflow, too_wide);
        }
        overflow
    }

    fn truncate_wrapping_result(
        &mut self,
        value: ValueId,
        kind: Option<ArithmeticKind>,
    ) -> ValueId {
        match kind {
            Some(ArithmeticKind::Unsigned(bits)) if bits < 256 => {
                let max = self.builder.imm_u256((U256::from(1) << bits) - U256::ONE);
                self.builder.and(value, max)
            }
            Some(ArithmeticKind::Signed(bits)) if (8..256).contains(&bits) => {
                let byte = self.builder.imm_u64(u64::from(bits / 8 - 1));
                self.builder.signextend(byte, value)
            }
            _ => value,
        }
    }

    fn checked_pow(&mut self, base: ValueId, exponent: ValueId, kind: ArithmeticKind) -> ValueId {
        let one = self.builder.imm_u256(U256::ONE);
        let zero = self.builder.imm_u256(U256::ZERO);
        let preheader = self.builder.current_block();
        let header = self.builder.create_block();
        let body = self.builder.create_block();
        let exit = self.builder.create_block();
        self.builder.jump(header);

        self.builder.switch_to_block(header);
        let power = self.builder.phi(vec![(preheader, one)]);
        let current_base = self.builder.phi(vec![(preheader, base)]);
        let current_exponent = self.builder.phi(vec![(preheader, exponent)]);
        let has_exponent = self.builder.gt(current_exponent, zero);
        self.builder.branch(has_exponent, body, exit);

        self.builder.switch_to_block(body);
        let odd = self.builder.and(current_exponent, one);
        let product = self.builder.mul(power, current_base);
        let product_overflow = self.mul_overflow(power, current_base, product, kind);
        let product_check = self.builder.and(odd, product_overflow);
        self.panic_if(product_check, 0x11);
        let next_power = self.builder.select(odd, product, power);

        let next_exponent = self.builder.shr(one, current_exponent);
        let square = self.builder.mul(current_base, current_base);
        let square_overflow = self.mul_overflow(current_base, current_base, square, kind);
        let has_next_exponent = self.builder.gt(next_exponent, zero);
        let square_check = self.builder.and(has_next_exponent, square_overflow);
        self.panic_if(square_check, 0x11);
        let latch = self.builder.current_block();
        self.builder.jump(header);
        self.builder.add_phi_incoming(power, latch, next_power);
        self.builder.add_phi_incoming(current_base, latch, square);
        self.builder.add_phi_incoming(current_exponent, latch, next_exponent);

        self.builder.switch_to_block(exit);
        power
    }

    fn binary(
        &mut self,
        op: BinOpKind,
        lhs: ValueId,
        rhs: ValueId,
        ty: Option<Ty<'gcx>>,
    ) -> ValueId {
        let arithmetic = ty.and_then(arithmetic_kind);
        match op {
            BinOpKind::Add => {
                let result = self.builder.add(lhs, rhs);
                if self.unchecked {
                    self.truncate_wrapping_result(result, arithmetic)
                } else {
                    if let Some(kind) = arithmetic {
                        let overflow = match kind {
                            ArithmeticKind::Unsigned(bits) => {
                                if bits == 256 {
                                    self.builder.lt(result, lhs)
                                } else {
                                    let max =
                                        self.builder.imm_u256((U256::from(1) << bits) - U256::ONE);
                                    self.builder.gt(result, max)
                                }
                            }
                            ArithmeticKind::Signed(bits) => {
                                self.signed_add_overflow(lhs, rhs, result, bits)
                            }
                        };
                        self.panic_if(overflow, 0x11);
                    }
                    result
                }
            }
            BinOpKind::Sub => {
                let result = self.builder.sub(lhs, rhs);
                if self.unchecked {
                    self.truncate_wrapping_result(result, arithmetic)
                } else {
                    if let Some(kind) = arithmetic {
                        let overflow = match kind {
                            ArithmeticKind::Unsigned(_) => self.builder.lt(lhs, rhs),
                            ArithmeticKind::Signed(bits) => {
                                self.signed_sub_overflow(lhs, rhs, result, bits)
                            }
                        };
                        self.panic_if(overflow, 0x11);
                    }
                    result
                }
            }
            BinOpKind::Mul => {
                let result = self.builder.mul(lhs, rhs);
                if self.unchecked {
                    self.truncate_wrapping_result(result, arithmetic)
                } else {
                    if let Some(kind) = arithmetic {
                        let overflow = self.mul_overflow(lhs, rhs, result, kind);
                        self.panic_if(overflow, 0x11);
                    }
                    result
                }
            }
            BinOpKind::Div => {
                if !self.unchecked {
                    let zero = self.builder.iszero(rhs);
                    self.panic_if(zero, 0x12);
                    if let Some(ArithmeticKind::Signed(bits)) = arithmetic {
                        let (min, _) = signed_bounds(bits, &mut self.builder);
                        let lhs_is_min = self.builder.eq(lhs, min);
                        let minus_one = self.builder.imm_u256(U256::MAX);
                        let rhs_is_minus_one = self.builder.eq(rhs, minus_one);
                        let overflow = self.builder.and(lhs_is_min, rhs_is_minus_one);
                        self.panic_if(overflow, 0x11);
                    }
                }
                match arithmetic {
                    Some(ArithmeticKind::Signed(_)) => self.builder.sdiv(lhs, rhs),
                    _ => self.builder.div(lhs, rhs),
                }
            }
            BinOpKind::Rem => {
                if !self.unchecked {
                    let zero = self.builder.iszero(rhs);
                    self.panic_if(zero, 0x12);
                }
                match arithmetic {
                    Some(ArithmeticKind::Signed(_)) => self.builder.smod(lhs, rhs),
                    _ => self.builder.mod_(lhs, rhs),
                }
            }
            BinOpKind::Lt => match arithmetic {
                Some(ArithmeticKind::Signed(_)) => self.builder.slt(lhs, rhs),
                _ => self.builder.lt(lhs, rhs),
            },
            BinOpKind::Gt => match arithmetic {
                Some(ArithmeticKind::Signed(_)) => self.builder.sgt(lhs, rhs),
                _ => self.builder.gt(lhs, rhs),
            },
            BinOpKind::Eq => self.builder.eq(lhs, rhs),
            BinOpKind::Ne => {
                let eq = self.builder.eq(lhs, rhs);
                self.builder.iszero(eq)
            }
            BinOpKind::Le => {
                let gt = match arithmetic {
                    Some(ArithmeticKind::Signed(_)) => self.builder.sgt(lhs, rhs),
                    _ => self.builder.gt(lhs, rhs),
                };
                self.builder.iszero(gt)
            }
            BinOpKind::Ge => {
                let lt = match arithmetic {
                    Some(ArithmeticKind::Signed(_)) => self.builder.slt(lhs, rhs),
                    _ => self.builder.lt(lhs, rhs),
                };
                self.builder.iszero(lt)
            }
            BinOpKind::And | BinOpKind::BitAnd => self.builder.and(lhs, rhs),
            BinOpKind::Or | BinOpKind::BitOr => self.builder.or(lhs, rhs),
            BinOpKind::BitXor => self.builder.xor(lhs, rhs),
            BinOpKind::Shl => self.builder.shl(rhs, lhs),
            BinOpKind::Shr => match arithmetic {
                Some(ArithmeticKind::Signed(_)) => self.builder.sar(rhs, lhs),
                _ => self.builder.shr(rhs, lhs),
            },
            BinOpKind::Sar => self.builder.sar(rhs, lhs),
            BinOpKind::Pow => {
                if self.unchecked {
                    let result = self.builder.exp(lhs, rhs);
                    self.truncate_wrapping_result(result, arithmetic)
                } else if let Some(kind) = arithmetic {
                    self.checked_pow(lhs, rhs, kind)
                } else {
                    self.builder.exp(lhs, rhs)
                }
            }
        }
    }

    fn unary(
        &mut self,
        op: UnOpKind,
        value: ValueId,
        span: Span,
        ty: Option<Ty<'gcx>>,
    ) -> Option<ValueId> {
        Some(match op {
            UnOpKind::Not => self.builder.iszero(value),
            UnOpKind::Neg => {
                if !self.unchecked
                    && let Some(ArithmeticKind::Signed(bits)) = ty.and_then(arithmetic_kind)
                {
                    let (min, _) = signed_bounds(bits, &mut self.builder);
                    let overflow = self.builder.eq(value, min);
                    self.panic_if(overflow, 0x11);
                }
                let zero = self.builder.imm_u256(U256::ZERO);
                let result = self.builder.sub(zero, value);
                if self.unchecked {
                    self.truncate_wrapping_result(result, ty.and_then(arithmetic_kind))
                } else {
                    result
                }
            }
            UnOpKind::BitNot => self.builder.not(value),
            UnOpKind::PreInc | UnOpKind::PostInc | UnOpKind::PreDec | UnOpKind::PostDec => {
                return report_unsupported(self.gcx, span, "increment or decrement");
            }
        })
    }

    fn merge_values(
        &mut self,
        before: FxHashMap<VariableId, ValueId>,
        then_branch: MergeBranch<ValueId>,
        else_branch: MergeBranch<ValueId>,
    ) -> FxHashMap<VariableId, ValueId> {
        let mut values = before;
        let mut ids = values.keys().copied().collect::<Vec<_>>();
        ids.extend(then_branch.values.keys().copied());
        ids.extend(else_branch.values.keys().copied());
        ids.sort_unstable();
        ids.dedup();
        for id in ids {
            let then_value = then_branch.values.get(&id).copied();
            let else_value = else_branch.values.get(&id).copied();
            let value =
                match (then_branch.terminated, else_branch.terminated, then_value, else_value) {
                    (true, false, _, value) | (false, true, value, _) => value,
                    (_, _, Some(lhs), Some(rhs)) if lhs == rhs => Some(lhs),
                    (false, false, Some(lhs), Some(rhs)) => Some(
                        self.builder.phi(vec![(then_branch.block, lhs), (else_branch.block, rhs)]),
                    ),
                    _ => then_value.or(else_value),
                };
            if let Some(value) = value {
                values.insert(id, value);
            }
        }
        values
    }

    fn merge_many_values(
        &mut self,
        mut before: FxHashMap<VariableId, ValueId>,
        states: &[LoopState],
    ) -> FxHashMap<VariableId, ValueId> {
        let mut ids = before.keys().copied().collect::<Vec<_>>();
        ids.extend(states.iter().flat_map(|state| state.values.keys().copied()));
        ids.sort_unstable();
        ids.dedup();
        for id in ids {
            let incoming = states
                .iter()
                .filter_map(|state| {
                    state
                        .values
                        .get(&id)
                        .copied()
                        .or_else(|| before.get(&id).copied())
                        .map(|value| (state.block, value))
                })
                .collect::<Vec<_>>();
            let value = match incoming.as_slice() {
                [] => None,
                [(_, value)] => Some(*value),
                [(_, first), rest @ ..] if rest.iter().all(|(_, value)| value == first) => {
                    Some(*first)
                }
                _ => Some(self.builder.phi(incoming)),
            };
            if let Some(value) = value {
                before.insert(id, value);
            }
        }
        before
    }

    fn merge_many_storage_refs(
        &mut self,
        mut before: FxHashMap<VariableId, StorageAccess>,
        states: &[LoopState],
    ) -> FxHashMap<VariableId, StorageAccess> {
        let ids = before
            .keys()
            .chain(states.iter().flat_map(|state| state.storage_refs.keys()))
            .copied()
            .collect::<solar_data_structures::map::FxHashSet<_>>();
        for id in ids {
            let fallback = before.get(&id).copied();
            let incoming = states
                .iter()
                .filter_map(|state| {
                    state
                        .storage_refs
                        .get(&id)
                        .copied()
                        .or(fallback)
                        .map(|access| (state.block, access))
                })
                .collect::<Vec<_>>();
            if let Some(access) = self.merge_storage_accesses(incoming).or(fallback) {
                before.insert(id, access);
            }
        }
        before
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
