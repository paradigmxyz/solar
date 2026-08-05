//! Function-level HIR to MIR lowering.

use super::{
    contract,
    storage::{StorageLayout, StorageLocation},
    types,
};
use crate::{
    memory::EvmMemoryLayout,
    mir::{
        AbiLayout, AbiParamLayout, AbiParamType, AbiType, AllocationSemantics, BlockId, Function,
        FunctionBuilder, FunctionId, ImmutableId, InstKind, MemoryObjectKind, MemoryObjectLayout,
        MirType, Module, SliceLocation, Value, ValueId,
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

/// Lowers one HIR function into a typed MIR function.
pub(super) fn lower(
    gcx: Gcx<'_>,
    module: &mut Module,
    storage: &StorageLayout,
    contract_id: hir::ContractId,
    id: hir::FunctionId,
    expose_selector: bool,
    function_ids: &FxHashMap<hir::FunctionId, FunctionId>,
    immutable_ids: &FxHashMap<VariableId, ImmutableId>,
    child_bytecodes: &FxHashMap<hir::ContractId, Bytes>,
    child_runtime_bytecodes: &FxHashMap<hir::ContractId, Bytes>,
    invalid_event_topics: &mut FxHashSet<hir::EventId>,
    pointer_registry: &mut InternalFunctionPointerRegistry,
) -> Option<Function> {
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

    let has_constructor_params = mir.attributes.is_constructor
        && input_shapes.as_ref().is_some_and(|shapes| !shapes.is_empty());
    if mir.selector.is_some() || has_constructor_params {
        let Some(input_shapes) = input_shapes else {
            return report_unsupported(gcx, hir_function.span, "function parameter shape");
        };
        mir.abi_params = Some(AbiParamLayout::new(input_shapes.into_boxed_slice()));
        mir.abi_args_lazy = true;
        if mir.selector.is_some() {
            let Some(output_shapes) = output_shapes else {
                return report_unsupported(gcx, hir_function.span, "function return shape");
            };
            mir.abi_returns =
                Some(module.intern_abi_layout(AbiLayout::new(output_shapes.into_boxed_slice())));
        }
    }

    let mut lowerer = FunctionLowerer::new(
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
        &mut mir,
    );
    lowerer.bind_signature(hir_function);
    if hir_function.kind == hir::FunctionKind::Constructor {
        let Some(contract_id) = hir_function.contract else {
            return report_unsupported(gcx, hir_function.span, "free constructor");
        };
        lowerer.lower_state_initializers(contract_id)?;
        lowerer.lower_implicit_base_constructors(contract_id)?;
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
    gcx: Gcx<'_>,
    module: &mut Module,
    storage: &StorageLayout,
    contract_id: hir::ContractId,
    function_ids: &FxHashMap<hir::FunctionId, FunctionId>,
    immutable_ids: &FxHashMap<VariableId, ImmutableId>,
    child_bytecodes: &FxHashMap<hir::ContractId, Bytes>,
    child_runtime_bytecodes: &FxHashMap<hir::ContractId, Bytes>,
    invalid_event_topics: &mut FxHashSet<hir::EventId>,
    pointer_registry: &mut InternalFunctionPointerRegistry,
) -> Option<Function> {
    let mut mir =
        Function::new(solar_interface::Ident::with_dummy_span(solar_interface::kw::Constructor));
    mir.attributes.is_constructor = true;
    let mut lowerer = FunctionLowerer::new(
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
        &mut mir,
    );
    lowerer.lower_state_initializers(contract_id)?;
    lowerer.lower_implicit_base_constructors(contract_id)?;
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
    storage: &'mir StorageLayout,
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
    return_targets: Vec<BlockId>,
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

#[derive(Clone, Copy)]
struct ModifierContext<'gcx> {
    modifiers: &'gcx [hir::Modifier<'gcx>],
    body: hir::Block<'gcx>,
    next: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct StorageAccess {
    slot: ValueId,
    location: StorageLocation,
    offset: Option<ValueId>,
}

enum PackedPiece {
    Bytes(Vec<u8>),
    Static { value: ValueId, length: u64, fixed_bytes: bool },
    Dynamic { source: ValueId, length: ValueId },
    Array { value: ValueId, length: ValueId, source: PackedArraySource },
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
        gcx: Gcx<'gcx>,
        module: &'module mut Module,
        storage: &'mir StorageLayout,
        contract_id: hir::ContractId,
        function_ids: &'ids FxHashMap<hir::FunctionId, FunctionId>,
        immutable_ids: &'ids FxHashMap<VariableId, ImmutableId>,
        child_bytecodes: &'bytes FxHashMap<hir::ContractId, Bytes>,
        child_runtime_bytecodes: &'bytes FxHashMap<hir::ContractId, Bytes>,
        invalid_event_topics: &'events mut FxHashSet<hir::EventId>,
        pointer_registry: &'pointers mut InternalFunctionPointerRegistry,
        function: &'mir mut Function,
    ) -> Self {
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
            let value = self.default_binding_value(ty);
            self.values.insert(ret, value);
        }
        self.returns.extend_from_slice(function.returns);
    }

    fn lower_state_initializers(&mut self, contract_id: hir::ContractId) -> Option<()> {
        let contract = self.gcx.hir.contract(contract_id);
        for &base in contract.linearized_bases.iter().rev() {
            for id in self.gcx.hir.contract(base).variables() {
                let variable = self.gcx.hir.variable(id);
                if !variable.is_state_variable() || variable.is_constant() {
                    continue;
                }
                let Some(initializer) = variable.initializer else { continue };
                let value = self.lower_expr(initializer)?;
                if let Some(&immutable_id) = self.immutable_ids.get(&id) {
                    self.builder.store_immutable(immutable_id, value);
                } else {
                    self.store_state_variable(id, value, initializer.span)?;
                }
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
            let Some(constructor_id) = self.gcx.hir.contract(base_id).ctor else { continue };
            if lowered.contains(&base_id) {
                continue;
            }
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
                let value = self.values.get(&id).copied()?;
                values.push(self.materialize_memory_argument(
                    self.gcx.type_of_item(id.into()),
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

    fn lower_function_body(
        &mut self,
        modifiers: &'gcx [hir::Modifier<'gcx>],
        body: hir::Block<'gcx>,
    ) -> Option<()> {
        if modifiers.is_empty() {
            self.lower_block(body)
        } else {
            let return_block = self.builder.create_block();
            self.return_targets.push(return_block);
            let result = self.lower_modifier_chain(modifiers, body);
            self.return_targets.pop();
            result?;
            if !self.is_terminated() {
                self.builder.jump(return_block);
            }
            self.builder.switch_to_block(return_block);
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

    fn lower_stmt(&mut self, stmt: &hir::Stmt<'_>) -> Option<()> {
        match &stmt.kind {
            StmtKind::DeclSingle(id) => {
                let initializer = self.gcx.hir.variable(*id).initializer;
                let ty = self.gcx.type_of_item((*id).into());
                if ty.is_ref_at(DataLocation::Storage) {
                    let Some(initializer) = initializer else { return Some(()) };
                    let Some(access) = self.storage_access(initializer) else {
                        return report_unsupported(self.gcx, initializer.span, "storage reference");
                    };
                    self.storage_refs.insert(*id, access);
                    return Some(());
                }
                let value = if let Some(expr) = initializer {
                    let source_ty = self.gcx.type_of_expr(expr.id);
                    if self.types.memory_layout(ty).is_some()
                        && source_ty.is_some_and(|source| source.is_ref_at(DataLocation::Storage))
                    {
                        let access = self.storage_access(expr)?;
                        self.load_storage_object(ty, access.slot, expr.span)?
                    } else {
                        self.lower_expr(expr)?
                    }
                } else if let Some(value) = self.default_object(ty) {
                    value
                } else {
                    self.builder.imm_u256(U256::ZERO)
                };
                let value = self.materialize_memory_argument(
                    ty,
                    value,
                    initializer.map_or(stmt.span, |expr| expr.span),
                )?;
                self.values.insert(*id, value);
            }
            StmtKind::DeclMulti(ids, expr) => {
                if let ExprKind::Tuple(values) = &expr.peel_parens().kind
                    && values.len() == ids.len()
                {
                    for (id, value) in ids.iter().zip(values.iter()) {
                        let Some(value) = value else {
                            if id.is_some() {
                                return report_unsupported(
                                    self.gcx,
                                    expr.span,
                                    "tuple declaration value",
                                );
                            }
                            continue;
                        };
                        let value = self.lower_expr(value)?;
                        let Some(id) = id else { continue };
                        self.values.insert(*id, value);
                    }
                    return Some(());
                }
                if self.is_low_level_call_expr(expr) {
                    let values = self.lower_low_level_call_values(
                        expr,
                        ids.iter().flatten().count(),
                        ids.first().is_some_and(Option::is_none),
                    )?;
                    for (id, value) in ids.iter().flatten().zip(values) {
                        self.values.insert(*id, value);
                    }
                    return Some(());
                }
                let values = self.lower_values(expr)?;
                if values.len() != ids.len() {
                    return report_unsupported(self.gcx, expr.span, "tuple declaration arity");
                }
                for (id, value) in ids.iter().zip(values) {
                    if let Some(id) = id {
                        self.values.insert(*id, value);
                    }
                }
            }
            StmtKind::Expr(expr) => {
                self.lower_expr(expr)?;
            }
            StmtKind::Block(block) => self.lower_block(*block)?,
            StmtKind::UncheckedBlock(block) => {
                let previous = self.unchecked;
                self.unchecked = true;
                let result = self.lower_block(*block);
                self.unchecked = previous;
                result?;
            }
            StmtKind::If(cond, then_stmt, else_stmt) => {
                self.lower_if(cond, then_stmt, *else_stmt)?;
            }
            StmtKind::Switch(switch) => self.lower_switch(switch)?,
            StmtKind::Loop(block, source) => self.lower_loop(*block, *source)?,
            StmtKind::Break => {
                let Some(target) = self.loops.last().map(|targets| targets.break_block) else {
                    return report_unsupported(self.gcx, stmt.span, "break outside loop");
                };
                let state = LoopState {
                    block: self.builder.current_block(),
                    values: self.values.clone(),
                    storage_refs: self.storage_refs.clone(),
                };
                self.loops.last_mut().expect("loop target exists").break_states.push(state);
                self.builder.jump(target);
            }
            StmtKind::Continue => {
                let Some(target) = self.loops.last().map(|targets| targets.continue_block) else {
                    return report_unsupported(self.gcx, stmt.span, "continue outside loop");
                };
                let state = LoopState {
                    block: self.builder.current_block(),
                    values: self.values.clone(),
                    storage_refs: self.storage_refs.clone(),
                };
                self.loops.last_mut().expect("loop target exists").continue_states.push(state);
                self.builder.jump(target);
            }
            StmtKind::Return(expr) => {
                let values =
                    expr.map_or_else(|| Some(Vec::new()), |expr| self.lower_values(expr))?;
                if let Some(&target) = self.return_targets.last() {
                    if !values.is_empty() {
                        if values.len() != self.returns.len() {
                            return report_unsupported(self.gcx, stmt.span, "return value count");
                        }
                        let return_ids = self.returns.clone();
                        for (id, value) in return_ids.into_iter().zip(values) {
                            let value = self.materialize_memory_argument(
                                self.gcx.type_of_item(id.into()),
                                value,
                                stmt.span,
                            )?;
                            self.values.insert(id, value);
                        }
                    }
                    self.builder.jump(target);
                } else if !self.is_terminated() {
                    if values.is_empty() {
                        self.builder.stop();
                    } else {
                        if values.len() != self.returns.len() {
                            return report_unsupported(self.gcx, stmt.span, "return value count");
                        }
                        let return_ids = self.returns.clone();
                        let values = return_ids
                            .into_iter()
                            .zip(values)
                            .map(|(id, value)| {
                                self.materialize_memory_argument(
                                    self.gcx.type_of_item(id.into()),
                                    value,
                                    stmt.span,
                                )
                            })
                            .collect::<Option<Vec<_>>>()?;
                        self.builder.ret(values);
                    }
                }
            }
            StmtKind::Revert(expr) => self.lower_revert_payload(expr)?,
            StmtKind::AssemblyBlock(block) => self.lower_block(*block)?,
            StmtKind::Placeholder => {
                self.lower_modifier_placeholder(stmt.span)?;
            }
            StmtKind::Emit(expr) => self.lower_emit(expr)?,
            StmtKind::Try(try_stmt) => self.lower_try(try_stmt)?,
            StmtKind::Err(_) => return report_unsupported(self.gcx, stmt.span, "statement"),
        }
        Some(())
    }

    fn lower_revert_payload(&mut self, expr: &hir::Expr<'_>) -> Option<()> {
        if let ExprKind::Call(callee, args, _) = &expr.kind
            && let Some(hir::Res::Item(hir::ItemId::Error(error_id))) =
                self.gcx.resolved_expr(callee)
        {
            return self.lower_custom_error_revert(error_id, *args);
        }

        let value = self.lower_expr(expr)?;
        let ty = self.gcx.type_of_expr(expr.id)?;
        let value =
            if self.needs_calldata_materialization(value, &AbiType::Bytes(SliceLocation::Memory)) {
                self.materialize_calldata_argument(ty, value, expr.span)?
            } else {
                value
            };
        let selector = keccak256("Error(string)");
        let selector = self.builder.imm_u256(U256::from_be_slice(&selector[..4]) << 224);
        let layout = Arc::new(AbiLayout::new(
            vec![AbiType::Bytes(SliceLocation::Memory)].into_boxed_slice(),
        ));
        let encoded =
            self.builder.abi_encode(layout, Some(selector), vec![value].into_boxed_slice());
        let pointer = self.builder.slice_ptr(encoded);
        let length = self.builder.slice_len(encoded);
        self.builder.revert(pointer, length);
        Some(())
    }

    fn lower_custom_error_revert(
        &mut self,
        error_id: hir::ErrorId,
        args: hir::CallArgs<'_>,
    ) -> Option<()> {
        let parameters = self.gcx.item_parameters(hir::ItemId::Error(error_id));
        if args.len() != parameters.len() {
            return report_unsupported(self.gcx, args.span, "error arguments");
        }
        let parameter_names = self.gcx.callable_param_names(CallableParamSource::Error(error_id));
        let mut values = Vec::with_capacity(parameters.len());
        let mut types = Vec::with_capacity(parameters.len());
        for (index, &parameter) in parameters.iter().enumerate() {
            let Some(argument) =
                args.argument_for_parameter(index, Some(parameter_names.as_slice()))
            else {
                return report_unsupported(self.gcx, args.span, "error argument");
            };
            let parameter_ty = self.gcx.type_of_item(parameter.into());
            let mut value = self.lower_expr(argument)?;
            let mut abi_type = self.types.abi_type(parameter_ty)?;
            abi_type = self.abi_type_for_value(value, abi_type);
            if self.needs_calldata_materialization(value, &abi_type) {
                value = self.materialize_calldata_argument(parameter_ty, value, argument.span)?;
                abi_type = Self::memory_abi_type(abi_type);
            }
            if matches!(abi_type, AbiType::Word) {
                value = self.lower_word_value(parameter_ty, argument, value);
            }
            values.push(value);
            types.push(abi_type);
        }
        let layout = Arc::new(AbiLayout::new(types.into_boxed_slice()));
        let selector = self
            .builder
            .imm_u256(U256::from_be_slice(&self.gcx.function_selector(error_id).0) << 224);
        let encoded = self.builder.abi_encode(layout, Some(selector), values.into_boxed_slice());
        let pointer = self.builder.slice_ptr(encoded);
        let length = self.builder.slice_len(encoded);
        self.builder.revert(pointer, length);
        Some(())
    }

    fn lower_emit(&mut self, expr: &hir::Expr<'_>) -> Option<()> {
        let ExprKind::Call(callee, args, _) = &expr.kind else {
            return report_unsupported(self.gcx, expr.span, "event emission");
        };
        let Some(hir::Res::Item(hir::ItemId::Event(event_id))) = self.gcx.resolved_expr(callee)
        else {
            return report_unsupported(self.gcx, expr.span, "event emission");
        };

        let event = self.gcx.hir.event(event_id);
        let max_indexed = if event.anonymous { 4 } else { 3 };
        let indexed_count =
            event.parameters.iter().filter(|&&id| self.gcx.hir.variable(id).indexed).count();
        if indexed_count > max_indexed {
            if self.invalid_event_topics.insert(event_id) {
                self.gcx
                    .dcx()
                    .err(format!("event cannot have more than {max_indexed} indexed parameters"))
                    .span(event.span)
                    .emit();
            }
            return Some(());
        }
        if args.len() != event.parameters.len() {
            return report_unsupported(self.gcx, args.span, "event arguments");
        }

        let parameter_names = self.gcx.callable_param_names(CallableParamSource::Event(event_id));
        let mut topics = Vec::with_capacity(indexed_count + usize::from(!event.anonymous));
        if !event.anonymous {
            topics.push(
                self.builder
                    .imm_u256(U256::from_be_slice(self.gcx.event_selector(event_id).as_slice())),
            );
        }
        let mut data_values = Vec::new();
        let mut data_types = Vec::new();
        for (index, &parameter) in event.parameters.iter().enumerate() {
            let Some(argument) =
                args.argument_for_parameter(index, Some(parameter_names.as_slice()))
            else {
                return report_unsupported(self.gcx, args.span, "event argument");
            };
            let parameter_ty = self.gcx.type_of_item(parameter.into());
            let variable = self.gcx.hir.variable(parameter);
            let mut value = self.lower_expr(argument)?;
            if variable.indexed {
                match parameter_ty.peel_refs().kind {
                    TyKind::Elementary(
                        solar_sema::hir::ElementaryType::Bytes
                        | solar_sema::hir::ElementaryType::String,
                    ) => {
                        if matches!(self.builder.func().value_ty(value), Some(MirType::Slice(_))) {
                            value = self.materialize_memory_slice(value);
                        }
                        topics.push(self.builder.keccak256_bytes(value));
                    }
                    TyKind::Struct(_)
                    | TyKind::Array(..)
                    | TyKind::DynArray(_)
                    | TyKind::Slice(_)
                    | TyKind::Tuple(_) => {
                        let mut abi_type = self.types.abi_type(parameter_ty)?;
                        abi_type = self.abi_type_for_value(value, abi_type);
                        if abi_type.is_dynamic() {
                            if let Some(packed) = self.lower_packed_word_array(parameter_ty, value)
                            {
                                topics.push(self.builder.keccak256_bytes(packed));
                                continue;
                            }
                            self.gcx
                                .dcx()
                                .err(
                                    "codegen does not support indexed event aggregate encoding yet",
                                )
                                .span(argument.span)
                                .emit();
                            return Some(());
                        }
                        let layout = Arc::new(AbiLayout::new(vec![abi_type].into_boxed_slice()));
                        let encoded = self.builder.abi_encode(layout, None, [value]);
                        let pointer = self.builder.slice_ptr(encoded);
                        let length = self.builder.slice_len(encoded);
                        topics.push(self.builder.keccak256(pointer, length));
                    }
                    _ => topics.push(self.lower_word_value(parameter_ty, argument, value)),
                }
            } else {
                let mut abi_type = self.types.abi_type(parameter_ty)?;
                abi_type = self.abi_type_for_value(value, abi_type);
                if self.needs_calldata_materialization(value, &abi_type) {
                    value =
                        self.materialize_calldata_argument(parameter_ty, value, argument.span)?;
                    abi_type = Self::memory_abi_type(abi_type);
                }
                if matches!(abi_type, AbiType::Word) {
                    value = self.lower_word_value(parameter_ty, argument, value);
                }
                data_values.push(value);
                data_types.push(abi_type);
            }
        }

        let (data_ptr, data_size) = if data_types.is_empty() {
            let zero = self.builder.imm_u256(U256::ZERO);
            (zero, zero)
        } else {
            let layout = Arc::new(AbiLayout::new(data_types.into_boxed_slice()));
            let encoded = self.builder.abi_encode(layout, None, data_values.into_boxed_slice());
            (self.builder.slice_ptr(encoded), self.builder.slice_len(encoded))
        };
        match topics.as_slice() {
            [] => self.builder.log0(data_ptr, data_size),
            &[topic] => self.builder.log1(data_ptr, data_size, topic),
            &[topic1, topic2] => self.builder.log2(data_ptr, data_size, topic1, topic2),
            &[topic1, topic2, topic3] => {
                self.builder.log3(data_ptr, data_size, topic1, topic2, topic3)
            }
            &[topic1, topic2, topic3, topic4] => {
                self.builder.log4(data_ptr, data_size, topic1, topic2, topic3, topic4)
            }
            _ => return report_unsupported(self.gcx, args.span, "event topics"),
        }
        Some(())
    }

    fn lower_word_value(&mut self, ty: Ty<'gcx>, expr: &hir::Expr<'_>, value: ValueId) -> ValueId {
        let expr = expr.peel_parens();
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

    fn lower_modifier_chain(
        &mut self,
        modifiers: &'gcx [hir::Modifier<'gcx>],
        body: hir::Block<'gcx>,
    ) -> Option<()> {
        self.lower_modifier_at(modifiers, body, 0)
    }

    fn lower_modifier_at(
        &mut self,
        modifiers: &'gcx [hir::Modifier<'gcx>],
        body: hir::Block<'gcx>,
        index: usize,
    ) -> Option<()> {
        let Some(modifier) = modifiers.get(index) else {
            return self.lower_block(body);
        };
        if modifier.id.as_contract().is_some() {
            return self.lower_modifier_at(modifiers, body, index + 1);
        }
        let hir::ItemId::Function(modifier_id) = modifier.id else {
            return report_unsupported(self.gcx, modifier.span, "base constructor modifier");
        };
        let modifier_id = self.gcx.resolve_virtual_function(self.contract_id, modifier_id);
        let modifier_function = self.gcx.hir.function(modifier_id);
        if modifier_function.kind == hir::FunctionKind::Constructor {
            return self.lower_modifier_at(modifiers, body, index + 1);
        }
        if !modifier_function.kind.is_modifier() {
            return report_unsupported(self.gcx, modifier.span, "modifier target");
        }
        let Some(modifier_body) = modifier_function.body else {
            return report_unsupported(self.gcx, modifier.span, "modifier body");
        };
        if modifier.args.len() != modifier_function.parameters.len() {
            return report_unsupported(self.gcx, modifier.span, "modifier argument list");
        }
        let parameter_names = self.gcx.callable_param_names(CallableParamSource::Function {
            id: modifier_id,
            skips_receiver: false,
        });
        let mut saved_parameters = Vec::with_capacity(modifier_function.parameters.len());
        let mut saved_storage_parameters = Vec::new();
        for (index, &parameter) in modifier_function.parameters.iter().enumerate() {
            let Some(argument) =
                modifier.args.argument_for_parameter(index, Some(parameter_names.as_slice()))
            else {
                return report_unsupported(self.gcx, modifier.span, "named modifier argument");
            };
            let parameter_ty = self.gcx.type_of_item(parameter.into());
            if Self::is_storage_parameter(parameter_ty) {
                let access = self.storage_access(argument)?;
                saved_storage_parameters
                    .push((parameter, self.storage_refs.insert(parameter, access)));
            } else {
                let value = self.lower_expr(argument)?;
                saved_parameters.push((parameter, self.values.insert(parameter, value)));
            }
        }

        self.modifiers.push(ModifierContext { modifiers, body, next: index + 1 });
        let result = self.lower_block(modifier_body);
        self.modifiers.pop();
        for (parameter, previous) in saved_parameters {
            if let Some(value) = previous {
                self.values.insert(parameter, value);
            } else {
                self.values.remove(&parameter);
            }
        }
        for (parameter, previous) in saved_storage_parameters {
            if let Some(access) = previous {
                self.storage_refs.insert(parameter, access);
            } else {
                self.storage_refs.remove(&parameter);
            }
        }
        result
    }

    fn lower_base_constructor(
        &mut self,
        modifier: &hir::Modifier<'_>,
        constructor_id: hir::FunctionId,
        constructor: &'gcx hir::Function<'gcx>,
        lowered: &mut FxHashSet<hir::ContractId>,
    ) -> Option<()> {
        let Some(body) = constructor.body else {
            return report_unsupported(self.gcx, modifier.span, "base constructor body");
        };
        if modifier.args.len() != constructor.parameters.len() {
            return report_unsupported(self.gcx, modifier.span, "base constructor arguments");
        }
        let parameter_names = self.gcx.callable_param_names(CallableParamSource::Function {
            id: constructor_id,
            skips_receiver: false,
        });
        let mut saved_parameters = Vec::with_capacity(constructor.parameters.len());
        for (index, &parameter) in constructor.parameters.iter().enumerate() {
            let Some(argument) =
                modifier.args.argument_for_parameter(index, Some(parameter_names.as_slice()))
            else {
                return report_unsupported(
                    self.gcx,
                    modifier.span,
                    "named base constructor argument",
                );
            };
            let value = self.lower_expr(argument)?;
            saved_parameters.push((parameter, self.values.insert(parameter, value)));
        }

        let continuation = self.builder.create_block();
        self.return_targets.push(continuation);
        if let Some(contract_id) = constructor.contract {
            self.lower_implicit_base_constructors_inner(contract_id, lowered)?;
        }
        let result = self.lower_function_body(constructor.modifiers, body);
        self.return_targets.pop();
        result?;
        if !self.is_terminated() {
            self.builder.jump(continuation);
        }
        self.builder.switch_to_block(continuation);
        for (parameter, previous) in saved_parameters {
            if let Some(value) = previous {
                self.values.insert(parameter, value);
            } else {
                self.values.remove(&parameter);
            }
        }
        Some(())
    }

    fn lower_modifier_placeholder(&mut self, span: Span) -> Option<()> {
        let Some(context) = self.modifiers.last().copied() else {
            return report_unsupported(self.gcx, span, "modifier placeholder");
        };
        let continuation = self.builder.create_block();
        self.return_targets.push(continuation);
        let result = self.lower_modifier_at(context.modifiers, context.body, context.next);
        self.return_targets.pop();
        result?;
        if !self.is_terminated() {
            self.builder.jump(continuation);
        }
        self.builder.switch_to_block(continuation);
        Some(())
    }

    fn lower_if(
        &mut self,
        condition: &hir::Expr<'_>,
        then_stmt: &hir::Stmt<'_>,
        else_stmt: Option<&hir::Stmt<'_>>,
    ) -> Option<()> {
        let condition = self.lower_expr(condition)?;
        let then_block = self.builder.create_block();
        let else_block = self.builder.create_block();
        let merge_block = self.builder.create_block();
        let before = self.values.clone();
        let before_storage_refs = self.storage_refs.clone();
        self.builder.branch(condition, then_block, else_block);

        self.builder.switch_to_block(then_block);
        self.lower_stmt(then_stmt)?;
        let then_terminated = self.is_terminated();
        let then_exit = self.builder.current_block();
        let then_values = self.values.clone();
        let then_storage_refs = self.storage_refs.clone();
        if !then_terminated {
            self.builder.jump(merge_block);
        }

        self.values = before.clone();
        self.storage_refs = before_storage_refs.clone();
        self.builder.switch_to_block(else_block);
        if let Some(stmt) = else_stmt {
            self.lower_stmt(stmt)?;
        }
        let else_terminated = self.is_terminated();
        let else_exit = self.builder.current_block();
        let else_values = self.values.clone();
        let else_storage_refs = self.storage_refs.clone();
        if !else_terminated {
            self.builder.jump(merge_block);
        }

        self.builder.switch_to_block(merge_block);
        self.values = self.merge_values(
            before,
            then_exit,
            then_values,
            then_terminated,
            else_exit,
            else_values,
            else_terminated,
        );
        self.storage_refs = self.merge_storage_refs(
            before_storage_refs,
            then_exit,
            then_storage_refs,
            then_terminated,
            else_exit,
            else_storage_refs,
            else_terminated,
        );
        if then_terminated && else_terminated {
            self.builder.invalid();
        }
        Some(())
    }

    fn lower_switch(&mut self, switch: &hir::StmtSwitch<'_>) -> Option<()> {
        let selector = self.lower_yul_word_expr(switch.selector)?;
        let switch_block = self.builder.current_block();
        let merge_block = self.builder.create_block();
        let before_values = self.values.clone();
        let before_storage_refs = self.storage_refs.clone();
        let mut case_blocks = Vec::new();
        let mut body_blocks = Vec::new();
        let mut default_block = merge_block;
        let mut has_default = false;

        for case in switch.cases {
            let block = self.builder.create_block();
            if let Some(constant) = case.constant {
                let value = self.lower_yul_word_literal(constant)?;
                case_blocks.push((value, block));
            } else {
                default_block = block;
                has_default = true;
            }
            body_blocks.push((case, block));
        }
        self.builder.switch(selector, default_block, case_blocks);

        let mut states = Vec::with_capacity(body_blocks.len() + usize::from(!has_default));
        if !has_default {
            states.push(LoopState {
                block: switch_block,
                values: before_values.clone(),
                storage_refs: before_storage_refs.clone(),
            });
        }
        for (case, block) in body_blocks {
            self.values = before_values.clone();
            self.storage_refs = before_storage_refs.clone();
            self.builder.switch_to_block(block);
            self.lower_block(case.body)?;
            let terminated = self.is_terminated();
            let exit = self.builder.current_block();
            if !terminated {
                self.builder.jump(merge_block);
                states.push(LoopState {
                    block: exit,
                    values: self.values.clone(),
                    storage_refs: self.storage_refs.clone(),
                });
            }
        }

        self.builder.switch_to_block(merge_block);
        self.values = self.merge_loop_values(before_values, &states, &FxHashMap::default());
        self.storage_refs = self.merge_loop_storage_refs(before_storage_refs, &states);
        Some(())
    }

    fn lower_try(&mut self, try_stmt: &hir::StmtTry<'_>) -> Option<()> {
        let ExprKind::Call(callee, args, call_opts) = &try_stmt.expr.kind else {
            return report_unsupported(self.gcx, try_stmt.expr.span, "try expression");
        };
        let (
            parameter_types,
            return_types,
            parameter_names,
            selector,
            selector_bytes,
            address,
            receiver,
            static_call,
        ) = if let ExprKind::Member(receiver, _) = callee.kind {
            let Some(function_id) = self.gcx.resolved_function(callee) else {
                return report_unsupported(self.gcx, try_stmt.expr.span, "try target");
            };
            let function = self.gcx.hir.function(function_id);
            (
                function
                    .parameters
                    .iter()
                    .map(|&parameter| self.gcx.type_of_item(parameter.into()))
                    .collect::<Vec<_>>(),
                function
                    .returns
                    .iter()
                    .map(|&return_id| self.gcx.type_of_item(return_id.into()))
                    .collect::<Vec<_>>(),
                Some(self.gcx.callable_param_names(CallableParamSource::Function {
                    id: function_id,
                    skips_receiver: false,
                })),
                None,
                Some(self.gcx.function_selector(function_id).0),
                None,
                Some(receiver),
                matches!(
                    function.state_mutability,
                    hir::StateMutability::Pure | hir::StateMutability::View
                ) && self.gcx.sess.opts.evm_version.has_static_call(),
            )
        } else if let Some(TyKind::Fn(function)) =
            self.gcx.type_of_expr(callee.id).map(|ty| ty.kind)
            && function.is_external()
            && function.function_id.is_none()
        {
            let function_value = self.lower_expr(callee)?;
            let selector_mask = self.builder.imm_u256(U256::from(u32::MAX));
            let selector = self.builder.and(function_value, selector_mask);
            let selector_shift = self.builder.imm_u64(224);
            let selector = self.builder.shl(selector_shift, selector);
            let address_shift = self.builder.imm_u64(32);
            let address = self.builder.shr(address_shift, function_value);
            (
                function.parameters.to_vec(),
                function.returns.to_vec(),
                None,
                Some(selector),
                None,
                Some(address),
                None,
                matches!(
                    function.state_mutability,
                    hir::StateMutability::Pure | hir::StateMutability::View
                ) && self.gcx.sess.opts.evm_version.has_static_call(),
            )
        } else {
            return report_unsupported(self.gcx, try_stmt.expr.span, "try target");
        };
        let Some((returns_clause, catch_clauses)) = try_stmt.clauses.split_first() else {
            return report_unsupported(self.gcx, try_stmt.expr.span, "try/catch clauses");
        };
        if catch_clauses.is_empty() {
            return report_unsupported(self.gcx, try_stmt.expr.span, "try/catch clauses");
        }
        if returns_clause.name.is_some() || returns_clause.args.len() != return_types.len() {
            return report_unsupported(self.gcx, returns_clause.span, "try return bindings");
        }
        for catch_clause in catch_clauses {
            let catch_error = catch_clause.name.is_some_and(|name| name.name == sym::Error);
            let catch_panic = catch_clause.name.is_some_and(|name| name.name == sym::Panic);
            if catch_clause.name.is_some() && !catch_error && !catch_panic {
                return report_unsupported(self.gcx, catch_clause.span, "try catch clause");
            }
            if catch_clause.args.len() > 1 {
                return report_unsupported(self.gcx, catch_clause.span, "try catch clause");
            }
            if let Some(&binding) = catch_clause.args.first() {
                let ty = self.gcx.type_of_item(binding.into());
                let expected = if catch_error {
                    TyKind::Elementary(solar_sema::hir::ElementaryType::String)
                } else if catch_panic {
                    TyKind::Elementary(solar_sema::hir::ElementaryType::UInt(
                        solar_ast::TypeSize::new_int_bits(256),
                    ))
                } else {
                    TyKind::Elementary(solar_sema::hir::ElementaryType::Bytes)
                };
                if ty.peel_refs().kind != expected {
                    return report_unsupported(self.gcx, catch_clause.span, "try catch clause");
                }
                if !self.gcx.sess.opts.evm_version.supports_returndata() {
                    return report_error(
                        self.gcx,
                        try_stmt.expr.span,
                        "codegen cannot bind try/catch returndata before Byzantium",
                    );
                }
            }
        }
        if args.len() != parameter_types.len() {
            return report_unsupported(self.gcx, args.span, "try arguments");
        }

        let mut values = Vec::with_capacity(parameter_types.len());
        let mut types = Vec::with_capacity(parameter_types.len());
        for (index, parameter_ty) in parameter_types.iter().copied().enumerate() {
            let Some(argument) = args.argument_for_parameter(index, parameter_names.as_deref())
            else {
                return report_unsupported(self.gcx, args.span, "try argument");
            };
            let mut value = self.lower_expr(argument)?;
            let mut abi_type = self.types.abi_type(parameter_ty)?;
            abi_type = self.abi_type_for_value(value, abi_type);
            if self.needs_calldata_materialization(value, &abi_type) {
                value = self.materialize_calldata_argument(parameter_ty, value, argument.span)?;
                abi_type = Self::memory_abi_type(abi_type);
            }
            values.push(value);
            types.push(abi_type);
        }
        let selector = if let Some(selector) = selector {
            selector
        } else {
            let Some(selector_bytes) = selector_bytes else {
                return report_unsupported(self.gcx, try_stmt.expr.span, "try selector");
            };
            self.builder.imm_u256(U256::from_be_slice(&selector_bytes) << 224)
        };
        let layout = Arc::new(AbiLayout::new(types.into_boxed_slice()));
        let encoded = self.builder.abi_encode(layout, Some(selector), values.into_boxed_slice());
        let input = self.builder.slice_ptr(encoded);
        let input_size = self.builder.slice_len(encoded);
        let address = if let Some(address) = address {
            address
        } else {
            let Some(receiver) = receiver else {
                return report_unsupported(self.gcx, try_stmt.expr.span, "try receiver");
            };
            self.lower_expr(receiver)?
        };
        let zero = self.builder.imm_u256(U256::ZERO);
        let mut call_value = zero;
        let mut gas = self.builder.gas();
        if let Some(options) = call_opts {
            for option in options.args {
                let value = self.lower_expr(&option.value)?;
                match option.name.name {
                    kw::Gas => gas = value,
                    sym::value => call_value = value,
                    _ => return report_unsupported(self.gcx, option.name.span, "try call option"),
                }
            }
        }
        let ret_offset = zero;
        let ret_size = self.builder.imm_u64(0);
        let success = if static_call {
            self.builder.staticcall(gas, address, input, input_size, ret_offset, ret_size)
        } else {
            self.builder.call(gas, address, call_value, input, input_size, ret_offset, ret_size)
        };

        let success_block = self.builder.create_block();
        let catch_block = self.builder.create_block();
        let merge_block = self.builder.create_block();
        let before = self.values.clone();
        let before_storage_refs = self.storage_refs.clone();
        self.builder.branch(success, success_block, catch_block);

        self.values = before.clone();
        self.storage_refs = before_storage_refs.clone();
        self.builder.switch_to_block(success_block);
        if !return_types.is_empty() {
            let data = self.materialize_returndata_bytes();
            let return_types = return_types
                .iter()
                .copied()
                .map(|ty| ty.with_loc_if_ref(self.gcx, DataLocation::Memory))
                .collect::<Vec<_>>();
            let values = self.lower_abi_decode_values(data, &return_types, returns_clause.span)?;
            for (&binding, value) in returns_clause.args.iter().zip(values) {
                self.values.insert(binding, value);
            }
        }
        self.lower_block(returns_clause.block)?;
        let success_terminated = self.is_terminated();
        let success_exit = self.builder.current_block();
        let success_values = self.values.clone();
        let success_storage_refs = self.storage_refs.clone();
        if !success_terminated {
            self.builder.jump(merge_block);
        }

        self.values = before.clone();
        self.storage_refs = before_storage_refs.clone();
        self.builder.switch_to_block(catch_block);
        let catch_data = self.materialize_returndata_bytes();
        let catch_data_ptr = self.builder.memory_object_data(catch_data, MemoryObjectKind::Bytes);
        let catch_data_len = self.builder.memory_object_len(catch_data, MemoryObjectKind::Bytes);
        let zero = self.builder.imm_u256(U256::ZERO);
        let selector_slice =
            self.builder.make_slice(catch_data_ptr, catch_data_len, SliceLocation::Memory);
        let selector_word = self.builder.memory_slice_load_word(selector_slice, zero);
        let four = self.builder.imm_u64(4);
        let selector_short = self.builder.lt(catch_data_len, four);
        let has_selector = self.builder.iszero(selector_short);
        let selector_shift = self.builder.imm_u64(224);
        let selector = self.builder.shr(selector_shift, selector_word);
        let error_selector =
            self.builder.imm_u256(U256::from_be_slice(&keccak256("Error(string)")[..4]));
        let panic_selector = self.builder.imm_u256(U256::from(0x4e48_7b71_u64));
        let error_selector_matches = self.builder.eq(selector, error_selector);
        let error_matches = self.builder.and(has_selector, error_selector_matches);
        let panic_size = self.builder.imm_u64(36);
        let panic_short = self.builder.lt(catch_data_len, panic_size);
        let panic_has_payload = self.builder.iszero(panic_short);
        let panic_selector_matches = self.builder.eq(selector, panic_selector);
        let panic_matches = self.builder.and(panic_has_payload, panic_selector_matches);
        let mut catch_states = Vec::with_capacity(catch_clauses.len());
        let mut next_catch = self.builder.current_block();
        for catch_clause in catch_clauses {
            self.builder.switch_to_block(next_catch);
            let clause_block = self.builder.create_block();
            let next_block = self.builder.create_block();
            let catch_error = catch_clause.name.is_some_and(|name| name.name == sym::Error);
            let catch_panic = catch_clause.name.is_some_and(|name| name.name == sym::Panic);
            let condition = if catch_error {
                error_matches
            } else if catch_panic {
                panic_matches
            } else {
                self.builder.imm_bool(true)
            };
            self.builder.branch(condition, clause_block, next_block);

            self.values = before.clone();
            self.storage_refs = before_storage_refs.clone();
            self.builder.switch_to_block(clause_block);
            if let Some(&binding) = catch_clause.args.first() {
                let value = if catch_error {
                    self.lower_error_catch_string(catch_data, try_stmt.expr.span)?
                } else if catch_panic {
                    self.lower_panic_catch_word(catch_data)
                } else {
                    catch_data
                };
                self.values.insert(binding, value);
            }
            self.lower_block(catch_clause.block)?;
            let catch_terminated = self.is_terminated();
            let catch_exit = self.builder.current_block();
            if !catch_terminated {
                self.builder.jump(merge_block);
                catch_states.push(LoopState {
                    block: catch_exit,
                    values: self.values.clone(),
                    storage_refs: self.storage_refs.clone(),
                });
            }
            next_catch = next_block;
        }
        self.builder.switch_to_block(next_catch);
        self.builder.revert(catch_data_ptr, catch_data_len);

        let mut states = Vec::with_capacity(catch_states.len() + 1);
        if !success_terminated {
            states.push(LoopState {
                block: success_exit,
                values: success_values,
                storage_refs: success_storage_refs,
            });
        }
        states.extend(catch_states);
        self.builder.switch_to_block(merge_block);
        self.values = self.merge_many_values(before, &states);
        self.storage_refs = self.merge_many_storage_refs(before_storage_refs, &states);
        Some(())
    }

    fn lower_yul_word_literal(&mut self, lit: &hir::Lit<'_>) -> Option<ValueId> {
        if let LitKind::Str(_, bytes, _) = &lit.kind {
            let bytes = bytes.as_byte_str();
            if bytes.len() > 32 {
                return report_unsupported(self.gcx, lit.span, "switch literal");
            }
            return Some(
                self.builder.imm_u256(U256::from_be_slice(bytes) << ((32 - bytes.len()) * 8)),
            );
        }
        if let LitKind::Bool(value) = lit.kind {
            return Some(self.builder.imm_u256(if value { U256::ONE } else { U256::ZERO }));
        }
        if let LitKind::Address(value) = lit.kind {
            return Some(self.builder.imm_u256(U256::from_be_slice(value.as_slice())));
        }
        self.lower_literal(lit.kind, lit.span)
    }

    fn lower_yul_word_expr(&mut self, expr: &hir::Expr<'_>) -> Option<ValueId> {
        if let ExprKind::Lit(lit) = expr.peel_parens().kind {
            return self.lower_yul_word_literal(lit);
        }
        self.lower_expr(expr)
    }

    fn merge_storage_refs(
        &mut self,
        before: FxHashMap<VariableId, StorageAccess>,
        then_exit: BlockId,
        then_values: FxHashMap<VariableId, StorageAccess>,
        then_terminated: bool,
        else_exit: BlockId,
        else_values: FxHashMap<VariableId, StorageAccess>,
        else_terminated: bool,
    ) -> FxHashMap<VariableId, StorageAccess> {
        let mut merged = FxHashMap::default();
        let ids = before
            .keys()
            .chain(then_values.keys())
            .chain(else_values.keys())
            .copied()
            .collect::<solar_data_structures::map::FxHashSet<_>>();
        for id in ids {
            let then = then_values.get(&id).copied().or_else(|| before.get(&id).copied());
            let else_ = else_values.get(&id).copied().or_else(|| before.get(&id).copied());
            let mut incoming = Vec::with_capacity(2);
            if !then_terminated && let Some(access) = then {
                incoming.push((then_exit, access));
            }
            if !else_terminated && let Some(access) = else_ {
                incoming.push((else_exit, access));
            }
            let access = self.merge_storage_accesses(incoming).or(then.or(else_));
            if let Some(access) = access {
                merged.insert(id, access);
            }
        }
        merged
    }

    fn lower_ternary(
        &mut self,
        condition: &hir::Expr<'_>,
        then_expr: &hir::Expr<'_>,
        else_expr: &hir::Expr<'_>,
    ) -> Option<ValueId> {
        let condition = self.lower_expr(condition)?;
        let then_block = self.builder.create_block();
        let else_block = self.builder.create_block();
        let merge_block = self.builder.create_block();
        let before = self.values.clone();
        self.builder.branch(condition, then_block, else_block);

        self.builder.switch_to_block(then_block);
        let then_value = self.lower_expr(then_expr)?;
        let then_terminated = self.is_terminated();
        let then_exit = self.builder.current_block();
        let then_values = self.values.clone();
        if !then_terminated {
            self.builder.jump(merge_block);
        }

        self.values = before.clone();
        self.builder.switch_to_block(else_block);
        let else_value = self.lower_expr(else_expr)?;
        let else_terminated = self.is_terminated();
        let else_exit = self.builder.current_block();
        let else_values = self.values.clone();
        if !else_terminated {
            self.builder.jump(merge_block);
        }

        self.builder.switch_to_block(merge_block);
        self.values = self.merge_values(
            before,
            then_exit,
            then_values,
            then_terminated,
            else_exit,
            else_values,
            else_terminated,
        );
        match (then_terminated, else_terminated) {
            (true, false) => Some(else_value),
            (false, true) => Some(then_value),
            _ if then_value == else_value => Some(then_value),
            _ => Some(self.builder.phi(vec![(then_exit, then_value), (else_exit, else_value)])),
        }
    }

    fn lower_ternary_values(
        &mut self,
        condition: &hir::Expr<'_>,
        then_expr: &hir::Expr<'_>,
        else_expr: &hir::Expr<'_>,
    ) -> Option<Vec<ValueId>> {
        let condition = self.lower_expr(condition)?;
        let then_block = self.builder.create_block();
        let else_block = self.builder.create_block();
        let merge_block = self.builder.create_block();
        let before = self.values.clone();
        self.builder.branch(condition, then_block, else_block);

        self.builder.switch_to_block(then_block);
        let then_result = self.lower_values(then_expr)?;
        let then_terminated = self.is_terminated();
        let then_exit = self.builder.current_block();
        let then_values = self.values.clone();
        if !then_terminated {
            self.builder.jump(merge_block);
        }

        self.values = before.clone();
        self.builder.switch_to_block(else_block);
        let else_result = self.lower_values(else_expr)?;
        let else_terminated = self.is_terminated();
        let else_exit = self.builder.current_block();
        let else_values = self.values.clone();
        if !else_terminated {
            self.builder.jump(merge_block);
        }

        self.builder.switch_to_block(merge_block);
        self.values = self.merge_values(
            before,
            then_exit,
            then_values,
            then_terminated,
            else_exit,
            else_values,
            else_terminated,
        );
        if !then_terminated && !else_terminated && then_result.len() != else_result.len() {
            return report_unsupported(self.gcx, then_expr.span, "ternary value count");
        }
        let values = match (then_terminated, else_terminated) {
            (true, false) => else_result,
            (false, true) => then_result,
            (true, true) => Vec::new(),
            (false, false) => then_result
                .into_iter()
                .zip(else_result)
                .map(|(then, else_)| {
                    if then == else_ {
                        then
                    } else {
                        self.builder.phi(vec![(then_exit, then), (else_exit, else_)])
                    }
                })
                .collect(),
        };
        Some(values)
    }

    fn lower_loop(&mut self, block: hir::Block<'_>, source: LoopSource<'_>) -> Option<()> {
        let update_stmt = match source {
            LoopSource::For { update } => update,
            LoopSource::While | LoopSource::DoWhile => None,
        };
        let preheader = self.builder.current_block();
        let header = self.builder.create_block();
        let exit = self.builder.create_block();
        let update = update_stmt.map(|_| self.builder.create_block());
        self.builder.jump(header);
        self.builder.switch_to_block(header);
        let before_values = self.values.clone();
        let before_storage_refs = self.storage_refs.clone();
        let mut header_values = before_values.clone();
        let mut header_phis = FxHashMap::default();
        for (&id, &value) in &before_values {
            let phi = self.builder.phi(vec![(preheader, value)]);
            header_values.insert(id, phi);
            header_phis.insert(id, phi);
        }
        self.values = header_values.clone();
        let mut header_storage_refs = before_storage_refs.clone();
        for (&id, &access) in &before_storage_refs {
            let slot = self.builder.phi(vec![(preheader, access.slot)]);
            let offset = access.offset.map(|offset| self.builder.phi(vec![(preheader, offset)]));
            header_storage_refs.insert(id, StorageAccess { slot, offset, ..access });
        }
        self.storage_refs = header_storage_refs.clone();
        self.loops.push(LoopTargets {
            break_block: exit,
            continue_block: update.unwrap_or(header),
            break_states: Vec::new(),
            continue_states: Vec::new(),
        });
        let update_state = if let Some(update_stmt) = update_stmt {
            self.lower_block(block)?;
            let normal_state = (!self.is_terminated()).then(|| LoopState {
                block: self.builder.current_block(),
                values: self.values.clone(),
                storage_refs: self.storage_refs.clone(),
            });
            if normal_state.is_some() {
                self.builder.jump(update.expect("for loop update block"));
            }

            let mut update_states = Vec::with_capacity(
                usize::from(normal_state.is_some())
                    + self.loops.last().expect("loop target exists").continue_states.len(),
            );
            if let Some(state) = normal_state {
                update_states.push(state);
            }
            update_states.extend(
                self.loops.last().expect("loop target exists").continue_states.iter().cloned(),
            );

            if update_states.is_empty() {
                let update = update.expect("for loop update block");
                self.builder.switch_to_block(update);
                self.builder.invalid();
                None
            } else {
                self.builder.switch_to_block(update.expect("for loop update block"));
                self.values = self.merge_loop_values(
                    header_values.clone(),
                    &update_states,
                    &FxHashMap::default(),
                );
                self.storage_refs =
                    self.merge_loop_storage_refs(header_storage_refs.clone(), &update_states);
                self.lower_stmt(update_stmt)?;
                let update_state = (!self.is_terminated()).then(|| LoopState {
                    block: self.builder.current_block(),
                    values: self.values.clone(),
                    storage_refs: self.storage_refs.clone(),
                });
                if update_state.is_some() {
                    self.builder.jump(header);
                }
                update_state
            }
        } else {
            self.lower_block(block)?;
            let normal_state = (!self.is_terminated()).then(|| LoopState {
                block: self.builder.current_block(),
                values: self.values.clone(),
                storage_refs: self.storage_refs.clone(),
            });
            if normal_state.is_some() {
                self.builder.jump(header);
            }
            normal_state
        };
        let loop_targets = self.loops.pop().expect("loop target exists");
        if let Some(state) = &update_state {
            self.add_loop_phi_incoming(&header_phis, state);
            self.add_loop_storage_phi_incoming(&header_storage_refs, state);
        } else if update_stmt.is_none() {
            for state in &loop_targets.continue_states {
                self.add_loop_phi_incoming(&header_phis, state);
                self.add_loop_storage_phi_incoming(&header_storage_refs, state);
            }
        }
        self.builder.switch_to_block(exit);
        self.values =
            self.merge_loop_values(before_values, &loop_targets.break_states, &header_phis);
        self.storage_refs =
            self.merge_loop_storage_refs(header_storage_refs, &loop_targets.break_states);
        Some(())
    }

    fn add_loop_phi_incoming(
        &mut self,
        header_phis: &FxHashMap<VariableId, ValueId>,
        state: &LoopState,
    ) {
        for (&id, &phi) in header_phis {
            let value = state.values.get(&id).copied().unwrap_or(phi);
            self.builder.add_phi_incoming(phi, state.block, value);
        }
    }

    fn add_loop_storage_phi_incoming(
        &mut self,
        header_refs: &FxHashMap<VariableId, StorageAccess>,
        state: &LoopState,
    ) {
        for (&id, &header) in header_refs {
            let access = state.storage_refs.get(&id).copied().unwrap_or(header);
            self.builder.add_phi_incoming(header.slot, state.block, access.slot);
            if let Some(offset) = header.offset {
                self.builder.add_phi_incoming(offset, state.block, access.offset.unwrap_or(offset));
            }
        }
    }

    fn merge_loop_values(
        &mut self,
        before: FxHashMap<VariableId, ValueId>,
        exits: &[LoopState],
        header_phis: &FxHashMap<VariableId, ValueId>,
    ) -> FxHashMap<VariableId, ValueId> {
        let mut merged = before.clone();
        for &id in before.keys() {
            let incoming =
                exits.iter().filter_map(|state| state.values.get(&id).copied()).collect::<Vec<_>>();
            let value = match incoming.as_slice() {
                [] => header_phis.get(&id).copied().or_else(|| before.get(&id).copied()),
                [value] => Some(*value),
                [first, rest @ ..] if rest.iter().all(|value| value == first) => Some(*first),
                _ => Some(
                    self.builder.phi(
                        exits
                            .iter()
                            .filter_map(|state| {
                                state.values.get(&id).copied().map(|value| (state.block, value))
                            })
                            .collect(),
                    ),
                ),
            };
            if let Some(value) = value {
                merged.insert(id, value);
            }
        }
        merged
    }

    fn merge_loop_storage_refs(
        &mut self,
        mut before: FxHashMap<VariableId, StorageAccess>,
        exits: &[LoopState],
    ) -> FxHashMap<VariableId, StorageAccess> {
        let ids = before
            .keys()
            .chain(exits.iter().flat_map(|state| state.storage_refs.keys()))
            .copied()
            .collect::<solar_data_structures::map::FxHashSet<_>>();
        for id in ids {
            let fallback = before.get(&id).copied();
            let incoming = exits
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

    fn merge_storage_accesses(
        &mut self,
        incoming: Vec<(BlockId, StorageAccess)>,
    ) -> Option<StorageAccess> {
        let first = incoming.first().map(|&(_, access)| access)?;
        if incoming.iter().all(|&(_, access)| access == first) {
            return Some(first);
        }

        let slot = if incoming.iter().all(|&(_, access)| access.slot == first.slot) {
            first.slot
        } else {
            self.builder.phi(incoming.iter().map(|&(block, access)| (block, access.slot)).collect())
        };

        let offset = if incoming.iter().all(|&(_, access)| access.offset.is_none()) {
            None
        } else {
            let offsets = incoming
                .iter()
                .map(|&(_, access)| {
                    access
                        .offset
                        .unwrap_or_else(|| self.builder.imm_u64(u64::from(access.location.offset)))
                })
                .collect::<Vec<_>>();
            let first_offset = offsets[0];
            if offsets.iter().all(|&offset| offset == first_offset) {
                Some(first_offset)
            } else {
                Some(
                    self.builder.phi(
                        incoming
                            .iter()
                            .zip(offsets)
                            .map(|(&(block, _), offset)| (block, offset))
                            .collect(),
                    ),
                )
            }
        };
        Some(StorageAccess { slot, location: first.location, offset })
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
        if let ExprKind::Call(callee, args, ..) = &expr.kind {
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
                    values.push(self.load_multi_return_value(base, index));
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
                return self.lower_external_function_pointer_call_values(callee, function, *args);
            }
            if let Some(returns) = returns
                && returns > 1
            {
                let first = self.lower_expr(expr)?;
                let base = self.multi_return_buffer_base();
                let mut values = Vec::with_capacity(returns);
                values.push(first);
                for index in 1..returns {
                    values.push(self.load_multi_return_value(base, index));
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

    fn lower_tuple_assignment(
        &mut self,
        elements: &[Option<&hir::Expr<'_>>],
        rhs: &hir::Expr<'_>,
    ) -> Option<()> {
        let rhs = rhs.peel_parens();
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
            for (index, value) in rhs_elements.iter().enumerate() {
                let Some(value) = value else {
                    if elements.get(index).is_some_and(Option::is_some) {
                        return report_unsupported(self.gcx, rhs.span, "tuple assignment value");
                    }
                    continue;
                };
                let value = self.lower_expr(value)?;
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

    fn multi_return_buffer_base(&mut self) -> ValueId {
        let slot = self.builder.imm_u64(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT);
        self.builder.mload(slot)
    }

    fn ensure_multi_return_buffer(&mut self, words: usize) -> ValueId {
        debug_assert!(words > 1);
        let slot = self.builder.imm_u64(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT);
        // The published pointer has no capacity, so each producer gets a fresh object.
        let words = u64::try_from(words).unwrap_or(u64::MAX);
        let size = self.builder.imm_u64(words.saturating_mul(32));
        let layout = MemoryObjectLayout::FixedArray { len: words, element_words: 1 };
        let object = self.builder.alloc_object(size, layout, AllocationSemantics::INTERNAL);
        let base = self.builder.memory_object_data(object, MemoryObjectKind::FixedArray);
        self.builder.mstore(slot, base);
        base
    }

    fn load_multi_return_value(&mut self, base: ValueId, index: usize) -> ValueId {
        let offset = self.builder.imm_u64(u64::try_from(index).unwrap_or(u64::MAX) * 32);
        let address = self.builder.add(base, offset);
        self.builder.mload(address)
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
                    let old = self.load_lvalue(value)?;
                    let one = self.builder.imm_u256(U256::from(1));
                    let kind = if matches!(op.kind, UnOpKind::PreInc | UnOpKind::PostInc) {
                        BinOpKind::Add
                    } else {
                        BinOpKind::Sub
                    };
                    let new = self.binary(kind, old, one, self.gcx.type_of_expr(value.id));
                    self.store_lvalue(value, new)?;
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
                let rhs_value = self.lower_expr(rhs)?;
                let lhs_ty = self.type_of_expr_or_variable(lhs)?;
                let rhs_ty = self.gcx.type_of_expr(rhs.id).unwrap_or(lhs_ty);
                let preserve_calldata_slice = lhs_ty.is_ref_at(DataLocation::Calldata)
                    && matches!(
                        self.builder.func().value_ty(rhs_value),
                        Some(MirType::Slice(SliceLocation::Calldata))
                    );
                let value = if let Some(kind) = op.map(|op| op.kind) {
                    let lhs_value = self.load_lvalue(lhs)?;
                    self.binary(kind, lhs_value, rhs_value, Some(lhs_ty))
                } else if preserve_calldata_slice {
                    rhs_value
                } else {
                    self.materialize_memory_argument(lhs_ty, rhs_value, rhs.span)?
                };
                let value = self.coerce_value(value, rhs_ty, lhs_ty);
                self.store_lvalue(lhs, value)?;
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

    fn storage_access(&mut self, expr: &hir::Expr<'_>) -> Option<StorageAccess> {
        match &expr.peel_parens().kind {
            ExprKind::Ident(_) => {
                let id = self.gcx.resolved_variable(expr)?;
                if let Some(access) = self.storage_refs.get(&id).copied() {
                    return Some(access);
                }
                let var = self.gcx.hir.variable(id);
                if !var.is_state_variable() {
                    return None;
                }
                let location = self.storage.get(id)?;
                let slot = self.builder.imm_u256(location.slot);
                Some(StorageAccess { slot, location, offset: None })
            }
            ExprKind::Member(receiver, _) => {
                let id = self.gcx.resolved_variable(expr)?;
                let variable = self.gcx.hir.variable(id);
                let hir::ItemId::Struct(struct_id) = variable.parent? else { return None };
                let field =
                    self.gcx.hir.strukt(struct_id).fields.iter().position(|&field| field == id)?;
                let base = self.storage_access(receiver)?;
                let location = self.storage.field_location(self.gcx, struct_id, field)?;
                let slot = self.add_storage_offset(base.slot, location.slot);
                Some(StorageAccess { slot, location, offset: None })
            }
            ExprKind::Index(receiver, Some(index)) => {
                let base = self.storage_access(receiver)?;
                let ty = self.gcx.type_of_expr(receiver.id)?.peel_refs();
                let index_ty = self.gcx.type_of_expr(index.id)?;
                let index = self.lower_expr(index)?;
                if let TyKind::Mapping(_, value) = ty.kind {
                    let slot = self.mapping_slot(index, index_ty, base.slot);
                    if let Some((size, encoding)) = self.storage.packed_encoding(self.gcx, value) {
                        let location =
                            StorageLocation { slot: U256::ZERO, offset: 0, size, encoding };
                        return Some(StorageAccess { slot, location, offset: None });
                    }
                    return Some(StorageAccess {
                        slot,
                        location: StorageLocation::word(U256::ZERO),
                        offset: None,
                    });
                }
                let (element, dynamic, length) = match ty.kind {
                    TyKind::Array(element, len) => {
                        (element, false, self.builder.imm_u64(u64::try_from(len).ok()?))
                    }
                    TyKind::DynArray(element) => (element, true, self.builder.sload(base.slot)),
                    _ => return None,
                };
                self.bounds_check(index, length);
                self.storage_array_element_access(base.slot, index, element, dynamic)
            }
            ExprKind::Call(callee, arguments, _)
                if arguments.is_empty()
                    && self.gcx.resolved_builtin(callee) == Some(Builtin::ArrayPush0) =>
            {
                let ExprKind::Member(receiver, _) = &callee.kind else { return None };
                let (access, _, new_length, base_slot) =
                    self.storage_array_push_access(receiver)?;
                self.builder.sstore(base_slot, new_length);
                Some(access)
            }
            _ => None,
        }
    }

    fn is_storage_reference_binding(&self, expr: &hir::Expr<'_>) -> bool {
        if !matches!(expr.peel_parens().kind, ExprKind::Ident(_)) {
            return false;
        }
        self.gcx
            .resolved_variable(expr)
            .is_some_and(|id| !self.gcx.hir.variable(id).is_state_variable())
            && self.gcx.type_of_expr(expr.id).is_some_and(|ty| ty.is_ref_at(DataLocation::Storage))
    }

    fn storage_array_push_access(
        &mut self,
        receiver: &hir::Expr<'_>,
    ) -> Option<(StorageAccess, Ty<'gcx>, ValueId, ValueId)> {
        let base = self.storage_access(receiver)?;
        let receiver_ty = self.gcx.type_of_expr(receiver.id)?.peel_refs();
        let TyKind::DynArray(element) = receiver_ty.kind else { return None };
        let length = self.builder.sload(base.slot);
        let one = self.builder.imm_u64(1);
        let new_length = self.checked_add(length, one);
        let access = self.storage_array_element_access(base.slot, length, element, true)?;
        Some((access, element, new_length, base.slot))
    }

    fn mapping_slot(&mut self, key: ValueId, key_ty: Ty<'gcx>, slot: ValueId) -> ValueId {
        let is_calldata = key_ty.data_stored_in(DataLocation::Calldata)
            || matches!(key_ty.kind, TyKind::Slice(inner) if inner.data_stored_in(DataLocation::Calldata));
        let is_dynamic = matches!(
            key_ty.peel_refs().kind,
            TyKind::Elementary(
                solar_sema::hir::ElementaryType::String | solar_sema::hir::ElementaryType::Bytes
            ) | TyKind::Slice(_)
        );
        if is_dynamic {
            if is_calldata {
                self.builder.mapping_slot_calldata(key, slot)
            } else {
                self.builder.mapping_slot_memory(key, slot)
            }
        } else {
            self.builder.mapping_slot(key, slot)
        }
    }

    fn storage_array_element_access(
        &mut self,
        base_slot: ValueId,
        index: ValueId,
        element: solar_sema::ty::Ty<'gcx>,
        dynamic: bool,
    ) -> Option<StorageAccess> {
        if let Some((size, encoding)) = self.storage.packed_encoding(self.gcx, element)
            && size.bits() < 256
        {
            let bytes = u64::from(size.bytes());
            let per_slot_value = self.builder.imm_u64(32 / bytes);
            let slot_base =
                if dynamic { self.builder.storage_array_data_slot(base_slot) } else { base_slot };
            let slot_delta = self.builder.div(index, per_slot_value);
            let slot = self.builder.add(slot_base, slot_delta);
            let index_in_slot = self.builder.mod_(index, per_slot_value);
            let byte_size = self.builder.imm_u64(bytes);
            let offset = self.builder.mul(index_in_slot, byte_size);
            let location = StorageLocation { slot: U256::ZERO, offset: 0, size, encoding };
            return Some(StorageAccess { slot, location, offset: Some(offset) });
        }
        let element_slots = self.storage.element_slots(self.gcx, element);
        let slot = if dynamic {
            self.builder.storage_array_element_slot(base_slot, index, element_slots)
        } else {
            self.fixed_array_element_slot(base_slot, index, element_slots)
        };
        Some(StorageAccess { slot, location: StorageLocation::word(U256::ZERO), offset: None })
    }

    fn fixed_array_element_slot(
        &mut self,
        base_slot: ValueId,
        index: ValueId,
        element_slots: u64,
    ) -> ValueId {
        if element_slots == 1 {
            self.builder.add(base_slot, index)
        } else {
            let stride = self.builder.imm_u64(element_slots);
            let offset = self.builder.mul(index, stride);
            self.builder.add(base_slot, offset)
        }
    }

    fn add_storage_offset(&mut self, slot: ValueId, offset: U256) -> ValueId {
        if offset.is_zero() {
            slot
        } else {
            let offset = self.builder.imm_u256(offset);
            self.builder.add(slot, offset)
        }
    }

    fn load_storage_access(
        &mut self,
        expr: &hir::Expr<'_>,
        access: StorageAccess,
    ) -> Option<ValueId> {
        let ty = self.gcx.type_of_expr(expr.id)?;
        if ty.is_ref_at(DataLocation::Storage) {
            return Some(access.slot);
        }
        if self.types.memory_layout(ty).is_some() {
            return self.load_storage_object(ty, access.slot, expr.span);
        }
        Some(if let Some(offset) = access.offset {
            self.storage.load_packed_at_slot(
                &mut self.builder,
                access.location,
                access.slot,
                offset,
            )
        } else {
            self.storage.load_at_slot(&mut self.builder, access.location, access.slot)
        })
    }

    fn store_storage_access(
        &mut self,
        expr: &hir::Expr<'_>,
        access: StorageAccess,
        value: ValueId,
    ) -> Option<()> {
        let ty = self.gcx.type_of_expr(expr.id)?;
        if self.types.memory_layout(ty).is_some() {
            return self.store_storage_object(ty, access.slot, value, expr.span);
        }
        if let Some(offset) = access.offset {
            self.storage.store_packed_at_slot(
                &mut self.builder,
                access.location,
                access.slot,
                offset,
                value,
            );
        } else {
            self.storage.store_at_slot(&mut self.builder, access.location, access.slot, value);
        }
        Some(())
    }

    fn load_storage_object(
        &mut self,
        ty: solar_sema::ty::Ty<'gcx>,
        slot: ValueId,
        span: Span,
    ) -> Option<ValueId> {
        match ty.peel_refs().kind {
            solar_sema::ty::TyKind::Elementary(
                solar_sema::hir::ElementaryType::Bytes | solar_sema::hir::ElementaryType::String,
            ) => self.load_storage_bytes(slot),
            solar_sema::ty::TyKind::Struct(struct_id) => {
                let fields = self.gcx.hir.strukt(struct_id).fields.len() as u64;
                let layout = MemoryObjectLayout::Struct { fields };
                let size = self.builder.imm_u64(fields.saturating_mul(32));
                let object =
                    self.builder.alloc_object(size, layout, AllocationSemantics::SOLIDITY_ZEROED);
                for (index, &field) in self.gcx.hir.strukt(struct_id).fields.iter().enumerate() {
                    let field_ty = self.gcx.type_of_item(field.into());
                    let location = self.storage.field_location(self.gcx, struct_id, index)?;
                    let field_slot = self.add_storage_offset(slot, location.slot);
                    let value = if self.types.memory_layout(field_ty).is_some() {
                        self.load_storage_object(field_ty, field_slot, span)?
                    } else {
                        self.storage.load_at_slot(&mut self.builder, location, field_slot)
                    };
                    self.builder.memory_object_store_field(object, layout, index as u64, value);
                }
                Some(object)
            }
            solar_sema::ty::TyKind::Array(element, len) => {
                let len = u64::try_from(len).ok()?;
                let element_words = self.types.element_words(element);
                let layout = MemoryObjectLayout::FixedArray { len, element_words };
                let size = self
                    .builder
                    .imm_u64(len.checked_mul(u64::from(element_words))?.saturating_mul(32));
                let object =
                    self.builder.alloc_object(size, layout, AllocationSemantics::SOLIDITY_ZEROED);
                for index in 0..len {
                    let index_value = self.builder.imm_u64(index);
                    let access =
                        self.storage_array_element_access(slot, index_value, element, false)?;
                    let value = if self.types.memory_layout(element).is_some() {
                        self.load_storage_object(element, access.slot, span)?
                    } else if let Some(offset) = access.offset {
                        self.storage.load_packed_at_slot(
                            &mut self.builder,
                            access.location,
                            access.slot,
                            offset,
                        )
                    } else {
                        self.storage.load_at_slot(&mut self.builder, access.location, access.slot)
                    };
                    self.builder.memory_object_store_element(object, layout, index_value, value);
                }
                Some(object)
            }
            solar_sema::ty::TyKind::DynArray(element) => {
                self.load_dynamic_storage_object(element, slot, span)
            }
            _ => report_unsupported(self.gcx, span, "storage object copy"),
        }
    }

    fn load_dynamic_storage_object(
        &mut self,
        element: solar_sema::ty::Ty<'gcx>,
        slot: ValueId,
        span: Span,
    ) -> Option<ValueId> {
        let length = self.builder.sload(slot);
        let element_words = self.types.element_words(element);
        let stride = self.builder.imm_u64(u64::from(element_words));
        let words = self.checked_mul(length, stride);
        let one = self.builder.imm_u64(1);
        let words = self.checked_add(words, one);
        let word_size = self.builder.imm_u64(32);
        let size = self.checked_mul(words, word_size);
        let layout = MemoryObjectLayout::DynamicArray { element_words };
        let object = self.builder.alloc_object(size, layout, AllocationSemantics::SOLIDITY_ZEROED);
        self.builder.set_memory_object_len(object, length, layout.kind());

        let preheader = self.builder.current_block();
        let header = self.builder.create_block();
        let body = self.builder.create_block();
        let exit = self.builder.create_block();
        self.builder.jump(header);
        self.builder.switch_to_block(header);
        let zero = self.builder.imm_u64(0);
        let index = self.builder.phi(vec![(preheader, zero)]);
        let condition = self.builder.lt(index, length);
        self.builder.branch(condition, body, exit);

        self.builder.switch_to_block(body);
        let access = self.storage_array_element_access(slot, index, element, true)?;
        let value = if self.types.memory_layout(element).is_some() {
            self.load_storage_object(element, access.slot, span)?
        } else if let Some(offset) = access.offset {
            self.storage.load_packed_at_slot(
                &mut self.builder,
                access.location,
                access.slot,
                offset,
            )
        } else {
            self.storage.load_at_slot(&mut self.builder, access.location, access.slot)
        };
        self.builder.memory_object_store_element(object, layout, index, value);
        let one = self.builder.imm_u64(1);
        let next = self.builder.add(index, one);
        let backedge = self.builder.current_block();
        self.builder.jump(header);
        self.builder.add_phi_incoming(index, backedge, next);
        self.builder.switch_to_block(exit);
        Some(object)
    }

    fn store_storage_object(
        &mut self,
        ty: solar_sema::ty::Ty<'gcx>,
        slot: ValueId,
        object: ValueId,
        span: Span,
    ) -> Option<()> {
        match ty.peel_refs().kind {
            solar_sema::ty::TyKind::Elementary(
                solar_sema::hir::ElementaryType::Bytes | solar_sema::hir::ElementaryType::String,
            ) => self.store_storage_bytes(slot, object),
            solar_sema::ty::TyKind::Struct(struct_id) => {
                let fields = self.gcx.hir.strukt(struct_id).fields.len() as u64;
                let layout = MemoryObjectLayout::Struct { fields };
                for (index, &field) in self.gcx.hir.strukt(struct_id).fields.iter().enumerate() {
                    let field_ty = self.gcx.type_of_item(field.into());
                    let location = self.storage.field_location(self.gcx, struct_id, index)?;
                    let field_slot = self.add_storage_offset(slot, location.slot);
                    let value = self.builder.memory_object_load_field(object, layout, index as u64);
                    if self.types.memory_layout(field_ty).is_some() {
                        self.store_storage_object(field_ty, field_slot, value, span)?;
                    } else {
                        self.storage.store_at_slot(&mut self.builder, location, field_slot, value);
                    }
                }
                Some(())
            }
            solar_sema::ty::TyKind::Array(element, len) => {
                let len = u64::try_from(len).ok()?;
                let element_words = self.types.element_words(element);
                let layout = MemoryObjectLayout::FixedArray { len, element_words };
                for index in 0..len {
                    let index_value = self.builder.imm_u64(index);
                    let value =
                        self.builder.memory_object_load_element(object, layout, index_value);
                    let access =
                        self.storage_array_element_access(slot, index_value, element, false)?;
                    if let Some(offset) = access.offset {
                        self.storage.store_packed_at_slot(
                            &mut self.builder,
                            access.location,
                            access.slot,
                            offset,
                            value,
                        );
                    } else if self.types.memory_layout(element).is_some() {
                        self.store_storage_object(element, access.slot, value, span)?;
                    } else {
                        self.storage.store_at_slot(
                            &mut self.builder,
                            access.location,
                            access.slot,
                            value,
                        );
                    }
                }
                Some(())
            }
            solar_sema::ty::TyKind::DynArray(element) => {
                self.store_dynamic_storage_object(element, slot, object, span)
            }
            _ => report_unsupported(self.gcx, span, "storage object copy"),
        }
    }

    fn load_storage_bytes(&mut self, slot: ValueId) -> Option<ValueId> {
        let header = self.builder.sload(slot);
        let one = self.builder.imm_u64(1);
        let flag = self.builder.and(header, one);
        let is_long = self.builder.eq(flag, one);
        let two = self.builder.imm_u64(2);
        let short_mask = self.builder.imm_u256(U256::MAX << 8);
        let short_tag = self.builder.imm_u64(0xfe);
        let short_len_tag = self.builder.and(header, short_tag);
        let short_len = self.builder.div(short_len_tag, two);
        let long_len = self.builder.div(header, two);
        let length = self.builder.select(is_long, long_len, short_len);
        let thirty_one = self.builder.imm_u64(31);
        let rounded = self.checked_add(length, thirty_one);
        let word_size = self.builder.imm_u64(32);
        let words = self.builder.div(rounded, word_size);
        let total_words = self.checked_add(words, one);
        let size = self.checked_mul(total_words, word_size);
        let layout = MemoryObjectLayout::Bytes;
        let object = self.builder.alloc_object(size, layout, AllocationSemantics::SOLIDITY_ZEROED);
        self.builder.set_memory_object_len(object, length, layout.kind());

        let short_block = self.builder.create_block();
        let long_block = self.builder.create_block();
        let merge_block = self.builder.create_block();
        self.builder.branch(is_long, long_block, short_block);

        self.builder.switch_to_block(short_block);
        let zero = self.builder.imm_u64(0);
        let short_data = self.builder.and(header, short_mask);
        self.builder.memory_object_store_word(object, zero, short_data);
        self.builder.jump(merge_block);

        self.builder.switch_to_block(long_block);
        let data_slot = self.builder.storage_array_data_slot(slot);
        let preheader = self.builder.current_block();
        let header_block = self.builder.create_block();
        let body = self.builder.create_block();
        let exit = self.builder.create_block();
        self.builder.jump(header_block);
        self.builder.switch_to_block(header_block);
        let index = self.builder.phi(vec![(preheader, zero)]);
        let condition = self.builder.lt(index, words);
        self.builder.branch(condition, body, exit);
        self.builder.switch_to_block(body);
        let element_slot = self.builder.add(data_slot, index);
        let value = self.builder.sload(element_slot);
        let byte_offset = self.builder.mul(index, word_size);
        self.builder.memory_object_store_word(object, byte_offset, value);
        let next = self.builder.add(index, one);
        let backedge = self.builder.current_block();
        self.builder.jump(header_block);
        self.builder.add_phi_incoming(index, backedge, next);
        self.builder.switch_to_block(exit);
        self.builder.jump(merge_block);

        self.builder.switch_to_block(merge_block);
        Some(object)
    }

    fn store_storage_bytes(&mut self, slot: ValueId, object: ValueId) -> Option<()> {
        let length = self.builder.memory_object_len(object, MemoryObjectKind::Bytes);
        let word_size = self.builder.imm_u64(32);
        let short = self.builder.lt(length, word_size);
        let short_block = self.builder.create_block();
        let long_block = self.builder.create_block();
        let merge_block = self.builder.create_block();
        self.builder.branch(short, short_block, long_block);

        self.builder.switch_to_block(short_block);
        let zero = self.builder.imm_u64(0);
        let data = self.builder.memory_object_load_element(object, MemoryObjectLayout::Bytes, zero);
        let unused_bytes = self.builder.sub(word_size, length);
        let bits = self.builder.imm_u64(8);
        let shift = self.builder.mul(unused_bytes, bits);
        let one = self.builder.imm_u64(1);
        let high_bit = self.builder.shl(shift, one);
        let low_mask = self.builder.sub(high_bit, one);
        let data_mask = self.builder.not(low_mask);
        let data = self.builder.and(data, data_mask);
        let two = self.builder.imm_u64(2);
        let tag = self.builder.mul(length, two);
        let header = self.builder.or(data, tag);
        self.builder.sstore(slot, header);
        self.builder.jump(merge_block);

        self.builder.switch_to_block(long_block);
        let one = self.builder.imm_u64(1);
        let shifted = self.builder.shl(one, length);
        let tag = self.builder.or(shifted, one);
        self.builder.sstore(slot, tag);
        let thirty_one = self.builder.imm_u64(31);
        let rounded = self.checked_add(length, thirty_one);
        let words = self.builder.div(rounded, word_size);
        let data_slot = self.builder.storage_array_data_slot(slot);
        let preheader = self.builder.current_block();
        let header_block = self.builder.create_block();
        let body = self.builder.create_block();
        let exit = self.builder.create_block();
        self.builder.jump(header_block);
        self.builder.switch_to_block(header_block);
        let index = self.builder.phi(vec![(preheader, zero)]);
        let condition = self.builder.lt(index, words);
        self.builder.branch(condition, body, exit);
        self.builder.switch_to_block(body);
        let byte_offset = self.builder.mul(index, word_size);
        let value =
            self.builder.memory_object_load_element(object, MemoryObjectLayout::Bytes, byte_offset);
        let element_slot = self.builder.add(data_slot, index);
        self.builder.sstore(element_slot, value);
        let next = self.builder.add(index, one);
        let backedge = self.builder.current_block();
        self.builder.jump(header_block);
        self.builder.add_phi_incoming(index, backedge, next);
        self.builder.switch_to_block(exit);
        self.builder.jump(merge_block);

        self.builder.switch_to_block(merge_block);
        Some(())
    }

    fn store_dynamic_storage_object(
        &mut self,
        element: solar_sema::ty::Ty<'gcx>,
        slot: ValueId,
        object: ValueId,
        span: Span,
    ) -> Option<()> {
        let length = self.builder.memory_object_len(object, MemoryObjectKind::DynamicArray);
        self.builder.sstore(slot, length);
        let element_words = self.types.element_words(element);
        let array_layout = MemoryObjectLayout::DynamicArray { element_words };

        let preheader = self.builder.current_block();
        let header = self.builder.create_block();
        let body = self.builder.create_block();
        let exit = self.builder.create_block();
        self.builder.jump(header);
        self.builder.switch_to_block(header);
        let zero = self.builder.imm_u64(0);
        let index = self.builder.phi(vec![(preheader, zero)]);
        let condition = self.builder.lt(index, length);
        self.builder.branch(condition, body, exit);

        self.builder.switch_to_block(body);
        let value = self.builder.memory_object_load_element(object, array_layout, index);
        let access = self.storage_array_element_access(slot, index, element, true)?;
        if self.types.memory_layout(element).is_some() {
            self.store_storage_object(element, access.slot, value, span)?;
        } else if let Some(offset) = access.offset {
            self.storage.store_packed_at_slot(
                &mut self.builder,
                access.location,
                access.slot,
                offset,
                value,
            );
        } else {
            self.storage.store_at_slot(&mut self.builder, access.location, access.slot, value);
        }
        let one = self.builder.imm_u64(1);
        let next = self.builder.add(index, one);
        let backedge = self.builder.current_block();
        self.builder.jump(header);
        self.builder.add_phi_incoming(index, backedge, next);
        self.builder.switch_to_block(exit);
        Some(())
    }

    fn lower_array(&mut self, expr: &hir::Expr<'_>, elements: &[hir::Expr<'_>]) -> Option<ValueId> {
        let ty = self.gcx.type_of_expr(expr.id)?;
        let TyKind::Array(element_ty, _) = ty.peel_refs().kind else {
            return report_unsupported(self.gcx, expr.span, "array literal");
        };
        let layout = self.types.memory_layout(ty)?;
        let (size, kind) = match layout {
            MemoryObjectLayout::FixedArray { len, element_words } => {
                let words = len.checked_mul(u64::from(element_words))?;
                (words.checked_mul(32)?, MemoryObjectKind::FixedArray)
            }
            MemoryObjectLayout::DynamicArray { element_words } => {
                let words =
                    u64::try_from(elements.len()).ok()?.checked_mul(u64::from(element_words))?;
                (words.checked_add(1)?.checked_mul(32)?, MemoryObjectKind::DynamicArray)
            }
            _ => return report_unsupported(self.gcx, expr.span, "array literal"),
        };
        let size = self.builder.imm_u64(size);
        let object = self.builder.alloc_object(size, layout, AllocationSemantics::SOLIDITY_ZEROED);
        if kind == MemoryObjectKind::DynamicArray {
            let length = self.builder.imm_u64(u64::try_from(elements.len()).ok()?);
            self.builder.set_memory_object_len(object, length, kind);
        }
        for (index, element) in elements.iter().enumerate() {
            let value = self.lower_expr(element)?;
            let value = self.coerce_value(value, self.gcx.type_of_expr(element.id)?, element_ty);
            let index = self.builder.imm_u64(index as u64);
            self.builder.memory_object_store_element(object, layout, index, value);
        }
        Some(object)
    }

    fn lower_call(
        &mut self,
        expr: &hir::Expr<'_>,
        callee: &hir::Expr<'_>,
        args: hir::CallArgs<'_>,
        call_opts: Option<&hir::CallOptions<'_>>,
    ) -> Option<ValueId> {
        let arguments = args.exprs().collect::<Vec<_>>();
        if let Some(struct_id) = self.gcx.resolved_expr(callee).and_then(|res| match res {
            hir::Res::Item(item) => item.as_struct(),
            _ => None,
        }) {
            return self.lower_struct_constructor(expr, struct_id, args);
        }
        let is_type_conversion = matches!(callee.kind, ExprKind::TypeCall(_) | ExprKind::Type(_))
            || self.gcx.resolved_expr(callee).is_some_and(|res| {
                matches!(res, hir::Res::Item(hir::ItemId::Contract(_) | hir::ItemId::Enum(_)))
            });
        if is_type_conversion {
            let [arg] = arguments.as_slice() else {
                return report_unsupported(self.gcx, expr.span, "type conversion");
            };
            let value = self.lower_expr(arg)?;
            return Some(self.coerce_value(
                value,
                self.gcx.type_of_expr(arg.id)?,
                self.gcx.type_of_expr(expr.id)?,
            ));
        }
        if let ExprKind::New(ty) = &callee.kind {
            if let TyKind::Contract(contract_id) = self.gcx.type_of_hir_ty(ty).kind {
                return self.lower_new_contract(expr, ty, contract_id, args, call_opts);
            }
            let [arg] = arguments.as_slice() else {
                return report_unsupported(self.gcx, expr.span, "dynamic allocation");
            };
            let len = self.lower_expr(arg)?;
            let ty = self.gcx.type_of_expr(expr.id)?;
            let layout = self.types.memory_layout(ty)?;
            let words = match layout {
                MemoryObjectLayout::Bytes => {
                    let thirty_one = self.builder.imm_u64(31);
                    let rounded = self.checked_add(len, thirty_one);
                    let thirty_two = self.builder.imm_u64(32);
                    let words = self.builder.div(rounded, thirty_two);
                    let one = self.builder.imm_u64(1);
                    self.checked_add(words, one)
                }
                MemoryObjectLayout::DynamicArray { element_words } => {
                    let stride = self.builder.imm_u64(u64::from(element_words));
                    let payload = self.checked_mul(len, stride);
                    let one = self.builder.imm_u64(1);
                    self.checked_add(payload, one)
                }
                _ => return report_unsupported(self.gcx, expr.span, "allocation type"),
            };
            let word_size = self.builder.imm_u64(32);
            let size = self.checked_mul(words, word_size);
            let object =
                self.builder.alloc_object(size, layout, AllocationSemantics::SOLIDITY_ZEROED);
            self.builder.set_memory_object_len(object, len, layout.kind());
            return Some(object);
        }
        if let Some(builtin) = self.gcx.resolved_builtin(callee) {
            if matches!(
                builtin,
                Builtin::AddressCall | Builtin::AddressStaticcall | Builtin::AddressDelegatecall
            ) && let ExprKind::Member(receiver, _) = callee.kind
            {
                return self.lower_address_call(
                    callee.span,
                    receiver,
                    builtin,
                    args,
                    call_opts,
                    false,
                );
            }
            if matches!(builtin, Builtin::AddressPayableSend | Builtin::AddressPayableTransfer)
                && let ExprKind::Member(receiver, _) = callee.kind
            {
                return self.lower_payable_address_call(receiver, builtin, args);
            }
            return self.lower_builtin_call(expr, callee, builtin, args);
        }
        if let Some(TyKind::Fn(function)) = self.gcx.type_of_expr(callee.id).map(|ty| ty.kind)
            && function.is_external()
            && function.function_id.is_none()
        {
            return self.lower_external_function_pointer_call(callee, function, args);
        }
        if let Some(TyKind::Fn(function)) = self.gcx.type_of_expr(callee.id).map(|ty| ty.kind)
            && function.is_internal()
            && function.function_id.is_none()
        {
            return self.lower_internal_function_pointer_call(expr, callee, function, args);
        }
        if let Some(function_id) = self.gcx.resolved_function(callee) {
            return self.lower_function_call(expr, callee, function_id, args, call_opts);
        }
        if self.gcx.dcx().has_errors().is_err() {
            return Some(self.builder.imm_u256(U256::ZERO));
        }
        report_unsupported(self.gcx, expr.span, "function call")
    }

    fn materialize_array_element(
        &mut self,
        object: ValueId,
        layout: MemoryObjectLayout,
        index: ValueId,
        element: Ty<'gcx>,
        value: ValueId,
    ) -> Option<ValueId> {
        let zero = self.builder.imm_u256(U256::ZERO);
        let is_null = self.builder.eq(value, zero);
        let preheader = self.builder.current_block();
        let allocate = self.builder.create_block();
        let merge = self.builder.create_block();
        self.builder.branch(is_null, allocate, merge);

        self.builder.switch_to_block(allocate);
        let allocated = self.default_object(element)?;
        self.builder.memory_object_store_element(object, layout, index, allocated);
        let allocation_block = self.builder.current_block();
        self.builder.jump(merge);

        self.builder.switch_to_block(merge);
        Some(self.builder.phi(vec![(preheader, value), (allocation_block, allocated)]))
    }

    fn lower_payable_address_call(
        &mut self,
        receiver: &hir::Expr<'_>,
        builtin: Builtin,
        args: hir::CallArgs<'_>,
    ) -> Option<ValueId> {
        let amount = &self.builtin_args::<1>(builtin, &args)?[0];
        let address = self.lower_expr(receiver)?;
        let amount = self.lower_expr(amount)?;
        let zero = self.builder.imm_u256(U256::ZERO);
        let stipend = self.builder.imm_u64(2300);
        let amount_is_zero = self.builder.iszero(amount);
        let gas = self.builder.select(amount_is_zero, stipend, zero);
        let success = self.builder.call(gas, address, amount, zero, zero, zero, zero);
        if builtin == Builtin::AddressPayableTransfer {
            self.revert_external_call(success);
            Some(zero)
        } else {
            Some(success)
        }
    }

    fn lower_new_contract(
        &mut self,
        _expr: &hir::Expr<'_>,
        ty: &hir::Type<'_>,
        contract_id: hir::ContractId,
        args: hir::CallArgs<'_>,
        call_opts: Option<&hir::CallOptions<'_>>,
    ) -> Option<ValueId> {
        let contract = self.gcx.hir.contract(contract_id);
        let bytecode = self.child_bytecodes.get(&contract_id).ok_or_else(|| {
            self.gcx
                .dcx()
                .err(format!("codegen is missing creation bytecode for `new {}`", contract.name))
                .span(ty.span)
                .note("the deployed contract did not compile or was not lowered first")
                .emit()
        });
        let Ok(bytecode) = bytecode else { return None };

        let mut call_value = self.builder.imm_u256(U256::ZERO);
        let mut salt = None;
        if let Some(options) = call_opts {
            for option in options.args {
                let value = self.lower_expr(&option.value)?;
                match option.name.name {
                    sym::value => call_value = value,
                    sym::salt => salt = Some(value),
                    _ => return report_unsupported(self.gcx, option.name.span, "creation option"),
                }
            }
        }

        let (parameters, parameter_names) = contract
            .ctor
            .map(|id| {
                let constructor = self.gcx.hir.function(id);
                (
                    constructor.parameters,
                    self.gcx.callable_param_names(CallableParamSource::Function {
                        id,
                        skips_receiver: false,
                    }),
                )
            })
            .unwrap_or((&[], Vec::new().into()));
        if args.len() != parameters.len() {
            return report_unsupported(self.gcx, args.span, "constructor arguments");
        }

        let mut values = Vec::with_capacity(parameters.len());
        let mut types = Vec::with_capacity(parameters.len());
        for (index, &parameter) in parameters.iter().enumerate() {
            let Some(argument) =
                args.argument_for_parameter(index, Some(parameter_names.as_slice()))
            else {
                return report_unsupported(self.gcx, args.span, "constructor argument");
            };
            let parameter_ty = self.gcx.type_of_item(parameter.into());
            let mut value = self.lower_expr(argument)?;
            let mut abi_type = self.types.abi_type(parameter_ty)?;
            abi_type = self.abi_type_for_value(value, abi_type);
            if self.needs_calldata_materialization(value, &abi_type) {
                value = self.materialize_calldata_argument(parameter_ty, value, argument.span)?;
                abi_type = Self::memory_abi_type(abi_type);
            }
            values.push(value);
            types.push(abi_type);
        }
        let layout = Arc::new(AbiLayout::new(types.into_boxed_slice()));
        let encoded = self.builder.abi_encode(layout, None, values.into_boxed_slice());
        let encoded_len = self.builder.slice_len(encoded);

        let bytecode_len = u64::try_from(bytecode.len()).ok()?;
        let bytecode_len_value = self.builder.imm_u64(bytecode_len);
        let total_len = self.checked_add(bytecode_len_value, encoded_len);
        let thirty_one = self.builder.imm_u64(31);
        let rounded = self.checked_add(total_len, thirty_one);
        let word_size = self.builder.imm_u64(32);
        let words = self.builder.div(rounded, word_size);
        let one = self.builder.imm_u64(1);
        let object_words = self.checked_add(words, one);
        let size = self.checked_mul(object_words, word_size);
        let object = self.builder.alloc_object(
            size,
            MemoryObjectLayout::Bytes,
            AllocationSemantics::INTERNAL,
        );
        self.builder.set_memory_object_len(object, total_len, MemoryObjectKind::Bytes);

        for (index, chunk) in bytecode.chunks(32).enumerate() {
            let mut padded = [0u8; 32];
            padded[..chunk.len()].copy_from_slice(chunk);
            let offset = self.builder.imm_u64(u64::try_from(index).ok()?.saturating_mul(32));
            let value = self.builder.imm_u256(U256::from_be_bytes(padded));
            self.builder.memory_object_store_word(object, offset, value);
        }
        self.builder.memory_object_copy_from_slice_at(
            object,
            MemoryObjectKind::Bytes,
            bytecode_len_value,
            encoded,
        );

        let data = self.builder.memory_object_data(object, MemoryObjectKind::Bytes);
        let created = if let Some(salt) = salt {
            self.builder.create2(call_value, data, total_len, salt)
        } else {
            self.builder.create(call_value, data, total_len)
        };
        self.revert_external_call(created);
        Some(created)
    }

    fn lower_external_function_pointer_call(
        &mut self,
        callee: &hir::Expr<'_>,
        function: &solar_sema::ty::TyFn<'gcx>,
        args: hir::CallArgs<'_>,
    ) -> Option<ValueId> {
        let values = self.lower_external_function_pointer_call_values(callee, function, args)?;
        Some(values.into_iter().next().unwrap_or_else(|| self.builder.imm_u256(U256::ZERO)))
    }

    fn lower_external_function_pointer_call_values(
        &mut self,
        callee: &hir::Expr<'_>,
        function: &solar_sema::ty::TyFn<'gcx>,
        args: hir::CallArgs<'_>,
    ) -> Option<Vec<ValueId>> {
        let arg_exprs = self.builtin_arg_exprs(Builtin::AbiEncode, &args)?;
        if arg_exprs.len() != function.parameters.len() {
            return report_unsupported(self.gcx, args.span, "external function arguments");
        }
        let function_value = self.lower_expr(callee)?;
        let selector_mask = self.builder.imm_u256(U256::from(u32::MAX));
        let selector = self.builder.and(function_value, selector_mask);
        let selector_shift = self.builder.imm_u64(224);
        let selector = self.builder.shl(selector_shift, selector);
        let address_shift = self.builder.imm_u64(32);
        let address = self.builder.shr(address_shift, function_value);

        let mut values = Vec::with_capacity(arg_exprs.len());
        let mut types = Vec::with_capacity(arg_exprs.len());
        for (argument, &parameter) in arg_exprs.iter().zip(function.parameters) {
            let mut value = self.lower_expr(argument)?;
            let mut abi_type = self.types.abi_type(parameter)?;
            abi_type = self.abi_type_for_value(value, abi_type);
            if self.needs_calldata_materialization(value, &abi_type) {
                value = self.materialize_calldata_argument(parameter, value, argument.span)?;
                abi_type = Self::memory_abi_type(abi_type);
            }
            values.push(value);
            types.push(abi_type);
        }
        let layout = Arc::new(AbiLayout::new(types.into_boxed_slice()));
        let encoded = self.builder.abi_encode(layout, Some(selector), values.into_boxed_slice());
        let input = self.builder.slice_ptr(encoded);
        let input_size = self.builder.slice_len(encoded);
        let zero = self.builder.imm_u256(U256::ZERO);
        let returns = function.returns.len();
        let decode_returndata = function.returns.iter().any(|&ret| {
            self.types.abi_return_type(ret).is_some_and(|ty| !matches!(ty, AbiType::Word))
        }) || self.gcx.sess.opts.evm_version.supports_returndata();
        let ret_offset = if !decode_returndata && returns > 1 { input } else { zero };
        let ret_size = if decode_returndata {
            zero
        } else {
            self.builder.imm_u64((returns as u64).saturating_mul(32))
        };
        let gas = self.builder.gas();
        let success = if matches!(
            function.state_mutability,
            hir::StateMutability::Pure | hir::StateMutability::View
        ) && self.gcx.sess.opts.evm_version.has_static_call()
        {
            self.builder.staticcall(gas, address, input, input_size, ret_offset, ret_size)
        } else {
            self.builder.call(gas, address, zero, input, input_size, ret_offset, ret_size)
        };
        self.revert_external_call(success);
        if returns == 0 {
            return Some(Vec::new());
        }
        if decode_returndata {
            if !self.gcx.sess.opts.evm_version.supports_returndata() {
                return report_error(
                    self.gcx,
                    callee.span,
                    "codegen cannot decode external function-pointer returndata before Byzantium",
                );
            }
            let data = self.materialize_returndata_bytes();
            let return_types = function
                .returns
                .iter()
                .copied()
                .map(|ty| ty.with_loc_if_ref(self.gcx, DataLocation::Memory))
                .collect::<Vec<_>>();
            return self.lower_abi_decode_values(data, &return_types, callee.span);
        }
        if returns > 1 {
            let slot = self.builder.imm_u64(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT);
            self.builder.mstore(slot, ret_offset);
            let mut values = Vec::with_capacity(returns);
            for index in 0..returns {
                values.push(self.load_multi_return_value(ret_offset, index));
            }
            return Some(values);
        }
        Some(vec![self.builder.mload(ret_offset)])
    }

    fn lower_internal_function_pointer_call(
        &mut self,
        expr: &hir::Expr<'_>,
        callee: &hir::Expr<'_>,
        function: &solar_sema::ty::TyFn<'gcx>,
        args: hir::CallArgs<'_>,
    ) -> Option<ValueId> {
        if args.len() != function.parameters.len() {
            return report_unsupported(self.gcx, expr.span, "internal function arguments");
        }
        let function_value = self.lower_expr(callee)?;
        let parameter_names =
            self.gcx.call_param_source(callee).map(|source| self.gcx.callable_param_names(source));
        let mut values = Vec::with_capacity(function.parameters.len());
        for (index, &parameter) in function.parameters.iter().enumerate() {
            let Some(argument) = args.argument_for_parameter(index, parameter_names.as_deref())
            else {
                return report_unsupported(self.gcx, expr.span, "named internal function argument");
            };
            let value = self.lower_expr(argument)?;
            values.push(self.materialize_call_argument(parameter, value, argument.span)?);
        }
        values.insert(0, function_value);

        let dispatcher = self.ensure_internal_function_pointer_dispatcher(function);
        let returns = function.returns.len();
        if returns == 0 {
            self.builder.internal_call_void(dispatcher, values, 0);
            return Some(self.builder.imm_u256(U256::ZERO));
        }
        let result_ty = types::TypeLowerer::mir_return_type(function.returns[0]);
        Some(self.builder.internal_call(dispatcher, values, result_ty, returns))
    }

    fn lower_internal_function_value(&mut self, expr: &hir::Expr<'_>) -> Option<ValueId> {
        let TyKind::Fn(function) = self.gcx.type_of_expr(expr.id)?.kind else { return None };
        if !function.is_internal() {
            return None;
        }
        let hir::Res::Item(hir::ItemId::Function(function_id)) = self.gcx.resolved_expr(expr)?
        else {
            return None;
        };
        let function_id = self.resolve_call_target(expr, function_id);
        self.pointer_registry.targets.insert(function_id);
        Some(self.builder.imm_u64(internal_function_pointer_id(function_id)))
    }

    fn ensure_internal_function_pointer_dispatcher(
        &mut self,
        function: &solar_sema::ty::TyFn<'gcx>,
    ) -> FunctionId {
        let shape = InternalFunctionPointerShape {
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
        if let Some(&dispatcher) = self.pointer_registry.dispatchers.get(&shape) {
            return dispatcher;
        }
        let index = self.pointer_registry.dispatchers.len();
        let dispatcher = self
            .module
            .add_function(Function::new(Ident::from_str(&format!("__internal_dispatch_{index}"))));
        self.pointer_registry.dispatchers.insert(shape, dispatcher);
        dispatcher
    }

    fn lower_struct_constructor(
        &mut self,
        expr: &hir::Expr<'_>,
        struct_id: hir::StructId,
        args: hir::CallArgs<'_>,
    ) -> Option<ValueId> {
        let struct_fields = self.gcx.hir.strukt(struct_id).fields;
        let fields = struct_fields.len() as u64;
        if args.len() != fields as usize {
            return report_unsupported(self.gcx, expr.span, "struct constructor arguments");
        }
        let parameter_names = self.gcx.callable_param_names(CallableParamSource::Struct(struct_id));
        let layout = MemoryObjectLayout::Struct { fields };
        let size = self.builder.imm_u64(fields.saturating_mul(32));
        let object = self.builder.alloc_object(size, layout, AllocationSemantics::SOLIDITY_ZEROED);
        for (index, &field) in struct_fields.iter().enumerate() {
            let Some(argument) =
                args.argument_for_parameter(index, Some(parameter_names.as_slice()))
            else {
                return report_unsupported(self.gcx, args.span, "struct constructor argument");
            };
            let value = self.lower_expr(argument)?;
            let field_ty = self.gcx.type_of_item(field.into());
            let value = self.materialize_memory_argument(field_ty, value, argument.span)?;
            let value = self.coerce_value(value, self.gcx.type_of_expr(argument.id)?, field_ty);
            self.builder.memory_object_store_field(object, layout, index as u64, value);
        }
        Some(object)
    }

    fn coerce_value(&mut self, value: ValueId, from: Ty<'gcx>, to: Ty<'gcx>) -> ValueId {
        if let TyKind::Enum(id) = to.peel_refs().kind {
            if !matches!(from.peel_refs().kind, TyKind::Enum(from_id) if from_id == id) {
                let limit = self.gcx.hir.enumm(id).variants.len() as u64;
                let limit = self.builder.imm_u64(limit);
                let valid = self.builder.lt(value, limit);
                let invalid = self.builder.iszero(valid);
                self.panic_if(invalid, 0x21);
            }
            return value;
        }
        if matches!(
            to.peel_refs().kind,
            TyKind::Elementary(solar_sema::hir::ElementaryType::Address(_))
        ) && let TyKind::Elementary(solar_sema::hir::ElementaryType::FixedBytes(size)) =
            from.peel_refs().kind
        {
            let shift = self.builder.imm_u64(u64::from(32 - size.bytes()) * 8);
            return self.builder.shr(shift, value);
        }
        let TyKind::Elementary(solar_sema::hir::ElementaryType::FixedBytes(size)) =
            to.peel_refs().kind
        else {
            return value;
        };
        if matches!(from.peel_refs().kind, TyKind::StringLiteral(..)) {
            let zero = self.builder.imm_u256(U256::ZERO);
            return self.builder.memory_object_load_element(value, MemoryObjectLayout::Bytes, zero);
        }
        if matches!(
            from.peel_refs().kind,
            TyKind::Elementary(
                solar_sema::hir::ElementaryType::Bytes | solar_sema::hir::ElementaryType::String,
            )
        ) {
            let value = match self.builder.func().value_ty(value) {
                Some(MirType::MemoryObject(MemoryObjectKind::Bytes)) => value,
                Some(MirType::Slice(_)) => self.materialize_memory_slice(value),
                _ => return value,
            };
            let zero = self.builder.imm_u256(U256::ZERO);
            return self.builder.memory_object_load_element(value, MemoryObjectLayout::Bytes, zero);
        }
        if matches!(
            from.peel_refs().kind,
            TyKind::Elementary(solar_sema::hir::ElementaryType::FixedBytes(_))
        ) {
            return value;
        }
        let shift = self.builder.imm_u64(u64::from(32 - size.bytes()) * 8);
        self.builder.shl(shift, value)
    }

    fn lower_storage_array_push(
        &mut self,
        expr: &hir::Expr<'_>,
        callee: &hir::Expr<'_>,
        builtin: Builtin,
        arguments: &[hir::Expr<'_>],
    ) -> Option<ValueId> {
        let ExprKind::Member(receiver, _) = &callee.kind else {
            return report_unsupported(self.gcx, expr.span, "storage array push target");
        };
        let receiver_ty = self.gcx.type_of_expr(receiver.id)?.peel_refs();
        if matches!(
            receiver_ty.kind,
            TyKind::Elementary(
                solar_sema::hir::ElementaryType::Bytes | solar_sema::hir::ElementaryType::String
            )
        ) {
            return self.lower_storage_bytes_push(expr, receiver, builtin, arguments);
        }
        let Some((element_access, element, new_length, base_slot)) =
            self.storage_array_push_access(receiver)
        else {
            return report_unsupported(self.gcx, expr.span, "storage array push target");
        };
        let value = if builtin == Builtin::ArrayPush {
            let [argument] = arguments else {
                return report_unsupported(self.gcx, expr.span, "storage array push arguments");
            };
            let value = self.lower_expr(argument)?;
            self.coerce_value(value, self.gcx.type_of_expr(argument.id)?, element)
        } else {
            if !arguments.is_empty() {
                return report_unsupported(self.gcx, expr.span, "storage array push arguments");
            }
            self.default_value(element)
        };
        if self.types.memory_layout(element).is_some() {
            self.store_storage_object(element, element_access.slot, value, expr.span)?;
        } else if let Some(offset) = element_access.offset {
            self.storage.store_packed_at_slot(
                &mut self.builder,
                element_access.location,
                element_access.slot,
                offset,
                value,
            );
        } else {
            self.storage.store_at_slot(
                &mut self.builder,
                element_access.location,
                element_access.slot,
                value,
            );
        }
        self.builder.sstore(base_slot, new_length);
        Some(self.builder.imm_u256(U256::ZERO))
    }

    fn lower_storage_bytes_push(
        &mut self,
        expr: &hir::Expr<'_>,
        receiver: &hir::Expr<'_>,
        builtin: Builtin,
        arguments: &[hir::Expr<'_>],
    ) -> Option<ValueId> {
        let access = self.storage_access(receiver)?;
        let old = self.load_storage_bytes(access.slot)?;
        let old_length = self.builder.memory_object_len(old, MemoryObjectKind::Bytes);
        let one = self.builder.imm_u64(1);
        let length = self.checked_add(old_length, one);
        let word_size = self.builder.imm_u64(32);
        let thirty_one = self.builder.imm_u64(31);
        let rounded = self.checked_add(length, thirty_one);
        let words = self.builder.div(rounded, word_size);
        let total_words = self.checked_add(words, one);
        let size = self.checked_mul(total_words, word_size);
        let layout = MemoryObjectLayout::Bytes;
        let object = self.builder.alloc_object(size, layout, AllocationSemantics::SOLIDITY_ZEROED);
        self.builder.set_memory_object_len(object, length, layout.kind());
        self.builder.memory_object_copy(object, layout.kind(), old, layout.kind(), old_length);
        let value = if builtin == Builtin::ArrayPush {
            let [argument] = arguments else {
                return report_unsupported(self.gcx, expr.span, "storage bytes push arguments");
            };
            let value = self.lower_expr(argument)?;
            let shift = self.builder.imm_u64(248);
            self.builder.shr(shift, value)
        } else {
            if !arguments.is_empty() {
                return report_unsupported(self.gcx, expr.span, "storage bytes push arguments");
            }
            self.builder.imm_u64(0)
        };
        self.builder.memory_object_store_byte(object, old_length, value);
        self.store_storage_bytes(access.slot, object)?;
        Some(self.builder.imm_u256(U256::ZERO))
    }

    fn lower_storage_array_pop(
        &mut self,
        expr: &hir::Expr<'_>,
        callee: &hir::Expr<'_>,
    ) -> Option<ValueId> {
        let ExprKind::Member(receiver, _) = &callee.kind else {
            return report_unsupported(self.gcx, expr.span, "storage array pop target");
        };
        let receiver_ty = self.gcx.type_of_expr(receiver.id)?.peel_refs();
        if matches!(
            receiver_ty.kind,
            TyKind::Elementary(
                solar_sema::hir::ElementaryType::Bytes | solar_sema::hir::ElementaryType::String
            )
        ) {
            let access = self.storage_access(receiver)?;
            let old = self.load_storage_bytes(access.slot)?;
            let old_length = self.builder.memory_object_len(old, MemoryObjectKind::Bytes);
            let zero = self.builder.imm_u64(0);
            let empty = self.builder.eq(old_length, zero);
            self.panic_if(empty, 0x31);
            let one = self.builder.imm_u64(1);
            let length = self.builder.sub(old_length, one);
            let word_size = self.builder.imm_u64(32);
            let thirty_one = self.builder.imm_u64(31);
            let rounded = self.checked_add(length, thirty_one);
            let words = self.builder.div(rounded, word_size);
            let total_words = self.checked_add(words, one);
            let size = self.checked_mul(total_words, word_size);
            let layout = MemoryObjectLayout::Bytes;
            let object =
                self.builder.alloc_object(size, layout, AllocationSemantics::SOLIDITY_ZEROED);
            self.builder.set_memory_object_len(object, length, layout.kind());
            self.builder.memory_object_copy(object, layout.kind(), old, layout.kind(), length);
            self.store_storage_bytes(access.slot, object)?;
            return Some(zero);
        }

        let Some((base, element)) = self.storage_array_base(receiver) else {
            return report_unsupported(self.gcx, expr.span, "storage array pop target");
        };
        let length = self.builder.sload(base.slot);
        let zero = self.builder.imm_u64(0);
        let empty = self.builder.eq(length, zero);
        self.panic_if(empty, 0x31);
        let one = self.builder.imm_u64(1);
        let last = self.builder.sub(length, one);
        self.builder.sstore(base.slot, last);
        let access = self.storage_array_element_access(base.slot, last, element, true)?;
        let value = self.default_value(element);
        if self.types.memory_layout(element).is_some() {
            self.store_storage_object(element, access.slot, value, expr.span)?;
        } else if let Some(offset) = access.offset {
            self.storage.store_packed_at_slot(
                &mut self.builder,
                access.location,
                access.slot,
                offset,
                value,
            );
        } else {
            self.storage.store_at_slot(&mut self.builder, access.location, access.slot, value);
        }
        Some(zero)
    }

    fn storage_array_base(
        &mut self,
        receiver: &hir::Expr<'_>,
    ) -> Option<(StorageAccess, Ty<'gcx>)> {
        let base = self.storage_access(receiver)?;
        let ty = self.gcx.type_of_expr(receiver.id)?.peel_refs();
        let TyKind::DynArray(element) = ty.kind else { return None };
        Some((base, element))
    }

    fn lower_builtin_call(
        &mut self,
        expr: &hir::Expr<'_>,
        callee: &hir::Expr<'_>,
        builtin: Builtin,
        args: hir::CallArgs<'_>,
    ) -> Option<ValueId> {
        if builtin.is_yul() {
            let Some(returns) = builtin.ty(self.gcx).returns() else {
                return report_error(
                    self.gcx,
                    callee.span,
                    "codegen expected Yul builtin to have a function type",
                );
            };
            return if returns.is_empty() {
                match self.lower_yul_unit_builtin_call(builtin, args) {
                    Some(()) => Some(self.builder.imm_u256(U256::ZERO)),
                    None => Some(self.builder.imm_u256(U256::ZERO)),
                }
            } else {
                self.lower_yul_value_builtin_call(builtin, args)
                    .or_else(|| Some(self.builder.imm_u256(U256::ZERO)))
            };
        }

        if matches!(builtin, Builtin::ArrayPush | Builtin::ArrayPush0 | Builtin::ArrayPop) {
            let result = match builtin {
                Builtin::ArrayPush => {
                    let Some(arguments) = self.builtin_args::<1>(builtin, &args) else {
                        return Some(self.builder.imm_u256(U256::ZERO));
                    };
                    self.lower_storage_array_push(expr, callee, builtin, arguments)
                }
                Builtin::ArrayPush0 => {
                    let Some(arguments) = self.builtin_args::<0>(builtin, &args) else {
                        return Some(self.builder.imm_u256(U256::ZERO));
                    };
                    self.lower_storage_array_push(expr, callee, builtin, arguments)
                }
                Builtin::ArrayPop => {
                    if self.builtin_args::<0>(builtin, &args).is_none() {
                        return Some(self.builder.imm_u256(U256::ZERO));
                    }
                    self.lower_storage_array_pop(expr, callee)
                }
                _ => unreachable!(),
            };
            return result.or_else(|| Some(self.builder.imm_u256(U256::ZERO)));
        }

        match builtin {
            Builtin::Selfdestruct
            | Builtin::Require
            | Builtin::Assert
            | Builtin::Revert
            | Builtin::RevertMsg => match self.lower_unit_builtin_call(builtin, args) {
                Some(()) => Some(self.builder.imm_u256(U256::ZERO)),
                None => Some(self.builder.imm_u256(U256::ZERO)),
            },
            _ => self
                .lower_solidity_value_builtin_call(builtin, args)
                .or_else(|| Some(self.builder.imm_u256(U256::ZERO))),
        }
    }

    fn lower_address_call(
        &mut self,
        call_span: Span,
        receiver: &hir::Expr<'_>,
        builtin: Builtin,
        args: hir::CallArgs<'_>,
        call_opts: Option<&hir::CallOptions<'_>>,
        capture_returndata: bool,
    ) -> Option<ValueId> {
        self.lower_address_call_result(
            call_span,
            receiver,
            builtin,
            args,
            call_opts,
            capture_returndata,
        )
        .map(|(success, _)| success)
    }

    fn lower_address_call_result(
        &mut self,
        call_span: Span,
        receiver: &hir::Expr<'_>,
        builtin: Builtin,
        args: hir::CallArgs<'_>,
        call_opts: Option<&hir::CallOptions<'_>>,
        capture_returndata: bool,
    ) -> Option<(ValueId, Option<ValueId>)> {
        let data = &self.builtin_args::<1>(builtin, &args)?[0];
        let address = self.lower_expr(receiver)?;
        let data = self.lower_expr(data)?;
        let data = match self.builder.func().value_ty(data) {
            Some(MirType::Slice(_)) => self.materialize_memory_slice(data),
            _ => data,
        };
        let input = self.builder.memory_object_data(data, MemoryObjectKind::Bytes);
        let input_size = self.builder.memory_object_len(data, MemoryObjectKind::Bytes);
        let zero = self.builder.imm_u256(U256::ZERO);
        if capture_returndata && !self.gcx.sess.opts.evm_version.supports_returndata() {
            return report_error(
                self.gcx,
                call_span,
                "codegen cannot bind low-level call returndata before Byzantium",
            );
        }
        let mut gas = self.builder.gas();
        let mut value = zero;
        if let Some(options) = call_opts {
            for option in options.args {
                let option_value = self.lower_expr(&option.value)?;
                match option.name.name {
                    kw::Gas => gas = option_value,
                    sym::value if builtin == Builtin::AddressCall => value = option_value,
                    _ => return report_unsupported(self.gcx, option.name.span, "call option"),
                }
            }
        }
        let success = match builtin {
            Builtin::AddressCall => {
                self.builder.call(gas, address, value, input, input_size, zero, zero)
            }
            Builtin::AddressStaticcall => {
                if !self.gcx.sess.opts.evm_version.has_static_call() {
                    return report_error(
                        self.gcx,
                        call_span,
                        "codegen cannot use `staticcall` before Byzantium",
                    );
                }
                self.builder.staticcall(gas, address, input, input_size, zero, zero)
            }
            Builtin::AddressDelegatecall => {
                self.builder.delegatecall(gas, address, input, input_size, zero, zero)
            }
            _ => unreachable!(),
        };
        let returndata = capture_returndata.then(|| self.materialize_returndata_bytes());
        Some((success, returndata))
    }

    fn lower_unit_builtin_call(&mut self, builtin: Builtin, args: hir::CallArgs<'_>) -> Option<()> {
        match builtin {
            Builtin::Assert => {
                let condition = &self.builtin_args::<1>(builtin, &args)?[0];
                let condition = self.lower_expr(condition)?;
                let invalid = self.builder.iszero(condition);
                self.panic_if(invalid, 0x01);
            }
            Builtin::Require => {
                let (required, message) = self.builtin_args_with_optional::<1>(builtin, &args)?;
                let condition = required.first()?;
                let condition = self.lower_expr(condition)?;
                let is_false = self.builder.iszero(condition);
                let revert_block = self.builder.create_block();
                let continue_block = self.builder.create_block();
                self.builder.branch(is_false, revert_block, continue_block);
                self.builder.switch_to_block(revert_block);
                if let Some(message) = message {
                    self.lower_revert_payload(message)?;
                } else {
                    let zero = self.builder.imm_u256(U256::ZERO);
                    self.builder.revert(zero, zero);
                }
                self.builder.switch_to_block(continue_block);
            }
            Builtin::Revert => {
                let _ = self.builtin_args::<0>(builtin, &args)?;
                let zero = self.builder.imm_u256(U256::ZERO);
                self.builder.revert(zero, zero);
            }
            Builtin::RevertMsg => {
                let message = &self.builtin_args::<1>(builtin, &args)?[0];
                self.lower_revert_payload(message)?;
            }
            Builtin::Selfdestruct => {
                let address = &self.builtin_args::<1>(builtin, &args)?[0];
                let address = self.lower_expr(address)?;
                self.builder.selfdestruct(address);
            }
            _ => {
                return report_error(
                    self.gcx,
                    args.span,
                    "codegen routed a value builtin through unit lowering",
                );
            }
        }
        Some(())
    }

    fn lower_solidity_value_builtin_call(
        &mut self,
        builtin: Builtin,
        args: hir::CallArgs<'_>,
    ) -> Option<ValueId> {
        match builtin {
            Builtin::Keccak256 => {
                let value = &self.builtin_args::<1>(builtin, &args)?[0];
                if let ExprKind::Call(callee, encode_args, _) = &value.kind
                    && self.gcx.resolved_builtin(callee) == Some(Builtin::AbiEncode)
                {
                    let exprs = self.variadic_builtin_args(Builtin::AbiEncode, encode_args)?;
                    let encoded = self.lower_abi_encode_slice(exprs, None)?;
                    let pointer = self.builder.slice_ptr(encoded);
                    let length = self.builder.slice_len(encoded);
                    return Some(self.builder.keccak256(pointer, length));
                }
                let value = self.lower_expr(value)?;
                let value = match self.builder.func().value_ty(value) {
                    Some(MirType::Slice(_)) => self.materialize_memory_slice(value),
                    _ => value,
                };
                Some(self.builder.keccak256_bytes(value))
            }
            Builtin::Gasleft => {
                let _ = self.builtin_args::<0>(builtin, &args)?;
                Some(self.builder.gas())
            }
            Builtin::AbiEncode => self.lower_abi_encode_builtin(args, None),
            Builtin::AbiEncodeWithSelector => {
                let (selector, rest) = self.builtin_args_with_rest::<1>(builtin, &args)?;
                let selector = self.lower_selector_word(&selector[0])?;
                self.lower_abi_encode_builtin_args(rest, Some(selector))
            }
            Builtin::AbiEncodePacked => self.lower_abi_encode_packed(args),
            Builtin::AbiEncodeWithSignature => self.lower_abi_encode_with_signature(args),
            Builtin::AbiEncodeCall => self.lower_abi_encode_call(args),
            Builtin::AbiDecode => self.lower_abi_decode(args),
            Builtin::Blockhash | Builtin::Blobhash => {
                let value = &self.builtin_args::<1>(builtin, &args)?[0];
                let value = self.lower_expr(value)?;
                Some(if builtin == Builtin::Blockhash {
                    self.builder.blockhash(value)
                } else {
                    self.builder.blobhash(value)
                })
            }
            Builtin::AddMod | Builtin::MulMod => {
                let [a, b, modulus] = self.lower_builtin_args(builtin, &args)?;
                Some(if builtin == Builtin::AddMod {
                    self.builder.addmod(a, b, modulus)
                } else {
                    self.builder.mulmod(a, b, modulus)
                })
            }
            Builtin::Erc7201 => self.lower_erc7201(args),
            Builtin::Sha256 | Builtin::Ripemd160 => self.lower_hash_precompile_call(builtin, args),
            Builtin::EcRecover => self.lower_ecrecover_call(args),
            Builtin::StringConcat | Builtin::BytesConcat => {
                self.lower_concat_builtin_call(builtin, args)
            }
            Builtin::UdvtWrap | Builtin::UdvtUnwrap => {
                let value = self.builtin_args::<1>(builtin, &args)?.first()?;
                self.lower_expr(value)
            }
            Builtin::Selfdestruct
            | Builtin::Require
            | Builtin::Assert
            | Builtin::Revert
            | Builtin::RevertMsg => report_error(
                self.gcx,
                args.span,
                "codegen routed a unit builtin through value lowering",
            ),
            _ => {
                if self.validate_builtin_arity(builtin, &args) {
                    self.unsupported_builtin(builtin, args.span)
                } else {
                    None
                }
            }
        }
    }

    fn lower_erc7201(&mut self, args: hir::CallArgs<'_>) -> Option<ValueId> {
        let argument = &self.builtin_args::<1>(Builtin::Erc7201, &args)?[0];
        let inner = if let ExprKind::Lit(lit) = &argument.kind
            && let LitKind::Str(_, bytes, _) = &lit.kind
        {
            self.builder.imm_u256(U256::from_be_slice(keccak256(bytes.as_byte_str()).as_slice()))
        } else {
            let value = self.lower_expr(argument)?;
            self.builder.keccak256_bytes(value)
        };
        let one = self.builder.imm_u256(U256::from(1));
        let inner = self.builder.sub(inner, one);
        let zero = self.builder.imm_u256(U256::ZERO);
        let word_size = self.builder.imm_u64(32);
        self.builder.mstore(zero, inner);
        let outer = self.builder.keccak256(zero, word_size);
        let mask = self.builder.imm_u256(!U256::from(0xff));
        Some(self.builder.and(outer, mask))
    }

    fn lower_concat_builtin_call(
        &mut self,
        builtin: Builtin,
        args: hir::CallArgs<'_>,
    ) -> Option<ValueId> {
        let exprs = self.variadic_builtin_args(builtin, &args)?;
        let mut total = self.builder.imm_u64(0);
        let mut parts = Vec::with_capacity(exprs.len());
        for expr in exprs {
            let ty = self.gcx.type_of_expr(expr.id)?;
            match ty.peel_refs().kind {
                TyKind::StringLiteral(..)
                | TyKind::Elementary(
                    solar_sema::hir::ElementaryType::String
                    | solar_sema::hir::ElementaryType::Bytes,
                ) => {
                    let value = self.lower_expr(expr)?;
                    let length = self.builder.memory_object_len(value, MemoryObjectKind::Bytes);
                    total = self.checked_add(total, length);
                    parts.push((value, Some(length), 0));
                }
                TyKind::Elementary(solar_sema::hir::ElementaryType::FixedBytes(size)) => {
                    let value = self.lower_expr(expr)?;
                    let length = u64::from(size.bytes());
                    let length_value = self.builder.imm_u64(length);
                    total = self.checked_add(total, length_value);
                    parts.push((value, None, length));
                }
                _ => return report_unsupported(self.gcx, expr.span, "concat argument"),
            }
        }

        let thirty_one = self.builder.imm_u64(31);
        let rounded = self.checked_add(total, thirty_one);
        let word_size = self.builder.imm_u64(32);
        let words = self.builder.div(rounded, word_size);
        let one = self.builder.imm_u64(1);
        let words = self.checked_add(words, one);
        let size = self.checked_mul(words, word_size);
        let output = self.builder.alloc_object(
            size,
            MemoryObjectLayout::Bytes,
            AllocationSemantics::SOLIDITY_ZEROED,
        );
        self.builder.set_memory_object_len(output, total, MemoryObjectKind::Bytes);

        let mut offset = self.builder.imm_u64(0);
        for (value, dynamic_length, static_length) in parts {
            if let Some(length) = dynamic_length {
                let source_ptr = self.builder.memory_object_data(value, MemoryObjectKind::Bytes);
                let source = self.builder.make_slice(source_ptr, length, SliceLocation::Memory);
                self.builder.memory_object_copy_from_slice_at(
                    output,
                    MemoryObjectKind::Bytes,
                    offset,
                    source,
                );
                offset = self.checked_add(offset, length);
            } else {
                self.builder.memory_object_store_word(output, offset, value);
                let length = self.builder.imm_u64(static_length);
                offset = self.checked_add(offset, length);
            }
        }
        Some(output)
    }

    fn lower_abi_encode_builtin(
        &mut self,
        args: hir::CallArgs<'_>,
        selector: Option<ValueId>,
    ) -> Option<ValueId> {
        let exprs = self.variadic_builtin_args(Builtin::AbiEncode, &args)?;
        self.lower_abi_encode_builtin_args(exprs, selector)
    }

    fn lower_abi_encode_builtin_args(
        &mut self,
        exprs: &[hir::Expr<'_>],
        selector: Option<ValueId>,
    ) -> Option<ValueId> {
        let encoded = self.lower_abi_encode_slice(exprs, selector)?;
        Some(self.materialize_memory_slice(encoded))
    }

    fn lower_abi_encode_slice(
        &mut self,
        exprs: &[hir::Expr<'_>],
        selector: Option<ValueId>,
    ) -> Option<ValueId> {
        let mut values = Vec::with_capacity(exprs.len());
        let mut types = Vec::with_capacity(exprs.len());
        for expr in exprs {
            let ty = self.gcx.type_of_expr(expr.id)?;
            let mut value = self.lower_expr(expr)?;
            let mut abi_type = if matches!(ty.peel_refs().kind, TyKind::StringLiteral(..)) {
                AbiType::Bytes(SliceLocation::Memory)
            } else {
                self.types.abi_type(ty)?
            };
            abi_type = self.abi_type_for_value(value, abi_type);
            if self.needs_calldata_materialization(value, &abi_type) {
                value = self.materialize_calldata_argument(ty, value, expr.span)?;
                abi_type = Self::memory_abi_type(abi_type);
            }
            values.push(value);
            types.push(abi_type);
        }
        let layout = Arc::new(AbiLayout::new(types.into_boxed_slice()));
        Some(self.builder.abi_encode(layout, selector, values.into_boxed_slice()))
    }

    fn lower_selector_word(&mut self, expr: &hir::Expr<'_>) -> Option<ValueId> {
        let value = self.lower_expr(expr)?;
        let fixed_bytes = self.gcx.type_of_expr(expr.id).is_some_and(|ty| {
            matches!(
                ty.peel_refs().kind,
                TyKind::Elementary(solar_sema::hir::ElementaryType::FixedBytes(_))
            )
        });
        if !fixed_bytes
            && matches!(expr.peel_parens().kind, ExprKind::Lit(lit) if matches!(
                lit.kind,
                LitKind::Number(_) | LitKind::Rational(_)
            ))
        {
            let shift = self.builder.imm_u64(224);
            return Some(self.builder.shl(shift, value));
        }
        Some(value)
    }

    fn lower_abi_encode_with_signature(&mut self, args: hir::CallArgs<'_>) -> Option<ValueId> {
        let (signature, rest) =
            self.builtin_args_with_rest::<1>(Builtin::AbiEncodeWithSignature, &args)?;
        let selector = self.lower_signature_selector(&signature[0])?;
        self.lower_abi_encode_builtin_args(rest, Some(selector))
    }

    fn lower_signature_selector(&mut self, signature: &hir::Expr<'_>) -> Option<ValueId> {
        if let ExprKind::Lit(lit) = &signature.kind
            && let LitKind::Str(_, value, _) = &lit.kind
        {
            let hash = keccak256(value.as_byte_str());
            let selector = U256::from_be_slice(&hash[..4]) << 224;
            return Some(self.builder.imm_u256(selector));
        }

        if let ExprKind::Ternary(condition, then_expr, else_expr) = &signature.kind {
            let condition = self.lower_expr(condition)?;
            let then_selector = self.lower_signature_selector(then_expr)?;
            let else_selector = self.lower_signature_selector(else_expr)?;
            return Some(self.builder.select(condition, then_selector, else_selector));
        }

        let signature = self.lower_expr(signature)?;
        let signature = match self.builder.func().value_ty(signature) {
            Some(MirType::Slice(_)) => self.materialize_memory_slice(signature),
            _ => signature,
        };
        let hash = self.builder.keccak256_bytes(signature);
        let shift = self.builder.imm_u64(224);
        let selector = self.builder.shr(shift, hash);
        Some(self.builder.shl(shift, selector))
    }

    fn lower_abi_encode_call(&mut self, args: hir::CallArgs<'_>) -> Option<ValueId> {
        let args = self.builtin_args::<2>(Builtin::AbiEncodeCall, &args)?;
        let function = &args[0];
        let tuple = &args[1];
        let (selector, parameter_types) =
            if let Some(function_id) = self.gcx.resolved_function(function) {
                let selector = self.gcx.function_selector(function_id).0;
                let parameter_types = self
                    .gcx
                    .hir
                    .function(function_id)
                    .parameters
                    .iter()
                    .map(|&parameter| self.gcx.type_of_item(parameter.into()))
                    .collect::<Vec<_>>();
                (self.builder.imm_u256(U256::from_be_slice(&selector) << 224), parameter_types)
            } else {
                let Some(TyKind::Fn(function_ty)) =
                    self.gcx.type_of_expr(function.id).map(|ty| ty.kind)
                else {
                    return report_unsupported(self.gcx, function.span, "abi.encodeCall function");
                };
                if !function_ty.is_external() {
                    return report_unsupported(self.gcx, function.span, "abi.encodeCall function");
                }
                let function_value = self.lower_expr(function)?;
                let mask = self.builder.imm_u256(U256::from(u32::MAX));
                let selector = self.builder.and(function_value, mask);
                let shift = self.builder.imm_u64(224);
                (self.builder.shl(shift, selector), function_ty.parameters.to_vec())
            };
        let exprs = match tuple.peel_parens().kind {
            ExprKind::Tuple(elements) => elements.iter().flatten().copied().collect::<Vec<_>>(),
            _ => vec![tuple],
        };
        if exprs.len() != parameter_types.len() {
            return report_unsupported(self.gcx, tuple.span, "abi.encodeCall argument list");
        }
        let mut values = Vec::with_capacity(exprs.len());
        let mut types = Vec::with_capacity(exprs.len());
        for (index, expr) in exprs.into_iter().enumerate() {
            let ty = parameter_types[index];
            let from_ty = self.gcx.type_of_expr(expr.id)?;
            let mut value = self.lower_expr(expr)?;
            value = self.coerce_value(value, from_ty, ty);
            let mut abi_type = self.types.abi_type(ty)?;
            abi_type = self.abi_type_for_value(value, abi_type);
            if self.needs_calldata_materialization(value, &abi_type) {
                value = self.materialize_calldata_argument(ty, value, expr.span)?;
                abi_type = Self::memory_abi_type(abi_type);
            }
            values.push(value);
            types.push(abi_type);
        }
        let layout = Arc::new(AbiLayout::new(types.into_boxed_slice()));
        let encoded = self.builder.abi_encode(layout, Some(selector), values.into_boxed_slice());
        Some(self.materialize_memory_slice(encoded))
    }

    fn lower_abi_decode(&mut self, args: hir::CallArgs<'_>) -> Option<ValueId> {
        let args = self.builtin_args::<2>(Builtin::AbiDecode, &args)?;
        let types = match args[1].kind {
            ExprKind::Tuple(types) => types.iter().flatten().copied().collect::<Vec<_>>(),
            _ => return report_unsupported(self.gcx, args[1].span, "abi.decode target type"),
        };
        if types.is_empty() {
            return report_unsupported(self.gcx, args[1].span, "abi.decode target type");
        }
        let mut decoded_types = Vec::with_capacity(types.len());
        for ty_expr in &types {
            let Some(TyKind::Type(ty)) = self.gcx.type_of_expr(ty_expr.id).map(|ty| ty.kind) else {
                return report_unsupported(self.gcx, ty_expr.span, "abi.decode target type");
            };
            decoded_types.push(ty.with_loc_if_ref(self.gcx, DataLocation::Memory));
        }

        let data = self.lower_expr(&args[0])?;
        let (data, layout) = self.lower_abi_decode_layout(data, &decoded_types, args[1].span)?;
        Some(self.builder.abi_decode(layout, data))
    }

    fn lower_abi_decode_layout(
        &mut self,
        data: ValueId,
        types: &[Ty<'gcx>],
        span: Span,
    ) -> Option<(ValueId, AbiParamLayout)> {
        let data = match self.builder.func().value_ty(data) {
            Some(MirType::Slice(_)) => self.materialize_memory_slice(data),
            _ => data,
        };
        let mut abi_types = Vec::with_capacity(types.len());
        for &ty in types {
            let Some(abi_type) = self.types.abi_param_type(ty) else {
                return report_unsupported(self.gcx, span, "abi.decode target type");
            };
            abi_types.push(abi_type);
        }
        Some((data, AbiParamLayout::new(abi_types.into_boxed_slice())))
    }

    fn lower_abi_decode_values(
        &mut self,
        data: ValueId,
        types: &[Ty<'gcx>],
        span: Span,
    ) -> Option<Vec<ValueId>> {
        let (data, layout) = self.lower_abi_decode_layout(data, types, span)?;
        let length = self.builder.memory_object_len(data, MemoryObjectKind::Bytes);
        let base = self.builder.memory_object_data(data, MemoryObjectKind::Bytes);
        crate::transform::lower_abi::decode_memory_tuple(
            &mut self.builder,
            base,
            length,
            &layout,
            None,
        )
    }

    fn revert_external_call(&mut self, success: ValueId) {
        let revert = self.builder.create_block();
        let continue_block = self.builder.create_block();
        self.builder.branch(success, continue_block, revert);
        self.builder.switch_to_block(revert);
        let zero = self.builder.imm_u256(U256::ZERO);
        if self.gcx.sess.opts.evm_version.supports_returndata() {
            let size = self.builder.returndatasize();
            self.builder.returndatacopy(zero, zero, size);
            self.builder.revert(zero, size);
        } else {
            self.builder.revert(zero, zero);
        }
        self.builder.switch_to_block(continue_block);
    }

    fn materialize_memory_slice(&mut self, slice: ValueId) -> ValueId {
        let length = self.builder.slice_len(slice);
        let thirty_one = self.builder.imm_u64(31);
        let rounded = self.checked_add(length, thirty_one);
        let word_size = self.builder.imm_u64(32);
        let words = self.builder.div(rounded, word_size);
        let one = self.builder.imm_u64(1);
        let total_words = self.checked_add(words, one);
        let size = self.checked_mul(total_words, word_size);
        let object = self.builder.alloc_object(
            size,
            MemoryObjectLayout::Bytes,
            AllocationSemantics::INTERNAL,
        );
        self.builder.set_memory_object_len(object, length, MemoryObjectKind::Bytes);
        self.builder.memory_object_copy_from_slice(object, MemoryObjectKind::Bytes, slice);
        object
    }

    fn materialize_returndata_bytes(&mut self) -> ValueId {
        let length = self.builder.returndatasize();
        let thirty_one = self.builder.imm_u64(31);
        let rounded = self.checked_add(length, thirty_one);
        let word_size = self.builder.imm_u64(32);
        let words = self.builder.div(rounded, word_size);
        let one = self.builder.imm_u64(1);
        let total_words = self.checked_add(words, one);
        let size = self.checked_mul(total_words, word_size);
        let object = self.builder.alloc_object(
            size,
            MemoryObjectLayout::Bytes,
            AllocationSemantics::INTERNAL,
        );
        self.builder.set_memory_object_len(object, length, MemoryObjectKind::Bytes);
        let zero = self.builder.imm_u256(U256::ZERO);
        let source = self.builder.make_slice(zero, length, SliceLocation::Returndata);
        self.builder.memory_object_copy_from_slice(object, MemoryObjectKind::Bytes, source);
        object
    }

    fn lower_error_catch_string(&mut self, data: ValueId, span: Span) -> Option<ValueId> {
        let data_ptr = self.builder.memory_object_data(data, MemoryObjectKind::Bytes);
        let data_len = self.builder.memory_object_len(data, MemoryObjectKind::Bytes);
        let four = self.builder.imm_u64(4);
        let payload_ptr = self.builder.add(data_ptr, four);
        let payload_len = self.builder.sub(data_len, four);
        let layout = AbiParamLayout::new(vec![AbiParamType::Bytes].into_boxed_slice());
        crate::transform::lower_abi::decode_memory_tuple(
            &mut self.builder,
            payload_ptr,
            payload_len,
            &layout,
            None,
        )
        .and_then(|mut values| values.pop())
        .or_else(|| report_unsupported(self.gcx, span, "Error catch payload"))
    }

    fn lower_panic_catch_word(&mut self, data: ValueId) -> ValueId {
        let data_ptr = self.builder.memory_object_data(data, MemoryObjectKind::Bytes);
        let zero = self.builder.imm_u256(U256::ZERO);
        let four = self.builder.imm_u64(4);
        let payload_ptr = self.builder.add(data_ptr, four);
        let word_size = self.builder.imm_u64(32);
        let payload = self.builder.make_slice(payload_ptr, word_size, SliceLocation::Memory);
        self.builder.memory_slice_load_word(payload, zero)
    }

    fn lower_abi_encode_packed(&mut self, args: hir::CallArgs<'_>) -> Option<ValueId> {
        let exprs = self.variadic_builtin_args(Builtin::AbiEncodePacked, &args)?;
        let mut total = self.builder.imm_u64(0);
        let mut pieces = Vec::with_capacity(exprs.len());
        for expr in exprs {
            let ty = self.gcx.type_of_expr(expr.id)?;
            if let ExprKind::Lit(lit) = &expr.kind
                && let LitKind::Str(_, bytes, _) = &lit.kind
            {
                let bytes = bytes.as_byte_str().to_vec();
                let length = self.builder.imm_u64(bytes.len() as u64);
                total = self.checked_add(total, length);
                pieces.push(PackedPiece::Bytes(bytes));
                continue;
            }

            let value = self.lower_expr(expr)?;
            if self.is_calldata_dynamic_bytes_type(ty)
                || matches!(
                    ty.peel_refs().kind,
                    TyKind::Elementary(
                        solar_sema::hir::ElementaryType::Bytes
                            | solar_sema::hir::ElementaryType::String,
                    )
                )
            {
                let value_ty = self.builder.func().value_ty(value);
                let is_slice = matches!(
                    value_ty,
                    Some(MirType::Slice(SliceLocation::Calldata | SliceLocation::Memory))
                );
                let length = if is_slice {
                    self.builder.slice_len(value)
                } else {
                    self.builder.memory_object_len(value, MemoryObjectKind::Bytes)
                };
                total = self.checked_add(total, length);
                let source = if is_slice {
                    value
                } else {
                    let pointer = self.builder.memory_object_data(value, MemoryObjectKind::Bytes);
                    self.builder.make_slice(pointer, length, SliceLocation::Memory)
                };
                pieces.push(PackedPiece::Dynamic { source, length });
                continue;
            }

            if let Some((length, source)) = self.packed_array_shape(ty, value) {
                let word = self.builder.imm_u64(32);
                let byte_length = self.checked_mul(length, word);
                total = self.checked_add(total, byte_length);
                pieces.push(PackedPiece::Array { value, length, source });
                continue;
            }

            let Some((length, fixed_bytes)) = self.packed_static_shape(ty) else {
                return report_unsupported(self.gcx, expr.span, "abi.encodePacked argument");
            };
            let length_value = self.builder.imm_u64(length);
            total = self.checked_add(total, length_value);
            pieces.push(PackedPiece::Static { value, length, fixed_bytes });
        }

        let thirty_one = self.builder.imm_u64(31);
        let rounded = self.checked_add(total, thirty_one);
        let word_size = self.builder.imm_u64(32);
        let words = self.builder.div(rounded, word_size);
        let one = self.builder.imm_u64(1);
        let words = self.checked_add(words, one);
        let size = self.checked_mul(words, word_size);
        let output = self.builder.alloc_object(
            size,
            MemoryObjectLayout::Bytes,
            AllocationSemantics::INTERNAL,
        );
        self.builder.set_memory_object_len(output, total, MemoryObjectKind::Bytes);

        let mut offset = self.builder.imm_u64(0);
        let mut index = 0;
        while index < pieces.len() {
            if let Some((consumed, length)) =
                self.try_write_packed_word(output, offset, &pieces[index..])
            {
                let length = self.builder.imm_u64(length);
                offset = self.checked_add(offset, length);
                index += consumed;
                continue;
            }

            match &pieces[index] {
                PackedPiece::Bytes(bytes) => {
                    for chunk in bytes.chunks(32) {
                        let mut padded = [0u8; 32];
                        padded[..chunk.len()].copy_from_slice(chunk);
                        let value = self.builder.imm_u256(U256::from_be_bytes(padded));
                        self.builder.memory_object_store_word(output, offset, value);
                        let length = self.builder.imm_u64(chunk.len() as u64);
                        offset = self.checked_add(offset, length);
                    }
                }
                PackedPiece::Dynamic { source, length } => {
                    self.builder.memory_object_copy_from_slice_at(
                        output,
                        MemoryObjectKind::Bytes,
                        offset,
                        *source,
                    );
                    offset = self.checked_add(offset, *length);
                }
                PackedPiece::Array { value, length, source } => {
                    offset = self.copy_packed_array(output, offset, *value, *length, *source);
                }
                PackedPiece::Static { value, length, fixed_bytes } => {
                    let value = if *fixed_bytes || *length == 32 {
                        *value
                    } else {
                        let shift = self.builder.imm_u64((32 - *length) * 8);
                        self.builder.shl(shift, *value)
                    };
                    self.builder.memory_object_store_word(output, offset, value);
                    let length = self.builder.imm_u64(*length);
                    offset = self.checked_add(offset, length);
                }
            }
            index += 1;
        }
        Some(output)
    }

    fn packed_array_shape(
        &mut self,
        ty: Ty<'gcx>,
        value: ValueId,
    ) -> Option<(ValueId, PackedArraySource)> {
        let element = match ty.peel_refs().kind {
            TyKind::DynArray(element) | TyKind::Slice(element) | TyKind::Array(element, _) => {
                element
            }
            _ => return None,
        };
        if !matches!(self.types.abi_type(element)?, AbiType::Word) {
            return None;
        }

        let layout = self.types.memory_layout(ty)?;
        let source = match self.builder.func().value_ty(value) {
            Some(MirType::MemoryObject(
                MemoryObjectKind::DynamicArray | MemoryObjectKind::FixedArray,
            )) => PackedArraySource::Memory { layout },
            Some(MirType::Slice(location @ (SliceLocation::Memory | SliceLocation::Calldata))) => {
                PackedArraySource::Slice(location)
            }
            _ => return None,
        };
        let length = match source {
            PackedArraySource::Memory { layout: MemoryObjectLayout::DynamicArray { .. } } => {
                self.builder.memory_object_len(value, MemoryObjectKind::DynamicArray)
            }
            PackedArraySource::Memory { layout: MemoryObjectLayout::FixedArray { len, .. } } => {
                self.builder.imm_u64(len)
            }
            PackedArraySource::Memory { .. } => return None,
            PackedArraySource::Slice(_) => self.builder.slice_len(value),
        };
        Some((length, source))
    }

    fn lower_packed_word_array(&mut self, ty: Ty<'gcx>, value: ValueId) -> Option<ValueId> {
        let (length, source) = self.packed_array_shape(ty, value)?;
        let word = self.builder.imm_u64(32);
        let byte_length = self.checked_mul(length, word);
        let size = self.checked_add(word, byte_length);
        let output = self.builder.alloc_object(
            size,
            MemoryObjectLayout::Bytes,
            AllocationSemantics::INTERNAL,
        );
        self.builder.set_memory_object_len(output, byte_length, MemoryObjectKind::Bytes);
        let offset = self.builder.imm_u64(0);
        self.copy_packed_array(output, offset, value, length, source);
        Some(output)
    }

    fn copy_packed_array(
        &mut self,
        output: ValueId,
        offset: ValueId,
        value: ValueId,
        length: ValueId,
        source: PackedArraySource,
    ) -> ValueId {
        let word = self.builder.imm_u64(32);
        let byte_length = self.checked_mul(length, word);
        let end_offset = self.checked_add(offset, byte_length);
        let base = match source {
            PackedArraySource::Memory { .. } => None,
            PackedArraySource::Slice(_) => Some(self.builder.slice_ptr(value)),
        };

        let preheader = self.builder.current_block();
        let header = self.builder.create_block();
        let body = self.builder.create_block();
        let exit = self.builder.create_block();
        self.builder.jump(header);

        self.builder.switch_to_block(header);
        let zero = self.builder.imm_u64(0);
        let index = self.builder.phi(vec![(preheader, zero)]);
        let more = self.builder.lt(index, length);
        self.builder.branch(more, body, exit);

        self.builder.switch_to_block(body);
        let element_offset = self.checked_mul(index, word);
        let destination = self.checked_add(offset, element_offset);
        let element = match source {
            PackedArraySource::Memory { layout } => {
                self.builder.memory_object_load_element(value, layout, index)
            }
            PackedArraySource::Slice(location) => {
                let pointer = self.builder.add(base.expect("slice base"), element_offset);
                match location {
                    SliceLocation::Memory => self.builder.mload(pointer),
                    SliceLocation::Calldata => self.builder.calldataload(pointer),
                    SliceLocation::Returndata => unreachable!("returndata packed array"),
                }
            }
        };
        self.builder.memory_object_store_word(output, destination, element);
        let one = self.builder.imm_u64(1);
        let next = self.builder.add(index, one);
        let backedge = self.builder.current_block();
        self.builder.jump(header);
        self.builder.add_phi_incoming(index, backedge, next);

        self.builder.switch_to_block(exit);
        end_offset
    }

    fn is_calldata_dynamic_bytes_type(&self, ty: Ty<'gcx>) -> bool {
        match ty.kind {
            TyKind::Ref(inner, DataLocation::Calldata) => matches!(
                inner.kind,
                TyKind::Elementary(
                    solar_sema::hir::ElementaryType::Bytes
                        | solar_sema::hir::ElementaryType::String,
                )
            ),
            TyKind::Slice(inner) => {
                inner.is_ref_at(DataLocation::Calldata)
                    && matches!(
                        inner.peel_refs().kind,
                        TyKind::Elementary(
                            solar_sema::hir::ElementaryType::Bytes
                                | solar_sema::hir::ElementaryType::String,
                        )
                    )
            }
            _ => false,
        }
    }

    fn try_write_packed_word(
        &mut self,
        output: ValueId,
        offset: ValueId,
        pieces: &[PackedPiece],
    ) -> Option<(usize, u64)> {
        let mut constant = U256::ZERO;
        let mut terms = Vec::new();
        let mut length = 0u64;
        let mut consumed = 0;

        for piece in pieces {
            match piece {
                PackedPiece::Bytes(bytes) => {
                    let piece_length = u64::try_from(bytes.len()).ok()?;
                    if piece_length == 0 {
                        consumed += 1;
                        continue;
                    }
                    if length.checked_add(piece_length)? > 32 {
                        break;
                    }
                    let shift = (32 - length - piece_length) * 8;
                    constant |= U256::from_be_slice(bytes) << shift;
                    length += piece_length;
                    consumed += 1;
                }
                PackedPiece::Static { value, length: piece_length, fixed_bytes: false }
                    if *piece_length < 32 =>
                {
                    if *piece_length == 0 {
                        consumed += 1;
                        continue;
                    }
                    if length.checked_add(*piece_length)? > 32 {
                        break;
                    }
                    let shift = (32 - length - *piece_length) * 8;
                    terms.push((*value, shift));
                    length += *piece_length;
                    consumed += 1;
                }
                _ => break,
            }
        }

        if consumed < 2 || length == 0 || terms.is_empty() {
            return None;
        }

        let mut value = self.builder.imm_u256(constant);
        for (term, shift) in terms {
            let term = if shift == 0 {
                term
            } else {
                let shift = self.builder.imm_u64(shift);
                self.builder.shl(shift, term)
            };
            value = self.builder.or(value, term);
        }
        self.builder.memory_object_store_word(output, offset, value);
        Some((consumed, length))
    }

    fn packed_static_shape(&self, ty: Ty<'gcx>) -> Option<(u64, bool)> {
        match ty.peel_refs().kind {
            TyKind::Elementary(elementary) => Some(match elementary {
                solar_sema::hir::ElementaryType::Bool => (1, false),
                solar_sema::hir::ElementaryType::Address(_) => (20, false),
                solar_sema::hir::ElementaryType::Int(size)
                | solar_sema::hir::ElementaryType::UInt(size)
                | solar_sema::hir::ElementaryType::Fixed(size, _)
                | solar_sema::hir::ElementaryType::UFixed(size, _) => {
                    (u64::from(size.bytes()), false)
                }
                solar_sema::hir::ElementaryType::FixedBytes(size) => {
                    (u64::from(size.bytes()), true)
                }
                _ => return None,
            }),
            TyKind::Contract(_) => Some((20, false)),
            TyKind::Enum(id) => {
                let variants = self.gcx.hir.enumm(id).variants.len().max(1);
                let bits = (usize::BITS - (variants - 1).leading_zeros()).max(1);
                Some((u64::from(bits.div_ceil(8)), false))
            }
            TyKind::Udvt(inner, _) => self.packed_static_shape(inner),
            TyKind::IntLiteral(..) => Some((32, false)),
            _ => None,
        }
    }

    fn lower_hash_precompile_call(
        &mut self,
        builtin: Builtin,
        args: hir::CallArgs<'_>,
    ) -> Option<ValueId> {
        let input = &self.builtin_args::<1>(builtin, &args)?[0];
        let input_ty = self.gcx.type_of_expr(input.id)?;
        if !matches!(self.types.memory_layout(input_ty)?, MemoryObjectLayout::Bytes) {
            return report_unsupported(self.gcx, input.span, "precompile input");
        }
        let input = self.lower_expr(input)?;
        let input_ptr = self.builder.memory_object_data(input, MemoryObjectKind::Bytes);
        let input_len = self.builder.memory_object_len(input, MemoryObjectKind::Bytes);

        let output_size = self.builder.imm_u64(64);
        let output = self.builder.alloc_object(
            output_size,
            MemoryObjectLayout::Bytes,
            AllocationSemantics::SOLIDITY_ZEROED,
        );
        let output_len = self.builder.imm_u64(32);
        self.builder.set_memory_object_len(output, output_len, MemoryObjectKind::Bytes);
        let output_ptr = self.builder.memory_object_data(output, MemoryObjectKind::Bytes);
        let address = self.builder.imm_u64(if builtin == Builtin::Sha256 { 2 } else { 3 });
        let output_size = self.builder.imm_u64(32);
        self.lower_precompile_call(address, input_ptr, input_len, output_ptr, output_size);
        let value = self.builder.mload(output_ptr);
        Some(if builtin == Builtin::Ripemd160 {
            let scale = self.builder.imm_u256(U256::from(1) << 96);
            self.builder.mul(scale, value)
        } else {
            value
        })
    }

    fn lower_ecrecover_call(&mut self, args: hir::CallArgs<'_>) -> Option<ValueId> {
        let values = self.builtin_args::<4>(Builtin::EcRecover, &args)?;
        let hash = &values[0];
        let v = &values[1];
        let r = &values[2];
        let s = &values[3];
        let hash = self.lower_expr(hash)?;
        let v = self.lower_expr(v)?;
        let r = self.lower_expr(r)?;
        let s = self.lower_expr(s)?;

        let input_size = self.builder.imm_u64(192);
        let input = self.builder.alloc_object(
            input_size,
            MemoryObjectLayout::Bytes,
            AllocationSemantics::SOLIDITY_ZEROED,
        );
        let input_len = self.builder.imm_u64(160);
        self.builder.set_memory_object_len(input, input_len, MemoryObjectKind::Bytes);
        let input_ptr = self.builder.memory_object_data(input, MemoryObjectKind::Bytes);
        self.builder.mstore(input_ptr, hash);
        for (offset, value) in [(32, v), (64, r), (96, s)] {
            let offset = self.builder.imm_u64(offset);
            let ptr = self.builder.add(input_ptr, offset);
            self.builder.mstore(ptr, value);
        }
        let output_offset = self.builder.imm_u64(128);
        let output_ptr = self.builder.add(input_ptr, output_offset);
        let zero = self.builder.imm_u256(U256::ZERO);
        self.builder.mstore(output_ptr, zero);

        let address = self.builder.imm_u64(1);
        let input_size = self.builder.imm_u64(128);
        let output_size = self.builder.imm_u64(32);
        self.lower_precompile_call(address, input_ptr, input_size, output_ptr, output_size);
        Some(self.builder.mload(output_ptr))
    }

    fn lower_precompile_call(
        &mut self,
        address: ValueId,
        input_ptr: ValueId,
        input_size: ValueId,
        output_ptr: ValueId,
        output_size: ValueId,
    ) {
        let evm_version = self.gcx.sess.opts.evm_version;
        let gas = crate::utils::precompile_gas(&mut self.builder, evm_version);
        if evm_version.has_static_call() {
            self.builder.staticcall(gas, address, input_ptr, input_size, output_ptr, output_size);
        } else {
            let value = self.builder.imm_u256(U256::ZERO);
            self.builder.call(gas, address, value, input_ptr, input_size, output_ptr, output_size);
        }
    }

    fn lower_yul_unit_builtin_call(
        &mut self,
        builtin: Builtin,
        args: hir::CallArgs<'_>,
    ) -> Option<()> {
        match builtin {
            Builtin::YulMstore => {
                let [offset, value] = self.lower_builtin_args(builtin, &args)?;
                self.builder.mstore(offset, value);
            }
            Builtin::YulMstore8 => {
                let [offset, value] = self.lower_builtin_args(builtin, &args)?;
                self.builder.mstore8(offset, value);
            }
            Builtin::YulMcopy => {
                if !self.gcx.sess.opts.evm_version.has_mcopy() {
                    return self.unsupported_yul_version(
                        "codegen requires Cancun-compatible EVM for memory copy",
                        "compile with `--evm-version cancun` or newer",
                        args.span,
                    );
                }
                let [dest, src, size] = self.lower_builtin_args(builtin, &args)?;
                self.builder.mcopy(dest, src, size);
            }
            Builtin::YulSstore => {
                let [slot, value] = self.lower_builtin_args(builtin, &args)?;
                self.builder.sstore(slot, value);
            }
            Builtin::YulTstore => {
                let [slot, value] = self.lower_builtin_args(builtin, &args)?;
                self.builder.tstore(slot, value);
            }
            Builtin::YulCalldatacopy => {
                let [dest, src, size] = self.lower_builtin_args(builtin, &args)?;
                self.builder.calldatacopy(dest, src, size);
            }
            Builtin::YulCodecopy => {
                let [dest, src, size] = self.lower_builtin_args(builtin, &args)?;
                self.builder.codecopy(dest, src, size);
            }
            Builtin::YulExtcodecopy => {
                let [address, dest, src, size] = self.lower_builtin_args(builtin, &args)?;
                self.builder.extcodecopy(address, dest, src, size);
            }
            Builtin::YulReturndatacopy => {
                let [dest, src, size] = self.lower_builtin_args(builtin, &args)?;
                self.builder.returndatacopy(dest, src, size);
            }
            Builtin::YulLog0 => {
                let [offset, size] = self.lower_builtin_args(builtin, &args)?;
                self.builder.log0(offset, size);
            }
            Builtin::YulLog1 => {
                let [offset, size, topic1] = self.lower_builtin_args(builtin, &args)?;
                self.builder.log1(offset, size, topic1);
            }
            Builtin::YulLog2 => {
                let [offset, size, topic1, topic2] = self.lower_builtin_args(builtin, &args)?;
                self.builder.log2(offset, size, topic1, topic2);
            }
            Builtin::YulLog3 => {
                let [offset, size, topic1, topic2, topic3] =
                    self.lower_builtin_args(builtin, &args)?;
                self.builder.log3(offset, size, topic1, topic2, topic3);
            }
            Builtin::YulLog4 => {
                let [offset, size, topic1, topic2, topic3, topic4] =
                    self.lower_builtin_args(builtin, &args)?;
                self.builder.log4(offset, size, topic1, topic2, topic3, topic4);
            }
            Builtin::YulRevert => {
                let [offset, size] = self.lower_builtin_args(builtin, &args)?;
                self.builder.revert(offset, size);
            }
            Builtin::YulReturn => {
                let [offset, size] = self.lower_builtin_args(builtin, &args)?;
                self.builder.ret_data(offset, size);
            }
            Builtin::YulStop => {
                let [] = self.lower_builtin_args(builtin, &args)?;
                self.builder.stop();
            }
            Builtin::YulInvalid => {
                let [] = self.lower_builtin_args(builtin, &args)?;
                self.builder.invalid();
            }
            Builtin::YulSelfdestruct => {
                let [address] = self.lower_builtin_args(builtin, &args)?;
                self.builder.selfdestruct(address);
            }
            Builtin::YulPop => {
                let [_value] = self.lower_builtin_args(builtin, &args)?;
            }
            _ => {
                return report_error(
                    self.gcx,
                    args.span,
                    "codegen routed a value Yul builtin through unit lowering",
                );
            }
        }
        Some(())
    }

    fn lower_yul_value_builtin_call(
        &mut self,
        builtin: Builtin,
        args: hir::CallArgs<'_>,
    ) -> Option<ValueId> {
        macro_rules! lower {
            ($method:ident($($arg:ident),* $(,)?)) => {{
                let [$($arg),*] = self.lower_builtin_args(builtin, &args)?;
                Some(self.builder.$method($($arg),*))
            }};
        }
        match builtin {
            Builtin::YulAdd => lower!(add(lhs, rhs)),
            Builtin::YulSub => lower!(sub(lhs, rhs)),
            Builtin::YulMul => lower!(mul(lhs, rhs)),
            Builtin::YulDiv => lower!(div(lhs, rhs)),
            Builtin::YulSdiv => lower!(sdiv(lhs, rhs)),
            Builtin::YulMod => lower!(mod_(lhs, rhs)),
            Builtin::YulSmod => lower!(smod(lhs, rhs)),
            Builtin::YulExp => lower!(exp(base, exponent)),
            Builtin::YulSignextend => lower!(signextend(byte, value)),
            Builtin::YulEq => lower!(eq(lhs, rhs)),
            Builtin::YulLt => lower!(lt(lhs, rhs)),
            Builtin::YulGt => lower!(gt(lhs, rhs)),
            Builtin::YulSlt => lower!(slt(lhs, rhs)),
            Builtin::YulSgt => lower!(sgt(lhs, rhs)),
            Builtin::YulAnd => lower!(and(lhs, rhs)),
            Builtin::YulOr => lower!(or(lhs, rhs)),
            Builtin::YulXor => lower!(xor(lhs, rhs)),
            Builtin::YulNot => lower!(not(value)),
            Builtin::YulByte => lower!(byte(index, value)),
            Builtin::YulShl => lower!(shl(shift, value)),
            Builtin::YulShr => lower!(shr(shift, value)),
            Builtin::YulSar => lower!(sar(shift, value)),
            Builtin::YulIszero => lower!(iszero(value)),
            Builtin::YulAddmod => lower!(addmod(a, b, modulus)),
            Builtin::YulMulmod => lower!(mulmod(a, b, modulus)),
            Builtin::YulClz => {
                let [value] = self.lower_builtin_args(builtin, &args)?;
                if !self.gcx.sess.opts.evm_version.has_clz() {
                    return self.unsupported_yul_version(
                        "codegen requires Osaka-compatible EVM for `clz`",
                        "compile with `--evm-version osaka` or newer",
                        args.span,
                    );
                }
                Some(self.builder.clz(value))
            }
            Builtin::YulMload => lower!(mload(offset)),
            Builtin::YulMsize => lower!(msize()),
            Builtin::YulSload => lower!(sload(slot)),
            Builtin::YulTload => lower!(tload(slot)),
            Builtin::YulCalldataload => lower!(calldataload(offset)),
            Builtin::YulCalldatasize => lower!(calldatasize()),
            Builtin::YulCodesize => lower!(codesize()),
            Builtin::YulExtcodesize => lower!(extcodesize(address)),
            Builtin::YulExtcodehash => lower!(extcodehash(address)),
            Builtin::YulReturndatasize => lower!(returndatasize()),
            Builtin::YulAddress => lower!(address()),
            Builtin::YulBalance => lower!(balance(address)),
            Builtin::YulSelfbalance => lower!(selfbalance()),
            Builtin::YulCaller => lower!(caller()),
            Builtin::YulCallvalue => lower!(callvalue()),
            Builtin::YulOrigin => lower!(origin()),
            Builtin::YulGasprice => lower!(gasprice()),
            Builtin::YulBlockhash => lower!(blockhash(number)),
            Builtin::YulCoinbase => lower!(coinbase()),
            Builtin::YulTimestamp => lower!(timestamp()),
            Builtin::YulNumber => lower!(number()),
            Builtin::YulDifficulty | Builtin::YulPrevrandao => lower!(prevrandao()),
            Builtin::YulGaslimit => lower!(gaslimit()),
            Builtin::YulChainid => lower!(chainid()),
            Builtin::YulGas => lower!(gas()),
            Builtin::YulBasefee => lower!(basefee()),
            Builtin::YulBlobbasefee => lower!(blobbasefee()),
            Builtin::YulBlobhash => lower!(blobhash(index)),
            Builtin::YulKeccak256 => lower!(keccak256(offset, size)),
            Builtin::YulCall => {
                lower!(call(gas, address, value, in_offset, in_size, out_offset, out_size))
            }
            Builtin::YulCallcode => {
                lower!(callcode(gas, address, value, in_offset, in_size, out_offset, out_size))
            }
            Builtin::YulStaticcall => {
                lower!(staticcall(gas, address, in_offset, in_size, out_offset, out_size))
            }
            Builtin::YulDelegatecall => {
                lower!(delegatecall(gas, address, in_offset, in_size, out_offset, out_size))
            }
            Builtin::YulCreate => lower!(create(value, offset, size)),
            Builtin::YulCreate2 => lower!(create2(value, offset, size, salt)),
            Builtin::YulExtcall => {
                let [address, input_offset, input_size, value] =
                    self.lower_builtin_args(builtin, &args)?;
                if !self.gcx.sess.opts.evm_version.has_ext_call() {
                    return self.unsupported_yul_version(
                        "codegen requires Prague-compatible EVM for `extcall`",
                        "compile with `--evm-version prague` or newer",
                        args.span,
                    );
                }
                Some(self.builder.extcall(address, input_offset, input_size, value))
            }
            Builtin::YulExtdelegatecall => {
                let [address, input_offset, input_size] =
                    self.lower_builtin_args(builtin, &args)?;
                if !self.gcx.sess.opts.evm_version.has_ext_call() {
                    return self.unsupported_yul_version(
                        "codegen requires Prague-compatible EVM for `extdelegatecall`",
                        "compile with `--evm-version prague` or newer",
                        args.span,
                    );
                }
                Some(self.builder.extdelegatecall(address, input_offset, input_size))
            }
            Builtin::YulExtstaticcall => {
                let [address, input_offset, input_size] =
                    self.lower_builtin_args(builtin, &args)?;
                if !self.gcx.sess.opts.evm_version.has_ext_call() {
                    return self.unsupported_yul_version(
                        "codegen requires Prague-compatible EVM for `extstaticcall`",
                        "compile with `--evm-version prague` or newer",
                        args.span,
                    );
                }
                Some(self.builder.extstaticcall(address, input_offset, input_size))
            }
            _ => report_error(
                self.gcx,
                args.span,
                "codegen routed a unit Yul builtin through value lowering",
            ),
        }
    }

    fn emit_wrong_builtin_arg_count(
        &self,
        builtin: Builtin,
        span: Span,
        expected: BuiltinArgCount,
        actual: usize,
    ) {
        let kind = if builtin.is_yul() { "Yul builtin" } else { "builtin" };
        let expected = expected.description();
        self.gcx
            .dcx()
            .err(format!(
                "wrong number of arguments for {kind} `{}`: expected {expected}, found {actual}",
                builtin.name()
            ))
            .span(span)
            .emit();
    }

    fn builtin_arg_exprs<'hir>(
        &self,
        builtin: Builtin,
        args: &hir::CallArgs<'hir>,
    ) -> Option<&'hir [hir::Expr<'hir>]> {
        match args.kind {
            hir::CallArgsKind::Unnamed(exprs) => Some(exprs),
            hir::CallArgsKind::Named(_) => {
                let kind = if builtin.is_yul() { "Yul builtin" } else { "builtin" };
                self.gcx
                    .dcx()
                    .err(format!(
                        "named arguments are not supported for {kind} `{}` in codegen",
                        builtin.name()
                    ))
                    .span(args.span)
                    .emit();
                None
            }
        }
    }

    fn builtin_args<'hir, const N: usize>(
        &self,
        builtin: Builtin,
        args: &hir::CallArgs<'hir>,
    ) -> Option<&'hir [hir::Expr<'hir>]> {
        let exprs = self.builtin_arg_exprs(builtin, args)?;
        if exprs.len() == N {
            return Some(exprs);
        }
        self.emit_wrong_builtin_arg_count(
            builtin,
            args.span,
            BuiltinArgCount::Exact(N),
            exprs.len(),
        );
        None
    }

    fn builtin_args_with_rest<'hir, const N: usize>(
        &self,
        builtin: Builtin,
        args: &hir::CallArgs<'hir>,
    ) -> Option<(&'hir [hir::Expr<'hir>], &'hir [hir::Expr<'hir>])> {
        let exprs = self.builtin_arg_exprs(builtin, args)?;
        if exprs.len() < N {
            self.emit_wrong_builtin_arg_count(
                builtin,
                args.span,
                BuiltinArgCount::AtLeast(N),
                exprs.len(),
            );
            return None;
        }
        Some(exprs.split_at(N))
    }

    fn builtin_args_with_optional<'hir, const N: usize>(
        &self,
        builtin: Builtin,
        args: &hir::CallArgs<'hir>,
    ) -> Option<(&'hir [hir::Expr<'hir>], Option<&'hir hir::Expr<'hir>>)> {
        let exprs = self.builtin_arg_exprs(builtin, args)?;
        if (N..=N + 1).contains(&exprs.len()) {
            let (required, optional) = exprs.split_at(N);
            return Some((required, optional.first()));
        }
        self.emit_wrong_builtin_arg_count(
            builtin,
            args.span,
            BuiltinArgCount::Between(N, N + 1),
            exprs.len(),
        );
        None
    }

    fn variadic_builtin_args<'hir>(
        &self,
        builtin: Builtin,
        args: &hir::CallArgs<'hir>,
    ) -> Option<&'hir [hir::Expr<'hir>]> {
        self.builtin_arg_exprs(builtin, args)
    }

    fn validate_builtin_arity(&self, builtin: Builtin, args: &hir::CallArgs<'_>) -> bool {
        let Some(exprs) = self.builtin_arg_exprs(builtin, args) else {
            return false;
        };
        let TyKind::Fn(function) = builtin.ty(self.gcx).kind else {
            return true;
        };
        let variadic =
            function.parameters.last().is_some_and(|ty| matches!(ty.kind, TyKind::Variadic));
        let (valid, expected) = if variadic {
            let minimum = function.parameters.len().saturating_sub(1);
            (exprs.len() >= minimum, BuiltinArgCount::AtLeast(minimum))
        } else {
            let expected = function.parameters.len();
            (exprs.len() == expected, BuiltinArgCount::Exact(expected))
        };
        if !valid {
            self.emit_wrong_builtin_arg_count(builtin, args.span, expected, exprs.len());
        }
        valid
    }

    fn lower_builtin_args<const N: usize>(
        &mut self,
        builtin: Builtin,
        args: &hir::CallArgs<'_>,
    ) -> Option<[ValueId; N]> {
        let exprs = self.builtin_args::<N>(builtin, args)?;
        let values = exprs.iter().map(|arg| self.lower_expr(arg)).collect::<Option<Vec<_>>>()?;
        values.try_into().ok()
    }

    fn unsupported_builtin<T>(&self, builtin: Builtin, span: Span) -> Option<T> {
        self.gcx
            .dcx()
            .err(format!("unsupported builtin call `{}`", builtin.name()))
            .span(span)
            .emit();
        None
    }

    fn unsupported_yul_version<T>(
        &self,
        message: &'static str,
        help: &'static str,
        span: Span,
    ) -> Option<T> {
        self.gcx.dcx().err(message).span(span).help(help).emit();
        None
    }

    fn panic_if(&mut self, condition: ValueId, code: u64) {
        let panic_block = self.builder.create_block();
        let continue_block = self.builder.create_block();
        self.builder.branch(condition, panic_block, continue_block);
        self.builder.switch_to_block(panic_block);
        let selector = self.builder.imm_u256(U256::from(0x4e48_7b71_u64) << 224);
        let code = self.builder.imm_u256(U256::from(code));
        let zero = self.builder.imm_u256(U256::ZERO);
        self.builder.mstore(zero, selector);
        let four = self.builder.imm_u256(U256::from(4));
        self.builder.mstore(four, code);
        let size = self.builder.imm_u256(U256::from(36));
        self.builder.revert(zero, size);
        self.builder.switch_to_block(continue_block);
    }

    fn checked_add(&mut self, lhs: ValueId, rhs: ValueId) -> ValueId {
        let result = self.builder.add(lhs, rhs);
        let overflow = self.builder.lt(result, lhs);
        self.panic_if(overflow, 0x41);
        result
    }

    fn checked_mul(&mut self, lhs: ValueId, rhs: ValueId) -> ValueId {
        let result = self.builder.mul(lhs, rhs);
        let rhs_zero = self.builder.iszero(rhs);
        let quotient = self.builder.div(result, rhs);
        let exact = self.builder.eq(quotient, lhs);
        let valid = self.builder.or(rhs_zero, exact);
        let overflow = self.builder.iszero(valid);
        self.panic_if(overflow, 0x41);
        result
    }

    fn bounds_check(&mut self, index: ValueId, length: ValueId) {
        let in_range = self.builder.lt(index, length);
        let invalid = self.builder.iszero(in_range);
        self.panic_if(invalid, 0x32);
    }

    fn lower_function_call(
        &mut self,
        expr: &hir::Expr<'_>,
        callee: &hir::Expr<'_>,
        function_id: hir::FunctionId,
        args: hir::CallArgs<'_>,
        call_opts: Option<&hir::CallOptions<'_>>,
    ) -> Option<ValueId> {
        let function_id = self.resolve_call_target(callee, function_id);
        let function = self.gcx.hir.function(function_id);
        let attached = self.gcx.resolved_callee(callee.id).is_some_and(|callee| callee.attached);
        if !attached
            && matches!(function.visibility, hir::Visibility::Public | hir::Visibility::External)
            && let Some(address) = self.linked_library_address(function_id)
        {
            return self.lower_linked_library_call(expr, function_id, args, address);
        }
        let receiver_count = usize::from(attached);
        if args.len() + receiver_count != function.parameters.len() {
            return report_unsupported(self.gcx, expr.span, "function argument list");
        }
        let parameter_names =
            self.gcx.call_param_source(callee).map(|source| self.gcx.callable_param_names(source));
        let mut values = Vec::with_capacity(function.parameters.len());
        if attached {
            let ExprKind::Member(receiver, _) = callee.kind else {
                return report_unsupported(self.gcx, expr.span, "attached function receiver");
            };
            let parameter_ty = self.gcx.type_of_item(function.parameters[0].into());
            let value = if Self::is_storage_parameter(parameter_ty) {
                self.storage_access(receiver)?.slot
            } else {
                self.lower_expr(receiver)?
            };
            values.push(self.materialize_call_argument(parameter_ty, value, receiver.span)?);
        }
        for index in receiver_count..function.parameters.len() {
            let Some(argument) =
                args.argument_for_parameter(index - receiver_count, parameter_names.as_deref())
            else {
                return report_unsupported(self.gcx, expr.span, "named function argument");
            };
            let parameter_ty = self.gcx.type_of_item(function.parameters[index].into());
            let value = if Self::is_storage_parameter(parameter_ty) {
                self.storage_access(argument)?.slot
            } else {
                self.lower_expr(argument)?
            };
            values.push(self.materialize_call_argument(parameter_ty, value, argument.span)?);
        }
        let Some(&mir_id) = self.function_ids.get(&function_id) else {
            return self.lower_external_function_call(expr, callee, function_id, args, call_opts);
        };
        if function.returns.is_empty() {
            self.builder.internal_call_void(mir_id, values, 0);
            return Some(self.builder.imm_u256(U256::ZERO));
        }
        let result_ty = types::TypeLowerer::mir_return_type(
            self.gcx.type_of_item((*function.returns.first()?).into()),
        );
        Some(self.builder.internal_call(mir_id, values, result_ty, function.returns.len()))
    }

    fn lower_external_function_call(
        &mut self,
        expr: &hir::Expr<'_>,
        callee: &hir::Expr<'_>,
        function_id: hir::FunctionId,
        args: hir::CallArgs<'_>,
        call_opts: Option<&hir::CallOptions<'_>>,
    ) -> Option<ValueId> {
        let ExprKind::Member(receiver, _) = callee.kind else {
            return report_unsupported(self.gcx, expr.span, "external function target");
        };
        let function = self.gcx.hir.function(function_id);
        if args.len() != function.parameters.len() {
            return report_unsupported(self.gcx, expr.span, "external function arguments");
        }
        let parameter_names = self.gcx.callable_param_names(CallableParamSource::Function {
            id: function_id,
            skips_receiver: false,
        });
        let mut values = Vec::with_capacity(function.parameters.len());
        let mut types = Vec::with_capacity(function.parameters.len());
        for (index, &parameter) in function.parameters.iter().enumerate() {
            let Some(argument) =
                args.argument_for_parameter(index, Some(parameter_names.as_slice()))
            else {
                return report_unsupported(self.gcx, expr.span, "external function argument");
            };
            let parameter_ty = self.gcx.type_of_item(parameter.into());
            let mut value = self.lower_expr(argument)?;
            let mut abi_type = self.types.abi_type(parameter_ty)?;
            abi_type = self.abi_type_for_value(value, abi_type);
            if self.needs_calldata_materialization(value, &abi_type) {
                value = self.materialize_calldata_argument(parameter_ty, value, argument.span)?;
                abi_type = Self::memory_abi_type(abi_type);
            }
            values.push(value);
            types.push(abi_type);
        }
        let selector = self.gcx.function_selector(function_id).0;
        let selector = self.builder.imm_u256(U256::from_be_slice(&selector) << 224);
        let layout = Arc::new(AbiLayout::new(types.into_boxed_slice()));
        let encoded = self.builder.abi_encode(layout, Some(selector), values.into_boxed_slice());
        let input = self.builder.slice_ptr(encoded);
        let input_size = self.builder.slice_len(encoded);
        let address = self.lower_expr(receiver)?;
        let zero = self.builder.imm_u256(U256::ZERO);
        let mut call_value = zero;
        let mut gas = self.builder.gas();
        if let Some(options) = call_opts {
            for option in options.args {
                let value = self.lower_expr(&option.value)?;
                match option.name.name {
                    kw::Gas => gas = value,
                    sym::value => call_value = value,
                    _ => return report_unsupported(self.gcx, option.name.span, "call option"),
                }
            }
        }
        let returns = function.returns.len();
        let decode_returndata = function.returns.iter().any(|&ret| {
            self.types
                .abi_return_type(self.gcx.type_of_item(ret.into()))
                .is_some_and(|ty| !matches!(ty, AbiType::Word))
        });
        let ret_offset = if !decode_returndata && returns > 1 { input } else { zero };
        let ret_size = if decode_returndata {
            zero
        } else {
            self.builder.imm_u64((returns as u64).saturating_mul(32))
        };
        let success = if matches!(
            function.state_mutability,
            hir::StateMutability::Pure | hir::StateMutability::View
        ) && self.gcx.sess.opts.evm_version.has_static_call()
        {
            self.builder.staticcall(gas, address, input, input_size, ret_offset, ret_size)
        } else {
            self.builder.call(gas, address, call_value, input, input_size, ret_offset, ret_size)
        };
        self.revert_external_call(success);
        if returns == 0 {
            return Some(zero);
        }
        if decode_returndata {
            if !self.gcx.sess.opts.evm_version.supports_returndata() {
                return report_error(
                    self.gcx,
                    expr.span,
                    "codegen cannot decode external function returndata before Byzantium",
                );
            }
            let data = self.materialize_returndata_bytes();
            let return_types = function
                .returns
                .iter()
                .map(|&ret| {
                    self.gcx
                        .type_of_item(ret.into())
                        .with_loc_if_ref(self.gcx, DataLocation::Memory)
                })
                .collect::<Vec<_>>();
            let values = self.lower_abi_decode_values(data, &return_types, expr.span)?;
            if values.len() > 1 {
                let base = self.ensure_multi_return_buffer(values.len());
                for (index, value) in values.iter().copied().enumerate().skip(1) {
                    let offset = self.builder.imm_u64((index as u64).saturating_mul(32));
                    let address = self.builder.add(base, offset);
                    self.builder.mstore(address, value);
                }
            }
            return values.into_iter().next().or(Some(zero));
        }
        if returns > 1 {
            let base = self.builder.imm_u64(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT);
            self.builder.mstore(base, ret_offset);
        }
        Some(self.builder.mload(ret_offset))
    }

    fn linked_library_address(&self, function_id: hir::FunctionId) -> Option<U256> {
        let contract_id = self.gcx.hir.function(function_id).contract?;
        let contract = self.gcx.hir.contract(contract_id);
        if contract.kind != hir::ContractKind::Library {
            return None;
        }
        let source = self.gcx.hir.source(contract.source).file.name.display().to_string();
        self.gcx
            .sess
            .opts
            .libraries
            .iter()
            .find(|library| {
                library.name == contract.name.as_str_in(self.gcx.sess)
                    && library.source.as_ref().is_none_or(|path| source.ends_with(path))
            })
            .map(|library| U256::from_be_slice(library.address.as_slice()))
    }

    fn lower_linked_library_call(
        &mut self,
        expr: &hir::Expr<'_>,
        function_id: hir::FunctionId,
        args: hir::CallArgs<'_>,
        address: U256,
    ) -> Option<ValueId> {
        let function = self.gcx.hir.function(function_id);
        if args.len() != function.parameters.len() {
            return report_unsupported(self.gcx, expr.span, "linked library arguments");
        }
        let parameter_names = self.gcx.callable_param_names(CallableParamSource::Function {
            id: function_id,
            skips_receiver: false,
        });
        let mut values = Vec::with_capacity(function.parameters.len());
        let mut types = Vec::with_capacity(function.parameters.len());
        for (index, &parameter) in function.parameters.iter().enumerate() {
            let Some(argument) =
                args.argument_for_parameter(index, Some(parameter_names.as_slice()))
            else {
                return report_unsupported(self.gcx, expr.span, "linked library argument");
            };
            let parameter_ty = self.gcx.type_of_item(parameter.into());
            let mut value = if Self::is_storage_parameter(parameter_ty) {
                self.storage_access(argument)?.slot
            } else {
                self.lower_expr(argument)?
            };
            let mut abi_type = if Self::is_storage_parameter(parameter_ty) {
                AbiType::Word
            } else {
                self.types.abi_type(parameter_ty)?
            };
            abi_type = self.abi_type_for_value(value, abi_type);
            if self.needs_calldata_materialization(value, &abi_type) {
                value = self.materialize_calldata_argument(parameter_ty, value, argument.span)?;
                abi_type = Self::memory_abi_type(abi_type);
            }
            values.push(value);
            types.push(abi_type);
        }

        let selector = self.gcx.function_selector(function_id).0;
        let selector = self.builder.imm_u256(U256::from_be_slice(&selector) << 224);
        let layout = Arc::new(AbiLayout::new(types.into_boxed_slice()));
        let encoded = self.builder.abi_encode(layout, Some(selector), values.into_boxed_slice());
        let input = self.builder.slice_ptr(encoded);
        let input_size = self.builder.slice_len(encoded);
        let zero = self.builder.imm_u256(U256::ZERO);
        let address = self.builder.imm_u256(address);
        let gas = self.builder.gas();
        let success = self.builder.delegatecall(gas, address, input, input_size, zero, zero);
        self.revert_external_call(success);
        if function.returns.is_empty() {
            return Some(zero);
        }
        if !self.gcx.sess.opts.evm_version.supports_returndata() {
            return report_error(
                self.gcx,
                expr.span,
                "codegen cannot decode linked library returndata before Byzantium",
            );
        }
        let data = self.materialize_returndata_bytes();
        let return_types = function
            .returns
            .iter()
            .map(|&ret| {
                self.gcx.type_of_item(ret.into()).with_loc_if_ref(self.gcx, DataLocation::Memory)
            })
            .collect::<Vec<_>>();
        let values = self.lower_abi_decode_values(data, &return_types, expr.span)?;
        if values.len() > 1 {
            let base = self.ensure_multi_return_buffer(values.len());
            for (index, value) in values.iter().copied().enumerate().skip(1) {
                let offset = self.builder.imm_u64((index as u64).saturating_mul(32));
                let address = self.builder.add(base, offset);
                self.builder.mstore(address, value);
            }
        }
        values.into_iter().next().or(Some(zero))
    }

    fn is_storage_parameter(ty: Ty<'gcx>) -> bool {
        ty.is_ref_at(DataLocation::Storage) || matches!(ty.peel_refs().kind, TyKind::Mapping(..))
    }

    fn needs_calldata_materialization(&self, value: ValueId, ty: &AbiType) -> bool {
        if !matches!(
            self.builder.func().value_ty(value),
            Some(MirType::Slice(SliceLocation::Calldata))
        ) {
            return false;
        }
        match ty {
            AbiType::Bytes(SliceLocation::Memory)
            | AbiType::DynamicArray { location: SliceLocation::Memory, .. } => true,
            AbiType::DynamicArray { element, location: SliceLocation::Calldata } => {
                !matches!(element.as_ref(), AbiType::Word)
            }
            _ => false,
        }
    }

    fn materialize_memory_argument(
        &mut self,
        ty: Ty<'gcx>,
        value: ValueId,
        span: Span,
    ) -> Option<ValueId> {
        let Some(MirType::Slice(location)) = self.builder.func().value_ty(value) else {
            return Some(value);
        };
        match location {
            SliceLocation::Calldata => self.materialize_calldata_argument(ty, value, span),
            SliceLocation::Memory => match ty.peel_refs().kind {
                TyKind::Elementary(
                    solar_sema::hir::ElementaryType::Bytes
                    | solar_sema::hir::ElementaryType::String,
                ) => Some(self.materialize_memory_slice(value)),
                _ => report_unsupported(self.gcx, span, "memory slice materialization"),
            },
            SliceLocation::Returndata => {
                report_unsupported(self.gcx, span, "returndata slice materialization")
            }
        }
    }

    fn materialize_call_argument(
        &mut self,
        ty: Ty<'gcx>,
        value: ValueId,
        span: Span,
    ) -> Option<ValueId> {
        if ty.is_ref_at(DataLocation::Calldata)
            && matches!(
                self.builder.func().value_ty(value),
                Some(MirType::Slice(SliceLocation::Calldata))
            )
        {
            Some(value)
        } else {
            self.materialize_memory_argument(ty, value, span)
        }
    }

    fn memory_abi_type(ty: AbiType) -> AbiType {
        match ty {
            AbiType::Word => AbiType::Word,
            AbiType::Bytes(_) => AbiType::Bytes(SliceLocation::Memory),
            AbiType::DynamicArray { element, .. } => AbiType::DynamicArray {
                element: Box::new(Self::memory_abi_type(*element)),
                location: SliceLocation::Memory,
            },
            AbiType::FixedArray { element, len } => {
                AbiType::FixedArray { element: Box::new(Self::memory_abi_type(*element)), len }
            }
            AbiType::Tuple(fields) => {
                AbiType::Tuple(fields.into_vec().into_iter().map(Self::memory_abi_type).collect())
            }
        }
    }

    fn abi_type_for_value(&self, value: ValueId, ty: AbiType) -> AbiType {
        if matches!(
            self.builder.func().value_ty(value),
            Some(MirType::MemoryObject(_)) | Some(MirType::Slice(SliceLocation::Memory))
        ) || matches!(self.builder.func().value(value), Value::Inst(inst) if matches!(
            &self.builder.func().inst(*inst).kind,
            InstKind::MemoryObjectLoadField { .. } | InstKind::MemoryObjectLoadElement { .. }
        )) {
            Self::memory_abi_type(ty)
        } else {
            ty
        }
    }

    fn materialize_calldata_argument(
        &mut self,
        ty: Ty<'gcx>,
        value: ValueId,
        span: Span,
    ) -> Option<ValueId> {
        match ty.peel_refs().kind {
            TyKind::Elementary(
                solar_sema::hir::ElementaryType::Bytes | solar_sema::hir::ElementaryType::String,
            ) => Some(self.materialize_memory_slice(value)),
            TyKind::DynArray(element) | TyKind::Slice(element) => {
                let element_type = self.types.abi_type(element)?;
                let length = self.builder.slice_len(value);
                let data = self.builder.slice_ptr(value);
                if matches!(element_type, AbiType::Word) {
                    return Some(self.copy_calldata_word_array(data, length));
                }
                self.materialize_calldata_nested_array(element, data, length, span)
            }
            _ => report_unsupported(self.gcx, span, "calldata argument materialization"),
        }
    }

    fn copy_calldata_word_array(&mut self, data: ValueId, length: ValueId) -> ValueId {
        let word = self.builder.imm_u64(32);
        let byte_length = self.checked_mul(length, word);
        let size = self.checked_add(word, byte_length);
        let object = self.builder.alloc_object(
            size,
            MemoryObjectLayout::WORD_ARRAY,
            AllocationSemantics::INTERNAL,
        );
        self.builder.set_memory_object_len(object, length, MemoryObjectKind::DynamicArray);
        let source = self.builder.make_slice(data, byte_length, SliceLocation::Calldata);
        self.builder.memory_object_copy_from_slice(object, MemoryObjectKind::DynamicArray, source);
        object
    }

    fn materialize_calldata_nested_array(
        &mut self,
        element: Ty<'gcx>,
        data: ValueId,
        length: ValueId,
        span: Span,
    ) -> Option<ValueId> {
        let word = self.builder.imm_u64(32);
        let element_head_size = self.builder.imm_u64(self.types.abi_type(element)?.head_size());
        let payload_size = self.checked_mul(length, word);
        let size = self.checked_add(word, payload_size);
        let object = self.builder.alloc_object(
            size,
            MemoryObjectLayout::WORD_ARRAY,
            AllocationSemantics::INTERNAL,
        );
        self.builder.set_memory_object_len(object, length, MemoryObjectKind::DynamicArray);

        let preheader = self.builder.current_block();
        let header = self.builder.create_block();
        let body = self.builder.create_block();
        let exit = self.builder.create_block();
        self.builder.jump(header);

        self.builder.switch_to_block(header);
        let zero = self.builder.imm_u64(0);
        let index = self.builder.phi(vec![(preheader, zero)]);
        let more = self.builder.lt(index, length);
        self.builder.branch(more, body, exit);

        self.builder.switch_to_block(body);
        let offset = self.checked_mul(index, element_head_size);
        let head = self.builder.add(data, offset);
        let value = self.materialize_calldata_value_at(element, head, data, span)?;
        self.builder.memory_object_store_element(
            object,
            MemoryObjectLayout::WORD_ARRAY,
            index,
            value,
        );
        let one = self.builder.imm_u64(1);
        let next = self.builder.add(index, one);
        let backedge = self.builder.current_block();
        self.builder.jump(header);
        self.builder.add_phi_incoming(index, backedge, next);

        self.builder.switch_to_block(exit);
        Some(object)
    }

    fn materialize_calldata_value_at(
        &mut self,
        ty: Ty<'gcx>,
        head: ValueId,
        tuple_base: ValueId,
        span: Span,
    ) -> Option<ValueId> {
        let ty = ty.peel_refs();
        if let TyKind::Udvt(inner, _) = ty.kind {
            return self.materialize_calldata_value_at(inner, head, tuple_base, span);
        }
        let value_pos = if self.types.abi_type(ty)?.is_dynamic() {
            let offset = self.builder.calldataload(head);
            self.builder.add(tuple_base, offset)
        } else {
            head
        };
        match ty.kind {
            TyKind::Elementary(
                solar_sema::hir::ElementaryType::Bytes | solar_sema::hir::ElementaryType::String,
            ) => Some(self.materialize_calldata_bytes_at(value_pos)),
            TyKind::DynArray(element) | TyKind::Slice(element) => {
                let length = self.builder.calldataload(value_pos);
                let word = self.builder.imm_u64(32);
                let data = self.builder.add(value_pos, word);
                let element_type = self.types.abi_type(element)?;
                if matches!(element_type, AbiType::Word) {
                    Some(self.copy_calldata_word_array(data, length))
                } else {
                    self.materialize_calldata_nested_array(element, data, length, span)
                }
            }
            TyKind::Array(element, length) => {
                let length = u64::try_from(length).ok()?;
                self.materialize_calldata_fixed_array(element, length, value_pos, span)
            }
            TyKind::Struct(id) => {
                let fields = self.gcx.hir.strukt(id).fields.to_vec();
                let field_types = fields
                    .iter()
                    .map(|&field| self.gcx.type_of_item(field.into()))
                    .collect::<Vec<_>>();
                self.materialize_calldata_fields(field_types, value_pos, span)
            }
            TyKind::Tuple(fields) => {
                self.materialize_calldata_fields(fields.iter().copied(), value_pos, span)
            }
            _ => Some(self.builder.calldataload(value_pos)),
        }
    }

    fn materialize_calldata_bytes_at(&mut self, position: ValueId) -> ValueId {
        let length = self.builder.calldataload(position);
        let word = self.builder.imm_u64(32);
        let data = self.builder.add(position, word);
        let slice = self.builder.make_slice(data, length, SliceLocation::Calldata);
        self.materialize_memory_slice(slice)
    }

    fn materialize_calldata_fixed_array(
        &mut self,
        element: Ty<'gcx>,
        length: u64,
        base: ValueId,
        span: Span,
    ) -> Option<ValueId> {
        let word = self.builder.imm_u64(32);
        let length_value = self.builder.imm_u64(length);
        let size = self.checked_mul(length_value, word);
        let layout = MemoryObjectLayout::FixedArray { len: length, element_words: 1 };
        let object = self.builder.alloc_object(size, layout, AllocationSemantics::INTERNAL);
        let element_head_size = self.types.abi_type(element)?.head_size();
        for index in 0..length {
            let index_value = self.builder.imm_u64(index);
            let element_head_size_value = self.builder.imm_u64(element_head_size);
            let head_offset = self.checked_mul(index_value, element_head_size_value);
            let head = self.builder.add(base, head_offset);
            let value = self.materialize_calldata_value_at(element, head, base, span)?;
            self.builder.memory_object_store_element(object, layout, index_value, value);
        }
        Some(object)
    }

    fn materialize_calldata_fields(
        &mut self,
        fields: impl IntoIterator<Item = Ty<'gcx>>,
        base: ValueId,
        span: Span,
    ) -> Option<ValueId> {
        let fields = fields.into_iter().collect::<Vec<_>>();
        let layout = MemoryObjectLayout::Struct { fields: fields.len() as u64 };
        let size = self.builder.imm_u64(fields.len().checked_mul(32)? as u64);
        let object = self.builder.alloc_object(size, layout, AllocationSemantics::INTERNAL);
        let mut offset = 0u64;
        for (index, field) in fields.iter().copied().enumerate() {
            let field_offset = self.builder.imm_u64(offset);
            let head = self.builder.add(base, field_offset);
            let value = self.materialize_calldata_value_at(field, head, base, span)?;
            self.builder.memory_object_store_field(object, layout, index as u64, value);
            offset = offset.checked_add(self.types.abi_type(field)?.head_size())?;
        }
        Some(object)
    }

    fn resolve_call_target(
        &self,
        callee: &hir::Expr<'_>,
        function: hir::FunctionId,
    ) -> hir::FunctionId {
        if let ExprKind::Member(base, _) = callee.kind
            && let Some(TyKind::Type(ty)) = self.gcx.type_of_expr(base.id).map(|ty| ty.kind)
        {
            return match ty.kind {
                TyKind::Contract(_) => function,
                TyKind::Super(defining_contract) => {
                    self.gcx.resolve_super_function(self.contract_id, defining_contract, function)
                }
                _ => self.gcx.resolve_virtual_function(self.contract_id, function),
            };
        }
        self.gcx.resolve_virtual_function(self.contract_id, function)
    }

    fn lower_tuple(
        &mut self,
        expr: &hir::Expr<'_>,
        values: &[Option<&hir::Expr<'_>>],
    ) -> Option<ValueId> {
        let ty = self.gcx.type_of_expr(expr.id)?;
        let MemoryObjectLayout::Struct { fields } = self.types.memory_layout(ty)? else {
            return report_unsupported(self.gcx, expr.span, "tuple object");
        };
        let size = fields.checked_mul(32)?;
        let size = self.builder.imm_u64(size);
        let object = self.builder.alloc_object(
            size,
            MemoryObjectLayout::Struct { fields },
            AllocationSemantics::SOLIDITY_ZEROED,
        );
        for (index, value) in values.iter().enumerate() {
            let Some(value) = value else { continue };
            let value = self.lower_expr(value)?;
            self.builder.memory_object_store_field(
                object,
                MemoryObjectLayout::Struct { fields },
                index as u64,
                value,
            );
        }
        Some(object)
    }

    fn lower_bytes_literal(&mut self, bytes: &[u8], span: Span) -> Option<ValueId> {
        let words = u64::try_from(bytes.len().div_ceil(32)).ok()?;
        let size = words.checked_add(1)?.checked_mul(32)?;
        let size = self.builder.imm_u64(size);
        let object = self.builder.alloc_object(
            size,
            MemoryObjectLayout::Bytes,
            AllocationSemantics::SOLIDITY_ZEROED,
        );
        let kind = MemoryObjectKind::Bytes;
        let length = self.builder.imm_u64(u64::try_from(bytes.len()).ok()?);
        self.builder.set_memory_object_len(object, length, kind);
        for (index, chunk) in bytes.chunks(32).enumerate() {
            let mut word = U256::from_be_slice(chunk);
            word <<= (32 - chunk.len()) * 8;
            let value = self.builder.imm_u256(word);
            let offset = self.builder.imm_u64(index as u64 * 32);
            self.builder.memory_object_store_word(object, offset, value);
        }
        let _ = span;
        Some(object)
    }

    fn default_value(&mut self, ty: solar_sema::ty::Ty<'gcx>) -> ValueId {
        self.default_object(ty).unwrap_or_else(|| self.builder.imm_u256(U256::ZERO))
    }

    fn default_binding_value(&mut self, ty: solar_sema::ty::Ty<'gcx>) -> ValueId {
        if ty.is_ref_at(DataLocation::Calldata)
            && matches!(
                ty.peel_refs().kind,
                TyKind::DynArray(_)
                    | TyKind::Slice(_)
                    | TyKind::Elementary(
                        solar_sema::hir::ElementaryType::Bytes
                            | solar_sema::hir::ElementaryType::String,
                    )
            )
        {
            let zero = self.builder.imm_u256(U256::ZERO);
            return self.builder.make_slice(zero, zero, SliceLocation::Calldata);
        }
        self.default_value(ty)
    }

    fn default_object(&mut self, ty: solar_sema::ty::Ty<'gcx>) -> Option<ValueId> {
        let layout = self.types.memory_layout(ty)?;
        let size = match layout {
            MemoryObjectLayout::Bytes | MemoryObjectLayout::DynamicArray { .. } => 32,
            MemoryObjectLayout::FixedArray { len, element_words } => {
                len.checked_mul(u64::from(element_words))?.checked_mul(32)?
            }
            MemoryObjectLayout::Struct { fields } => fields.checked_mul(32)?,
        };
        let size = self.builder.imm_u64(size);
        let object = self.builder.alloc_object(size, layout, AllocationSemantics::SOLIDITY_ZEROED);
        match ty.peel_refs().kind {
            solar_sema::ty::TyKind::Elementary(
                solar_sema::hir::ElementaryType::Bytes | solar_sema::hir::ElementaryType::String,
            )
            | solar_sema::ty::TyKind::DynArray(_) => {
                let zero = self.builder.imm_u256(U256::ZERO);
                self.builder.set_memory_object_len(object, zero, layout.kind());
            }
            solar_sema::ty::TyKind::Struct(id) => {
                for (index, &field) in self.gcx.hir.strukt(id).fields.iter().enumerate() {
                    let field_ty = self.gcx.type_of_item(field.into());
                    if let Some(value) = self.default_object(field_ty) {
                        self.builder.memory_object_store_field(object, layout, index as u64, value);
                    }
                }
            }
            solar_sema::ty::TyKind::Array(element, len) => {
                let Ok(len) = u64::try_from(len) else { return Some(object) };
                if self.types.memory_layout(element).is_some() {
                    for index in 0..len {
                        let Some(value) = self.default_object(element) else { continue };
                        let index = self.builder.imm_u64(index);
                        self.builder.memory_object_store_element(object, layout, index, value);
                    }
                }
            }
            _ => {}
        }
        Some(object)
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
                let access = self.storage_access(receiver)?;
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
        let object = self.lower_expr(receiver)?;
        let receiver_ty = self.type_of_expr_or_variable(receiver)?;
        let layout = self.types.memory_layout(receiver_ty)?;
        Some(self.builder.memory_object_load_field(object, layout, field as u64))
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
        if builtin == Builtin::FunctionSelector {
            let selector = match self.gcx.resolved_expr(expr).and_then(|res| match res {
                hir::Res::Item(item @ (hir::ItemId::Function(_) | hir::ItemId::Error(_))) => {
                    Some(self.gcx.function_selector(item).0)
                }
                _ => None,
            }) {
                Some(selector) => selector,
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
                let access = self.storage_access(receiver)?;
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
                let word = self.builder.calldataload(offset);
                let shift = self.builder.imm_u64(224);
                self.builder.shr(shift, word)
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

    fn lower_index(
        &mut self,
        expr: &hir::Expr<'_>,
        receiver: &hir::Expr<'_>,
        index: Option<&hir::Expr<'_>>,
    ) -> Option<ValueId> {
        if let Some(access) = self.storage_access(expr) {
            return self.load_storage_access(expr, access);
        }
        let Some(index) = index else {
            return report_unsupported(self.gcx, expr.span, "index");
        };
        let object = self.lower_expr(receiver)?;
        let index = self.lower_expr(index)?;
        let receiver_ty = self.gcx.type_of_expr(receiver.id)?;
        if let Some(MirType::Slice(location)) = self.builder.func().value_ty(object) {
            let length = self.builder.slice_len(object);
            self.bounds_check(index, length);
            let base = self.builder.slice_ptr(object);
            return match receiver_ty.peel_refs().kind {
                TyKind::Elementary(
                    solar_sema::hir::ElementaryType::Bytes
                    | solar_sema::hir::ElementaryType::String,
                ) => {
                    let pointer = self.builder.add(base, index);
                    let word = match location {
                        SliceLocation::Calldata => self.builder.calldataload(pointer),
                        SliceLocation::Memory => self.builder.mload(pointer),
                        SliceLocation::Returndata => {
                            return report_unsupported(self.gcx, expr.span, "returndata index");
                        }
                    };
                    let zero = self.builder.imm_u64(0);
                    let byte = self.builder.byte(zero, word);
                    Some(self.normalize_byte_value(expr, byte))
                }
                TyKind::DynArray(element) | TyKind::Slice(element) => {
                    if location != SliceLocation::Calldata {
                        return report_unsupported(self.gcx, expr.span, "memory array slice index");
                    }
                    let head_size = self.types.abi_type(element)?.head_size();
                    let head_size = self.builder.imm_u64(head_size);
                    let offset = self.checked_mul(index, head_size);
                    let head = self.builder.add(base, offset);
                    self.materialize_calldata_value_at(element, head, base, expr.span)
                }
                _ => report_unsupported(self.gcx, expr.span, "slice index"),
            };
        }
        let layout = self.types.memory_layout(receiver_ty)?;
        match layout {
            MemoryObjectLayout::DynamicArray { .. } => {
                let length = self.builder.memory_object_len(object, layout.kind());
                self.bounds_check(index, length);
                let value = self.builder.memory_object_load_element(object, layout, index);
                if let TyKind::DynArray(element) = receiver_ty.peel_refs().kind
                    && self.types.memory_layout(element).is_some()
                {
                    return self.materialize_array_element(object, layout, index, element, value);
                }
                Some(value)
            }
            MemoryObjectLayout::FixedArray { len, .. } => {
                let length = self.builder.imm_u64(len);
                self.bounds_check(index, length);
                let value = self.builder.memory_object_load_element(object, layout, index);
                if let TyKind::Array(element, _) = receiver_ty.peel_refs().kind
                    && self.types.memory_layout(element).is_some()
                {
                    return self.materialize_array_element(object, layout, index, element, value);
                }
                Some(value)
            }
            MemoryObjectLayout::Bytes => {
                let length = self.builder.memory_object_len(object, layout.kind());
                self.bounds_check(index, length);
                let value = self.builder.memory_object_load_byte(object, index);
                Some(self.normalize_byte_value(expr, value))
            }
            MemoryObjectLayout::Struct { .. } => {
                report_unsupported(self.gcx, expr.span, "struct index")
            }
        }
    }

    fn lower_slice(
        &mut self,
        expr: &hir::Expr<'_>,
        receiver: &hir::Expr<'_>,
        start: Option<&hir::Expr<'_>>,
        end: Option<&hir::Expr<'_>>,
    ) -> Option<ValueId> {
        let receiver_ty = self.gcx.type_of_expr(receiver.id)?;
        let value = self.lower_expr(receiver)?;
        let (source, location) = match self.builder.func().value_ty(value) {
            Some(MirType::Slice(location)) => (value, location),
            _ => {
                let layout = self.types.memory_layout(receiver_ty)?;
                if layout != MemoryObjectLayout::Bytes {
                    return report_unsupported(self.gcx, expr.span, "slice");
                }
                let length = self.builder.memory_object_len(value, MemoryObjectKind::Bytes);
                let pointer = self.builder.memory_object_data(value, MemoryObjectKind::Bytes);
                (
                    self.builder.make_slice(pointer, length, SliceLocation::Memory),
                    SliceLocation::Memory,
                )
            }
        };
        let is_bytes = self.is_calldata_dynamic_bytes_type(receiver_ty)
            || matches!(
                receiver_ty.peel_refs().kind,
                TyKind::Elementary(
                    solar_sema::hir::ElementaryType::Bytes
                        | solar_sema::hir::ElementaryType::String,
                )
            );
        let element_stride = if is_bytes {
            1
        } else {
            if !matches!(receiver_ty.peel_refs().kind, TyKind::DynArray(_) | TyKind::Slice(_)) {
                return report_unsupported(self.gcx, expr.span, "slice");
            }
            if location != SliceLocation::Calldata {
                return report_unsupported(self.gcx, expr.span, "slice");
            }
            let element = receiver_ty.base_type(self.gcx)?;
            let element_type = self.types.abi_type(element)?;
            if element_type.is_dynamic() {
                return report_unsupported(self.gcx, expr.span, "slice");
            }
            element_type.head_size()
        };
        let base_ptr = self.builder.slice_ptr(source);
        let base_len = self.builder.slice_len(source);
        let start =
            if let Some(start) = start { self.lower_expr(start)? } else { self.builder.imm_u64(0) };
        let end = if let Some(end) = end {
            let end = self.lower_expr(end)?;
            let past_end = self.builder.gt(end, base_len);
            self.panic_if(past_end, 0x32);
            end
        } else {
            base_len
        };
        let backwards = self.builder.lt(end, start);
        self.panic_if(backwards, 0x32);
        let length = self.builder.sub(end, start);
        let start_offset = if element_stride == 1 {
            start
        } else {
            let stride = self.builder.imm_u64(element_stride);
            self.checked_mul(start, stride)
        };
        let pointer = self.builder.add(base_ptr, start_offset);
        Some(self.builder.make_slice(pointer, length, location))
    }

    fn load_variable(&mut self, id: VariableId, span: Span) -> Option<ValueId> {
        if let Some(value) = self.values.get(&id).copied() {
            return Some(value);
        }
        if let Some(&immutable_id) = self.immutable_ids.get(&id) {
            let ty = self.gcx.type_of_item(id.into());
            return Some(
                self.builder.load_immutable(immutable_id, types::TypeLowerer::mir_type(ty)),
            );
        }
        if let Some(access) = self.storage_refs.get(&id).copied() {
            let ty = self.gcx.type_of_item(id.into());
            if ty.is_ref_at(DataLocation::Storage) {
                return Some(access.slot);
            }
            return Some(self.storage.load_at_slot(
                &mut self.builder,
                access.location,
                access.slot,
            ));
        }
        let var = self.gcx.hir.variable(id);
        if var.is_constant() {
            return self.lower_constant(var.initializer, span);
        }
        if let Some(location) = self.storage.get(id) {
            let ty = self.gcx.type_of_item(id.into());
            if self.types.memory_layout(ty).is_some() {
                let slot = self.builder.imm_u256(location.slot);
                return self.load_storage_object(ty, slot, span);
            }
            if matches!(ty.peel_refs().kind, solar_sema::ty::TyKind::Mapping(..)) {
                return report_unsupported(self.gcx, span, "mapping value");
            }
            return Some(self.storage.load(&mut self.builder, location));
        }
        report_unsupported(self.gcx, span, "identifier")
    }

    fn normalize_byte_value(&mut self, expr: &hir::Expr<'_>, value: ValueId) -> ValueId {
        let Some(ty) = self.gcx.type_of_expr(expr.id) else { return value };
        let solar_sema::ty::TyKind::Elementary(solar_sema::hir::ElementaryType::FixedBytes(size)) =
            ty.peel_refs().kind
        else {
            return value;
        };
        let shift = self.builder.imm_u64(u64::from(32 - size.bytes()) * 8);
        self.builder.shl(shift, value)
    }

    fn load_lvalue(&mut self, expr: &hir::Expr<'_>) -> Option<ValueId> {
        if let Some(id) = self.gcx.resolved_variable(expr) {
            let variable = self.gcx.hir.variable(id);
            if variable.is_state_variable()
                || self.values.contains_key(&id)
                || self.storage_refs.contains_key(&id)
                || variable.parent.is_none()
            {
                return self.load_variable(id, expr.span);
            }
        }
        match &expr.kind {
            ExprKind::Member(receiver, name) => self.lower_member(expr, receiver, *name),
            ExprKind::Index(receiver, index) => self.lower_index(expr, receiver, *index),
            _ => report_unsupported(self.gcx, expr.span, "l-value"),
        }
    }

    fn store_variable(&mut self, id: VariableId, value: ValueId, span: Span) -> Option<()> {
        if let StdEntry::Occupied(mut entry) = self.values.entry(id) {
            entry.insert(value);
            return Some(());
        }
        if let Some(&immutable_id) = self.immutable_ids.get(&id) {
            self.builder.store_immutable(immutable_id, value);
            return Some(());
        }
        if let Some(location) = self.storage.get(id) {
            self.storage.store(&mut self.builder, location, value);
            return Some(());
        }
        report_unsupported(self.gcx, span, "assignment target")
    }

    fn store_state_variable(&mut self, id: VariableId, value: ValueId, span: Span) -> Option<()> {
        let ty = self.gcx.type_of_item(id.into());
        let Some(location) = self.storage.get(id) else {
            return report_unsupported(self.gcx, span, "state initializer target");
        };
        if self.types.memory_layout(ty).is_some() {
            let slot = self.builder.imm_u256(location.slot);
            self.store_storage_object(ty, slot, value, span)
        } else {
            self.storage.store(&mut self.builder, location, value);
            Some(())
        }
    }

    fn store_lvalue(&mut self, expr: &hir::Expr<'_>, value: ValueId) -> Option<()> {
        if let Some(access) = self.storage_access(expr) {
            return self.store_storage_access(expr, access, value);
        }
        if let Some(id) = self.gcx.resolved_variable(expr)
            && self.gcx.hir.variable(id).is_state_variable()
        {
            let ty = self.gcx.type_of_item(id.into());
            if self.types.memory_layout(ty).is_some() {
                let Some(location) = self.storage.get(id) else {
                    return report_unsupported(self.gcx, expr.span, "storage assignment target");
                };
                let slot = self.builder.imm_u256(location.slot);
                return self.store_storage_object(ty, slot, value, expr.span);
            }
        }
        if let Some(id) = self.gcx.resolved_variable(expr) {
            let variable = self.gcx.hir.variable(id);
            if variable.is_state_variable()
                || self.values.contains_key(&id)
                || variable.parent.is_none()
            {
                return self.store_variable(id, value, expr.span);
            }
        }
        match &expr.kind {
            ExprKind::Member(receiver, name)
                if self.gcx.resolved_builtin(expr) == Some(Builtin::ArrayLength)
                    || (name.name == sym::offset
                        && self
                            .type_of_expr_or_variable(receiver)
                            .is_some_and(|ty| ty.is_ref_at(DataLocation::Calldata))) =>
            {
                self.store_yul_member(receiver, *name, value, expr.span)
            }
            ExprKind::Member(receiver, name) => {
                let id = self.gcx.resolved_variable(expr)?;
                let variable = self.gcx.hir.variable(id);
                let Some(hir::ItemId::Struct(struct_id)) = variable.parent else {
                    return report_unsupported(self.gcx, expr.span, "member assignment");
                };
                let Some(field) =
                    self.gcx.hir.strukt(struct_id).fields.iter().position(|&field| field == id)
                else {
                    return report_unsupported(self.gcx, name.span, "struct field assignment");
                };
                let object = self.lower_expr(receiver)?;
                let receiver_ty = self.gcx.type_of_expr(receiver.id)?;
                let layout = self.types.memory_layout(receiver_ty)?;
                self.builder.memory_object_store_field(object, layout, field as u64, value);
                Some(())
            }
            ExprKind::YulMember(receiver, name) => {
                self.store_yul_member(receiver, *name, value, expr.span)
            }
            ExprKind::Index(receiver, index) => {
                let Some(index) = index else {
                    return report_unsupported(self.gcx, expr.span, "index assignment");
                };
                let object = self.lower_expr(receiver)?;
                let index = self.lower_expr(index)?;
                let receiver_ty = self.gcx.type_of_expr(receiver.id)?;
                let layout = self.types.memory_layout(receiver_ty)?;
                match layout {
                    MemoryObjectLayout::DynamicArray { .. } => {
                        let length = self.builder.memory_object_len(object, layout.kind());
                        self.bounds_check(index, length);
                        self.builder.memory_object_store_element(object, layout, index, value);
                        Some(())
                    }
                    MemoryObjectLayout::FixedArray { len, .. } => {
                        let length = self.builder.imm_u64(len);
                        self.bounds_check(index, length);
                        self.builder.memory_object_store_element(object, layout, index, value);
                        Some(())
                    }
                    MemoryObjectLayout::Bytes => {
                        let length = self.builder.memory_object_len(object, layout.kind());
                        self.bounds_check(index, length);
                        let zero = self.builder.imm_u256(U256::ZERO);
                        let value = self.builder.byte(zero, value);
                        self.builder.memory_object_store_byte(object, index, value);
                        Some(())
                    }
                    _ => report_unsupported(self.gcx, expr.span, "index assignment"),
                }
            }
            _ => report_unsupported(self.gcx, expr.span, "assignment target"),
        }
    }

    fn store_yul_member(
        &mut self,
        receiver: &hir::Expr<'_>,
        name: solar_interface::Ident,
        value: ValueId,
        span: Span,
    ) -> Option<()> {
        let receiver_ty = self.type_of_expr_or_variable(receiver)?;
        if receiver_ty.is_ref_at(DataLocation::Calldata) {
            let base = self.lower_expr(receiver)?;
            let pointer = self.builder.slice_ptr(base);
            let length = self.builder.slice_len(base);
            let slice = match name.name {
                sym::offset => self.builder.make_slice(value, length, SliceLocation::Calldata),
                sym::length => self.builder.make_slice(pointer, value, SliceLocation::Calldata),
                _ => return report_unsupported(self.gcx, span, "Yul calldata assignment"),
            };
            return self.store_lvalue(receiver, slice);
        }

        if name.name != sym::slot {
            return report_unsupported(self.gcx, span, "Yul storage assignment");
        }
        let Some(id) = self.gcx.resolved_variable(receiver) else {
            return report_unsupported(self.gcx, span, "Yul storage assignment target");
        };
        if self.gcx.hir.variable(id).is_state_variable() {
            return report_unsupported(self.gcx, span, "Yul state-variable slot assignment");
        }
        let Some(access) = self.storage_refs.get(&id).copied() else {
            return report_unsupported(self.gcx, span, "Yul storage assignment target");
        };
        self.storage_refs.insert(id, StorageAccess { slot: value, ..access });
        Some(())
    }

    fn delete_lvalue(&mut self, expr: &hir::Expr<'_>) -> Option<()> {
        let ty = self.gcx.type_of_expr(expr.id)?;
        if ty.is_ref_at(DataLocation::Storage)
            && let Some(access) = self.storage_access(expr)
        {
            return self.clear_storage_access(ty, access);
        }
        let Some(layout) = self.types.memory_layout(ty) else {
            let zero = self.builder.imm_u256(U256::ZERO);
            return self.store_lvalue(expr, zero);
        };
        let object = self.load_lvalue(expr)?;
        let zero = self.builder.imm_u256(U256::ZERO);
        match layout {
            MemoryObjectLayout::Bytes | MemoryObjectLayout::DynamicArray { .. } => {
                self.builder.set_memory_object_len(object, zero, layout.kind());
            }
            MemoryObjectLayout::FixedArray { len, element_words: _ } => {
                for index in 0..len {
                    let index_value = self.builder.imm_u64(index);
                    let element_ty = match ty.peel_refs().kind {
                        solar_sema::ty::TyKind::Array(element, _) => element,
                        _ => return report_unsupported(self.gcx, expr.span, "array deletion"),
                    };
                    let value = self.default_value(element_ty);
                    self.builder.memory_object_store_element(object, layout, index_value, value);
                }
            }
            MemoryObjectLayout::Struct { fields: _ } => {
                let solar_sema::ty::TyKind::Struct(id) = ty.peel_refs().kind else {
                    return report_unsupported(self.gcx, expr.span, "struct deletion");
                };
                for (index, &field) in self.gcx.hir.strukt(id).fields.iter().enumerate() {
                    let field_ty = self.gcx.type_of_item(field.into());
                    let value = self.default_value(field_ty);
                    self.builder.memory_object_store_field(object, layout, index as u64, value);
                }
            }
        }
        Some(())
    }

    fn clear_storage_access(
        &mut self,
        ty: solar_sema::ty::Ty<'gcx>,
        access: StorageAccess,
    ) -> Option<()> {
        let zero = self.builder.imm_u256(U256::ZERO);
        match ty.peel_refs().kind {
            TyKind::Elementary(
                solar_sema::hir::ElementaryType::Bytes | solar_sema::hir::ElementaryType::String,
            )
            | TyKind::DynArray(_) => {
                self.builder.sstore(access.slot, zero);
            }
            TyKind::Struct(struct_id) => {
                for (index, &field) in self.gcx.hir.strukt(struct_id).fields.iter().enumerate() {
                    let field_ty = self.gcx.type_of_item(field.into());
                    let location = self.storage.field_location(self.gcx, struct_id, index)?;
                    let field_slot = self.add_storage_offset(access.slot, location.slot);
                    self.clear_storage_access(
                        field_ty,
                        StorageAccess { slot: field_slot, location, offset: None },
                    )?;
                }
            }
            TyKind::Array(element, len) => {
                let len = u64::try_from(len).ok()?;
                for index in 0..len {
                    let index = self.builder.imm_u64(index);
                    let element_access =
                        self.storage_array_element_access(access.slot, index, element, false)?;
                    self.clear_storage_access(element, element_access)?;
                }
            }
            _ => {
                self.storage.store_at_slot(&mut self.builder, access.location, access.slot, zero);
            }
        }
        Some(())
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
        then_block: BlockId,
        then_values: FxHashMap<VariableId, ValueId>,
        then_terminated: bool,
        else_block: BlockId,
        else_values: FxHashMap<VariableId, ValueId>,
        else_terminated: bool,
    ) -> FxHashMap<VariableId, ValueId> {
        let mut values = before;
        let mut ids = values.keys().copied().collect::<Vec<_>>();
        ids.extend(then_values.keys().copied());
        ids.extend(else_values.keys().copied());
        ids.sort_unstable();
        ids.dedup();
        for id in ids {
            let then_value = then_values.get(&id).copied();
            let else_value = else_values.get(&id).copied();
            let value = match (then_terminated, else_terminated, then_value, else_value) {
                (true, false, _, value) | (false, true, value, _) => value,
                (_, _, Some(lhs), Some(rhs)) if lhs == rhs => Some(lhs),
                (false, false, Some(lhs), Some(rhs)) => {
                    Some(self.builder.phi(vec![(then_block, lhs), (else_block, rhs)]))
                }
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
        function.attributes.no_inline = true;
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
                        let slot = builder.imm_u64(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT);
                        let base = builder.mload(slot);
                        for index in 1..shape.returns.len() {
                            let offset = builder.imm_u64(
                                u64::try_from(index).unwrap_or(u64::MAX).saturating_mul(32),
                            );
                            let address = builder.add(base, offset);
                            values.push(builder.mload(address));
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
