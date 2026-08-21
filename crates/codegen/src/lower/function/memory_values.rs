//! Memory-backed value construction and default aggregate values.

use super::*;

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    pub(super) fn lower_array(
        &mut self,
        expr: &hir::Expr<'_>,
        elements: &[hir::Expr<'_>],
    ) -> Option<ValueId> {
        let ty = self.context.gcx.type_of_expr(expr.id)?;
        let TyKind::Array(element_ty, _) = ty.peel_refs().kind else {
            return report_unsupported(self.context.gcx, expr.span, "array literal");
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
            _ => return report_unsupported(self.context.gcx, expr.span, "array literal"),
        };
        let size = self.builder.imm_u64(size);
        let object = self.builder.alloc_object(size, layout, AllocationSemantics::INTERNAL);
        if kind == MemoryObjectKind::DynamicArray {
            let length = self.builder.imm_u64(u64::try_from(elements.len()).ok()?);
            self.builder.set_memory_object_len(object, length, kind);
        }
        for (index, element) in elements.iter().enumerate() {
            let value = self.lower_expr(element)?;
            let value =
                self.coerce_value(value, self.context.gcx.type_of_expr(element.id)?, element_ty);
            let index = self.builder.imm_u64(index as u64);
            self.builder.memory_object_store_element(object, layout, index, value);
        }
        Some(object)
    }

    pub(super) fn materialize_array_element(
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

    pub(super) fn lower_struct_constructor(
        &mut self,
        expr: &hir::Expr<'_>,
        struct_id: hir::StructId,
        args: hir::CallArgs<'_>,
    ) -> Option<ValueId> {
        let struct_fields = self.context.gcx.hir.strukt(struct_id).fields;
        let fields = struct_fields.len() as u64;
        if args.len() != fields as usize {
            return report_unsupported(self.context.gcx, expr.span, "struct constructor arguments");
        }
        let parameter_names =
            self.context.gcx.callable_param_names(CallableParamSource::Struct(struct_id));
        let layout = MemoryObjectLayout::Struct { fields };
        let size = self.builder.imm_u64(fields.saturating_mul(32));
        let object = self.builder.alloc_object(size, layout, AllocationSemantics::INTERNAL);
        for (index, &field) in struct_fields.iter().enumerate() {
            let Some(argument) =
                args.argument_for_parameter(index, Some(parameter_names.as_slice()))
            else {
                return report_unsupported(
                    self.context.gcx,
                    args.span,
                    "struct constructor argument",
                );
            };
            let field_ty = self.context.gcx.type_of_item(field.into());
            let value = self.lower_typed_expr(argument, field_ty)?;
            let value = self.materialize_memory_argument(field_ty, value, argument.span)?;
            self.builder.memory_object_store_field(object, layout, index as u64, value);
        }
        Some(object)
    }

    pub(super) fn lower_tuple(
        &mut self,
        expr: &hir::Expr<'_>,
        values: &[Option<&hir::Expr<'_>>],
    ) -> Option<ValueId> {
        let ty = self.context.gcx.type_of_expr(expr.id)?;
        let MemoryObjectLayout::Struct { fields } = self.types.memory_layout(ty)? else {
            return report_unsupported(self.context.gcx, expr.span, "tuple object");
        };
        let size = fields.checked_mul(32)?;
        let size = self.builder.imm_u64(size);
        let initialization = if values.iter().all(Option::is_some) {
            AllocationSemantics::INTERNAL
        } else {
            AllocationSemantics::SOLIDITY_ZEROED
        };
        let object =
            self.builder.alloc_object(size, MemoryObjectLayout::Struct { fields }, initialization);
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

    pub(super) fn lower_bytes_literal(&mut self, bytes: &[u8], span: Span) -> Option<ValueId> {
        let value = if !bytes.is_empty()
            && bytes.len() <= 32
            && self.context.shared_word_literals.contains(bytes)
        {
            let helper = self.ensure_bytes_word_helper();
            let word = self.lower_string_literal_word(bytes);
            let length = self.builder.imm_u64(bytes.len() as u64);
            self.builder.internal_call(
                helper,
                vec![word, length],
                MirType::MemoryObject(MemoryObjectKind::Bytes),
                1,
            )
        } else if self.context.shared_literals.contains(bytes) {
            let helper = self.ensure_bytes_literal_helper(bytes);
            self.builder.internal_call(
                helper,
                Vec::new(),
                MirType::MemoryObject(MemoryObjectKind::Bytes),
                1,
            )
        } else {
            Self::build_bytes_literal(&mut self.builder, bytes, AllocationSemantics::INTERNAL)?
        };
        let _ = span;
        Some(value)
    }

    fn ensure_bytes_word_helper(&mut self) -> FunctionId {
        if let Some(id) = *self.context.literal_word_helper {
            return id;
        }
        let mut function = Function::new(Ident::from_str("__literal_bytes_word"));
        function.attributes.no_inline = true;
        {
            let mut builder = FunctionBuilder::new(&mut function);
            let word = builder.add_param(MirType::bytes32());
            let length = builder.add_param(MirType::uint256());
            builder.add_return(MirType::MemoryObject(MemoryObjectKind::Bytes));
            let size = builder.imm_u64(64);
            let object = builder.alloc_object(
                size,
                MemoryObjectLayout::Bytes,
                AllocationSemantics::INTERNAL,
            );
            builder.set_memory_object_len(object, length, MemoryObjectKind::Bytes);
            let zero = builder.imm_u64(0);
            builder.memory_object_store_word(object, zero, word);
            builder.ret([object]);
        }
        let id = self.context.module.add_function(function);
        *self.context.literal_word_helper = Some(id);
        id
    }

    pub(super) fn build_bytes_literal(
        builder: &mut FunctionBuilder<'_>,
        bytes: &[u8],
        semantics: AllocationSemantics,
    ) -> Option<ValueId> {
        let words = u64::try_from(bytes.len().div_ceil(32)).ok()?;
        let size = words.checked_add(1)?.checked_mul(32)?;
        let size = builder.imm_u64(size);
        let object = builder.alloc_object(size, MemoryObjectLayout::Bytes, semantics);
        let length = builder.imm_u64(u64::try_from(bytes.len()).ok()?);
        builder.set_memory_object_len(object, length, MemoryObjectKind::Bytes);
        for (index, chunk) in bytes.chunks(32).enumerate() {
            let mut word = U256::from_be_slice(chunk);
            word <<= (32 - chunk.len()) * 8;
            let value = builder.imm_u256(word);
            let offset = builder.imm_u64(index as u64 * 32);
            builder.memory_object_store_word(object, offset, value);
        }
        Some(object)
    }

    fn ensure_bytes_literal_helper(&mut self, bytes: &[u8]) -> FunctionId {
        if let Some(&id) = self.context.literal_helpers.get(bytes) {
            return id;
        }
        let index = self.context.literal_helpers.len();
        let mut function = Function::new(Ident::from_str(&format!("__literal_bytes_{index}")));
        function.attributes.no_inline = true;
        {
            let mut builder = FunctionBuilder::new(&mut function);
            builder.add_return(MirType::MemoryObject(MemoryObjectKind::Bytes));
            let object =
                Self::build_bytes_literal(&mut builder, bytes, AllocationSemantics::INTERNAL)
                    .expect("literal length fits in a memory object");
            builder.ret([object]);
        }
        let id = self.context.module.add_function(function);
        self.context.literal_helpers.insert(bytes.to_vec(), id);
        id
    }

    pub(super) fn default_value(&mut self, ty: Ty<'gcx>) -> ValueId {
        self.default_object(ty).unwrap_or_else(|| self.builder.imm_u256(U256::ZERO))
    }

    pub(super) fn default_binding_value(&mut self, ty: Ty<'gcx>) -> ValueId {
        if ty.is_ref_at(DataLocation::Calldata)
            && matches!(
                ty.peel_refs().kind,
                TyKind::DynArray(_)
                    | TyKind::Slice(_)
                    | TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String,)
            )
        {
            let zero = self.builder.imm_u256(U256::ZERO);
            return self.builder.make_slice(zero, zero, SliceLocation::Calldata);
        }
        self.default_value(ty)
    }

    pub(super) fn default_object(&mut self, ty: Ty<'gcx>) -> Option<ValueId> {
        self.default_object_with_semantics(ty, AllocationSemantics::INTERNAL)
    }

    fn default_object_with_semantics(
        &mut self,
        ty: Ty<'gcx>,
        semantics: AllocationSemantics,
    ) -> Option<ValueId> {
        let layout = self.types.memory_layout(ty)?;
        let size = Self::default_object_size(layout)?;
        let size = self.builder.imm_u64(size);
        let object = self.builder.alloc_object(size, layout, semantics);
        match ty.peel_refs().kind {
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
            | TyKind::DynArray(_) => {
                let zero = self.builder.imm_u256(U256::ZERO);
                self.builder.set_memory_object_len(object, zero, layout.kind());
            }
            TyKind::Struct(id) => {
                let zero = self.builder.imm_u256(U256::ZERO);
                for (index, &field) in self.context.gcx.hir.strukt(id).fields.iter().enumerate() {
                    let field_ty = self.context.gcx.type_of_item(field.into());
                    let value =
                        self.default_object_with_semantics(field_ty, semantics).unwrap_or(zero);
                    self.builder.memory_object_store_field(object, layout, index as u64, value);
                }
            }
            TyKind::Array(element, len) => {
                if !self.default_object_is_fully_initialized(ty) {
                    self.builder.memory_zero(object, size);
                }
                let Ok(len) = u64::try_from(len) else { return Some(object) };
                if self.types.memory_layout(element).is_some() {
                    for index in 0..len {
                        let Some(value) = self.default_object_with_semantics(element, semantics)
                        else {
                            continue;
                        };
                        let index = self.builder.imm_u64(index);
                        self.builder.memory_object_store_element(object, layout, index, value);
                    }
                }
            }
            _ => {}
        }
        Some(object)
    }

    fn default_object_is_fully_initialized(&self, ty: Ty<'gcx>) -> bool {
        let Some(layout) = self.types.memory_layout(ty) else { return false };
        if Self::default_object_size(layout).is_none() {
            return false;
        }
        match ty.peel_refs().kind {
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
            | TyKind::DynArray(_) => true,
            TyKind::Struct(id) => self.context.gcx.hir.strukt(id).fields.iter().all(|&field| {
                let field_ty = self.context.gcx.type_of_item(field.into());
                self.types.memory_layout(field_ty).and_then(Self::default_object_size).is_some()
            }),
            TyKind::Array(element, _) => {
                self.types.memory_layout(element).and_then(Self::default_object_size).is_some()
            }
            _ => false,
        }
    }

    fn default_object_size(layout: MemoryObjectLayout) -> Option<u64> {
        match layout {
            MemoryObjectLayout::Bytes | MemoryObjectLayout::DynamicArray { .. } => Some(32),
            MemoryObjectLayout::FixedArray { len, element_words } => {
                len.checked_mul(u64::from(element_words))?.checked_mul(32)
            }
            MemoryObjectLayout::Struct { fields } => fields.checked_mul(32),
        }
    }
}
