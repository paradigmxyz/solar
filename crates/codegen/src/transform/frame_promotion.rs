//! Compiler-local scalar promotion.
//!
//! This is Solar's EVM-shaped version of LLVM's mem2reg: promote compiler-owned
//! local slots from memory traffic into SSA values. The pass is deliberately
//! conservative. A slot is promotable only when its address is used as the exact
//! address of full-word `mload`/`mstore` instructions, and the function has no
//! observations that could make removing that memory traffic visible.

use crate::{
    mir::{BlockId, Function, InstId, InstKind, Instruction, MirType, Terminator, Value, ValueId},
    pass::FunctionPass,
    transform::repair_reachability_phis,
};
use solar_data_structures::map::{FxHashMap, FxHashSet};
use std::collections::VecDeque;

const LOW_MEMORY_START: u64 = 0x80;

/// Statistics for one frame promotion run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FramePromotionStats {
    /// Number of distinct compiler-local slots promoted.
    pub slots_promoted: usize,
    /// Number of local-slot loads replaced by SSA values.
    pub loads_promoted: usize,
    /// Number of local-slot stores removed.
    pub stores_promoted: usize,
    /// Number of phi nodes inserted.
    pub phis_inserted: usize,
}

impl FramePromotionStats {
    /// Returns the total number of MIR edits made by this pass.
    pub const fn total(self) -> usize {
        self.loads_promoted + self.stores_promoted + self.phis_inserted
    }
}

/// Promotes non-escaping compiler-local slots to SSA values.
#[derive(Debug, Default)]
pub struct FrameSlotPromoter {
    stats: FramePromotionStats,
}

/// Function pass for internal-frame scalar promotion.
pub struct FrameSlotPromotionPass;

impl FunctionPass for FrameSlotPromotionPass {
    fn name(&self) -> &str {
        "frame-slot-promotion"
    }

    fn run_on_function(&mut self, func: &mut Function) -> bool {
        let changed = FrameSlotPromoter::new().run(func).total() != 0;
        repair_reachability_phis(func);
        changed
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum PromotableSlot {
    InternalFrame(u64),
    ExternalLocal(u64),
}

#[derive(Clone, Debug)]
struct PendingPhi {
    block: BlockId,
    inst: InstId,
    value: ValueId,
    incoming: Vec<(BlockId, ValueId)>,
}

struct SlotSsaBuilder<'a> {
    slot: PromotableSlot,
    reachable: &'a FxHashSet<BlockId>,
    inst_results: &'a FxHashMap<InstId, ValueId>,
    replacements: FxHashMap<ValueId, ValueId>,
    dead: FxHashSet<InstId>,
    entry_values: FxHashMap<BlockId, Option<ValueId>>,
    exit_values: FxHashMap<BlockId, Option<ValueId>>,
    processing_exit: FxHashSet<BlockId>,
    phis: FxHashMap<BlockId, PendingPhi>,
    failed: bool,
    loads_promoted: usize,
    stores_promoted: usize,
}

impl FrameSlotPromoter {
    /// Creates a new compiler-local-slot promoter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the statistics from the most recent run.
    pub const fn stats(&self) -> FramePromotionStats {
        self.stats
    }

    /// Runs compiler-local-slot promotion on a function.
    pub fn run(&mut self, func: &mut Function) -> FramePromotionStats {
        self.stats = FramePromotionStats::default();

        if Self::has_promotion_barrier(func) {
            return self.stats;
        }

        let reachable = Self::reachable_blocks(func);
        let inst_results = Self::inst_results(func);
        let Some(slots) = Self::collect_promotable_slots(func, &reachable, &inst_results) else {
            return self.stats;
        };

        for slot in slots {
            let inst_results = Self::inst_results(func);
            let mut builder = SlotSsaBuilder::new(slot, &reachable, &inst_results);
            if builder.run(func) {
                self.stats.slots_promoted += 1;
                self.stats.loads_promoted += builder.loads_promoted;
                self.stats.stores_promoted += builder.stores_promoted;
                self.stats.phis_inserted += builder.phis.len();
                builder.apply(func);
            }
        }

        self.stats
    }

    fn has_promotion_barrier(func: &Function) -> bool {
        func.blocks.iter().any(|block| {
            block.instructions.iter().any(|&inst_id| {
                matches!(
                    func.instructions[inst_id].kind,
                    InstKind::Gas
                        | InstKind::MSize
                        | InstKind::Call { .. }
                        | InstKind::StaticCall { .. }
                        | InstKind::DelegateCall { .. }
                        | InstKind::Create(_, _, _)
                        | InstKind::Create2(_, _, _, _)
                )
            })
        })
    }

    fn reachable_blocks(func: &Function) -> FxHashSet<BlockId> {
        let mut reachable = FxHashSet::default();
        let mut worklist = VecDeque::new();

        reachable.insert(func.entry_block);
        worklist.push_back(func.entry_block);

        while let Some(block) = worklist.pop_front() {
            let Some(term) = &func.blocks[block].terminator else {
                continue;
            };
            for succ in term.successors() {
                if reachable.insert(succ) {
                    worklist.push_back(succ);
                }
            }
        }

        reachable
    }

    fn collect_promotable_slots(
        func: &Function,
        reachable: &FxHashSet<BlockId>,
        inst_results: &FxHashMap<InstId, ValueId>,
    ) -> Option<Vec<PromotableSlot>> {
        let mut loads: FxHashMap<PromotableSlot, usize> = FxHashMap::default();
        let mut stores: FxHashMap<PromotableSlot, usize> = FxHashMap::default();
        let mut internal_frame_escaped = false;

        for (block_id, block) in func.blocks.iter_enumerated() {
            if !reachable.contains(&block_id) {
                continue;
            }

            for &inst_id in &block.instructions {
                let kind = &func.instructions[inst_id].kind;
                match *kind {
                    InstKind::MLoad(addr) => {
                        if let Some(slot) = Self::promotable_slot(func, addr) {
                            *loads.entry(slot).or_default() += 1;
                        }
                    }
                    InstKind::MStore(addr, value) => {
                        if let Some(slot) = Self::promotable_slot(func, addr) {
                            if Self::internal_frame_offset(func, value).is_some() {
                                internal_frame_escaped = true;
                            }
                            *stores.entry(slot).or_default() += 1;
                        } else if Self::internal_frame_offset(func, value).is_some() {
                            internal_frame_escaped = true;
                        }
                    }
                    _ => {
                        if matches!(kind, InstKind::Add(_, _))
                            && inst_results
                                .get(&inst_id)
                                .and_then(|&value| Self::internal_frame_offset(func, value))
                                .is_some()
                        {
                            continue;
                        }
                        if kind
                            .operands()
                            .iter()
                            .any(|&value| Self::internal_frame_offset(func, value).is_some())
                        {
                            internal_frame_escaped = true;
                        }
                    }
                }
            }

            if let Some(term) = &block.terminator
                && term
                    .operands()
                    .iter()
                    .any(|&value| Self::internal_frame_offset(func, value).is_some())
            {
                internal_frame_escaped = true;
            }
        }

        let mut slots: Vec<PromotableSlot> = loads
            .into_iter()
            .filter_map(|(slot, load_count)| {
                let has_store = stores.get(&slot).copied().unwrap_or(0) > 0;
                (load_count > 0 && has_store).then_some(slot)
            })
            .filter(|slot| match *slot {
                PromotableSlot::InternalFrame(_) => !internal_frame_escaped,
                PromotableSlot::ExternalLocal(addr) => Self::external_local_slot_safe(func, addr),
            })
            .collect();
        slots.sort_unstable();
        Some(slots)
    }

    fn inst_results(func: &Function) -> FxHashMap<InstId, ValueId> {
        func.values
            .iter_enumerated()
            .filter_map(|(value_id, value)| {
                if let Value::Inst(inst_id) = value { Some((*inst_id, value_id)) } else { None }
            })
            .collect()
    }

    fn promotable_slot(func: &Function, value: ValueId) -> Option<PromotableSlot> {
        Self::internal_frame_offset(func, value)
            .map(PromotableSlot::InternalFrame)
            .or_else(|| Self::external_local_addr(func, value).map(PromotableSlot::ExternalLocal))
    }

    fn external_local_addr(func: &Function, value: ValueId) -> Option<u64> {
        let addr = Self::as_u64(func, value)?;
        let local_end = LOW_MEMORY_START.checked_add(func.internal_frame_size)?;
        (addr >= LOW_MEMORY_START
            && addr < local_end
            && (addr - LOW_MEMORY_START).is_multiple_of(32))
        .then_some(addr)
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

    fn as_u64(func: &Function, value: ValueId) -> Option<u64> {
        let value = func.values[value].as_immediate()?.as_u256()?;
        u64::try_from(value).ok()
    }

    fn external_local_slot_safe(func: &Function, slot_addr: u64) -> bool {
        if Self::ranges_overlap(slot_addr, 32, LOW_MEMORY_START, func.external_static_return_size) {
            return false;
        }

        for block in func.blocks.iter() {
            for &inst_id in &block.instructions {
                if Self::inst_may_observe_external_slot(
                    func,
                    &func.instructions[inst_id].kind,
                    slot_addr,
                ) {
                    return false;
                }
            }
            if let Some(term) = &block.terminator
                && Self::terminator_may_observe_external_slot(func, term, slot_addr)
            {
                return false;
            }
        }

        true
    }

    fn inst_may_observe_external_slot(func: &Function, kind: &InstKind, slot_addr: u64) -> bool {
        match *kind {
            InstKind::MLoad(addr) | InstKind::MStore(addr, _) => {
                !Self::is_exact_external_slot_access(func, addr, slot_addr)
                    && Self::memory_range_may_overlap(func, addr, Some(32), slot_addr)
            }
            InstKind::MStore8(addr, _) => {
                Self::memory_range_may_overlap(func, addr, Some(1), slot_addr)
            }
            InstKind::Keccak256(addr, size)
            | InstKind::Log0(addr, size)
            | InstKind::ReturnDataCopy(addr, _, size)
            | InstKind::CodeCopy(addr, _, size)
            | InstKind::CalldataCopy(addr, _, size) => {
                Self::memory_range_may_overlap(func, addr, Self::as_u64(func, size), slot_addr)
            }
            InstKind::MCopy(dest, src, size) => {
                let size = Self::as_u64(func, size);
                Self::memory_range_may_overlap(func, dest, size, slot_addr)
                    || Self::memory_range_may_overlap(func, src, size, slot_addr)
            }
            InstKind::ExtCodeCopy(_, dest, _, size) => {
                Self::memory_range_may_overlap(func, dest, Self::as_u64(func, size), slot_addr)
            }
            InstKind::Log1(addr, size, _)
            | InstKind::Log2(addr, size, _, _)
            | InstKind::Log3(addr, size, _, _, _)
            | InstKind::Log4(addr, size, _, _, _, _) => {
                Self::memory_range_may_overlap(func, addr, Self::as_u64(func, size), slot_addr)
            }
            InstKind::Call { .. }
            | InstKind::StaticCall { .. }
            | InstKind::DelegateCall { .. }
            | InstKind::InternalCall { .. }
            | InstKind::Create(_, _, _)
            | InstKind::Create2(_, _, _, _)
            | InstKind::MSize => true,
            _ => false,
        }
    }

    fn terminator_may_observe_external_slot(
        func: &Function,
        term: &Terminator,
        slot_addr: u64,
    ) -> bool {
        match term {
            Terminator::Revert { offset, size } | Terminator::ReturnData { offset, size } => {
                Self::memory_range_may_overlap(func, *offset, Self::as_u64(func, *size), slot_addr)
            }
            Terminator::Jump(_)
            | Terminator::Branch { .. }
            | Terminator::Switch { .. }
            | Terminator::Return { .. }
            | Terminator::Stop
            | Terminator::Invalid
            | Terminator::SelfDestruct { .. } => false,
        }
    }

    fn is_exact_external_slot_access(func: &Function, addr: ValueId, slot_addr: u64) -> bool {
        Self::external_local_addr(func, addr) == Some(slot_addr)
    }

    fn memory_range_may_overlap(
        func: &Function,
        addr: ValueId,
        size: Option<u64>,
        slot_addr: u64,
    ) -> bool {
        let Some(size) = size else { return true };
        if size == 0 {
            return false;
        }
        let Some(addr) = Self::as_u64(func, addr) else { return true };
        Self::ranges_overlap(addr, size, slot_addr, 32)
    }

    fn ranges_overlap(a_start: u64, a_size: u64, b_start: u64, b_size: u64) -> bool {
        let Some(a_end) = a_start.checked_add(a_size) else { return true };
        let Some(b_end) = b_start.checked_add(b_size) else { return true };
        a_start < b_end && b_start < a_end
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

impl<'a> SlotSsaBuilder<'a> {
    fn new(
        slot: PromotableSlot,
        reachable: &'a FxHashSet<BlockId>,
        inst_results: &'a FxHashMap<InstId, ValueId>,
    ) -> Self {
        Self {
            slot,
            reachable,
            inst_results,
            replacements: FxHashMap::default(),
            dead: FxHashSet::default(),
            entry_values: FxHashMap::default(),
            exit_values: FxHashMap::default(),
            processing_exit: FxHashSet::default(),
            phis: FxHashMap::default(),
            failed: false,
            loads_promoted: 0,
            stores_promoted: 0,
        }
    }

    fn run(&mut self, func: &mut Function) -> bool {
        let block_ids: Vec<BlockId> = func.blocks.indices().collect();
        for block in block_ids {
            if self.reachable.contains(&block) {
                self.rewrite_block(func, block);
            }
            if self.failed {
                return false;
            }
        }
        true
    }

    fn apply(self, func: &mut Function) {
        for pending in self.phis.values() {
            func.instructions[pending.inst].kind = InstKind::Phi(pending.incoming.clone());
            let insert_pos = func.blocks[pending.block]
                .instructions
                .iter()
                .take_while(|&&inst_id| matches!(func.instructions[inst_id].kind, InstKind::Phi(_)))
                .count();
            func.blocks[pending.block].instructions.insert(insert_pos, pending.inst);
        }

        FrameSlotPromoter::replace_uses(func, &self.replacements);

        for block in func.blocks.iter_mut() {
            block.instructions.retain(|id| !self.dead.contains(id));
        }
    }

    fn rewrite_block(&mut self, func: &mut Function, block: BlockId) {
        let mut current = self.entry_value(func, block);
        if self.failed {
            return;
        }

        let insts = func.blocks[block].instructions.clone();
        for inst_id in insts {
            match func.instructions[inst_id].kind {
                InstKind::MLoad(addr)
                    if FrameSlotPromoter::promotable_slot(func, addr) == Some(self.slot) =>
                {
                    let Some(value) = current else {
                        self.failed = true;
                        return;
                    };
                    if let Some(&load_value) = self.inst_results.get(&inst_id) {
                        self.replacements.insert(
                            load_value,
                            FrameSlotPromoter::resolve_replacement(&self.replacements, value),
                        );
                        self.dead.insert(inst_id);
                        self.loads_promoted += 1;
                    }
                }
                InstKind::MStore(addr, value)
                    if FrameSlotPromoter::promotable_slot(func, addr) == Some(self.slot) =>
                {
                    current =
                        Some(FrameSlotPromoter::resolve_replacement(&self.replacements, value));
                    self.dead.insert(inst_id);
                    self.stores_promoted += 1;
                }
                _ => {}
            }
        }
    }

    fn entry_value(&mut self, func: &mut Function, block: BlockId) -> Option<ValueId> {
        if let Some(&value) = self.entry_values.get(&block) {
            return value;
        }

        let preds: Vec<BlockId> = func.blocks[block]
            .predecessors
            .iter()
            .copied()
            .filter(|pred| self.reachable.contains(pred))
            .collect();

        let value = match preds.as_slice() {
            [] => None,
            [pred] if *pred != block => self.exit_value(func, *pred),
            _ => Some(self.block_phi(func, block, &preds)),
        };

        self.entry_values.insert(block, value);
        value
    }

    fn exit_value(&mut self, func: &mut Function, block: BlockId) -> Option<ValueId> {
        if let Some(&value) = self.exit_values.get(&block) {
            return value;
        }
        if self.processing_exit.contains(&block) {
            return self.entry_value(func, block);
        }

        self.processing_exit.insert(block);
        let mut current = self.entry_value(func, block);

        let insts = func.blocks[block].instructions.clone();
        for inst_id in insts {
            if let InstKind::MStore(addr, value) = func.instructions[inst_id].kind
                && FrameSlotPromoter::promotable_slot(func, addr) == Some(self.slot)
            {
                current = Some(FrameSlotPromoter::resolve_replacement(&self.replacements, value));
            }
        }

        self.processing_exit.remove(&block);
        self.exit_values.insert(block, current);
        current
    }

    fn block_phi(&mut self, func: &mut Function, block: BlockId, preds: &[BlockId]) -> ValueId {
        if let Some(phi) = self.phis.get(&block) {
            return phi.value;
        }

        let inst =
            func.alloc_inst(Instruction::new(InstKind::Phi(Vec::new()), Some(MirType::uint256())));
        let value = func.alloc_value(Value::Inst(inst));
        self.phis.insert(
            block,
            PendingPhi { block, inst, value, incoming: Vec::with_capacity(preds.len()) },
        );
        self.entry_values.insert(block, Some(value));

        let mut incoming = Vec::with_capacity(preds.len());
        for &pred in preds {
            let Some(pred_value) = self.exit_value(func, pred) else {
                self.failed = true;
                return value;
            };
            incoming.push((
                pred,
                FrameSlotPromoter::resolve_replacement(&self.replacements, pred_value),
            ));
        }

        if let Some(phi) = self.phis.get_mut(&block) {
            phi.incoming = incoming;
        }
        value
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

    fn count_active_frame_ops(func: &Function, offset: u64) -> (usize, usize) {
        let mut loads = 0;
        let mut stores = 0;

        for block in func.blocks.iter() {
            for &inst_id in &block.instructions {
                match func.instructions[inst_id].kind {
                    InstKind::MLoad(addr)
                        if FrameSlotPromoter::internal_frame_offset(func, addr) == Some(offset) =>
                    {
                        loads += 1;
                    }
                    InstKind::MStore(addr, _)
                        if FrameSlotPromoter::internal_frame_offset(func, addr) == Some(offset) =>
                    {
                        stores += 1;
                    }
                    _ => {}
                }
            }
        }

        (loads, stores)
    }

    #[test]
    fn promotes_loop_carried_frame_slot() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let header = builder.create_block();
        let body = builder.create_block();
        let exit = builder.create_block();

        let frame = builder.internal_frame_addr(128);
        let zero = builder.imm_u64(0);
        let limit = builder.imm_u64(4);
        builder.mstore(frame, zero);
        builder.jump(header);

        builder.switch_to_block(header);
        let header_frame = builder.internal_frame_addr(128);
        let i = builder.mload(header_frame);
        let cond = builder.lt(i, limit);
        builder.branch(cond, body, exit);

        builder.switch_to_block(body);
        let body_frame = builder.internal_frame_addr(128);
        let current = builder.mload(body_frame);
        let one = builder.imm_u64(1);
        let next = builder.add(current, one);
        let body_frame = builder.internal_frame_addr(128);
        builder.mstore(body_frame, next);
        builder.jump(header);

        builder.switch_to_block(exit);
        let exit_frame = builder.internal_frame_addr(128);
        let result = builder.mload(exit_frame);
        builder.ret([result]);

        let mut pass = FrameSlotPromoter::new();
        let stats = pass.run(&mut func);

        assert_eq!(stats.slots_promoted, 1);
        assert_eq!(stats.loads_promoted, 3);
        assert_eq!(stats.stores_promoted, 2);
        assert_eq!(stats.phis_inserted, 1);
        assert_eq!(count_active_frame_ops(&func, 128), (0, 0));

        let Some(Terminator::Return { values }) = &func.blocks[exit].terminator else {
            panic!("expected return");
        };
        assert_ne!(values.as_slice(), &[result]);
    }

    #[test]
    fn skips_escaped_frame_address() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let frame = builder.internal_frame_addr(128);
        let value = builder.imm_u64(42);
        let size = builder.imm_u64(32);
        builder.mstore(frame, value);
        let hash = builder.keccak256(frame, size);
        let loaded = builder.mload(frame);
        builder.ret([hash, loaded]);

        let mut pass = FrameSlotPromoter::new();
        let stats = pass.run(&mut func);

        assert_eq!(stats.total(), 0);
        assert_eq!(count_active_frame_ops(&func, 128), (1, 1));
    }

    #[test]
    fn skips_gas_observed_functions() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let frame = builder.internal_frame_addr(128);
        let value = builder.imm_u64(42);
        builder.mstore(frame, value);
        builder.gas();
        let loaded = builder.mload(frame);
        builder.ret([loaded]);

        let mut pass = FrameSlotPromoter::new();
        let stats = pass.run(&mut func);

        assert_eq!(stats.total(), 0);
        assert_eq!(count_active_frame_ops(&func, 128), (1, 1));
    }

    #[test]
    fn promotes_across_internal_calls() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let frame = builder.internal_frame_addr(128);
        let value = builder.imm_u64(42);
        builder.mstore(frame, value);
        builder.internal_call(FunctionId::from_usize(0), Vec::new(), None, 0);
        let loaded = builder.mload(frame);
        builder.ret([loaded]);

        let mut pass = FrameSlotPromoter::new();
        let stats = pass.run(&mut func);

        assert_eq!(stats.slots_promoted, 1);
        assert_eq!(count_active_frame_ops(&func, 128), (0, 0));
    }

    #[test]
    fn promotes_frame_address_with_constant_offset() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let base = builder.internal_frame_addr(128);
        let offset = builder.imm_u64(32);
        let addr = builder.add(base, offset);
        let value = builder.imm_u64(99);
        builder.mstore(addr, value);
        let base = builder.internal_frame_addr(128);
        let offset = builder.imm_u64(32);
        let addr = builder.add(base, offset);
        let loaded = builder.mload(addr);
        builder.ret([loaded]);

        let mut pass = FrameSlotPromoter::new();
        let stats = pass.run(&mut func);

        assert_eq!(stats.slots_promoted, 1);
        assert_eq!(count_active_frame_ops(&func, 160), (0, 0));
    }
}
