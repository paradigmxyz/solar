//! Whole-program transforms over finalized, layout-linear EVM IR.

use super::{AsmInst, AsmInstKind, Label, Program, PushValueId};
use crate::backend::evm::{
    assembler::{IdCounter, LocalInterner},
    ir::PassOptions,
    opcode as op,
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_config::OptimizationMode;
use solar_data_structures::{
    bit_set::{DenseBitSet, GrowableBitSet},
    map::FxHashMap,
};

const MIN_CLOSED_RUN: usize = 4;
type ClosedRunSite = (usize, usize, u16);

pub(in crate::backend::evm) fn run(
    program: &mut Program,
    options: PassOptions,
    push_values: &LocalInterner<U256, PushValueId>,
    next_label: &mut IdCounter<Label>,
) {
    let mut cx = Context { options, push_values, next_label };
    cx.invert_branches_over_empty_reverts(&mut program.instructions);
    let has_labels =
        program.instructions.iter().any(|inst| matches!(inst.kind(), AsmInstKind::Label(_)));
    if has_labels {
        dedup_terminal_spans(&mut program.instructions);
        remove_unreferenced_labels(&mut program.instructions);
        dedup_terminal_spans(&mut program.instructions);
        remove_unreferenced_labels(&mut program.instructions);
        loop {
            let merged = cx.merge_terminal_suffixes(&mut program.instructions);
            let threaded = thread_jump_thunks(&mut program.instructions);
            let swept = remove_unreachable_code(&mut program.instructions);
            if !merged && !threaded && !swept {
                break;
            }
            dedup_terminal_spans(&mut program.instructions);
            remove_unreferenced_labels(&mut program.instructions);
        }
    } else {
        remove_unreachable_code(&mut program.instructions);
    }
    cx.outline_closed_computations(program);
    if program
        .instructions
        .iter()
        .filter(|inst| matches!(inst.kind(), AsmInstKind::Push(_)))
        .take(2)
        .count()
        >= 2
    {
        cx.outline_repeated_pushes(program);
    }
    cx.hoist_hot_terminal_spans(&mut program.instructions);
}

struct Context<'a> {
    options: PassOptions,
    push_values: &'a LocalInterner<U256, PushValueId>,
    next_label: &'a mut IdCounter<Label>,
}

impl Context<'_> {
    fn new_label(&mut self) -> Label {
        self.next_label.next()
    }

    fn push_value(&self, index: PushValueId) -> U256 {
        *self.push_values.get(index)
    }

    fn push_len(&self, value: U256) -> usize {
        let width = value.byte_len();
        if width == 0 && !self.options.evm_version.has_push0() { 2 } else { width + 1 }
    }

    fn invert_branches_over_empty_reverts(&mut self, instructions: &mut Vec<AsmInst>) {
        if self.options.optimization == OptimizationMode::None {
            return;
        }
        let is_zero_push = |inst: AsmInst| matches!(inst.kind(), AsmInstKind::PushInline(0));
        let mut shared = None;
        let mut alias = FxHashMap::default();
        let mut out = Vec::<AsmInst>::with_capacity(instructions.len());
        let mut i = 0;
        while i < instructions.len() {
            if let AsmInstKind::PushLabel(cont) = instructions[i].kind()
                && i + 1 < instructions.len()
                && matches!(instructions[i + 1].kind(), AsmInstKind::Op(op::JUMPI))
            {
                let mut j = i + 2;
                let stub_label = if j < instructions.len()
                    && let AsmInstKind::Label(stub) = instructions[j].kind()
                {
                    j += 1;
                    Some(stub)
                } else {
                    None
                };
                if j + 3 < instructions.len()
                    && is_zero_push(instructions[j])
                    && matches!(instructions[j + 1].kind(), AsmInstKind::Op(op::DUP1))
                    && matches!(instructions[j + 2].kind(), AsmInstKind::Op(op::REVERT))
                    && matches!(instructions[j + 3].kind(), AsmInstKind::Label(label) if label == cont)
                {
                    let target = if let Some(target) = shared {
                        target
                    } else {
                        let target = self.new_label();
                        shared = Some(target);
                        target
                    };
                    if let Some(stub) = stub_label {
                        alias.insert(stub, target);
                    }
                    match out.last().map(|inst| inst.kind()) {
                        Some(AsmInstKind::Op(op::ISZERO)) => {
                            out.pop();
                        }
                        Some(AsmInstKind::Op(op::EQ)) => {
                            *out.last_mut().unwrap() = AsmInst::op(op::SUB);
                        }
                        _ => out.push(AsmInst::op(op::ISZERO)),
                    }
                    out.push(AsmInst::push_label(target));
                    out.push(AsmInst::op(op::JUMPI));
                    i = j + 3;
                    continue;
                }
            }
            out.push(instructions[i]);
            i += 1;
        }
        let Some(target) = shared else { return };
        out.push(AsmInst::label(target));
        out.push(AsmInst::push_inline(0).unwrap());
        out.push(AsmInst::op(op::DUP1));
        out.push(AsmInst::op(op::REVERT));
        for inst in &mut out {
            if let AsmInstKind::PushLabel(label) = inst.kind()
                && let Some(&representative) = alias.get(&label)
            {
                *inst = AsmInst::push_label(representative);
            }
        }
        *instructions = out;
    }

    fn merge_terminal_suffixes(&mut self, instructions: &mut Vec<AsmInst>) -> bool {
        if self.options.optimization == OptimizationMode::None {
            return false;
        }
        let mut spans = SmallVec::<[(usize, usize); 16]>::new();
        let mut i = 0;
        while i < instructions.len() {
            if matches!(instructions[i].kind(), AsmInstKind::Label(_)) {
                let mut j = i + 1;
                while j < instructions.len() {
                    if matches!(instructions[j].kind(), AsmInstKind::Label(_)) {
                        break;
                    }
                    if is_terminal(instructions[j]) {
                        spans.push((i + 1, j));
                        break;
                    }
                    j += 1;
                }
            }
            i += 1;
        }

        let mut representatives = SmallVec::<[(usize, usize); 16]>::new();
        let mut merges = SmallVec::<[(usize, usize, usize, usize); 8]>::new();
        for &(start, end) in &spans {
            let body = &instructions[start..=end];
            let mut best = None;
            for (representative, &(rep_start, rep_end)) in representatives.iter().enumerate() {
                let rep_body = &instructions[rep_start..=rep_end];
                let max = body.len().min(rep_body.len());
                let mut common = 0;
                while common < max
                    && body[body.len() - 1 - common] == rep_body[rep_body.len() - 1 - common]
                {
                    common += 1;
                }
                if common > best.map_or(0, |(_, len)| len) {
                    best = Some((representative, common));
                }
            }
            if let Some((representative, common)) = best
                && lower_bound(&body[body.len() - common..]) > 5
            {
                merges.push((start, end, representative, common));
            } else {
                representatives.push((start, end));
            }
        }
        if merges.is_empty() {
            return false;
        }

        let mut cut_labels = FxHashMap::default();
        for &(_, _, representative, common) in &merges {
            cut_labels.entry((representative, common)).or_insert_with(|| self.new_label());
        }

        enum Edit {
            Cut { end: usize, target: Label },
            Mid { label: Label },
        }
        let mut edits = Vec::new();
        for &(_, end, representative, common) in &merges {
            edits.push((
                end + 1 - common,
                Edit::Cut { end, target: cut_labels[&(representative, common)] },
            ));
        }
        for (&(representative, common), &label) in &cut_labels {
            let (_, rep_end) = representatives[representative];
            edits.push((rep_end + 1 - common, Edit::Mid { label }));
        }
        edits.sort_unstable_by_key(|(at, _)| *at);
        for (at, edit) in edits.into_iter().rev() {
            match edit {
                Edit::Cut { end, target } => {
                    instructions
                        .splice(at..=end, [AsmInst::push_label(target), AsmInst::op(op::JUMP)]);
                }
                Edit::Mid { label } => instructions.insert(at, AsmInst::label(label)),
            }
        }
        true
    }

    fn whitelisted_effect(inst: AsmInst) -> Option<(u16, u16, u16)> {
        Some(match inst.kind() {
            AsmInstKind::PushInline(_)
            | AsmInstKind::Push(_)
            | AsmInstKind::PushLabel(_)
            | AsmInstKind::PushDeferred(_)
            | AsmInstKind::PushImmutable(_) => (0, 0, 1),
            AsmInstKind::Op(opcode) => match opcode {
                op::CALLDATASIZE | op::PUSH0 | op::RETURNDATASIZE | op::MSIZE | op::CALLVALUE => {
                    (0, 0, 1)
                }
                op::ISZERO | op::NOT | op::CALLDATALOAD | op::MLOAD => (1, 1, 1),
                op::ADD
                | op::SUB
                | op::MUL
                | op::AND
                | op::OR
                | op::XOR
                | op::SHL
                | op::SHR
                | op::LT
                | op::GT
                | op::SLT
                | op::SGT
                | op::EQ
                | op::DIV => (2, 2, 1),
                op::MSTORE => (2, 2, 0),
                op::POP => (1, 1, 0),
                dup if (op::DUP1..=op::DUP16).contains(&dup) => {
                    let n = u16::from(dup - op::DUP1) + 1;
                    (n, 0, 1)
                }
                swap if (op::SWAP1..=op::SWAP16).contains(&swap) => {
                    let n = u16::from(swap - op::SWAP1) + 1;
                    (n + 1, 0, 0)
                }
                _ => return None,
            },
            _ => return None,
        })
    }

    fn outline_closed_computations(&mut self, program: &mut Program) {
        if self.options.optimization == OptimizationMode::None {
            return;
        }
        let instructions = &program.instructions;
        let mut candidates: FxHashMap<&[AsmInst], SmallVec<[ClosedRunSite; 2]>> =
            FxHashMap::default();
        let mut i = 0;
        while i < instructions.len() {
            if matches!(instructions[i].kind(), AsmInstKind::Label(_)) {
                i += 1;
                continue;
            }
            let mut height = 0i32;
            let mut j = i;
            while j < instructions.len() {
                if matches!(instructions[j].kind(), AsmInstKind::Label(_)) {
                    break;
                }
                let Some((reads, pops, pushes)) = Self::whitelisted_effect(instructions[j]) else {
                    break;
                };
                if height < i32::from(reads) {
                    break;
                }
                height = height - i32::from(pops) + i32::from(pushes);
                j += 1;
                let len = j - i;
                if len >= MIN_CLOSED_RUN && matches!(height, 0 | 1) {
                    candidates.entry(&instructions[i..j]).or_default().push((
                        i,
                        len,
                        height as u16,
                    ));
                }
            }
            i += 1;
        }

        let mut groups: Vec<_> = candidates.iter().filter(|(_, sites)| sites.len() >= 2).collect();
        groups.sort_by_key(|(key, _)| std::cmp::Reverse(key.len()));
        let mut claimed = DenseBitSet::new_empty(instructions.len());
        let mut chosen = Vec::new();
        for (_, sites) in groups {
            let (run_len, height) = (sites[0].1, sites[0].2);
            let free: Vec<_> = sites
                .iter()
                .filter(|&&(start, len, _)| {
                    (start..start + len).all(|index| !claimed.contains(index))
                })
                .map(|&(start, _, _)| start)
                .collect();
            if free.len() < 2 {
                continue;
            }
            let run_size = lower_bound(&instructions[free[0]..free[0] + run_len]);
            let stub_size = 1 + run_size + usize::from(height) + 1;
            let site_size = if free.len() >= 4 { 7 } else { 8 };
            if free.len() * run_size < free.len() * site_size + stub_size + 2 {
                continue;
            }
            for &start in &free {
                claimed.insert_range(start..start + run_len);
            }
            chosen.push((free, run_len, height));
        }
        if chosen.is_empty() {
            return;
        }

        let stub_labels: Vec<_> = chosen.iter().map(|_| self.new_label()).collect();
        let mut site_to_group = FxHashMap::default();
        for (group, (sites, _, _)) in chosen.iter().enumerate() {
            for &site in sites {
                site_to_group.insert(site, group);
            }
        }
        let mut out = Vec::with_capacity(program.instructions.len());
        let mut index = 0;
        while index < program.instructions.len() {
            if let Some(&group) = site_to_group.get(&index) {
                let ret = self.new_label();
                out.push(AsmInst::push_label(ret));
                out.push(AsmInst::push_label(stub_labels[group]));
                out.push(AsmInst::op(op::JUMP));
                out.push(AsmInst::label(ret));
                index += chosen[group].1;
            } else {
                out.push(program.instructions[index]);
                index += 1;
            }
        }
        for (group, (sites, run_len, height)) in chosen.iter().enumerate() {
            let body = program.instructions[sites[0]..sites[0] + run_len].to_vec();
            out.push(AsmInst::label(stub_labels[group]));
            out.extend_from_slice(&body);
            if *height == 1 {
                out.push(AsmInst::op(op::SWAP1));
            }
            out.push(AsmInst::op(op::JUMP));
        }
        program.instructions = out;
    }

    fn outline_repeated_pushes(&mut self, program: &mut Program) {
        if self.options.optimization == OptimizationMode::None {
            return;
        }
        let mut counts = FxHashMap::default();
        for inst in &program.instructions {
            if let AsmInstKind::Push(index) = inst.kind() {
                *counts.entry(index).or_insert(0u32) += 1;
            }
        }
        const SITE_BYTES: usize = 8;
        const MIN_SAVING: usize = 8;
        let mut stubs = FxHashMap::default();
        let mut order = Vec::new();
        for (&index, &count) in &counts {
            let push_len = self.push_len(self.push_value(index));
            let count = count as usize;
            let inline = count * push_len;
            let outlined = count * SITE_BYTES + push_len + 3;
            if count >= 2 && inline >= outlined + MIN_SAVING {
                stubs.insert(index, self.new_label());
                order.push(index);
            }
        }
        if stubs.is_empty() {
            return;
        }
        order.sort_unstable();
        let mut rewritten = Vec::with_capacity(program.instructions.len());
        for &inst in &program.instructions {
            if let AsmInstKind::Push(index) = inst.kind()
                && let Some(&stub) = stubs.get(&index)
            {
                let ret = self.new_label();
                rewritten.push(AsmInst::push_label(ret));
                rewritten.push(AsmInst::push_label(stub));
                rewritten.push(AsmInst::op(op::JUMP));
                rewritten.push(AsmInst::label(ret));
            } else {
                rewritten.push(inst);
            }
        }
        for index in order {
            rewritten.push(AsmInst::label(stubs[&index]));
            rewritten.push(AsmInst::push(index));
            rewritten.push(AsmInst::op(op::SWAP1));
            rewritten.push(AsmInst::op(op::JUMP));
        }
        program.instructions = rewritten;
    }

    fn hoist_hot_terminal_spans(&self, instructions: &mut Vec<AsmInst>) {
        if self.options.optimization == OptimizationMode::None {
            return;
        }
        let instruction_size = |inst: AsmInst| match inst.kind() {
            AsmInstKind::Op(_) | AsmInstKind::Label(_) => 1,
            AsmInstKind::PushLabel(_) | AsmInstKind::PushDeferred(_) => 3,
            AsmInstKind::PushInline(value) => self.push_len(U256::from(value)),
            AsmInstKind::Push(index) => self.push_len(self.push_value(index)),
            AsmInstKind::PushImmutable(_) => 33,
        };
        let Some(first_terminal) = instructions.iter().position(|&inst| is_terminal(inst)) else {
            return;
        };
        let insert_at = first_terminal + 1;
        let insert_offset: usize =
            instructions[..insert_at].iter().map(|&inst| instruction_size(inst)).sum();
        if insert_offset >= 0xff {
            return;
        }
        let mut references = FxHashMap::default();
        for inst in instructions.iter() {
            if let AsmInstKind::PushLabel(label) = inst.kind() {
                *references.entry(label).or_insert(0usize) += 1;
            }
        }
        struct Candidate {
            start: usize,
            end: usize,
            size: usize,
            references: usize,
        }
        let mut candidates = Vec::new();
        let mut i = insert_at;
        while i < instructions.len() {
            if let AsmInstKind::Label(label) = instructions[i].kind()
                && i > 0
                && is_terminal(instructions[i - 1])
            {
                let mut j = i + 1;
                while j < instructions.len()
                    && !matches!(instructions[j].kind(), AsmInstKind::Label(_))
                {
                    if is_terminal(instructions[j]) {
                        let size =
                            instructions[i..=j].iter().map(|&inst| instruction_size(inst)).sum();
                        let references = references.get(&label).copied().unwrap_or(0);
                        if size <= 32 && references >= 2 {
                            candidates.push(Candidate { start: i, end: j, size, references });
                        }
                        break;
                    }
                    j += 1;
                }
            }
            i += 1;
        }
        candidates.sort_by(|a, b| {
            (b.references * a.size)
                .cmp(&(a.references * b.size))
                .then(b.references.cmp(&a.references))
                .then(a.start.cmp(&b.start))
        });
        let mut budget = 0xff_usize.saturating_sub(insert_offset);
        let mut picked = Vec::new();
        for candidate in candidates {
            if candidate.size <= budget {
                budget -= candidate.size;
                picked.push(candidate);
            }
        }
        let mut extraction: Vec<_> = picked.iter().map(|c| (c.start, c.end)).collect();
        extraction.sort_by_key(|(start, _)| std::cmp::Reverse(*start));
        let mut moved = FxHashMap::default();
        for (start, end) in extraction {
            moved.insert(start, instructions.drain(start..=end).collect::<Vec<_>>());
        }
        let mut block = Vec::new();
        for candidate in picked {
            block.extend(moved.remove(&candidate.start).expect("extracted terminal span"));
        }
        instructions.splice(insert_at..insert_at, block);
    }
}

fn is_terminal(inst: AsmInst) -> bool {
    matches!(
        inst.kind(),
        AsmInstKind::Op(
            op::STOP | op::JUMP | op::RETURN | op::REVERT | op::INVALID | op::SELFDESTRUCT,
        )
    )
}

fn lower_bound(instructions: &[AsmInst]) -> usize {
    instructions
        .iter()
        .map(|inst| if matches!(inst.kind(), AsmInstKind::Op(_)) { 1 } else { 2 })
        .sum()
}

fn thread_jump_thunks(instructions: &mut [AsmInst]) -> bool {
    let mut thunks = FxHashMap::default();
    for window in instructions.windows(3) {
        if let AsmInstKind::Label(label) = window[0].kind()
            && let AsmInstKind::PushLabel(target) = window[1].kind()
            && matches!(window[2].kind(), AsmInstKind::Op(op::JUMP))
            && label != target
        {
            thunks.insert(label, target);
        }
    }
    if thunks.is_empty() {
        return false;
    }
    let resolve = |mut label: Label| {
        let mut hops = 0;
        while let Some(&next) = thunks.get(&label) {
            label = next;
            hops += 1;
            if hops > thunks.len() {
                break;
            }
        }
        label
    };
    let mut changed = false;
    for inst in instructions {
        if let AsmInstKind::PushLabel(label) = inst.kind() {
            let target = resolve(label);
            if target != label {
                *inst = AsmInst::push_label(target);
                changed = true;
            }
        }
    }
    changed
}

fn remove_unreachable_code(instructions: &mut Vec<AsmInst>) -> bool {
    let before = instructions.len();
    let mut reachable = true;
    instructions.retain(|inst| {
        if matches!(inst.kind(), AsmInstKind::Label(_)) {
            reachable = true;
            return true;
        }
        let keep = reachable;
        if keep && is_terminal(*inst) {
            reachable = false;
        }
        keep
    });
    instructions.len() != before
}

fn remove_unreferenced_labels(instructions: &mut Vec<AsmInst>) {
    let mut referenced = GrowableBitSet::new_empty();
    for inst in instructions.iter() {
        if let AsmInstKind::PushLabel(label) = inst.kind() {
            referenced.insert(label);
        }
    }
    instructions.retain(|inst| match inst.kind() {
        AsmInstKind::Label(label) => referenced.contains(label),
        _ => true,
    });
}

pub(in crate::backend::evm) fn dedup_terminal_spans(instructions: &mut Vec<AsmInst>) {
    struct Span {
        label: Label,
        start: usize,
        end: usize,
    }
    let mut spans = Vec::new();
    let mut i = 0;
    while i < instructions.len() {
        if let AsmInstKind::Label(label) = instructions[i].kind() {
            let mut j = i + 1;
            while j < instructions.len() {
                if matches!(instructions[j].kind(), AsmInstKind::Label(_)) {
                    break;
                }
                if is_terminal(instructions[j]) {
                    spans.push(Span { label, start: i, end: j });
                    break;
                }
                j += 1;
            }
        }
        i += 1;
    }

    let mut representatives = FxHashMap::default();
    let mut alias = FxHashMap::default();
    let mut delete = Vec::new();
    let mut convert = Vec::new();
    for span in &spans {
        let body = instructions[span.start + 1..=span.end].to_vec();
        match representatives.entry(body) {
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert(span.label);
            }
            std::collections::hash_map::Entry::Occupied(representative) => {
                if span.start > 0 && is_terminal(instructions[span.start - 1]) {
                    alias.insert(span.label, *representative.get());
                    delete.push((span.start, span.end));
                } else if lower_bound(&instructions[span.start + 1..=span.end]) > 6 {
                    alias.insert(span.label, *representative.get());
                    convert.push((span.start, span.end, *representative.get()));
                }
            }
        }
    }
    if alias.is_empty() {
        return;
    }
    for inst in instructions.iter_mut() {
        if let AsmInstKind::PushLabel(label) = inst.kind()
            && let Some(&representative) = alias.get(&label)
        {
            *inst = AsmInst::push_label(representative);
        }
    }
    let mut edits: Vec<_> = delete
        .into_iter()
        .map(|(start, end)| (start, end, None))
        .chain(convert.into_iter().map(|(start, end, target)| (start, end, Some(target))))
        .collect();
    edits.sort_unstable_by_key(|(start, _, _)| *start);
    for (start, end, target) in edits.into_iter().rev() {
        if let Some(target) = target {
            instructions
                .splice(start + 1..=end, [AsmInst::push_label(target), AsmInst::op(op::JUMP)]);
        } else {
            instructions.drain(start..=end);
        }
    }
}
