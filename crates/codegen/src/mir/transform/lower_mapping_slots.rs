//! Lower mapping-slot and storage-array slot builtins to physical hashing.
//!
//! This pass expands storage-location hashing at the memory boundary. Variable-size
//! hash inputs use the free-memory pointer as transient scratch; fixed-width
//! mapping and storage-array hashes use reserved scratch. The semantic instructions
//! declare these writes so earlier passes cannot forward stale scratch reads.
//! CSE can share fixed-width hashes only while their inputs and scratch writes
//! remain unchanged. Variable-width hashes require physical alias proofs.
//!
//! A fixed-width hash of constant words, such as the data slot of an array at a
//! constant slot, is computed here instead when its constant, pushed at every
//! use, costs less over the deployment's lifetime than the scratch stores and
//! the hash; no later pass folds the physical stores. Solc's IR optimizer folds
//! the same hashes under a similar price. The fold drops the scratch writes,
//! which the semantic instructions declare only for their own hashing.

use super::egraph::use_counts;
use crate::{
    backend::evm::op,
    mir::{
        Function, FunctionBuilder, InstKind, MemoryObjectKind, Module, SliceLocation, ValueId,
        pass::{MirPass, run_function_pass},
    },
    target::Target,
};
use alloy_primitives::{U256, keccak256};
use solar_config::OptimizationMode;
use solar_data_structures::{index::IndexVec, map::FxHashMap};

/// Lowers mapping-slot hash builtins at the memory boundary.
pub(crate) struct LowerMappingSlots;

impl MirPass for LowerMappingSlots {
    fn name(&self) -> &'static str {
        "lower-mapping-slots"
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        let target = Target::new(gcx);
        run_function_pass(module, analyses, |func, _| {
            let has_mapping_slots = func.instructions().any(|inst_id| {
                matches!(
                    func.inst(inst_id).kind,
                    InstKind::MappingSlot(_, _)
                        | InstKind::MappingSlotMemory(_, _)
                        | InstKind::MappingSlotCalldata(_, _)
                        | InstKind::StorageArrayDataSlot(_)
                        | InstKind::StorageArrayElementSlot { .. }
                )
            });
            if !has_mapping_slots {
                return false;
            }

            let folding = HashFolding { target, uses: use_counts(func) };
            let mut replacements = FxHashMap::default();
            let block_ids = func.blocks.indices();
            for block_id in block_ids {
                let instructions = std::mem::take(&mut func.blocks[block_id].instructions);
                let mut builder = FunctionBuilder::new(func);
                builder.switch_to_block(block_id);
                for inst_id in instructions {
                    let result = builder.func().inst_result_value(inst_id);
                    let replacement = match builder.func().inst(inst_id).kind {
                        InstKind::MappingSlot(key, slot) => {
                            if let Some(hash) = folding.fold(builder.func(), result, &[key, slot]) {
                                // result = keccak256(key . slot)
                                builder.imm(hash)
                            } else {
                                lower_word_mapping_slot(&mut builder, key, slot)
                            }
                        }
                        InstKind::MappingSlotMemory(key, slot) => {
                            let location = match builder.func().value_ty(key) {
                                Some(crate::mir::MirType::Slice(SliceLocation::Calldata)) => {
                                    SliceLocation::Calldata
                                }
                                _ => SliceLocation::Memory,
                            };
                            lower_slice_mapping_slot(&mut builder, location, key, slot)
                        }
                        InstKind::MappingSlotCalldata(key, slot) => lower_slice_mapping_slot(
                            &mut builder,
                            SliceLocation::Calldata,
                            key,
                            slot,
                        ),
                        InstKind::StorageArrayDataSlot(slot) => {
                            if let Some(hash) = folding.fold(builder.func(), result, &[slot]) {
                                // result = keccak256(slot)
                                builder.imm(hash)
                            } else {
                                lower_storage_array_data_slot(&mut builder, slot)
                            }
                        }
                        InstKind::StorageArrayElementSlot { slot, index, element_slots } => {
                            // The data slot is used once, by the element offset.
                            let data_slot = folding.fold_single(builder.func(), &[slot]);
                            lower_storage_array_element_slot(
                                &mut builder,
                                data_slot,
                                slot,
                                index,
                                element_slots,
                            )
                        }
                        _ => {
                            builder.func_mut().blocks[block_id].instructions.push(inst_id);
                            continue;
                        }
                    };
                    let result = result.expect("mapping slot must produce a value");
                    replacements.insert(result, replacement);
                }
            }
            func.replace_uses_canonicalized(&replacements);
            true
        })
    }
}

fn lower_storage_array_data_slot(
    builder: &mut FunctionBuilder<'_>,
    slot: crate::mir::ValueId,
) -> crate::mir::ValueId {
    // mstore(0, slot)
    // result = keccak256(0, 32)
    let word = builder.imm(32);
    let zero = builder.imm(0);
    builder.mstore(zero, slot);
    builder.keccak256(zero, word)
}

fn lower_storage_array_element_slot(
    builder: &mut FunctionBuilder<'_>,
    folded_data_slot: Option<U256>,
    slot: crate::mir::ValueId,
    index: crate::mir::ValueId,
    element_slots: u64,
) -> crate::mir::ValueId {
    let data_slot = match folded_data_slot {
        // data_slot = keccak256(slot), computed now
        Some(hash) => builder.imm(hash),
        // mstore(0, slot)
        // data_slot = keccak256(0, 32)
        None => lower_storage_array_data_slot(builder, slot),
    };
    // result = data_slot + index * element_slots
    let offset = if element_slots <= 1 {
        index
    } else {
        let stride = builder.imm(element_slots);
        builder.mul(index, stride)
    };
    builder.add(data_slot, offset)
}

/// Hash a fixed-width mapping key in the reserved scratch region.
fn lower_word_mapping_slot(
    builder: &mut FunctionBuilder<'_>,
    key: crate::mir::ValueId,
    slot: crate::mir::ValueId,
) -> crate::mir::ValueId {
    // mstore(0, key)
    // mstore(32, slot)
    // result = keccak256(0, 64)
    let zero = builder.imm(0);
    let word = builder.imm(32);
    let size = builder.imm(64);
    builder.mstore(zero, key);
    builder.mstore(word, slot);
    builder.keccak256(zero, size)
}

fn lower_slice_mapping_slot(
    builder: &mut FunctionBuilder<'_>,
    location: SliceLocation,
    value: crate::mir::ValueId,
    slot: crate::mir::ValueId,
) -> crate::mir::ValueId {
    // length = slice_len(value)
    // scratch = fmp
    // copy(scratch, slice_ptr(value), length)
    // mstore(scratch + length, slot)
    // result = keccak256(scratch, length + 32)
    let len = match location {
        SliceLocation::Memory => builder.memory_object_len(value, MemoryObjectKind::Bytes),
        SliceLocation::Calldata | SliceLocation::Returndata => builder.slice_len(value),
    };
    let word_size = builder.imm(32);
    let payload_size = builder.add(len, word_size);
    let scratch = builder.fmp();
    let source = match location {
        SliceLocation::Memory => builder.memory_object_data(value, MemoryObjectKind::Bytes),
        SliceLocation::Calldata | SliceLocation::Returndata => builder.slice_ptr(value),
    };
    builder.copy_slice_data(location, scratch, source, len);
    let slot_address = builder.add(scratch, len);
    builder.mstore(slot_address, slot);
    builder.keccak256(scratch, payload_size)
}

/// Chooses between hashing constant scratch words at run time and pushing
/// their hash.
struct HashFolding {
    target: Target,
    /// Uses of each value before lowering, which the hash result inherits.
    uses: IndexVec<ValueId, u32>,
}

impl HashFolding {
    /// Returns the hash of `words` when every one is constant and pushing it at
    /// each use of `result` is the cheaper shape.
    fn fold(&self, func: &Function, result: Option<ValueId>, words: &[ValueId]) -> Option<U256> {
        let uses = result.and_then(|result| self.uses.get(result).copied()).unwrap_or(1);
        self.fold_uses(func, words, uses)
    }

    /// Returns the hash of `words` when every one is constant and pushing it
    /// once is the cheaper shape.
    fn fold_single(&self, func: &Function, words: &[ValueId]) -> Option<U256> {
        self.fold_uses(func, words, 1)
    }

    fn fold_uses(&self, func: &Function, words: &[ValueId], uses: u32) -> Option<U256> {
        if matches!(self.target.optimization(), OptimizationMode::None) {
            return None;
        }
        let words = words.iter().map(|&word| func.value_u256(word)).collect::<Option<Vec<_>>>()?;
        let bytes = words.iter().flat_map(|word| word.to_be_bytes::<32>()).collect::<Vec<_>>();
        let hash = U256::from_be_bytes(keccak256(&bytes).0);
        let uses = uses.max(1);
        // The run-time shape stores each word in scratch, hashes them once and
        // duplicates the hash for every later use.
        let size = U256::from(bytes.len());
        let mut hashed = self.target.push(size)
            + self.target.push(U256::ZERO)
            + self.target.opcode_with_immediates(op::KECCAK256, &[Some(U256::ZERO), Some(size)])
            + self.target.dup().times(uses - 1);
        for (index, &word) in words.iter().enumerate() {
            hashed += self.target.push(word)
                + self.target.push(U256::from(32 * index))
                + self.target.opcode(op::MSTORE);
        }
        let pushed = self.target.push(hash).times(uses);
        let cheaper = if self.target.optimization().is_gas() {
            self.target.lifetime_gas(pushed) < self.target.lifetime_gas(hashed)
        } else {
            self.target.cmp(pushed, hashed).is_lt()
        };
        cheaper.then_some(hash)
    }
}
