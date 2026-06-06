//! Local dead memory optimization.
//!
//! This pass removes full-word `mstore` instructions that are overwritten by a
//! later full-word `mstore` to the same exact address within the same basic
//! block, before any operation can observe memory or gas. It also forwards
//! same-block `mload` instructions from the latest exact-address `mstore` when
//! no intervening operation can mutate memory.

use crate::{
    mir::{BlockId, Function, InstId, InstKind, Terminator, Value, ValueId},
    pass::FunctionPass,
};
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
        let inst_results = Self::inst_results(func);
        let dominators = Self::compute_dominators(func);
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
                if Self::mem_addr_key_static(func, dest)
                    != Self::mem_addr_key_static(func, load_addr)
                {
                    continue;
                }
                let Some(&loaded_value) = inst_results.get(&load) else {
                    continue;
                };

                if let Some(cached_copy) = cached.get(&key).copied()
                    && Self::copy_dominates(&dominators, cached_copy, block_id, index)
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

        Self::replace_uses(func, &replacements);
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
        self.forward_loads(func, block_id);

        let inst_ids = func.blocks[block_id].instructions.clone();
        let mut overwritten: FxHashSet<MemAddrKey> = FxHashSet::default();
        let mut dead: FxHashSet<InstId> = FxHashSet::default();

        for &inst_id in inst_ids.iter().rev() {
            let inst = &func.instructions[inst_id];
            match &inst.kind {
                InstKind::MStore(addr, _) => {
                    if let Some(key) = self.mem_addr_key(func, *addr) {
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
                    if let Some(key) = self.mem_addr_key(func, *addr) {
                        Self::remove_overlapping_set(&mut overwritten, key);
                    } else {
                        overwritten.clear();
                    }
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

    fn forward_loads(&mut self, func: &mut Function, block_id: BlockId) {
        let inst_ids = func.blocks[block_id].instructions.clone();
        let inst_results = Self::inst_results(func);
        let mut stored_values: FxHashMap<MemAddrKey, ValueId> = FxHashMap::default();
        let mut replacements: FxHashMap<ValueId, ValueId> = FxHashMap::default();
        let mut dead: FxHashSet<InstId> = FxHashSet::default();

        for &inst_id in &inst_ids {
            let inst = &func.instructions[inst_id];
            match &inst.kind {
                InstKind::MStore(addr, value) => {
                    if let Some(key) = self.mem_addr_key(func, *addr) {
                        Self::remove_overlapping_map(&mut stored_values, key);
                        stored_values.insert(key, Self::resolve_replacement(&replacements, *value));
                    } else {
                        stored_values.clear();
                    }
                }
                InstKind::MLoad(addr) => {
                    let Some(key) = self.mem_addr_key(func, *addr) else {
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
                kind if Self::can_mutate_memory(kind) => {
                    stored_values.clear();
                }
                _ => {}
            }
        }

        if dead.is_empty() {
            return;
        }

        Self::replace_uses(func, &replacements);
        self.eliminated_count += dead.len();
        func.blocks[block_id].instructions.retain(|id| !dead.contains(id));
    }

    fn inst_results(func: &Function) -> FxHashMap<InstId, ValueId> {
        func.values
            .iter_enumerated()
            .filter_map(|(value_id, value)| {
                if let Value::Inst(inst_id) = value { Some((*inst_id, value_id)) } else { None }
            })
            .collect()
    }

    fn resolve_replacement(
        replacements: &FxHashMap<ValueId, ValueId>,
        mut value: ValueId,
    ) -> ValueId {
        while let Some(&replacement) = replacements.get(&value) {
            value = replacement;
        }
        value
    }

    fn mem_addr_key(&self, func: &Function, value: ValueId) -> Option<MemAddrKey> {
        Self::mem_addr_key_static(func, value)
    }

    fn mem_addr_key_static(func: &Function, value: ValueId) -> Option<MemAddrKey> {
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
            Value::Arg { .. } | Value::Phi { .. } => {
                Some(MemAddrKey::BaseOffset { base: value, offset: 0 })
            }
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

    fn compute_dominators(func: &Function) -> FxHashMap<BlockId, FxHashSet<BlockId>> {
        let all_blocks: FxHashSet<BlockId> = func.blocks.indices().collect();
        let mut dominators = FxHashMap::default();

        for block_id in func.blocks.indices() {
            if block_id == func.entry_block {
                dominators.insert(block_id, FxHashSet::from_iter([block_id]));
            } else {
                dominators.insert(block_id, all_blocks.clone());
            }
        }

        let mut changed = true;
        while changed {
            changed = false;
            for (block_id, block) in func.blocks.iter_enumerated() {
                if block_id == func.entry_block {
                    continue;
                }

                let mut new_doms: Option<FxHashSet<BlockId>> = None;
                for &pred in &block.predecessors {
                    let Some(pred_doms) = dominators.get(&pred) else {
                        continue;
                    };
                    match &mut new_doms {
                        Some(doms) => doms.retain(|block| pred_doms.contains(block)),
                        None => new_doms = Some(pred_doms.clone()),
                    }
                }

                let mut new_doms = new_doms.unwrap_or_default();
                new_doms.insert(block_id);
                if dominators.get(&block_id) != Some(&new_doms) {
                    dominators.insert(block_id, new_doms);
                    changed = true;
                }
            }
        }

        dominators
    }

    fn copy_dominates(
        dominators: &FxHashMap<BlockId, FxHashSet<BlockId>>,
        cached: CachedImmutableCopy,
        block: BlockId,
        index: usize,
    ) -> bool {
        if cached.block == block {
            return cached.index < index;
        }
        dominators.get(&block).is_some_and(|doms| doms.contains(&cached.block))
    }

    fn as_u64(func: &Function, value: ValueId) -> Option<u64> {
        let value = func.values[value].as_immediate()?.as_u256()?;
        u64::try_from(value).ok()
    }

    fn overlaps(a: MemAddrKey, b: MemAddrKey) -> bool {
        match (a, b) {
            (MemAddrKey::Const(a), MemAddrKey::Const(b)) => Self::ranges_overlap(a, b),
            (
                MemAddrKey::BaseOffset { base: a_base, offset: a_offset },
                MemAddrKey::BaseOffset { base: b_base, offset: b_offset },
            ) if a_base == b_base => Self::ranges_overlap(a_offset, b_offset),
            _ => false,
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

    fn replace_uses(func: &mut Function, replacements: &FxHashMap<ValueId, ValueId>) {
        if replacements.is_empty() {
            return;
        }

        for inst in func.instructions.iter_mut() {
            Self::replace_inst_operands(&mut inst.kind, replacements);
        }
        for value in func.values.iter_mut() {
            if let Value::Phi { incoming, .. } = value {
                for (_, value) in incoming {
                    if replacements.contains_key(value) {
                        *value = Self::resolve_replacement(replacements, *value);
                    }
                }
            }
        }
        for block in func.blocks.iter_mut() {
            if let Some(term) = &mut block.terminator {
                Self::replace_terminator_operands(term, replacements);
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn replace_inst_operands(kind: &mut InstKind, replacements: &FxHashMap<ValueId, ValueId>) {
        let replace = |value: &mut ValueId| {
            if replacements.contains_key(value) {
                *value = Self::resolve_replacement(replacements, *value);
            }
        };

        match kind {
            InstKind::Add(a, b)
            | InstKind::Sub(a, b)
            | InstKind::Mul(a, b)
            | InstKind::Div(a, b)
            | InstKind::SDiv(a, b)
            | InstKind::Mod(a, b)
            | InstKind::SMod(a, b)
            | InstKind::Exp(a, b)
            | InstKind::And(a, b)
            | InstKind::Or(a, b)
            | InstKind::Xor(a, b)
            | InstKind::Shl(a, b)
            | InstKind::Shr(a, b)
            | InstKind::Sar(a, b)
            | InstKind::Byte(a, b)
            | InstKind::Lt(a, b)
            | InstKind::Gt(a, b)
            | InstKind::SLt(a, b)
            | InstKind::SGt(a, b)
            | InstKind::Eq(a, b)
            | InstKind::MStore(a, b)
            | InstKind::MStore8(a, b)
            | InstKind::SStore(a, b)
            | InstKind::TStore(a, b)
            | InstKind::Keccak256(a, b)
            | InstKind::Log0(a, b)
            | InstKind::SignExtend(a, b) => {
                replace(a);
                replace(b);
            }
            InstKind::Not(a)
            | InstKind::IsZero(a)
            | InstKind::MLoad(a)
            | InstKind::SLoad(a)
            | InstKind::TLoad(a)
            | InstKind::CalldataLoad(a)
            | InstKind::ExtCodeSize(a)
            | InstKind::ExtCodeHash(a)
            | InstKind::Balance(a)
            | InstKind::BlockHash(a)
            | InstKind::BlobHash(a) => {
                replace(a);
            }
            InstKind::AddMod(a, b, c)
            | InstKind::MulMod(a, b, c)
            | InstKind::MCopy(a, b, c)
            | InstKind::CalldataCopy(a, b, c)
            | InstKind::CodeCopy(a, b, c)
            | InstKind::ReturnDataCopy(a, b, c)
            | InstKind::Create(a, b, c)
            | InstKind::Log1(a, b, c)
            | InstKind::Select(a, b, c) => {
                replace(a);
                replace(b);
                replace(c);
            }
            InstKind::ExtCodeCopy(a, b, c, d)
            | InstKind::Create2(a, b, c, d)
            | InstKind::Log2(a, b, c, d) => {
                replace(a);
                replace(b);
                replace(c);
                replace(d);
            }
            InstKind::Log3(a, b, c, d, e) => {
                replace(a);
                replace(b);
                replace(c);
                replace(d);
                replace(e);
            }
            InstKind::Log4(a, b, c, d, e, f) => {
                replace(a);
                replace(b);
                replace(c);
                replace(d);
                replace(e);
                replace(f);
            }
            InstKind::Call { gas, addr, value, args_offset, args_size, ret_offset, ret_size } => {
                replace(gas);
                replace(addr);
                replace(value);
                replace(args_offset);
                replace(args_size);
                replace(ret_offset);
                replace(ret_size);
            }
            InstKind::StaticCall { gas, addr, args_offset, args_size, ret_offset, ret_size }
            | InstKind::DelegateCall { gas, addr, args_offset, args_size, ret_offset, ret_size } => {
                replace(gas);
                replace(addr);
                replace(args_offset);
                replace(args_size);
                replace(ret_offset);
                replace(ret_size);
            }
            InstKind::InternalCall { args, .. } => {
                for arg in args {
                    replace(arg);
                }
            }
            InstKind::Phi(incoming) => {
                for (_, value) in incoming {
                    replace(value);
                }
            }
            InstKind::MSize
            | InstKind::CalldataSize
            | InstKind::InternalFrameAddr(_)
            | InstKind::CodeSize
            | InstKind::ReturnDataSize
            | InstKind::Caller
            | InstKind::CallValue
            | InstKind::Origin
            | InstKind::GasPrice
            | InstKind::Coinbase
            | InstKind::Timestamp
            | InstKind::BlockNumber
            | InstKind::PrevRandao
            | InstKind::GasLimit
            | InstKind::ChainId
            | InstKind::Address
            | InstKind::SelfBalance
            | InstKind::Gas
            | InstKind::BaseFee
            | InstKind::BlobBaseFee => {}
        }
    }

    fn replace_terminator_operands(
        term: &mut Terminator,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) {
        let replace = |value: &mut ValueId| {
            if replacements.contains_key(value) {
                *value = Self::resolve_replacement(replacements, *value);
            }
        };

        match term {
            Terminator::Jump(_) | Terminator::Stop | Terminator::Invalid => {}
            Terminator::Branch { condition, .. } => replace(condition),
            Terminator::Switch { value, cases, .. } => {
                replace(value);
                for (case_value, _) in cases {
                    replace(case_value);
                }
            }
            Terminator::Return { values } => {
                for value in values {
                    replace(value);
                }
            }
            Terminator::Revert { offset, size } | Terminator::ReturnData { offset, size } => {
                replace(offset);
                replace(size);
            }
            Terminator::SelfDestruct { recipient } => replace(recipient),
        }
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
