//! Function-level HIR to MIR lowering.

use super::{
    contract,
    storage::{StorageLayout, StorageLocation},
    types,
};
use crate::{
    memory::EvmMemoryLayout,
    mir::{
        AbiLayout, AbiParamLayout, AbiType, AllocationSemantics, BlockId, Function,
        FunctionBuilder, FunctionId, MemoryObjectKind, MemoryObjectLayout, MirType, Module,
        SliceLocation, ValueId,
    },
};
use alloy_primitives::{U256, keccak256};
use solar_ast::{BinOpKind, DataLocation, LitKind, UnOpKind};
use solar_data_structures::map::{FxHashMap, StdEntry};
use solar_interface::{Span, sym};
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
    function_ids: &FxHashMap<hir::FunctionId, FunctionId>,
) -> Option<Function> {
    let hir_function = gcx.hir.function(id);
    let mut mir = contract::declaration(gcx, id, hir_function);
    let mut type_lowerer = types::TypeLowerer::new(gcx);

    let input_shapes = hir_function
        .parameters
        .iter()
        .map(|&param| type_lowerer.abi_param_type(gcx.type_of_item(param.into())))
        .collect::<Option<Vec<_>>>();
    let output_shapes = hir_function
        .returns
        .iter()
        .map(|&ret| type_lowerer.abi_type(gcx.type_of_item(ret.into())))
        .collect::<Option<Vec<_>>>();

    if mir.selector.is_some() {
        let Some(input_shapes) = input_shapes else {
            return report_unsupported(gcx, hir_function.span, "function parameter shape");
        };
        let Some(output_shapes) = output_shapes else {
            return report_unsupported(gcx, hir_function.span, "function return shape");
        };
        mir.abi_params = Some(AbiParamLayout::new(input_shapes.into_boxed_slice()));
        mir.abi_returns =
            Some(module.intern_abi_layout(AbiLayout::new(output_shapes.into_boxed_slice())));
        mir.abi_args_lazy = true;
    }

    let mut lowerer = FunctionLowerer::new(gcx, storage, contract_id, function_ids, &mut mir);
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
        lowerer.finish(hir_function.returns);
    }
    Some(mir)
}

/// Lowers the synthetic constructor used when state initializers exist without
/// an explicit constructor body.
pub(super) fn lower_synthetic_constructor(
    gcx: Gcx<'_>,
    storage: &StorageLayout,
    contract_id: hir::ContractId,
    function_ids: &FxHashMap<hir::FunctionId, FunctionId>,
) -> Option<Function> {
    let mut mir =
        Function::new(solar_interface::Ident::with_dummy_span(solar_interface::kw::Constructor));
    mir.attributes.is_constructor = true;
    let mut lowerer = FunctionLowerer::new(gcx, storage, contract_id, function_ids, &mut mir);
    lowerer.lower_state_initializers(contract_id)?;
    lowerer.lower_implicit_base_constructors(contract_id)?;
    if !lowerer.is_terminated() {
        lowerer.finish(&[]);
    }
    Some(mir)
}

/// The mutable state for one function lowering.
///
/// Keeping the HIR context, variable environment, loop targets, and builder in
/// one object makes scope changes explicit. Child lowering methods do not need
/// to pass a growing collection of loosely related maps and flags.
struct FunctionLowerer<'gcx, 'mir, 'ids> {
    gcx: Gcx<'gcx>,
    storage: &'mir StorageLayout,
    contract_id: hir::ContractId,
    function_ids: &'ids FxHashMap<hir::FunctionId, FunctionId>,
    builder: FunctionBuilder<'mir>,
    types: types::TypeLowerer<'gcx>,
    values: FxHashMap<VariableId, ValueId>,
    storage_refs: FxHashMap<VariableId, StorageAccess>,
    returns: Vec<VariableId>,
    loops: Vec<LoopTargets>,
    modifiers: Vec<ModifierContext<'gcx>>,
    return_targets: Vec<BlockId>,
}

struct LoopTargets {
    break_block: BlockId,
    continue_block: BlockId,
    break_states: Vec<LoopState>,
    continue_states: Vec<LoopState>,
}

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

impl<'gcx, 'mir, 'ids> FunctionLowerer<'gcx, 'mir, 'ids> {
    fn new(
        gcx: Gcx<'gcx>,
        storage: &'mir StorageLayout,
        contract_id: hir::ContractId,
        function_ids: &'ids FxHashMap<hir::FunctionId, FunctionId>,
        function: &'mir mut Function,
    ) -> Self {
        Self {
            gcx,
            storage,
            contract_id,
            function_ids,
            builder: FunctionBuilder::new(function),
            types: types::TypeLowerer::new(gcx),
            values: FxHashMap::default(),
            storage_refs: FxHashMap::default(),
            returns: Vec::new(),
            loops: Vec::new(),
            modifiers: Vec::new(),
            return_targets: Vec::new(),
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
                .add_return(types::TypeLowerer::mir_type(self.gcx.type_of_item(ret.into())));
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
                if !variable.is_state_variable()
                    || variable.is_constant()
                    || variable.is_immutable()
                {
                    continue;
                }
                let Some(initializer) = variable.initializer else { continue };
                let value = self.lower_expr(initializer)?;
                self.store_state_variable(id, value, initializer.span)?;
            }
        }
        Some(())
    }

    fn lower_implicit_base_constructors(&mut self, contract_id: hir::ContractId) -> Option<()> {
        let contract = self.gcx.hir.contract(contract_id);
        for (index, &base_id) in contract.linearized_bases.iter().skip(1).enumerate() {
            let Some(constructor_id) = self.gcx.hir.contract(base_id).ctor else { continue };
            let constructor = self.gcx.hir.function(constructor_id);
            let Some(args) = contract
                .linearized_bases_args
                .get(index)
                .and_then(|modifier| modifier.map(|modifier| modifier.args))
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
            self.lower_base_constructor(&modifier, constructor_id, constructor)?;
        }
        Some(())
    }

    fn finish(&mut self, returns: &[VariableId]) {
        if returns.is_empty() {
            self.builder.stop();
        } else {
            self.builder.ret(returns.iter().filter_map(|id| self.values.get(id).copied()));
        }
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
                for (id, value) in ids.iter().flatten().zip(values) {
                    self.values.insert(*id, value);
                }
            }
            StmtKind::Expr(expr) => {
                self.lower_expr(expr)?;
            }
            StmtKind::Block(block) | StmtKind::UncheckedBlock(block) => self.lower_block(*block)?,
            StmtKind::If(cond, then_stmt, else_stmt) => {
                self.lower_if(cond, then_stmt, *else_stmt)?;
            }
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
                        for (&id, value) in self.returns.iter().zip(values) {
                            self.values.insert(id, value);
                        }
                    }
                    self.builder.jump(target);
                } else if !self.is_terminated() {
                    if values.is_empty() {
                        self.builder.stop();
                    } else {
                        self.builder.ret(values);
                    }
                }
            }
            StmtKind::Revert(_) => {
                let zero = self.builder.imm_u256(U256::ZERO);
                self.builder.revert(zero, zero);
            }
            StmtKind::AssemblyBlock(block) => self.lower_block(*block)?,
            StmtKind::Placeholder => {
                self.lower_modifier_placeholder(stmt.span)?;
            }
            StmtKind::Emit(_) | StmtKind::Switch(_) | StmtKind::Try(_) | StmtKind::Err(_) => {
                return report_unsupported(self.gcx, stmt.span, "statement");
            }
        }
        Some(())
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
        for (index, &parameter) in modifier_function.parameters.iter().enumerate() {
            let Some(argument) =
                modifier.args.argument_for_parameter(index, Some(parameter_names.as_slice()))
            else {
                return report_unsupported(self.gcx, modifier.span, "named modifier argument");
            };
            let value = self.lower_expr(argument)?;
            saved_parameters.push((parameter, self.values.insert(parameter, value)));
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
        result
    }

    fn lower_base_constructor(
        &mut self,
        modifier: &hir::Modifier<'_>,
        constructor_id: hir::FunctionId,
        constructor: &'gcx hir::Function<'gcx>,
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
            self.lower_implicit_base_constructors(contract_id)?;
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
        Some(())
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
            let access = match (then_terminated, else_terminated, then, else_) {
                (true, false, _, Some(value)) => Some(value),
                (false, true, Some(value), _) => Some(value),
                (false, false, Some(lhs), Some(rhs)) if lhs == rhs => Some(lhs),
                (false, false, Some(lhs), Some(rhs)) => {
                    let slot = self.builder.phi(vec![(then_exit, lhs.slot), (else_exit, rhs.slot)]);
                    let offset = match (lhs.offset, rhs.offset) {
                        (Some(lhs), Some(rhs)) if lhs != rhs => {
                            Some(self.builder.phi(vec![(then_exit, lhs), (else_exit, rhs)]))
                        }
                        (Some(offset), _) | (_, Some(offset)) => Some(offset),
                        _ => None,
                    };
                    Some(StorageAccess { slot, location: lhs.location, offset })
                }
                (_, _, Some(value), _) | (_, _, _, Some(value)) => Some(value),
                _ => None,
            };
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

    fn lower_loop(&mut self, block: hir::Block<'_>, _source: LoopSource) -> Option<()> {
        let preheader = self.builder.current_block();
        let header = self.builder.create_block();
        let exit = self.builder.create_block();
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
        self.values = header_values;
        let mut header_storage_refs = before_storage_refs.clone();
        for (&id, &access) in &before_storage_refs {
            let slot = self.builder.phi(vec![(preheader, access.slot)]);
            let offset = access.offset.map(|offset| self.builder.phi(vec![(preheader, offset)]));
            header_storage_refs.insert(id, StorageAccess { slot, offset, ..access });
        }
        self.storage_refs = header_storage_refs.clone();
        self.loops.push(LoopTargets {
            break_block: exit,
            continue_block: header,
            break_states: Vec::new(),
            continue_states: Vec::new(),
        });
        self.lower_block(block)?;
        let normal_state = (!self.is_terminated()).then(|| LoopState {
            block: self.builder.current_block(),
            values: self.values.clone(),
            storage_refs: self.storage_refs.clone(),
        });
        if normal_state.is_some() {
            self.builder.jump(header);
        }
        let loop_targets = self.loops.pop().expect("loop target exists");
        if let Some(state) = &normal_state {
            self.add_loop_phi_incoming(&header_phis, state);
            self.add_loop_storage_phi_incoming(&header_storage_refs, state);
        }
        for state in &loop_targets.continue_states {
            self.add_loop_phi_incoming(&header_phis, state);
            self.add_loop_storage_phi_incoming(&header_storage_refs, state);
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
            let incoming = exits
                .iter()
                .filter_map(|state| state.storage_refs.get(&id).copied())
                .collect::<Vec<_>>();
            let Some(first) = incoming.first().copied().or_else(|| before.get(&id).copied()) else {
                continue;
            };
            if incoming.iter().all(|access| *access == first) {
                before.insert(id, first);
                continue;
            }
            let slot = self.builder.phi(
                exits
                    .iter()
                    .filter_map(|state| {
                        state.storage_refs.get(&id).map(|access| (state.block, access.slot))
                    })
                    .collect(),
            );
            let offset = first.offset;
            before.insert(id, StorageAccess { slot, location: first.location, offset });
        }
        before
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
        let ExprKind::Call(callee, args, _) = &expr.kind else { return None };
        let builtin = self.gcx.resolved_builtin(callee)?;
        if !matches!(
            builtin,
            Builtin::AddressCall | Builtin::AddressStaticcall | Builtin::AddressDelegatecall
        ) {
            return None;
        }
        let ExprKind::Member(receiver, _) = callee.kind else { return None };
        let capture_returndata = count > 1 || first_is_omitted;
        let success =
            self.lower_address_call(callee.span, receiver, builtin, *args, capture_returndata)?;
        if count <= 1 && !first_is_omitted {
            return Some(vec![success]);
        }
        if count == 1 && first_is_omitted {
            return Some(vec![self.multi_return_buffer_base()]);
        }
        if count != 2 {
            return report_unsupported(self.gcx, expr.span, "low-level call return values");
        }
        let base = self.multi_return_buffer_base();
        Some(vec![success, base])
    }

    fn lower_values(&mut self, expr: &hir::Expr<'_>) -> Option<Vec<ValueId>> {
        if let ExprKind::Call(callee, ..) = &expr.kind {
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
            if let Some(function_id) = self.gcx.resolved_function(callee) {
                let returns = self.gcx.hir.function(function_id).returns.len();
                if returns > 1 {
                    let first = self.lower_expr(expr)?;
                    let base = self.multi_return_buffer_base();
                    let mut values = Vec::with_capacity(returns);
                    values.push(first);
                    for index in 1..returns {
                        values.push(self.load_multi_return_value(base, index));
                    }
                    return Some(values);
                }
            }
            let returns_empty = self
                .gcx
                .resolved_function(callee)
                .is_some_and(|id| self.gcx.hir.function(id).returns.is_empty())
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
            if rhs_elements.len() != elements.len() {
                return report_unsupported(self.gcx, rhs.span, "tuple assignment arity");
            }
            let mut values = Vec::with_capacity(rhs_elements.len());
            for value in rhs_elements.iter() {
                values.push(match value {
                    Some(value) => Some(self.lower_expr(value)?),
                    None => None,
                });
            }
            for (element, value) in elements.iter().zip(values) {
                let Some(value) = value else {
                    if element.is_some() {
                        return report_unsupported(self.gcx, rhs.span, "tuple assignment value");
                    }
                    continue;
                };
                let Some(element) = element else { continue };
                self.store_lvalue(element, value)?;
            }
            return Some(());
        }
        let values = self.lower_values(rhs)?;
        if values.len() != elements.iter().flatten().count() {
            return report_unsupported(self.gcx, rhs.span, "tuple assignment arity");
        }
        let mut values = values.into_iter();
        for element in elements.iter().flatten() {
            let value = values.next().expect("tuple assignment arity checked");
            self.store_lvalue(element, value)?;
        }
        Some(())
    }

    fn multi_return_buffer_base(&mut self) -> ValueId {
        let slot = self.builder.imm_u64(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT);
        self.builder.mload(slot)
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
                let id = self.gcx.resolved_variable(expr)?;
                self.load_variable(id, expr.span)
            }
            ExprKind::Binary(lhs, op, rhs) => {
                let lhs = self.lower_expr(lhs)?;
                let rhs = self.lower_expr(rhs)?;
                Some(self.binary(op.kind, lhs, rhs))
            }
            ExprKind::Call(callee, args, _) => self.lower_call(expr, callee, *args),
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
                    let new = self.binary(kind, old, one);
                    self.store_lvalue(value, new)?;
                    return Some(if matches!(op.kind, UnOpKind::PreInc | UnOpKind::PreDec) {
                        new
                    } else {
                        old
                    });
                }
                let value = self.lower_expr(value)?;
                self.unary(op.kind, value, expr.span)
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
                let value = if let Some(kind) = op.map(|op| op.kind) {
                    let lhs_value = self.load_lvalue(lhs)?;
                    self.binary(kind, lhs_value, rhs_value)
                } else {
                    self.materialize_memory_argument(
                        self.gcx.type_of_expr(lhs.id)?,
                        rhs_value,
                        rhs.span,
                    )?
                };
                let value = self.coerce_value(
                    value,
                    self.gcx.type_of_expr(rhs.id)?,
                    self.gcx.type_of_expr(lhs.id)?,
                );
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
    ) -> Option<ValueId> {
        let arguments = args.exprs().collect::<Vec<_>>();
        if let Some(struct_id) = self.gcx.resolved_expr(callee).and_then(|res| match res {
            hir::Res::Item(item) => item.as_struct(),
            _ => None,
        }) {
            return self.lower_struct_constructor(expr, struct_id, &arguments);
        }
        if matches!(callee.kind, ExprKind::TypeCall(_) | ExprKind::Type(_)) {
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
        if matches!(callee.kind, ExprKind::New(_)) {
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
                return self.lower_address_call(callee.span, receiver, builtin, args, false);
            }
            return self.lower_builtin_call(expr, callee, builtin, args);
        }
        if let Some(TyKind::Fn(function)) = self.gcx.type_of_expr(callee.id).map(|ty| ty.kind)
            && function.is_external()
            && function.function_id.is_none()
        {
            return self.lower_external_function_pointer_call(callee, function, args);
        }
        if let Some(function_id) = self.gcx.resolved_function(callee) {
            return self.lower_function_call(expr, callee, function_id, args);
        }
        if self.gcx.dcx().has_errors().is_err() {
            return Some(self.builder.imm_u256(U256::ZERO));
        }
        report_unsupported(self.gcx, expr.span, "function call")
    }

    fn lower_external_function_pointer_call(
        &mut self,
        callee: &hir::Expr<'_>,
        function: &solar_sema::ty::TyFn<'gcx>,
        args: hir::CallArgs<'_>,
    ) -> Option<ValueId> {
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
        let ret_offset = if returns > 1 { input } else { zero };
        let ret_size = self.builder.imm_u64((returns as u64).saturating_mul(32));
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
            return Some(zero);
        }
        if returns > 1 {
            let base = self.builder.imm_u64(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT);
            self.builder.mstore(base, ret_offset);
        }
        Some(self.builder.mload(ret_offset))
    }

    fn lower_struct_constructor(
        &mut self,
        expr: &hir::Expr<'_>,
        struct_id: hir::StructId,
        arguments: &[&hir::Expr<'_>],
    ) -> Option<ValueId> {
        let fields = self.gcx.hir.strukt(struct_id).fields.len() as u64;
        if arguments.len() != fields as usize {
            return report_unsupported(self.gcx, expr.span, "struct constructor arguments");
        }
        let layout = MemoryObjectLayout::Struct { fields };
        let size = self.builder.imm_u64(fields.saturating_mul(32));
        let object = self.builder.alloc_object(size, layout, AllocationSemantics::SOLIDITY_ZEROED);
        for (index, argument) in arguments.iter().enumerate() {
            let field = self.gcx.hir.strukt(struct_id).fields[index];
            let value = self.lower_expr(argument)?;
            let field_ty = self.gcx.type_of_item(field.into());
            let value = self.materialize_memory_argument(field_ty, value, argument.span)?;
            let value = self.coerce_value(value, self.gcx.type_of_expr(argument.id)?, field_ty);
            self.builder.memory_object_store_field(object, layout, index as u64, value);
        }
        Some(object)
    }

    fn coerce_value(&mut self, value: ValueId, from: Ty<'gcx>, to: Ty<'gcx>) -> ValueId {
        let TyKind::Elementary(solar_sema::hir::ElementaryType::FixedBytes(size)) =
            to.peel_refs().kind
        else {
            return value;
        };
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
        capture_returndata: bool,
    ) -> Option<ValueId> {
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
        let gas = self.builder.gas();
        let success = match builtin {
            Builtin::AddressCall => {
                self.builder.call(gas, address, zero, input, input_size, zero, zero)
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
        if capture_returndata {
            let length = self.builder.returndatasize();
            let thirty_one = self.builder.imm_u64(31);
            let rounded = self.checked_add(length, thirty_one);
            let word_size = self.builder.imm_u64(32);
            let words = self.builder.div(rounded, word_size);
            let one = self.builder.imm_u64(1);
            let total_words = self.checked_add(words, one);
            let size = self.checked_mul(total_words, word_size);
            let output = self.builder.alloc_object(
                size,
                MemoryObjectLayout::Bytes,
                AllocationSemantics::INTERNAL,
            );
            self.builder.set_memory_object_len(output, length, MemoryObjectKind::Bytes);
            let source = self.builder.make_slice(zero, length, SliceLocation::Returndata);
            self.builder.memory_object_copy_from_slice(output, MemoryObjectKind::Bytes, source);
            let base = self.builder.imm_u64(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT);
            self.builder.mstore(base, output);
        }
        Some(success)
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
                let (required, _message) = self.builtin_args_with_optional::<1>(builtin, &args)?;
                let condition = required.first()?;
                let condition = self.lower_expr(condition)?;
                let is_false = self.builder.iszero(condition);
                let revert_block = self.builder.create_block();
                let continue_block = self.builder.create_block();
                self.builder.branch(is_false, revert_block, continue_block);
                self.builder.switch_to_block(revert_block);
                let zero = self.builder.imm_u256(U256::ZERO);
                self.builder.revert(zero, zero);
                self.builder.switch_to_block(continue_block);
            }
            Builtin::Revert => {
                let _ = self.builtin_args::<0>(builtin, &args)?;
                let zero = self.builder.imm_u256(U256::ZERO);
                self.builder.revert(zero, zero);
            }
            Builtin::RevertMsg => {
                let _ = self.builtin_args::<1>(builtin, &args)?;
                return self.unsupported_builtin(builtin, args.span);
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
            let mut abi_type = self.types.abi_type(ty)?;
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
        let function_id = self.gcx.resolved_function(function)?;
        let selector = self.gcx.function_selector(function_id).0;
        let selector = self.builder.imm_u256(U256::from_be_slice(&selector) << 224);
        let exprs = match tuple.peel_parens().kind {
            ExprKind::Tuple(elements) => elements.iter().flatten().copied().collect::<Vec<_>>(),
            _ => vec![tuple],
        };
        let mut values = Vec::with_capacity(exprs.len());
        let mut types = Vec::with_capacity(exprs.len());
        for expr in exprs {
            let ty = self.gcx.type_of_expr(expr.id)?;
            let mut value = self.lower_expr(expr)?;
            let mut abi_type = self.types.abi_type(ty)?;
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
        let data = match self.builder.func().value_ty(data) {
            Some(MirType::Slice(_)) => self.materialize_memory_slice(data),
            _ => data,
        };
        let length = self.builder.memory_object_len(data, MemoryObjectKind::Bytes);
        let data_start = self.builder.memory_object_data(data, MemoryObjectKind::Bytes);
        let values =
            self.lower_abi_decode_region(data_start, length, &decoded_types, args[1].span)?;
        if values.len() > 1 {
            let base = self.multi_return_buffer_base();
            for (index, value) in values.iter().copied().enumerate().skip(1) {
                let offset = self.builder.imm_u64((index as u64).saturating_mul(32));
                let address = self.builder.add(base, offset);
                self.builder.mstore(address, value);
            }
        }
        values.into_iter().next()
    }

    fn abi_decode_is_dynamic(&mut self, ty: Ty<'gcx>) -> Option<bool> {
        Some(self.types.abi_type(ty)?.is_dynamic())
    }

    fn abi_decode_head_size(&mut self, ty: Ty<'gcx>) -> Option<u64> {
        self.types.abi_type(ty)?.checked_head_size()
    }

    fn lower_abi_decode_region(
        &mut self,
        base: ValueId,
        length: ValueId,
        types: &[Ty<'gcx>],
        span: Span,
    ) -> Option<Vec<ValueId>> {
        let head_size = types
            .iter()
            .try_fold(0u64, |size, &ty| size.checked_add(self.abi_decode_head_size(ty)?))?;
        let required = self.builder.imm_u64(head_size);
        let short = self.builder.lt(length, required);
        self.revert_if_empty(short);

        let mut values = Vec::with_capacity(types.len());
        let mut head_offset = 0u64;
        for &ty in types {
            let offset = self.builder.imm_u64(head_offset);
            let head = self.builder.add(base, offset);
            let value = if self.abi_decode_is_dynamic(ty)? {
                let relative = self.builder.mload(head);
                self.lower_abi_decode_dynamic(base, length, required, relative, ty, span)?
            } else {
                self.lower_abi_decode_static(head, ty, span)?
            };
            values.push(value);
            head_offset = head_offset.checked_add(self.abi_decode_head_size(ty)?)?;
        }
        Some(values)
    }

    fn lower_abi_decode_static(
        &mut self,
        address: ValueId,
        ty: Ty<'gcx>,
        span: Span,
    ) -> Option<ValueId> {
        let ty = ty.peel_refs();
        if let TyKind::Udvt(inner, _) = ty.kind {
            return self.lower_abi_decode_static(address, inner, span);
        }
        let value = self.builder.mload(address);
        self.decode_abi_word(ty, value, span)
    }

    fn lower_abi_decode_dynamic(
        &mut self,
        base: ValueId,
        length: ValueId,
        head_size: ValueId,
        head: ValueId,
        ty: Ty<'gcx>,
        span: Span,
    ) -> Option<ValueId> {
        let ty = ty.peel_refs();
        if let TyKind::Udvt(inner, _) = ty.kind {
            return self.lower_abi_decode_dynamic(base, length, head_size, head, inner, span);
        }
        match ty.kind {
            TyKind::Elementary(
                solar_sema::hir::ElementaryType::Bytes | solar_sema::hir::ElementaryType::String,
            ) => self.lower_abi_decode_dynamic_bytes(base, length, head_size, head),
            TyKind::DynArray(element) => {
                self.lower_abi_decode_dynamic_array(base, length, head_size, head, element, span)
            }
            TyKind::Slice(element) => {
                self.lower_abi_decode_dynamic(base, length, head_size, head, element, span)
            }
            _ => report_unsupported(self.gcx, span, "abi.decode target type"),
        }
    }

    fn lower_abi_decode_dynamic_bytes(
        &mut self,
        base: ValueId,
        length: ValueId,
        head_size: ValueId,
        head: ValueId,
    ) -> Option<ValueId> {
        let before_head = self.builder.lt(head, head_size);
        self.revert_if_empty(before_head);
        let word_size = self.builder.imm_u64(32);
        let tail_end = self.builder.add(head, word_size);
        let head_overflow = self.builder.lt(tail_end, head);
        self.revert_if_empty(head_overflow);
        let tail_oob = self.builder.gt(tail_end, length);
        self.revert_if_empty(tail_oob);

        let length_address = self.builder.add(base, head);
        let value_length = self.builder.mload(length_address);
        let thirty_one = self.builder.imm_u64(31);
        let rounded = self.builder.add(value_length, thirty_one);
        let rounded_overflow = self.builder.lt(rounded, value_length);
        self.revert_if_empty(rounded_overflow);
        let mask = self.builder.not(thirty_one);
        let padded = self.builder.and(rounded, mask);
        let payload_end = self.builder.add(tail_end, padded);
        let payload_overflow = self.builder.lt(payload_end, tail_end);
        self.revert_if_empty(payload_overflow);
        let payload_oob = self.builder.gt(payload_end, length);
        self.revert_if_empty(payload_oob);

        let empty = self.builder.iszero(padded);
        let data_size = self.builder.select(empty, word_size, padded);
        let total_size = self.checked_add(word_size, data_size);
        let object = self.builder.alloc_object(
            total_size,
            MemoryObjectLayout::Bytes,
            AllocationSemantics::INTERNAL,
        );
        self.builder.set_memory_object_len(object, value_length, MemoryObjectKind::Bytes);
        let destination = self.builder.memory_object_data(object, MemoryObjectKind::Bytes);
        let source = self.builder.add(length_address, word_size);
        self.builder.mcopy(destination, source, value_length);
        Some(object)
    }

    fn lower_abi_decode_dynamic_array(
        &mut self,
        base: ValueId,
        length: ValueId,
        head_size: ValueId,
        head: ValueId,
        element: Ty<'gcx>,
        span: Span,
    ) -> Option<ValueId> {
        let before_head = self.builder.lt(head, head_size);
        self.revert_if_empty(before_head);
        let word_size = self.builder.imm_u64(32);
        let tail_end = self.builder.add(head, word_size);
        let head_overflow = self.builder.lt(tail_end, head);
        self.revert_if_empty(head_overflow);
        let tail_oob = self.builder.gt(tail_end, length);
        self.revert_if_empty(tail_oob);

        let array_base = self.builder.add(base, head);
        let element_count = self.builder.mload(array_base);
        let shift = self.builder.imm_u64(250);
        let shifted_count = self.builder.shr(shift, element_count);
        let count_in_range = self.builder.iszero(shifted_count);
        let count_invalid = self.builder.iszero(count_in_range);
        self.revert_if_empty(count_invalid);
        let payload_size = self.checked_mul(element_count, word_size);
        let payload_end = self.builder.add(tail_end, payload_size);
        let payload_overflow = self.builder.lt(payload_end, tail_end);
        self.revert_if_empty(payload_overflow);
        let payload_oob = self.builder.gt(payload_end, length);
        self.revert_if_empty(payload_oob);

        let total_size = self.checked_add(word_size, payload_size);
        let layout =
            MemoryObjectLayout::DynamicArray { element_words: self.types.element_words(element) };
        let object = self.builder.alloc_object(total_size, layout, AllocationSemantics::INTERNAL);
        self.builder.set_memory_object_len(object, element_count, MemoryObjectKind::DynamicArray);
        let destination = self.builder.memory_object_data(object, MemoryObjectKind::DynamicArray);
        let source = self.builder.add(array_base, word_size);

        if !self.abi_decode_is_dynamic(element)? {
            self.builder.mcopy(destination, source, payload_size);
            self.lower_abi_decode_elements(element_count, |this, index| {
                let offset = this.builder.mul(index, word_size);
                let address = this.builder.add(destination, offset);
                let value = this.lower_abi_decode_static(address, element, span)?;
                this.builder.memory_object_store_element(object, layout, index, value);
                Some(())
            })?;
        } else {
            let region_length = self.builder.sub(length, tail_end);
            self.lower_abi_decode_elements(element_count, |this, index| {
                let offset = this.builder.mul(index, word_size);
                let address = this.builder.add(source, offset);
                let element_head = this.builder.mload(address);
                let value = this.lower_abi_decode_dynamic(
                    source,
                    region_length,
                    payload_size,
                    element_head,
                    element,
                    span,
                )?;
                this.builder.memory_object_store_element(object, layout, index, value);
                Some(())
            })?;
        }
        Some(object)
    }

    fn lower_abi_decode_elements(
        &mut self,
        length: ValueId,
        mut body: impl FnMut(&mut Self, ValueId) -> Option<()>,
    ) -> Option<()> {
        let preheader = self.builder.current_block();
        let header = self.builder.create_block();
        let body_block = self.builder.create_block();
        let exit = self.builder.create_block();
        self.builder.jump(header);
        self.builder.switch_to_block(header);
        let zero = self.builder.imm_u64(0);
        let index = self.builder.phi(vec![(preheader, zero)]);
        let more = self.builder.lt(index, length);
        self.builder.branch(more, body_block, exit);
        self.builder.switch_to_block(body_block);
        body(self, index)?;
        let one = self.builder.imm_u64(1);
        let next = self.builder.add(index, one);
        let latch = self.builder.current_block();
        self.builder.jump(header);
        self.builder.add_phi_incoming(index, latch, next);
        self.builder.switch_to_block(exit);
        Some(())
    }

    fn decode_abi_word(&mut self, ty: Ty<'gcx>, value: ValueId, span: Span) -> Option<ValueId> {
        let ty = ty.peel_refs();
        if let TyKind::Udvt(inner, _) = ty.kind {
            return self.decode_abi_word(inner, value, span);
        }
        let (cleaned, valid) = match ty.kind {
            TyKind::Elementary(elementary) => match elementary {
                solar_sema::hir::ElementaryType::Bool => {
                    let is_zero = self.builder.iszero(value);
                    let cleaned = self.builder.iszero(is_zero);
                    let valid = self.builder.eq(value, cleaned);
                    (cleaned, valid)
                }
                solar_sema::hir::ElementaryType::Address(_) => {
                    let mask = self.builder.imm_u256((U256::from(1) << 160) - U256::from(1));
                    let cleaned = self.builder.and(value, mask);
                    let valid = self.builder.eq(value, cleaned);
                    (cleaned, valid)
                }
                solar_sema::hir::ElementaryType::UInt(size) => {
                    if size.bits() == 256 {
                        (value, self.builder.imm_bool(true))
                    } else {
                        let mask =
                            self.builder.imm_u256((U256::from(1) << size.bits()) - U256::ONE);
                        let cleaned = self.builder.and(value, mask);
                        let valid = self.builder.eq(value, cleaned);
                        (cleaned, valid)
                    }
                }
                solar_sema::hir::ElementaryType::Int(size) => {
                    if size.bits() == 256 {
                        (value, self.builder.imm_bool(true))
                    } else {
                        let byte = self.builder.imm_u64(u64::from(size.bytes().saturating_sub(1)));
                        let cleaned = self.builder.signextend(byte, value);
                        let valid = self.builder.eq(value, cleaned);
                        (cleaned, valid)
                    }
                }
                solar_sema::hir::ElementaryType::FixedBytes(size) => {
                    let shift = self.builder.imm_u64(u64::from(32 - size.bytes()) * 8);
                    let shifted = self.builder.shr(shift, value);
                    let cleaned = self.builder.shl(shift, shifted);
                    let valid = self.builder.eq(value, cleaned);
                    (cleaned, valid)
                }
                _ => (value, self.builder.imm_bool(true)),
            },
            TyKind::Enum(id) => {
                let count = self.gcx.hir.enumm(id).variants.len();
                let limit = self.builder.imm_u64(count as u64);
                let valid = self.builder.lt(value, limit);
                (value, valid)
            }
            TyKind::Contract(_) => {
                let mask = self.builder.imm_u256((U256::from(1) << 160) - U256::from(1));
                let cleaned = self.builder.and(value, mask);
                let valid = self.builder.eq(value, cleaned);
                (cleaned, valid)
            }
            _ => return report_unsupported(self.gcx, span, "abi.decode target type"),
        };
        let invalid = self.builder.iszero(valid);
        self.revert_if_empty(invalid);
        Some(cleaned)
    }

    fn revert_if_empty(&mut self, condition: ValueId) {
        let revert = self.builder.create_block();
        let continue_block = self.builder.create_block();
        self.builder.branch(condition, revert, continue_block);
        self.builder.switch_to_block(revert);
        let zero = self.builder.imm_u256(U256::ZERO);
        self.builder.revert(zero, zero);
        self.builder.switch_to_block(continue_block);
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
        let pointer = self.builder.slice_ptr(slice);
        let location = match self.builder.func().value_ty(slice) {
            Some(MirType::Slice(location)) => location,
            _ => SliceLocation::Memory,
        };
        let source = self.builder.make_slice(pointer, length, location);
        self.builder.memory_object_copy_from_slice(object, MemoryObjectKind::Bytes, source);
        object
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
                let [_address, _input, _value, _gas] = self.lower_builtin_args(builtin, &args)?;
                self.unsupported_yul_builtin(builtin, args.span)
            }
            Builtin::YulExtdelegatecall | Builtin::YulExtstaticcall => {
                let [_address, _input, _gas] = self.lower_builtin_args(builtin, &args)?;
                self.unsupported_yul_builtin(builtin, args.span)
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

    fn unsupported_yul_builtin<T>(&self, builtin: Builtin, span: Span) -> Option<T> {
        self.gcx
            .dcx()
            .err(format!("unsupported Yul builtin `{}`", builtin.name()))
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
    ) -> Option<ValueId> {
        let function_id = self.resolve_call_target(callee, function_id);
        let function = self.gcx.hir.function(function_id);
        if args.len() != function.parameters.len() {
            return report_unsupported(self.gcx, expr.span, "function argument list");
        }
        let parameter_names =
            self.gcx.call_param_source(callee).map(|source| self.gcx.callable_param_names(source));
        let mut values = Vec::with_capacity(function.parameters.len());
        for index in 0..function.parameters.len() {
            let Some(argument) = args.argument_for_parameter(index, parameter_names.as_deref())
            else {
                return report_unsupported(self.gcx, expr.span, "named function argument");
            };
            let parameter_ty = self.gcx.type_of_item(function.parameters[index].into());
            let value = self.lower_expr(argument)?;
            values.push(self.materialize_memory_argument(parameter_ty, value, argument.span)?);
        }
        let Some(&mir_id) = self.function_ids.get(&function_id) else {
            return self.lower_external_function_call(expr, callee, function_id, args);
        };
        if function.returns.is_empty() {
            self.builder.internal_call_void(mir_id, values, 0);
            return Some(self.builder.imm_u256(U256::ZERO));
        }
        let result_ty = types::TypeLowerer::mir_type(
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
        let gas = self.builder.gas();
        let returns = function.returns.len();
        let ret_offset = if returns > 1 { input } else { zero };
        let ret_size = self.builder.imm_u64((returns as u64).saturating_mul(32));
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
            return Some(zero);
        }
        if returns > 1 {
            let base = self.builder.imm_u64(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT);
            self.builder.mstore(base, ret_offset);
        }
        Some(self.builder.mload(ret_offset))
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
        if matches!(
            self.builder.func().value_ty(value),
            Some(MirType::Slice(SliceLocation::Calldata))
        ) {
            self.materialize_calldata_argument(ty, value, span)
        } else {
            Some(value)
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
        let offset = self.checked_mul(index, word);
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
                    let Some(item) = self.gcx.resolved_expr(receiver).and_then(|res| match res {
                        hir::Res::Item(
                            item @ (hir::ItemId::Function(_) | hir::ItemId::Error(_)),
                        ) => Some(item),
                        _ => None,
                    }) else {
                        return report_unsupported(self.gcx, expr.span, "function selector");
                    };
                    self.gcx.function_selector(item).0
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
            let receiver_ty = self.gcx.type_of_expr(receiver.id)?;
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
            if receiver_ty.is_ref_at(DataLocation::Calldata) {
                let slice = self.lower_expr(receiver)?;
                return Some(self.builder.slice_len(slice));
            }
            let object = self.lower_expr(receiver)?;
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
        let layout = self.types.memory_layout(receiver_ty)?;
        match layout {
            MemoryObjectLayout::DynamicArray { .. } => {
                let length = self.builder.memory_object_len(object, layout.kind());
                self.bounds_check(index, length);
                Some(self.builder.memory_object_load_element(object, layout, index))
            }
            MemoryObjectLayout::FixedArray { len, .. } => {
                let length = self.builder.imm_u64(len);
                self.bounds_check(index, length);
                Some(self.builder.memory_object_load_element(object, layout, index))
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
        let is_bytes = self.is_calldata_dynamic_bytes_type(receiver_ty)
            || matches!(
                receiver_ty.peel_refs().kind,
                TyKind::Elementary(
                    solar_sema::hir::ElementaryType::Bytes
                        | solar_sema::hir::ElementaryType::String,
                )
            );
        if !is_bytes {
            return report_unsupported(self.gcx, expr.span, "slice");
        }

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
        let pointer = self.builder.add(base_ptr, start);
        Some(self.builder.make_slice(pointer, length, location))
    }

    fn load_variable(&mut self, id: VariableId, span: Span) -> Option<ValueId> {
        if let Some(value) = self.values.get(&id).copied() {
            return Some(value);
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
        let var = self.gcx.hir.variable(id);
        if var.is_constant() {
            return self.lower_constant(var.initializer, span);
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

    fn lower_constant(
        &mut self,
        initializer: Option<&hir::Expr<'_>>,
        span: Span,
    ) -> Option<ValueId> {
        let Some(initializer) = initializer else {
            return report_unsupported(self.gcx, span, "constant initializer");
        };
        match self.gcx.try_eval_const_value(initializer).ok()? {
            ConstValue::Bool(value) => Some(self.builder.imm_bool(*value)),
            ConstValue::Integer(value) => Some(self.builder.imm_u256(value.as_u256()?)),
            _ => report_unsupported(self.gcx, span, "constant value"),
        }
    }

    fn binary(&mut self, op: BinOpKind, lhs: ValueId, rhs: ValueId) -> ValueId {
        match op {
            BinOpKind::Add => self.builder.add(lhs, rhs),
            BinOpKind::Sub => self.builder.sub(lhs, rhs),
            BinOpKind::Mul => self.builder.mul(lhs, rhs),
            BinOpKind::Div => self.builder.div(lhs, rhs),
            BinOpKind::Rem => self.builder.mod_(lhs, rhs),
            BinOpKind::Lt => self.builder.lt(lhs, rhs),
            BinOpKind::Gt => self.builder.gt(lhs, rhs),
            BinOpKind::Eq => self.builder.eq(lhs, rhs),
            BinOpKind::Ne => {
                let eq = self.builder.eq(lhs, rhs);
                self.builder.iszero(eq)
            }
            BinOpKind::Le => {
                let gt = self.builder.gt(lhs, rhs);
                self.builder.iszero(gt)
            }
            BinOpKind::Ge => {
                let lt = self.builder.lt(lhs, rhs);
                self.builder.iszero(lt)
            }
            BinOpKind::And | BinOpKind::BitAnd => self.builder.and(lhs, rhs),
            BinOpKind::Or | BinOpKind::BitOr => self.builder.or(lhs, rhs),
            BinOpKind::BitXor => self.builder.xor(lhs, rhs),
            BinOpKind::Shl => self.builder.shl(lhs, rhs),
            BinOpKind::Shr => self.builder.shr(lhs, rhs),
            BinOpKind::Sar => self.builder.sar(lhs, rhs),
            BinOpKind::Pow => self.builder.exp(lhs, rhs),
        }
    }

    fn unary(&mut self, op: UnOpKind, value: ValueId, span: Span) -> Option<ValueId> {
        Some(match op {
            UnOpKind::Not => self.builder.iszero(value),
            UnOpKind::Neg => {
                let zero = self.builder.imm_u256(U256::ZERO);
                self.builder.sub(zero, value)
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
}

fn report_unsupported<T>(gcx: Gcx<'_>, span: Span, what: &str) -> Option<T> {
    gcx.dcx().err(format!("codegen rewrite does not support this {what} yet")).span(span).emit();
    None
}

fn report_error<T>(gcx: Gcx<'_>, span: Span, message: &'static str) -> Option<T> {
    gcx.dcx().err(message).span(span).emit();
    None
}
