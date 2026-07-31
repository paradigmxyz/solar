//! Statement lowering.

use super::{LoopContext, Lowerer, MIN_BULK_ZERO_MEMORY_WORDS};
use crate::{
    memory::EvmMemoryLayout,
    mir::{FunctionBuilder, MemoryObjectKind, ValueId},
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_interface::{Span, diagnostics::ErrorGuaranteed, kw, sym};
use solar_sema::{
    builtins::Builtin,
    hir::{self, ElementaryType, ExprKind, StmtKind},
    ty::{CallableParamSource, Ty, TyKind},
};

impl<'gcx> Lowerer<'gcx> {
    /// Lowers a block of statements.
    pub(super) fn lower_block(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        block: &hir::Block<'_>,
    ) {
        let mut index = 0;
        while let Some(stmt) = block.stmts.get(index) {
            if let Some(args) =
                self.immediate_packed_hash_return_args(stmt, block.stmts.get(index + 1))
                && self.lower_immediate_packed_hash_return(builder, &args)
            {
                break;
            }

            self.lower_stmt(builder, stmt);
            if builder.func().block(builder.current_block()).terminator.is_some() {
                break;
            }
            index += 1;
        }
    }

    fn immediate_packed_hash_return_args(
        &self,
        stmt: &hir::Stmt<'_>,
        next: Option<&hir::Stmt<'_>>,
    ) -> Option<hir::CallArgs<'gcx>> {
        let StmtKind::DeclSingle(var_id) = stmt.kind else {
            return None;
        };
        let var = self.gcx.hir.variable(var_id);
        let initializer = var.initializer?;
        if self.hir_has_errors && self.expr_references_error(initializer).is_err() {
            return None;
        }
        let packed_args = self.abi_encode_packed_call_args(initializer)?;

        let StmtKind::Return(Some(ret)) = &next?.kind else {
            return None;
        };
        self.is_keccak_call_of_local(ret, var_id).then_some(*packed_args)
    }

    fn is_keccak_call_of_local(&self, expr: &hir::Expr<'_>, var_id: hir::VariableId) -> bool {
        let ExprKind::Call(callee, args, _) = &expr.kind else {
            return false;
        };
        if self.gcx.resolved_builtin(callee) != Some(Builtin::Keccak256) {
            return false;
        }

        let hir::CallArgsKind::Unnamed([arg]) = args.kind else {
            return false;
        };
        self.resolves_to_variable(arg, var_id)
    }

    fn resolves_to_variable(&self, expr: &hir::Expr<'_>, var_id: hir::VariableId) -> bool {
        self.gcx.resolved_variable(expr) == Some(var_id)
    }

    fn lower_immediate_packed_hash_return(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        args: &hir::CallArgs<'_>,
    ) -> bool {
        if self.current_return_tys.len() != 1 {
            return false;
        }
        let ty = self.current_return_tys[0];
        let hash = match self
            .variadic_builtin_args(Builtin::AbiEncodePacked, args)
            .and_then(|exprs| self.lower_keccak_abi_encode_packed(builder, exprs))
        {
            Ok(hash) => hash,
            Err(guar) => builder.error_value(guar),
        };
        self.finish_return(builder, vec![(hash, ty)]);
        true
    }

    /// Lowers a statement to MIR.
    pub(super) fn lower_stmt(&mut self, builder: &mut FunctionBuilder<'_>, stmt: &hir::Stmt<'_>) {
        match &stmt.kind {
            StmtKind::DeclSingle(var_id) => {
                self.lower_single_var_decl(builder, *var_id);
            }

            StmtKind::DeclMulti(var_ids, init) => {
                self.lower_multi_var_decl(builder, var_ids, init);
            }

            StmtKind::Expr(expr) => {
                let _ = self.lower_expr(builder, expr);
            }

            StmtKind::Block(block) => {
                self.lower_block(builder, block);
            }

            StmtKind::If(cond, then_stmt, else_stmt) => {
                self.lower_if(builder, cond, then_stmt, *else_stmt);
            }

            StmtKind::Loop(block, source) => {
                self.lower_loop(builder, block, *source);
            }

            StmtKind::Switch(switch) => {
                self.lower_switch(builder, switch);
            }

            StmtKind::Return(value) => {
                self.lower_return(builder, *value);
            }

            StmtKind::Revert(expr) => {
                let _ = self.lower_expr(builder, expr);
                if builder.func().block(builder.current_block()).terminator.is_none() {
                    let zero = builder.imm_u64(0);
                    builder.revert(zero, zero);
                }
            }

            StmtKind::Emit(expr) => {
                self.lower_emit(builder, expr);
            }

            StmtKind::Try(try_stmt) => {
                self.lower_try(builder, try_stmt);
            }

            StmtKind::Continue => {
                if let Some(loop_ctx) = self.current_loop() {
                    builder.jump(loop_ctx.continue_target);
                }
            }

            StmtKind::Break => {
                if let Some(loop_ctx) = self.current_loop() {
                    builder.jump(loop_ctx.break_target);
                }
            }

            StmtKind::Placeholder => {}

            StmtKind::UncheckedBlock(block) => self.lower_unchecked_block(builder, block),

            StmtKind::AssemblyBlock(block) => {
                self.lower_block(builder, block);
            }

            StmtKind::Err(_) => {}
        }
    }

    fn lower_unchecked_block(&mut self, builder: &mut FunctionBuilder<'_>, block: &hir::Block<'_>) {
        let prev = self.in_unchecked_block;
        self.in_unchecked_block = true;
        self.lower_block(builder, block);
        self.in_unchecked_block = prev;
    }
    /// Lowers a single variable declaration.
    /// Variables that are never assigned after declaration and don't involve external calls
    /// are kept as SSA values. Variables that are assigned later or initialized from external
    /// calls (which use shared memory) are stored in memory.
    fn lower_single_var_decl(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        var_id: hir::VariableId,
    ) {
        let var = self.gcx.hir.variable(var_id);
        let var_ty = self.gcx.type_of_item(var_id.into());

        // Storage reference: `T storage r = <lvalue>`. Bind the storage *slot*
        // (not the dereferenced value) so `r.field` reads/writes `sload`/`sstore`
        // at `slot + offset` rather than treating the value as a memory pointer.
        if var.data_location == Some(solar_ast::DataLocation::Storage) {
            self.storage_ref_locals.insert(var_id);
            let slot = if let Some(init) = var.initializer {
                if let Some(slot) = self.lower_lvalue_slot(builder, init) {
                    Some(slot)
                } else {
                    // Unhandled storage-reference initializer: don't silently
                    // miscompile it as a memory pointer.
                    self.gcx
                        .dcx()
                        .err("unsupported storage reference initializer")
                        .span(init.span)
                        .emit();
                    return;
                }
            } else {
                None
            };

            if self.is_var_assigned(&var_id) {
                // Reserve a mergeable slot without inventing an initial storage address.
                // Storage-reference assignment writes the actual address into it.
                let offset = self.alloc_local_memory(var_id);
                if let Some(slot) = slot {
                    let addr = self.local_memory_addr(builder, offset);
                    builder.mstore(addr, slot);
                }
            } else if let Some(slot) = slot {
                self.locals.insert(var_id, slot);
            }
            return;
        }

        // Check if initializer involves external calls (results stored in shared memory)
        let has_external_call = var.initializer.is_some_and(|init| self.has_external_call(init));

        // Check if this is a struct type - struct returns from external calls are already
        // allocated in proper memory, so they don't need extra local memory storage
        let is_struct_type = matches!(var_ty.peel_refs().kind, TyKind::Struct(_));

        // Variables need memory storage if they are assigned after declaration
        // or initialized from external calls, which write to shared memory at
        // offset zero. Struct results already have properly allocated memory.
        let needs_local_memory =
            self.is_var_assigned(&var_id) || (has_external_call && !is_struct_type);
        let is_calldata_dynamic = Lowerer::calldata_dynamic_var_kind(var).is_some();

        // An uninitialized SSA value local is semantically zero. Leave it absent from the map
        // until it is actually read; `lower_ident` materializes that zero on demand.
        if var.initializer.is_none() && !needs_local_memory && var_ty.is_value_type() {
            return;
        }
        if var.initializer.is_none() && !needs_local_memory && is_calldata_dynamic {
            return;
        }

        let initial_value = if let Some(init) = var.initializer {
            if self.var_expects_memory_bytes_value(var) {
                self.lower_expr_as_memory_bytes(builder, init)
            } else if self.var_expects_memory_dyn_array_value(var) {
                self.lower_expr_as_memory_dyn_array(builder, init)
            } else {
                self.lower_value_expr(builder, init)
            }
        } else {
            self.lower_default_variable_value(builder, var_id).unwrap_or_else(|| {
                self.err_value(builder, var.span, "codegen cannot initialize this local variable")
            })
        };

        if needs_local_memory {
            if is_calldata_dynamic {
                // A rebindable calldata slice local keeps its two words in a
                // dedicated slot so joins read one merged representation. An
                // uninitialized one seeds an empty slice, not a zero word, so
                // the slot store projects a real `make_slice` that folds away
                // rather than a `slice_ptr`/`slice_len` of a non-slice value.
                let offset = self.alloc_local_slice_memory(var_id);
                self.store_slice_slot(builder, offset, initial_value);
                return;
            }
            let offset = self.alloc_local_memory(var_id);
            let offset_val = self.local_memory_addr(builder, offset);
            builder.mstore(offset_val, initial_value);
        } else {
            // Variable is never reassigned and not from external call - keep as SSA value
            self.locals.insert(var_id, initial_value);
        }
    }

    pub(super) fn zero_memory_field_value_ty(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        ty: Ty<'gcx>,
        span: Span,
    ) -> ValueId {
        let ty = ty.peel_refs();
        match ty.kind {
            TyKind::Array(elem_ty, len) => {
                let Some(len) = u64::try_from(len).ok() else {
                    return self.err_value(
                        builder,
                        span,
                        "fixed-size memory array is too large for codegen",
                    );
                };
                let Some(alloc_size) = len.checked_mul(32) else {
                    return self.err_value(
                        builder,
                        span,
                        "fixed-size memory array is too large for codegen",
                    );
                };
                let ptr = self.allocate_memory_object(
                    builder,
                    alloc_size,
                    crate::mir::MemoryObjectKind::FixedArray,
                );
                if len >= MIN_BULK_ZERO_MEMORY_WORDS && elem_ty.peel_refs().is_value_type() {
                    let size = builder.imm_u64(alloc_size);
                    builder.memory_zero(ptr, size);
                    return ptr;
                }
                for i in 0..len {
                    let value = self.zero_memory_field_value_ty(builder, elem_ty, span);
                    let index = builder.imm_u64(i);
                    let addr = builder.memory_object_element_addr(
                        ptr,
                        crate::mir::MemoryObjectLayout::word_fixed_array(len),
                        index,
                    );
                    builder.mstore(addr, value);
                }
                ptr
            }
            TyKind::DynArray(_) => {
                let ptr = self.allocate_memory_object(
                    builder,
                    32,
                    crate::mir::MemoryObjectKind::DynamicArray,
                );
                let zero = builder.imm_u256(U256::ZERO);
                builder.set_memory_object_len(
                    ptr,
                    zero,
                    crate::mir::MemoryObjectKind::DynamicArray,
                );
                ptr
            }
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                let ptr =
                    self.allocate_memory_object(builder, 32, crate::mir::MemoryObjectKind::Bytes);
                let zero = builder.imm_u256(U256::ZERO);
                builder.set_memory_object_len(ptr, zero, crate::mir::MemoryObjectKind::Bytes);
                ptr
            }
            TyKind::Struct(struct_id) => {
                let ptr = self.allocate_memory_object(
                    builder,
                    self.calculate_memory_words_for_ty(ty) * 32,
                    crate::mir::MemoryObjectKind::Struct,
                );
                self.zero_initialize_memory_struct(builder, struct_id, ptr, span);
                ptr
            }
            TyKind::Err(guar) => builder.error_value(guar),
            _ if ty.is_value_type() => builder.imm_u256(U256::ZERO),
            _ => self.err_value(builder, span, "codegen cannot materialize this memory default"),
        }
    }

    fn zero_initialize_memory_struct(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        struct_id: hir::StructId,
        ptr: ValueId,
        span: Span,
    ) {
        let field_tys = self.gcx.struct_field_types(struct_id).to_vec();
        let layout = crate::mir::MemoryObjectLayout::structure(field_tys.len() as u64);
        for (i, field_ty) in field_tys.into_iter().enumerate() {
            let value = self.zero_memory_field_value_ty(builder, field_ty, span);
            let field_addr = builder.memory_object_field_addr(ptr, layout, i as u64);
            builder.mstore(field_addr, value);
        }
    }

    /// Lowers a multi-variable declaration.
    /// Multi-return expressions leave their first value in MIR and stage the
    /// remaining values in the ephemeral multi-return buffer.
    pub(super) fn lower_multi_var_decl(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        var_ids: &[Option<hir::VariableId>],
        init: &hir::Expr<'_>,
    ) {
        let bound: SmallVec<[bool; 4]> = var_ids.iter().map(Option::is_some).collect();
        let Some(values) = self.lower_multi_values(builder, &bound, init) else { return };
        for (&var_id, value) in var_ids.iter().zip(values) {
            if let (Some(var_id), Some(value)) = (var_id, value) {
                self.bind_local_value(builder, var_id, value);
            }
        }
    }

    /// Binds a lowered value to a freshly declared local, mirroring
    /// single-declaration lowering: a reassigned local gets a memory slot (two
    /// words for a calldata slice), everything else stays an SSA value.
    fn bind_local_value(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        var_id: hir::VariableId,
        val: ValueId,
    ) {
        let var = self.gcx.hir.variable(var_id);
        if var.data_location == Some(solar_ast::DataLocation::Storage) {
            self.storage_ref_locals.insert(var_id);
        }
        if Self::calldata_dynamic_var_kind(var).is_some() {
            if self.is_var_assigned(&var_id) {
                let offset = self.alloc_local_slice_memory(var_id);
                self.store_slice_slot(builder, offset, val);
            } else {
                self.locals.insert(var_id, val);
            }
        } else if self.is_var_assigned(&var_id) {
            let offset = self.alloc_local_memory(var_id);
            let addr = self.local_memory_addr(builder, offset);
            builder.mstore(addr, val);
        } else {
            self.locals.insert(var_id, val);
        }
    }

    /// Assigns a multi-valued RHS to a tuple of EXISTING lvalues, `(a, b) = rhs`.
    /// Mirrors [`Self::lower_multi_var_decl`] but stores through the existing
    /// lvalues (via [`Self::lower_assign`]) instead of allocating fresh locals.
    /// Holes in the tuple (`(x, )`) are skipped.
    pub(super) fn lower_tuple_assign(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        elements: &[Option<&hir::Expr<'_>>],
        rhs: &hir::Expr<'_>,
    ) {
        let bound: SmallVec<[bool; 4]> = elements.iter().map(Option::is_some).collect();
        let Some(values) = self.lower_multi_values(builder, &bound, rhs) else { return };
        for (&element, value) in elements.iter().zip(values) {
            if let (Some(element), Some(value)) = (element, value) {
                self.lower_assign(builder, element, value);
            }
        }
    }

    /// Evaluates a tuple or multi-return expression and snapshots every bound
    /// value before any declaration or assignment writes memory.
    fn lower_multi_values(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        bound: &[bool],
        expr: &hir::Expr<'_>,
    ) -> Option<Vec<Option<ValueId>>> {
        if let hir::ExprKind::Tuple(elements) = &expr.peel_parens().kind {
            let values = self.lower_tuple_values(builder, elements, expr.span).ok()?;
            if values.len() != bound.len() {
                self.gcx.dcx().err("tuple arity mismatch in codegen").span(expr.span).emit();
                return None;
            }
            return Some(
                bound.iter().zip(values).map(|(&bound, value)| bound.then_some(value)).collect(),
            );
        }

        if self.is_low_level_call_expr(expr) {
            if bound.iter().skip(1).any(|&bound| bound)
                && !self.gcx.sess.opts.evm_version.supports_returndata()
            {
                self.gcx
                    .dcx()
                    .err("codegen cannot bind low-level call returndata before Byzantium")
                    .span(expr.span)
                    .emit();
                return None;
            }
            let success = self.lower_value_expr(builder, expr);
            return Some(
                bound
                    .iter()
                    .enumerate()
                    .map(|(i, &bound)| {
                        bound.then(|| {
                            if i == 0 {
                                success
                            } else {
                                self.materialize_returndata_bytes(builder)
                            }
                        })
                    })
                    .collect(),
            );
        }

        let delivers_pending = self.is_slice_multi_return_call(expr);
        self.pending_inline_returns = None;
        let first = self.lower_value_expr(builder, expr);
        if delivers_pending && let Some(values) = self.pending_inline_returns.take() {
            return Some(
                bound
                    .iter()
                    .enumerate()
                    .map(|(i, &bound)| bound.then(|| values.get(i).copied().unwrap_or(first)))
                    .collect(),
            );
        }
        self.pending_inline_returns = None;

        if !bound.iter().skip(1).any(|&bound| bound) {
            return Some(
                bound
                    .iter()
                    .enumerate()
                    .map(|(i, &bound)| (bound && i == 0).then_some(first))
                    .collect(),
            );
        }
        let ptr_slot = builder.imm_u64(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT);
        let tail_base = builder.mload(ptr_slot);
        Some(
            bound
                .iter()
                .enumerate()
                .map(|(i, &bound)| {
                    bound.then(|| {
                        if i == 0 {
                            first
                        } else {
                            let offset = builder.imm_u64(i as u64 * 32);
                            let addr = builder.add(tail_base, offset);
                            builder.mload(addr)
                        }
                    })
                })
                .collect(),
        )
    }

    /// Lowers every component of a tuple value without materializing an
    /// aggregate or staging values through the multi-return buffer.
    pub(super) fn lower_tuple_values(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        elements: &[Option<&hir::Expr<'_>>],
        span: Span,
    ) -> Result<SmallVec<[ValueId; 4]>, ErrorGuaranteed> {
        let mut values = SmallVec::new();
        for &element in elements {
            let Some(element) = element else {
                return Err(self
                    .gcx
                    .dcx()
                    .err("tuple value contains an omitted element")
                    .span(span)
                    .emit());
            };
            values.push(self.lower_value_expr(builder, element));
        }
        Ok(values)
    }

    /// Stages return values 2..N at the unbumped free-memory pointer and
    /// publishes the buffer base through the memory policy's scratch slot.
    pub(super) fn stage_multi_return_tail(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        values: &[ValueId],
    ) {
        if values.len() <= 1 {
            return;
        }
        self.stage_multi_return_values_from(builder, values, 1);
    }

    /// Stages every value for a control-flow expression whose first result
    /// must also cross a block boundary through the return buffer.
    pub(super) fn stage_multi_return_values(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        values: &[ValueId],
    ) {
        self.stage_multi_return_values_from(builder, values, 0);
    }

    fn stage_multi_return_values_from(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        values: &[ValueId],
        start: usize,
    ) {
        let base = builder.fmp();
        for (i, &value) in values.iter().enumerate().skip(start) {
            let offset = builder.imm_u64(i as u64 * 32);
            let addr = builder.add(base, offset);
            builder.mstore(addr, value);
        }
        let ptr_slot = builder.imm_u64(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT);
        builder.mstore(ptr_slot, base);
    }

    /// Loads the base published by the latest multi-return producer.
    pub(super) fn multi_return_buffer_base(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
    ) -> ValueId {
        let ptr_slot = builder.imm_u64(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT);
        builder.mload(ptr_slot)
    }

    /// Loads return value `index` from an already-snapshotted buffer base.
    pub(super) fn load_multi_return_value(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        base: ValueId,
        index: usize,
    ) -> ValueId {
        let offset = builder.imm_u64(index as u64 * 32);
        let addr = builder.add(base, offset);
        builder.mload(addr)
    }

    pub(super) fn is_low_level_call_expr(&self, expr: &hir::Expr<'_>) -> bool {
        let ExprKind::Call(callee, ..) = &expr.kind else { return false };
        let ExprKind::Member(base, member) = &callee.kind else { return false };
        matches!(member.name, kw::Call | kw::Staticcall | kw::Delegatecall)
            && !self.is_contract_type_expr(base)
    }

    /// Lowers an if statement.
    fn lower_if(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        cond: &hir::Expr<'_>,
        then_stmt: &hir::Stmt<'_>,
        else_stmt: Option<&hir::Stmt<'_>>,
    ) {
        let cond_val = self.lower_value_expr(builder, cond);

        let then_block = builder.create_block();
        let merge_block = builder.create_block();
        let else_block = if else_stmt.is_some() { builder.create_block() } else { merge_block };

        builder.branch(cond_val, then_block, else_block);

        builder.switch_to_block(then_block);
        self.lower_stmt(builder, then_stmt);
        if !builder.func().block(builder.current_block()).is_terminated() {
            builder.jump(merge_block);
        }

        if let Some(else_stmt) = else_stmt {
            builder.switch_to_block(else_block);
            self.lower_stmt(builder, else_stmt);
            if !builder.func().block(builder.current_block()).is_terminated() {
                builder.jump(merge_block);
            }
        }

        builder.switch_to_block(merge_block);
    }

    /// Lowers a switch statement.
    fn lower_switch(&mut self, builder: &mut FunctionBuilder<'_>, switch: &hir::StmtSwitch<'_>) {
        let selector = self.lower_value_expr(builder, switch.selector);
        let merge_block = builder.create_block();
        let mut case_blocks = Vec::new();
        let mut body_blocks = Vec::new();
        let mut default_block = merge_block;

        for case in switch.cases {
            let block = builder.create_block();
            if let Some(constant) = case.constant {
                let value = self.lower_literal(builder, constant);
                case_blocks.push((value, block));
            } else {
                default_block = block;
            }
            body_blocks.push((case, block));
        }

        builder.switch(selector, default_block, case_blocks);

        for (case, block) in body_blocks {
            builder.switch_to_block(block);
            self.lower_block(builder, &case.body);
            if !builder.func().block(builder.current_block()).is_terminated() {
                builder.jump(merge_block);
            }
        }

        builder.switch_to_block(merge_block);
    }

    /// Lowers a loop statement (desugared from for/while/do-while).
    fn lower_loop(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        block: &hir::Block<'_>,
        source: hir::LoopSource,
    ) {
        let loop_block = builder.create_block();
        let exit_block = builder.create_block();

        // For `for` loops, we need a separate update block for `continue` to jump to.
        // The desugared structure is: if (cond) { body; update; } else { break; }
        // We need to handle the update separately so continue jumps to it.
        let (continue_target, is_for_with_update) = if source == hir::LoopSource::For {
            if self.is_for_loop_with_update(block) {
                let update_block = builder.create_block();
                (update_block, true)
            } else {
                (loop_block, false)
            }
        } else {
            (loop_block, false)
        };

        // Push loop context for break/continue
        self.push_loop(LoopContext { break_target: exit_block, continue_target });

        builder.jump(loop_block);

        builder.switch_to_block(loop_block);

        // For for loops with update, lower body without the update, then emit update block
        if is_for_with_update {
            self.lower_for_loop_body(builder, block, continue_target, loop_block);
        } else {
            self.lower_block(builder, block);
            if !builder.func().block(builder.current_block()).is_terminated() {
                builder.jump(loop_block);
            }
        }

        // Pop loop context
        self.pop_loop();

        builder.switch_to_block(exit_block);
    }

    /// Checks if a for loop has an update expression in the expected desugared structure.
    fn is_for_loop_with_update(&self, block: &hir::Block<'_>) -> bool {
        let stmts = block.stmts;
        if stmts.len() != 1 {
            return false;
        }

        let StmtKind::If(_, then_stmt, _) = &stmts[0].kind else {
            return false;
        };

        let StmtKind::Block(b) = &then_stmt.kind else {
            return false;
        };

        // Need at least 2 statements: body and update
        if b.stmts.len() < 2 {
            return false;
        }

        // Last statement should be an expression (the update)
        matches!(b.stmts.last().map(|s| &s.kind), Some(StmtKind::Expr(_)))
    }

    /// Lowers a for loop body with special handling for update expression.
    /// Creates: loop_block -> if(cond) { body -> update_block -> loop_block } else { exit }
    fn lower_for_loop_body(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        block: &hir::Block<'_>,
        update_block: crate::mir::BlockId,
        loop_block: crate::mir::BlockId,
    ) {
        let stmts = block.stmts;

        // Extract the if statement
        let StmtKind::If(cond, then_stmt, else_stmt) = &stmts[0].kind else {
            self.lower_block(builder, block);
            return;
        };

        let StmtKind::Block(then_body) = &then_stmt.kind else {
            self.lower_block(builder, block);
            return;
        };

        // Create blocks for the if
        let then_block = builder.create_block();
        let else_block = builder.create_block();

        let cond_val = self.lower_value_expr(builder, cond);
        builder.branch(cond_val, then_block, else_block);

        // Then branch: lower all statements except the last (update)
        builder.switch_to_block(then_block);
        let body_stmts = &then_body.stmts[..then_body.stmts.len() - 1];
        for stmt in body_stmts {
            self.lower_stmt(builder, stmt);
            if builder.func().block(builder.current_block()).is_terminated() {
                break;
            }
        }
        if !builder.func().block(builder.current_block()).is_terminated() {
            builder.jump(update_block);
        }

        // Update block: lower the update expression, then jump to loop
        builder.switch_to_block(update_block);
        if let Some(last_stmt) = then_body.stmts.last() {
            self.lower_stmt(builder, last_stmt);
        }
        if !builder.func().block(builder.current_block()).is_terminated() {
            builder.jump(loop_block);
        }

        // Else branch: should be break
        builder.switch_to_block(else_block);
        if let Some(else_s) = else_stmt {
            self.lower_stmt(builder, else_s);
        }
        // Note: else branch with break will be terminated, no need for explicit jump
    }

    /// Lowers a return statement.
    fn lower_return(&mut self, builder: &mut FunctionBuilder<'_>, value: Option<&hir::Expr<'_>>) {
        if let Some(expr) = value
            && self.get_expr_type(expr).is_some_and(|ty| ty.is_unit())
        {
            let _ = self.lower_expr(builder, expr);
            if !builder.func().block(builder.current_block()).is_terminated() {
                if let Some(ctx) = &self.inline_returns {
                    builder.jump(ctx.exit_block);
                } else if builder.func().is_public() && !self.lowering_internal_function {
                    builder.stop();
                } else {
                    builder.ret([]);
                }
            }
            return;
        }

        // A `return` inside a body being inlined delivers its values to the
        // call site: store them into the callee's return-variable slots and
        // jump to the inline exit block. This must precede the external check —
        // the inlined body may live inside a public function's lowering.
        if let Some(ctx) = self.inline_returns.clone() {
            self.lower_inline_return(builder, &ctx, value);
            return;
        }
        let external = builder.func().is_public() && !self.lowering_internal_function;
        if external {
            let items = self.gather_return_items(builder, value);
            if items.is_empty() {
                builder.stop();
            } else {
                self.finish_return(builder, items);
            }
            return;
        }
        if let Some(expr) = value {
            let items = self.gather_return_items(builder, Some(expr));
            self.finish_return(builder, items);
        } else {
            builder.ret([]);
        }
    }

    /// Delivers an inlined body's `return` values to its call site: each value
    /// is stored into the matching return variable's slot (two words for a
    /// calldata slice, one otherwise) and control jumps to the inline exit
    /// block, where [`super::Lowerer::inline_slice_return_body`] reads the
    /// slots back.
    fn lower_inline_return(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        ctx: &crate::lower::InlineReturnCtx,
        value: Option<&hir::Expr<'_>>,
    ) {
        let n = ctx.return_vars.len();
        let mut values = Vec::with_capacity(n);
        if let Some(expr) = value {
            if let hir::ExprKind::Tuple(elements) = &expr.kind {
                for elem in elements.iter().flatten() {
                    values.push(self.lower_value_expr(builder, elem));
                }
            } else {
                self.pending_inline_returns = None;
                let first = self.lower_value_expr(builder, expr);
                if n > 1 {
                    // A forwarded multi-return call: an inlined slice-returning
                    // callee leaves its values pending; anything else staged
                    // the tail in the multi-return buffer.
                    if let Some(pending) = self.pending_inline_returns.take() {
                        values = pending;
                    } else {
                        values.push(first);
                        let base = self.multi_return_buffer_base(builder);
                        for i in 1..n {
                            values.push(self.load_multi_return_value(builder, base, i));
                        }
                    }
                } else {
                    values.push(first);
                }
            }
        }
        for (i, &ret_id) in ctx.return_vars.iter().enumerate() {
            let Some(&val) = values.get(i) else { continue };
            if let Some(offset) = self.get_local_memory_offset(&ret_id) {
                if self.is_slice_slot_local(&ret_id) {
                    self.store_slice_slot(builder, offset, val);
                } else {
                    let addr = self.local_memory_addr(builder, offset);
                    builder.mstore(addr, val);
                }
            } else {
                self.locals.insert(ret_id, val);
            }
        }
        builder.jump(ctx.exit_block);
    }

    /// Gets the tuple arity if this is a ternary expression with tuple branches.
    pub(super) fn get_ternary_tuple_arity(&self, expr: &hir::Expr<'_>) -> Option<usize> {
        if let hir::ExprKind::Ternary(_, then_expr, else_expr) = &expr.kind {
            // Check if either branch is a tuple
            if let hir::ExprKind::Tuple(elements) = &then_expr.kind
                && elements.len() > 1
            {
                return Some(elements.len());
            }
            if let hir::ExprKind::Tuple(elements) = &else_expr.kind
                && elements.len() > 1
            {
                return Some(elements.len());
            }
        }
        None
    }

    /// Lowers an emit statement.
    fn lower_emit(&mut self, builder: &mut FunctionBuilder<'_>, expr: &hir::Expr<'_>) {
        // expr is always a Call expression: EventName(args)
        let hir::ExprKind::Call(callee, args, _named) = &expr.kind else {
            return;
        };

        // Get the event from the callee, using the overload target selected by
        // the type checker: `emit E(...)` may name an overloaded event.
        let Some(hir::Res::Item(hir::ItemId::Event(event_id))) = self.gcx.resolved_expr(callee)
        else {
            return;
        };

        let event = self.gcx.hir.event(event_id);
        let max_indexed = if event.anonymous { 4 } else { 3 };
        let indexed = event
            .parameters
            .iter()
            .filter(|&&param_id| self.gcx.hir.variable(param_id).indexed)
            .count();
        if indexed > max_indexed {
            if self.invalid_event_topics.insert(event_id) {
                self.gcx
                    .dcx()
                    .err(format!("event cannot have more than {max_indexed} indexed parameters"))
                    .span(event.span)
                    .emit();
            }
            return;
        }

        let arg_exprs =
            match self.ordered_args_for(args, Some(CallableParamSource::Event(event_id))) {
                Ok(exprs) => exprs,
                Err(_) => return,
            };

        // Collect indexed parameters (additional topics) and non-indexed (data).
        let mut topics = SmallVec::<[ValueId; 4]>::new();
        if !event.anonymous {
            let selector = self.gcx.event_selector(event_id);
            topics.push(builder.imm_u256(U256::from_be_bytes(selector.0)));
        }
        let mut data_items = Vec::new();

        let mut arg_exprs = arg_exprs.into_iter();
        for param_id in event.parameters {
            let param = self.gcx.hir.variable(*param_id);
            let Some(arg) = arg_exprs.next() else { continue };

            let ty = self.gcx.type_of_item((*param_id).into());

            if param.indexed {
                if matches!(
                    ty.peel_refs().kind,
                    TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
                ) {
                    // An indexed dynamic `bytes`/`string` is topic'd by the
                    // keccak256 of its contents, not by its (pointer) value.
                    let topic = self.keccak_dynamic_bytes(builder, arg).unwrap_or_else(|| {
                        self.err_value(
                            builder,
                            arg.span,
                            "codegen expected dynamic event bytes to have a byte representation",
                        )
                    });
                    topics.push(topic);
                } else if matches!(
                    ty.peel_refs().kind,
                    TyKind::Struct(_)
                        | TyKind::Array(..)
                        | TyKind::DynArray(_)
                        | TyKind::Slice(_)
                        | TyKind::Tuple(_)
                ) {
                    topics.push(self.err_value(
                        builder,
                        arg.span,
                        "codegen does not support indexed event aggregate encoding yet",
                    ));
                } else {
                    topics.push(self.lower_return_value_for_ty(builder, arg, ty));
                }
            } else {
                let arg_val = self.lower_return_value_for_ty(builder, arg, ty);
                data_items.push((arg_val, ty));
            }
        }

        let (mem_offset, size) = self.abi_encode_event_data(builder, &data_items);

        // Emit the appropriate LOG instruction based on number of topics
        match topics.as_slice() {
            [] => builder.log0(mem_offset, size),
            &[a] => builder.log1(mem_offset, size, a),
            &[a, b] => builder.log2(mem_offset, size, a, b),
            &[a, b, c] => builder.log3(mem_offset, size, a, b, c),
            &[a, b, c, d] => builder.log4(mem_offset, size, a, b, c, d),
            _ => {
                self.recovery_error(Some(args.span), "codegen cannot emit more than four topics");
            }
        }
    }

    /// Lowers a try/catch statement.
    ///
    /// try expr returns (...) { success_block } catch (...) { catch_block }
    ///
    /// EVM semantics:
    /// 1. Execute the call (expr must be an external call)
    /// 2. CALL returns 1 for success, 0 for failure
    /// 3. If success (1), jump to success block
    /// 4. If failure (0), jump to catch block
    fn lower_try(&mut self, builder: &mut FunctionBuilder<'_>, try_stmt: &hir::StmtTry<'_>) {
        let success_block = builder.create_block();
        let catch_block = builder.create_block();
        let merge_block = builder.create_block();

        // Lower the call expression and get the success flag.
        // We need to handle the call specially to get the success flag, not the return value.
        let success = self.lower_try_call(builder, &try_stmt.expr);

        // Branch: if success (non-zero), go to success_block, else catch_block
        builder.branch(success, success_block, catch_block);

        // Success block (the `returns` clause is always first): decode the
        // call's returndata into the bound variables, then run the block.
        builder.switch_to_block(success_block);
        if let Some(returns_clause) = try_stmt.clauses.first() {
            if !returns_clause.args.is_empty() {
                self.bind_try_returns(builder, returns_clause.args, try_stmt.expr.span);
            }
            self.lower_block(builder, &returns_clause.block);
        }
        if !builder.func().block(builder.current_block()).is_terminated() {
            builder.jump(merge_block);
        }

        // Catch clauses: dispatch on the revert data's selector. `Error` and
        // `Panic` handlers match their selectors; a low-level `catch (bytes)`
        // or bare `catch` takes everything else; with no applicable handler
        // the revert data is rethrown.
        builder.switch_to_block(catch_block);
        self.lower_catch_clauses(builder, &try_stmt.clauses[1..], merge_block, try_stmt.expr.span);

        // Continue after try/catch
        builder.switch_to_block(merge_block);
    }

    /// Decodes the successful call's returndata into the `returns` clause
    /// bindings. Like solc, malformed returndata reverts rather than reaching
    /// any catch clause: the external call itself succeeded.
    fn bind_try_returns(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        vars: &[hir::VariableId],
        span: Span,
    ) {
        // Validate every binding is decodable before emitting; report the
        // first that is not.
        let mut tys = Vec::with_capacity(vars.len());
        for &var_id in vars {
            let var = self.gcx.hir.variable(var_id);
            let ty = self.gcx.type_of_hir_ty(&var.ty);
            if self.abi_decode_strategy(ty).is_none() {
                self.gcx
                    .dcx()
                    .err("codegen does not support this try return type yet")
                    .span(var.span)
                    .emit();
                return;
            }
            tys.push(ty);
        }

        let slice = self.returndata_slice(builder);
        let ptr = self.materialize_returndata_slice(builder, slice);
        let len = builder.memory_object_len(ptr, MemoryObjectKind::Bytes);
        let data_start = builder.memory_object_data(ptr, MemoryObjectKind::Bytes);

        let decoded = self.decode_abi_region(builder, data_start, len, &tys);
        for (&var_id, value) in vars.iter().zip(decoded) {
            self.bind_local_value(builder, var_id, value);
        }
        let _ = span;
    }

    fn lower_catch_clauses(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        clauses: &[hir::TryCatchClause<'_>],
        merge_block: crate::mir::BlockId,
        span: Span,
    ) {
        let mut error_clause = None;
        let mut panic_clause = None;
        let mut fallback_clause = None;
        for clause in clauses {
            match clause.name {
                Some(name) if name.name == sym::Error => error_clause = Some(clause),
                Some(name) if name.name == sym::Panic => panic_clause = Some(clause),
                Some(name) => {
                    self.gcx
                        .dcx()
                        .err(format!("unknown try/catch handler `{}`", name.name))
                        .span(clause.span)
                        .emit();
                }
                None => fallback_clause = Some(clause),
            }
        }

        // Rethrow: forward the revert data unchanged.
        let rethrow_block = builder.create_block();

        // Selector of the revert data; too-short data reads as selector zero,
        // which no handler matches. The copy size degrades to zero so the
        // returndata access stays in bounds.
        let rds = builder.returndatasize();
        let four = builder.imm_u64(4);
        let zero = builder.imm_u64(0);
        let too_short = builder.lt(rds, four);
        let has_selector = builder.iszero(too_short);
        builder.mstore(zero, zero);
        let copy_size = builder.select(has_selector, four, zero);
        builder.returndatacopy(zero, zero, copy_size);
        let word = builder.mload(zero);
        let shift = builder.imm_u64(224);
        let selector = builder.shr(shift, word);

        let fallback_target = builder.create_block();

        // `catch Error(string memory reason)`.
        if let Some(clause) = error_clause {
            let error_selector = builder.imm_u64(0x08c379a0);
            let matches = builder.eq(selector, error_selector);
            let matches = builder.and(matches, has_selector);
            let body = builder.create_block();
            let next = builder.create_block();
            builder.branch(matches, body, next);

            builder.switch_to_block(body);
            let slice = self.returndata_slice(builder);
            let ptr = self.materialize_returndata_slice(builder, slice);
            let len = builder.memory_object_len(ptr, MemoryObjectKind::Bytes);
            let data = builder.memory_object_data(ptr, MemoryObjectKind::Bytes);
            let region_base = builder.add(data, four);
            let region_len = builder.sub(len, four);
            let head = builder.mload(region_base);
            let word = builder.imm_u64(32);
            // Malformed `Error(string)` data reverts; solc instead falls
            // through to a lower-level handler, which only differs for
            // hostile callees that revert with a mangled Error selector.
            let reason =
                self.lower_abi_decode_dynamic_bytes(builder, region_base, region_len, word, head);
            if let [var_id] = clause.args {
                self.bind_local_value(builder, *var_id, reason);
            }
            self.lower_block(builder, &clause.block);
            if !builder.func().block(builder.current_block()).is_terminated() {
                builder.jump(merge_block);
            }

            builder.switch_to_block(next);
        }

        // `catch Panic(uint256 code)`.
        if let Some(clause) = panic_clause {
            let panic_selector = builder.imm_u64(0x4e487b71);
            let matches = builder.eq(selector, panic_selector);
            let matches = builder.and(matches, has_selector);
            let thirty_six = builder.imm_u64(36);
            let panic_short = builder.lt(rds, thirty_six);
            let long_enough = builder.iszero(panic_short);
            let matches = builder.and(matches, long_enough);
            let body = builder.create_block();
            let next = builder.create_block();
            builder.branch(matches, body, next);

            builder.switch_to_block(body);
            let zero = builder.imm_u64(0);
            let word = builder.imm_u64(32);
            builder.returndatacopy(zero, four, word);
            let code = builder.mload(zero);
            if let [var_id] = clause.args {
                self.bind_local_value(builder, *var_id, code);
            }
            self.lower_block(builder, &clause.block);
            if !builder.func().block(builder.current_block()).is_terminated() {
                builder.jump(merge_block);
            }

            builder.switch_to_block(next);
        }
        builder.jump(fallback_target);

        // Low-level `catch (bytes memory data)` or bare `catch`; otherwise
        // rethrow.
        builder.switch_to_block(fallback_target);
        if let Some(clause) = fallback_clause {
            if let [var_id] = clause.args {
                let slice = self.returndata_slice(builder);
                let ptr = self.materialize_returndata_slice(builder, slice);
                self.bind_local_value(builder, *var_id, ptr);
            }
            self.lower_block(builder, &clause.block);
            if !builder.func().block(builder.current_block()).is_terminated() {
                builder.jump(merge_block);
            }
        } else {
            builder.jump(rethrow_block);
        }

        builder.switch_to_block(rethrow_block);
        let zero = builder.imm_u64(0);
        let rds = builder.returndatasize();
        builder.returndatacopy(zero, zero, rds);
        builder.revert(zero, rds);
        let _ = span;
    }

    /// Lowers a call expression for try/catch, returning the success flag.
    /// This is different from lower_expr which returns the return value.
    fn lower_try_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        expr: &hir::Expr<'_>,
    ) -> crate::mir::ValueId {
        use hir::ExprKind;

        // The try expression should be a call
        if let ExprKind::Call(callee, args, call_opts) = &expr.kind {
            // Check if this is a member access (external call)
            if let ExprKind::Member(base, member) = &callee.kind {
                return self.lower_try_member_call(
                    builder,
                    callee,
                    base,
                    *member,
                    args,
                    (*call_opts).map(|opts| opts.args),
                );
            }
            if let Some(TyKind::Fn(function)) = self.get_expr_type(callee).map(|ty| ty.kind)
                && function.is_external()
                && function.function_id.is_none()
                && self.gcx.resolved_function(callee).is_none()
            {
                return self
                    .emit_external_function_pointer_call(
                        builder,
                        callee,
                        args,
                        (*call_opts).map(|opts| opts.args),
                        function,
                    )
                    .0;
            }
        }

        let guar = self
            .gcx
            .dcx()
            .err("codegen does not support this try expression yet")
            .span(expr.span)
            .emit();
        builder.error_value(guar)
    }

    /// Lowers a member call for try/catch, returning the CALL success flag.
    fn lower_try_member_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        callee: &hir::Expr<'_>,
        base: &hir::Expr<'_>,
        member: solar_interface::Ident,
        args: &hir::CallArgs<'_>,
        call_opts: Option<&[hir::NamedArg<'_>]>,
    ) -> crate::mir::ValueId {
        let resolved_func = self.resolved_function_callee(callee);
        let selector = resolved_func.map_or_else(
            || self.compute_member_selector(base, member),
            |func_id| u32::from_be_bytes(self.gcx.function_selector(func_id).0),
        );
        let arg_exprs = match self.ordered_call_args(callee, args) {
            Ok(exprs) => exprs,
            Err(guar) => return builder.error_value(guar),
        };
        let selector = builder.imm_u256(U256::from(selector) << 224);
        let (args_offset, args_size) =
            match self.abi_encode_call_payload(builder, Some(selector), arg_exprs.into_iter()) {
                Ok(payload) => payload,
                Err(guar) => return builder.error_value(guar),
            };
        let addr = self.lower_value_expr(builder, base);
        let ret_offset = builder.imm_u64(0);
        let ret_size = builder.imm_u64(0);
        let kind = self.external_function_call_kind(resolved_func);
        let (gas, value) =
            self.lower_external_call_options(builder, call_opts, kind.accepts_value());

        self.emit_external_call(
            builder,
            kind,
            gas,
            addr,
            value,
            args_offset,
            args_size,
            ret_offset,
            ret_size,
        )
    }
}
