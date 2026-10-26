//! Scalar replacement of non-escaping memory-object allocations.
//!
//! A constant-size memory allocation that never escapes and is accessed
//! only by word loads and stores at constant offsets can be dissolved into
//! SSA values: each field store feeds the matching field load directly. The
//! backing allocation remains because its free-memory-pointer bump and failure
//! behavior are observable independently of accesses through its result.
//!
//! Eligible accesses may span branches and loops. The pruned SSA constructor
//! shared with frame promotion inserts phis only where a field is live-in and
//! rejects loads lacking a reaching store on any path. All fields are planned
//! together before mutation, including full-object zero fills. The allocation
//! stays in place. Full-width pointer casts retain the same object identity;
//! escapes, narrowing casts, dynamic indices and partial/unknown accesses reject promotion.
//! Run before memory-object lowering, while `ptradd` keeps object offsets explicit.
//!
//! One walk over the function serves every candidate: it maps each cast or constant `ptradd` to
//! its root object and offset, collects each object's accesses in layout order, and records the
//! first use that rules the object out. A second walk, made when a candidate first needs it,
//! files the memory effects of every other instruction by the allocation they touch, so an
//! object only looks at its own entries. Promotion rewrites uses across the function, so the
//! first one discards the walks and the next candidate starts afresh; failed candidates leave
//! the function as it was and share the walks. The decisions equal those of checking each object
//! against the whole function in turn.

use crate::mir::{
    BlockId, Function, Immediate, InstId, InstKind, Module, PtrLocation, Value, ValueId,
    analysis::{
        Access, AddressSpace, AliasAnalysis, CfgInfo, Location, LocationSize, MemoryBase,
        MemoryLocation, ModRef,
    },
    memory::EvmMemoryLayout,
    pass::{MirPass, run_function_pass_with_alias},
    transform::frame_promotion::{SlotAccessInfo, promote_object_slots},
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_data_structures::{
    bit_set::GrowableBitSet,
    map::{FxHashMap, FxHashSet},
};

/// Scalar-replacement-of-aggregates pass for memory objects.
pub(crate) struct Sroa;

impl MirPass for Sroa {
    fn name(&self) -> &'static str {
        "sroa"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        run_function_pass_with_alias(module, analyses, |func, analyses| {
            SroaCx::default().run(func, analyses.alias())
        })
    }
}

#[derive(Debug, Default)]
struct SroaCx {
    /// Number of allocations dissolved.
    eliminated: usize,
}

/// A constant-size allocation that may be dissolved.
struct Candidate {
    object: ValueId,
    inst: InstId,
    size: u64,
}

impl SroaCx {
    fn run(&mut self, func: &mut Function, alias: &AliasAnalysis) -> bool {
        let mut candidates = Vec::new();
        for block_id in func.blocks.indices() {
            for &inst_id in &func.blocks[block_id].instructions {
                if let InstKind::Alloc { size, .. } = func.inst(inst_id).kind
                    && let Some(size) = func.value_u64(size)
                    && EvmMemoryLayout::word_slot(size).is_some()
                    && let Some(object) = func.inst_result_value(inst_id)
                {
                    candidates.push(Candidate { object, inst: inst_id, size });
                }
            }
        }
        if candidates.is_empty() {
            return false;
        }

        // Promotion adds phis but no blocks, so one block order serves every allocation.
        let order = CfgInfo::new(func).rpo().to_vec();
        let mut scan = None::<Scan>;
        let mut changed = false;
        for (index, candidate) in candidates.iter().enumerate() {
            if alias.value_escapes(func, candidate.object) {
                continue;
            }
            let current =
                scan.get_or_insert_with(|| Scan::new(func, alias, &order, &candidates, index));
            if self.try_promote(func, alias, current, index, candidate).unwrap_or(false) {
                alias.clear_cached_addresses();
                scan = None;
                self.eliminated += 1;
                changed = true;
            }
        }
        changed
    }

    /// Verifies eligibility and scalarizes every field in one transaction.
    fn try_promote(
        &self,
        func: &mut Function,
        alias: &AliasAnalysis,
        scan: &mut Scan,
        index: usize,
        candidate: &Candidate,
    ) -> Option<bool> {
        let &Candidate { object, inst: alloc, size: zero_size } = candidate;
        let scanned = &scan.objects[index];
        if scanned.rejected {
            return None;
        }

        let location = alias.bare_memory_location(func, object, LocationSize::Const(zero_size))?;
        // Effects are filed by allocation site, so the object must be named by its own.
        let (MemoryBase::Allocation(site) | MemoryBase::DynamicAllocation(site)) =
            location.address.base
        else {
            return None;
        };
        debug_assert_eq!(site, alloc);
        let effects = scan.effects.get_or_insert_default();
        if effects.touches(func, alias, location, alloc, scanned) {
            return None;
        }

        let mut fields: Vec<_> = scanned
            .accesses
            .iter()
            .filter_map(|access| match *access {
                ObjectAccess::Load { slot, .. } | ObjectAccess::Store { slot, .. } => Some(slot),
                ObjectAccess::Reset { .. } | ObjectAccess::Zero { .. } => None,
            })
            .collect();
        fields.sort_unstable();
        fields.dedup();
        if fields.is_empty() {
            return Some(false);
        }
        let zero = func.alloc_value(Value::Immediate(Immediate::I256(U256::ZERO)));
        let mut slots: Vec<_> = fields
            .iter()
            .map(|&slot| SlotAccessInfo::object(object, slot, func.blocks.len()))
            .collect();
        let mut zeros = FxHashSet::default();
        for &access in &scanned.accesses {
            match access {
                ObjectAccess::Reset { block, inst } => {
                    for slot in &mut slots {
                        slot.note_reset(block, inst);
                    }
                }
                ObjectAccess::Store { block, inst, slot, value } => {
                    let field = fields.binary_search(&slot).ok()?;
                    slots[field].note_store(block, inst, value);
                }
                ObjectAccess::Load { block, inst, slot } => {
                    let field = fields.binary_search(&slot).ok()?;
                    slots[field].note_load(block, inst);
                }
                ObjectAccess::Zero { block, inst } => {
                    zeros.insert(inst);
                    for slot in &mut slots {
                        slot.note_store(block, inst, zero);
                    }
                }
            }
        }
        if !promote_object_slots(func, &slots) {
            return Some(false);
        }
        // preserving_cast object; ptradd object, offset; memory_zero object, size
        // => SSA field values
        let addresses = FxHashSet::from_iter(scanned.addresses.iter().copied());
        for block in &mut func.blocks {
            block.instructions.retain(|inst| !addresses.contains(inst) && !zeros.contains(inst));
        }
        Some(true)
    }
}

/// What one walk of the function says about each candidate from the one that started it on,
/// and, once a candidate asks, about the memory effects of the whole function.
struct Scan {
    objects: Vec<ObjectScan>,
    effects: Option<Effects>,
}

/// The accesses to one object's address words, or the first use that rules it out.
#[derive(Default)]
struct ObjectScan {
    /// Pointer casts and constant `ptradd`s of the object, each addressing one of its words.
    addresses: Vec<InstId>,
    /// Resets, word accesses, and zero fills in layout order.
    accesses: Vec<ObjectAccess>,
    /// A derived address feeds an instruction other than a word access or full zero fill.
    rejected: bool,
}

#[derive(Clone, Copy)]
enum ObjectAccess {
    Reset { block: BlockId, inst: InstId },
    Load { block: BlockId, inst: InstId, slot: u64 },
    Store { block: BlockId, inst: InstId, slot: u64, value: ValueId },
    Zero { block: BlockId, inst: InstId },
}

/// How an instruction uses the addresses of one object.
enum Use {
    Ignored,
    Access(ObjectAccess),
    Rejected,
}

impl ObjectScan {
    /// Rules the object out. Returns whether it was still a candidate.
    fn reject(&mut self) -> bool {
        !std::mem::replace(&mut self.rejected, true)
    }
}

impl ObjectAccess {
    fn inst(self) -> InstId {
        match self {
            Self::Reset { inst, .. }
            | Self::Load { inst, .. }
            | Self::Store { inst, .. }
            | Self::Zero { inst, .. } => inst,
        }
    }
}

impl Scan {
    fn new(
        func: &Function,
        alias: &AliasAnalysis,
        order: &[BlockId],
        candidates: &[Candidate],
        first: usize,
    ) -> Self {
        // Pointer casts and constant `ptradd`s of an object address its words; map each derived
        // address to its object and byte offset. A derivation is no phi, so its operand comes
        // first in reverse postorder.
        let mut roots = FxHashMap::default();
        for (index, candidate) in candidates.iter().enumerate().skip(first) {
            if !alias.value_escapes(func, candidate.object) {
                roots.insert(candidate.object, (index, 0_u64));
            }
        }
        let mut live = roots.len();
        let mut objects = candidates.iter().map(|_| ObjectScan::default()).collect::<Vec<_>>();
        let mut address_insts = GrowableBitSet::new_empty();
        for &block in order {
            for &id in &func.blocks[block].instructions {
                let derived = match func.inst(id).kind {
                    InstKind::PtrToInt(base, 256) | InstKind::IntToPtr(base) => {
                        roots.get(&base).copied()
                    }
                    InstKind::PtrAdd(base, offset) => {
                        roots.get(&base).and_then(|&(index, base)| {
                            Some((index, base.checked_add(func.value_u64(offset)?)?))
                        })
                    }
                    _ => None,
                };
                if let Some(derived) = derived
                    && let Some(result) = func.inst_result_value(id)
                {
                    roots.insert(result, derived);
                    objects[derived.0].addresses.push(id);
                    address_insts.insert(id);
                }
            }
        }

        // Derived addresses may only feed full-object zero fills and word accesses inside the
        // object; a store must not leak one.
        let mut touched = SmallVec::<[usize; 2]>::new();
        'blocks: for (block, data) in func.blocks.iter_enumerated() {
            for &inst in &data.instructions {
                if live == 0 {
                    break 'blocks;
                }
                if address_insts.contains(inst) {
                    continue;
                }
                let kind = &func.inst(inst).kind;
                if matches!(kind, InstKind::Alloc { .. })
                    && let Some(result) = func.inst_result_value(inst)
                    && let Some(&(index, _)) = roots.get(&result)
                {
                    objects[index].accesses.push(ObjectAccess::Reset { block, inst });
                }
                touched.clear();
                kind.visit_operands(|value| {
                    if let Some(&(index, _)) = roots.get(&value)
                        && !touched.contains(&index)
                    {
                        touched.push(index);
                    }
                });
                for &index in &touched {
                    let object = &mut objects[index];
                    match Self::classify(func, &roots, index, candidates[index].size, block, inst) {
                        Use::Access(access) => object.accesses.push(access),
                        Use::Ignored => {}
                        Use::Rejected => live -= usize::from(object.reject()),
                    }
                }
            }
        }

        // Non-capturing terminators can still read the object's memory.
        for data in &func.blocks {
            if let Some(terminator) = &data.terminator
                && live != 0
            {
                terminator.visit_operands(|value| {
                    if let Some(&(index, _)) = roots.get(&value) {
                        live -= usize::from(objects[index].reject());
                    }
                });
            }
        }
        Self { objects, effects: None }
    }

    /// Classifies how `inst` uses the addresses of object `index`.
    fn classify(
        func: &Function,
        roots: &FxHashMap<ValueId, (usize, u64)>,
        index: usize,
        zero_size: u64,
        block: BlockId,
        inst: InstId,
    ) -> Use {
        let offset = |value| roots.get(&value).filter(|root| root.0 == index).map(|root| root.1);
        // The word an address inside the object accesses, or the use that is no access.
        let word = |address| {
            let Some(offset) = offset(address) else { return Err(Use::Ignored) };
            let words = zero_size / EvmMemoryLayout::WORD_SIZE;
            EvmMemoryLayout::word_slot(offset).filter(|&slot| slot < words).ok_or(Use::Rejected)
        };
        let kind = &func.inst(inst).kind;
        match *kind {
            InstKind::MemoryZero(base, size) if offset(base) == Some(0) => {
                if func.value_u64(size) == Some(zero_size) {
                    Use::Access(ObjectAccess::Zero { block, inst })
                } else {
                    Use::Rejected
                }
            }
            InstKind::Load(address, PtrLocation::Memory) => match word(address) {
                Ok(slot) => Use::Access(ObjectAccess::Load { block, inst, slot }),
                Err(other) => other,
            },
            InstKind::Store(address, value, PtrLocation::Memory) if offset(value).is_none() => {
                match word(address) {
                    Ok(slot) => Use::Access(ObjectAccess::Store { block, inst, slot, value }),
                    Err(other) => other,
                }
            }
            // Any other use of the object (a store of the pointer, a raw address) blocks
            // scalarization.
            _ if kind.any_operand(|value| offset(value).is_some()) => Use::Rejected,
            _ => Use::Ignored,
        }
    }
}

/// The memory effects of the instructions and terminators scanned so far, filed by what they
/// may touch. Instructions come in layout order, then terminators; a candidate scans only as far
/// as it needs to find something that touches its object, and later candidates resume there.
#[derive(Default)]
struct Effects {
    /// The instruction (`None` for a terminator) of each access to unknown memory.
    blockers: Vec<Option<InstId>>,
    /// Accesses based on something other than an allocation site, with the instruction making
    /// them (`None` for a terminator).
    elsewhere: Vec<(Option<InstId>, MemoryLocation)>,
    /// Accesses to each allocation site, with the instruction making them.
    touched: FxHashMap<InstId, Vec<(Option<InstId>, MemoryLocation)>>,
    /// The instructions of the candidate being checked.
    own: GrowableBitSet<InstId>,
    block: usize,
    index: usize,
    terminators: usize,
}

impl Effects {
    /// Returns whether an instruction or terminator outside the object's own accesses may read
    /// or write `location`, the memory of the object allocated at `alloc`.
    fn touches(
        &mut self,
        func: &Function,
        alias: &AliasAnalysis,
        location: MemoryLocation,
        alloc: InstId,
        object: &ObjectScan,
    ) -> bool {
        let own = || {
            object
                .addresses
                .iter()
                .copied()
                .chain(object.accesses.iter().map(|access| access.inst()))
                .chain([alloc])
        };
        for inst in own() {
            self.own.insert(inst);
        }
        let touches = self.scan(func, alias, location, alloc);
        for inst in own() {
            self.own.remove(inst);
        }
        touches
    }

    fn scan(
        &mut self,
        func: &Function,
        alias: &AliasAnalysis,
        location: MemoryLocation,
        alloc: InstId,
    ) -> bool {
        let is_other = |owner: Option<InstId>, own: &GrowableBitSet<InstId>| {
            owner.is_none_or(|inst| !own.contains(inst))
        };
        let (mut blockers, mut elsewhere, mut touched) = (0, 0, 0);
        loop {
            while blockers < self.blockers.len() {
                blockers += 1;
                if is_other(self.blockers[blockers - 1], &self.own) {
                    return true;
                }
            }
            while elsewhere < self.elsewhere.len() {
                elsewhere += 1;
                let (owner, other) = self.elsewhere[elsewhere - 1];
                if is_other(owner, &self.own) && alias.memory_alias(other, location).may_alias() {
                    return true;
                }
            }
            let accesses = self.touched.get(&alloc).map_or(&[][..], Vec::as_slice);
            while touched < accesses.len() {
                touched += 1;
                let (owner, other) = accesses[touched - 1];
                if is_other(owner, &self.own) && alias.memory_alias(other, location).may_alias() {
                    return true;
                }
            }
            if !self.advance(func, alias) {
                return false;
            }
        }
    }

    /// Records the effects of the next instruction or terminator. Returns `false` at the end.
    fn advance(&mut self, func: &Function, alias: &AliasAnalysis) -> bool {
        while self.block < func.blocks.len() {
            let block = BlockId::from_usize(self.block);
            if let Some(&inst) = func.blocks[block].instructions.get(self.index) {
                self.index += 1;
                self.record(Some(inst), &alias.instruction_mod_ref(func, inst));
                return true;
            }
            self.block += 1;
            self.index = 0;
        }
        while self.terminators < func.blocks.len() {
            let block = BlockId::from_usize(self.terminators);
            self.terminators += 1;
            if let Some(terminator) = &func.blocks[block].terminator {
                self.record(None, &alias.terminator_mod_ref(func, terminator));
                return true;
            }
        }
        false
    }

    fn record(&mut self, owner: Option<InstId>, effects: &ModRef) {
        let mut unknown = false;
        for &access in effects.reads().iter().chain(effects.writes()) {
            match access {
                Access::Any(AddressSpace::Memory) => unknown = true,
                Access::Location(Location::Memory(location)) => match location.address.base {
                    // Accesses to other allocation sites never overlap the object.
                    MemoryBase::Allocation(site) | MemoryBase::DynamicAllocation(site) => {
                        self.touched.entry(site).or_default().push((owner, location));
                    }
                    MemoryBase::Absolute | MemoryBase::InternalFrame | MemoryBase::Value(_) => {
                        self.elsewhere.push((owner, location));
                    }
                },
                Access::Any(_) | Access::Location(_) => {}
            }
        }
        if unknown {
            self.blockers.push(owner);
        }
    }
}
