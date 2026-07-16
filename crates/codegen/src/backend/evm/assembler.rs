//! Two-pass assembler with label resolution.
//!
//! The assembler handles:
//! - Label definition and reference tracking
//! - Two-pass assembly for resolving jump targets
//! - Variable-width PUSH sizing based on offset magnitudes

use crate::mir::IMMUTABLE_WORD_SIZE;
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_config::{EvmVersion, OptimizationMode};
use solar_data_structures::map::{FxHashMap, FxHashSet};

const EVM_WORD_BYTES: usize = 32;
const EVM_WORD_BITS: usize = EVM_WORD_BYTES * 8;
// The synthesized mask is 5 bytes on Shanghai (6 before), and `compact_push`
// only picks it when strictly smaller than the literal, so any mask of five
// or more bytes can profit: a `uint64` timestamp mask alone is 9 bytes as a
// literal.
const MIN_COMPACT_MASK_WIDTH: u8 = 5;

/// Shortest closed run worth outlining into a shared stub.
const MIN_CLOSED_RUN: usize = 4;

/// A closed-run occurrence: `(start index, length, net stack height)`.
type ClosedRunSite = (usize, usize, u16);

mod id_counter;
use id_counter::IdCounter;

mod inst;
pub(super) use inst::{AsmInst, AsmInstKind, PushValueId};
pub use inst::{DeferredConst, Label};

mod local_interner;
use local_interner::LocalInterner;

mod program;
pub(in crate::backend::evm) use program::{
    EvmAsmProgram, StructuredAsmContext, StructuredAsmProgram,
};

/// A `PUSH32` immutable placeholder emitted into the assembled bytecode.
///
/// TODO: Track placeholder byte width here when smaller immutable references
/// are supported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImmutableRef {
    /// The immutable's byte offset identifier.
    pub id: u32,
    /// Byte offset of the `PUSH32` opcode in the assembled bytecode.
    /// The 32 placeholder bytes start one byte later.
    pub code_offset: usize,
}

/// Result of assembly.
#[derive(Debug)]
pub struct AssembledCode {
    /// The final bytecode.
    pub bytecode: Vec<u8>,
    /// Map from label to its final offset.
    pub label_offsets: FxHashMap<Label, usize>,
    /// All immutable placeholders, in emission order.
    pub immutable_refs: Vec<ImmutableRef>,
}

/// Configuration for EVM bytecode assembly.
#[derive(Clone, Copy, Debug, Default)]
pub struct AssemblerConfig {
    /// EVM version to target when selecting hardfork-gated opcodes.
    pub evm_version: EvmVersion,
    /// Optimization mode for alternate byte encodings.
    pub optimization: OptimizationMode,
    /// Print the time spent in each EVM IR pass.
    pub time_passes: bool,
    /// Run the experimental EVM IR `StackSchedule` pass in the assembler bridge.
    ///
    /// Off by default. See `StructuredAsmProgram::optimize_with_evm_ir` for why
    /// the pass is a verified near no-op on the bridge's operand-cleared IR.
    pub evm_ir_stack_schedule: bool,
    /// Run EVM IR layout/code-size passes in the assembler bridge.
    ///
    /// Kept separate from `evm_ir_stack_schedule` so the experimental scheduler
    /// flag remains bytecode-neutral.
    pub evm_ir_layout_passes: bool,
}

/// Two-pass assembler for EVM bytecode.
#[derive(Debug)]
pub struct Assembler {
    /// Bytecode assembly configuration.
    config: AssemblerConfig,
    /// Structured assembler block program to assemble.
    pub(in crate::backend::evm) program: StructuredAsmProgram,
    /// Interned push immediates too large for inline storage.
    push_values: LocalInterner<U256, PushValueId>,
    /// Next label ID.
    next_label: IdCounter<Label>,
    /// Next deferred constant ID.
    next_deferred: IdCounter<DeferredConst>,
    /// Resolved values for deferred constants.
    deferred_values: FxHashMap<DeferredConst, U256>,
    /// Whether to run structural outlining that changes the emitted length in
    /// content-dependent ways. Off for constructor code, whose argument offset
    /// is resolved by a separate length fixpoint that such passes would break.
    run_structural_outlining: bool,
}

impl Assembler {
    /// Creates a new assembler.
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(AssemblerConfig::default())
    }

    /// Creates a new assembler with the given configuration.
    #[must_use]
    pub fn with_config(config: AssemblerConfig) -> Self {
        Self {
            config,
            program: StructuredAsmProgram::default(),
            push_values: LocalInterner::new(),
            next_label: IdCounter::new(),
            next_deferred: IdCounter::new(),
            deferred_values: FxHashMap::default(),
            run_structural_outlining: false,
        }
    }

    /// Enables or disables structural outlining for the next `assemble`.
    pub(in crate::backend::evm) fn set_structural_outlining(&mut self, enabled: bool) {
        self.run_structural_outlining = enabled;
    }

    /// Clears all emitted instructions and local identifiers while retaining allocated storage.
    pub fn clear(&mut self) {
        self.program.clear();
        self.push_values.clear();
        self.next_label.clear();
        self.next_deferred.clear();
        self.deferred_values.clear();
    }

    /// Creates a new label.
    pub fn new_label(&mut self) -> Label {
        self.next_label.next()
    }

    /// Creates a new deferred constant.
    pub fn new_deferred_const(&mut self) -> DeferredConst {
        self.next_deferred.next()
    }

    /// Emits a raw opcode.
    pub fn emit_op(&mut self, opcode: u8) {
        self.program.push(AsmInst::op(opcode));
    }

    /// Emits a push instruction with an immediate value.
    pub fn emit_push(&mut self, value: U256) {
        let inst = self.push_inst(value);
        self.program.push(inst);
    }

    /// Emits a push instruction that will be resolved to a label's offset.
    pub fn emit_push_label(&mut self, label: Label) {
        self.program.push(AsmInst::push_label(label));
    }

    /// Emits a push instruction for a deferred constant.
    pub fn emit_push_deferred(&mut self, id: DeferredConst) {
        self.program.push(AsmInst::push_deferred(id));
    }

    /// Sets the value of a deferred constant.
    pub fn set_deferred_const(&mut self, id: DeferredConst, value: U256) {
        self.deferred_values.insert(id, value);
    }

    /// Emits a `PUSH32` zero placeholder for the immutable identified by `id`.
    pub fn emit_push_immutable(&mut self, id: u32) {
        self.program.push(AsmInst::push_immutable(id));
    }

    /// Defines a label and emits a `JUMPDEST` at the current position.
    pub fn define_label(&mut self, label: Label) {
        self.program.define_label(label);
    }

    /// Marks a label-started block as cold for EVM IR layout passes.
    pub(in crate::backend::evm) fn mark_label_cold(&mut self, label: Label) {
        self.program.mark_cold(label);
    }

    fn push_inst(&mut self, value: U256) -> AsmInst {
        if let Ok(value) = u32::try_from(value)
            && let Some(inst) = AsmInst::push_inline(value)
        {
            return inst;
        }

        AsmInst::push(self.push_values.intern(value))
    }

    pub(super) fn push_value(&self, index: PushValueId) -> U256 {
        *self.push_values.get(index)
    }

    /// Threads jump references through pure `label: PUSH2 target JUMP` thunks so
    /// the thunks become unreferenced and the dedup chain deletes them. Runtime
    /// only: it changes the emitted length, which the constructor's argument
    /// offset fixpoint would not track.
    fn thread_jump_thunks(instructions: &mut [AsmInst]) -> bool {
        let mut thunks: FxHashMap<Label, Label> = FxHashMap::default();
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
        for inst in instructions.iter_mut() {
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

    /// Removes instruction runs that can never execute: code following an
    /// unconditional control transfer with no label to enter it. Thunk
    /// threading and fallthrough-elided jumps leave such corpses behind (the
    /// `PUSH2 target JUMP` body of a threaded thunk whose label was dropped
    /// still occupies bytes). Removing the run also drops its outgoing label
    /// references, letting the unreferenced-label pass cascade. Runtime only,
    /// like the other length-changing cleanups.
    fn remove_unreachable_code(instructions: &mut Vec<AsmInst>) -> bool {
        let is_terminator = |inst: &AsmInst| {
            matches!(
                inst.kind(),
                AsmInstKind::Op(
                    op::JUMP | op::RETURN | op::REVERT | op::STOP | op::INVALID | op::SELFDESTRUCT
                )
            )
        };
        let before = instructions.len();
        let mut reachable = true;
        instructions.retain(|inst| {
            if matches!(inst.kind(), AsmInstKind::Label(_)) {
                reachable = true;
                return true;
            }
            let keep = reachable;
            if keep && is_terminator(inst) {
                reachable = false;
            }
            keep
        });
        instructions.len() != before
    }

    /// Shares one `revert(0, 0)` stub by inverting the branches that skip it.
    ///
    /// A failed check compiles as `PUSH2 cont JUMPI; PUSH0 DUP1 REVERT; cont:`
    /// — a three-byte stub per site, reached by fallthrough (so span dedup
    /// cannot merge it). Inverting the condition makes every site jump to one
    /// shared stub: `ISZERO PUSH2 shared JUMPI` falls through into `cont`. The
    /// inserted `ISZERO` folds with an existing negation in the later peephole
    /// (`ISZERO ISZERO <label> JUMPI -> <label> JUMPI`), so already-inverted
    /// checks get the stub removed for free. Runtime-only: constructor code
    /// resolves its argument offset with a separate length fixpoint that a
    /// content-dependent length change would break.
    fn invert_branches_over_empty_reverts(&mut self, instructions: &mut Vec<AsmInst>) {
        if self.config.optimization == OptimizationMode::None || !self.run_structural_outlining {
            return;
        }
        let is_zero_push = |inst: AsmInst| matches!(inst.kind(), AsmInstKind::PushInline(0));
        let mut shared: Option<Label> = None;
        let mut alias: FxHashMap<Label, Label> = FxHashMap::default();
        let mut out = Vec::with_capacity(instructions.len());
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
                    && matches!(instructions[j + 1].kind(), AsmInstKind::Op(d) if d == op::DUP1)
                    && matches!(instructions[j + 2].kind(), AsmInstKind::Op(op::REVERT))
                    && matches!(instructions[j + 3].kind(), AsmInstKind::Label(l) if l == cont)
                {
                    let target = *shared.get_or_insert_with(|| self.new_label());
                    if let Some(stub) = stub_label {
                        alias.insert(stub, target);
                    }
                    out.push(AsmInst::op(op::ISZERO));
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
        if !alias.is_empty() {
            for inst in out.iter_mut() {
                if let AsmInstKind::PushLabel(label) = inst.kind()
                    && let Some(&rep) = alias.get(&label)
                {
                    *inst = AsmInst::push_label(rep);
                }
            }
        }
        *instructions = out;
    }

    /// Merges identical terminal suffixes of distinct spans.
    ///
    /// Function epilogues repeat: `LOG3 ... MSTORE ... RETURN` tails compile
    /// identically for every `approve`-shaped function, but the spans differ
    /// earlier, so whole-span dedup cannot merge them. An identical
    /// instruction suffix behaves as a function of the stack it consumes —
    /// each predecessor supplies its own values positionally, exactly as the
    /// inlined copy would have — so a duplicate suffix can be cut and replaced
    /// with a jump into the representative's tail at a freshly inserted label.
    /// Suffixes containing label definitions are never merged, and a cut only
    /// happens when the suffix's conservative emitted size (pushes counted as
    /// two bytes) exceeds the jump that replaces it.
    fn merge_terminal_suffixes(&mut self, instructions: &mut Vec<AsmInst>) -> bool {
        if self.config.optimization == OptimizationMode::None {
            return false;
        }
        fn is_terminal(inst: AsmInst) -> bool {
            matches!(
                inst.kind(),
                AsmInstKind::Op(
                    op::STOP | op::JUMP | op::RETURN | op::REVERT | op::INVALID | op::SELFDESTRUCT,
                )
            )
        }
        fn lower_bound(insts: &[AsmInst]) -> usize {
            insts
                .iter()
                .map(|inst| match inst.kind() {
                    AsmInstKind::Op(_) => 1,
                    _ => 2,
                })
                .sum()
        }

        // Terminal spans: label .. first terminal, with no label inside.
        let mut spans = SmallVec::<[(usize, usize); 16]>::new();
        let mut i = 0;
        while i < instructions.len() {
            if let AsmInstKind::Label(_) = instructions[i].kind() {
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

        // Greedy: each span merges into the earlier span sharing its longest
        // terminal suffix, when that suffix outweighs the jump plus the
        // representative's inserted `JUMPDEST`.
        let mut reps = SmallVec::<[(usize, usize); 16]>::new();
        // (span_start, span_end, rep_index, suffix_len)
        let mut merges = SmallVec::<[(usize, usize, usize, usize); 8]>::new();
        for &(start, end) in &spans {
            let body = &instructions[start..=end];
            let mut best: Option<(usize, usize)> = None;
            for (rep_idx, &(rep_start, rep_end)) in reps.iter().enumerate() {
                let rep_body = &instructions[rep_start..=rep_end];
                let max = body.len().min(rep_body.len());
                let mut common = 0;
                while common < max
                    && body[body.len() - 1 - common] == rep_body[rep_body.len() - 1 - common]
                {
                    common += 1;
                }
                if common > best.map_or(0, |(_, len)| len) {
                    best = Some((rep_idx, common));
                }
            }
            if let Some((rep_idx, common)) = best
                && lower_bound(&body[body.len() - common..]) > 5
            {
                merges.push((start, end, rep_idx, common));
            } else {
                reps.push((start, end));
            }
        }
        if merges.is_empty() {
            return false;
        }

        // One label per (representative, suffix length) cut point.
        let mut cut_labels: FxHashMap<(usize, usize), Label> = FxHashMap::default();
        for &(_, _, rep_idx, common) in &merges {
            cut_labels.entry((rep_idx, common)).or_insert_with(|| self.new_label());
        }

        // Apply from the back so earlier indices stay valid: suffix cuts on
        // merged spans, then label insertions into representatives.
        let mut edits: Vec<(usize, Edit)> = Vec::new();
        enum Edit {
            /// Replace `at..=end` with a jump to the label.
            Cut { end: usize, target: Label },
            /// Insert a label definition at `at`.
            Mid { label: Label },
        }
        for &(_start, end, rep_idx, common) in &merges {
            let target = cut_labels[&(rep_idx, common)];
            edits.push((end + 1 - common, Edit::Cut { end, target }));
        }
        for (&(rep_idx, common), &label) in &cut_labels {
            let (_, rep_end) = reps[rep_idx];
            edits.push((rep_end + 1 - common, Edit::Mid { label }));
        }
        edits.sort_unstable_by_key(|&(at, _)| at);
        for &(at, ref edit) in edits.iter().rev() {
            match *edit {
                Edit::Cut { end, target } => {
                    instructions
                        .splice(at..=end, [AsmInst::push_label(target), AsmInst::op(op::JUMP)]);
                }
                Edit::Mid { label } => {
                    instructions.insert(at, AsmInst::label(label));
                }
            }
        }
        true
    }

    /// Net stack effect of an opcode from a small verified whitelist:
    /// `(reads_below_needed, pops, pushes)`. `reads_below_needed` is how deep
    /// below the top the opcode inspects (its pops, or the index for
    /// `DUP`/`SWAP`); a run stays closed only while the running height covers
    /// it. Returns `None` for anything not on the whitelist, so a run
    /// containing an unmodeled opcode is never outlined.
    fn whitelisted_effect(inst: AsmInst) -> Option<(u16, u16, u16)> {
        Some(match inst.kind() {
            AsmInstKind::PushInline(_)
            | AsmInstKind::Push(_)
            | AsmInstKind::PushLabel(_)
            | AsmInstKind::PushDeferred(_)
            | AsmInstKind::PushImmutable(_) => (0, 0, 1),
            AsmInstKind::Op(op) => match op {
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
                d if (op::DUP1..=op::DUP1 + 15).contains(&d) => {
                    let n = u16::from(d - op::DUP1) + 1;
                    (n, 0, 1)
                }
                sw if (op::SWAP1..=op::SWAP1 + 15).contains(&sw) => {
                    let n = u16::from(sw - op::SWAP1) + 1;
                    (n + 1, 0, 0)
                }
                _ => return None,
            },
            _ => return None,
        })
    }

    /// Outlines identical stack-closed straight-line computations.
    ///
    /// ABI decode prologues repeat — an address argument's dirty-word check or
    /// the calldata-size guard compile identically at each site — but they end
    /// by falling through, not at a terminal, so neither span dedup nor suffix
    /// merging reaches them. A run that reads nothing below its entry and
    /// leaves at most one value behaves as a nullary-or-unary closed
    /// computation: a caller reaches a shared stub with only its return address
    /// on the stack, the stub's produced value (if any) ends up above that
    /// address, and a final `SWAP1` (for one output) then `JUMP` returns it.
    /// Runs are outlined only when structural outlining is enabled (runtime
    /// code, not constructor code), every opcode is on the verified whitelist,
    /// the run never inspects below its entry, its net height is 0 or 1, and
    /// the shared stub is smaller than the sites it replaces.
    fn outline_closed_computations(&mut self, program: &mut EvmAsmProgram) {
        if self.config.optimization == OptimizationMode::None || !self.run_structural_outlining {
            return;
        }
        let insts = &program.instructions;

        // Borrow candidate runs from the immutable input instead of cloning
        // every prefix into an owned hash key. Most runs occur only once, so
        // keep their first two sites inline as well.
        let mut candidates: FxHashMap<&[AsmInst], SmallVec<[ClosedRunSite; 2]>> =
            FxHashMap::default();
        let mut i = 0;
        while i < insts.len() {
            if matches!(insts[i].kind(), AsmInstKind::Label(_)) {
                i += 1;
                continue;
            }
            let mut height: i32 = 0;
            let mut j = i;
            while j < insts.len() {
                if matches!(insts[j].kind(), AsmInstKind::Label(_)) {
                    break;
                }
                let Some((reads, pops, pushes)) = Self::whitelisted_effect(insts[j]) else {
                    break;
                };
                if height < i32::from(reads) {
                    break;
                }
                height = height - i32::from(pops) + i32::from(pushes);
                j += 1;
                let len = j - i;
                if len >= MIN_CLOSED_RUN && (height == 0 || height == 1) {
                    candidates.entry(&insts[i..j]).or_default().push((i, len, height as u16));
                }
            }
            i += 1;
        }

        let mut groups: Vec<_> = candidates.iter().filter(|(_, sites)| sites.len() >= 2).collect();
        groups.sort_by_key(|(key, _)| std::cmp::Reverse(key.len()));

        let mut claimed: Vec<bool> = vec![false; insts.len()];
        let mut chosen: Vec<(Vec<usize>, usize, u16)> = Vec::new();
        for (_, sites) in groups {
            let (run_len, height) = (sites[0].1, sites[0].2);
            let free: Vec<usize> = sites
                .iter()
                .filter(|&&(start, len, _)| (start..start + len).all(|k| !claimed[k]))
                .map(|&(start, _, _)| start)
                .collect();
            if free.len() < 2 {
                continue;
            }
            let run_lb: usize = insts[free[0]..free[0] + run_len]
                .iter()
                .map(|inst| match inst.kind() {
                    AsmInstKind::Op(_) => 1,
                    _ => 2,
                })
                .sum();
            let stub = 1 + run_lb + usize::from(height) + 1;
            // Per-site replacement is `PUSH2 ret PUSH2 stub JUMP JUMPDEST`,
            // but a heavily-referenced stub is a small hot terminal span the
            // hoist pass places below the one-byte boundary, relaxing its
            // push at every site: credit that byte only when the group's
            // reference count makes it a certain hoist pick.
            let per_site = if free.len() >= 4 { 7 } else { 8 };
            if free.len() * run_lb < free.len() * per_site + stub + 2 {
                continue;
            }
            for &start in &free {
                claimed[start..start + run_len].fill(true);
            }
            chosen.push((free, run_len, height));
        }
        if chosen.is_empty() {
            return;
        }

        let stub_labels: Vec<Label> = chosen.iter().map(|_| self.new_label()).collect();
        let mut site_to_group: FxHashMap<usize, usize> = FxHashMap::default();
        for (g, (sites, _, _)) in chosen.iter().enumerate() {
            for &site in sites {
                site_to_group.insert(site, g);
            }
        }

        let mut out = Vec::with_capacity(program.instructions.len());
        let mut k = 0;
        while k < program.instructions.len() {
            if let Some(&g) = site_to_group.get(&k) {
                let run_len = chosen[g].1;
                let ret = self.new_label();
                out.push(AsmInst::push_label(ret));
                out.push(AsmInst::push_label(stub_labels[g]));
                out.push(AsmInst::op(op::JUMP));
                out.push(AsmInst::label(ret));
                k += run_len;
            } else {
                out.push(program.instructions[k]);
                k += 1;
            }
        }
        for (g, (sites, run_len, height)) in chosen.iter().enumerate() {
            let body = program.instructions[sites[0]..sites[0] + run_len].to_vec();
            out.push(AsmInst::label(stub_labels[g]));
            out.extend_from_slice(&body);
            if *height == 1 {
                out.push(AsmInst::op(op::SWAP1));
            }
            out.push(AsmInst::op(op::JUMP));
        }
        program.instructions = out;
    }

    /// Outlines repeated large constants into shared push stubs.
    ///
    /// Event topics and EIP-712 typehashes are 32-byte keccak constants pushed
    /// inline at every use site — 33 bytes each time. When the same constant
    /// appears at several sites and the arithmetic pays, each site becomes a
    /// call-shaped `PUSH2 ret PUSH2 stub JUMP JUMPDEST` (8 bytes) to a shared
    /// `JUMPDEST PUSH<n> value SWAP1 JUMP` stub appended after the program's
    /// terminal instruction. The rewrite is stack-neutral: one value is pushed
    /// either way. Skipped entirely when optimization is disabled.
    fn outline_repeated_pushes(&mut self, program: &mut EvmAsmProgram) {
        if self.config.optimization == OptimizationMode::None {
            return;
        }

        // Count the sites of every interned (large) push value.
        let mut counts: FxHashMap<PushValueId, u32> = FxHashMap::default();
        for inst in &program.instructions {
            if let AsmInstKind::Push(index) = inst.kind() {
                *counts.entry(index).or_default() += 1;
            }
        }
        // A site shrinks from the value's real emitted encoding (compact
        // shapes like `PUSH1 0x1f NOT` can be far narrower than the value's
        // byte length) to 8 bytes; the stub costs the encoding plus 3.
        // Outline when the total saving is worth the jump indirection.
        const SITE_BYTES: usize = 8;
        const MIN_SAVING: usize = 8;
        let mut stubs: FxHashMap<PushValueId, Label> = FxHashMap::default();
        let mut order = Vec::new();
        let sizer = BytecodeAssembler::new(self.config);
        for (&index, &count) in &counts {
            let len = sizer.encoded_push_len(self.push_value(index));
            let count = count as usize;
            let inline = count * len;
            let outlined = count * SITE_BYTES + len + 3;
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

    pub(super) fn inst_push_value(&self, inst: AsmInst) -> Option<U256> {
        match inst.kind() {
            AsmInstKind::PushInline(value) => Some(U256::from(value)),
            AsmInstKind::Push(index) => Some(self.push_value(index)),
            _ => None,
        }
    }

    /// Assembles the instructions into bytecode.
    /// Uses an iterative two-pass algorithm that handles PUSH width changes.
    #[must_use]
    pub fn assemble(&mut self) -> AssembledCode {
        let mut ir_program = std::mem::take(&mut self.program);
        let deferred_values = &self.deferred_values;
        let push_values = &mut self.push_values;
        ir_program.resolve_deferred_consts(|id| {
            let value = deferred_values
                .get(&id)
                .copied()
                .unwrap_or_else(|| panic!("deferred constant {id:?} was never resolved"));
            if let Ok(value) = u32::try_from(value)
                && let Some(inst) = AsmInst::push_inline(value)
            {
                return inst;
            }
            AsmInst::push(push_values.intern(value))
        });
        if self.config.evm_ir_stack_schedule || self.config.evm_ir_layout_passes {
            ir_program.optimize_with_evm_ir(self);
        }
        let mut program = ir_program.to_asm_program();
        self.invert_branches_over_empty_reverts(&mut program.instructions);
        self.run_assembler_passes(&mut program);
        let has_labels =
            program.instructions.iter().any(|inst| matches!(inst.kind(), AsmInstKind::Label(_)));
        if has_labels {
            // Dedup first at the original span granularity, then drop the labels
            // nothing references (which merges fallthrough-split spans), and
            // dedup once more at the coarser granularity that removal exposes.
            Self::dedup_terminal_spans(&mut program.instructions);
            Self::remove_unreferenced_labels(&mut program.instructions);
            Self::dedup_terminal_spans(&mut program.instructions);
            Self::remove_unreferenced_labels(&mut program.instructions);
            // Suffix cuts create new identical spans and tails; iterate the merge
            // with the dedup chain to a capped fixpoint. The unreachable-code
            // sweep needs the label drop at the end of the previous iteration to
            // expose a threaded thunk's corpse, so give the fixpoint headroom.
            for _ in 0..6 {
                let merged = self.merge_terminal_suffixes(&mut program.instructions);
                let threaded = self.run_structural_outlining
                    && Self::thread_jump_thunks(&mut program.instructions);
                let swept = self.run_structural_outlining
                    && Self::remove_unreachable_code(&mut program.instructions);
                if !merged && !threaded && !swept {
                    break;
                }
                Self::dedup_terminal_spans(&mut program.instructions);
                Self::remove_unreferenced_labels(&mut program.instructions);
            }
        } else if self.run_structural_outlining {
            Self::remove_unreachable_code(&mut program.instructions);
        }
        self.outline_closed_computations(&mut program);
        if program
            .instructions
            .iter()
            .filter(|inst| matches!(inst.kind(), AsmInstKind::Push(_)))
            .take(2)
            .count()
            >= 2
        {
            self.outline_repeated_pushes(&mut program);
        }
        self.hoist_hot_terminal_spans(&mut program.instructions);

        // Label-free constructor and deployment snippets need neither offset
        // discovery nor push-width relaxation.
        if !program
            .instructions
            .iter()
            .any(|inst| matches!(inst.kind(), AsmInstKind::Label(_) | AsmInstKind::PushLabel(_)))
        {
            let result = self.emit_bytecode(&program, FxHashMap::default(), &FxHashMap::default());
            self.clear();
            return result;
        }

        // We need to iterate until PUSH widths stabilize
        let mut push_widths: FxHashMap<usize, u8> = FxHashMap::default();

        // Initialize all label pushes to 2 bytes (PUSH2)
        for (idx, inst) in program.instructions.iter().enumerate() {
            if matches!(inst.kind(), AsmInstKind::PushLabel(_)) {
                push_widths.insert(idx, 2);
            }
        }

        // Iterate until stable
        let max_iterations = 10;
        for _ in 0..max_iterations {
            let (label_offsets, new_widths) = self.compute_offsets(&program, &push_widths);

            let mut changed = false;
            for (idx, &width) in &new_widths {
                if push_widths.get(idx) != Some(&width) {
                    changed = true;
                }
            }

            if !changed {
                // Stable - emit final bytecode
                let result = self.emit_bytecode(&program, label_offsets, &push_widths);
                self.clear();
                return result;
            }

            for (idx, width) in new_widths {
                push_widths.insert(idx, width);
            }
        }

        // Fallback - just emit with current widths
        let (label_offsets, _) = self.compute_offsets(&program, &push_widths);
        let result = self.emit_bytecode(&program, label_offsets, &push_widths);
        self.clear();
        result
    }

    /// Moves the most-referenced small terminal spans (shared revert and
    /// panic stubs, merged return tails) to the front of the program, right
    /// after its first terminating instruction, so their addresses fit in one
    /// byte and every `PUSH2 <label>` reference relaxes to `PUSH1` — one byte
    /// per reference. Placement is control-flow-neutral: a candidate span is
    /// preceded by a terminating instruction (no fallthrough in), ends with
    /// one itself (no fallthrough out), and the insertion point directly
    /// follows a terminating instruction. Runtime only: reordering moves the
    /// width relaxation, which the constructor's appended-argument offset
    /// fixpoint does not track.
    fn hoist_hot_terminal_spans(&self, instructions: &mut Vec<AsmInst>) {
        if self.config.optimization == OptimizationMode::None || !self.run_structural_outlining {
            return;
        }

        fn is_terminal(inst: AsmInst) -> bool {
            matches!(
                inst.kind(),
                AsmInstKind::Op(
                    op::STOP | op::JUMP | op::RETURN | op::REVERT | op::INVALID | op::SELFDESTRUCT,
                )
            )
        }
        // Pessimistic emitted size, before width relaxation. Only steers the
        // hoist budget; the exact widths come from the relaxation loop.
        let inst_size = |inst: AsmInst| -> usize {
            match inst.kind() {
                AsmInstKind::Op(_) | AsmInstKind::Label(_) => 1,
                AsmInstKind::PushLabel(_) | AsmInstKind::PushDeferred(_) => 3,
                AsmInstKind::PushInline(value) => 1 + U256::from(value).byte_len(),
                AsmInstKind::Push(id) => 1 + self.push_values.get(id).byte_len(),
                AsmInstKind::PushImmutable(_) => 33,
            }
        };

        let Some(first_terminal) = instructions.iter().position(|&inst| is_terminal(inst)) else {
            return;
        };
        let insert_at = first_terminal + 1;
        let insert_offset: usize = instructions[..insert_at].iter().map(|&i| inst_size(i)).sum();
        if insert_offset >= 0xff {
            return;
        }

        let mut refs: FxHashMap<Label, usize> = FxHashMap::default();
        for inst in instructions.iter() {
            if let AsmInstKind::PushLabel(label) = inst.kind() {
                *refs.entry(label).or_default() += 1;
            }
        }

        // Candidate spans: `label: ... <terminal>` with no interior labels,
        // preceded by a terminal, past the insertion point, and small.
        struct Candidate {
            start: usize,
            end: usize,
            size: usize,
            refs: usize,
        }
        let mut candidates: Vec<Candidate> = Vec::new();
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
                        let size: usize =
                            instructions[i..=j].iter().map(|&inst| inst_size(inst)).sum();
                        let refs = refs.get(&label).copied().unwrap_or(0);
                        if size <= 32 && refs >= 2 {
                            candidates.push(Candidate { start: i, end: j, size, refs });
                        }
                        break;
                    }
                    j += 1;
                }
            }
            i += 1;
        }
        if candidates.is_empty() {
            return;
        }

        // Best saving per budget byte first (each reference saves one byte,
        // the span consumes its size from the one-byte address window), with
        // reference count and position as deterministic tie-breaks.
        candidates.sort_by(|a, b| {
            (b.refs * a.size)
                .cmp(&(a.refs * b.size))
                .then(b.refs.cmp(&a.refs))
                .then(a.start.cmp(&b.start))
        });
        let mut budget = 0xff_usize.saturating_sub(insert_offset);
        let mut picked: Vec<Candidate> = Vec::new();
        for cand in candidates {
            if cand.size <= budget {
                budget -= cand.size;
                picked.push(cand);
            }
        }
        if picked.is_empty() {
            return;
        }

        // Extract in descending start order so earlier ranges stay valid,
        // then splice at the insertion point in rank order.
        let mut extraction = picked.iter().map(|c| (c.start, c.end)).collect::<Vec<_>>();
        extraction.sort_by_key(|&(start, _)| std::cmp::Reverse(start));
        let mut moved: FxHashMap<usize, Vec<AsmInst>> = FxHashMap::default();
        for (start, end) in extraction {
            moved.insert(start, instructions.drain(start..=end).collect());
        }
        let mut block = Vec::new();
        for cand in &picked {
            block.extend(moved.remove(&cand.start).expect("extracted span"));
        }
        instructions.splice(insert_at..insert_at, block);
    }

    /// Deletes label definitions that nothing references: such a block can
    /// only be entered by fallthrough, so its `JUMPDEST` byte is pure waste.
    /// Rewrites and span dedup orphan labels, and running this before the
    /// span dedup also exposes more spans whose predecessor is terminating.
    fn remove_unreferenced_labels(instructions: &mut Vec<AsmInst>) {
        let referenced: FxHashSet<Label> = instructions
            .iter()
            .filter_map(|inst| match inst.kind() {
                AsmInstKind::PushLabel(label) => Some(label),
                _ => None,
            })
            .collect();
        instructions.retain(|inst| match inst.kind() {
            AsmInstKind::Label(label) => referenced.contains(&label),
            _ => true,
        });
    }

    /// Merges byte-identical label-started spans that end in a terminating
    /// opcode, across the whole program (functions included): duplicates are
    /// deleted and every reference to their label is rewritten to the first
    /// occurrence. MIR-level dedup is per function; the shared panic, revert,
    /// and helper-call tails that repeat across functions land here.
    ///
    /// A duplicate is only removed when the instruction before its label is
    /// itself terminating, so no fallthrough path can enter it, and spans
    /// contain no interior label definitions, so no jump can land mid-span.
    fn dedup_terminal_spans(instructions: &mut Vec<AsmInst>) {
        fn is_terminal(inst: AsmInst) -> bool {
            matches!(
                inst.kind(),
                AsmInstKind::Op(
                    op::STOP | op::JUMP | op::RETURN | op::REVERT | op::INVALID | op::SELFDESTRUCT,
                )
            )
        }

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

        // A span whose body's emitted size is provably larger than an
        // explicit `PUSH2 <label> JUMP` (conservative lower bound: pushes are
        // at least two bytes).
        fn body_outweighs_jump(body: &[AsmInst]) -> bool {
            let lower_bound: usize = body
                .iter()
                .map(|inst| match inst.kind() {
                    AsmInstKind::Op(_) => 1,
                    _ => 2,
                })
                .sum();
            lower_bound > 6
        }

        let mut representatives: FxHashMap<Vec<AsmInst>, Label> = FxHashMap::default();
        let mut alias: FxHashMap<Label, Label> = FxHashMap::default();
        let mut delete: Vec<(usize, usize)> = Vec::new();
        let mut convert: Vec<(usize, usize, Label)> = Vec::new();
        for span in &spans {
            let body = instructions[span.start + 1..=span.end].to_vec();
            match representatives.entry(body) {
                std::collections::hash_map::Entry::Vacant(slot) => {
                    slot.insert(span.label);
                }
                std::collections::hash_map::Entry::Occupied(rep) => {
                    if span.start > 0 && is_terminal(instructions[span.start - 1]) {
                        alias.insert(span.label, *rep.get());
                        delete.push((span.start, span.end));
                    } else if body_outweighs_jump(&instructions[span.start + 1..=span.end]) {
                        // Something falls into this copy, so it cannot be
                        // deleted — but its body can become a jump to the
                        // representative.
                        alias.insert(span.label, *rep.get());
                        convert.push((span.start, span.end, *rep.get()));
                    }
                }
            }
        }
        if alias.is_empty() {
            return;
        }

        for inst in instructions.iter_mut() {
            if let AsmInstKind::PushLabel(label) = inst.kind()
                && let Some(&rep) = alias.get(&label)
            {
                *inst = AsmInst::push_label(rep);
            }
        }
        let mut edits: Vec<(usize, usize, Option<Label>)> = delete
            .into_iter()
            .map(|(start, end)| (start, end, None))
            .chain(convert.into_iter().map(|(start, end, rep)| (start, end, Some(rep))))
            .collect();
        edits.sort_unstable_by_key(|&(start, _, _)| start);
        for &(start, end, rep) in edits.iter().rev() {
            match rep {
                None => {
                    instructions.drain(start..=end);
                }
                Some(rep) => {
                    instructions
                        .splice(start + 1..=end, [AsmInst::push_label(rep), AsmInst::op(op::JUMP)]);
                }
            }
        }
    }

    /// Computes label offsets given current PUSH widths.
    fn compute_offsets(
        &self,
        program: &EvmAsmProgram,
        push_widths: &FxHashMap<usize, u8>,
    ) -> (FxHashMap<Label, usize>, FxHashMap<usize, u8>) {
        let mut offset = 0usize;
        let mut label_offsets = FxHashMap::default();
        let mut new_widths = FxHashMap::default();
        let out = BytecodeAssembler::new(self.config);

        for (idx, inst) in program.instructions.iter().enumerate() {
            match inst.kind() {
                AsmInstKind::Op(_) => {
                    offset += 1;
                }
                AsmInstKind::PushInline(value) => {
                    offset += out.encoded_push_len(U256::from(value));
                }
                AsmInstKind::Push(index) => {
                    offset += out.encoded_push_len(self.push_value(index));
                }
                AsmInstKind::PushLabel(_) => {
                    // Use current estimated width
                    let width = push_widths.get(&idx).copied().unwrap_or(2);
                    offset += out.fixed_push_len(width);
                }
                AsmInstKind::PushDeferred(_) => {
                    unreachable!("deferred constants must be resolved before assembly");
                }
                AsmInstKind::PushImmutable(_) => {
                    // PUSH32 opcode plus 32 placeholder bytes.
                    offset += 33;
                }
                AsmInstKind::Label(label) => {
                    label_offsets.insert(label, offset);
                    offset += 1;
                }
            }
        }

        // Compute new widths based on resolved offsets
        for (idx, inst) in program.instructions.iter().enumerate() {
            if let AsmInstKind::PushLabel(label) = inst.kind()
                && let Some(&target_offset) = label_offsets.get(&label)
            {
                let width = out.push_width(U256::from(target_offset));
                new_widths.insert(idx, width);
            }
        }

        (label_offsets, new_widths)
    }

    /// Emits the final bytecode.
    fn emit_bytecode(
        &self,
        program: &EvmAsmProgram,
        label_offsets: FxHashMap<Label, usize>,
        push_widths: &FxHashMap<usize, u8>,
    ) -> AssembledCode {
        let mut out = BytecodeAssembler::new(self.config);

        for (idx, inst) in program.instructions.iter().enumerate() {
            match inst.kind() {
                AsmInstKind::Op(opcode) => {
                    out.emit_op(opcode);
                }
                AsmInstKind::PushInline(value) => {
                    out.emit_push_value(U256::from(value));
                }
                AsmInstKind::Push(index) => {
                    out.emit_push_value(self.push_value(index));
                }
                AsmInstKind::PushLabel(label) => {
                    let target_offset = label_offsets
                        .get(&label)
                        .copied()
                        .unwrap_or_else(|| panic!("label {label:?} was never defined"));
                    let width = push_widths.get(&idx).copied().unwrap_or(2);
                    out.emit_push_fixed_width(U256::from(target_offset), width);
                }
                AsmInstKind::PushDeferred(_) => {
                    unreachable!("deferred constants must be resolved before assembly");
                }
                AsmInstKind::PushImmutable(id) => {
                    out.emit_push_immutable(id);
                }
                AsmInstKind::Label(_) => {
                    out.emit_op(op::JUMPDEST);
                }
            }
        }

        out.finish(label_offsets)
    }

    /// Returns the minimum number of non-zero bytes needed to push a value.
    #[cfg(test)]
    fn push_width(value: U256) -> u8 {
        value.byte_len() as u8
    }
}

impl Default for Assembler {
    fn default() -> Self {
        Self::new()
    }
}

impl StructuredAsmContext for Assembler {
    fn push_value(&self, index: PushValueId) -> U256 {
        self.push_value(index)
    }

    fn push_inst(&mut self, value: U256) -> AsmInst {
        self.push_inst(value)
    }

    fn new_label(&mut self) -> Label {
        self.new_label()
    }

    fn time_passes(&self) -> bool {
        self.config.time_passes
    }

    fn run_evm_ir_stack_schedule(&self) -> bool {
        self.config.evm_ir_stack_schedule
    }

    fn run_evm_ir_layout_passes(&self) -> bool {
        self.config.evm_ir_layout_passes
    }
}

#[derive(Debug)]
struct BytecodeAssembler {
    config: AssemblerConfig,
    bytecode: Vec<u8>,
    immutable_refs: Vec<ImmutableRef>,
}

impl BytecodeAssembler {
    fn new(config: AssemblerConfig) -> Self {
        Self { config, bytecode: Vec::new(), immutable_refs: Vec::new() }
    }

    fn emit_op(&mut self, opcode: u8) {
        self.bytecode.push(opcode);
    }

    fn emit_push_immutable(&mut self, id: u32) {
        self.immutable_refs.push(ImmutableRef { id, code_offset: self.bytecode.len() });
        self.bytecode.push(op::PUSH32);
        self.bytecode.extend(std::iter::repeat_n(0, IMMUTABLE_WORD_SIZE));
    }

    fn encoded_push_len(&self, value: U256) -> usize {
        match self.compact_push(value) {
            CompactPush::Literal { width } => self.fixed_push_len(width),
            CompactPush::FullWord => self.zero_push_len() + 1,
            CompactPush::LowerAllOnesMask { .. } => self.zero_push_len() + 4,
            CompactPush::Not => self.fixed_push_len(self.push_width(!value)) + 1,
            CompactPush::Shl { shift } => {
                self.fixed_push_len(self.push_width(value >> usize::from(shift))) + 3
            }
        }
    }

    fn compact_push(&self, value: U256) -> CompactPush {
        let width = self.push_width(value);
        let normal_len = self.fixed_push_len(width);
        let mut best = (normal_len, CompactPush::Literal { width });

        if self.config.optimization == OptimizationMode::None {
            return best.1;
        }

        let mut consider = |len: usize, compact: CompactPush| {
            if len < best.0 {
                best = (len, compact);
            }
        };

        if value == U256::MAX {
            consider(self.zero_push_len() + 1, CompactPush::FullWord);
        }

        // `PUSH0 NOT PUSH1 <shift> SHR` is fixed-size apart from PUSH0
        // availability: 5 bytes on Shanghai+, 6 bytes before Shanghai; the
        // `consider` comparison keeps the literal whenever it is not larger.
        if width >= MIN_COMPACT_MASK_WIDTH {
            let bytes = value.to_be_bytes::<EVM_WORD_BYTES>();
            let start = EVM_WORD_BYTES - width as usize;
            if bytes[start..].iter().all(|&byte| byte == 0xff) {
                let shift = EVM_WORD_BITS - usize::from(width) * 8;
                consider(
                    self.zero_push_len() + 4,
                    CompactPush::LowerAllOnesMask { shift: shift as u8 },
                );
            }
        }

        // `PUSH<!value> NOT` costs one extra opcode but can be much smaller
        // for values with many leading one bits. It only has a chance to win
        // for full-width values: narrower values have zero high bytes, so
        // inversion turns those into leading `0xff` bytes and needs PUSH32.
        if width as usize == EVM_WORD_BYTES {
            let inverted = !value;
            let inverted_width = self.push_width(inverted);
            let inverted_len = self.fixed_push_len(inverted_width) + 1;
            consider(inverted_len, CompactPush::Not);
        }

        // A left shift can avoid embedding right-aligned zero bytes. The
        // sequence pays three bytes over the shifted literal (`PUSH1
        // <shift> SHL`), so `consider` keeps it only when that actually beats
        // the normal literal.
        let trailing_zero_bytes = (0..EVM_WORD_BYTES).take_while(|&i| value.byte(i) == 0).count();
        if trailing_zero_bytes > 0 && trailing_zero_bytes < EVM_WORD_BYTES {
            let shift = trailing_zero_bytes * 8;
            let shifted = value >> shift;
            let shifted_width = self.push_width(shifted);
            let shifted_len = self.fixed_push_len(shifted_width) + 3;
            consider(shifted_len, CompactPush::Shl { shift: shift as u8 });
        }

        best.1
    }

    /// Emits a PUSH instruction with automatically sized width.
    fn emit_push_value(&mut self, value: U256) {
        match self.compact_push(value) {
            CompactPush::Literal { width } => {
                self.emit_push_fixed_width(value, width);
            }
            CompactPush::FullWord => {
                self.emit_push_zero();
                self.bytecode.push(op::NOT);
            }
            CompactPush::LowerAllOnesMask { shift } => {
                self.emit_push_zero();
                self.bytecode.push(op::NOT);
                self.bytecode.push(op::PUSH1);
                self.bytecode.push(shift);
                self.bytecode.push(op::SHR);
            }
            CompactPush::Not => {
                let inverted = !value;
                self.emit_push_fixed_width(inverted, self.push_width(inverted));
                self.bytecode.push(op::NOT);
            }
            CompactPush::Shl { shift } => {
                let shifted = value >> usize::from(shift);
                self.emit_push_fixed_width(shifted, self.push_width(shifted));
                self.bytecode.push(op::PUSH1);
                self.bytecode.push(shift);
                self.bytecode.push(op::SHL);
            }
        }
    }

    /// Emits a PUSH instruction with a specific width.
    fn emit_push_fixed_width(&mut self, value: U256, width: u8) {
        if width == 0 {
            self.emit_push_zero();
            return;
        }

        self.bytecode.push(op::push(width));

        let bytes = value.to_be_bytes::<EVM_WORD_BYTES>();
        let start = EVM_WORD_BYTES - width as usize;
        self.bytecode.extend_from_slice(&bytes[start..]);
    }

    fn emit_push_zero(&mut self) {
        if self.config.evm_version.has_push0() {
            self.bytecode.push(op::PUSH0);
        } else {
            self.bytecode.push(op::PUSH1);
            self.bytecode.push(0);
        }
    }

    fn fixed_push_len(&self, width: u8) -> usize {
        if width == 0 { self.zero_push_len() } else { 1 + width as usize }
    }

    fn zero_push_len(&self) -> usize {
        if self.config.evm_version.has_push0() { 1 } else { 2 }
    }

    /// Returns the minimum immediate width needed to push a value for this EVM version.
    fn push_width(&self, value: U256) -> u8 {
        if value.is_zero() && !self.config.evm_version.has_push0() {
            1
        } else {
            value.byte_len() as u8
        }
    }

    fn finish(self, label_offsets: FxHashMap<Label, usize>) -> AssembledCode {
        AssembledCode {
            bytecode: self.bytecode,
            label_offsets,
            immutable_refs: self.immutable_refs,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CompactPush {
    /// Emit the value as the shortest literal PUSH for the active EVM version.
    Literal { width: u8 },
    /// Emit all ones as `PUSH0 NOT`.
    FullWord,
    /// Emit a lower-bit all-ones mask as `PUSH0 NOT PUSH1 <shift> SHR`.
    LowerAllOnesMask { shift: u8 },
    /// Emit a value with many leading one bits as `PUSH<!value> NOT`.
    Not,
    /// Emit a value with trailing zero bytes as `PUSH<value >> shift> PUSH1 <shift> SHL`.
    Shl { shift: u8 },
}

/// Common EVM op.
pub mod op {
    pub const STOP: u8 = 0x00;
    pub const ADD: u8 = 0x01;
    pub const MUL: u8 = 0x02;
    pub const SUB: u8 = 0x03;
    pub const DIV: u8 = 0x04;
    pub const SDIV: u8 = 0x05;
    pub const MOD: u8 = 0x06;
    pub const SMOD: u8 = 0x07;
    pub const ADDMOD: u8 = 0x08;
    pub const MULMOD: u8 = 0x09;
    pub const EXP: u8 = 0x0a;
    pub const SIGNEXTEND: u8 = 0x0b;

    pub const LT: u8 = 0x10;
    pub const GT: u8 = 0x11;
    pub const SLT: u8 = 0x12;
    pub const SGT: u8 = 0x13;
    pub const EQ: u8 = 0x14;
    pub const ISZERO: u8 = 0x15;
    pub const AND: u8 = 0x16;
    pub const OR: u8 = 0x17;
    pub const XOR: u8 = 0x18;
    pub const NOT: u8 = 0x19;
    pub const BYTE: u8 = 0x1a;
    pub const SHL: u8 = 0x1b;
    pub const SHR: u8 = 0x1c;
    pub const SAR: u8 = 0x1d;
    pub const CLZ: u8 = 0x1e;

    pub const KECCAK256: u8 = 0x20;

    pub const ADDRESS: u8 = 0x30;
    pub const BALANCE: u8 = 0x31;
    pub const ORIGIN: u8 = 0x32;
    pub const CALLER: u8 = 0x33;
    pub const CALLVALUE: u8 = 0x34;
    pub const CALLDATALOAD: u8 = 0x35;
    pub const CALLDATASIZE: u8 = 0x36;
    pub const CALLDATACOPY: u8 = 0x37;
    pub const CODESIZE: u8 = 0x38;
    pub const CODECOPY: u8 = 0x39;
    pub const GASPRICE: u8 = 0x3a;
    pub const EXTCODESIZE: u8 = 0x3b;
    pub const EXTCODECOPY: u8 = 0x3c;
    pub const RETURNDATASIZE: u8 = 0x3d;
    pub const RETURNDATACOPY: u8 = 0x3e;
    pub const EXTCODEHASH: u8 = 0x3f;

    pub const BLOCKHASH: u8 = 0x40;
    pub const COINBASE: u8 = 0x41;
    pub const TIMESTAMP: u8 = 0x42;
    pub const NUMBER: u8 = 0x43;
    pub const PREVRANDAO: u8 = 0x44;
    pub const GASLIMIT: u8 = 0x45;
    pub const CHAINID: u8 = 0x46;
    pub const SELFBALANCE: u8 = 0x47;
    pub const BASEFEE: u8 = 0x48;
    pub const BLOBHASH: u8 = 0x49;
    pub const BLOBBASEFEE: u8 = 0x4a;

    pub const POP: u8 = 0x50;
    pub const MLOAD: u8 = 0x51;
    pub const MSTORE: u8 = 0x52;
    pub const MSTORE8: u8 = 0x53;
    pub const SLOAD: u8 = 0x54;
    pub const SSTORE: u8 = 0x55;
    pub const JUMP: u8 = 0x56;
    pub const JUMPI: u8 = 0x57;
    pub const PC: u8 = 0x58;
    pub const MSIZE: u8 = 0x59;
    pub const GAS: u8 = 0x5a;
    pub const JUMPDEST: u8 = 0x5b;
    pub const TLOAD: u8 = 0x5c;
    pub const TSTORE: u8 = 0x5d;
    pub const MCOPY: u8 = 0x5e;
    pub const PUSH0: u8 = 0x5f;
    pub const PUSH1: u8 = 0x60;
    pub const PUSH2: u8 = 0x61;
    pub const PUSH3: u8 = 0x62;
    pub const PUSH4: u8 = 0x63;
    pub const PUSH5: u8 = 0x64;
    pub const PUSH6: u8 = 0x65;
    pub const PUSH7: u8 = 0x66;
    pub const PUSH8: u8 = 0x67;
    pub const PUSH9: u8 = 0x68;
    pub const PUSH10: u8 = 0x69;
    pub const PUSH11: u8 = 0x6a;
    pub const PUSH12: u8 = 0x6b;
    pub const PUSH13: u8 = 0x6c;
    pub const PUSH14: u8 = 0x6d;
    pub const PUSH15: u8 = 0x6e;
    pub const PUSH16: u8 = 0x6f;
    pub const PUSH17: u8 = 0x70;
    pub const PUSH18: u8 = 0x71;
    pub const PUSH19: u8 = 0x72;
    pub const PUSH20: u8 = 0x73;
    pub const PUSH21: u8 = 0x74;
    pub const PUSH22: u8 = 0x75;
    pub const PUSH23: u8 = 0x76;
    pub const PUSH24: u8 = 0x77;
    pub const PUSH25: u8 = 0x78;
    pub const PUSH26: u8 = 0x79;
    pub const PUSH27: u8 = 0x7a;
    pub const PUSH28: u8 = 0x7b;
    pub const PUSH29: u8 = 0x7c;
    pub const PUSH30: u8 = 0x7d;
    pub const PUSH31: u8 = 0x7e;
    pub const PUSH32: u8 = 0x7f;

    pub const DUP1: u8 = 0x80;
    pub const DUP2: u8 = 0x81;
    pub const DUP3: u8 = 0x82;
    pub const DUP4: u8 = 0x83;
    pub const DUP5: u8 = 0x84;
    pub const DUP6: u8 = 0x85;
    pub const DUP7: u8 = 0x86;
    pub const DUP8: u8 = 0x87;
    pub const DUP9: u8 = 0x88;
    pub const DUP10: u8 = 0x89;
    pub const DUP11: u8 = 0x8a;
    pub const DUP12: u8 = 0x8b;
    pub const DUP13: u8 = 0x8c;
    pub const DUP14: u8 = 0x8d;
    pub const DUP15: u8 = 0x8e;
    pub const DUP16: u8 = 0x8f;

    pub const SWAP1: u8 = 0x90;
    pub const SWAP2: u8 = 0x91;
    pub const SWAP3: u8 = 0x92;
    pub const SWAP4: u8 = 0x93;
    pub const SWAP5: u8 = 0x94;
    pub const SWAP6: u8 = 0x95;
    pub const SWAP7: u8 = 0x96;
    pub const SWAP8: u8 = 0x97;
    pub const SWAP9: u8 = 0x98;
    pub const SWAP10: u8 = 0x99;
    pub const SWAP11: u8 = 0x9a;
    pub const SWAP12: u8 = 0x9b;
    pub const SWAP13: u8 = 0x9c;
    pub const SWAP14: u8 = 0x9d;
    pub const SWAP15: u8 = 0x9e;
    pub const SWAP16: u8 = 0x9f;

    pub const LOG0: u8 = 0xa0;
    pub const LOG1: u8 = 0xa1;
    pub const LOG2: u8 = 0xa2;
    pub const LOG3: u8 = 0xa3;
    pub const LOG4: u8 = 0xa4;

    pub const DATALOAD: u8 = 0xd0;
    pub const DATALOADN: u8 = 0xd1;
    pub const DATASIZE: u8 = 0xd2;
    pub const DATACOPY: u8 = 0xd3;

    pub const RJUMP: u8 = 0xe0;
    pub const RJUMPI: u8 = 0xe1;
    pub const RJUMPV: u8 = 0xe2;
    pub const CALLF: u8 = 0xe3;
    pub const RETF: u8 = 0xe4;
    pub const JUMPF: u8 = 0xe5;
    pub const DUPN: u8 = 0xe6;
    pub const SWAPN: u8 = 0xe7;
    pub const EXCHANGE: u8 = 0xe8;
    pub const EOFCREATE: u8 = 0xec;
    pub const RETURNCONTRACT: u8 = 0xee;

    pub const CREATE: u8 = 0xf0;
    pub const CALL: u8 = 0xf1;
    pub const CALLCODE: u8 = 0xf2;
    pub const RETURN: u8 = 0xf3;
    pub const DELEGATECALL: u8 = 0xf4;
    pub const CREATE2: u8 = 0xf5;
    pub const RETURNDATALOAD: u8 = 0xf7;
    pub const EXTCALL: u8 = 0xf8;
    pub const EXTDELEGATECALL: u8 = 0xf9;
    pub const STATICCALL: u8 = 0xfa;
    pub const EXTSTATICCALL: u8 = 0xfb;
    pub const REVERT: u8 = 0xfd;
    pub const INVALID: u8 = 0xfe;
    pub const SELFDESTRUCT: u8 = 0xff;

    /// Returns the PUSH opcode for the given width (1-32).
    #[must_use]
    pub const fn push(width: u8) -> u8 {
        debug_assert!(width >= 1 && width <= 32);
        PUSH1 + width - 1
    }

    /// Returns the DUP opcode for the given depth (1-16).
    #[must_use]
    pub const fn dup(n: u8) -> u8 {
        debug_assert!(n >= 1 && n <= 16);
        DUP1 + n - 1
    }

    /// Returns the SWAP opcode for the given depth (1-16).
    #[must_use]
    pub const fn swap(n: u8) -> u8 {
        debug_assert!(n >= 1 && n <= 16);
        SWAP1 + n - 1
    }

    /// Returns whether an opcode halts or unconditionally transfers control.
    #[must_use]
    pub const fn is_terminal(op: u8) -> bool {
        matches!(op, STOP | JUMP | RETURN | REVERT | INVALID | SELFDESTRUCT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size_optimized_assembler() -> Assembler {
        Assembler::with_config(AssemblerConfig {
            evm_version: EvmVersion::Shanghai,
            optimization: OptimizationMode::Size,
            ..AssemblerConfig::default()
        })
    }

    #[test]
    fn test_push_width() {
        assert_eq!(Assembler::push_width(U256::ZERO), 0);
        assert_eq!(Assembler::push_width(U256::from(1)), 1);
        assert_eq!(Assembler::push_width(U256::from(255)), 1);
        assert_eq!(Assembler::push_width(U256::from(256)), 2);
        assert_eq!(Assembler::push_width(U256::from(0xFFFF)), 2);
        assert_eq!(Assembler::push_width(U256::from(0x10000)), 3);
    }

    #[test]
    fn assembler_inst_is_compact() {
        assert_eq!(std::mem::size_of::<AsmInst>(), 4);
    }

    #[test]
    fn push_values_are_inline_or_interned() {
        let mut asm = Assembler::new();
        let inline = u32::MAX >> 1;
        let large = U256::from(1u64 << 31);

        assert!(AsmInst::push_inline(inline).is_some());
        assert!(AsmInst::push_inline(1u32 << 31).is_none());

        asm.emit_push(U256::from(inline));
        asm.emit_push(large);
        asm.emit_push(large);

        let instructions = asm.program.instructions();
        assert_eq!(instructions[0].kind(), AsmInstKind::PushInline(inline));
        assert_eq!(instructions[1].kind(), AsmInstKind::Push(PushValueId::from_usize(0)));
        assert_eq!(instructions[1], instructions[2]);
        assert_eq!(asm.push_values.len(), 1);
        assert_eq!(*asm.push_values.get(PushValueId::from_usize(0)), large);
    }

    #[test]
    fn assembler_can_be_reused_after_assembly() {
        let mut asm = Assembler::new();
        let large = U256::from(1u64 << 31);

        asm.emit_push(large);
        let first = asm.assemble();

        assert_eq!(first.bytecode, vec![0x63, 0x80, 0, 0, 0]);
        assert!(asm.program.instructions().is_empty());
        assert_eq!(asm.push_values.len(), 0);

        asm.emit_push(U256::from(2));
        let second = asm.assemble();

        assert_eq!(second.bytecode, vec![0x60, 2]);
    }

    #[test]
    fn push_zero_uses_push0_when_available() {
        let mut asm = Assembler::with_config(AssemblerConfig {
            evm_version: EvmVersion::Shanghai,
            optimization: OptimizationMode::None,
            ..AssemblerConfig::default()
        });

        asm.emit_push(U256::ZERO);
        let result = asm.assemble();

        assert_eq!(result.bytecode, vec![op::PUSH0]);
    }

    #[test]
    fn push_zero_uses_push1_before_shanghai() {
        let mut asm = Assembler::with_config(AssemblerConfig {
            evm_version: EvmVersion::Berlin,
            optimization: OptimizationMode::Gas,
            ..AssemblerConfig::default()
        });

        asm.emit_push(U256::ZERO);
        let result = asm.assemble();

        assert_eq!(result.bytecode, vec![op::PUSH1, 0]);
    }

    #[test]
    fn compact_push_respects_optimization_mode() {
        let mut size_optimized = Assembler::with_config(AssemblerConfig {
            evm_version: EvmVersion::Shanghai,
            optimization: OptimizationMode::Size,
            ..AssemblerConfig::default()
        });
        size_optimized.emit_push(U256::MAX);

        let mut gas_optimized = Assembler::with_config(AssemblerConfig {
            evm_version: EvmVersion::Shanghai,
            optimization: OptimizationMode::Gas,
            ..AssemblerConfig::default()
        });
        gas_optimized.emit_push(U256::MAX);

        let mut unoptimized = Assembler::with_config(AssemblerConfig {
            evm_version: EvmVersion::Shanghai,
            optimization: OptimizationMode::None,
            ..AssemblerConfig::default()
        });
        unoptimized.emit_push(U256::MAX);

        let compact = vec![op::PUSH0, op::NOT];
        assert_eq!(size_optimized.assemble().bytecode, compact);
        assert_eq!(gas_optimized.assemble().bytecode, compact);

        let mut expected = vec![op::PUSH32];
        expected.extend(std::iter::repeat_n(0xff, 32));
        assert_eq!(unoptimized.assemble().bytecode, expected);
    }

    #[test]
    fn compact_push_uses_push1_zero_before_shanghai() {
        let mut asm = Assembler::with_config(AssemblerConfig {
            evm_version: EvmVersion::Berlin,
            optimization: OptimizationMode::Size,
            ..AssemblerConfig::default()
        });

        asm.emit_push(U256::MAX);
        let result = asm.assemble();

        assert_eq!(result.bytecode, vec![op::PUSH1, 0, op::NOT]);
    }

    #[test]
    fn test_simple_assembly() {
        let mut asm = Assembler::new();

        asm.emit_push(U256::from(42));
        asm.emit_push(U256::from(10));
        asm.emit_op(op::ADD);
        asm.emit_op(op::STOP);

        let result = asm.assemble();

        // PUSH1 42, PUSH1 10, ADD, STOP
        assert_eq!(result.bytecode, vec![0x60, 42, 0x60, 10, 0x01, 0x00]);
    }

    #[test]
    fn test_label_resolution() {
        let mut asm = Assembler::new();

        let loop_label = asm.new_label();
        let end_label = asm.new_label();

        asm.define_label(loop_label);
        asm.emit_push(U256::from(1));
        asm.emit_push_label(end_label);
        asm.emit_op(op::JUMPI);
        asm.emit_push_label(loop_label);
        asm.emit_op(op::JUMP);

        asm.define_label(end_label);
        asm.emit_op(op::STOP);

        let result = asm.assemble();

        // Check labels were resolved
        assert!(result.label_offsets.contains_key(&loop_label));
        assert!(result.label_offsets.contains_key(&end_label));
        assert_eq!(result.label_offsets[&loop_label], 0);
    }

    #[test]
    fn cold_terminal_block_moves_after_hot_block() {
        let mut asm = Assembler::with_config(AssemblerConfig {
            evm_ir_layout_passes: true,
            ..AssemblerConfig::default()
        });
        let cold = asm.new_label();
        let hot = asm.new_label();

        asm.emit_push_label(hot);
        asm.emit_op(op::JUMP);
        asm.mark_label_cold(cold);
        asm.define_label(cold);
        asm.emit_push(U256::ZERO);
        asm.emit_push(U256::ZERO);
        asm.emit_op(op::REVERT);
        asm.define_label(hot);
        asm.emit_op(op::STOP);

        let result = asm.assemble();

        assert_eq!(
            result.bytecode,
            // The unreferenced cold block's `JUMPDEST` is elided: nothing
            // jumps to it, so it is a dead byte.
            vec![op::PUSH1, 3, op::JUMP, op::JUMPDEST, op::STOP, op::PUSH0, op::PUSH0, op::REVERT,]
        );
    }

    #[test]
    fn cold_terminal_block_keeps_fallthrough_position() {
        let mut asm = Assembler::with_config(AssemblerConfig {
            evm_ir_layout_passes: true,
            ..AssemblerConfig::default()
        });
        let cold = asm.new_label();

        asm.emit_push(U256::ONE);
        asm.mark_label_cold(cold);
        asm.define_label(cold);
        asm.emit_push(U256::ZERO);
        asm.emit_push(U256::ZERO);
        asm.emit_op(op::REVERT);

        let result = asm.assemble();

        assert_eq!(
            result.bytecode,
            // The unreferenced cold `JUMPDEST` is elided.
            vec![op::PUSH1, 1, op::PUSH0, op::PUSH0, op::REVERT]
        );
    }

    #[test]
    fn terminal_span_dedup_converts_fallthrough_copy() {
        let mut asm = size_optimized_assembler();
        let representative = asm.new_label();
        let duplicate = asm.new_label();
        let body = [
            AsmInst::push_inline(0x1234).unwrap(),
            AsmInst::push_inline(0).unwrap(),
            AsmInst::op(op::MSTORE),
            AsmInst::push_inline(17).unwrap(),
            AsmInst::push_inline(4).unwrap(),
            AsmInst::op(op::MSTORE),
            AsmInst::push_inline(36).unwrap(),
            AsmInst::push_inline(0).unwrap(),
            AsmInst::op(op::REVERT),
        ];
        let mut instructions = vec![AsmInst::label(representative)];
        instructions.extend(body);
        instructions.push(AsmInst::push_inline(1).unwrap());
        instructions.push(AsmInst::label(duplicate));
        instructions.extend(body);

        Assembler::dedup_terminal_spans(&mut instructions);

        assert_eq!(
            &instructions[instructions.len() - 3..],
            &[
                AsmInst::label(duplicate),
                AsmInst::push_label(representative),
                AsmInst::op(op::JUMP),
            ]
        );
    }

    #[test]
    fn compact_full_word_all_ones_push() {
        let mut asm = size_optimized_assembler();

        asm.emit_push(U256::MAX);
        asm.emit_op(op::STOP);

        let result = asm.assemble();

        assert_eq!(result.bytecode, vec![op::PUSH0, op::NOT, op::STOP]);
    }

    #[test]
    fn compact_lower_all_ones_mask_push() {
        let mut asm = size_optimized_assembler();
        let mask = (U256::from(1) << 160) - U256::from(1);

        asm.emit_push(mask);
        asm.emit_op(op::STOP);

        let result = asm.assemble();

        assert_eq!(result.bytecode, vec![op::PUSH0, op::NOT, 0x60, 96, op::SHR, op::STOP]);
    }

    #[test]
    fn compact_not_small_push() {
        let mut asm = size_optimized_assembler();

        asm.emit_push(!U256::from(31));
        asm.emit_op(op::STOP);

        let result = asm.assemble();

        assert_eq!(result.bytecode, vec![0x60, 31, op::NOT, op::STOP]);
    }

    #[test]
    fn compact_not_byte_push() {
        let mut asm = size_optimized_assembler();

        asm.emit_push(!U256::from(255));
        asm.emit_op(op::STOP);

        let result = asm.assemble();

        assert_eq!(result.bytecode, vec![0x60, 255, op::NOT, op::STOP]);
    }

    #[test]
    fn compact_left_aligned_selector_push() {
        let mut asm = size_optimized_assembler();
        let selector = U256::from(0x35ea6a75u64) << 224;

        asm.emit_push(selector);
        asm.emit_op(op::STOP);

        let result = asm.assemble();

        assert_eq!(
            result.bytecode,
            vec![0x63, 0x35, 0xea, 0x6a, 0x75, 0x60, 224, op::SHL, op::STOP]
        );
    }

    #[test]
    fn compact_right_padded_text_push() {
        let mut asm = size_optimized_assembler();
        let text = U256::from_be_slice(b"Machine finished:");
        let value = text << ((32 - "Machine finished:".len()) * 8);

        asm.emit_push(value);
        asm.emit_op(op::STOP);

        let result = asm.assemble();

        let mut expected = vec![0x70];
        expected.extend_from_slice(b"Machine finished:");
        expected.extend_from_slice(&[0x60, 120, op::SHL, op::STOP]);
        assert_eq!(result.bytecode, expected);
    }
}
