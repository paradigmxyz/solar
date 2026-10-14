//! Non-executable variable recovery records.
//!
//! A source read associates a declaration with an SSA operand at its original source point.
//! After lowering, the record is anchored to an instruction and its observed SSA operands. It is
//! not visited by liveness, DCE, or the verifier's executable use walker. Equivalence replacements
//! salvage bindings, aggregate lowering splits their carriers, and inlining remaps both values
//! and instruction anchors while retaining a distinct `inlinedAt` chain.
//!
//! NOTE: These are observation records, not location ranges. They describe variables used at
//! surviving source operations, rather than pretending a binding stays recoverable until its
//! next assignment. An unavailable carrier never extends the lifetime of executable values.

use super::{
    ArgIdx, Function, InstId, InstKind, MirType, Value, ValueId,
    analysis::{AddressSpace, AliasAnalysis},
};
use crate::source_info::DebugInlineFrame;
use smallvec::{SmallVec, smallvec};
use solar_data_structures::{index::IndexVec, map::FxHashMap};
use solar_interface::Span;
use solar_sema::hir::VariableId;

#[derive(Clone, Debug, Default)]
pub(crate) struct DebugValues {
    pub(crate) enabled: bool,
    pub(crate) reads: Vec<(VariableId, ValueId, Span, usize, usize)>,
    anchors: FxHashMap<InstId, DebugValueRecord>,
    pub(crate) writes: Vec<(VariableId, usize, usize)>,
    /// Unique source-point records for instructions synthesized by semantic lowering.
    origins: FxHashMap<(Span, SmallVec<[DebugInlineFrame; 1]>), Option<DebugValueRecord>>,
    next_instance: u32,
    frames: FxHashMap<InstId, SmallVec<[DebugInlineFrame; 1]>>,
    snapshots: Vec<BindingSnapshot>,
    after: FxHashMap<InstId, DebugValueRecord>,
    after_origins: FxHashMap<(Span, SmallVec<[DebugInlineFrame; 1]>), DebugValueRecord>,
}

#[derive(Clone, Debug)]
struct DebugValueRecord {
    span: Span,
    values: Vec<DebugBinding>,
}

#[derive(Clone, Debug)]
struct BindingSnapshot {
    span: Span,
    start: usize,
    end: usize,
    sequence: usize,
    bindings: Vec<(VariableId, ValueId)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DebugBinding {
    pub(crate) variable: VariableId,
    pub(crate) values: SmallVec<[ValueId; 2]>,
    pub(crate) inline: SmallVec<[DebugInlineFrame; 1]>,
    pub(crate) calldata_bias: u8,
    pub(crate) abi_head: Option<u64>,
}

impl DebugValues {
    /// Follows the same equivalence map as executable operands, without keeping a use alive.
    pub(crate) fn replace_uses(
        &mut self,
        replacements: &FxHashMap<ValueId, ValueId>,
        canonical: bool,
    ) {
        self.rewrite_values(|value| {
            *value = if canonical {
                super::utils::resolve_replacement(*value, replacements)
            } else {
                replacements.get(value).copied().unwrap_or(*value)
            };
        });
    }

    pub(crate) fn rewrite_values(&mut self, mut rewrite: impl FnMut(&mut ValueId)) {
        for record in self
            .anchors
            .values_mut()
            .chain(self.after.values_mut())
            .chain(self.after_origins.values_mut())
        {
            for binding in &mut record.values {
                binding.values.iter_mut().for_each(&mut rewrite);
            }
        }
        for record in self.origins.values_mut().flatten() {
            for binding in &mut record.values {
                binding.values.iter_mut().for_each(&mut rewrite);
            }
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.anchors.is_empty() && self.after.is_empty()
    }

    pub(crate) fn anchor(func: &mut Function) {
        let reads = std::mem::take(&mut func.debug_values.reads);
        if reads.is_empty() && func.debug_values.snapshots.is_empty() {
            return;
        }
        let mut by_value = FxHashMap::<ValueId, Vec<(VariableId, Span, usize, usize)>>::default();
        for (variable, value, span, position, sequence) in reads {
            by_value.entry(value).or_default().push((variable, span, position, sequence));
        }
        let mut anchors = FxHashMap::default();
        let aa = AliasAnalysis::new(func);
        let readonly = func.selector.is_some()
            && !func.attributes.inline_assembly
            && func.instructions().all(|inst| {
                !aa.instruction_mod_ref(func, inst)
                    .writes()
                    .iter()
                    .any(|access| access.address_space() == AddressSpace::Memory)
            });
        let mut head = Some(4u64);
        let heads = func
            .abi_params
            .as_ref()
            .map(|layout| {
                layout
                    .types
                    .iter()
                    .map(|ty| {
                        let current = head;
                        head = head.and_then(|head| head.checked_add(ty.checked_head_size()?));
                        current
                    })
                    .collect::<IndexVec<ArgIdx, _>>()
            })
            .unwrap_or_default();
        for inst in func.instructions() {
            let instruction = func.inst(inst);
            let Some(span) = instruction.metadata.source_span() else { continue };
            let operands = instruction.operands();
            let mut values = Vec::new();
            for value in operands {
                for &(variable, read_span, position, sequence) in
                    by_value.get(&value).into_iter().flatten()
                {
                    let reassigned =
                        func.debug_values.writes.iter().any(|&(written, at, written_sequence)| {
                            written == variable
                                && written_sequence > sequence
                                && at >= position
                                && at <= inst.index()
                        });
                    if !reassigned
                        && span.contains(read_span)
                        && !values.iter().any(|binding: &DebugBinding| binding.variable == variable)
                    {
                        values.push(DebugBinding {
                            variable,
                            values: smallvec![value],
                            inline: SmallVec::new(),
                            calldata_bias: 0,
                            abi_head: None,
                        });
                    }
                }
            }
            // Source bindings are independent of executable uses. An intervening assignment
            // kills the old binding even when both values still survive on the stack.
            for snapshot in func.debug_values.snapshots.iter().rev() {
                if inst.index() < snapshot.start
                    || inst.index() >= snapshot.end
                    || !snapshot.span.contains(span)
                {
                    continue;
                }
                for &(variable, value) in &snapshot.bindings {
                    if values.iter().any(|binding| binding.variable == variable) {
                        continue;
                    }
                    let reassigned =
                        func.debug_values.writes.iter().any(|&(written, at, written_sequence)| {
                            written == variable
                                && written_sequence >= snapshot.sequence
                                && at >= snapshot.start
                                && at <= inst.index()
                        });
                    if !reassigned {
                        values.push(DebugBinding {
                            variable,
                            values: smallvec![value],
                            inline: SmallVec::new(),
                            calldata_bias: 0,
                            abi_head: None,
                        });
                    }
                }
            }
            if !values.is_empty() {
                if readonly {
                    for binding in &mut values {
                        if let Value::Arg(index) =
                            func.value(word_identity(func, binding.values[0]))
                            && matches!(func.arg_ty(*index), MirType::MemPtr | MirType::Slice(_))
                            && func
                                .abi_params
                                .as_ref()
                                .and_then(|layout| layout.types.get(index.index()))
                                .is_some_and(|ty| !ty.is_scalar_word())
                        {
                            binding.abi_head = heads.get(*index).copied().flatten();
                        }
                    }
                }
                values.sort_by_key(|binding| binding.variable);
                anchors.insert(inst, DebugValueRecord { span, values });
            }
        }
        for record in anchors.values() {
            func.debug_values
                .origins
                .entry((record.span, SmallVec::new()))
                .and_modify(|previous| {
                    if let Some(previous) = previous {
                        previous.values.retain(|binding| record.values.contains(binding));
                    }
                })
                .or_insert_with(|| Some(record.clone()));
        }
        func.debug_values.anchors = anchors;
        func.debug_values.snapshots.clear();
    }

    pub(crate) fn snapshot_bindings(
        &mut self,
        span: Span,
        start: usize,
        end: usize,
        sequence: usize,
        bindings: Vec<(VariableId, ValueId)>,
    ) {
        if start < end && !bindings.is_empty() {
            self.snapshots.push(BindingSnapshot { span, start, end, sequence, bindings });
        }
    }

    pub(crate) fn assign_after(
        &mut self,
        inst: InstId,
        span: Span,
        bindings: Vec<(VariableId, ValueId)>,
    ) {
        let record = DebugValueRecord {
            span,
            values: bindings
                .into_iter()
                .map(|(variable, value)| DebugBinding {
                    variable,
                    values: smallvec![value],
                    inline: SmallVec::new(),
                    calldata_bias: 0,
                    abi_head: None,
                })
                .collect(),
        };
        self.after.insert(inst, record.clone());
        self.after_origins
            .entry((span, SmallVec::new()))
            .and_modify(|old| old.values.retain(|binding| record.values.contains(binding)))
            .or_insert(record);
    }

    pub(crate) fn at_after<'a>(&'a self, func: &Function, inst: InstId) -> &'a [DebugBinding] {
        let metadata = &func.inst(inst).metadata;
        let Some(frames) = metadata.inlined_at() else { return &[] };
        let Some(span) = metadata.source_span() else { return &[] };
        if metadata.source_spans().count() != 1 {
            return &[];
        }
        self.after
            .get(&inst)
            .filter(|record| record.span == span)
            .or_else(|| self.after_origins.get(&(span, SmallVec::from_slice(frames))))
            .map_or(&[], |record| &record.values)
    }

    pub(crate) fn at<'a>(&'a self, func: &Function, inst: InstId) -> &'a [DebugBinding] {
        let instruction = func.inst(inst);
        let Some(frames) = instruction.metadata.inlined_at() else { return &[] };
        let record = self.anchors.get(&inst).or_else(|| {
            self.origins
                .get(&(instruction.metadata.source_span()?, SmallVec::from_slice(frames)))
                .and_then(Option::as_ref)
        });
        let Some(record) = record else { return &[] };
        if instruction.metadata.source_span() != Some(record.span)
            || instruction.metadata.source_spans().count() != 1
        {
            return &[];
        }
        &record.values
    }

    /// Splits a logical slice into its independently recoverable pointer and length.
    pub(crate) fn split_value(&mut self, logical: ValueId, words: &[ValueId]) {
        for record in self
            .anchors
            .values_mut()
            .chain(self.origins.values_mut().flatten())
            .chain(self.after.values_mut())
            .chain(self.after_origins.values_mut())
        {
            for binding in &mut record.values {
                if binding.values.as_slice() == [logical] {
                    binding.values = words.into();
                }
            }
        }
    }

    pub(crate) fn allocate_instance(&mut self) -> u32 {
        let instance = self.next_instance;
        self.next_instance += 1;
        instance
    }

    pub(crate) fn compact_calldata(&mut self, logical: ValueId) {
        for record in self
            .anchors
            .values_mut()
            .chain(self.origins.values_mut().flatten())
            .chain(self.after.values_mut())
            .chain(self.after_origins.values_mut())
        {
            for binding in &mut record.values {
                if binding.values.as_slice() == [logical] {
                    binding.calldata_bias = 4;
                }
            }
        }
    }

    pub(crate) fn frames_at(&self, inst: InstId) -> &[DebugInlineFrame] {
        self.frames.get(&inst).map_or(&[], SmallVec::as_slice)
    }

    pub(crate) fn clone_binding(
        &mut self,
        callee: &Function,
        source: InstId,
        destination: InstId,
        frame: DebugInlineFrame,
        parents: &[DebugInlineFrame],
        mut map: impl FnMut(ValueId) -> Option<ValueId>,
    ) {
        let old = &callee.debug_values;
        if !old.enabled {
            return;
        }
        let mut chain = SmallVec::from_slice(parents);
        chain.push(frame);
        chain.extend_from_slice(old.frames_at(source));
        self.frames.insert(destination, chain);
        let Some(span) = callee.inst(source).metadata.source_span() else { return };
        for (after, bindings) in
            [(false, old.at(callee, source)), (true, old.at_after(callee, source))]
        {
            let mut record = DebugValueRecord { span, values: bindings.to_vec() };
            record.values.retain_mut(|binding| {
                let mapped = binding
                    .values
                    .iter()
                    .copied()
                    .map(&mut map)
                    .collect::<Option<SmallVec<[_; 2]>>>();
                binding.values = match mapped {
                    Some(values) => values,
                    None if binding.abi_head.is_some() => SmallVec::new(),
                    None => return false,
                };
                let mut chain = SmallVec::from_slice(parents);
                chain.push(frame);
                chain.extend_from_slice(&binding.inline);
                binding.inline = chain;
                true
            });
            if after {
                if !record.values.is_empty() {
                    self.after_origins
                        .insert((span, record.values[0].inline.clone()), record.clone());
                    self.after.insert(destination, record);
                }
                continue;
            }
            if !record.values.is_empty() {
                self.origins
                    .entry((record.span, record.values[0].inline.clone()))
                    .and_modify(|previous| {
                        if let Some(previous) = previous {
                            previous.values.retain(|binding| record.values.contains(binding));
                        }
                    })
                    .or_insert_with(|| Some(record.clone()));
                self.anchors.insert(destination, record);
            }
        }
    }
}

/// Resolves casts that leave all 256 physical stack bits unchanged.
pub(crate) fn word_identity(func: &Function, mut value: ValueId) -> ValueId {
    for _ in 0..16 {
        let Value::Inst(inst) = func.value(value) else { break };
        match func.inst(*inst).kind {
            InstKind::Zext(input) | InstKind::IntToPtr(input) | InstKind::PtrToInt(input, 256) => {
                value = input;
            }
            _ => break,
        }
    }
    value
}

pub(crate) fn same_word(func: &Function, lhs: ValueId, rhs: ValueId) -> bool {
    let lhs = word_identity(func, lhs);
    let rhs = word_identity(func, rhs);
    lhs == rhs
        || matches!((func.value(lhs), func.value(rhs)), (Value::Arg(a), Value::Arg(b)) if a == b)
}
