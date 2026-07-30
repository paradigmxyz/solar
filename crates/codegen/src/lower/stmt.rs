//! Statement lowering.

use super::{LoopContext, Lowerer, checked_arith::PanicCode};
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
    ty::{Ty, TyKind},
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

        let mut exprs = args.exprs();
        let Some(arg) = exprs.next() else {
            return false;
        };
        exprs.next().is_none() && self.resolves_to_variable(arg, var_id)
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
            .collect_builtin_args(Builtin::AbiEncodePacked, args)
            .and_then(|_| self.lower_keccak_abi_encode_packed(builder, args))
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
        if let hir::ExprKind::Tuple(elements) = &init.peel_parens().kind {
            let Ok(values) = self.lower_tuple_values(builder, elements, init.span) else { return };
            if values.len() != var_ids.len() {
                self.gcx
                    .dcx()
                    .err("tuple declaration arity mismatch in codegen")
                    .span(init.span)
                    .emit();
                return;
            }

            for (&var_id, value) in var_ids.iter().zip(values) {
                if let Some(var_id) = var_id {
                    self.bind_local_value(builder, var_id, value);
                }
            }
            return;
        }

        if self.is_low_level_call_expr(init) {
            // `(bool success, bytes memory data) = addr.call(...)`: the call
            // lowering returns the success flag, and the full returndata is
            // copied into a fresh `bytes memory` allocation right after the
            // call (nothing can clobber the return buffer in between).
            let success = self.lower_value_expr(builder, init);
            for (i, var_id_opt) in var_ids.iter().enumerate() {
                let Some(var_id) = var_id_opt else { continue };
                let val = if i == 0 { success } else { self.materialize_returndata_bytes(builder) };
                let offset = self.alloc_local_memory(*var_id);
                let offset_val = self.local_memory_addr(builder, offset);
                builder.mstore(offset_val, val);
            }
            return;
        }

        // Snapshot every bound tail value before storing any local. This keeps
        // the unbumped return buffer independent of subsequent memory writes.
        let init_delivers_pending = self.is_slice_multi_return_call(init);
        self.pending_inline_returns = None;
        let first_val = self.lower_value_expr(builder, init);
        // An inlined multi-return callee with calldata-slice returns delivers
        // its values directly — a slice cannot ride the one-word-per-value
        // buffer — so bind them here instead of reading the buffer.
        if init_delivers_pending && let Some(values) = self.pending_inline_returns.take() {
            for (i, var_id_opt) in var_ids.iter().enumerate() {
                if let Some(var_id) = var_id_opt {
                    let val = values.get(i).copied().unwrap_or(first_val);
                    self.bind_local_value(builder, *var_id, val);
                }
            }
            return;
        }
        self.pending_inline_returns = None;
        let tail_base = var_ids.iter().skip(1).any(Option::is_some).then(|| {
            let ptr_slot = builder.imm_u64(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT);
            builder.mload(ptr_slot)
        });
        let vals: Vec<Option<ValueId>> = var_ids
            .iter()
            .enumerate()
            .map(|(i, var_id)| {
                var_id.map(|_| {
                    if i == 0 {
                        first_val
                    } else {
                        let offset = builder.imm_u64(i as u64 * 32);
                        let addr = builder.add(tail_base.expect("tail base is available"), offset);
                        builder.mload(addr)
                    }
                })
            })
            .collect();

        for (var_id_opt, val) in var_ids.iter().zip(vals) {
            if let Some(var_id) = var_id_opt {
                // Allocate memory slot and store value
                let offset = self.alloc_local_memory(*var_id);
                let offset_val = self.local_memory_addr(builder, offset);
                builder.mstore(offset_val, val.expect("bound variable has a value"));
                if self.gcx.hir.variable(*var_id).data_location
                    == Some(solar_ast::DataLocation::Storage)
                {
                    self.storage_ref_locals.insert(*var_id);
                }
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
        let source_tys = if let hir::ExprKind::Tuple(rhs_elems) = &rhs.peel_parens().kind {
            rhs_elems
                .iter()
                .map(|element| element.and_then(|element| self.get_expr_type(element)))
                .collect::<Vec<_>>()
        } else if let Some(TyKind::Tuple(tys)) = self.get_expr_type(rhs).map(|ty| ty.kind) {
            tys.iter().copied().map(Some).collect()
        } else {
            vec![self.get_expr_type(rhs)]
        };

        // Tuple RHS, `(a, b) = (x, y)` (including swaps `(a, b) = (b, a)`):
        // evaluate every RHS element before assigning any, so a swap reads the
        // old values.
        if let hir::ExprKind::Tuple(rhs_elems) = &rhs.peel_parens().kind {
            let Ok(values) = self.lower_tuple_values(builder, rhs_elems, rhs.span) else { return };
            if values.len() != elements.len() {
                self.gcx
                    .dcx()
                    .err("tuple assignment arity mismatch in codegen")
                    .span(rhs.span)
                    .emit();
                return;
            }
            let values = values.into_iter().map(Some).collect::<Vec<_>>();
            self.assign_tuple_values(builder, elements, &values, &source_tys);
            return;
        }

        if self.is_low_level_call_expr(rhs) {
            // `(ok, data) = addr.call(...)`: the call lowering yields the success
            // flag. Snapshot returndata before evaluating either lvalue because
            // an effectful destination can make another external call.
            let success = self.lower_value_expr(builder, rhs);
            let returndata = elements
                .iter()
                .skip(1)
                .any(Option::is_some)
                .then(|| self.materialize_returndata_bytes(builder));
            let values = elements
                .iter()
                .enumerate()
                .map(|(i, element)| {
                    element.map(|_| {
                        if i == 0 {
                            success
                        } else {
                            returndata.expect("returndata value is available")
                        }
                    })
                })
                .collect::<Vec<_>>();
            self.assign_tuple_values(builder, elements, &values, &source_tys);
            return;
        }

        // Snapshot every tail value before assigning the first lvalue. Mapping
        // and other complex lvalues may use scratch memory while computing
        // their destination and must not corrupt later tuple elements.
        let rhs_delivers_pending = self.is_slice_multi_return_call(rhs);
        self.pending_inline_returns = None;
        let first_val = self.lower_value_expr(builder, rhs);
        // An inlined multi-return callee with calldata-slice returns delivers
        // its values directly; assign them through the regular lvalue path,
        // which routes slice-slot locals through their two-word slots.
        if rhs_delivers_pending && let Some(values) = self.pending_inline_returns.take() {
            let values = elements
                .iter()
                .enumerate()
                .map(|(i, element)| element.map(|_| values.get(i).copied().unwrap_or(first_val)))
                .collect::<Vec<_>>();
            self.assign_tuple_values(builder, elements, &values, &source_tys);
            return;
        }
        self.pending_inline_returns = None;
        let tail_base = elements.iter().skip(1).any(Option::is_some).then(|| {
            let ptr_slot = builder.imm_u64(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT);
            builder.mload(ptr_slot)
        });
        let vals: Vec<Option<ValueId>> = elements
            .iter()
            .enumerate()
            .map(|(i, elem)| {
                elem.map(|_| {
                    if i == 0 {
                        first_val
                    } else {
                        let offset = builder.imm_u64(i as u64 * 32);
                        let addr = builder.add(tail_base.expect("tail base is available"), offset);
                        builder.mload(addr)
                    }
                })
            })
            .collect();
        self.assign_tuple_values(builder, elements, &vals, &source_tys);
    }

    /// Evaluates tuple lvalue locations left-to-right, then stores right-to-left.
    fn assign_tuple_values(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        elements: &[Option<&hir::Expr<'_>>],
        values: &[Option<ValueId>],
        source_tys: &[Option<Ty<'gcx>>],
    ) {
        let locations = elements
            .iter()
            .map(|element| element.map(|element| self.lower_tuple_lvalue(builder, element)))
            .collect::<Vec<_>>();
        for (index, location) in locations.into_iter().enumerate().rev() {
            let Some(location) = location else { continue };
            let value =
                values.get(index).copied().flatten().expect("tuple destination has a value");
            self.store_tuple_lvalue(
                builder,
                location,
                value,
                source_tys.get(index).copied().flatten(),
            );
        }
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
        if let ExprKind::Call(callee, ..) = &expr.kind
            && let ExprKind::Member(base, member) = &callee.kind
        {
            matches!(member.name, kw::Call | kw::Staticcall | kw::Delegatecall)
                && !self.is_contract_type_expr(base)
        } else {
            false
        }
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
                for (&ret_id, elem) in ctx.return_vars.iter().zip(elements.iter().flatten()) {
                    values.push(self.lower_inline_return_value(builder, ret_id, elem));
                }
            } else if n == 1 {
                values.push(self.lower_inline_return_value(builder, ctx.return_vars[0], expr));
            } else {
                self.pending_inline_returns = None;
                let first = self.lower_value_expr(builder, expr);
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

    fn lower_inline_return_value(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        ret_id: hir::VariableId,
        expr: &hir::Expr<'_>,
    ) -> ValueId {
        if Self::calldata_dynamic_var_kind(self.gcx.hir.variable(ret_id)).is_some() {
            self.lower_value_expr(builder, expr)
        } else {
            let ty = self.gcx.type_of_item(ret_id.into());
            self.lower_return_value_for_ty(builder, expr, ty)
        }
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

        // Compute event signature hash (topic0 for non-anonymous events)
        let sig = self.compute_event_signature(event);
        let sig_hash = alloy_primitives::keccak256(sig.as_bytes());
        let topic0 = builder.imm_u256(alloy_primitives::U256::from_be_bytes(sig_hash.0));

        // Collect indexed parameters (additional topics) and non-indexed (data).
        let mut topics = vec![topic0];
        let mut data_items = Vec::new();

        let mut arg_exprs = args.exprs();
        for param_id in event.parameters {
            let param = self.gcx.hir.variable(*param_id);
            let Some(arg) = arg_exprs.next() else { continue };

            let ty = self.gcx.type_of_item((*param_id).into());

            if param.indexed {
                // An indexed dynamic `bytes`/`string` is topic'd by the
                // keccak256 of its contents, not by its (pointer) value.
                if let Some(topic) = self.keccak_dynamic_bytes(builder, arg) {
                    topics.push(topic);
                } else {
                    let arg_val = self.lower_return_value_for_ty(builder, arg, ty);
                    topics.push(arg_val);
                }
            } else {
                let arg_val = self.lower_return_value_for_ty(builder, arg, ty);
                data_items.push((arg_val, ty));
            }
        }

        // ABI-encode non-indexed data to memory
        let has_dynamic_data = data_items.iter().any(|&(_, ty)| self.abi_is_dynamic(ty));
        let (mem_offset, size) = if has_dynamic_data {
            self.abi_encode_items_to_memory(builder, &data_items)
        } else {
            let mem_offset = builder.imm_u64(0);
            for (i, (val, _)) in data_items.iter().enumerate() {
                let offset = builder.imm_u64(i as u64 * 32);
                builder.mstore(offset, *val);
            }
            let size = builder.imm_u64((data_items.len() * 32) as u64);
            (mem_offset, size)
        };

        // Emit the appropriate LOG instruction based on number of topics
        match topics.len() {
            0 => builder.log0(mem_offset, size),
            1 => builder.log1(mem_offset, size, topics[0]),
            2 => builder.log2(mem_offset, size, topics[0], topics[1]),
            3 => builder.log3(mem_offset, size, topics[0], topics[1], topics[2]),
            4 => builder.log4(mem_offset, size, topics[0], topics[1], topics[2], topics[3]),
            _ => {} // More than 4 topics not supported by EVM
        }
    }

    /// Computes the event signature string: "EventName(type1,type2,...)"
    fn compute_event_signature(&self, event: &hir::Event<'_>) -> String {
        let params: Vec<String> = event
            .parameters
            .iter()
            .map(|param_id| {
                let param = self.gcx.hir.variable(*param_id);
                self.type_to_abi_string(&param.ty)
            })
            .collect();
        format!("{}({})", event.name.name, params.join(","))
    }

    /// Converts a HIR type to its ABI string representation
    fn type_to_abi_string(&self, ty: &hir::Type<'_>) -> String {
        match &ty.kind {
            hir::TypeKind::Elementary(elem) => elem.to_abi_str().to_string(),
            hir::TypeKind::Custom(item_id) => {
                // For contracts, use "address"
                if let hir::ItemId::Contract(_) = item_id {
                    "address".to_string()
                } else {
                    "uint256".to_string() // Fallback
                }
            }
            hir::TypeKind::Array(arr) => {
                let inner = self.type_to_abi_string(&arr.element);
                format!("{inner}[]")
            }
            _ => "uint256".to_string(), // Fallback for other types
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
        let (success, created_contract) = self.lower_try_call(builder, &try_stmt.expr);

        // Branch: if success (non-zero), go to success_block, else catch_block
        builder.branch(success, success_block, catch_block);

        // Success block (the `returns` clause is always first): decode the
        // call's returndata into the bound variables, then run the block.
        builder.switch_to_block(success_block);
        if let Some(returns_clause) = try_stmt.clauses.first() {
            if let (Some(created), [var_id]) = (created_contract, returns_clause.args) {
                self.bind_local_value(builder, *var_id, created);
            } else if !returns_clause.args.is_empty() {
                self.bind_try_returns(builder, returns_clause.args, try_stmt.expr.span);
            } else if created_contract.is_none()
                && let Some(return_tys) = self.try_call_return_types(&try_stmt.expr)
                && !return_tys.is_empty()
            {
                self.decode_try_returns(builder, return_tys, try_stmt.expr.span);
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

        // Continue after try/catch.
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
        let mut tys = Vec::with_capacity(vars.len());
        for &var_id in vars {
            let var = self.gcx.hir.variable(var_id);
            let ty = self.gcx.type_of_hir_ty(&var.ty);
            tys.push(ty);
        }

        let Some(decoded) = self.decode_try_returns(builder, &tys, span) else { return };
        for (&var_id, value) in vars.iter().zip(decoded) {
            self.bind_local_value(builder, var_id, value);
        }
    }

    /// Returns the declared result types of the call in a try expression.
    fn try_call_return_types(&self, expr: &hir::Expr<'_>) -> Option<&'gcx [Ty<'gcx>]> {
        let ExprKind::Call(callee, ..) = &expr.peel_parens().kind else { return None };
        let callee_ty = self.get_expr_type(callee)?;
        Some(self.gcx.callable_signature_of_ty(callee_ty)?.returns)
    }

    /// Decodes successful try-call returndata. This is required even when the
    /// success clause omits return bindings because malformed returndata from
    /// a successful external call reverts outside the catch clauses.
    fn decode_try_returns(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        tys: &[Ty<'gcx>],
        span: Span,
    ) -> Option<Vec<ValueId>> {
        if tys.iter().any(|&ty| self.abi_decode_strategy(ty).is_none()) {
            self.gcx
                .dcx()
                .err("codegen does not support this try return type yet")
                .span(span)
                .emit();
            return None;
        }

        let slice = self.returndata_slice(builder);
        let ptr = self.materialize_returndata_slice(builder, slice);
        let len = builder.memory_object_len(ptr, MemoryObjectKind::Bytes);
        let data_start = builder.memory_object_data(ptr, MemoryObjectKind::Bytes);

        Some(self.decode_abi_region(builder, data_start, len, tys))
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
            let thirty_six = builder.imm_u64(36);
            let too_short = builder.lt(rds, thirty_six);
            let has_head = builder.iszero(too_short);
            let matches = builder.and(matches, has_head);
            let validate_tail = builder.create_block();
            let body = builder.create_block();
            let next = builder.create_block();
            builder.branch(matches, validate_tail, next);

            builder.switch_to_block(validate_tail);
            let word = builder.imm_u64(32);
            builder.returndatacopy(zero, four, word);
            let head = builder.mload(zero);
            let region_len = builder.sub(rds, four);
            let head_before_tail = builder.lt(head, word);
            let tail_head_end = builder.add(head, word);
            let head_overflow = builder.lt(tail_head_end, head);
            let head_oob = builder.gt(tail_head_end, region_len);
            let invalid_head = builder.or(head_before_tail, head_overflow);
            let invalid_head = builder.or(invalid_head, head_oob);
            let valid_head = builder.iszero(invalid_head);
            let validate_payload = builder.create_block();
            builder.branch(valid_head, validate_payload, next);

            builder.switch_to_block(validate_payload);
            let tail_len_pos = builder.add(four, head);
            builder.returndatacopy(zero, tail_len_pos, word);
            let tail_len = builder.mload(zero);
            let thirty_one = builder.imm_u64(31);
            let rounded = builder.add(tail_len, thirty_one);
            let rounded_overflow = builder.lt(rounded, tail_len);
            let mask = builder.not(thirty_one);
            let padded = builder.and(rounded, mask);
            // Error(string) revert data is accepted without ABI tail padding,
            // matching solc's try/catch decoder.
            let tail_end = builder.add(tail_head_end, tail_len);
            let tail_overflow = builder.lt(tail_end, tail_head_end);
            let tail_oob = builder.gt(tail_end, region_len);
            let invalid_tail = builder.or(rounded_overflow, tail_overflow);
            let invalid_tail = builder.or(invalid_tail, tail_oob);
            let valid_tail = builder.iszero(invalid_tail);
            builder.branch(valid_tail, body, next);

            builder.switch_to_block(body);
            let is_empty = builder.iszero(padded);
            let data_size = builder.select(is_empty, word, padded);
            let total_size = builder.add(word, data_size);
            let total_overflow = builder.lt(total_size, data_size);
            self.emit_panic_if(builder, total_overflow, PanicCode::MemoryAllocationOverflow);
            let reason =
                self.allocate_memory_object_dynamic(builder, total_size, MemoryObjectKind::Bytes);
            builder.set_memory_object_len(reason, tail_len, MemoryObjectKind::Bytes);
            let data = builder.memory_object_data(reason, MemoryObjectKind::Bytes);
            let last_word_offset = builder.sub(data_size, word);
            let last_word = builder.add(data, last_word_offset);
            builder.mstore(last_word, zero);
            let tail_data_pos = builder.add(tail_len_pos, word);
            builder.returndatacopy(data, tail_data_pos, tail_len);
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
    ) -> (crate::mir::ValueId, Option<crate::mir::ValueId>) {
        use hir::ExprKind;

        // The try expression should be a call
        if let ExprKind::Call(callee, args, call_opts) = &expr.kind {
            if let ExprKind::New(ty) = &callee.kind {
                let created =
                    self.lower_new_contract(builder, ty, args, (*call_opts).map(|opts| opts.args));
                let failed = builder.iszero(created);
                let success = builder.iszero(failed);
                return (success, Some(created));
            }
            if let Some(TyKind::Fn(function)) = self.get_expr_type(callee).map(|ty| ty.kind)
                && function.is_external()
                && function.function_id.is_none()
                && self.gcx.resolved_function(callee).is_none()
            {
                return (
                    self.emit_external_function_pointer_call(
                        builder,
                        callee,
                        args,
                        (*call_opts).map(|opts| opts.args),
                        function,
                    ),
                    None,
                );
            }
            // Check if this is a statically resolved member access.
            if let ExprKind::Member(base, member) = &callee.kind {
                return (
                    self.lower_try_member_call(
                        builder,
                        callee,
                        base,
                        *member,
                        args,
                        (*call_opts).map(|opts| opts.args),
                    ),
                    None,
                );
            }
        }

        // Fallback: lower as normal and use the result
        // This is incorrect but allows compilation to continue
        let result = self.lower_value_expr(builder, expr);
        let is_zero = builder.iszero(result);
        (builder.iszero(is_zero), None)
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
        let resolved_function = self.resolved_function_callee(callee);
        let (selector, returns_empty, state_mutability) =
            if let Some(function_id) = resolved_function {
                let function = self.gcx.hir.function(function_id);
                (
                    u32::from_be_bytes(self.gcx.function_selector(function_id).0),
                    function.returns.is_empty(),
                    function.state_mutability,
                )
            } else {
                (
                    self.compute_member_selector(base, member),
                    self.get_member_function_return_count(base, member) == 0,
                    hir::StateMutability::NonPayable,
                )
            };
        let selector_word = builder.imm_u256(U256::from(selector) << 224);
        let addr = self.lower_value_expr(builder, base);
        let (value, gas_override) = self.lower_call_options(builder, call_opts);

        let spill = self.allocate_memory(builder, 96);
        builder.mstore(spill, addr);
        let value_spill = self.offset_ptr(builder, spill, 32);
        builder.mstore(value_spill, value);
        let gas_spill = gas_override.map(|gas| {
            let gas_spill = self.offset_ptr(builder, spill, 64);
            builder.mstore(gas_spill, gas);
            gas_spill
        });

        let arg_exprs = args.exprs().collect::<Vec<_>>();
        let (calldata_start, calldata_size) =
            match self.abi_encode_call_payload(builder, Some(selector_word), &arg_exprs) {
                Ok(payload) => payload,
                Err(guar) => return builder.error_value(guar),
            };

        let addr = builder.mload(spill);
        let zero = builder.imm_u64(0);
        let value = builder.mload(value_spill);
        if returns_empty {
            let code_size = builder.extcodesize(addr);
            let no_code = builder.iszero(code_size);
            self.emit_abi_decode_revert_if(builder, no_code);
        }
        let gas = if let Some(slot) = gas_spill { builder.mload(slot) } else { builder.gas() };

        // The success and catch paths consume the complete return buffer with
        // RETURNDATA instructions, so the call needs no fixed output area.
        if matches!(state_mutability, hir::StateMutability::Pure | hir::StateMutability::View)
            && self.gcx.sess.opts.evm_version.has_static_call()
        {
            builder.staticcall(gas, addr, calldata_start, calldata_size, zero, zero)
        } else {
            builder.call(gas, addr, value, calldata_start, calldata_size, zero, zero)
        }
    }
}
