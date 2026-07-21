//! Call and member-call lowering.

use super::{Lowerer, checked_arith::PanicCode};
use crate::{
    memory::EvmMemoryLayout,
    mir::{FunctionBuilder, ValueId},
};
use alloy_primitives::{U256, keccak256};
use solar_ast::{LitKind, Span};
use solar_data_structures::bit_set::GrowableBitSet;
use solar_interface::{Ident, Symbol, kw, sym};
use solar_sema::{
    builtins::Builtin,
    eval::erc7201_slot,
    hir::{self, CallArgs, ElementaryType, ExprKind},
    ty::{Ty, TyKind},
};

/// How a value travels across a linked-library delegatecall boundary.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum LinkedFieldKind {
    /// One head word holding the value itself.
    Value,
    /// One head word holding the args-relative offset of a
    /// `[len][elems...]` tail.
    DynArray,
    /// One head word holding the args-relative offset of a
    /// `[len][padded bytes]` tail.
    DynBytes,
}

impl<'gcx> Lowerer<'gcx> {
    /// Lowers a function call.
    pub(super) fn lower_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        callee: &hir::Expr<'_>,
        args: &CallArgs<'_>,
        call_opts: Option<&[hir::NamedArg<'_>]>,
    ) -> ValueId {
        if let Some(builtin) = self.gcx.builtin_callee(callee.id) {
            // `T.wrap(x)` / `T.unwrap(v)` for a user-defined value type are identity
            // operations at the EVM level: a UDVT value is represented exactly as its
            // underlying type, so no wrapper is added or removed.
            if matches!(builtin, Builtin::UdvtWrap | Builtin::UdvtUnwrap)
                && let Some(arg) = args.exprs().next()
            {
                return self.lower_expr(builder, arg);
            }

            if Self::builtin_uses_direct_call_lowering(builtin) {
                return self.lower_builtin_call(builder, builtin, args);
            }
        }

        if let Some(error_id) = self.custom_error_id_from_callee(callee) {
            self.emit_custom_error_revert(builder, error_id, args);
            return builder.imm_u64(0);
        }

        if let ExprKind::Member(base, member) = &callee.kind {
            return self
                .lower_member_call_with_opts(builder, callee, base, *member, args, call_opts);
        }

        // Handle `new Contract(args)` - contract creation
        if let ExprKind::New(ty) = &callee.kind {
            if self.is_memory_array_new_type(ty) {
                return self.lower_new_array(builder, ty, args);
            }
            return self.lower_new_contract(builder, ty, args, call_opts);
        }

        // Handle internal function calls: func(args) where func is a function in the same contract
        if let ExprKind::Ident(_) = &callee.kind
            && let Some(resolved) = self.gcx.resolved_callee(callee.id)
            && let hir::Res::Item(item_id) = resolved.res
        {
            match item_id {
                hir::ItemId::Function(func_id) => {
                    return self.lower_internal_call(builder, func_id, args);
                }
                hir::ItemId::Contract(_) | hir::ItemId::Enum(_) => {
                    if let Some(first_arg) = args.exprs().next() {
                        return self.lower_expr(builder, first_arg);
                    }
                }
                hir::ItemId::Struct(struct_id) => {
                    return self.lower_struct_constructor(builder, struct_id, args);
                }
                _ => {}
            }
        }

        // Handle Type(expr) where callee is an explicit Type expression
        // e.g., uint256(x), address(y), bytes32(z)
        if let ExprKind::Type(ty) = &callee.kind
            && let Some(first_arg) = args.exprs().next()
        {
            let value = self.lower_expr(builder, first_arg);
            return self.lower_type_conversion(builder, ty, first_arg, value);
        }

        builder.imm_u64(0)
    }

    fn builtin_uses_direct_call_lowering(builtin: Builtin) -> bool {
        !matches!(
            builtin,
            Builtin::AddressCall
                | Builtin::AddressDelegatecall
                | Builtin::AddressStaticcall
                | Builtin::AddressPayableTransfer
                | Builtin::AddressPayableSend
                | Builtin::ArrayLength
                | Builtin::ArrayPush0
                | Builtin::ArrayPush
                | Builtin::ArrayPop
                | Builtin::UdvtWrap
                | Builtin::UdvtUnwrap
        )
    }

    fn custom_error_id_from_callee(&self, callee: &hir::Expr<'_>) -> Option<hir::ErrorId> {
        if let Some(resolved) = self.gcx.resolved_callee(callee.id)
            && let hir::Res::Item(hir::ItemId::Error(error_id)) = resolved.res
        {
            return Some(error_id);
        }

        if let Some(ty) = self.get_expr_type(callee)
            && let TyKind::Error(_, error_id) = ty.kind
            && self.gcx.dcx().has_errors().is_ok()
        {
            panic!("typeck did not record resolved custom-error callee {error_id:?}");
        }

        None
    }

    fn emit_revert_payload_from_expr(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        expr: &hir::Expr<'_>,
    ) -> bool {
        if self.emit_custom_error_revert_from_expr(builder, expr) {
            return true;
        }
        self.emit_revert_error_string_from_expr(builder, expr)
    }

    fn emit_custom_error_revert_from_expr(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        expr: &hir::Expr<'_>,
    ) -> bool {
        let ExprKind::Call(callee, args, _) = &expr.kind else { return false };
        let Some(error_id) = self.custom_error_id_from_callee(callee) else {
            return false;
        };
        self.emit_custom_error_revert(builder, error_id, args);
        true
    }

    fn emit_custom_error_revert(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        error_id: hir::ErrorId,
        args: &CallArgs<'_>,
    ) {
        let param_tys = self.gcx.item_parameter_types(hir::ItemId::Error(error_id));
        let arg_exprs = self.ordered_custom_error_args(error_id, args);
        let mut items = Vec::with_capacity(param_tys.len());
        for (&ty, arg) in param_tys.iter().zip(arg_exprs) {
            let value = self.lower_return_value_for_ty(builder, arg, ty);
            items.push((value, ty));
        }

        let selector = self.custom_error_selector(error_id);
        self.emit_abi_error_revert(builder, selector, &items);
    }

    fn ordered_custom_error_args<'a>(
        &self,
        error_id: hir::ErrorId,
        args: &'a CallArgs<'a>,
    ) -> Vec<&'a hir::Expr<'a>> {
        match args.kind {
            hir::CallArgsKind::Unnamed(exprs) => exprs.iter().collect(),
            hir::CallArgsKind::Named(named_args) => {
                let error = self.gcx.hir.error(error_id);
                let mut ordered = Vec::with_capacity(error.parameters.len());
                for &param_id in error.parameters {
                    let Some(param_name) =
                        self.gcx.hir.variable(param_id).name.map(|name| name.name)
                    else {
                        continue;
                    };
                    if let Some(arg) = named_args.iter().find(|arg| arg.name.name == param_name) {
                        ordered.push(&arg.value);
                    }
                }
                ordered
            }
        }
    }

    fn custom_error_selector(&self, error_id: hir::ErrorId) -> [u8; 4] {
        let signature = self.gcx.item_signature(hir::ItemId::Error(error_id));
        let hash = keccak256(signature.as_bytes());
        [hash[0], hash[1], hash[2], hash[3]]
    }

    /// Lowers a `new T[](len)` memory array expression.
    fn lower_new_array(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        ty: &hir::Type<'_>,
        args: &CallArgs<'_>,
    ) -> ValueId {
        if !self.is_memory_array_new_type(ty) {
            return builder.imm_u64(0);
        }

        let len = args
            .exprs()
            .next()
            .map(|arg| self.lower_expr(builder, arg))
            .unwrap_or_else(|| builder.imm_u64(0));

        let word_size = builder.imm_u64(EvmMemoryLayout::DYNAMIC_HEADER_SIZE);
        let data_size = if matches!(
            &ty.kind,
            hir::TypeKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
        ) {
            // `bytes`/`string`: the length counts bytes; the data area is the
            // length padded up to a word.
            let thirty_one = builder.imm_u64(31);
            let rounded = builder.add(len, thirty_one);
            let rounded_overflow = builder.lt(rounded, len);
            self.emit_panic_if(builder, rounded_overflow, PanicCode::MemoryAllocationOverflow);
            let mask = builder.not(thirty_one);
            builder.and(rounded, mask)
        } else {
            // Arrays: one word per element.
            let data_size = builder.mul(len, word_size);
            let checked_len = builder.div(data_size, word_size);
            let overflow = builder.eq(checked_len, len);
            self.emit_panic_if_zero(builder, overflow, PanicCode::MemoryAllocationOverflow);
            data_size
        };
        let total_size = builder.add(data_size, word_size);
        let total_overflow = builder.lt(total_size, data_size);
        self.emit_panic_if(builder, total_overflow, PanicCode::MemoryAllocationOverflow);
        let object_layout = if matches!(
            &ty.kind,
            hir::TypeKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
        ) {
            crate::mir::MemoryObjectLayout::Bytes
        } else {
            crate::mir::MemoryObjectLayout::DynamicArray { element_words: 1 }
        };
        let ptr = builder.alloc_object(
            total_size,
            object_layout,
            crate::mir::AllocationSemantics::SOLIDITY_ZEROED,
        );
        builder.set_memory_object_len(ptr, len, object_layout.kind());

        ptr
    }

    fn is_memory_array_new_type(&self, ty: &hir::Type<'_>) -> bool {
        match &ty.kind {
            hir::TypeKind::Array(array) => array.size.is_none(),
            hir::TypeKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => true,
            _ => false,
        }
    }

    /// Lowers a `new Contract(args)` expression.
    /// Supports call options like `new Contract{salt: s, value: v}(args)`.
    fn lower_new_contract(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        ty: &hir::Type<'_>,
        args: &CallArgs<'_>,
        call_opts: Option<&[hir::NamedArg<'_>]>,
    ) -> ValueId {
        // Extract ContractId from the type
        let contract_id = match &ty.kind {
            hir::TypeKind::Custom(hir::ItemId::Contract(id)) => *id,
            _ => {
                return self.err_value(
                    builder,
                    ty.span,
                    "codegen expected a contract type for `new` expression",
                );
            }
        };

        // Look up pre-compiled bytecode
        let bytecode = match self.contract_bytecodes.get(&contract_id) {
            Some(bc) => bc.clone(),
            None => {
                let guar = self
                    .gcx
                    .dcx()
                    .err(format!(
                        "codegen is missing creation bytecode for `new {}`",
                        self.gcx.hir.contract(contract_id).name
                    ))
                    .span(ty.span)
                    .note("the deployed contract did not compile or was not lowered first")
                    .emit();
                return builder.error_value(guar);
            }
        };

        let bytecode_len = bytecode.len();

        // Extract call options (salt, value)
        let mut salt_opt: Option<ValueId> = None;
        let mut value_opt: Option<ValueId> = None;

        if let Some(opts) = call_opts {
            for opt in opts {
                match opt.name.name {
                    sym::salt => {
                        salt_opt = Some(self.lower_expr(builder, &opt.value));
                    }
                    sym::value => {
                        value_opt = Some(self.lower_expr(builder, &opt.value));
                    }
                    _ => {
                        // gas option is not supported for contract creation
                    }
                }
            }
        }

        // Allocate memory for bytecode + constructor args from free memory pointer
        let mem_offset = builder.fmp();

        // Copy bytecode to memory using MSTORE
        // For each 32-byte chunk of bytecode, emit an MSTORE at (mem_offset + offset)
        for (i, chunk) in bytecode.chunks(32).enumerate() {
            let mut padded = [0u8; 32];
            padded[..chunk.len()].copy_from_slice(chunk);
            let value = U256::from_be_bytes(padded);
            let val_id = builder.imm_u256(value);
            let chunk_offset = builder.imm_u64((i as u64) * 32);
            let dest = builder.add(mem_offset, chunk_offset);
            builder.mstore(dest, val_id);
        }

        // Append constructor arguments after bytecode
        let mut args_offset = bytecode_len as u64;
        for arg in args.exprs() {
            let arg_val = self.lower_expr(builder, arg);
            let arg_offset_imm = builder.imm_u64(args_offset);
            let arg_dest = builder.add(mem_offset, arg_offset_imm);
            builder.mstore(arg_dest, arg_val);
            args_offset += 32; // Each arg is 32 bytes ABI encoded
        }

        // Total size = bytecode + args
        let total_size = builder.imm_u64(args_offset);

        // Update free memory pointer: new_free = mem_offset + ((total_size + 31) & ~31)
        let thirty_one = builder.imm_u64(31);
        let aligned_size = builder.add(total_size, thirty_one);
        let mask = builder.imm_u256(U256::from(!31u64));
        let aligned_size = builder.and(aligned_size, mask);
        let new_free = builder.add(mem_offset, aligned_size);
        builder.set_fmp(new_free);

        // Value to send with CREATE/CREATE2 (0 for non-payable, or from value option)
        let value = value_opt.unwrap_or_else(|| builder.imm_u64(0));

        // Emit CREATE2 if salt is provided, otherwise CREATE
        if let Some(salt) = salt_opt {
            builder.create2(value, mem_offset, total_size, salt)
        } else {
            builder.create(value, mem_offset, total_size)
        }
    }

    /// Lowers a builtin function call.
    fn lower_builtin_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        builtin: Builtin,
        args: &CallArgs<'_>,
    ) -> ValueId {
        match builtin {
            Builtin::Keccak256 => {
                let mut exprs = args.exprs();
                if let Some(first) = exprs.next() {
                    // TODO(OSS-413): syntax-directed special case. A string
                    // literal argument is hashed at compile time, but the same
                    // constant reaching here through a variable is not; folding
                    // keccak over known memory contents belongs in a MIR pass
                    // so both spellings are handled uniformly.
                    if let ExprKind::Lit(lit) = &first.kind
                        && let LitKind::Str(_, bytes, _) = &lit.kind
                    {
                        let hash = keccak256(bytes.as_byte_str());
                        return builder.imm_u256(U256::from_be_bytes(hash.0));
                    }

                    if let Some(packed_args) = self.abi_encode_packed_call_args(first) {
                        return self.lower_keccak_abi_encode_packed(builder, packed_args);
                    }
                    if let Some(encode_args) = self.abi_encode_call_args(first) {
                        let arg_exprs: Vec<_> = encode_args.exprs().collect();
                        if let Some(hash) = self.lower_keccak_abi_encode(builder, &arg_exprs) {
                            return hash;
                        }
                    }

                    // Dynamic `bytes`/`string` (incl. `bytes(s)` of a calldata
                    // value): hash the raw data after materializing it to memory.
                    if let Some(hash) = self.keccak_dynamic_bytes(builder, first) {
                        return hash;
                    }
                    let arg_val = self.lower_expr(builder, first);
                    let ptr = builder.imm_u64(0);
                    builder.mstore(ptr, arg_val);
                    let size = builder.imm_u64(32);
                    return builder.keccak256(ptr, size);
                }
                builder.imm_u64(0)
            }
            Builtin::Erc7201 => self.lower_erc7201_call(builder, args),
            Builtin::Require | Builtin::Assert => {
                let mut exprs = args.exprs();
                if let Some(first) = exprs.next() {
                    let cond = self.lower_expr(builder, first);
                    let is_false = builder.iszero(cond);

                    let revert_block = builder.create_block();
                    let continue_block = builder.create_block();

                    builder.branch(is_false, revert_block, continue_block);

                    builder.switch_to_block(revert_block);
                    if matches!(builtin, Builtin::Assert) {
                        self.emit_panic_revert(builder, PanicCode::Assert);
                    } else if let Some(message) = exprs.next() {
                        if !self.emit_revert_payload_from_expr(builder, message) {
                            let zero = builder.imm_u64(0);
                            builder.revert(zero, zero);
                        }
                    } else {
                        let zero = builder.imm_u64(0);
                        builder.revert(zero, zero);
                    }

                    builder.switch_to_block(continue_block);
                }
                builder.imm_u64(0)
            }
            Builtin::Revert => {
                let zero = builder.imm_u64(0);
                builder.revert(zero, zero);
                zero
            }
            Builtin::RevertMsg => {
                let mut exprs = args.exprs();
                let emitted = exprs.next().is_some_and(|message| {
                    self.emit_revert_error_string_from_expr(builder, message)
                });
                let zero = builder.imm_u64(0);
                if !emitted {
                    builder.revert(zero, zero);
                }
                zero
            }
            Builtin::AddressBalance => {
                let mut exprs = args.exprs();
                if let Some(first) = exprs.next() {
                    let addr = self.lower_expr(builder, first);
                    return builder.balance(addr);
                }
                builder.imm_u64(0)
            }
            Builtin::AddMod | Builtin::MulMod => {
                let mut exprs = args.exprs();
                let Some(a) = exprs.next() else { return builder.imm_u64(0) };
                let Some(b) = exprs.next() else { return builder.imm_u64(0) };
                let Some(n) = exprs.next() else { return builder.imm_u64(0) };
                let a = self.lower_expr(builder, a);
                let b = self.lower_expr(builder, b);
                let n = self.lower_expr(builder, n);
                if matches!(builtin, Builtin::AddMod) {
                    builder.addmod(a, b, n)
                } else {
                    builder.mulmod(a, b, n)
                }
            }
            Builtin::AbiEncode => {
                // abi.encode: a fresh `bytes memory` allocation holding the
                // padded ABI tuple encoding of the arguments.
                let arg_exprs: Vec<_> = args.exprs().collect();
                if let Some(ptr) = self.lower_abi_encode_to_bytes(builder, &arg_exprs) {
                    return ptr;
                }
                self.err_value(
                    builder,
                    args.span,
                    "codegen does not support these `abi.encode` arguments yet",
                )
            }
            Builtin::AbiEncodePacked => {
                // abi.encodePacked: pack values tightly based on their types
                // Returns bytes memory (length + data)
                self.lower_abi_encode_packed(builder, args)
            }
            Builtin::AbiEncodeWithSelector => {
                // A selector-prefixed payload adapted to a `bytes memory`
                // value: `[length][selector + ABI tuple encoding]`.
                let mut exprs = args.exprs();
                if let Some(selector_expr) = exprs.next() {
                    let selector = self.lower_selector_word(builder, selector_expr);
                    let arg_exprs: Vec<_> = exprs.collect();
                    if let Some((data, len)) =
                        self.abi_encode_call_payload(builder, Some(selector), &arg_exprs)
                    {
                        let slice =
                            builder.make_slice(data, len, crate::mir::SliceLocation::Memory);
                        return self.materialize_memory_slice_bytes(builder, slice);
                    }
                }
                self.err_value(
                    builder,
                    args.span,
                    "codegen does not support these `abi.encodeWithSelector` arguments yet",
                )
            }
            Builtin::AbiEncodeWithSignature => {
                let mut exprs = args.exprs();
                if let Some(sig_expr) = exprs.next()
                    && let hir::ExprKind::Lit(lit) = &sig_expr.kind
                    && let solar_ast::LitKind::Str(_, sig, _) = &lit.kind
                {
                    let hash = alloy_primitives::keccak256(sig.as_byte_str());
                    let selector =
                        U256::from(u32::from_be_bytes([hash[0], hash[1], hash[2], hash[3]])) << 224;
                    let selector = builder.imm_u256(selector);
                    let arg_exprs: Vec<_> = exprs.collect();
                    if let Some((data, len)) =
                        self.abi_encode_call_payload(builder, Some(selector), &arg_exprs)
                    {
                        let slice =
                            builder.make_slice(data, len, crate::mir::SliceLocation::Memory);
                        return self.materialize_memory_slice_bytes(builder, slice);
                    }
                }
                self.err_value(
                    builder,
                    args.span,
                    "codegen does not support these `abi.encodeWithSignature` arguments yet",
                )
            }
            Builtin::AbiDecode => self.lower_abi_decode(builder, args),
            Builtin::YulAdd
            | Builtin::YulSub
            | Builtin::YulMul
            | Builtin::YulDiv
            | Builtin::YulMod
            | Builtin::YulExp
            | Builtin::YulNot
            | Builtin::YulAnd
            | Builtin::YulOr
            | Builtin::YulXor
            | Builtin::YulShl
            | Builtin::YulShr
            | Builtin::YulSar
            | Builtin::YulStop
            | Builtin::YulSdiv
            | Builtin::YulSmod
            | Builtin::YulLt
            | Builtin::YulGt
            | Builtin::YulSlt
            | Builtin::YulSgt
            | Builtin::YulEq
            | Builtin::YulIszero
            | Builtin::YulByte
            | Builtin::YulClz
            | Builtin::YulAddmod
            | Builtin::YulMulmod
            | Builtin::YulSignextend
            | Builtin::YulKeccak256
            | Builtin::YulAddress
            | Builtin::YulBalance
            | Builtin::YulSelfbalance
            | Builtin::YulCaller
            | Builtin::YulCallvalue
            | Builtin::YulCalldataload
            | Builtin::YulCalldatasize
            | Builtin::YulCalldatacopy
            | Builtin::YulCodesize
            | Builtin::YulCodecopy
            | Builtin::YulExtcodesize
            | Builtin::YulExtcodecopy
            | Builtin::YulReturndatasize
            | Builtin::YulReturndatacopy
            | Builtin::YulExtcodehash
            | Builtin::YulMload
            | Builtin::YulMstore
            | Builtin::YulMstore8
            | Builtin::YulSload
            | Builtin::YulSstore
            | Builtin::YulTload
            | Builtin::YulTstore
            | Builtin::YulMsize
            | Builtin::YulGas
            | Builtin::YulLog0
            | Builtin::YulLog1
            | Builtin::YulLog2
            | Builtin::YulLog3
            | Builtin::YulLog4
            | Builtin::YulCreate
            | Builtin::YulCreate2
            | Builtin::YulCall
            | Builtin::YulCallcode
            | Builtin::YulDelegatecall
            | Builtin::YulStaticcall
            | Builtin::YulExtcall
            | Builtin::YulExtdelegatecall
            | Builtin::YulExtstaticcall
            | Builtin::YulReturn
            | Builtin::YulRevert
            | Builtin::YulSelfdestruct
            | Builtin::YulInvalid
            | Builtin::YulChainid
            | Builtin::YulBasefee
            | Builtin::YulBlobbasefee
            | Builtin::YulBlobhash
            | Builtin::YulCoinbase
            | Builtin::YulDifficulty
            | Builtin::YulPrevrandao
            | Builtin::YulGaslimit
            | Builtin::YulNumber
            | Builtin::YulTimestamp
            | Builtin::YulGasprice
            | Builtin::YulOrigin
            | Builtin::YulBlockhash
            | Builtin::YulPop
            | Builtin::YulMcopy => self.lower_yul_builtin_call(builder, builtin, args),
            _ => builder.imm_u64(0),
        }
    }

    fn lower_erc7201_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        args: &CallArgs<'_>,
    ) -> ValueId {
        let Some(first) = args.exprs().next() else { return builder.imm_u64(0) };
        if let ExprKind::Lit(lit) = &first.kind
            && let LitKind::Str(_, bytes, _) = &lit.kind
        {
            return builder.imm_u256(erc7201_slot(bytes.as_byte_str()).into());
        }

        let Some(inner_hash) = self.keccak_dynamic_bytes(builder, first) else {
            return builder.imm_u64(0);
        };
        let one = builder.imm_u64(1);
        let inner_hash_minus_one = builder.sub(inner_hash, one);
        let ptr = builder.imm_u64(0);
        builder.mstore(ptr, inner_hash_minus_one);
        let size = builder.imm_u64(32);
        let outer_hash = builder.keccak256(ptr, size);
        let mask = builder.imm_u256(!U256::from(0xff));
        builder.and(outer_hash, mask)
    }

    fn lower_yul_builtin_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        builtin: Builtin,
        args: &CallArgs<'_>,
    ) -> ValueId {
        let arg_vals: Vec<ValueId> =
            args.exprs().map(|arg| self.lower_expr(builder, arg)).collect();
        if let Some(expected) = Self::yul_builtin_arity(builtin)
            && arg_vals.len() != expected
        {
            let guar = self
                .gcx
                .dcx()
                .err(format!(
                    "wrong number of arguments for Yul builtin `{}`: expected {}, found {}",
                    builtin.name(),
                    expected,
                    arg_vals.len()
                ))
                .span(args.span)
                .emit();
            return builder.error_value(guar);
        }

        match builtin {
            Builtin::YulAdd => builder.add(arg_vals[0], arg_vals[1]),
            Builtin::YulSub => builder.sub(arg_vals[0], arg_vals[1]),
            Builtin::YulMul => builder.mul(arg_vals[0], arg_vals[1]),
            Builtin::YulDiv => builder.div(arg_vals[0], arg_vals[1]),
            Builtin::YulSdiv => builder.sdiv(arg_vals[0], arg_vals[1]),
            Builtin::YulMod => builder.mod_(arg_vals[0], arg_vals[1]),
            Builtin::YulSmod => builder.smod(arg_vals[0], arg_vals[1]),
            Builtin::YulAddmod => builder.addmod(arg_vals[0], arg_vals[1], arg_vals[2]),
            Builtin::YulMulmod => builder.mulmod(arg_vals[0], arg_vals[1], arg_vals[2]),
            Builtin::YulExp => builder.exp(arg_vals[0], arg_vals[1]),
            Builtin::YulSignextend => builder.signextend(arg_vals[0], arg_vals[1]),
            Builtin::YulAnd => builder.and(arg_vals[0], arg_vals[1]),
            Builtin::YulOr => builder.or(arg_vals[0], arg_vals[1]),
            Builtin::YulXor => builder.xor(arg_vals[0], arg_vals[1]),
            Builtin::YulNot => builder.not(arg_vals[0]),
            Builtin::YulByte => builder.byte(arg_vals[0], arg_vals[1]),
            Builtin::YulShl => builder.shl(arg_vals[0], arg_vals[1]),
            Builtin::YulShr => builder.shr(arg_vals[0], arg_vals[1]),
            Builtin::YulSar => builder.sar(arg_vals[0], arg_vals[1]),
            Builtin::YulLt => builder.lt(arg_vals[0], arg_vals[1]),
            Builtin::YulGt => builder.gt(arg_vals[0], arg_vals[1]),
            Builtin::YulSlt => builder.slt(arg_vals[0], arg_vals[1]),
            Builtin::YulSgt => builder.sgt(arg_vals[0], arg_vals[1]),
            Builtin::YulEq => builder.eq(arg_vals[0], arg_vals[1]),
            Builtin::YulIszero => builder.iszero(arg_vals[0]),
            Builtin::YulMload => builder.mload(arg_vals[0]),
            Builtin::YulMstore => {
                builder.mstore(arg_vals[0], arg_vals[1]);
                builder.imm_u64(0)
            }
            Builtin::YulMstore8 => {
                builder.mstore8(arg_vals[0], arg_vals[1]);
                builder.imm_u64(0)
            }
            Builtin::YulMsize => builder.msize(),
            Builtin::YulMcopy => {
                self.mcopy(builder, arg_vals[0], arg_vals[1], arg_vals[2], Some(args.span));
                builder.imm_u64(0)
            }
            Builtin::YulSload => builder.sload(arg_vals[0]),
            Builtin::YulSstore => {
                builder.sstore(arg_vals[0], arg_vals[1]);
                builder.imm_u64(0)
            }
            Builtin::YulTload => builder.tload(arg_vals[0]),
            Builtin::YulTstore => {
                builder.tstore(arg_vals[0], arg_vals[1]);
                builder.imm_u64(0)
            }
            Builtin::YulCalldataload => builder.calldataload(arg_vals[0]),
            Builtin::YulCalldatasize => builder.calldatasize(),
            Builtin::YulCalldatacopy => {
                builder.calldatacopy(arg_vals[0], arg_vals[1], arg_vals[2]);
                builder.imm_u64(0)
            }
            Builtin::YulCodesize => builder.codesize(),
            Builtin::YulCodecopy => {
                builder.codecopy(arg_vals[0], arg_vals[1], arg_vals[2]);
                builder.imm_u64(0)
            }
            Builtin::YulExtcodesize => builder.extcodesize(arg_vals[0]),
            Builtin::YulExtcodecopy => {
                builder.extcodecopy(arg_vals[0], arg_vals[1], arg_vals[2], arg_vals[3]);
                builder.imm_u64(0)
            }
            Builtin::YulExtcodehash => builder.extcodehash(arg_vals[0]),
            Builtin::YulReturndatasize => builder.returndatasize(),
            Builtin::YulReturndatacopy => {
                builder.returndatacopy(arg_vals[0], arg_vals[1], arg_vals[2]);
                builder.imm_u64(0)
            }
            Builtin::YulAddress => builder.address(),
            Builtin::YulBalance => builder.balance(arg_vals[0]),
            Builtin::YulSelfbalance => builder.selfbalance(),
            Builtin::YulCaller => builder.caller(),
            Builtin::YulCallvalue => builder.callvalue(),
            Builtin::YulOrigin => builder.origin(),
            Builtin::YulGasprice => builder.gasprice(),
            Builtin::YulBlockhash => builder.blockhash(arg_vals[0]),
            Builtin::YulCoinbase => builder.coinbase(),
            Builtin::YulTimestamp => builder.timestamp(),
            Builtin::YulNumber => builder.number(),
            Builtin::YulDifficulty | Builtin::YulPrevrandao => builder.prevrandao(),
            Builtin::YulGaslimit => builder.gaslimit(),
            Builtin::YulChainid => builder.chainid(),
            Builtin::YulGas => builder.gas(),
            Builtin::YulBasefee => builder.basefee(),
            Builtin::YulBlobbasefee => builder.blobbasefee(),
            Builtin::YulBlobhash => builder.blobhash(arg_vals[0]),
            Builtin::YulKeccak256 => builder.keccak256(arg_vals[0], arg_vals[1]),
            Builtin::YulCall => builder.call(
                arg_vals[0],
                arg_vals[1],
                arg_vals[2],
                arg_vals[3],
                arg_vals[4],
                arg_vals[5],
                arg_vals[6],
            ),
            Builtin::YulStaticcall => builder.staticcall(
                arg_vals[0],
                arg_vals[1],
                arg_vals[2],
                arg_vals[3],
                arg_vals[4],
                arg_vals[5],
            ),
            Builtin::YulDelegatecall => builder.delegatecall(
                arg_vals[0],
                arg_vals[1],
                arg_vals[2],
                arg_vals[3],
                arg_vals[4],
                arg_vals[5],
            ),
            Builtin::YulCreate => builder.create(arg_vals[0], arg_vals[1], arg_vals[2]),
            Builtin::YulCreate2 => {
                builder.create2(arg_vals[0], arg_vals[1], arg_vals[2], arg_vals[3])
            }
            Builtin::YulLog0 => {
                builder.log0(arg_vals[0], arg_vals[1]);
                builder.imm_u64(0)
            }
            Builtin::YulLog1 => {
                builder.log1(arg_vals[0], arg_vals[1], arg_vals[2]);
                builder.imm_u64(0)
            }
            Builtin::YulLog2 => {
                builder.log2(arg_vals[0], arg_vals[1], arg_vals[2], arg_vals[3]);
                builder.imm_u64(0)
            }
            Builtin::YulLog3 => {
                builder.log3(arg_vals[0], arg_vals[1], arg_vals[2], arg_vals[3], arg_vals[4]);
                builder.imm_u64(0)
            }
            Builtin::YulLog4 => {
                builder.log4(
                    arg_vals[0],
                    arg_vals[1],
                    arg_vals[2],
                    arg_vals[3],
                    arg_vals[4],
                    arg_vals[5],
                );
                builder.imm_u64(0)
            }
            Builtin::YulRevert => {
                builder.revert(arg_vals[0], arg_vals[1]);
                builder.imm_u64(0)
            }
            Builtin::YulReturn => {
                // `return(offset, size)` halts and returns `size` bytes of memory.
                builder.ret_data(arg_vals[0], arg_vals[1]);
                builder.imm_u64(0)
            }
            Builtin::YulStop => {
                builder.stop();
                builder.imm_u64(0)
            }
            Builtin::YulInvalid => {
                builder.invalid();
                builder.imm_u64(0)
            }
            Builtin::YulSelfdestruct => {
                builder.selfdestruct(arg_vals[0]);
                builder.imm_u64(0)
            }
            Builtin::YulPop => builder.imm_u64(0),
            Builtin::YulClz
            | Builtin::YulCallcode
            | Builtin::YulExtcall
            | Builtin::YulExtdelegatecall
            | Builtin::YulExtstaticcall => {
                self.unsupported_yul_builtin(builder, builtin, args.span)
            }
            _ => unreachable!("non-Yul builtin passed to Yul lowering"),
        }
    }

    fn yul_builtin_arity(builtin: Builtin) -> Option<usize> {
        Some(match builtin {
            Builtin::YulStop
            | Builtin::YulAddress
            | Builtin::YulSelfbalance
            | Builtin::YulCaller
            | Builtin::YulCallvalue
            | Builtin::YulCalldatasize
            | Builtin::YulCodesize
            | Builtin::YulReturndatasize
            | Builtin::YulMsize
            | Builtin::YulGas
            | Builtin::YulInvalid
            | Builtin::YulChainid
            | Builtin::YulBasefee
            | Builtin::YulBlobbasefee
            | Builtin::YulCoinbase
            | Builtin::YulDifficulty
            | Builtin::YulPrevrandao
            | Builtin::YulGaslimit
            | Builtin::YulNumber
            | Builtin::YulTimestamp
            | Builtin::YulGasprice
            | Builtin::YulOrigin => 0,
            Builtin::YulNot
            | Builtin::YulIszero
            | Builtin::YulClz
            | Builtin::YulBalance
            | Builtin::YulCalldataload
            | Builtin::YulExtcodesize
            | Builtin::YulExtcodehash
            | Builtin::YulMload
            | Builtin::YulSload
            | Builtin::YulTload
            | Builtin::YulBlobhash
            | Builtin::YulBlockhash
            | Builtin::YulPop
            | Builtin::YulSelfdestruct => 1,
            Builtin::YulAdd
            | Builtin::YulSub
            | Builtin::YulMul
            | Builtin::YulDiv
            | Builtin::YulMod
            | Builtin::YulExp
            | Builtin::YulAnd
            | Builtin::YulOr
            | Builtin::YulXor
            | Builtin::YulShl
            | Builtin::YulShr
            | Builtin::YulSar
            | Builtin::YulSdiv
            | Builtin::YulSmod
            | Builtin::YulLt
            | Builtin::YulGt
            | Builtin::YulSlt
            | Builtin::YulSgt
            | Builtin::YulEq
            | Builtin::YulByte
            | Builtin::YulSignextend
            | Builtin::YulKeccak256
            | Builtin::YulMstore
            | Builtin::YulMstore8
            | Builtin::YulSstore
            | Builtin::YulTstore
            | Builtin::YulLog0
            | Builtin::YulReturn
            | Builtin::YulRevert => 2,
            Builtin::YulAddmod
            | Builtin::YulMulmod
            | Builtin::YulCalldatacopy
            | Builtin::YulCodecopy
            | Builtin::YulReturndatacopy
            | Builtin::YulMcopy
            | Builtin::YulLog1
            | Builtin::YulCreate
            | Builtin::YulExtdelegatecall
            | Builtin::YulExtstaticcall => 3,
            Builtin::YulExtcodecopy
            | Builtin::YulLog2
            | Builtin::YulCreate2
            | Builtin::YulExtcall => 4,
            Builtin::YulLog3 => 5,
            Builtin::YulDelegatecall | Builtin::YulStaticcall | Builtin::YulLog4 => 6,
            Builtin::YulCall | Builtin::YulCallcode => 7,
            _ => return None,
        })
    }

    fn unsupported_yul_builtin(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        builtin: Builtin,
        span: Span,
    ) -> ValueId {
        self.err_value(builder, span, format!("unsupported Yul builtin `{}`", builtin.name()))
    }

    /// Lowers a member function call (e.g., counter.increment()).
    fn lower_member_call_with_opts(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        callee: &hir::Expr<'_>,
        base: &hir::Expr<'_>,
        member: Ident,
        args: &CallArgs<'_>,
        call_opts: Option<&[hir::NamedArg<'_>]>,
    ) -> ValueId {
        let resolved = self.gcx.resolved_callee(callee.id);
        let builtin = self.gcx.builtin_callee(callee.id);

        if let Some(builtin) = builtin
            && Self::builtin_uses_direct_call_lowering(builtin)
        {
            return self.lower_builtin_call(builder, builtin, args);
        }

        // Handle `Contract.StructType(args)`.
        if let Some(resolved) = resolved
            && let hir::Res::Item(hir::ItemId::Struct(struct_id)) = resolved.res
        {
            return self.lower_struct_constructor(builder, struct_id, args);
        }

        // Handle enum conversion written as `Container.Enum(x)`. An enum value is
        // represented by its integer, so — like the `Ident` enum-callee path in
        // `lower_call` — the conversion is the identity on the argument.
        if let Some(resolved) = resolved
            && let hir::Res::Item(hir::ItemId::Enum(_)) = resolved.res
            && let Some(arg) = args.exprs().next()
        {
            return self.lower_expr(builder, arg);
        }

        // Handle library function calls: Library.func(args).
        if self.is_library_type_expr(base)
            && let Some(func_id) = self.resolved_function_callee(callee)
        {
            return self.lower_library_call(builder, func_id, args, None);
        }

        // Handle address payable transfer/send builtins
        if matches!(builtin, Some(Builtin::AddressPayableTransfer | Builtin::AddressPayableSend)) {
            // payable(addr).transfer(amount) or payable(addr).send(amount)
            // CALL(2300, addr, amount, 0, 0, 0, 0)
            let addr = self.lower_expr(builder, base);
            let mut exprs = args.exprs();
            let amount = if let Some(first) = exprs.next() {
                self.lower_expr(builder, first)
            } else {
                builder.imm_u64(0)
            };

            // transfer/send uses 2300 gas stipend
            let gas_stipend = builder.imm_u64(2300);
            // Create fresh zero values for each CALL argument to avoid stack issues
            let zero_args_offset = builder.imm_u64(0);
            let zero_args_size = builder.imm_u64(0);
            let zero_ret_offset = builder.imm_u64(0);
            let zero_ret_size = builder.imm_u64(0);

            // CALL(gas, addr, value, argsOffset, argsSize, retOffset, retSize)
            let success = builder.call(
                gas_stipend,
                addr,
                amount,
                zero_args_offset,
                zero_args_size,
                zero_ret_offset,
                zero_ret_size,
            );

            if builtin == Some(Builtin::AddressPayableTransfer) {
                // transfer reverts on failure
                let is_failure = builder.iszero(success);
                let revert_block = builder.create_block();
                let continue_block = builder.create_block();
                builder.branch(is_failure, revert_block, continue_block);
                builder.switch_to_block(revert_block);
                let revert_offset = builder.imm_u64(0);
                let revert_size = builder.imm_u64(0);
                builder.revert(revert_offset, revert_size);
                builder.switch_to_block(continue_block);
                return builder.imm_u64(0);
            }
            // send returns success bool
            return success;
        }

        // Handle low-level call/staticcall/delegatecall
        // addr.call{value: X}(data) returns (bool success, bytes memory returndata)
        // addr.staticcall(data) returns (bool success, bytes memory returndata)
        // addr.delegatecall(data) returns (bool success, bytes memory returndata)
        if matches!(
            builtin,
            Some(Builtin::AddressCall | Builtin::AddressStaticcall | Builtin::AddressDelegatecall)
        ) {
            let addr = self.lower_expr(builder, base);

            // Get the calldata bytes argument.
            let mut exprs = args.exprs();
            let (calldata_offset, calldata_size) = if let Some(data_arg) = exprs.next() {
                // Supported inputs are literals and ABI encode calls. Other
                // bytes expressions panic in `lower_bytes_arg_to_memory`.
                self.lower_bytes_arg_to_memory(builder, data_arg)
            } else {
                // No argument means empty calldata
                (builder.imm_u64(0), builder.imm_u64(0))
            };

            // Gas: use all available gas
            let gas = builder.gas();

            // Value: extract from call options {value: X} or default to 0
            let value = if builtin == Some(Builtin::AddressCall) {
                self.extract_call_value(builder, call_opts)
            } else {
                // staticcall and delegatecall don't transfer value
                builder.imm_u64(0)
            };

            // This lowering models only the success flag. Solidity's second
            // `bytes` result is rejected by `lower_multi_var_decl` until the
            // compiler materializes returndata bytes.
            let ret_offset = builder.imm_u64(0);
            let ret_size = builder.imm_u64(0);

            // Emit the appropriate CALL/STATICCALL/DELEGATECALL instruction
            let success = match builtin {
                Some(Builtin::AddressCall) => builder.call(
                    gas,
                    addr,
                    value,
                    calldata_offset,
                    calldata_size,
                    ret_offset,
                    ret_size,
                ),
                Some(Builtin::AddressStaticcall) => builder.staticcall(
                    gas,
                    addr,
                    calldata_offset,
                    calldata_size,
                    ret_offset,
                    ret_size,
                ),
                Some(Builtin::AddressDelegatecall) => builder.delegatecall(
                    gas,
                    addr,
                    calldata_offset,
                    calldata_size,
                    ret_offset,
                    ret_size,
                ),
                _ => unreachable!(),
            };

            // Low-level calls return `(bool, bytes)`, but this expression path
            // exposes only the first value. `lower_multi_var_decl` copies the
            // returndata bytes out of the return buffer when they are bound.
            return success;
        }

        let array_method = builtin.and_then(Self::array_builtin_method_name);

        // Handle storage `bytes`/`string` methods before the generic member
        // call path. Their storage layout is Solidity's packed short/long
        // bytes form, not the generic dynamic-array layout. The receiver may be
        // a state variable, a storage-reference local, or a `bytes` field
        // reached through one (`state.part.push(b)`); `lower_lvalue_slot`
        // resolves the slot for all of these.
        if self.expr_is_storage_bytes_lvalue(base)
            && let Some(method) = array_method
            && let Some(slot) = self.lower_lvalue_slot(builder, base)
        {
            return self.lower_storage_bytes_method_call(builder, slot, method, args);
        }

        // Handle dynamic array methods (push, pop)
        if let Some(method) = array_method
            && let Some((var_id, slot)) = self.get_dyn_array_base_slot(base)
        {
            return self.lower_array_method_call(builder, var_id, slot, method, args);
        }

        // Handle `using X for Y` library calls: x.method(args) -> Library.method(x, args)
        if let Some(resolved) = resolved
            && resolved.attached
            && let hir::Res::Item(hir::ItemId::Function(func_id)) = resolved.res
        {
            let bound_arg = self.lower_expr(builder, base);
            return self.lower_library_call(builder, func_id, args, Some(bound_arg));
        }

        // Look up the function being called to get its selector and return count.
        let resolved_func = self.resolved_function_callee(callee);
        if resolved_func.is_none() && self.gcx.has_typeck_results() {
            // The callee is unresolved: either a prior error left the receiver
            // untyped, or it is a member call on a receiver shape codegen does
            // not handle yet (e.g. `push`/`pop` on a nested or mapping-nested
            // array). Report it instead of asserting the typeck invariant.
            return self.err_value(
                builder,
                member.span,
                format!("codegen does not support this `.{member}` member call yet"),
            );
        }
        let (selector, num_returns, struct_return_info) = if let Some(func_id) = resolved_func {
            (
                u32::from_be_bytes(self.gcx.function_selector(func_id).0),
                self.function_return_slot_count(func_id),
                self.function_struct_return(func_id),
            )
        } else {
            (
                self.compute_member_selector(base, member),
                self.get_member_function_return_count(base, member),
                None,
            )
        };

        // Use the recursive ABI encoder for every high-level call. The former
        // shallow struct loop copied nested memory pointers as calldata words
        // and treated dynamic bytes pointers as their encoded value.
        let arg_exprs: Vec<_> = args.exprs().collect();
        let selector_word = builder.imm_u256(U256::from(selector) << 224);
        let Some((calldata_start, calldata_size)) =
            self.abi_encode_call_payload(builder, Some(selector_word), &arg_exprs)
        else {
            return self.err_value(
                builder,
                callee.span,
                "codegen cannot determine external call argument types",
            );
        };

        // Evaluate the address and spill it to scratch memory at 0x00.
        // This ensures it survives all the MSTORE operations for calldata setup.
        // We reload it right before the CALL.
        let addr_expr = self.lower_expr(builder, base);
        let scratch_addr = builder.imm_u64(0x00);
        builder.mstore(scratch_addr, addr_expr);

        // Store calldata_start to scratch memory at 0x20.
        // We need to reload it right before the CALL because:
        // 1. The scheduler may lose track of this value after many MSTOREs
        // 2. For struct returns, we update the free memory pointer, so reading 0x40 again would be
        //    wrong
        let scratch_calldata = builder.imm_u64(0x20);
        builder.mstore(scratch_calldata, calldata_start);

        // Determine where to store return data and whether it's a struct
        let (ret_offset, ret_size, struct_ptr_opt) =
            if let Some((_struct_id, field_count)) = struct_return_info {
                // For struct returns, reserve a separate output allocation.
                let struct_size = (field_count as u64) * 32;
                let struct_size_val = builder.imm_u64(struct_size);
                let struct_ptr = builder.alloc_object(
                    struct_size_val,
                    crate::mir::MemoryObjectLayout::Struct { fields: field_count as u64 },
                    crate::mir::AllocationSemantics::INTERNAL,
                );

                let ret_size = builder.imm_u64(struct_size);
                (struct_ptr, ret_size, Some(struct_ptr))
            } else {
                // Reuse the unbumped calldata allocation for return data. CALL
                // has consumed the input before writing output, and the base
                // remains published in the multi-return pointer scratch word.
                let ret_offset = if num_returns > 1 { calldata_start } else { builder.imm_u64(0) };
                let ret_size = builder.imm_u64((num_returns * 32) as u64);
                (ret_offset, ret_size, None)
            };

        // Value: extract from call options {value: X} or default to 0
        let value = self.extract_call_value(builder, call_opts);

        // Reload the address from scratch memory (0x00) where we stored it earlier.
        // This avoids stack depth issues after all the MSTORE operations.
        let scratch_addr_reload = builder.imm_u64(0x00);
        let addr = builder.mload(scratch_addr_reload);

        // Gas: use all available gas (must be right before CALL to be on top of stack)
        let gas = builder.gas();

        // Reload calldata_start from scratch memory at 0x20.
        // Cannot re-read from 0x40 because struct return handling may have updated it.
        let scratch_calldata_reload = builder.imm_u64(0x20);
        let calldata_start_reload = builder.mload(scratch_calldata_reload);

        // Emit the CALL instruction
        let success = builder.call(
            gas,
            addr,
            value,
            calldata_start_reload,
            calldata_size,
            ret_offset,
            ret_size,
        );

        // High-level calls bubble the callee's revert data.
        let failed = builder.iszero(success);
        let fail_block = builder.create_block();
        let continue_block = builder.create_block();
        builder.branch(failed, fail_block, continue_block);
        builder.switch_to_block(fail_block);
        let zero = builder.imm_u64(0);
        let size = builder.returndatasize();
        builder.returndatacopy(zero, zero, size);
        builder.revert(zero, size);
        builder.switch_to_block(continue_block);

        // For struct returns, the data is already in the right place (at struct_ptr).
        // Just return the pointer.
        if let Some(struct_ptr) = struct_ptr_opt {
            return struct_ptr;
        }

        // Load first return value from memory
        // Multi-return consumers snapshot additional words from the ephemeral
        // buffer at `ret_offset` before lowering any lvalues.
        builder.mload(ret_offset)
    }

    fn resolved_function_callee(&self, callee: &hir::Expr<'_>) -> Option<hir::FunctionId> {
        let resolved = self.gcx.resolved_callee(callee.id)?;
        let hir::Res::Item(hir::ItemId::Function(func_id)) = resolved.res else { return None };
        Some(func_id)
    }

    fn is_library_type_expr(&self, expr: &hir::Expr<'_>) -> bool {
        let Some(ty) = self.get_expr_type(expr) else { return false };
        let TyKind::Type(ty) = ty.kind else { return false };
        let TyKind::Contract(contract_id) = ty.kind else { return false };
        self.gcx.hir.contract(contract_id).kind.is_library()
    }

    fn array_builtin_method_name(builtin: Builtin) -> Option<Symbol> {
        match builtin {
            Builtin::ArrayPush0 | Builtin::ArrayPush => Some(sym::push),
            Builtin::ArrayPop => Some(kw::Pop),
            _ => None,
        }
    }

    fn function_return_slot_count(&self, func_id: hir::FunctionId) -> usize {
        self.return_slot_count(self.gcx.hir.function(func_id).returns)
    }

    fn return_slot_count(&self, returns: &[hir::VariableId]) -> usize {
        let mut total = 0;
        for &var_id in returns {
            let var = self.gcx.hir.variable(var_id);
            if let hir::TypeKind::Custom(hir::ItemId::Struct(struct_id)) = &var.ty.kind {
                total += self.gcx.hir.strukt(*struct_id).fields.len();
            } else {
                total += 1;
            }
        }
        total.max(1)
    }

    fn function_struct_return(&self, func_id: hir::FunctionId) -> Option<(hir::StructId, usize)> {
        self.struct_return(self.gcx.hir.function(func_id).returns)
    }

    fn struct_return(&self, returns: &[hir::VariableId]) -> Option<(hir::StructId, usize)> {
        if returns.len() == 1 {
            let var = self.gcx.hir.variable(returns[0]);
            if let hir::TypeKind::Custom(hir::ItemId::Struct(struct_id)) = &var.ty.kind {
                return Some((*struct_id, self.gcx.hir.strukt(*struct_id).fields.len()));
            }
        }
        None
    }

    /// Extracts the `value` from call options `{value: X}`, or returns 0 if not present.
    pub(super) fn extract_call_value(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        call_opts: Option<&[hir::NamedArg<'_>]>,
    ) -> ValueId {
        if let Some(opts) = call_opts {
            for opt in opts {
                if opt.name.name == sym::value {
                    return self.lower_expr(builder, &opt.value);
                }
            }
        }
        builder.imm_u64(0)
    }

    /// Computes the function selector for a member call.
    pub(super) fn compute_member_selector(&self, base: &hir::Expr<'_>, member: Ident) -> u32 {
        // Try to get the type of the base expression and find the function
        // For contract types, we look up the function in the contract's interface

        // Helper to look up selector from a contract, including inherited functions.
        // Searches through the linearized inheritance chain.
        let lookup_in_contract = |contract_id: hir::ContractId| -> Option<u32> {
            let contract = self.gcx.hir.contract(contract_id);
            // Search through the inheritance chain (linearized_bases includes self at index 0)
            for &base_id in contract.linearized_bases.iter() {
                let base_contract = self.gcx.hir.contract(base_id);
                for func_id in base_contract.all_functions() {
                    let func = self.gcx.hir.function(func_id);
                    if func.name.is_some_and(|n| n.name == member.name) {
                        let selector = self.gcx.function_selector(func_id);
                        return Some(u32::from_be_bytes(selector.0));
                    }
                }
            }
            None
        };

        // Case 1: base is an identifier (variable with contract type)
        if let ExprKind::Ident(res_slice) = &base.kind
            && let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first()
        {
            let var = self.gcx.hir.variable(*var_id);
            let ty = self.gcx.type_of_hir_ty(&var.ty);
            if let solar_sema::ty::TyKind::Contract(contract_id) = ty.kind
                && let Some(sel) = lookup_in_contract(contract_id)
            {
                return sel;
            }
        }

        // Case 2: base is a type conversion call like ICallee(addr)
        // The call's callee is an Ident resolving to a Contract/Interface
        if let ExprKind::Call(callee, _args, _named) = &base.kind
            && let ExprKind::Ident(res_slice) = &callee.kind
            && let Some(hir::Res::Item(hir::ItemId::Contract(contract_id))) = res_slice.first()
            && let Some(sel) = lookup_in_contract(*contract_id)
        {
            return sel;
        }

        // Case 2b: base is the contract/interface name itself, e.g.
        // `IERC20Minimal.transfer.selector`.
        if let ExprKind::Ident(res_slice) = &base.kind
            && let Some(hir::Res::Item(hir::ItemId::Contract(contract_id))) = res_slice.first()
            && let Some(sel) = lookup_in_contract(*contract_id)
        {
            return sel;
        }

        // Case 3: base is `this` (Builtin::This)
        if let ExprKind::Ident(res_slice) = &base.kind
            && let Some(hir::Res::Builtin(Builtin::This)) = res_slice.first()
            && let Some(contract_id) = self.current_contract_id
            && let Some(sel) = lookup_in_contract(contract_id)
        {
            return sel;
        }

        // Fallback: compute selector from member name
        // This is a simplified version - proper implementation would use full signature
        let sig = format!("{}()", member.name);
        let hash = alloy_primitives::keccak256(sig.as_bytes());
        u32::from_be_bytes(hash[..4].try_into().unwrap())
    }

    /// Gets the number of return values for a member function call.
    pub(super) fn get_member_function_return_count(
        &self,
        base: &hir::Expr<'_>,
        member: Ident,
    ) -> usize {
        // Helper to count the number of 32-byte slots a return type occupies.
        // Structs are expanded to their number of fields.
        let count_return_slots = |returns: &[hir::VariableId]| -> usize {
            let mut total = 0;
            for &var_id in returns {
                let var = self.gcx.hir.variable(var_id);
                if let hir::TypeKind::Custom(hir::ItemId::Struct(struct_id)) = &var.ty.kind {
                    // Struct: count its fields
                    let strukt = self.gcx.hir.strukt(*struct_id);
                    total += strukt.fields.len();
                } else {
                    // Non-struct: 1 slot
                    total += 1;
                }
            }
            total.max(1)
        };

        // Helper to look up return count from a contract, including inherited functions.
        // Searches through the linearized inheritance chain.
        let lookup_in_contract = |contract_id: hir::ContractId| -> Option<usize> {
            let contract = self.gcx.hir.contract(contract_id);
            // Search through the inheritance chain (linearized_bases includes self at index 0)
            for &base_id in contract.linearized_bases.iter() {
                let base_contract = self.gcx.hir.contract(base_id);
                for func_id in base_contract.all_functions() {
                    let func = self.gcx.hir.function(func_id);
                    if func.name.is_some_and(|n| n.name == member.name) {
                        return Some(count_return_slots(func.returns));
                    }
                }
            }
            None
        };

        // Case 1: base is an identifier (variable with contract type)
        if let ExprKind::Ident(res_slice) = &base.kind
            && let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first()
        {
            let var = self.gcx.hir.variable(*var_id);
            let ty = self.gcx.type_of_hir_ty(&var.ty);
            if let solar_sema::ty::TyKind::Contract(contract_id) = ty.kind
                && let Some(count) = lookup_in_contract(contract_id)
            {
                return count;
            }
        }

        // Case 2: base is a type conversion call like ICallee(addr)
        if let ExprKind::Call(callee, _args, _named) = &base.kind
            && let ExprKind::Ident(res_slice) = &callee.kind
            && let Some(hir::Res::Item(hir::ItemId::Contract(contract_id))) = res_slice.first()
            && let Some(count) = lookup_in_contract(*contract_id)
        {
            return count;
        }

        // Case 3: base is `this` (Builtin::This)
        if let ExprKind::Ident(res_slice) = &base.kind
            && let Some(hir::Res::Builtin(Builtin::This)) = res_slice.first()
        {
            // Look up the function in the current contract
            // We need to find it through the module's functions
            // Search all known contracts because `this` carries the current
            // contract value rather than a specific function declaration.
            for contract_id in self.gcx.hir.contract_ids() {
                if let Some(count) = lookup_in_contract(contract_id) {
                    return count;
                }
            }
        }

        // Unknown member calls are treated as single-value calls.
        1
    }

    /// Whether a parameter is a storage reference — a `mapping` (always storage)
    /// or any type declared with the `storage` data location. Such parameters are
    /// passed by slot number rather than by value.
    pub(super) fn param_is_storage_ref(&self, param_id: hir::VariableId) -> bool {
        let var = self.gcx.hir.variable(param_id);
        matches!(var.ty.kind, hir::TypeKind::Mapping(_))
            || var.data_location == Some(solar_ast::DataLocation::Storage)
    }

    /// Lowers an internal function call by inlining it.
    /// This handles calls like `add(a, b)` where `add` is a function in the same contract.
    fn lower_internal_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        func_id: hir::FunctionId,
        args: &CallArgs<'_>,
    ) -> ValueId {
        let func = self.gcx.hir.function(func_id);

        // Collect argument values FIRST (before entering inline tracking)
        // This allows nested calls to the same function (e.g., add(add(x, 1), 2))
        // because we evaluate arguments before marking ourselves as "in progress".
        //
        // A storage-reference parameter (a `mapping`, or an array/struct in
        // `storage`) is passed by slot number, so such an argument is lowered to
        // its storage slot rather than as a value — lowering it as a value would
        // `sload` the slot and pass the wrong thing.
        let params = func.parameters;
        let arg_vals: Vec<ValueId> = args
            .exprs()
            .enumerate()
            .map(|(i, arg)| {
                if params.get(i).is_some_and(|&p| self.param_is_storage_ref(p))
                    && let Some(slot) = self.lower_lvalue_slot(builder, arg)
                {
                    slot
                } else {
                    // A `bytes memory` parameter receives one word; an
                    // ABI-encode payload (a memory slice) materializes first.
                    let value = self.lower_expr(builder, arg);
                    self.coerce_memory_slice_value(builder, value)
                }
            })
            .collect();

        // A callee that takes a storage-reference parameter must be lowered
        // through the internal-frame path, whose normal statement lowering binds
        // storage-reference locals correctly. The straight-line SSA inline path
        // (`lower_library_body_simple`) lowers a `T storage x = ...;` declaration
        // as a memory value and would miscompile subsequent field/element reads.
        let has_storage_ref_param = params.iter().any(|&p| self.param_is_storage_ref(p));

        // `-O size`: lowering-time inlining duplicates the body at every call
        // site with no call-count awareness. Emit a shared call instead — with
        // static frames and stack return addresses a call site costs a few
        // bytes, so sharing multi-use helpers now shrinks the output (the
        // opposite held under the old memory-frame protocol).
        let size_mode = self.gcx.sess.opts.optimization.is_size();

        if func.returns.is_empty() {
            if size_mode || has_storage_ref_param || self.function_is_recursive(func_id) {
                return self.lower_internal_call_fallback(builder, func_id, arg_vals);
            }
            return self.lower_inline_void_call(builder, func_id, arg_vals);
        }

        // A `bytes`/`string` calldata slice return crosses the internal-call
        // boundary as an `(offset, length)` pair, which slice lowering does not
        // expand on the return side; a real `internal_call` would leave a slice
        // the backend cannot lower. Inline the callee instead so its named slice
        // return is reconstructed at the call site (where it folds away). This
        // is the `bytes calldata` helper idiom (`_emptyData`, `emptySignature`).
        if self.returns_calldata_slice(func) {
            return self.lower_calldata_slice_return_call(
                builder,
                func_id,
                arg_vals,
                has_storage_ref_param,
            );
        }

        // The SSA inline path (`lower_library_body_simple`) only models a
        // straight-line body that ends in a `return`. Anything else — a loop, an
        // `if`, a multi-statement control flow — is lowered as a real
        // `internal_call` instead, where the memory-backed internal frame handles
        // reassigned locals, loops, and recursion correctly. Recursive functions
        // with a simple ternary body (which `is_simple_return_function` accepts)
        // are caught separately so inlining does not hit a recursive cycle.
        // Simple, non-recursive functions still inline. Internal/private callees
        // use the internal-frame convention directly; a public callee is compiled
        // for the external ABI, so it needs an internal-frame copy
        // (`ensure_internal_mir_function`) for `internal_call` to target.
        let needs_call = size_mode
            || has_storage_ref_param
            || !Self::is_simple_return_function(func)
            || self.function_is_recursive(func_id);
        if needs_call {
            return self.lower_internal_call_fallback(builder, func_id, arg_vals);
        }

        // Check for recursive inlining cycle AFTER evaluating arguments.
        if !self.try_enter_inline(func_id) {
            return self.lower_internal_call_fallback(builder, func_id, arg_vals);
        }

        // Save current locals
        let saved_locals = std::mem::take(&mut self.locals);

        // Bind parameters to argument values directly (SSA style)
        for (i, &param_id) in func.parameters.iter().enumerate() {
            if let Some(&arg_val) = arg_vals.get(i) {
                self.locals.insert(param_id, arg_val);
            }
        }

        // For simple functions with a single return statement, extract and evaluate directly
        let result = if let Some(body) = &func.body {
            self.lower_library_body_simple(builder, body, func)
        } else {
            builder.imm_u64(0)
        };

        // Restore locals
        self.locals = saved_locals;

        // Exit inline tracking
        self.exit_inline();

        result
    }

    /// Whether every return of `func` is a `bytes`/`string` calldata slice.
    fn returns_calldata_slice(&self, func: &hir::Function<'_>) -> bool {
        !func.returns.is_empty()
            && func
                .returns
                .iter()
                .all(|&id| Self::calldata_dynamic_var_kind(self.gcx.hir.variable(id)).is_some())
    }

    /// A body whose statements are straight-line — declarations, expressions,
    /// assembly, and returns, with no statement-level control flow. Reading a
    /// named return from `locals` after such a body observes its final value
    /// with no branch merge to reconstruct, so inlining it is sound.
    fn is_straight_line_body(body: &hir::Block<'_>) -> bool {
        body.stmts.iter().all(|stmt| {
            matches!(
                stmt.kind,
                hir::StmtKind::DeclSingle(_)
                    | hir::StmtKind::Expr(_)
                    | hir::StmtKind::AssemblyBlock(_)
                    | hir::StmtKind::Return(_)
            )
        })
    }

    /// Whether any statement in `body`, at any depth, is an explicit `return`.
    /// Control-flow inlining lowers the body through [`Self::lower_block`], which
    /// would turn such a `return` into a terminator that returns from the
    /// caller, so a body with one cannot be inlined that way.
    fn body_has_return(body: &hir::Block<'_>) -> bool {
        body.stmts.iter().any(Self::stmt_has_return)
    }

    fn stmt_has_return(stmt: &hir::Stmt<'_>) -> bool {
        use hir::StmtKind;
        match &stmt.kind {
            StmtKind::Return(_) => true,
            StmtKind::Block(b) | StmtKind::UncheckedBlock(b) | StmtKind::AssemblyBlock(b) => {
                Self::body_has_return(b)
            }
            StmtKind::If(_, then_stmt, else_stmt) => {
                Self::stmt_has_return(then_stmt)
                    || else_stmt.is_some_and(Self::stmt_has_return)
            }
            StmtKind::Loop(b, _) => Self::body_has_return(b),
            StmtKind::Switch(sw) => sw.cases.iter().any(|case| Self::body_has_return(&case.body)),
            StmtKind::Try(t) => t.clauses.iter().any(|clause| Self::body_has_return(&clause.block)),
            _ => false,
        }
    }

    /// Lowers a call to an internal function that returns a calldata slice by
    /// inlining its body, so the returned slice is a `make_slice` at the call
    /// site that folds away. A straight-line body inlines through the
    /// simple-return path; a control-flow body without explicit returns inlines
    /// through full block lowering, its named-return slices merging across
    /// branches through their two-word slots. A callee that cannot be inlined —
    /// a storage-reference parameter, an explicit return under control flow, or
    /// recursion — is reported instead of lowered to a slice the backend cannot
    /// handle.
    fn lower_calldata_slice_return_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        func_id: hir::FunctionId,
        arg_vals: Vec<ValueId>,
        has_storage_ref_param: bool,
    ) -> ValueId {
        let func = self.gcx.hir.function(func_id);
        if let Some(body) = func.body
            && !has_storage_ref_param
            && !self.function_is_recursive(func_id)
        {
            if Self::is_straight_line_body(&body) && self.try_enter_inline(func_id) {
                let saved_locals = std::mem::take(&mut self.locals);
                for (i, &param_id) in func.parameters.iter().enumerate() {
                    if let Some(&arg_val) = arg_vals.get(i) {
                        self.locals.insert(param_id, arg_val);
                    }
                }
                let result = self.lower_library_body_simple(builder, &body, func);
                self.locals = saved_locals;
                self.exit_inline();
                return result;
            }
            if !Self::body_has_return(&body) && self.try_enter_inline(func_id) {
                let result =
                    self.inline_calldata_slice_control_flow(builder, func, &body, &arg_vals);
                self.exit_inline();
                return result;
            }
        }
        let guar = self
            .gcx
            .dcx()
            .err("returning a `bytes`/`string` calldata slice from this internal function is not yet supported in codegen")
            .span(func.span)
            .emit();
        builder.error_value(guar)
    }

    /// Inlines a calldata-slice-returning function whose body has statement-level
    /// control flow but no explicit `return`. The full body is lowered through
    /// [`Self::lower_block`] so branches lower correctly; each named calldata
    /// slice return is given a two-word slot seeded with an empty slice, so a
    /// slice reassigned on only one path merges through memory. The first
    /// return's slice value is handed back for the call site to consume.
    fn inline_calldata_slice_control_flow(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        func: &hir::Function<'_>,
        body: &hir::Block<'_>,
        arg_vals: &[ValueId],
    ) -> ValueId {
        let saved_locals = std::mem::take(&mut self.locals);
        let saved_local_memory_slots = std::mem::take(&mut self.local_memory_slots);
        let saved_next_local_memory_offset = self.next_local_memory_offset;
        let saved_assigned_vars = std::mem::take(&mut self.assigned_vars);
        let saved_slice_slot_locals = std::mem::take(&mut self.slice_slot_locals);

        self.collect_assigned_vars_block(body);

        // Every named calldata slice return lives in a slot so a value assigned
        // on only one arm of a branch is merged there, not left leaking. An
        // empty seed makes an unwritten return read as empty rather than junk.
        for &ret_id in func.returns {
            if Self::calldata_dynamic_var_kind(self.gcx.hir.variable(ret_id)).is_some() {
                let offset = self.alloc_local_slice_memory(ret_id);
                let zero = builder.imm_u64(0);
                let empty = builder.make_slice(zero, zero, crate::mir::SliceLocation::Calldata);
                self.store_slice_slot(builder, offset, empty);
            }
        }

        for (i, &param_id) in func.parameters.iter().enumerate() {
            if let Some(&arg_val) = arg_vals.get(i) {
                self.locals.insert(param_id, arg_val);
            }
        }

        let saved_in_unchecked_block = self.in_unchecked_block;
        self.in_unchecked_block = false;
        self.lower_block(builder, body);
        self.in_unchecked_block = saved_in_unchecked_block;

        // Read the first named return through its slot before caller state is
        // restored; the loaded slice value stays valid afterwards.
        let result = func
            .returns
            .first()
            .map(|&ret_id| {
                if self.is_slice_slot_local(&ret_id)
                    && let Some(offset) = self.get_local_memory_offset(&ret_id)
                {
                    self.load_slice_slot(builder, offset, crate::mir::SliceLocation::Calldata)
                } else {
                    self.locals.get(&ret_id).copied().unwrap_or_else(|| builder.imm_u64(0))
                }
            })
            .unwrap_or_else(|| builder.imm_u64(0));

        self.locals = saved_locals;
        self.local_memory_slots = saved_local_memory_slots;
        self.next_local_memory_offset = saved_next_local_memory_offset;
        self.assigned_vars = saved_assigned_vars;
        self.slice_slot_locals = saved_slice_slot_locals;

        result
    }

    fn lower_internal_call_fallback(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        func_id: hir::FunctionId,
        arg_vals: Vec<ValueId>,
    ) -> ValueId {
        self.lower_internal_call_fallback_inner(builder, func_id, arg_vals)
    }

    fn lower_internal_call_fallback_inner(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        func_id: hir::FunctionId,
        arg_vals: Vec<ValueId>,
    ) -> ValueId {
        let func = self.gcx.hir.function(func_id);
        let result_ty = func
            .returns
            .first()
            .map(|&ret_id| self.lower_type_from_var(self.gcx.hir.variable(ret_id)));
        let is_internal =
            matches!(func.visibility, hir::Visibility::Internal | hir::Visibility::Private);
        let mir_id = if is_internal {
            self.ensure_function_lowered(func_id)
        } else {
            self.ensure_internal_mir_function(func_id)
        };
        let Some(result_ty) = result_ty else {
            // Void call: the instruction produces no value, so hand back a
            // placeholder for the expression position, which is never read.
            builder.internal_call_void(mir_id, arg_vals, func.returns.len());
            return builder.imm_u64(0);
        };
        builder.internal_call(mir_id, arg_vals, result_ty, func.returns.len())
    }

    /// Lowers a base constructor call using already-resolved constructor arguments.
    pub(super) fn lower_base_constructor_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        ctor_id: hir::FunctionId,
        modifier: Option<&hir::Modifier<'_>>,
    ) -> ValueId {
        let ctor = self.gcx.hir.function(ctor_id);
        let arg_exprs: Vec<_> = modifier.map(|m| m.args.exprs().collect()).unwrap_or_default();
        let arg_vals: Vec<ValueId> = ctor
            .parameters
            .iter()
            .enumerate()
            .map(|(i, &param_id)| {
                let param = self.gcx.hir.variable(param_id);
                if let Some(arg) = arg_exprs.get(i) {
                    self.lower_constructor_arg(builder, arg, &param.ty)
                } else {
                    builder.imm_u64(0)
                }
            })
            .collect();

        self.lower_inline_void_call(builder, ctor_id, arg_vals)
    }

    /// Lowers a void internal function by inlining its full statement body.
    fn lower_inline_void_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        func_id: hir::FunctionId,
        arg_vals: Vec<ValueId>,
    ) -> ValueId {
        let func = self.gcx.hir.function(func_id);
        let parameters: Vec<_> = func.parameters.to_vec();
        let body = func.body;

        if !self.try_enter_inline(func_id) {
            {
                let guar = self
                    .gcx
                    .dcx()
                    .err("codegen does not support this recursive call through inlining yet")
                    .emit();
                return builder.error_value(guar);
            }
        }

        let saved_locals = std::mem::take(&mut self.locals);
        let saved_local_memory_slots = std::mem::take(&mut self.local_memory_slots);
        let saved_next_local_memory_offset = self.next_local_memory_offset;
        let saved_assigned_vars = std::mem::take(&mut self.assigned_vars);

        if let Some(body) = body {
            self.collect_assigned_vars_block(&body);
        }

        for (i, param_id) in parameters.into_iter().enumerate() {
            if let Some(&arg_val) = arg_vals.get(i) {
                self.locals.insert(param_id, arg_val);
            }
        }

        if let Some(body) = body {
            let saved_in_unchecked_block = self.in_unchecked_block;
            self.in_unchecked_block = false;
            self.lower_block(builder, &body);
            self.in_unchecked_block = saved_in_unchecked_block;
        }

        self.locals = saved_locals;
        self.local_memory_slots = saved_local_memory_slots;
        self.next_local_memory_offset = saved_next_local_memory_offset;
        self.assigned_vars = saved_assigned_vars;
        self.exit_inline();

        builder.imm_u64(0)
    }

    /// Lowers constructor arguments into the representation expected by the
    /// callee body. Memory `bytes`/`string` parameters receive Solidity's
    /// `[length][data...]` memory pointer, including literal base-constructor
    /// arguments such as `ERC20("Name", "SYM")`.
    fn lower_constructor_arg(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        arg: &hir::Expr<'_>,
        param_ty: &hir::Type<'_>,
    ) -> ValueId {
        if matches!(
            param_ty.kind,
            hir::TypeKind::Elementary(hir::ElementaryType::String | hir::ElementaryType::Bytes)
        ) {
            return self.lower_expr_as_memory_bytes(builder, arg);
        }

        self.lower_expr(builder, arg)
    }

    /// Returns the linked address of the library that defines `func_id`, when
    /// one was supplied via `--libraries Name=0xADDRESS`.
    fn linked_library_address(&self, func_id: hir::FunctionId) -> Option<U256> {
        let libraries = &self.gcx.sess.opts.libraries;
        if libraries.is_empty() {
            return None;
        }
        let contract_id = self.gcx.hir.function(func_id).contract?;
        let contract = self.gcx.hir.contract(contract_id);
        if !contract.kind.is_library() {
            return None;
        }
        let name = contract.name.as_str();
        let library = libraries.iter().find(|library| library.name == name)?;
        Some(U256::from_be_slice(&library.address))
    }

    /// How a struct field travels across a linked-library call boundary.
    pub(super) fn linked_field_kind(&self, ty: Ty<'gcx>) -> Option<LinkedFieldKind> {
        let ty = ty.peel_refs();
        match ty.kind {
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                Some(LinkedFieldKind::DynBytes)
            }
            TyKind::DynArray(elem) | TyKind::Slice(elem) => {
                let elem = elem.peel_refs();
                (!self.abi_is_dynamic(elem)
                    && !matches!(elem.kind, TyKind::Struct(_) | TyKind::Array(..)))
                .then_some(LinkedFieldKind::DynArray)
            }
            // Aggregates are memory pointers in the field-inlined layout and
            // cannot cross the delegatecall boundary as a word.
            TyKind::Struct(_) | TyKind::Array(..) | TyKind::Tuple(_) => None,
            _ => (!self.abi_is_dynamic(ty)).then_some(LinkedFieldKind::Value),
        }
    }

    /// Whether every parameter of `func_id` is encodable by the linked-library
    /// delegatecall convention: value types, storage references (passed by
    /// slot), and memory structs whose fields are values or one-level dynamic
    /// arrays/bytes (offset + tail). Anything else falls back to inlining —
    /// a raw memory pointer would be meaningless in the callee's memory.
    fn linked_library_args_supported(&self, func_id: hir::FunctionId) -> bool {
        let func = self.gcx.hir.function(func_id);
        func.parameters.iter().all(|&param_id| {
            if self.param_is_storage_ref(param_id) {
                return true;
            }
            let ty = self.gcx.type_of_hir_ty(&self.gcx.hir.variable(param_id).ty);
            match ty.peel_refs().kind {
                TyKind::Struct(id) => self
                    .gcx
                    .struct_field_types(id)
                    .iter()
                    .all(|&field| self.linked_field_kind(field).is_some()),
                _ => self.linked_field_kind(ty) == Some(LinkedFieldKind::Value),
            }
        })
    }

    /// Lowers a call to a `public`/`external` function of a linked library as
    /// an ABI-encoded `DELEGATECALL` to the linked address, mirroring solc's
    /// library call convention: the library runs in the caller's storage and
    /// `msg` context, storage-reference arguments travel as their slot, and a
    /// failed call re-raises the callee's revert data.
    fn lower_linked_library_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        func_id: hir::FunctionId,
        args: &CallArgs<'_>,
        lib_addr: U256,
    ) -> ValueId {
        let func = self.gcx.hir.function(func_id);
        let selector = u32::from_be_bytes(self.gcx.function_selector(func_id).0);
        let num_returns = self.function_return_slot_count(func_id);
        let struct_return_info = self.function_struct_return(func_id);

        // Evaluate arguments first: storage references lower to their slot,
        // memory structs to their pointer (field-inlined below), scalars to
        // their value.
        let params = func.parameters;
        let mut arg_vals = Vec::with_capacity(args.len());
        let mut arg_slots = Vec::with_capacity(args.len());
        let mut arg_structs = Vec::with_capacity(args.len());
        for (i, arg) in args.exprs().enumerate() {
            let is_storage_ref = params.get(i).is_some_and(|&p| self.param_is_storage_ref(p));
            if is_storage_ref {
                let slot = self.lower_lvalue_slot(builder, arg).unwrap_or_else(|| {
                    self.err_value(
                        builder,
                        arg.span,
                        "cannot resolve the storage slot of this library call argument".to_string(),
                    )
                });
                arg_vals.push(slot);
                arg_slots.push(1usize);
                arg_structs.push(None);
            } else if let Some((struct_id, field_count)) = self.get_expr_struct_info(arg) {
                arg_vals.push(self.lower_expr(builder, arg));
                arg_slots.push(field_count);
                arg_structs.push(Some(struct_id));
            } else {
                arg_vals.push(self.lower_expr(builder, arg));
                arg_slots.push(1usize);
                arg_structs.push(None);
            }
        }
        let head_size_bytes = 4 + arg_slots.iter().sum::<usize>() * 32;

        // Build the calldata at the free pointer; stash its base in scratch
        // (0x20) so it survives the argument stores.
        let calldata_start = builder.fmp();
        let scratch_calldata = builder.imm_u64(0x20);
        builder.mstore(scratch_calldata, calldata_start);

        let selector_val = builder.imm_u256(U256::from(selector) << 224);
        builder.mstore(calldata_start, selector_val);

        // Heads. A dynamic struct field reserves its head slot here and is
        // filled by the tail pass below with the tail's args-relative offset.
        let mut pending_tails: Vec<(u64, ValueId, LinkedFieldKind)> = Vec::new();
        let mut arg_offset = 4u64;
        for (i, (arg_val, slots)) in arg_vals.iter().zip(&arg_slots).enumerate() {
            if let Some(struct_id) = arg_structs[i] {
                let field_tys = self.gcx.struct_field_types(struct_id);
                let layout = crate::mir::MemoryObjectLayout::structure(*slots as u64);
                for field_idx in 0..*slots {
                    let field_addr =
                        builder.memory_object_field_addr(*arg_val, layout, field_idx as u64);
                    let field_val = builder.mload(field_addr);
                    match field_tys.get(field_idx).and_then(|&f| self.linked_field_kind(f)) {
                        Some(kind @ (LinkedFieldKind::DynArray | LinkedFieldKind::DynBytes)) => {
                            // `field_val` is the caller-memory pointer of the
                            // array/bytes; its contents travel in the tail.
                            pending_tails.push((arg_offset, field_val, kind));
                        }
                        _ => {
                            let offset_val = builder.imm_u64(arg_offset);
                            let write_addr = builder.add(calldata_start, offset_val);
                            builder.mstore(write_addr, field_val);
                        }
                    }
                    arg_offset += 32;
                }
            } else {
                let offset_val = builder.imm_u64(arg_offset);
                let write_addr = builder.add(calldata_start, offset_val);
                builder.mstore(write_addr, *arg_val);
                arg_offset += 32;
            }
        }

        // Tails: `[len][data...]` blobs appended after the heads; each head
        // slot holds its tail's offset relative to the args start (after the
        // selector), so the callee decodes with `calldataload(4 + offset)`.
        let mut tail_off = builder.imm_u64((head_size_bytes - 4) as u64);
        let word = builder.imm_u64(32);
        for (head_off, src, kind) in pending_tails {
            let object_kind = match kind {
                LinkedFieldKind::DynBytes => crate::mir::MemoryObjectKind::Bytes,
                LinkedFieldKind::DynArray => crate::mir::MemoryObjectKind::DynamicArray,
                LinkedFieldKind::Value => unreachable!(),
            };
            let head_addr_off = builder.imm_u64(head_off);
            let head_addr = builder.add(calldata_start, head_addr_off);
            builder.mstore(head_addr, tail_off);

            let len = builder.memory_object_len(src, object_kind);
            let byte_len = match kind {
                LinkedFieldKind::DynBytes => {
                    let thirty_one = builder.imm_u64(31);
                    let padded = builder.add(len, thirty_one);
                    let mask = builder.imm_u256(U256::MAX - U256::from(31));
                    builder.and(padded, mask)
                }
                _ => builder.mul(len, word),
            };

            let four = builder.imm_u64(4);
            let args_base = builder.add(calldata_start, four);
            let dst = builder.add(args_base, tail_off);
            builder.mstore(dst, len);
            let dst_data = builder.add(dst, word);
            let src_data = builder.memory_object_data(src, object_kind);
            self.mcopy(builder, dst_data, src_data, byte_len, None);

            let advanced = builder.add(word, byte_len);
            tail_off = builder.add(tail_off, advanced);
        }
        let four = builder.imm_u64(4);
        let total_size = builder.add(four, tail_off);

        // Return area: reuse the unbumped calldata allocation for value-type
        // returns, or append an allocation for struct returns.
        let (ret_offset, ret_size, struct_ptr_opt) =
            if let Some((_struct_id, field_count)) = struct_return_info {
                let struct_size = field_count as u64 * 32;
                let struct_ptr = builder.add(calldata_start, total_size);
                let struct_size_val = builder.imm_u64(struct_size);
                let new_free_ptr = builder.add(struct_ptr, struct_size_val);
                builder.set_fmp(new_free_ptr);
                (struct_ptr, builder.imm_u64(struct_size), Some(struct_ptr))
            } else {
                let ret_offset = if num_returns > 1 { calldata_start } else { builder.imm_u64(0) };
                (ret_offset, builder.imm_u64(num_returns as u64 * 32), None)
            };

        let calldata_size = total_size;
        let addr = builder.imm_u256(lib_addr);
        let gas = builder.gas();
        let scratch_reload = builder.imm_u64(0x20);
        let calldata_reload = builder.mload(scratch_reload);
        let success =
            builder.delegatecall(gas, addr, calldata_reload, calldata_size, ret_offset, ret_size);

        // Bubble the callee's revert data on failure, like solc.
        let fail_block = builder.create_block();
        let cont_block = builder.create_block();
        builder.branch(success, cont_block, fail_block);
        builder.switch_to_block(fail_block);
        let zero = builder.imm_u64(0);
        let rds = builder.returndatasize();
        builder.returndatacopy(zero, zero, rds);
        builder.revert(zero, rds);
        builder.switch_to_block(cont_block);

        if let Some(struct_ptr) = struct_ptr_opt {
            return struct_ptr;
        }
        builder.mload(ret_offset)
    }

    /// Lowers an internal library function call by inlining it.
    /// For internal library functions, we inline the function body.
    fn lower_library_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        func_id: hir::FunctionId,
        args: &CallArgs<'_>,
        bound_arg: Option<ValueId>,
    ) -> ValueId {
        let func = self.gcx.hir.function(func_id);

        // A `public`/`external` function of a library with a linked address
        // (`--libraries Name=0xADDR`) is called through DELEGATECALL instead of
        // being inlined, matching solc's library model and keeping the library
        // body out of the caller's bytecode.
        if matches!(func.visibility, hir::Visibility::Public | hir::Visibility::External)
            && bound_arg.is_none()
            && self.linked_library_args_supported(func_id)
            && let Some(lib_addr) = self.linked_library_address(func_id)
        {
            return self.lower_linked_library_call(builder, func_id, args, lib_addr);
        }

        // Inline the library function body (or, for non-trivial bodies, call an
        // internal-frame copy). A library has no storage of its own and runs in
        // the caller's storage/`msg` context, so a `public`/`external` library
        // function produces the same result inlined as it does through solc's
        // delegatecall linking — without needing a separately deployed+linked
        // library. Only a body-less declaration cannot be lowered this way.
        if func.body.is_some() {
            // Collect argument values FIRST (before entering inline tracking)
            // This allows nested calls to the same function (e.g., add(add(x, 1), 2))
            // because we evaluate arguments before marking ourselves as "in progress"
            let mut arg_vals: Vec<ValueId> = Vec::new();

            // If there's a bound argument (from `using X for T`), it's the first argument
            let bound_offset = bound_arg.is_some() as usize;
            if let Some(bound_val) = bound_arg {
                arg_vals.push(bound_val);
            }

            // Lower all explicit arguments. A storage-reference parameter (a
            // `mapping`, or an array/struct in `storage`) is passed by slot, so
            // such an argument is lowered to its storage slot rather than as a
            // value — lowering it as a value would `sload` the slot and pass the
            // wrong thing.
            for (i, arg) in args.exprs().enumerate() {
                let param_idx = i + bound_offset;
                if func.parameters.get(param_idx).is_some_and(|&p| self.param_is_storage_ref(p))
                    && let Some(slot) = self.lower_lvalue_slot(builder, arg)
                {
                    arg_vals.push(slot);
                } else {
                    arg_vals.push(self.lower_expr(builder, arg));
                }
            }

            // A callee taking a storage-reference parameter must go through the
            // internal-frame path, whose normal statement lowering binds
            // storage-reference locals correctly; the straight-line SSA inline
            // path lowers a `T storage x = ...;` as a memory value and would
            // miscompile subsequent field/element reads.
            let has_storage_ref_param =
                func.parameters.iter().any(|&p| self.param_is_storage_ref(p));

            // `-O size`: share multi-use helpers through (cheap static-frame)
            // calls instead of duplicating their body at every call site.
            let size_mode = self.gcx.sess.opts.optimization.is_size();

            if func.returns.is_empty() {
                if size_mode || has_storage_ref_param || self.function_is_recursive(func_id) {
                    return self.lower_internal_call_fallback(builder, func_id, arg_vals);
                }
                return self.lower_inline_void_call(builder, func_id, arg_vals);
            }

            // A library helper returning a calldata slice must inline for the
            // same reason an internal one does (the fallback would leave a slice
            // the backend cannot lower); non-inlinable shapes are reported.
            if self.returns_calldata_slice(func) {
                return self.lower_calldata_slice_return_call(
                    builder,
                    func_id,
                    arg_vals,
                    has_storage_ref_param,
                );
            }

            if size_mode
                || has_storage_ref_param
                || !Self::is_simple_return_function(func)
                || self.function_is_recursive(func_id)
            {
                return self.lower_internal_call_fallback(builder, func_id, arg_vals);
            }

            // Check for recursive inlining cycle AFTER evaluating arguments.
            if !self.try_enter_inline(func_id) {
                return self.lower_internal_call_fallback(builder, func_id, arg_vals);
            }

            // Simple inlining: bind parameters directly as SSA values
            // This works for pure functions that don't mutate parameters
            // Save current locals
            let saved_locals = std::mem::take(&mut self.locals);
            let saved_local_memory_slots = std::mem::take(&mut self.local_memory_slots);
            let saved_next_local_memory_offset = self.next_local_memory_offset;
            let saved_assigned_vars = std::mem::take(&mut self.assigned_vars);

            if let Some(body) = &func.body {
                self.collect_assigned_vars_block(body);
            }

            // Bind parameters to argument values directly (SSA style)
            for (i, &param_id) in func.parameters.iter().enumerate() {
                if let Some(&arg_val) = arg_vals.get(i) {
                    self.locals.insert(param_id, arg_val);
                }
            }

            // For simple functions with a single return statement, extract and evaluate directly
            let result = if let Some(body) = &func.body {
                self.lower_library_body_simple(builder, body, func)
            } else {
                builder.imm_u64(0)
            };

            // Restore locals
            self.locals = saved_locals;
            self.local_memory_slots = saved_local_memory_slots;
            self.next_local_memory_offset = saved_next_local_memory_offset;
            self.assigned_vars = saved_assigned_vars;

            // Exit inline tracking
            self.exit_inline();

            result
        } else {
            {
                let guar = self
                    .gcx
                    .dcx()
                    .err("codegen does not support external library calls yet")
                    .emit();
                builder.error_value(guar)
            }
        }
    }

    fn is_simple_return_function(func: &hir::Function<'_>) -> bool {
        if func.returns.len() != 1 {
            return false;
        }
        let Some(body) = func.body else {
            return false;
        };
        body.stmts.iter().any(|stmt| matches!(stmt.kind, hir::StmtKind::Return(Some(_))))
            && body.stmts.iter().all(|stmt| {
                matches!(
                    stmt.kind,
                    hir::StmtKind::DeclSingle(_)
                        | hir::StmtKind::Expr(_)
                        | hir::StmtKind::Return(Some(_))
                )
            })
    }

    /// Whether `func_id` directly or indirectly calls itself (cached). A recursive function
    /// must be lowered as a real `internal_call` instead of being inlined.
    fn function_is_recursive(&mut self, func_id: hir::FunctionId) -> bool {
        if let Some(&cached) = self.recursive_functions.get(&func_id) {
            return cached;
        }
        let mut visiting = GrowableBitSet::new_empty();
        let result = self.function_reaches(func_id, func_id, &mut visiting);
        self.recursive_functions.insert(func_id, result);
        result
    }

    fn function_reaches(
        &self,
        current: hir::FunctionId,
        target: hir::FunctionId,
        visiting: &mut GrowableBitSet<hir::FunctionId>,
    ) -> bool {
        if !visiting.insert(current) {
            return false;
        }

        for callee in self.function_callees(current) {
            if callee == target || self.function_reaches(callee, target, visiting) {
                return true;
            }
        }

        false
    }

    fn function_callees(&self, func_id: hir::FunctionId) -> Vec<hir::FunctionId> {
        let mut callees = Vec::new();
        let func = self.gcx.hir.function(func_id);
        if let Some(body) = func.body {
            for stmt in body.stmts {
                self.stmt_collect_callees(stmt, &mut callees);
            }
        }
        callees
    }

    /// Collects calls contained recursively in a statement.
    fn stmt_collect_callees(&self, stmt: &hir::Stmt<'_>, callees: &mut Vec<hir::FunctionId>) {
        use hir::StmtKind;
        match &stmt.kind {
            StmtKind::Expr(e)
            | StmtKind::Return(Some(e))
            | StmtKind::Revert(e)
            | StmtKind::Emit(e) => self.expr_collect_callees(e, callees),
            StmtKind::Block(b) | StmtKind::UncheckedBlock(b) | StmtKind::AssemblyBlock(b) => {
                for stmt in b.stmts {
                    self.stmt_collect_callees(stmt, callees);
                }
            }
            StmtKind::If(c, t, e) => {
                self.expr_collect_callees(c, callees);
                self.stmt_collect_callees(t, callees);
                if let Some(e) = e {
                    self.stmt_collect_callees(e, callees);
                }
            }
            StmtKind::Loop(b, _) => {
                for stmt in b.stmts {
                    self.stmt_collect_callees(stmt, callees);
                }
            }
            StmtKind::Switch(sw) => {
                self.expr_collect_callees(sw.selector, callees);
                for case in sw.cases {
                    for stmt in case.body.stmts {
                        self.stmt_collect_callees(stmt, callees);
                    }
                }
            }
            StmtKind::Try(t) => {
                self.expr_collect_callees(&t.expr, callees);
                for clause in t.clauses {
                    for stmt in clause.block.stmts {
                        self.stmt_collect_callees(stmt, callees);
                    }
                }
            }
            StmtKind::DeclSingle(var_id) => {
                if let Some(init) = self.gcx.hir.variable(*var_id).initializer {
                    self.expr_collect_callees(init, callees);
                }
            }
            StmtKind::DeclMulti(_, init) => self.expr_collect_callees(init, callees),
            StmtKind::Return(None)
            | StmtKind::Continue
            | StmtKind::Break
            | StmtKind::Placeholder
            | StmtKind::Err(_) => {}
        }
    }

    /// Collects calls contained recursively in an expression.
    fn expr_collect_callees(&self, expr: &hir::Expr<'_>, callees: &mut Vec<hir::FunctionId>) {
        match &expr.kind {
            ExprKind::Call(callee, args, _) => {
                if let Some(func_id) = self.resolved_function_callee(callee) {
                    callees.push(func_id);
                }
                self.expr_collect_callees(callee, callees);
                for arg in args.exprs() {
                    self.expr_collect_callees(arg, callees);
                }
            }
            ExprKind::Binary(l, _, r) | ExprKind::Assign(l, _, r) => {
                self.expr_collect_callees(l, callees);
                self.expr_collect_callees(r, callees);
            }
            ExprKind::Unary(_, e)
            | ExprKind::Member(e, _)
            | ExprKind::YulMember(e, _)
            | ExprKind::Payable(e)
            | ExprKind::Delete(e) => self.expr_collect_callees(e, callees),
            ExprKind::Ternary(c, t, f) => {
                self.expr_collect_callees(c, callees);
                self.expr_collect_callees(t, callees);
                self.expr_collect_callees(f, callees);
            }
            ExprKind::Index(b, i) => {
                self.expr_collect_callees(b, callees);
                if let Some(i) = i {
                    self.expr_collect_callees(i, callees);
                }
            }
            ExprKind::Slice(b, s, e) => {
                self.expr_collect_callees(b, callees);
                if let Some(s) = s {
                    self.expr_collect_callees(s, callees);
                }
                if let Some(e) = e {
                    self.expr_collect_callees(e, callees);
                }
            }
            ExprKind::Array(es) => {
                for e in *es {
                    self.expr_collect_callees(e, callees);
                }
            }
            ExprKind::Tuple(es) => {
                for e in es.iter().flatten() {
                    self.expr_collect_callees(e, callees);
                }
            }
            ExprKind::New(_)
            | ExprKind::TypeCall(_)
            | ExprKind::Lit(_)
            | ExprKind::Ident(_)
            | ExprKind::Type(_)
            | ExprKind::Err(_) => {}
        }
    }

    /// Lowers a simple library function body.
    /// For functions with a single return statement, directly evaluate the return expression.
    fn lower_library_body_simple(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        body: &hir::Block<'_>,
        func: &hir::Function<'_>,
    ) -> ValueId {
        let saved_in_unchecked_block = self.in_unchecked_block;
        self.in_unchecked_block = false;

        for &return_id in func.returns {
            let zero = builder.imm_u64(0);
            self.locals.insert(return_id, zero);
        }

        let result = if let Some(value) = self.lower_library_block_return(builder, body) {
            value
        } else {
            // Implicit named returns: stage returns 2..N in the ephemeral
            // multi-return buffer; the first return flows back as MIR value.
            let return_values: Vec<_> =
                func.returns.iter().filter_map(|id| self.locals.get(id).copied()).collect();
            self.stage_multi_return_tail(builder, &return_values);

            if let Some(&return_id) = func.returns.first()
                && let Some(&value) = self.locals.get(&return_id)
            {
                value
            } else {
                builder.imm_u64(0)
            }
        };

        self.in_unchecked_block = saved_in_unchecked_block;
        result
    }

    fn lower_library_block_return(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        block: &hir::Block<'_>,
    ) -> Option<ValueId> {
        for stmt in block.stmts {
            if let Some(value) = self.lower_library_stmt_return(builder, stmt) {
                return Some(value);
            }
        }
        None
    }

    /// Extract return value from a statement after lowering prior side effects in that statement.
    fn lower_library_stmt_return(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        stmt: &hir::Stmt<'_>,
    ) -> Option<ValueId> {
        match &stmt.kind {
            hir::StmtKind::Return(Some(expr)) => {
                if let hir::ExprKind::Tuple(elements) = &expr.kind {
                    let values: Vec<_> = elements
                        .iter()
                        .flatten()
                        .map(|element| self.lower_expr(builder, element))
                        .collect();
                    self.stage_multi_return_tail(builder, &values);
                    values.first().copied()
                } else {
                    Some(self.lower_expr(builder, expr))
                }
            }
            hir::StmtKind::Return(None) => Some(builder.imm_u64(0)),
            hir::StmtKind::DeclSingle(var_id) => {
                let var = self.gcx.hir.variable(*var_id);
                let init_val = if let Some(init) = var.initializer {
                    self.lower_expr(builder, init)
                } else {
                    builder.imm_u64(0)
                };
                self.locals.insert(*var_id, init_val);
                None
            }
            hir::StmtKind::Expr(expr) => {
                self.lower_expr(builder, expr);
                None
            }
            hir::StmtKind::Block(block) => self.lower_library_block_return(builder, block),
            hir::StmtKind::UncheckedBlock(block) => self.lower_library_block_return(builder, block),
            hir::StmtKind::If(cond, then_stmt, else_stmt) => {
                let cond_val = self.lower_expr(builder, cond);
                let then_return = self.lower_library_stmt_return(builder, then_stmt);
                let else_return =
                    else_stmt.map(|else_stmt| self.lower_library_stmt_return(builder, else_stmt));

                match (then_return, else_return.flatten()) {
                    (Some(then_val), Some(else_val)) => {
                        Some(builder.select(cond_val, then_val, else_val))
                    }
                    // A one-sided return is an early-return control-flow shape. This helper
                    // returns expression values only, so let later statements provide the
                    // fallthrough value instead of treating the branch as unconditional.
                    _ => None,
                }
            }
            hir::StmtKind::DeclMulti(vars, rhs) => {
                self.lower_multi_var_decl(builder, vars, rhs);
                None
            }
            hir::StmtKind::Loop(..)
            | hir::StmtKind::AssemblyBlock(_)
            | hir::StmtKind::Switch(_)
            | hir::StmtKind::Emit(_)
            | hir::StmtKind::Revert(_)
            | hir::StmtKind::Break
            | hir::StmtKind::Continue
            | hir::StmtKind::Try(_)
            | hir::StmtKind::Placeholder
            | hir::StmtKind::Err(_) => {
                self.lower_stmt(builder, stmt);
                None
            }
        }
    }

    /// Checks if an expression has a contract value type.
    pub(super) fn is_contract_type_expr(&self, expr: &hir::Expr<'_>) -> bool {
        self.get_expr_type(expr).is_some_and(|ty| matches!(ty.kind, TyKind::Contract(_)))
    }
}
