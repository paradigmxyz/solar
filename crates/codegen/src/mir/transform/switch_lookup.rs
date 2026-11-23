//! Packed lookups for switches whose cases only select literals.
//!
//! A `switch v, default D, [k => Bk, ...]` whose case blocks hold no side effects and jump to
//! one join `J`, whose phis take a literal along each case edge, only picks constants. The
//! backend lowers it to a scan, a search, or a bounds-checked jump table with one stub per slot,
//! and then one block per case that pushes the literals and jumps to the join. This pass reads
//! the literals out of packed table words instead, in the switching block, and jumps to the join.
//!
//! The keys must span at most 256 values from the lowest, `low`, over `len` slots. Each phi
//! stores entry `e ^ base` per slot, where `base` is the default's literal, or the lowest key's
//! when the default branches elsewhere; a missing key stores zero. Two encodings exist:
//!
//! - Byte tables, when `len <= 32` and every entry fits a byte: one word per phi with the entry of
//!   key `low + i` in byte `32 - len + i`, read by `byte(v + 32 - len - low, table)`. The addition
//!   maps exactly the keys to bytes `32 - len..32` modulo 2^256, every other value to a lower byte,
//!   which holds zero, or past the word, where `byte` reads zero, so no range check is needed.
//! - Shifted slots: the entries of all phis side by side in a slot of `w` bits, as many slots to a
//!   word as fit, at most eight words. With `idx = v - low` and `s = idx * w`, the slot is `(T0 >>
//!   s) | (T1 >> (s - base1)) | ...`: a word's shift exceeds its bits for later slots and wraps for
//!   earlier ones, and shifts saturate, so only the slot's own word contributes. `idx < len` guards
//!   the read by multiplying it, since a large `idx` wraps `s`. Each phi then masks its field out
//!   of the slot.
//!
//! An exclusive-or with `base` restores each value, and a `trunc` gives narrow and `i1` phis
//! their type. When the default edge passes literals to the join too, out-of-range keys read
//! zero and get the default's value, and the switch becomes a jump. Otherwise the switch becomes
//! a branch to the default on a membership test: `idx < len` for contiguous keys, or a bitmap of
//! the keys present, `(bitmap >> idx) & 1`, which reads zero past the bitmap. Phis whose edges all
//! pass one value keep it, and a join that takes no phi from the switch needs only the membership
//! test, so a one-case switch to a block without phis becomes a branch. The lookup runs in the
//! switching block before that branch, on the default's path too, since it is pure.
//!
//! [`Target::switch_lookup_pays`] compares the cheaper encoding with an estimate of the switch as
//! the backend's planner lowers it, gas and bytes under the objective and optimizer runs. A switch
//! inside a loop, or in a function that a loop or recursion calls, counts as running several times
//! per call, and a gas build never pays more gas per execution there.
//!
//! The pass runs in the lowering pipeline right after `if-convert`, once folded conditions and
//! empty blocks are gone, and before inlining prices the helpers that hold such switches. It runs
//! only on semantic MIR, since lowered MIR rejects `trunc`, and only when `-Zswitch-lowering`
//! leaves the backend to choose, so a forced strategy still lowers every switch.
//!
//! Omissions: tables wider than eight words, key spans over 256 values, and case blocks that pass
//! a computed value stay switches. The pass is disabled before Constantinople, where
//! `lower-shifts` turns each variable shift into `EXP` and `DIV`, whose gas grows with the byte
//! length of the shift amount, up to 1,610 gas for a wrapped one. Tables are never read from
//! program data: MIR's `datacopy` copies a data object from its start, with no runtime source
//! offset, and a `CODECOPY` of one entry would also need a scratch-memory store and load.
//!
//! Debug locations follow `if-convert`: the code that replaces the switch takes the switch's
//! location, and each phi's value takes that phi's, or none when the phi has none. The new
//! terminator keeps the switch's metadata.

use super::{cfg_simplify::simplify_function, egraph::use_counts};
use crate::{
    backend::evm::op,
    mir::{
        BlockId, EffectKind, Function, FunctionBuilder, Immediate, InstId, InstKind,
        InstructionMetadata, MirPhase, MirType, Module, Terminator, Value, ValueId,
        analysis::{CallGraphInfo, CfgInfo},
        pass::{MirPass, run_function_pass},
        utils::{invalidate_unreachable_block, replace_terminator},
    },
    target::{Cost, Target},
};
use alloy_primitives::U256;
use solar_config::SwitchLowering;
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec, map::FxHashSet};
use solar_evmir::debug::DebugLocKind;

/// Function pass that replaces switches selecting literals with packed table lookups.
pub(crate) struct SwitchLookup;

impl MirPass for SwitchLookup {
    fn name(&self) -> &'static str {
        "switch-lookup"
    }

    fn is_enabled(&self, gcx: solar_sema::Gcx<'_>, module: &Module) -> bool {
        self.covers(module)
            && module.phase() == MirPhase::Semantic
            && gcx.sess.opts.unstable.switch_lowering == SwitchLowering::Auto
            && gcx.sess.opts.evm_version.has_bitwise_shifting()
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        // Inlining comes later, so a switch in a helper that a loop calls runs repeatedly too.
        let has_switch = module.functions.iter().any(|func| {
            func.blocks
                .iter()
                .any(|block| matches!(block.terminator, Some(Terminator::Switch { .. })))
        });
        if !has_switch {
            return false;
        }
        let mut bodies = DenseBitSet::new_empty(module.functions.len());
        for (id, func) in module.functions.iter_enumerated() {
            if !func.blocks.is_empty() {
                bodies.insert(id);
            }
        }
        let repeated = module.repeated_functions(&bodies, &CallGraphInfo::new(module));
        run_function_pass(module, analyses, |func, analyses| {
            let repeated = repeated.contains(analyses.function());
            switch_lookup_function(func, &analyses.loop_aware_target(gcx), repeated)
        })
    }

    fn preserves_debugify_sites(&self) -> bool {
        true
    }
}

/// Bits of an EVM word.
const WORD_BITS: usize = 256;
/// Bytes of an EVM word, the entries a byte table holds.
const WORD_BYTES: usize = 32;
/// The most table words a shifted lookup reads.
const MAX_TABLE_WORDS: usize = 8;

/// A switch whose cases only pass literals to one join.
struct Site {
    block: BlockId,
    value: ValueId,
    join: BlockId,
    /// The lowest key.
    low: U256,
    /// Slots from the lowest key to the highest.
    len: usize,
    /// Bit `i` is set when key `low + i` reaches the join.
    present: U256,
    /// The default block, when it stays a target because it passes no literals to the join.
    default: Option<BlockId>,
    /// The case and default blocks that only forward literals, which go with the switch.
    forwarders: Vec<BlockId>,
    fields: Vec<Field>,
    /// The sorted keys.
    keys: Vec<U256>,
    /// The case targets the backend would lower, with the keys reaching each and the literals it
    /// passes.
    blocks: Vec<(u32, Vec<U256>)>,
}

/// The value one join phi takes from the replaced edges.
struct Field {
    phi: InstId,
    ty: MirType,
    value: FieldValue,
}

enum FieldValue {
    /// Every replaced edge passes this value.
    Same(ValueId),
    /// Every replaced edge passes this literal.
    Literal(U256),
    /// Entry `i` is the literal for key `low + i` exclusive-ored with `base`, zero for a missing
    /// key; every entry fits `bits` bits.
    Table { base: U256, entries: Vec<U256>, bits: usize },
}

/// How the entries are packed.
#[derive(Clone, Copy)]
enum Encoding {
    /// One word per phi, an entry per byte.
    Bytes,
    /// Slots of `slot` bits holding every phi's entry, `per_word` to a word.
    Shifts { slot: usize, per_word: usize },
}

fn switch_lookup_function(func: &mut Function, target: &Target, repeated: bool) -> bool {
    let cfg = CfgInfo::new(func);
    let mut uses = use_counts(func);
    let mut changed = false;
    for block in func.blocks.indices() {
        if !cfg.is_reachable(block) {
            continue;
        }
        let Some(site) = find_site(func, block, &uses) else { continue };
        let in_loop = repeated || cfg.cyclic_blocks().contains(block);
        if let Some(encoding) = choose(target, &site, in_loop) {
            count_reads(func, &site, &mut uses, false);
            rewrite(func, &site, encoding);
            uses.resize(func.num_values(), 0);
            count_reads(func, &site, &mut uses, true);
            changed = true;
        }
    }
    if changed {
        simplify_function(func);
    }
    changed
}

/// The join a case block forwards to, when it only does pure work and `block` alone enters it.
fn forwarded_join(func: &Function, block: BlockId, target: BlockId) -> Option<BlockId> {
    let body = &func.blocks[target];
    if target == block
        || body.predecessors.is_empty()
        || body.predecessors.iter().any(|&pred| pred != block)
    {
        return None;
    }
    let Some(Terminator::Jump(join)) = body.terminator else { return None };
    let pure = body.instructions.iter().all(|&inst| {
        let kind = &func.inst(inst).kind;
        !matches!(kind, InstKind::Phi(_))
            && !kind.has_side_effects()
            && kind.effect_kind() == EffectKind::Pure
    });
    (pure && join != target && join != block).then_some(join)
}

fn find_site(func: &Function, block: BlockId, uses: &IndexVec<ValueId, u32>) -> Option<Site> {
    let Some(Terminator::Switch { value, default, cases }) = &func.blocks[block].terminator else {
        return None;
    };
    let (value, default) = (*value, *default);
    let Some(MirType::Int(_)) = func.value_ty(value) else { return None };
    let mut keyed = Vec::with_capacity(cases.len());
    for &(key, target) in cases {
        keyed.push((func.value_u256(key)?, target));
    }
    keyed.sort_unstable_by_key(|&(key, _)| key);
    if keyed.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return None;
    }
    let (&(low, first), &(high, _)) = (keyed.first()?, keyed.last()?);
    if high - low >= U256::from(WORD_BITS) {
        return None;
    }
    let len = (high - low).to::<usize>() + 1;
    let reaches = |target: BlockId, join: BlockId| {
        target == join || forwarded_join(func, block, target) == Some(join)
    };
    let join = [forwarded_join(func, block, first), Some(first)]
        .into_iter()
        .flatten()
        .find(|&join| join != block && keyed.iter().all(|&(_, target)| reaches(target, join)))?;
    // A literal default goes with the cases; any other default stays a branch target.
    if reaches(default, join)
        && let Some(site) = site(func, uses, block, value, join, &keyed, low, len, default, true)
    {
        return Some(site);
    }
    if default == join {
        return None;
    }
    site(func, uses, block, value, join, &keyed, low, len, default, false)
}

#[expect(clippy::too_many_arguments)]
fn site(
    func: &Function,
    uses: &IndexVec<ValueId, u32>,
    block: BlockId,
    value: ValueId,
    join: BlockId,
    keyed: &[(U256, BlockId)],
    low: U256,
    len: usize,
    default: BlockId,
    literal_default: bool,
) -> Option<Site> {
    let pred = |target: BlockId| if target == join { block } else { target };
    // In a branching site, keys that reach the default are missing.
    let present_keys = keyed
        .iter()
        .filter(|&&(_, target)| literal_default || target != default)
        .map(|&(key, target)| ((key - low).to::<usize>(), target))
        .collect::<Vec<_>>();
    if present_keys.is_empty() {
        return None;
    }
    let mut forwarders = Vec::new();
    let mut blocks = Vec::<(BlockId, u32)>::new();
    for &(_, target) in &present_keys {
        if target != join && !forwarders.contains(&target) {
            forwarders.push(target);
        }
        match blocks.iter_mut().find(|(block, _)| *block == target) {
            Some((_, reached)) => *reached += 1,
            None => blocks.push((target, 1)),
        }
    }
    if literal_default {
        if default != join && !forwarders.contains(&default) {
            forwarders.push(default);
        }
        if !blocks.iter().any(|&(block, _)| block == default) {
            blocks.push((default, 0));
        }
    }
    let forwarded = forwarders
        .iter()
        .flat_map(|&block| func.blocks[block].instructions.iter())
        .filter_map(|&inst| func.inst_result_value(inst))
        .collect::<FxHashSet<_>>();
    let mut fields = Vec::new();
    let mut block_literals = vec![Vec::new(); blocks.len()];
    for &phi in &func.blocks[join].instructions {
        let InstKind::Phi(incoming) = &func.inst(phi).kind else { continue };
        let incoming_from =
            |target| incoming.iter().find(|&&(from, _)| from == pred(target)).map(|&(_, v)| v);
        let ty = func.value_ty(func.inst_result_value(phi)?)?;
        let MirType::Int(_) = ty else { return None };
        let default_value = if literal_default { Some(incoming_from(default)?) } else { None };
        let mut values = Vec::with_capacity(present_keys.len());
        for &(index, target) in &present_keys {
            values.push((index, incoming_from(target)?));
        }
        let shared = default_value.unwrap_or(values[0].1);
        let field = if values.iter().all(|&(_, value)| value == shared) {
            FieldValue::Same(shared)
        } else {
            let base = func.value_u256(shared)?;
            let mut entries = vec![U256::ZERO; len];
            for &(index, value) in &values {
                entries[index] = func.value_u256(value)? ^ base;
            }
            let bits = entries.iter().map(|entry| entry.bit_len()).max().unwrap_or(0);
            if bits == 0 {
                FieldValue::Literal(base)
            } else {
                FieldValue::Table { base, entries, bits }
            }
        };
        if !matches!(field, FieldValue::Same(_)) {
            for (literals, &(target, _)) in block_literals.iter_mut().zip(&blocks) {
                literals.push(func.value_u256(incoming_from(target)?)?);
            }
        }
        fields.push(Field { phi, ty, value: field });
    }
    // The forwarders go with the switch, so nothing else may read what they compute: a forwarder
    // that dominates a loop header reaches every block below it.
    if !forwarded.is_empty() && used_outside(func, uses, &forwarders, &forwarded) {
        return None;
    }
    let present =
        present_keys.iter().fold(U256::ZERO, |bits, &(index, _)| bits | U256::ONE << index);
    Some(Site {
        block,
        value,
        join,
        low,
        len,
        present,
        default: (!literal_default).then_some(default),
        forwarders,
        fields,
        keys: keyed.iter().map(|&(key, _)| key).collect(),
        blocks: blocks.iter().map(|&(_, reached)| reached).zip(block_literals).collect(),
    })
}

/// Whether a block other than the `forwarders` reads a value in `forwarded`, given the reads of
/// every value in `uses`.
fn used_outside(
    func: &Function,
    uses: &IndexVec<ValueId, u32>,
    forwarders: &[BlockId],
    forwarded: &FxHashSet<ValueId>,
) -> bool {
    let mut outside = forwarded.iter().map(|&value| uses[value] as usize).sum::<usize>();
    // Forwarders end in operand-free jumps.
    for &block in forwarders {
        for &inst in &func.blocks[block].instructions {
            func.inst(inst).kind.visit_operands(|value| {
                if forwarded.contains(&value) {
                    outside -= 1;
                }
            });
        }
    }
    outside != 0
}

/// Adds the reads in the blocks that rewriting `site` changes to `uses`, or removes them.
fn count_reads(func: &Function, site: &Site, uses: &mut IndexVec<ValueId, u32>, add: bool) {
    let mut count = |value: ValueId| {
        if add {
            uses[value] += 1;
        } else {
            uses[value] -= 1;
        }
    };
    for &block in [site.block, site.join].iter().chain(&site.forwarders) {
        let body = &func.blocks[block];
        for &inst in &body.instructions {
            func.inst(inst).kind.visit_operands(&mut count);
        }
        if let Some(terminator) = &body.terminator {
            terminator.visit_operands(&mut count);
        }
    }
}

/// The cheapest encoding of the site, when it pays for the switch it replaces.
fn choose(target: &Target, site: &Site, in_loop: bool) -> Option<Encoding> {
    let slot = site.tables().map(|(_, _, bits)| bits).sum::<usize>();
    let mut candidates = Vec::with_capacity(2);
    if site.len <= WORD_BYTES && site.tables().all(|(_, _, bits)| bits <= 8) {
        candidates.push(Encoding::Bytes);
    }
    if slot <= WORD_BITS {
        let per_word = WORD_BITS.checked_div(slot).unwrap_or(site.len);
        if site.len.div_ceil(per_word) <= MAX_TABLE_WORDS {
            candidates.push(Encoding::Shifts { slot, per_word });
        }
    }
    let (encoding, cost) = candidates
        .into_iter()
        .map(|encoding| (encoding, price(target, site, encoding)))
        .min_by(|&(_, a), &(_, b)| target.cmp(a, b))?;
    let blocks = site
        .blocks
        .iter()
        .map(|(reached, literals)| {
            (*reached, literals.iter().map(|&literal| target.push(literal)).sum::<Cost>())
        })
        .collect::<Vec<_>>();
    target.switch_lookup_pays(&site.keys, &blocks, cost, in_loop).then_some(encoding)
}

impl Site {
    /// The table fields: base, entries, and bits.
    fn tables(&self) -> impl Iterator<Item = (U256, &[U256], usize)> {
        self.fields.iter().filter_map(|field| match &field.value {
            FieldValue::Table { base, entries, bits } => Some((*base, entries.as_slice(), *bits)),
            _ => None,
        })
    }

    fn has_gaps(&self) -> bool {
        self.present != U256::MAX >> (WORD_BITS - self.len)
    }

    /// The literal that addresses byte tables, `v - c` modulo 2^256 with `c = low + len - 32`,
    /// as `(true, c)` for a subtraction or `(false, -c)` for an addition, whichever is shorter.
    fn byte_offset(&self) -> (bool, U256) {
        let offset =
            self.low.wrapping_add(U256::from(self.len)).wrapping_sub(U256::from(WORD_BYTES));
        let negated = offset.wrapping_neg();
        if offset <= negated { (true, offset) } else { (false, negated) }
    }

    /// The bitmap of the keys present at their byte table indices.
    fn byte_bitmap(&self) -> U256 {
        self.present << (WORD_BYTES - self.len)
    }

    /// The words of a shifted lookup, with the bit offset of each table field in the slot.
    fn shifted_words(&self, slot: usize, per_word: usize) -> (Vec<U256>, Vec<usize>) {
        let mut offsets = Vec::new();
        let mut offset = 0;
        for (_, _, bits) in self.tables() {
            offsets.push(offset);
            offset += bits;
        }
        let words = (0..self.len.div_ceil(per_word))
            .map(|word| {
                let slots = word * per_word..self.len.min((word + 1) * per_word);
                slots.rev().fold(U256::ZERO, |packed, index| {
                    let entry = self
                        .tables()
                        .zip(&offsets)
                        .fold(U256::ZERO, |entry, ((_, entries, _), &offset)| {
                            entry | entries[index] << offset
                        });
                    packed << slot | entry
                })
            })
            .collect();
        (words, offsets)
    }
}

/// Cost of one execution of the lookup and its bytes, as [`rewrite`] emits it.
fn price(target: &Target, site: &Site, encoding: Encoding) -> Cost {
    let push = |value: U256| target.push(value);
    let opcode = |opcode| target.opcode(opcode);
    let tables = site.tables().count() as u32;
    let mut cost = Cost::ZERO;
    for field in &site.fields {
        if let FieldValue::Literal(literal) = field.value {
            cost += push(literal);
        }
    }
    // (m >> i) & 1, branched on
    let bitmap_test = |bitmap| {
        push(bitmap) + opcode(op::SHR) + push(U256::ONE) + opcode(op::AND) + target.branch()
    };
    let restore = |base: U256| {
        if base.is_zero() { Cost::ZERO } else { push(base) + opcode(op::XOR) }
    };
    match encoding {
        Encoding::Bytes => {
            let (_, offset) = site.byte_offset();
            if !offset.is_zero() {
                cost += push(offset) + opcode(op::ADD);
            }
            let mut uses = tables;
            if site.default.is_some() {
                uses += 1;
                cost += bitmap_test(site.byte_bitmap());
            }
            cost += target.dup().times(uses.saturating_sub(1));
            for (base, entries, _) in site.tables() {
                cost += push(byte_table(entries)) + opcode(op::BYTE) + restore(base);
            }
        }
        Encoding::Shifts { slot, per_word } => {
            if !site.low.is_zero() {
                cost += push(site.low) + opcode(op::SUB);
            }
            let reads = slot != 0;
            let mut uses = u32::from(reads);
            if site.default.is_some() {
                uses += 1;
                cost += if site.has_gaps() {
                    bitmap_test(site.present)
                } else {
                    push(U256::from(site.len)) + opcode(op::LT) + target.branch()
                };
            } else if reads {
                uses += 1;
                cost += push(U256::from(site.len)) + opcode(op::LT) + opcode(op::MUL);
            }
            cost += target.dup().times(uses.saturating_sub(1));
            if reads {
                cost += if slot.is_power_of_two() {
                    push(U256::from(slot.trailing_zeros())) + opcode(op::SHL)
                } else {
                    push(U256::from(slot)) + opcode(op::MUL)
                };
                let (words, offsets) = site.shifted_words(slot, per_word);
                for (index, &word) in words.iter().enumerate() {
                    cost += push(word) + opcode(op::SHR);
                    if index != 0 {
                        let base = U256::from(index * per_word * slot);
                        cost += target.dup() + push(base) + opcode(op::SUB) + opcode(op::OR);
                    }
                }
                cost += target.dup().times(tables - 1);
                for ((base, _, bits), offset) in site.tables().zip(offsets) {
                    if offset != 0 {
                        cost += push(U256::from(offset)) + opcode(op::SHR);
                    }
                    cost += push(mask(bits)) + opcode(op::AND) + restore(base);
                }
            }
        }
    }
    cost
}

/// The byte table of `entries`: entry `i` in byte `32 - len + i`.
fn byte_table(entries: &[U256]) -> U256 {
    entries.iter().fold(U256::ZERO, |table, &entry| table << 8 | entry)
}

fn mask(bits: usize) -> U256 {
    U256::MAX >> (WORD_BITS - bits)
}

fn literal(func: &mut Function, value: U256) -> ValueId {
    func.alloc_value(Value::Immediate(Immediate::I256(value)))
}

/// Appends `kind` with result type `ty` to `block`, located at `context`.
fn append(
    func: &mut Function,
    block: BlockId,
    kind: InstKind,
    ty: MirType,
    context: &InstructionMetadata,
) -> ValueId {
    let start = func.blocks[block].instructions.len();
    let mut builder = FunctionBuilder::new(func);
    builder.switch_to_block(block);
    let value = builder.emit_inst(kind, Some(ty));
    for index in start..func.blocks[block].instructions.len() {
        let inst = func.blocks[block].instructions[index];
        func.inst_mut(inst).metadata.preserve_debug_loc(context);
    }
    value
}

/// A located context that keeps the absence of a source location deliberately.
fn located(mut context: InstructionMetadata) -> InstructionMetadata {
    if context.source_spans().next().is_none() && context.debug_loc_kind() == DebugLocKind::Normal {
        context.drop_debug_loc(DebugLocKind::Dropped);
    }
    context
}

fn rewrite(func: &mut Function, site: &Site, encoding: Encoding) {
    let block = site.block;
    let context = located(func.blocks[block].terminator_metadata.debug_context());
    let int =
        |func: &mut Function, kind: InstKind, ty: MirType| append(func, block, kind, ty, &context);
    // v = zext v to i256, for a narrow switch value
    let value = match func.value_ty(site.value) {
        Some(MirType::I256) => site.value,
        _ => int(func, InstKind::Zext(site.value), MirType::I256),
    };
    // The slot of each table field, before its mask and base; and the branch condition.
    let mut reads = Vec::new();
    let mut member = None;
    match encoding {
        Encoding::Bytes => {
            // i = v - (low + len - 32), or v + (32 - len - low)
            let (sub, offset) = site.byte_offset();
            let index = match (offset.is_zero(), sub) {
                (true, _) => value,
                (false, true) => {
                    let offset = literal(func, offset);
                    int(func, InstKind::Sub(value, offset), MirType::I256)
                }
                (false, false) => {
                    let offset = literal(func, offset);
                    int(func, InstKind::Add(value, offset), MirType::I256)
                }
            };
            if site.default.is_some() {
                // member = ne (and (shr i, bitmap << (32 - len)), 1), 0
                let bitmap = literal(func, site.byte_bitmap());
                member = Some(bitmap_test(func, &int, index, bitmap));
            }
            let tables =
                site.tables().map(|(_, entries, _)| byte_table(entries)).collect::<Vec<_>>();
            for table in tables {
                // entry = byte i, table
                let table = literal(func, table);
                reads.push(int(func, InstKind::Byte(index, table), MirType::I256));
            }
        }
        Encoding::Shifts { slot, per_word } => {
            // idx = v - low
            let index = if site.low.is_zero() {
                value
            } else {
                let low = literal(func, site.low);
                int(func, InstKind::Sub(value, low), MirType::I256)
            };
            let len = literal(func, U256::from(site.len));
            let mut in_range = None;
            match (site.default.is_some(), site.has_gaps()) {
                (true, true) => {
                    // member = ne (and (shr idx, bitmap), 1), 0
                    let bitmap = literal(func, site.present);
                    member = Some(bitmap_test(func, &int, index, bitmap));
                }
                // member = lt idx, len
                (true, false) => member = Some(int(func, InstKind::Lt(index, len), MirType::I1)),
                // in_range = lt idx, len
                (false, _) if slot != 0 => {
                    in_range = Some(int(func, InstKind::Lt(index, len), MirType::I1));
                }
                (false, _) => {}
            }
            if slot != 0 {
                // s = shl idx, log2(slot), or mul idx, slot
                let shift = if slot.is_power_of_two() {
                    let amount = literal(func, U256::from(slot.trailing_zeros()));
                    int(func, InstKind::Shl(amount, index), MirType::I256)
                } else {
                    let amount = literal(func, U256::from(slot));
                    int(func, InstKind::Mul(index, amount), MirType::I256)
                };
                let (words, offsets) = site.shifted_words(slot, per_word);
                // packed = shr s, T0
                // packed = or packed, shr (sub s, base_j), Tj   for each further word j
                let mut packed = None;
                for (index, word) in words.into_iter().enumerate() {
                    let amount = if index == 0 {
                        shift
                    } else {
                        let base = literal(func, U256::from(index * per_word * slot));
                        int(func, InstKind::Sub(shift, base), MirType::I256)
                    };
                    let word = literal(func, word);
                    let read = int(func, InstKind::Shr(amount, word), MirType::I256);
                    packed = Some(match packed {
                        Some(packed) => int(func, InstKind::Or(packed, read), MirType::I256),
                        None => read,
                    });
                }
                let mut packed = packed.expect("a slot has a word");
                if let Some(in_range) = in_range {
                    // packed = mul packed, zext i1 in_range to i256
                    let guard = int(func, InstKind::Zext(in_range), MirType::I256);
                    packed = int(func, InstKind::Mul(packed, guard), MirType::I256);
                }
                let masks = site.tables().map(|(_, _, bits)| mask(bits)).collect::<Vec<_>>();
                for (offset, mask) in offsets.into_iter().zip(masks) {
                    // entry = and (shr packed, offset), mask
                    let shifted = if offset == 0 {
                        packed
                    } else {
                        let offset = literal(func, U256::from(offset));
                        int(func, InstKind::Shr(offset, packed), MirType::I256)
                    };
                    let mask = literal(func, mask);
                    reads.push(int(func, InstKind::And(shifted, mask), MirType::I256));
                }
            }
        }
    }
    let mut reads = reads.into_iter();
    for field in &site.fields {
        let (phi, ty) = (field.phi, field.ty);
        let value = match field.value {
            FieldValue::Same(value) => value,
            FieldValue::Literal(literal) => {
                func.alloc_value(Value::Immediate(Immediate::for_type(Some(ty), literal)))
            }
            FieldValue::Table { base, .. } => {
                // value = trunc (xor entry, base) to ty
                let read = reads.next().expect("one read per table field");
                let context = located(func.inst(phi).metadata.debug_context());
                let restored = if base.is_zero() {
                    read
                } else {
                    let base = literal(func, base);
                    append(func, block, InstKind::Xor(read, base), MirType::I256, &context)
                };
                match ty {
                    MirType::Int(bits) if bits.get() < 256 => {
                        append(func, block, InstKind::Trunc(restored, bits.get()), ty, &context)
                    }
                    _ => restored,
                }
            }
        };
        // join: v = phi [kept edges], [block: value]
        let InstKind::Phi(incoming) = &mut func.inst_mut(phi).kind else { unreachable!() };
        incoming.retain(|&(from, _)| from != block && !site.forwarders.contains(&from));
        incoming.push((block, value));
    }
    // switch v, ... -> jump join, or branch member, join, default
    let terminator = match site.default {
        Some(default) => Terminator::Branch {
            condition: member.expect("a branching site tests membership"),
            then_block: site.join,
            else_block: default,
        },
        None => Terminator::Jump(site.join),
    };
    replace_terminator(func, block, terminator);
    for &forwarder in &site.forwarders {
        let _ = invalidate_unreachable_block(func, forwarder);
    }
}

/// `ne (and (shr index, bitmap), 1), 0`: whether bit `index` of `bitmap` is set, zero past it.
fn bitmap_test(
    func: &mut Function,
    int: &impl Fn(&mut Function, InstKind, MirType) -> ValueId,
    index: ValueId,
    bitmap: ValueId,
) -> ValueId {
    let shifted = int(func, InstKind::Shr(index, bitmap), MirType::I256);
    let one = literal(func, U256::ONE);
    let bit = int(func, InstKind::And(shifted, one), MirType::I256);
    let zero = literal(func, U256::ZERO);
    int(func, InstKind::Ne(bit, zero), MirType::I1)
}
