//! Lazily outlined lowering helpers.

use super::{Lowerer, checked_arith::PanicCode};
use crate::mir::{
    AbiLayout, AbiType, Function, FunctionBuilder, FunctionId, MemoryObjectKind,
    MemoryObjectLayout, MirType, SliceLocation,
};
use alloy_primitives::U256;
use solar_data_structures::map::{FxHashMap, FxHashSet};
use solar_interface::{Ident, Span, Symbol, sym};

/// Semantic operations that have a shared MIR implementation in a module.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum HelperKey {
    /// Revert with a Solidity `Panic(uint256)` payload.
    Panic(PanicCode),
    /// Revert with a short `Error(string)` payload.
    RevertError,
    /// Decode one storage `bytes`/`string` value into a memory object.
    LoadStorageBytes,
}

/// Registry for lazily outlined helpers.
#[derive(Default)]
pub(super) struct OutlinedHelpers {
    functions: FxHashMap<HelperKey, FunctionId>,
    synthesizing: FxHashSet<HelperKey>,
}

impl OutlinedHelpers {
    fn get(&self, key: HelperKey) -> Option<FunctionId> {
        self.functions.get(&key).copied()
    }

    fn insert(&mut self, key: HelperKey, function: FunctionId) {
        debug_assert!(self.functions.insert(key, function).is_none());
    }

    pub(super) fn is_synthesizing(&self, key: HelperKey) -> bool {
        self.synthesizing.contains(&key)
    }

    fn begin(&mut self, key: HelperKey) {
        debug_assert!(self.synthesizing.insert(key));
    }

    fn end(&mut self, key: HelperKey) {
        debug_assert!(self.synthesizing.remove(&key));
    }
}

impl<'gcx> Lowerer<'gcx> {
    /// Returns the shared `Panic(uint256)` helper for `code`, creating it on demand.
    pub(super) fn ensure_panic_helper(&mut self, code: PanicCode) -> FunctionId {
        let key = HelperKey::Panic(code);
        if let Some(id) = self.outlined_helpers.get(key) {
            return id;
        }

        let name = format!("__panic_{:x}", code.as_u64());
        let mut func = Function::new(Ident::with_dummy_span(Symbol::intern(&name)));
        {
            let mut builder = FunctionBuilder::new(&mut func);
            self.outlined_helpers.begin(key);
            self.emit_panic_revert_inline(&mut builder, code);
            self.outlined_helpers.end(key);
        }
        let id = self.module.add_function(func);
        self.outlined_helpers.insert(key, id);
        id
    }

    /// Returns the shared short `Error(string)` helper, creating it on demand.
    pub(super) fn ensure_revert_error_helper(&mut self) -> FunctionId {
        let key = HelperKey::RevertError;
        if let Some(id) = self.outlined_helpers.get(key) {
            return id;
        }

        let name = Ident::new(sym::__revert_error, Span::DUMMY);
        let mut func = Function::new(name);
        {
            let mut builder = FunctionBuilder::new(&mut func);
            let len = builder.add_param(MirType::uint256());
            let data = builder.add_param(MirType::uint256());
            let object_size = builder.imm_u64(64);
            let object = builder.alloc_object(
                object_size,
                MemoryObjectLayout::Bytes,
                crate::mir::AllocationSemantics::INTERNAL,
            );
            builder.set_memory_object_len(object, len, MemoryObjectKind::Bytes);
            let zero = builder.imm_u64(0);
            builder.memory_object_store_element(object, MemoryObjectLayout::Bytes, zero, data);

            let layout = self
                .module
                .intern_abi_layout(AbiLayout::new(vec![AbiType::Bytes(SliceLocation::Memory)]));
            let selector = builder.imm_u256(U256::from(0x08c3_79a0u64) << 224);
            let payload = builder.abi_encode(layout, Some(selector), [object]);
            let ptr = builder.slice_ptr(payload);
            let size = builder.slice_len(payload);
            builder.revert(ptr, size);
        }
        let id = self.module.add_function(func);
        self.outlined_helpers.insert(key, id);
        id
    }

    /// Returns the shared storage-`bytes`/`string` loader, creating it on demand.
    pub(super) fn ensure_load_storage_bytes_helper(&mut self) -> FunctionId {
        let key = HelperKey::LoadStorageBytes;
        if let Some(id) = self.outlined_helpers.get(key) {
            return id;
        }

        let name = Ident::new(sym::__load_storage_bytes, Span::DUMMY);
        let mut func = Function::new(name);
        {
            let mut builder = FunctionBuilder::new(&mut func);
            let slot = builder.add_param(MirType::uint256());
            builder.add_return(MirType::MemoryObject(MemoryObjectKind::Bytes));
            self.outlined_helpers.begin(key);
            let ptr = self.materialize_storage_bytes_inline(&mut builder, slot);
            self.outlined_helpers.end(key);
            builder.ret([ptr]);
        }
        let id = self.module.add_function(func);
        self.outlined_helpers.insert(key, id);
        id
    }
}
