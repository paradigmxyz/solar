//! Function-level HIR to MIR lowering.

use super::{
    contract,
    storage::{StorageLayout, StorageLocation},
    types,
};
use crate::{
    memory::EvmMemoryLayout,
    mir::{
        AbiLayout, AbiParamLayout, AllocationSemantics, BlockId, Function, FunctionBuilder,
        FunctionId, MemoryObjectKind, MemoryObjectLayout, Module, SliceLocation, ValueId,
    },
};
use alloy_primitives::U256;
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
            let value = self.default_value(ty);
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

    fn lower_values(&mut self, expr: &hir::Expr<'_>) -> Option<Vec<ValueId>> {
        if let ExprKind::Call(callee, ..) = &expr.kind {
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
        if let ExprKind::Tuple(rhs_elements) = &rhs.peel_parens().kind {
            if rhs_elements.len() != elements.len() {
                return report_unsupported(self.gcx, rhs.span, "tuple assignment arity");
            }
            for (element, value) in elements.iter().zip(rhs_elements.iter()) {
                let Some(value) = value else {
                    if element.is_some() {
                        return report_unsupported(self.gcx, rhs.span, "tuple assignment value");
                    }
                    continue;
                };
                let value = self.lower_expr(value)?;
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
                    rhs_value
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
            ExprKind::Index(receiver, index) => self.lower_index(expr, receiver, *index),
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
            return self.lower_builtin_call(expr, callee, builtin, args);
        }
        if let Some(function_id) = self.gcx.resolved_function(callee) {
            return self.lower_function_call(expr, callee, function_id, args);
        }
        if self.gcx.dcx().has_errors().is_err() {
            return Some(self.builder.imm_u256(U256::ZERO));
        }
        report_unsupported(self.gcx, expr.span, "function call")
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
            let value = self.coerce_value(
                value,
                self.gcx.type_of_expr(argument.id)?,
                self.gcx.type_of_item(field.into()),
            );
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
            let returns_value =
                builtin.ty(self.gcx).returns().is_none_or(|returns| !returns.is_empty());
            return if returns_value {
                self.lower_yul_value_builtin_call(builtin, args)
                    .or_else(|| Some(self.builder.imm_u256(U256::ZERO)))
            } else {
                match self.lower_yul_unit_builtin_call(builtin, args) {
                    Some(()) => None,
                    None => Some(self.builder.imm_u256(U256::ZERO)),
                }
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
                Some(()) => None,
                None => Some(self.builder.imm_u256(U256::ZERO)),
            },
            _ => self
                .lower_solidity_value_builtin_call(builtin, args)
                .or_else(|| Some(self.builder.imm_u256(U256::ZERO))),
        }
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
            _ => return self.unsupported_builtin(builtin, args.span),
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
                let value = self.lower_expr(value)?;
                Some(self.builder.keccak256_bytes(value))
            }
            Builtin::Gasleft => {
                let _ = self.builtin_args::<0>(builtin, &args)?;
                Some(self.builder.gas())
            }
            Builtin::Blockhash | Builtin::Blobhash => {
                let value = &self.builtin_args::<1>(builtin, &args)?[0];
                let value = self.lower_expr(value)?;
                Some(if builtin == Builtin::Blockhash {
                    self.builder.blockhash(value)
                } else {
                    self.builder.blobhash(value)
                })
            }
            Builtin::Sha256 | Builtin::Ripemd160 => self.lower_hash_precompile_call(builtin, args),
            Builtin::EcRecover => self.lower_ecrecover_call(args),
            Builtin::StringConcat | Builtin::BytesConcat => {
                self.lower_concat_builtin_call(builtin, args)
            }
            Builtin::AbiEncode => {
                let _ = self.variadic_builtin_args(builtin, &args)?;
                self.unsupported_builtin(builtin, args.span)
            }
            Builtin::AbiEncodeWithSelector => {
                let _ = self.builtin_args_with_rest::<1>(builtin, &args)?;
                self.unsupported_builtin(builtin, args.span)
            }
            _ => {
                if self.validate_builtin_arity(builtin, &args) {
                    self.unsupported_builtin(builtin, args.span)
                } else {
                    None
                }
            }
        }
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
            Builtin::YulSstore => {
                let [slot, value] = self.lower_builtin_args(builtin, &args)?;
                self.builder.sstore(slot, value);
            }
            Builtin::YulMstore | Builtin::YulMstore8 => {
                let [offset, value] = self.lower_builtin_args(builtin, &args)?;
                if builtin == Builtin::YulMstore {
                    self.builder.mstore(offset, value);
                } else {
                    self.builder.mstore8(offset, value);
                }
            }
            Builtin::YulPop => {
                let [_value] = self.lower_builtin_args(builtin, &args)?;
            }
            _ => {
                if self.validate_builtin_arity(builtin, &args) {
                    return self.unsupported_yul_builtin(builtin, args.span);
                }
                return None;
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
            Builtin::YulSload => lower!(sload(slot)),
            Builtin::YulMload => lower!(mload(offset)),
            Builtin::YulAdd => lower!(add(lhs, rhs)),
            Builtin::YulSub => lower!(sub(lhs, rhs)),
            Builtin::YulMul => lower!(mul(lhs, rhs)),
            Builtin::YulDiv => lower!(div(lhs, rhs)),
            Builtin::YulMod => lower!(mod_(lhs, rhs)),
            Builtin::YulEq => lower!(eq(lhs, rhs)),
            Builtin::YulLt => lower!(lt(lhs, rhs)),
            Builtin::YulGt => lower!(gt(lhs, rhs)),
            Builtin::YulAnd => lower!(and(lhs, rhs)),
            Builtin::YulOr => lower!(or(lhs, rhs)),
            Builtin::YulXor => lower!(xor(lhs, rhs)),
            Builtin::YulExtcall => {
                let [_address, _input, _value, _gas] = self.lower_builtin_args(builtin, &args)?;
                self.unsupported_yul_builtin(builtin, args.span)
            }
            Builtin::YulExtdelegatecall | Builtin::YulExtstaticcall => {
                let [_address, _input, _gas] = self.lower_builtin_args(builtin, &args)?;
                self.unsupported_yul_builtin(builtin, args.span)
            }
            _ => {
                if self.validate_builtin_arity(builtin, &args) {
                    self.unsupported_yul_builtin(builtin, args.span)
                } else {
                    None
                }
            }
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
            values.push(self.lower_expr(argument)?);
        }
        let Some(&mir_id) = self.function_ids.get(&function_id) else {
            return report_unsupported(self.gcx, expr.span, "function target");
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
            return self.lower_environment_builtin(expr, builtin);
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
        let receiver_ty = self.gcx.type_of_expr(receiver.id)?;
        let layout = self.types.memory_layout(receiver_ty)?;
        Some(self.builder.memory_object_load_field(object, layout, field as u64))
    }

    fn lower_environment_builtin(
        &mut self,
        expr: &hir::Expr<'_>,
        builtin: Builtin,
    ) -> Option<ValueId> {
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
