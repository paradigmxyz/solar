//! Memory-backed value construction and default aggregate values.

use super::*;

const MIN_BULK_ZERO_STRUCT_FIELDS: usize = 4;

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    pub(super) fn lower_array(
        &mut self,
        expr: &hir::Expr<'_>,
        elements: &[hir::Expr<'_>],
    ) -> Option<ValueId> {
        let ty = self.cx.gcx.type_of_expr(expr.id)?;
        let TyKind::Array(element_ty, _) = ty.peel_refs().kind else {
            return self.cx.report_unsupported(expr.span, "array literal");
        };
        let layout = self.types.memory_layout(ty)?;
        let (size, dynamic) = match layout {
            MemoryObjectLayout::FixedArray { len, element_words } => {
                let words = len.checked_mul(u64::from(element_words))?;
                (words.checked_mul(32)?, false)
            }
            MemoryObjectLayout::DynamicArray { element_words } => {
                let words =
                    u64::try_from(elements.len()).ok()?.checked_mul(u64::from(element_words))?;
                (words.checked_add(1)?.checked_mul(32)?, true)
            }
            _ => return self.cx.report_unsupported(expr.span, "array literal"),
        };

        // object = alloc(array)
        let size = self.builder.imm(size);
        let object = self.builder.alloc_object(size, layout, AllocationSemantics::INTERNAL);
        if dynamic {
            // object.len = element_count
            let length = self.builder.imm(u64::try_from(elements.len()).ok()?);
            self.builder.set_memory_object_len(object, length, layout.kind());
        }

        // for element, i { object[i] = coerce(element) }
        for (index, element) in elements.iter().enumerate() {
            let value = self.lower_expr(element)?;
            let value = self.coerce_value(value, self.cx.gcx.type_of_expr(element.id)?, element_ty);
            let value = self.materialize_memory_argument(element_ty, value, element.span)?;
            let value = self.encode_memory_scalar(element_ty, value);
            let index = self.builder.imm(index as u64);
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
        let zero = self.builder.imm(U256::ZERO);
        let is_null = self.builder.eq(value, zero);
        let preheader = self.builder.current_block();
        let allocate = self.builder.create_block();
        let merge = self.builder.create_block();
        // if value == 0 { allocated = default_object(element) }
        self.builder.branch(is_null, allocate, merge);

        self.builder.switch_to_block(allocate);
        let allocated = self.default_object(element)?;
        self.builder.memory_object_store_element(object, layout, index, allocated);
        let allocation_block = self.builder.current_block();
        self.builder.jump(merge);

        // value = phi(value, allocated)
        self.builder.switch_to_block(merge);
        Some(self.builder.phi(vec![(preheader, value), (allocation_block, allocated)]))
    }

    pub(super) fn lower_struct_constructor(
        &mut self,
        expr: &hir::Expr<'_>,
        struct_id: hir::StructId,
        args: hir::CallArgs<'_>,
    ) -> Option<ValueId> {
        // object = alloc(struct_layout)
        // for field { value = lower_typed(argument); object[field] = value }
        let struct_fields = self.cx.gcx.hir.strukt(struct_id).fields;
        let fields = struct_fields.len() as u64;
        if args.len() != fields as usize {
            return self.cx.report_unsupported(expr.span, "struct constructor argument list");
        }
        let parameter_names =
            self.cx.gcx.callable_param_names(CallableParamSource::Struct(struct_id));
        let (object, layout) =
            self.builder.alloc_word_struct(fields, AllocationSemantics::INTERNAL);
        let arguments = self.lower_call_arguments(
            args,
            CallArgumentParams {
                count: struct_fields.len(),
                names: Some(parameter_names.as_slice()),
                reverse: false,
            },
            args.span,
            "struct constructor argument",
            |this, index, argument| {
                let field_ty = this.cx.gcx.type_of_item(struct_fields[index].into());
                let value = this.lower_typed_expr(argument, field_ty)?;
                let value = this.materialize_memory_argument(field_ty, value, argument.span)?;
                Some(this.encode_memory_scalar(field_ty, value))
            },
        )?;
        for (index, value) in arguments.into_iter().enumerate() {
            self.builder.memory_object_store_field(object, layout, index as u64, value);
        }
        Some(object)
    }

    pub(super) fn lower_tuple(
        &mut self,
        expr: &hir::Expr<'_>,
        values: &[Option<&hir::Expr<'_>>],
    ) -> Option<ValueId> {
        // object = alloc(tuple_layout, zeroed_if_omitted)
        // for present field { object[field] = value }
        let ty = self.cx.gcx.type_of_expr(expr.id)?;
        let MemoryObjectLayout::Struct { fields } = self.types.memory_layout(ty)? else {
            return self.cx.report_unsupported(expr.span, "tuple object");
        };
        let TyKind::Tuple(field_types) = ty.peel_refs().kind else {
            return self.cx.report_unsupported(expr.span, "tuple object");
        };
        let initialization = if values.iter().all(Option::is_some) {
            AllocationSemantics::INTERNAL
        } else {
            AllocationSemantics::SOLIDITY_ZEROED
        };
        let (object, layout) = self.builder.alloc_word_struct(fields, initialization);
        for (index, value) in values.iter().enumerate() {
            let Some(value) = value else { continue };
            let value = self.lower_expr(value)?;
            let value = self.encode_memory_scalar(field_types[index], value);
            self.builder.memory_object_store_field(object, layout, index as u64, value);
        }
        Some(object)
    }

    pub(super) fn lower_bytes_literal(&mut self, bytes: &[u8]) -> Option<ValueId> {
        Self::build_bytes_literal(
            self.cx.gcx,
            self.cx.module,
            &mut self.builder,
            bytes,
            AllocationSemantics::INTERNAL,
            None,
        )
    }

    pub(super) fn lower_shared_bytes_literal(&mut self, symbol: ByteSymbol) -> Option<ValueId> {
        let bytes = symbol.as_byte_str();
        let value = if !bytes.is_empty()
            && bytes.len() <= 32
            && self.cx.shared_word_literals.contains(&symbol)
        {
            let helper = self.ensure_bytes_word_helper();
            let word = self.lower_string_literal_word(bytes);
            let length = self.builder.imm(bytes.len() as u64);
            self.builder.icall(
                helper,
                vec![word, length],
                MirType::MemoryObject(MemoryObjectKind::Bytes),
                1,
            )
        } else if self.cx.shared_literals.contains(&symbol) {
            let helper = self.ensure_bytes_literal_helper(symbol);
            self.builder.icall(
                helper,
                Vec::new(),
                MirType::MemoryObject(MemoryObjectKind::Bytes),
                1,
            )
        } else {
            self.lower_bytes_literal(bytes)?
        };
        Some(value)
    }

    fn ensure_bytes_word_helper(&mut self) -> FunctionId {
        // object = bytes(word, length)
        // object[0] = word
        // return object
        self.lazy_helper(sym::literal_bytes_word, |_, function| {
            let mut builder = FunctionBuilder::new(function);
            let word = builder.add_param(MirType::bytes32());
            let length = builder.add_param(MirType::uint256());
            builder.add_return(MirType::MemoryObject(MemoryObjectKind::Bytes));
            let size = builder.imm(64);
            let object = builder.alloc_object(
                size,
                MemoryObjectLayout::Bytes,
                AllocationSemantics::INTERNAL,
            );
            builder.set_memory_object_len(object, length, MemoryObjectKind::Bytes);
            let zero = builder.imm(0);
            builder.memory_object_store_word(object, zero, word);
            builder.ret([object]);
            Some(())
        })
        .expect("literal word helper construction cannot fail")
    }

    pub(super) fn build_bytes_literal(
        gcx: Gcx<'_>,
        module: &mut Module,
        builder: &mut FunctionBuilder<'_>,
        bytes: &[u8],
        semantics: AllocationSemantics,
        name: Option<Symbol>,
    ) -> Option<ValueId> {
        // object = bytes(len)
        let words = u64::try_from(bytes.len().div_ceil(32)).ok()?;
        let size = builder.imm(words.checked_add(1)?.checked_mul(32)?);
        let object = builder.alloc_object(size, MemoryObjectLayout::Bytes, semantics);
        let length = builder.imm(u64::try_from(bytes.len()).ok()?);
        builder.set_memory_object_len(object, length, MemoryObjectKind::Bytes);
        let data = builder.memory_object_data(object, MemoryObjectKind::Bytes);
        super::super::data::copy_data_to_memory(
            gcx,
            module,
            builder,
            data,
            bytes,
            usize::try_from(words.checked_mul(32)?).ok()?,
            name,
        );
        Some(object)
    }

    fn ensure_bytes_literal_helper(&mut self, symbol: ByteSymbol) -> FunctionId {
        // literal_bytes() -> bytes
        self.lazy_helper(helper_name(sym::literal_bytes, symbol.as_u32()), |this, function| {
            let mut builder = FunctionBuilder::new(function);
            builder.add_return(MirType::MemoryObject(MemoryObjectKind::Bytes));
            let object = Self::build_bytes_literal(
                this.cx.gcx,
                this.cx.module,
                &mut builder,
                symbol.as_byte_str(),
                AllocationSemantics::INTERNAL,
                None,
            )
            .expect("literal length fits in a memory object");
            builder.ret([object]);
            Some(())
        })
        .expect("literal helper construction cannot fail")
    }

    pub(super) fn default_value(&mut self, ty: Ty<'gcx>) -> ValueId {
        self.default_object(ty).unwrap_or_else(|| self.builder.imm(U256::ZERO))
    }

    pub(super) fn default_binding_value(&mut self, ty: Ty<'gcx>) -> ValueId {
        if ty.is_ref_at(DataLocation::Calldata) {
            let zero = self.builder.imm(U256::ZERO);
            return self.builder.make_slice(zero, zero, SliceLocation::Calldata);
        }
        self.default_object_with_mode(ty, true).unwrap_or_else(|| self.builder.imm(U256::ZERO))
    }

    pub(super) fn default_object(&mut self, ty: Ty<'gcx>) -> Option<ValueId> {
        self.default_object_with_mode(ty, false)
    }

    fn default_object_with_mode(&mut self, ty: Ty<'gcx>, preserve_fmp: bool) -> Option<ValueId> {
        let layout = self.types.memory_layout(ty)?;
        if preserve_fmp
            && matches!(layout, MemoryObjectLayout::Bytes | MemoryObjectLayout::DynamicArray { .. })
        {
            // object = ZERO_SLOT
            return Some(self.builder.imm(EvmMemoryLayout::ZERO_SLOT));
        }

        // object = alloc(default_layout)
        let size = self.builder.imm(Self::default_object_size(layout)?);
        let object = self.builder.alloc_object(size, layout, AllocationSemantics::INTERNAL);
        if preserve_fmp {
            let Value::Inst(alloc) = *self.builder.func().value(object) else {
                unreachable!("allocation result must reference its instruction")
            };
            self.builder.func_mut().inst_mut(alloc).metadata.set_preserves_fmp(true);
        }
        match ty.peel_refs().kind {
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
            | TyKind::DynArray(_) => {
                // object.len = 0
                let zero = self.builder.imm(U256::ZERO);
                self.builder.set_memory_object_len(object, zero, layout.kind());
            }
            TyKind::Struct(id) => {
                let fields = self.cx.gcx.hir.strukt(id).fields;
                let bulk_zero = preserve_fmp && fields.len() >= MIN_BULK_ZERO_STRUCT_FIELDS;
                if bulk_zero {
                    // memory_zero(object, size)
                    self.builder.memory_zero(object, size);
                }
                let zero = self.builder.imm(U256::ZERO);

                // for reference_field { object[field] = default(reference_field) }
                for (index, &field) in fields.iter().enumerate() {
                    let field_ty = self.cx.gcx.type_of_item(field.into());
                    if bulk_zero && field_ty.peel_refs().is_value_type() {
                        continue;
                    }
                    let value =
                        self.default_object_with_mode(field_ty, preserve_fmp).unwrap_or(zero);
                    self.builder.memory_object_store_field(object, layout, index as u64, value);
                }
            }
            TyKind::Array(element, len) => {
                if !self.default_object_is_fully_initialized(ty) {
                    // memory_zero(object, size)
                    self.builder.memory_zero(object, size);
                }
                let Ok(len) = u64::try_from(len) else { return Some(object) };
                if self.types.memory_layout(element).is_some() {
                    // for i in 0..len { object[i] = default(element) }
                    let len = self.builder.imm(len);
                    self.counted_loop(len, |this, index| {
                        if let Some(value) = this.default_object_with_mode(element, preserve_fmp) {
                            this.builder.memory_object_store_element(object, layout, index, value);
                        }
                    });
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
            TyKind::Struct(id) => self.cx.gcx.hir.strukt(id).fields.iter().all(|&field| {
                let field_ty = self.cx.gcx.type_of_item(field.into());
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
