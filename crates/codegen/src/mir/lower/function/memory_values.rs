//! Memory-backed value construction and default aggregate values.

use super::*;
use crate::{
    link::{CodeKind, ContractCode},
    mir::lower::data::{DataDest, copy_data_to_memory},
};
use solar_interface::diagnostics::ErrorGuaranteed;

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
        let object = self.builder.alloc(size, AllocationSemantics::INTERNAL);
        if dynamic {
            // object.len = element_count
            let length = self.builder.imm(u64::try_from(elements.len()).ok()?);
            self.builder.set_memory_len(object, length);
        }

        // for element, i { object[i] = coerce(element) }
        for (index, element) in elements.iter().enumerate() {
            let source_ty = self.cx.gcx.type_of_expr(element.id)?;
            let value = if source_ty.is_ref_at(DataLocation::Storage) {
                // A storage element, such as `[flag ? a : b]`, lowers to its slot; copy it out.
                // element = load_storage_object(element_ty, slot)
                let slot = self.lower_component(element)?;
                self.convert_tuple_component(slot, source_ty, element_ty, element.span)?
            } else {
                let value = self.lower_expr(element)?;
                let value = self.coerce_value(value, source_ty, element_ty);
                self.materialize_memory_argument(element_ty, value, element.span)?
            };
            let value = self.encode_memory_scalar(element_ty, value);
            self.builder.memory_store_field(object, index as u64, value);
        }
        Some(object)
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
        let object = self.builder.alloc_word_struct(fields, AllocationSemantics::INTERNAL);
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
                let field_ty = this
                    .cx
                    .gcx
                    .type_of_item(struct_fields[index].into())
                    .with_loc_if_ref(this.cx.gcx, DataLocation::Memory);
                let value = this.lower_typed_expr(argument, field_ty)?;
                let value = this.materialize_memory_argument(field_ty, value, argument.span)?;
                Some(this.encode_memory_scalar(field_ty, value))
            },
        )?;
        for (index, value) in arguments.into_iter().enumerate() {
            self.builder.memory_store_field(object, index as u64, value);
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
        let object = self.builder.alloc_word_struct(fields, initialization);
        for (index, value) in values.iter().enumerate() {
            let Some(value) = value else { continue };
            let value = self.lower_expr(value)?;
            let value = self.encode_memory_scalar(field_types[index], value);
            self.builder.memory_store_field(object, index as u64, value);
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
            true,
        )
    }

    pub(super) fn lower_shared_bytes_literal(&mut self, symbol: ByteSymbol) -> Option<ValueId> {
        let bytes = symbol.as_byte_str();
        if bytes.is_empty() {
            // A heap object, unlike scratch memory, so that later reads of its length fold.
            return self.default_object(self.cx.gcx.types.bytes_ref.memory);
        }
        let value = if bytes.len() <= 32 && self.cx.shared_word_literals.contains(&symbol) {
            let helper = self.ensure_bytes_word_helper();
            let word = self.lower_string_literal_word(bytes);
            let length = self.builder.imm(bytes.len() as u64);
            self.builder.icall(helper, vec![word, length], MirType::MEMPTR)
        } else if let Some(index) = self.cx.shared_literals.get_index_of(&symbol) {
            let helper = self.ensure_bytes_literal_helper(symbol, index);
            self.builder.icall(helper, Vec::new(), MirType::MEMPTR)
        } else {
            self.lower_bytes_literal(bytes)?
        };
        Some(value)
    }

    fn ensure_bytes_word_helper(&mut self) -> FunctionId {
        // object = bytes(word, length) !preserves_fmp
        // object[0] = word
        // return object
        self.lazy_helper(sym::literal_bytes_word, |_, function| {
            let mut builder = FunctionBuilder::new_semantic(function);
            let word = builder.add_param(MirType::I256);
            let length = builder.add_param(MirType::I256);
            builder.set_return_type(MirType::MEMPTR);
            let size = builder.imm(64);
            let object = builder.alloc(size, AllocationSemantics::INTERNAL);
            let Value::Inst(alloc) = *builder.func().value(object) else {
                unreachable!("allocation result must reference its instruction")
            };
            builder.func_mut().inst_mut(alloc).metadata.set_preserves_fmp(true);
            builder.set_memory_len(object, length);
            let zero = builder.imm(0);
            builder.memory_store_word(object, zero, word);
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
        preserves_fmp: bool,
    ) -> Option<ValueId> {
        let (object, padded_size) =
            Self::alloc_const_bytes(builder, bytes.len(), semantics, preserves_fmp)?;
        // copy data to object + 32
        let data = DataDest { base: object, offset: EvmMemoryLayout::DYNAMIC_HEADER_SIZE };
        copy_data_to_memory(gcx, module, builder, data, bytes, padded_size, None);
        Some(object)
    }

    /// Returns a pointer to the zero slot, which holds an empty dynamic object that is only read.
    pub(super) fn zero_slot(&mut self) -> ValueId {
        // value = ZERO_SLOT
        let value = crate::mir::Immediate::for_type(
            Some(MirType::MEMPTR),
            U256::from(EvmMemoryLayout::ZERO_SLOT),
        );
        self.builder.alloc_value(Value::Immediate(value))
    }

    /// Lowers the creation or runtime bytecode of a contract to a `bytes` object.
    pub(super) fn lower_contract_code(
        &mut self,
        span: Span,
        contract_id: hir::ContractId,
        kind: CodeKind,
    ) -> Option<ValueId> {
        match self.contract_code(span, contract_id, kind) {
            // value = bytes(creation_bytecode(C) | runtime_bytecode(C))
            Ok(code) => Some(Self::build_bytecode(&mut self.builder, code)),
            // value = error
            Err(guar) => Some(self.builder.error_value(guar)),
        }
    }

    /// Returns the deferred data for the creation or runtime bytecode of a contract that
    /// this contract embeds, reporting an error when it is not a bytecode dependency.
    pub(super) fn contract_code(
        &mut self,
        span: Span,
        contract_id: hir::ContractId,
        kind: CodeKind,
    ) -> Result<DataId, ErrorGuaranteed> {
        let gcx = self.cx.gcx;
        let name = gcx.hir.contract(contract_id).name;
        if !self.cx.bytecode_dependencies.contains(contract_id) {
            return Err(gcx
                .dcx()
                .err(format!("codegen is missing {} for `{name}`", kind.keyword()))
                .span(span)
                .note("the contract is not a bytecode dependency of the contract being compiled")
                .emit());
        }
        let data_name = Symbol::intern(&format!("{name}_{}", kind.keyword()));
        let code = ContractCode { contract: crate::link::qualified_name(gcx, contract_id), kind };
        Ok(self.cx.module.intern_contract_code(code, data_name))
    }

    pub(super) fn build_bytecode(builder: &mut FunctionBuilder<'_>, code: DataId) -> ValueId {
        let word = EvmMemoryLayout::WORD_SIZE;
        // len = datasize code(C)
        // size = datasize code(C), 63, aligned
        // object = bytes(size, len) !preserves_fmp
        let len = builder.data_size(code, 0, false);
        let size = builder.data_size(code, 2 * word - 1, true);
        let object =
            Self::alloc_bytes_object(builder, size, len, AllocationSemantics::INTERNAL, true);
        let data = builder.memory_data(object);
        // The last data word starts one length word before the padded length. Empty code
        // has no data words, and this clears the length word, which is also zero.
        // mstore object + data_size(code(C), 31, aligned), 0
        // datacopy code(C), data, len
        let tail_offset = builder.data_size(code, word - 1, true);
        let tail = builder.add(object, tail_offset);
        let zero = builder.imm(0);
        builder.store(tail, zero);
        builder.data_copy(DataRef::new(code, 0), data, len);
        object
    }

    fn alloc_const_bytes(
        builder: &mut FunctionBuilder<'_>,
        len: usize,
        semantics: AllocationSemantics,
        preserves_fmp: bool,
    ) -> Option<(ValueId, usize)> {
        let words = u64::try_from(len.div_ceil(32)).ok()?;
        let size = builder.imm(words.checked_add(1)?.checked_mul(32)?);
        let length = builder.imm(u64::try_from(len).ok()?);
        let object = Self::alloc_bytes_object(builder, size, length, semantics, preserves_fmp);
        Some((object, usize::try_from(words.checked_mul(32)?).ok()?))
    }

    /// Allocates a bytes object of `size` bytes, including its length word, holding `len` bytes.
    fn alloc_bytes_object(
        builder: &mut FunctionBuilder<'_>,
        size: ValueId,
        len: ValueId,
        semantics: AllocationSemantics,
        preserves_fmp: bool,
    ) -> ValueId {
        // object = bytes(size) [!preserves_fmp]
        // mstore object, len
        let object = builder.alloc(size, semantics);
        let Value::Inst(alloc) = *builder.func().value(object) else {
            unreachable!("allocation result must reference its instruction")
        };
        builder.func_mut().inst_mut(alloc).metadata.set_preserves_fmp(preserves_fmp);
        builder.set_memory_len(object, len);
        object
    }

    fn ensure_bytes_literal_helper(&mut self, symbol: ByteSymbol, index: usize) -> FunctionId {
        // literal_bytes() -> bytes
        self.lazy_helper(helper_name(sym::literal_bytes, index), |this, function| {
            let mut builder = FunctionBuilder::new_semantic(function);
            builder.set_return_type(MirType::MEMPTR);
            let object = Self::build_bytes_literal(
                this.cx.gcx,
                this.cx.module,
                &mut builder,
                symbol.as_byte_str(),
                AllocationSemantics::INTERNAL,
                true,
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
            return self.builder.make_slice(zero, zero, PtrLocation::Calldata);
        }
        self.default_object_with_mode(ty, true).unwrap_or_else(|| self.builder.imm(U256::ZERO))
    }

    pub(super) fn default_object(&mut self, ty: Ty<'gcx>) -> Option<ValueId> {
        self.default_object_with_mode(ty, false)
    }

    fn default_object_with_mode(&mut self, ty: Ty<'gcx>, preserve_fmp: bool) -> Option<ValueId> {
        let Some((layout, size)) = self
            .types
            .memory_layout(ty)
            .and_then(|layout| Some((layout, Self::default_object_size(layout)?)))
        else {
            return self.oversized_default_object(ty);
        };
        if preserve_fmp
            && matches!(layout, MemoryObjectLayout::Bytes | MemoryObjectLayout::DynamicArray { .. })
        {
            return Some(self.zero_slot());
        }

        // object = alloc(default_layout)
        let size = self.builder.imm(size);
        let object = self.builder.alloc(size, AllocationSemantics::INTERNAL);
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
                self.builder.set_memory_len(object, zero);
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
                    self.builder.memory_store_field(object, index as u64, value);
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
                            this.builder.memory_store_element(object, layout, index, value);
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

    /// Reverts with `Panic(0x41)` when a static array is too large to allocate. Sema bounds memory
    /// variables, but not the fields of a struct array element.
    fn oversized_default_object(&mut self, ty: Ty<'gcx>) -> Option<ValueId> {
        if !matches!(ty.peel_refs().kind, TyKind::Array(..)) {
            return None;
        }
        // panic(MemoryAllocationOverflow)
        // unreachable:
        //   object = 0
        self.builder.panic(PanicCode::MemoryAllocationOverflow);
        let unreachable = self.builder.create_block();
        self.builder.switch_to_block(unreachable);
        Some(self.builder.imm(U256::ZERO))
    }
}
