//! Function-level HIR to MIR lowering.

use super::{contract, storage::StorageLayout, types};
use crate::mir::{
    AbiLayout, AbiParamLayout, AllocationSemantics, BlockId, Function, FunctionBuilder, FunctionId,
    MemoryObjectKind, MemoryObjectLayout, Module, ValueId,
};
use alloy_primitives::U256;
use solar_ast::{BinOpKind, LitKind, UnOpKind};
use solar_data_structures::map::{FxHashMap, StdEntry};
use solar_interface::{Span, sym};
use solar_sema::{
    Gcx,
    builtins::Builtin,
    eval::ConstValue,
    hir::{self, ExprKind, LoopSource, StmtKind, VariableId},
};

/// Lowers one HIR function into a typed MIR function.
pub(super) fn lower(
    gcx: Gcx<'_>,
    module: &mut Module,
    storage: &StorageLayout,
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

    let mut lowerer = FunctionLowerer::new(gcx, storage, function_ids, &mut mir);
    lowerer.bind_signature(hir_function);
    if let Some(body) = hir_function.body {
        lowerer.lower_block(body)?;
    }
    if !lowerer.is_terminated() {
        lowerer.finish(hir_function.returns);
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
    function_ids: &'ids FxHashMap<hir::FunctionId, FunctionId>,
    builder: FunctionBuilder<'mir>,
    types: types::TypeLowerer<'gcx>,
    values: FxHashMap<VariableId, ValueId>,
    returns: Vec<VariableId>,
    loops: Vec<LoopTargets>,
}

#[derive(Clone, Copy)]
struct LoopTargets {
    break_block: BlockId,
    continue_block: BlockId,
}

impl<'gcx, 'mir, 'ids> FunctionLowerer<'gcx, 'mir, 'ids> {
    fn new(
        gcx: Gcx<'gcx>,
        storage: &'mir StorageLayout,
        function_ids: &'ids FxHashMap<hir::FunctionId, FunctionId>,
        function: &'mir mut Function,
    ) -> Self {
        Self {
            gcx,
            storage,
            function_ids,
            builder: FunctionBuilder::new(function),
            types: types::TypeLowerer::new(gcx),
            values: FxHashMap::default(),
            returns: Vec::new(),
            loops: Vec::new(),
        }
    }

    fn bind_signature(&mut self, function: &hir::Function<'_>) {
        for &param in function.parameters {
            let value = self
                .builder
                .add_param(types::TypeLowerer::mir_type(self.gcx.type_of_item(param.into())));
            self.values.insert(param, value);
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
                let value = if let Some(expr) = initializer {
                    self.lower_expr(expr)?
                } else if let Some(value) = self.default_object(self.gcx.type_of_item((*id).into()))
                {
                    value
                } else {
                    self.builder.imm_u256(U256::ZERO)
                };
                self.values.insert(*id, value);
            }
            StmtKind::DeclMulti(ids, expr) => {
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
                let Some(targets) = self.loops.last().copied() else {
                    return report_unsupported(self.gcx, stmt.span, "break outside loop");
                };
                self.builder.jump(targets.break_block);
            }
            StmtKind::Continue => {
                let Some(targets) = self.loops.last().copied() else {
                    return report_unsupported(self.gcx, stmt.span, "continue outside loop");
                };
                self.builder.jump(targets.continue_block);
            }
            StmtKind::Return(expr) => {
                let values =
                    expr.map_or_else(|| Some(Vec::new()), |expr| self.lower_values(expr))?;
                if !self.is_terminated() {
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
                return report_unsupported(self.gcx, stmt.span, "modifier placeholder");
            }
            StmtKind::Emit(_) | StmtKind::Switch(_) | StmtKind::Try(_) | StmtKind::Err(_) => {
                return report_unsupported(self.gcx, stmt.span, "statement");
            }
        }
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
        self.builder.branch(condition, then_block, else_block);

        self.builder.switch_to_block(then_block);
        self.lower_stmt(then_stmt)?;
        let then_terminated = self.is_terminated();
        let then_exit = self.builder.current_block();
        let then_values = self.values.clone();
        if !then_terminated {
            self.builder.jump(merge_block);
        }

        self.values = before.clone();
        self.builder.switch_to_block(else_block);
        if let Some(stmt) = else_stmt {
            self.lower_stmt(stmt)?;
        }
        let else_terminated = self.is_terminated();
        let else_exit = self.builder.current_block();
        let else_values = self.values.clone();
        if !else_terminated {
            self.builder.jump(merge_block);
        }

        self.builder.switch_to_block(merge_block);
        self.values = merge_values(
            &mut self.builder,
            before,
            then_exit,
            then_values,
            then_terminated,
            else_exit,
            else_values,
            else_terminated,
        );
        Some(())
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
        self.values = merge_values(
            &mut self.builder,
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
        let header = self.builder.create_block();
        let exit = self.builder.create_block();
        self.builder.jump(header);
        self.builder.switch_to_block(header);
        self.loops.push(LoopTargets { break_block: exit, continue_block: header });
        self.lower_block(block)?;
        self.loops.pop();
        if !self.is_terminated() {
            self.builder.jump(header);
        }
        self.builder.switch_to_block(exit);
        Some(())
    }

    fn lower_values(&mut self, expr: &hir::Expr<'_>) -> Option<Vec<ValueId>> {
        if let ExprKind::Call(callee, ..) = &expr.kind {
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
                let rhs = self.lower_expr(rhs)?;
                let value = if let Some(kind) = op.map(|op| op.kind) {
                    let lhs = self.load_lvalue(lhs)?;
                    self.binary(kind, lhs, rhs)
                } else {
                    rhs
                };
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

    fn lower_array(&mut self, expr: &hir::Expr<'_>, elements: &[hir::Expr<'_>]) -> Option<ValueId> {
        let ty = self.gcx.type_of_expr(expr.id)?;
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
        if matches!(callee.kind, ExprKind::TypeCall(_) | ExprKind::Type(_)) {
            let [arg] = arguments.as_slice() else {
                return report_unsupported(self.gcx, expr.span, "type conversion");
            };
            return self.lower_expr(arg);
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
            return self.lower_builtin_call(expr, builtin, &arguments);
        }
        if let Some(function_id) = self.gcx.resolved_function(callee) {
            return self.lower_function_call(expr, callee, function_id, args);
        }
        report_unsupported(self.gcx, expr.span, "function call")
    }

    fn lower_builtin_call(
        &mut self,
        expr: &hir::Expr<'_>,
        builtin: Builtin,
        arguments: &[&hir::Expr<'_>],
    ) -> Option<ValueId> {
        match builtin {
            Builtin::Revert if arguments.is_empty() => {
                let zero = self.builder.imm_u256(U256::ZERO);
                self.builder.revert(zero, zero);
                Some(zero)
            }
            Builtin::Assert => {
                let [condition] = arguments else {
                    return report_unsupported(self.gcx, expr.span, "assert arguments");
                };
                let condition = self.lower_expr(condition)?;
                let invalid = self.builder.iszero(condition);
                self.panic_if(invalid, 0x01);
                Some(self.builder.imm_u256(U256::ZERO))
            }
            _ => report_unsupported(self.gcx, expr.span, "builtin call"),
        }
    }

    fn panic_if(&mut self, condition: ValueId, code: u64) {
        let panic_block = self.builder.create_block();
        let continue_block = self.builder.create_block();
        self.builder.branch(condition, panic_block, continue_block);
        self.builder.switch_to_block(panic_block);
        let selector = self.builder.imm_u256(U256::from(0x4e48_7b71_u64) << 224);
        let code = self.builder.imm_u256(U256::from(code) << 224);
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
        let quotient = self.builder.div(result, rhs);
        let exact = self.builder.eq(quotient, lhs);
        let overflow = self.builder.iszero(exact);
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
        if function.returns.len() != 1 {
            return report_unsupported(self.gcx, expr.span, "multiple return values");
        }
        let result_ty =
            types::TypeLowerer::mir_type(self.gcx.type_of_item(function.returns[0].into()));
        Some(self.builder.internal_call(mir_id, values, result_ty, 1))
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
        if name.name == sym::length {
            let object = self.lower_expr(receiver)?;
            let ty = self.gcx.type_of_expr(receiver.id)?;
            let layout = self.types.memory_layout(ty)?;
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

    fn lower_index(
        &mut self,
        expr: &hir::Expr<'_>,
        receiver: &hir::Expr<'_>,
        index: Option<&hir::Expr<'_>>,
    ) -> Option<ValueId> {
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
                Some(self.builder.memory_object_load_byte(object, index))
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
        if let Some(location) = self.storage.get(id) {
            return Some(self.storage.load(&mut self.builder, location));
        }
        let var = self.gcx.hir.variable(id);
        if var.is_constant() {
            return self.lower_constant(var.initializer, span);
        }
        report_unsupported(self.gcx, span, "identifier")
    }

    fn load_lvalue(&mut self, expr: &hir::Expr<'_>) -> Option<ValueId> {
        if let Some(id) = self.gcx.resolved_variable(expr) {
            let variable = self.gcx.hir.variable(id);
            if variable.is_state_variable()
                || self.values.contains_key(&id)
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

    fn store_lvalue(&mut self, expr: &hir::Expr<'_>, value: ValueId) -> Option<()> {
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
}

fn merge_values(
    builder: &mut FunctionBuilder<'_>,
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
                Some(builder.phi(vec![(then_block, lhs), (else_block, rhs)]))
            }
            _ => then_value.or(else_value),
        };
        if let Some(value) = value {
            values.insert(id, value);
        }
    }
    values
}

fn report_unsupported<T>(gcx: Gcx<'_>, span: Span, what: &str) -> Option<T> {
    gcx.dcx().err(format!("codegen rewrite does not support this {what} yet")).span(span).emit();
    None
}
