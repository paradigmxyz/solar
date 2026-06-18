//! Local dead memory optimization.
//!
//! This pass removes full-word `mstore` instructions that are overwritten by a
//! later full-word `mstore` to the same exact address within the same basic
//! block, before any operation can observe memory or gas. It also forwards
//! same-block `mload` instructions from the latest exact-address `mstore` when
//! no intervening operation can mutate memory.

use crate::{
    analysis::CfgInfo,
    mir::{BlockId, Function, Immediate, InstId, InstKind, Terminator, Value, ValueId},
    pass::FunctionPass,
};
use alloy_primitives::{U256, keccak256};
use solar_data_structures::map::{FxHashMap, FxHashSet};

/// Local dead memory optimization pass.
#[derive(Debug, Default)]
pub struct MemoryStoreEliminator {
    /// Number of memory instructions eliminated.
    pub eliminated_count: usize,
}

/// Function pass for local dead memory-store elimination.
pub struct MemoryDsePass;

impl FunctionPass for MemoryDsePass {
    fn name(&self) -> &str {
        "memory-dse"
    }

    fn run_on_function(&mut self, func: &mut Function) -> bool {
        MemoryStoreEliminator::new().run_to_fixpoint(func) != 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum MemAddrKey {
    Const(u64),
    BaseOffset { base: ValueId, offset: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ImmutableCopyKey {
    len: u64,
    offset: u64,
}

#[derive(Clone, Copy, Debug)]
struct CachedImmutableCopy {
    block: BlockId,
    index: usize,
    value: ValueId,
}

impl MemoryStoreEliminator {
    /// Creates a new memory optimization pass.
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs local memory optimization on a function.
    pub fn run(&mut self, func: &mut Function) -> usize {
        self.eliminated_count = 0;

        self.reuse_redundant_immutable_copies(func);
        self.remove_unused_internal_frame_stores(func);

        let block_ids: Vec<BlockId> = func.blocks.indices().collect();
        for block_id in block_ids {
            self.process_block(func, block_id);
        }
        self.remove_cross_block_overwrites(func);

        self.eliminated_count
    }

    /// Runs local memory optimization until no more instructions can be eliminated.
    pub fn run_to_fixpoint(&mut self, func: &mut Function) -> usize {
        let mut total = 0;
        loop {
            let eliminated = self.run(func);
            if eliminated == 0 {
                break;
            }
            total += eliminated;
        }
        total
    }

    fn reuse_redundant_immutable_copies(&mut self, func: &mut Function) {
        let inst_results = func.inst_results();
        let cfg = CfgInfo::new(func);
        let mut cached: FxHashMap<ImmutableCopyKey, CachedImmutableCopy> = FxHashMap::default();
        let mut replacements = FxHashMap::default();
        let mut dead = FxHashSet::default();

        for (block_id, block) in func.blocks.iter_enumerated() {
            let insts = block.instructions.clone();
            for (index, window) in insts.windows(2).enumerate() {
                let codecopy = window[0];
                let load = window[1];
                let InstKind::CodeCopy(dest, src, size) = func.instructions[codecopy].kind else {
                    continue;
                };
                if Self::as_u64(func, size) != Some(32) {
                    continue;
                }
                let Some(key) = Self::immutable_copy_key(func, src) else {
                    continue;
                };
                let InstKind::MLoad(load_addr) = func.instructions[load].kind else {
                    continue;
                };
                if Self::mem_addr_key(func, dest) != Self::mem_addr_key(func, load_addr) {
                    continue;
                }
                let Some(&loaded_value) = inst_results.get(&load) else {
                    continue;
                };

                if let Some(cached_copy) = cached.get(&key).copied()
                    && Self::copy_dominates(cfg.dominators(), cached_copy, block_id, index)
                {
                    replacements.insert(loaded_value, cached_copy.value);
                    func.instructions[codecopy].kind = InstKind::MStore(dest, cached_copy.value);
                    dead.insert(load);
                    self.eliminated_count += 1;
                    continue;
                }

                cached.insert(
                    key,
                    CachedImmutableCopy { block: block_id, index, value: loaded_value },
                );
            }
        }

        if replacements.is_empty() && dead.is_empty() {
            return;
        }

        func.replace_uses_canonicalized(&replacements);
        for block in func.blocks.iter_mut() {
            block.instructions.retain(|id| !dead.contains(id));
        }
    }

    fn remove_unused_internal_frame_stores(&mut self, func: &mut Function) {
        if Self::has_frame_observer(func) {
            return;
        }

        let Some(reads) = Self::internal_frame_read_ranges(func) else {
            return;
        };
        let mut dead = FxHashSet::default();

        for block in func.blocks.iter() {
            for &inst_id in &block.instructions {
                let InstKind::MStore(addr, _) = func.instructions[inst_id].kind else {
                    continue;
                };
                let Some(offset) = Self::internal_frame_offset(func, addr) else {
                    continue;
                };
                if !reads.iter().any(|&(read_offset, read_size)| {
                    Self::ranges_overlap_size(offset, 32, read_offset, read_size)
                }) {
                    dead.insert(inst_id);
                }
            }
        }

        if dead.is_empty() {
            return;
        }

        self.eliminated_count += dead.len();
        for block in func.blocks.iter_mut() {
            block.instructions.retain(|id| !dead.contains(id));
        }
    }

    fn process_block(&mut self, func: &mut Function, block_id: BlockId) {
        self.fold_constant_keccak(func, block_id);
        self.forward_loads(func, block_id);
        self.remove_equal_stores(func, block_id);

        let inst_ids = func.blocks[block_id].instructions.clone();
        let mut overwritten: FxHashSet<MemAddrKey> = FxHashSet::default();
        let mut dead: FxHashSet<InstId> = FxHashSet::default();

        for &inst_id in inst_ids.iter().rev() {
            let inst = &func.instructions[inst_id];
            match &inst.kind {
                InstKind::MStore(addr, _) => {
                    if let Some(key) = Self::mem_addr_key(func, *addr) {
                        if overwritten.contains(&key) {
                            dead.insert(inst_id);
                            self.eliminated_count += 1;
                        } else {
                            overwritten.insert(key);
                        }
                    } else {
                        overwritten.clear();
                    }
                }
                InstKind::MLoad(addr) => {
                    if let Some(key) = Self::mem_addr_key(func, *addr) {
                        Self::remove_overlapping_set(&mut overwritten, key);
                    } else {
                        overwritten.clear();
                    }
                }
                InstKind::CalldataCopy(dest, _, size)
                | InstKind::CodeCopy(dest, _, size)
                | InstKind::ReturnDataCopy(dest, _, size) => {
                    Self::insert_or_clear_full_word_overwritten_range(
                        func,
                        &mut overwritten,
                        *dest,
                        *size,
                    );
                }
                InstKind::ExtCodeCopy(_, dest, _, size) => {
                    Self::insert_or_clear_full_word_overwritten_range(
                        func,
                        &mut overwritten,
                        *dest,
                        *size,
                    );
                }
                kind if Self::is_memory_or_gas_observer(kind) => {
                    overwritten.clear();
                }
                _ => {}
            }
        }

        if dead.is_empty() {
            return;
        }

        func.blocks[block_id].instructions.retain(|id| !dead.contains(id));
    }

    fn remove_cross_block_overwrites(&mut self, func: &mut Function) {
        let mut dead = FxHashSet::default();

        for pred in func.blocks.indices() {
            let Some(succ) = Self::single_jump_successor(func, pred) else {
                continue;
            };
            if func.blocks[succ].predecessors.as_slice() != [pred] {
                continue;
            }

            let Some((store, pred_key)) = self.last_cross_block_store_candidate(func, pred) else {
                continue;
            };
            let Some(succ_key) = self.first_cross_block_overwrite(func, succ) else {
                continue;
            };
            if pred_key == succ_key {
                dead.insert(store);
            }
        }

        if dead.is_empty() {
            return;
        }

        self.eliminated_count += dead.len();
        for block in func.blocks.iter_mut() {
            block.instructions.retain(|id| !dead.contains(id));
        }
    }

    fn single_jump_successor(func: &Function, block: BlockId) -> Option<BlockId> {
        let Some(Terminator::Jump(target)) = func.blocks[block].terminator.as_ref() else {
            return None;
        };
        Some(*target)
    }

    fn last_cross_block_store_candidate(
        &self,
        func: &Function,
        block: BlockId,
    ) -> Option<(InstId, MemAddrKey)> {
        for &inst_id in func.blocks[block].instructions.iter().rev() {
            match func.instructions[inst_id].kind {
                InstKind::MStore(addr, _) => {
                    let key = Self::mem_addr_key(func, addr)?;
                    return Some((inst_id, key));
                }
                ref kind if Self::cross_block_memory_barrier(kind) => return None,
                _ => {}
            }
        }
        None
    }

    fn first_cross_block_overwrite(&self, func: &Function, block: BlockId) -> Option<MemAddrKey> {
        for &inst_id in &func.blocks[block].instructions {
            match func.instructions[inst_id].kind {
                InstKind::MStore(addr, _) => return Self::mem_addr_key(func, addr),
                ref kind if Self::cross_block_memory_barrier(kind) => return None,
                _ => {}
            }
        }
        None
    }

    fn fold_constant_keccak(&mut self, func: &mut Function, block_id: BlockId) {
        let inst_ids = func.blocks[block_id].instructions.clone();
        let inst_results = func.inst_results();
        let mut stored_words: FxHashMap<MemAddrKey, U256> = FxHashMap::default();
        let mut replacements: FxHashMap<ValueId, ValueId> = FxHashMap::default();
        let mut dead: FxHashSet<InstId> = FxHashSet::default();

        for &inst_id in &inst_ids {
            match &func.instructions[inst_id].kind {
                InstKind::MStore(addr, value) => {
                    let Some(key) = Self::mem_addr_key(func, *addr) else {
                        stored_words.clear();
                        continue;
                    };
                    Self::remove_overlapping_map(&mut stored_words, key);
                    if let Some(value) = Self::as_u256(func, *value) {
                        stored_words.insert(key, value);
                    }
                }
                InstKind::Keccak256(offset, size) => {
                    let Some(bytes) =
                        Self::constant_memory_bytes(func, &stored_words, *offset, *size)
                    else {
                        continue;
                    };
                    let Some(&result) = inst_results.get(&inst_id) else {
                        continue;
                    };
                    let hash = keccak256(&bytes);
                    let replacement = func.alloc_value(Value::Immediate(Immediate::uint256(
                        U256::from_be_bytes(hash.0),
                    )));
                    replacements.insert(result, replacement);
                    dead.insert(inst_id);
                    self.eliminated_count += 1;
                }
                kind if Self::can_mutate_memory(kind) => {
                    stored_words.clear();
                }
                _ => {}
            }
        }

        if dead.is_empty() {
            return;
        }

        func.replace_uses_canonicalized(&replacements);
        func.blocks[block_id].instructions.retain(|id| !dead.contains(id));
    }

    fn remove_equal_stores(&mut self, func: &mut Function, block_id: BlockId) {
        let inst_ids = func.blocks[block_id].instructions.clone();
        let mut stored_values: FxHashMap<MemAddrKey, ValueId> = FxHashMap::default();
        let mut dead: FxHashSet<InstId> = FxHashSet::default();

        for &inst_id in &inst_ids {
            let inst = &func.instructions[inst_id];
            match &inst.kind {
                InstKind::MStore(addr, value) => {
                    let Some(key) = Self::mem_addr_key(func, *addr) else {
                        stored_values.clear();
                        continue;
                    };

                    if stored_values.get(&key).is_some_and(|&stored| stored == *value) {
                        dead.insert(inst_id);
                        self.eliminated_count += 1;
                        continue;
                    }

                    Self::remove_overlapping_map(&mut stored_values, key);
                    stored_values.insert(key, *value);
                }
                kind if Self::can_mutate_memory(kind) => {
                    stored_values.clear();
                }
                _ => {}
            }
        }

        if dead.is_empty() {
            return;
        }

        func.blocks[block_id].instructions.retain(|id| !dead.contains(id));
    }

    fn forward_loads(&mut self, func: &mut Function, block_id: BlockId) {
        let inst_ids = func.blocks[block_id].instructions.clone();
        let inst_results = func.inst_results();
        let mut stored_values: FxHashMap<MemAddrKey, ValueId> = FxHashMap::default();
        let mut replacements: FxHashMap<ValueId, ValueId> = FxHashMap::default();
        let mut dead: FxHashSet<InstId> = FxHashSet::default();

        for &inst_id in &inst_ids {
            let inst = &func.instructions[inst_id];
            match &inst.kind {
                InstKind::MStore(addr, value) => {
                    if let Some(key) = Self::mem_addr_key(func, *addr) {
                        if !Self::remove_overlapping_write_range(
                            func,
                            &mut stored_values,
                            *addr,
                            32,
                        ) {
                            stored_values.clear();
                            continue;
                        }
                        stored_values
                            .insert(key, Function::resolve_replacement(*value, &replacements));
                    } else {
                        stored_values.clear();
                    }
                }
                InstKind::MLoad(addr) => {
                    let Some(key) = Self::mem_addr_key(func, *addr) else {
                        continue;
                    };
                    let Some(&stored_value) = stored_values.get(&key) else {
                        continue;
                    };
                    if let Some(&loaded_value) = inst_results.get(&inst_id) {
                        replacements.insert(loaded_value, stored_value);
                        dead.insert(inst_id);
                    }
                }
                InstKind::MStore8(addr, _)
                    if !Self::remove_overlapping_write_range(
                        func,
                        &mut stored_values,
                        *addr,
                        1,
                    ) =>
                {
                    stored_values.clear();
                }
                InstKind::CalldataCopy(dest, _, size)
                | InstKind::CodeCopy(dest, _, size)
                | InstKind::ReturnDataCopy(dest, _, size) => {
                    let Some(size) = Self::as_u64(func, *size) else {
                        stored_values.clear();
                        continue;
                    };
                    if !Self::remove_overlapping_write_range(func, &mut stored_values, *dest, size)
                    {
                        stored_values.clear();
                    }
                }
                InstKind::ExtCodeCopy(_, dest, _, size) => {
                    let Some(size) = Self::as_u64(func, *size) else {
                        stored_values.clear();
                        continue;
                    };
                    if !Self::remove_overlapping_write_range(func, &mut stored_values, *dest, size)
                    {
                        stored_values.clear();
                    }
                }
                kind if Self::can_mutate_memory(kind) => {
                    stored_values.clear();
                }
                _ => {}
            }
        }

        if dead.is_empty() {
            return;
        }

        func.replace_uses_canonicalized(&replacements);
        self.eliminated_count += dead.len();
        func.blocks[block_id].instructions.retain(|id| !dead.contains(id));
    }

    fn mem_addr_key(func: &Function, value: ValueId) -> Option<MemAddrKey> {
        Self::mem_addr_key_with_depth(func, value, 0)
    }

    fn mem_addr_key_with_depth(
        func: &Function,
        value: ValueId,
        depth: usize,
    ) -> Option<MemAddrKey> {
        if depth > 8 {
            return Some(MemAddrKey::BaseOffset { base: value, offset: 0 });
        }

        match &func.values[value] {
            Value::Immediate(imm) => {
                let addr = imm.as_u256()?;
                u64::try_from(addr).ok().map(MemAddrKey::Const)
            }
            Value::Inst(inst_id) => match func.instructions[*inst_id].kind {
                InstKind::Add(a, b) => Self::add_addr_offset(func, a, b, depth)
                    .or_else(|| Self::add_addr_offset(func, b, a, depth))
                    .or(Some(MemAddrKey::BaseOffset { base: value, offset: 0 })),
                _ => Some(MemAddrKey::BaseOffset { base: value, offset: 0 }),
            },
            Value::Arg { .. } => Some(MemAddrKey::BaseOffset { base: value, offset: 0 }),
            Value::Undef(_) => None,
        }
    }

    fn add_addr_offset(
        func: &Function,
        base: ValueId,
        offset: ValueId,
        depth: usize,
    ) -> Option<MemAddrKey> {
        let offset = Self::as_u64(func, offset)?;
        match Self::mem_addr_key_with_depth(func, base, depth + 1)? {
            MemAddrKey::Const(addr) => addr.checked_add(offset).map(MemAddrKey::Const),
            MemAddrKey::BaseOffset { base, offset: base_offset } => base_offset
                .checked_add(offset)
                .map(|offset| MemAddrKey::BaseOffset { base, offset }),
        }
    }

    fn immutable_copy_key(func: &Function, src: ValueId) -> Option<ImmutableCopyKey> {
        match func.values[src] {
            Value::Inst(inst_id) => match func.instructions[inst_id].kind {
                InstKind::Sub(code_size, len) if Self::is_codesize(func, code_size) => {
                    Some(ImmutableCopyKey { len: Self::as_u64(func, len)?, offset: 0 })
                }
                InstKind::Add(base, offset) => {
                    Self::immutable_copy_key_with_offset(func, base, offset)
                        .or_else(|| Self::immutable_copy_key_with_offset(func, offset, base))
                }
                _ => None,
            },
            _ => None,
        }
    }

    fn immutable_copy_key_with_offset(
        func: &Function,
        base: ValueId,
        offset: ValueId,
    ) -> Option<ImmutableCopyKey> {
        let mut key = Self::immutable_copy_key(func, base)?;
        key.offset = key.offset.checked_add(Self::as_u64(func, offset)?)?;
        Some(key)
    }

    fn is_codesize(func: &Function, value: ValueId) -> bool {
        matches!(func.values[value], Value::Inst(inst_id) if matches!(func.instructions[inst_id].kind, InstKind::CodeSize))
    }

    fn copy_dominates(
        dominators: &crate::analysis::DominatorTree,
        cached: CachedImmutableCopy,
        block: BlockId,
        index: usize,
    ) -> bool {
        if cached.block == block {
            return cached.index < index;
        }
        dominators.dominates(cached.block, block)
    }

    fn as_u64(func: &Function, value: ValueId) -> Option<u64> {
        let value = Self::as_u256(func, value)?;
        u64::try_from(value).ok()
    }

    fn as_u256(func: &Function, value: ValueId) -> Option<U256> {
        func.values[value].as_immediate()?.as_u256()
    }

    fn constant_memory_bytes(
        func: &Function,
        stored_words: &FxHashMap<MemAddrKey, U256>,
        offset: ValueId,
        size: ValueId,
    ) -> Option<Vec<u8>> {
        let offset = Self::as_u64(func, offset)?;
        let size = Self::as_u64(func, size)?;
        if size > 4096 || size % 32 != 0 {
            return None;
        }

        let mut bytes = Vec::with_capacity(size as usize);
        for word_offset in (0..size).step_by(32) {
            let addr = offset.checked_add(word_offset)?;
            let word = stored_words.get(&MemAddrKey::Const(addr))?;
            bytes.extend_from_slice(&word.to_be_bytes::<32>());
        }
        Some(bytes)
    }

    fn overlaps(a: MemAddrKey, b: MemAddrKey) -> bool {
        match (a, b) {
            (MemAddrKey::Const(a), MemAddrKey::Const(b)) => Self::ranges_overlap(a, b),
            (
                MemAddrKey::BaseOffset { base: a_base, offset: a_offset },
                MemAddrKey::BaseOffset { base: b_base, offset: b_offset },
            ) if a_base == b_base => Self::ranges_overlap(a_offset, b_offset),
            _ => true,
        }
    }

    fn ranges_overlap(a: u64, b: u64) -> bool {
        Self::ranges_overlap_size(a, 32, b, 32)
    }

    fn ranges_overlap_size(a: u64, a_size: u64, b: u64, b_size: u64) -> bool {
        let Some(a_end) = a.checked_add(a_size) else {
            return true;
        };
        let Some(b_end) = b.checked_add(b_size) else {
            return true;
        };
        a < b_end && b < a_end
    }

    fn remove_overlapping_map<T>(map: &mut FxHashMap<MemAddrKey, T>, key: MemAddrKey) {
        map.retain(|&stored, _| !Self::overlaps(stored, key));
    }

    fn remove_overlapping_set(set: &mut FxHashSet<MemAddrKey>, key: MemAddrKey) {
        set.retain(|&stored| !Self::overlaps(stored, key));
    }

    fn remove_overlapping_write_range<T>(
        func: &Function,
        map: &mut FxHashMap<MemAddrKey, T>,
        dest: ValueId,
        size: u64,
    ) -> bool {
        let Some(write) = Self::mem_addr_key(func, dest) else {
            return false;
        };
        map.retain(|&stored, _| !Self::ranges_overlap_mem_keys(func, stored, 32, write, size));
        true
    }

    fn insert_full_word_overwritten_range(
        func: &Function,
        overwritten: &mut FxHashSet<MemAddrKey>,
        dest: ValueId,
        size: ValueId,
    ) -> bool {
        let Some(size) = Self::as_u64(func, size) else {
            return false;
        };
        if size % 32 != 0 || size > 4096 {
            return false;
        }

        let Some(base) = Self::mem_addr_key(func, dest) else {
            return false;
        };
        for offset in (0..size).step_by(32) {
            let Some(key) = Self::offset_mem_addr_key(base, offset) else {
                return false;
            };
            overwritten.insert(key);
        }
        true
    }

    fn insert_or_clear_full_word_overwritten_range(
        func: &Function,
        overwritten: &mut FxHashSet<MemAddrKey>,
        dest: ValueId,
        size: ValueId,
    ) {
        if !Self::insert_full_word_overwritten_range(func, overwritten, dest, size) {
            overwritten.clear();
        }
    }

    fn offset_mem_addr_key(key: MemAddrKey, add: u64) -> Option<MemAddrKey> {
        match key {
            MemAddrKey::Const(offset) => offset.checked_add(add).map(MemAddrKey::Const),
            MemAddrKey::BaseOffset { base, offset } => {
                offset.checked_add(add).map(|offset| MemAddrKey::BaseOffset { base, offset })
            }
        }
    }

    fn ranges_overlap_mem_keys(
        func: &Function,
        read: MemAddrKey,
        read_size: u64,
        write: MemAddrKey,
        write_size: u64,
    ) -> bool {
        if (Self::is_scratch_const(read) && Self::is_fmp_heap_key(func, write))
            || (Self::is_fmp_heap_key(func, read) && Self::is_scratch_const(write))
        {
            return false;
        }

        match (read, write) {
            (MemAddrKey::Const(read), MemAddrKey::Const(write)) => {
                Self::ranges_overlap_size(read, read_size, write, write_size)
            }
            (
                MemAddrKey::BaseOffset { base: read_base, offset: read },
                MemAddrKey::BaseOffset { base: write_base, offset: write },
            ) if read_base == write_base => {
                Self::ranges_overlap_size(read, read_size, write, write_size)
            }
            _ => true,
        }
    }

    fn is_scratch_const(key: MemAddrKey) -> bool {
        matches!(key, MemAddrKey::Const(offset) if offset < 128)
    }

    fn is_fmp_heap_key(func: &Function, key: MemAddrKey) -> bool {
        let MemAddrKey::BaseOffset { base, .. } = key else {
            return false;
        };
        Self::is_fmp_heap_value(func, base, 0)
    }

    fn is_fmp_heap_value(func: &Function, value: ValueId, depth: usize) -> bool {
        if depth > 8 {
            return false;
        }
        let Value::Inst(inst_id) = func.values[value] else {
            return false;
        };
        match func.instructions[inst_id].kind {
            InstKind::MLoad(addr) => Self::as_u64(func, addr) == Some(0x40),
            InstKind::Add(a, b) => {
                Self::is_fmp_heap_value(func, a, depth + 1)
                    || Self::is_fmp_heap_value(func, b, depth + 1)
            }
            _ => false,
        }
    }

    fn is_memory_or_gas_observer(kind: &InstKind) -> bool {
        matches!(
            kind,
            InstKind::MStore8(_, _)
                | InstKind::MSize
                | InstKind::MCopy(_, _, _)
                | InstKind::CalldataCopy(_, _, _)
                | InstKind::CodeCopy(_, _, _)
                | InstKind::ReturnDataCopy(_, _, _)
                | InstKind::ExtCodeCopy(_, _, _, _)
                | InstKind::Keccak256(_, _)
                | InstKind::Call { .. }
                | InstKind::StaticCall { .. }
                | InstKind::DelegateCall { .. }
                | InstKind::InternalCall { .. }
                | InstKind::Create(_, _, _)
                | InstKind::Create2(_, _, _, _)
                | InstKind::Log0(_, _)
                | InstKind::Log1(_, _, _)
                | InstKind::Log2(_, _, _, _)
                | InstKind::Log3(_, _, _, _, _)
                | InstKind::Log4(_, _, _, _, _, _)
                | InstKind::Gas
        )
    }

    fn has_frame_observer(func: &Function) -> bool {
        func.blocks.iter().any(|block| {
            block.instructions.iter().any(|&inst_id| {
                matches!(
                    func.instructions[inst_id].kind,
                    InstKind::Gas | InstKind::MSize | InstKind::InternalCall { .. }
                )
            })
        })
    }

    fn internal_frame_read_ranges(func: &Function) -> Option<Vec<(u64, u64)>> {
        let mut reads = Vec::new();

        for block in func.blocks.iter() {
            for &inst_id in &block.instructions {
                match func.instructions[inst_id].kind {
                    InstKind::MLoad(addr) => {
                        if let Some(offset) = Self::internal_frame_offset(func, addr) {
                            reads.push((offset, 32));
                        }
                    }
                    InstKind::Keccak256(offset, size)
                    | InstKind::Log0(offset, size)
                    | InstKind::Create(_, offset, size) => {
                        Self::push_frame_read(func, &mut reads, offset, size)?;
                    }
                    InstKind::Log1(offset, size, _) => {
                        Self::push_frame_read(func, &mut reads, offset, size)?;
                    }
                    InstKind::Log2(offset, size, _, _)
                    | InstKind::MCopy(_, offset, size)
                    | InstKind::Create2(_, offset, size, _) => {
                        Self::push_frame_read(func, &mut reads, offset, size)?;
                    }
                    InstKind::Log3(offset, size, _, _, _) => {
                        Self::push_frame_read(func, &mut reads, offset, size)?;
                    }
                    InstKind::Log4(offset, size, _, _, _, _) => {
                        Self::push_frame_read(func, &mut reads, offset, size)?;
                    }
                    InstKind::Call { args_offset, args_size, .. }
                    | InstKind::StaticCall { args_offset, args_size, .. }
                    | InstKind::DelegateCall { args_offset, args_size, .. } => {
                        Self::push_frame_read(func, &mut reads, args_offset, args_size)?;
                    }
                    _ => {}
                }
            }
        }

        for block in func.blocks.iter() {
            match block.terminator.as_ref() {
                Some(Terminator::ReturnData { offset, size })
                | Some(Terminator::Revert { offset, size }) => {
                    Self::push_frame_read(func, &mut reads, *offset, *size)?;
                }
                _ => {}
            }
        }

        Some(reads)
    }

    fn push_frame_read(
        func: &Function,
        reads: &mut Vec<(u64, u64)>,
        offset: ValueId,
        size: ValueId,
    ) -> Option<()> {
        if let Some(frame_offset) = Self::internal_frame_offset(func, offset) {
            reads.push((frame_offset, Self::as_u64(func, size)?));
        }
        Some(())
    }

    fn internal_frame_offset(func: &Function, value: ValueId) -> Option<u64> {
        Self::internal_frame_offset_with_depth(func, value, 0)
    }

    fn internal_frame_offset_with_depth(
        func: &Function,
        value: ValueId,
        depth: usize,
    ) -> Option<u64> {
        if depth > 8 {
            return None;
        }

        match func.values[value] {
            Value::Inst(inst_id) => match func.instructions[inst_id].kind {
                InstKind::InternalFrameAddr(offset) => Some(offset),
                InstKind::Add(a, b) => Self::internal_frame_add_offset(func, a, b, depth)
                    .or_else(|| Self::internal_frame_add_offset(func, b, a, depth)),
                _ => None,
            },
            _ => None,
        }
    }

    fn internal_frame_add_offset(
        func: &Function,
        base: ValueId,
        offset: ValueId,
        depth: usize,
    ) -> Option<u64> {
        let base = Self::internal_frame_offset_with_depth(func, base, depth + 1)?;
        base.checked_add(Self::as_u64(func, offset)?)
    }

    fn can_mutate_memory(kind: &InstKind) -> bool {
        matches!(
            kind,
            InstKind::MStore8(_, _)
                | InstKind::MCopy(_, _, _)
                | InstKind::CalldataCopy(_, _, _)
                | InstKind::CodeCopy(_, _, _)
                | InstKind::ReturnDataCopy(_, _, _)
                | InstKind::ExtCodeCopy(_, _, _, _)
                | InstKind::Call { .. }
                | InstKind::StaticCall { .. }
                | InstKind::DelegateCall { .. }
                | InstKind::InternalCall { .. }
        )
    }

    fn cross_block_memory_barrier(kind: &InstKind) -> bool {
        matches!(kind, InstKind::MLoad(_)) || Self::is_memory_or_gas_observer(kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{FunctionBuilder, FunctionId};
    use solar_interface::Ident;

    fn test_func() -> Function {
        Function::new(Ident::DUMMY)
    }

    #[test]
    fn removes_overwritten_store() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let addr = builder.imm_u64(128);
        let zero = builder.imm_u64(0);
        let value = builder.imm_u64(42);
        builder.mstore(addr, zero);
        builder.mstore(addr, value);
        builder.stop();

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run(&mut func), 1);
        assert_eq!(func.blocks[func.entry_block].instructions.len(), 1);
    }

    #[test]
    fn forwards_store_observed_only_by_load() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let addr = builder.imm_u64(128);
        let zero = builder.imm_u64(0);
        let value = builder.imm_u64(42);
        builder.mstore(addr, zero);
        let loaded = builder.mload(addr);
        builder.mstore(addr, value);
        builder.ret(vec![loaded]);

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run(&mut func), 2);

        let block = &func.blocks[func.entry_block];
        assert_eq!(block.instructions.len(), 1);
        let Some(Terminator::Return { values }) = &block.terminator else {
            panic!("expected return terminator");
        };
        assert_eq!(values.as_slice(), &[zero]);
    }

    #[test]
    fn gas_is_a_barrier() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let addr = builder.imm_u64(128);
        let zero = builder.imm_u64(0);
        let value = builder.imm_u64(42);
        builder.mstore(addr, zero);
        builder.gas();
        builder.mstore(addr, value);
        builder.stop();

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run(&mut func), 0);
        assert_eq!(func.blocks[func.entry_block].instructions.len(), 3);
    }

    #[test]
    fn handles_distinct_immediate_values_for_same_address() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let addr1 = builder.imm_u64(128);
        let addr2 = builder.imm_u64(128);
        let zero = builder.imm_u64(0);
        let value = builder.imm_u64(42);
        builder.mstore(addr1, zero);
        builder.mstore(addr2, value);
        builder.stop();

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run(&mut func), 1);
        assert_eq!(func.blocks[func.entry_block].instructions.len(), 1);
    }

    #[test]
    fn handles_equivalent_base_offset_addresses() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let base = builder.add_param(crate::mir::MirType::uint256());
        let offset = builder.imm_u64(32);
        let value = builder.imm_u64(42);
        let addr1 = builder.add(base, offset);
        builder.mstore(addr1, value);
        let addr2 = builder.add(base, offset);
        let loaded = builder.mload(addr2);
        builder.ret(vec![loaded]);

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run(&mut func), 1);

        let block = &func.blocks[func.entry_block];
        assert_eq!(block.instructions.len(), 3);
        let Some(Terminator::Return { values }) = &block.terminator else {
            panic!("expected return terminator");
        };
        assert_eq!(values.as_slice(), &[value]);
    }

    #[test]
    fn removes_overwritten_store_to_equivalent_base_offset_address() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let base = builder.add_param(crate::mir::MirType::uint256());
        let offset = builder.imm_u64(32);
        let zero = builder.imm_u64(0);
        let value = builder.imm_u64(42);
        let addr1 = builder.add(base, offset);
        builder.mstore(addr1, zero);
        let addr2 = builder.add(base, offset);
        builder.mstore(addr2, value);
        builder.stop();

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run(&mut func), 1);
        assert_eq!(func.blocks[func.entry_block].instructions.len(), 3);
    }

    #[test]
    fn removes_unused_internal_frame_store() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let frame = builder.internal_frame_addr(192);
        let zero = builder.imm_u64(0);
        builder.mstore(frame, zero);
        builder.ret(vec![zero]);

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run_to_fixpoint(&mut func), 1);
        assert_eq!(func.blocks[func.entry_block].instructions.len(), 1);
    }

    #[test]
    fn reuses_dominated_immutable_copy_load() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let dest = builder.imm_u64(0);
        let immutable_len = builder.imm_u64(64);
        let word_size = builder.imm_u64(32);
        let code_size = builder.codesize();
        let offset = builder.sub(code_size, immutable_len);
        builder.codecopy(dest, offset, word_size);
        let first = builder.mload(dest);
        let next = builder.create_block();
        builder.jump(next);

        builder.switch_to_block(next);
        let code_size = builder.codesize();
        let offset = builder.sub(code_size, immutable_len);
        builder.codecopy(dest, offset, word_size);
        let second = builder.mload(dest);
        let sum = builder.add(first, second);
        builder.ret(vec![sum]);

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run_to_fixpoint(&mut func), 1);

        let active_insts = func.blocks.iter().flat_map(|block| block.instructions.iter().copied());
        let mut code_copies = 0;
        let mut loads = 0;
        let mut stores = 0;
        for inst_id in active_insts {
            match func.instructions[inst_id].kind {
                InstKind::CodeCopy(_, _, _) => code_copies += 1,
                InstKind::MLoad(_) => loads += 1,
                InstKind::MStore(_, _) => stores += 1,
                _ => {}
            }
        }
        assert_eq!(code_copies, 1);
        assert_eq!(loads, 1);
        assert_eq!(stores, 1);

        let add = match &func.values[sum] {
            Value::Inst(inst_id) => &func.instructions[*inst_id].kind,
            _ => panic!("expected add instruction"),
        };
        let InstKind::Add(lhs, rhs) = *add else {
            panic!("expected add instruction");
        };
        assert_eq!(lhs, first);
        assert_eq!(rhs, first);
    }

    #[test]
    fn overlapping_load_blocks_overwritten_store_elimination() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let base = builder.imm_u64(128);
        let one = builder.imm_u64(1);
        let zero = builder.imm_u64(0);
        let value = builder.imm_u64(42);
        builder.mstore(base, zero);
        let overlap = builder.add(base, one);
        builder.mload(overlap);
        builder.mstore(base, value);
        builder.stop();

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run(&mut func), 0);
        assert_eq!(func.blocks[func.entry_block].instructions.len(), 4);
    }

    #[test]
    fn forwards_load_from_store() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let addr = builder.imm_u64(128);
        let value = builder.imm_u64(42);
        builder.mstore(addr, value);
        let loaded = builder.mload(addr);
        builder.ret(vec![loaded]);

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run(&mut func), 1);

        let block = &func.blocks[func.entry_block];
        assert_eq!(block.instructions.len(), 1);
        let Some(Terminator::Return { values }) = &block.terminator else {
            panic!("expected return terminator");
        };
        assert_eq!(values.as_slice(), &[value]);
    }

    #[test]
    fn forwards_load_through_non_memory_operation() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let addr = builder.imm_u64(128);
        let value = builder.imm_u64(42);
        let one = builder.imm_u64(1);
        builder.mstore(addr, value);
        builder.add(one, one);
        let loaded = builder.mload(addr);
        builder.ret(vec![loaded]);

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run(&mut func), 1);

        let block = &func.blocks[func.entry_block];
        assert_eq!(block.instructions.len(), 2);
        let Some(Terminator::Return { values }) = &block.terminator else {
            panic!("expected return terminator");
        };
        assert_eq!(values.as_slice(), &[value]);
    }

    #[test]
    fn does_not_forward_load_across_memory_write() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let addr = builder.imm_u64(128);
        let other_addr = builder.imm_u64(160);
        let value = builder.imm_u64(42);
        let len = builder.imm_u64(32);
        builder.mstore(addr, value);
        builder.mcopy(other_addr, addr, len);
        let loaded = builder.mload(addr);
        builder.ret(vec![loaded]);

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run(&mut func), 0);
        assert_eq!(func.blocks[func.entry_block].instructions.len(), 3);
    }

    #[test]
    fn does_not_forward_load_across_internal_call() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let addr = builder.imm_u64(128);
        let value = builder.imm_u64(42);
        builder.mstore(addr, value);
        builder.internal_call(FunctionId::from_usize(0), Vec::new(), None, 0);
        let loaded = builder.mload(addr);
        builder.ret(vec![loaded]);

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run(&mut func), 0);
        assert_eq!(func.blocks[func.entry_block].instructions.len(), 3);
    }

    #[test]
    fn resolves_chained_forwarded_loads() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let addr1 = builder.imm_u64(128);
        let addr2 = builder.imm_u64(160);
        let value = builder.imm_u64(42);
        builder.mstore(addr1, value);
        let loaded1 = builder.mload(addr1);
        builder.mstore(addr2, loaded1);
        let loaded2 = builder.mload(addr2);
        builder.ret(vec![loaded2]);

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run(&mut func), 2);

        let block = &func.blocks[func.entry_block];
        assert_eq!(block.instructions.len(), 2);
        let Some(Terminator::Return { values }) = &block.terminator else {
            panic!("expected return terminator");
        };
        assert_eq!(values.as_slice(), &[value]);
    }
}
