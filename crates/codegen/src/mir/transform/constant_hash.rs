//! Fold hashes of bytes explicitly written earlier in the same block.
//!
//! Track a bounded byte map using the shared alias analysis. Constant MSTORE
//! and MSTORE8 writes supply big-endian bytes; ModRef removes every byte an
//! intervening instruction may overwrite. Allocation disjointness applies only
//! within the allocation bounds. Unknown bytes, oversized ranges,
//! joins, and unsupported addresses keep the runtime hash. No zero-initialized
//! memory is assumed, and facts never cross a block boundary.
//!
//! Every byte of a nonempty folded hash was already written, so removing the
//! hash cannot remove memory expansion visible to MSIZE. Empty hashes access
//! no memory, including at an out-of-range offset. Stores and other effects
//! stay in place. Target prices the replacement PUSH against the hash opcode,
//! including its known dynamic cost, before accepting the fold. Run after
//! physical memory lowering, before scalar cleanup and stack scheduling.

use crate::{
    mir::{
        BlockId, Function, Immediate, InstKind, Module, Value,
        analysis::{
            AddressSpace, AliasAnalysis, Location, LocationSize, MemoryAddress, MemoryLocation,
        },
        pass::{MirPass, ModuleAnalyses, run_function_pass},
    },
    target::Target,
};
use alloy_primitives::{U256, keccak256};
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};
use solar_sema::Gcx;

pub(crate) struct ConstantHash;

const MAX_BYTES: usize = 4096;

impl MirPass for ConstantHash {
    fn name(&self) -> &'static str {
        "constant-hash"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module, analyses: &mut ModuleAnalyses) -> bool {
        let target = Target::new(gcx);
        run_function_pass(module, analyses, |func, _| {
            if !func
                .instructions()
                .any(|inst| matches!(func.inst(inst).kind, InstKind::Keccak256(..)))
            {
                return false;
            }
            run(func, target)
        })
    }
}

fn run(func: &mut Function, target: Target) -> bool {
    let mut alias = AliasAnalysis::new(func);
    let mut changed = false;
    for block in func.blocks.indices() {
        if fold_block(func, block, &alias, target) != 0 {
            alias = AliasAnalysis::new(func);
            changed = true;
        }
    }
    changed
}

fn fold_block(
    func: &mut Function,
    block_id: BlockId,
    alias: &AliasAnalysis,
    target: Target,
) -> usize {
    let block = &func.blocks[block_id];
    if !block
        .instructions
        .iter()
        .any(|&inst| matches!(func.inst(inst).kind, InstKind::Keccak256(..)))
    {
        return 0;
    }
    let mut folded = Vec::new();
    let mut memory = FxHashMap::<MemoryAddress, u8>::default();
    for &inst in &block.instructions {
        let instruction = func.inst(inst);
        if let InstKind::Keccak256(address, size) = instruction.kind
            && instruction
                .metadata
                .effect()
                .is_none_or(|effect| effect == instruction.kind.effect_kind())
            && let Some(size) = func.value_u64(size).filter(|&size| size <= MAX_BYTES as u64)
        {
            let bytes = if size == 0 {
                Some(Vec::new())
            } else {
                alias.bare_memory_location(func, address, LocationSize::Const(size)).and_then(
                    |location| {
                        (0..size)
                            .map(|offset| {
                                memory.get(&location.address.checked_add(offset)?).copied()
                            })
                            .collect::<Option<Vec<_>>>()
                    },
                )
            };
            if let Some(bytes) = bytes {
                let hash = U256::from_be_bytes(keccak256(bytes).0);
                if target
                    .cmp(
                        target.push(hash),
                        target.op(&instruction.kind.op(), |value| func.value_u256(value)),
                    )
                    .is_lt()
                {
                    folded.push((inst, hash));
                }
            }
        }

        if !memory.is_empty() {
            let effects = alias.instruction_mod_ref(func, inst);
            if effects.writes_space(AddressSpace::Memory) {
                memory.retain(|address, _| {
                    !effects.may_write(
                        alias,
                        Location::Memory(MemoryLocation::new(*address, LocationSize::Const(1))),
                    )
                });
            }
        }
        let (address, value, width) = match instruction.kind {
            InstKind::MStore(address, value) => (address, value, 32),
            InstKind::MStore8(address, value) => (address, value, 1),
            _ => continue,
        };
        if instruction
            .metadata
            .effect()
            .is_none_or(|effect| effect == instruction.kind.effect_kind())
            && let Some(location) =
                alias.bare_memory_location(func, address, LocationSize::Const(width))
            && let Some(value) = func.value_u256(value)
            && location.address.checked_add(width - 1).is_some()
        {
            if memory.len() + width as usize > MAX_BYTES {
                memory.clear();
            }
            let bytes = value.to_be_bytes::<32>();
            for offset in 0..width {
                memory.insert(
                    location.address.checked_add(offset).unwrap(),
                    bytes[(32 - width + offset) as usize],
                );
            }
        }
    }
    if folded.is_empty() {
        return 0;
    }
    let count = folded.len();
    let mut replacements = FxHashMap::default();
    let mut removed = DenseBitSet::new_empty(func.num_insts());
    for (inst, hash) in folded {
        let result = func.inst_result_value(inst).unwrap();
        let constant = func.alloc_value(Value::Immediate(Immediate::I256(hash)));
        replacements.insert(result, constant);
        removed.insert(inst);
    }
    func.replace_uses_canonicalized(&replacements);
    func.blocks[block_id].instructions.retain(|&inst| !removed.contains(inst));
    count
}
