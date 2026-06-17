//! Two-pass assembler with label resolution.
//!
//! The assembler handles:
//! - Label definition and reference tracking
//! - Two-pass assembly for resolving jump targets
//! - Variable-width PUSH sizing based on offset magnitudes

use crate::mir::IMMUTABLE_WORD_SIZE;
use alloy_primitives::U256;
use solar_config::{EvmVersion, OptimizationMode};
use solar_data_structures::map::FxHashMap;

mod id_counter;
use id_counter::IdCounter;

mod inst;
use inst::{AsmInst, AsmInstKind, PushValueId};
pub use inst::{DeferredConst, Label};

mod local_interner;
use local_interner::LocalInterner;

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
}

/// Two-pass assembler for EVM bytecode.
#[derive(Debug)]
pub struct Assembler {
    /// Bytecode assembly configuration.
    config: AssemblerConfig,
    /// Instructions to assemble.
    instructions: Vec<AsmInst>,
    /// Interned push immediates too large for inline storage.
    push_values: LocalInterner<U256, PushValueId>,
    /// Next label ID.
    next_label: IdCounter<Label>,
    /// Next deferred constant ID.
    next_deferred: IdCounter<DeferredConst>,
    /// Resolved values for deferred constants.
    deferred_values: FxHashMap<DeferredConst, U256>,
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
            instructions: Vec::new(),
            push_values: LocalInterner::new(),
            next_label: IdCounter::new(),
            next_deferred: IdCounter::new(),
            deferred_values: FxHashMap::default(),
        }
    }

    /// Clears all emitted instructions and local identifiers while retaining allocated storage.
    pub fn clear(&mut self) {
        self.instructions.clear();
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
        self.instructions.push(AsmInst::op(opcode));
    }

    /// Emits a push instruction with an immediate value.
    pub fn emit_push(&mut self, value: U256) {
        let inst = self.push_inst(value);
        self.instructions.push(inst);
    }

    /// Emits a push instruction that will be resolved to a label's offset.
    pub fn emit_push_label(&mut self, label: Label) {
        self.instructions.push(AsmInst::push_label(label));
    }

    /// Emits a push instruction for a deferred constant.
    pub fn emit_push_deferred(&mut self, id: DeferredConst) {
        self.instructions.push(AsmInst::push_deferred(id));
    }

    /// Sets the value of a deferred constant.
    pub fn set_deferred_const(&mut self, id: DeferredConst, value: U256) {
        self.deferred_values.insert(id, value);
    }

    /// Emits a `PUSH32` zero placeholder for the immutable identified by `id`.
    pub fn emit_push_immutable(&mut self, id: u32) {
        self.instructions.push(AsmInst::push_immutable(id));
    }

    /// Defines a label and emits a `JUMPDEST` at the current position.
    pub fn define_label(&mut self, label: Label) {
        self.instructions.push(AsmInst::label(label));
    }

    /// Marks the current position with a label without emitting a `JUMPDEST`.
    pub fn mark_label(&mut self, label: Label) {
        self.instructions.push(AsmInst::mark(label));
    }

    fn resolve_deferred_consts(&mut self) {
        for i in 0..self.instructions.len() {
            if let AsmInstKind::PushDeferred(id) = self.instructions[i].kind() {
                let value = self
                    .deferred_values
                    .get(&id)
                    .copied()
                    .unwrap_or_else(|| panic!("deferred constant {id:?} was never resolved"));
                self.instructions[i] = self.push_inst(value);
            }
        }
    }

    fn push_inst(&mut self, value: U256) -> AsmInst {
        if let Ok(value) = u32::try_from(value)
            && let Some(inst) = AsmInst::push_inline(value)
        {
            return inst;
        }

        AsmInst::push(self.push_values.intern(value))
    }

    fn push_value(&self, index: PushValueId) -> U256 {
        *self.push_values.get(index)
    }

    fn inst_push_value(&self, inst: AsmInst) -> Option<U256> {
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
        self.resolve_deferred_consts();
        self.optimize_instructions();

        // We need to iterate until PUSH widths stabilize
        let mut push_widths: FxHashMap<usize, u8> = FxHashMap::default();

        // Initialize all label pushes to 2 bytes (PUSH2)
        for (idx, inst) in self.instructions.iter().enumerate() {
            if matches!(inst.kind(), AsmInstKind::PushLabel(_)) {
                push_widths.insert(idx, 2);
            }
        }

        // Iterate until stable
        let max_iterations = 10;
        for _ in 0..max_iterations {
            let (label_offsets, new_widths) = self.compute_offsets(&push_widths);

            let mut changed = false;
            for (idx, &width) in &new_widths {
                if push_widths.get(idx) != Some(&width) {
                    changed = true;
                }
            }

            if !changed {
                // Stable - emit final bytecode
                let result = self.emit_bytecode(&label_offsets, &push_widths);
                self.clear();
                return result;
            }

            for (idx, width) in new_widths {
                push_widths.insert(idx, width);
            }
        }

        // Fallback - just emit with current widths
        let (label_offsets, _) = self.compute_offsets(&push_widths);
        let result = self.emit_bytecode(&label_offsets, &push_widths);
        self.clear();
        result
    }

    /// Computes label offsets given current PUSH widths.
    fn compute_offsets(
        &self,
        push_widths: &FxHashMap<usize, u8>,
    ) -> (FxHashMap<Label, usize>, FxHashMap<usize, u8>) {
        let mut offset = 0usize;
        let mut label_offsets = FxHashMap::default();
        let mut new_widths = FxHashMap::default();
        let out = BytecodeAssembler::new(self.config);

        for (idx, inst) in self.instructions.iter().enumerate() {
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
                AsmInstKind::Mark(label) => {
                    label_offsets.insert(label, offset);
                }
            }
        }

        // Compute new widths based on resolved offsets
        for (idx, inst) in self.instructions.iter().enumerate() {
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
        label_offsets: &FxHashMap<Label, usize>,
        push_widths: &FxHashMap<usize, u8>,
    ) -> AssembledCode {
        let mut out = BytecodeAssembler::new(self.config);

        for (idx, inst) in self.instructions.iter().enumerate() {
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
                AsmInstKind::Mark(_) => {
                    // Position markers do not emit anything.
                }
            }
        }

        out.finish(label_offsets.clone())
    }

    /// Runs local peephole optimizations over assembler instructions.
    ///
    /// This pass runs before label resolution, so removing instructions cannot
    /// leave stale jump destinations.
    fn optimize_instructions(&mut self) -> usize {
        let mut total = 0;
        let len = self.instructions.len();
        let mut read = 0;
        let mut write = 0;

        while read < len {
            if write != read {
                self.instructions.swap(write, read);
            }
            read += 1;
            write += 1;

            while let Some(peephole) = self.try_peephole(write) {
                let skip = peephole.skip as usize;
                let replacement_len = peephole.replacement_len as usize;
                debug_assert!(
                    replacement_len <= skip,
                    "peepholes must not produce a larger replacement"
                );
                debug_assert!(skip <= write);

                let start = write - skip;
                self.instructions[start..start + replacement_len]
                    .copy_from_slice(&peephole.replacement[..replacement_len]);
                write = start + replacement_len;
                total += 1;
            }
        }
        self.instructions.truncate(write);

        total += self.deduplicate_terminal_blocks();
        total
    }

    fn deduplicate_terminal_blocks(&mut self) -> usize {
        let candidates = self.terminal_block_candidates();
        if candidates.is_empty() {
            return 0;
        }

        let mut canonical: FxHashMap<Vec<AsmInst>, TerminalBlock> = FxHashMap::default();
        let mut replacements: FxHashMap<usize, Label> = FxHashMap::default();

        for block in candidates {
            if let Some(target) = canonical.get(&block.key) {
                let replacement_size = 1 + 3 + 1; // JUMPDEST + PUSH2(label) + JUMP.
                if block.estimated_size > replacement_size {
                    replacements.insert(block.label_index, target.label);
                }
            } else {
                canonical.insert(block.key.clone(), block);
            }
        }

        if replacements.is_empty() {
            return 0;
        }

        let mut optimized = Vec::with_capacity(self.instructions.len());
        let mut removed = 0;
        let mut i = 0;
        while i < self.instructions.len() {
            if let Some(&target) = replacements.get(&i)
                && let AsmInstKind::Label(label) = self.instructions[i].kind()
                && let Some(end) = self.terminal_block_end(i)
            {
                optimized.push(AsmInst::label(label));
                optimized.push(AsmInst::push_label(target));
                optimized.push(AsmInst::op(op::JUMP));
                removed += 1;
                i = end + 1;
                continue;
            }
            optimized.push(self.instructions[i]);
            i += 1;
        }

        self.instructions = optimized;
        removed
    }

    fn terminal_block_candidates(&self) -> Vec<TerminalBlock> {
        let mut candidates = Vec::new();
        for i in 0..self.instructions.len().saturating_sub(1) {
            let AsmInstKind::Label(label) = self.instructions[i].kind() else {
                continue;
            };
            let Some(end) = self.terminal_block_end(i) else {
                continue;
            };
            let body = &self.instructions[i..=end];
            let key = body
                .iter()
                .copied()
                .filter(|inst| !matches!(inst.kind(), AsmInstKind::Label(_) | AsmInstKind::Mark(_)))
                .collect();
            let estimated_size = body.iter().map(|&inst| self.estimated_inst_size(inst)).sum();
            candidates.push(TerminalBlock { label, label_index: i, key, estimated_size });
        }
        candidates
    }

    fn terminal_block_end(&self, start: usize) -> Option<usize> {
        for i in start..self.instructions.len() {
            if i != start && matches!(self.instructions[i].kind(), AsmInstKind::Label(_)) {
                return None;
            }
            if matches!(self.instructions[i].kind(), AsmInstKind::Mark(_)) {
                continue;
            }
            if let AsmInstKind::Op(op) = self.instructions[i].kind()
                && op::is_terminal(op)
            {
                return Some(i);
            }
        }
        None
    }

    #[inline]
    fn try_peephole(&self, write: usize) -> Option<Peephole> {
        macro_rules! peephole {
            ($skip:expr => []) => {
                Some(Peephole::delete($skip))
            };
            ($skip:expr => [$inst:expr]) => {
                Some(Peephole::replace_1($skip, $inst))
            };
            ($skip:expr => [$a:expr, $b:expr]) => {
                Some(Peephole::replace_2($skip, $a, $b))
            };
        }

        let stack = &self.instructions[..write];

        if stack.len() >= 3
            && Self::is_removable_push(stack[stack.len() - 3])
            && let (Some(value), AsmInstKind::Op(op)) =
                (self.inst_push_value(stack[stack.len() - 2]), stack[stack.len() - 1].kind())
        {
            // `PUSH<N> PUSH0 MUL -> PUSH0`.
            if value.is_zero()
                && matches!(
                    op,
                    op::MUL | op::DIV | op::SDIV | op::MOD | op::SMOD | op::AND | op::GT
                )
            {
                return peephole!(3 => [AsmInst::push_inline(0).unwrap()]);
            }

            // `PUSH<N> PUSH1 EXP -> PUSH1`.
            if value == U256::ONE && op == op::EXP {
                return peephole!(3 => [AsmInst::push_inline(1).unwrap()]);
            }
        }

        if stack.len() >= 2
            && let (Some(value), AsmInstKind::Op(op)) =
                (self.inst_push_value(stack[stack.len() - 2]), stack[stack.len() - 1].kind())
        {
            if value.is_zero() {
                return match op {
                    // `PUSH0 ADD -> []`.
                    op::ADD | op::OR | op::XOR | op::SHL | op::SHR | op::SAR => peephole!(2 => []),
                    // `PUSH0 EQ -> ISZERO`.
                    op::EQ => peephole!(2 => [AsmInst::op(op::ISZERO)]),
                    // `PUSH0 MUL -> POP PUSH0`.
                    op::MUL | op::DIV | op::SDIV | op::MOD | op::SMOD | op::AND | op::GT => {
                        peephole!(2 => [
                            AsmInst::op(op::POP),
                            AsmInst::push_inline(0).unwrap()
                        ])
                    }
                    _ => None,
                };
            }

            if value == U256::ONE {
                return match op {
                    // `PUSH1 MUL -> []`.
                    op::MUL => peephole!(2 => []),
                    // `PUSH1 EXP -> POP PUSH1`.
                    op::EXP => peephole!(2 => [
                        AsmInst::op(op::POP),
                        AsmInst::push_inline(1).unwrap()
                    ]),
                    _ => None,
                };
            }
        }

        // `PUSH POP -> []`.
        if stack.len() >= 2
            && Self::is_removable_push(stack[stack.len() - 2])
            && matches!(stack[stack.len() - 1].kind(), AsmInstKind::Op(op::POP))
        {
            return peephole!(2 => []);
        }

        if stack.len() >= 2
            && let (AsmInstKind::Op(a), AsmInstKind::Op(b)) =
                (stack[stack.len() - 2].kind(), stack[stack.len() - 1].kind())
        {
            match (a, b) {
                // `NOT NOT -> []`.
                (op::NOT, op::NOT) => {
                    return peephole!(2 => []);
                }
                // `DUP<N> POP -> []`.
                (op, op::POP) if (op::DUP1..=op::DUP1 + 15).contains(&op) => {
                    return peephole!(2 => []);
                }
                // `SWAP<N> SWAP<N> -> []`.
                (a, b) if a == b && (op::SWAP1..=op::SWAP1 + 15).contains(&a) => {
                    return peephole!(2 => []);
                }
                _ => {}
            }
        }

        // `ISZERO ISZERO ISZERO -> ISZERO`.
        if stack.len() >= 3
            && matches!(stack[stack.len() - 3].kind(), AsmInstKind::Op(op::ISZERO))
            && matches!(stack[stack.len() - 2].kind(), AsmInstKind::Op(op::ISZERO))
            && matches!(stack[stack.len() - 1].kind(), AsmInstKind::Op(op::ISZERO))
        {
            return peephole!(3 => [AsmInst::op(op::ISZERO)]);
        }

        None
    }

    fn is_removable_push(inst: AsmInst) -> bool {
        matches!(
            inst.kind(),
            AsmInstKind::PushInline(_)
                | AsmInstKind::Push(_)
                | AsmInstKind::PushLabel(_)
                | AsmInstKind::PushImmutable(_)
        )
    }

    fn estimated_inst_size(&self, inst: AsmInst) -> usize {
        match inst.kind() {
            AsmInstKind::Op(_) => 1,
            AsmInstKind::PushInline(value) => {
                BytecodeAssembler::new(self.config).encoded_push_len(U256::from(value))
            }
            AsmInstKind::Push(index) => {
                BytecodeAssembler::new(self.config).encoded_push_len(self.push_value(index))
            }
            AsmInstKind::PushLabel(_) => 3,
            AsmInstKind::PushDeferred(_) => {
                unreachable!("deferred constants must be resolved before assembly")
            }
            AsmInstKind::PushImmutable(_) => 33,
            AsmInstKind::Label(_) => 1,
            AsmInstKind::Mark(_) => 0,
        }
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
        self.compact_push(value).map_or_else(
            || self.fixed_push_len(self.push_width(value)),
            |compact| match compact {
                CompactPush::FullWord => self.zero_push_len() + 1,
                CompactPush::LowerAllOnesMask { .. } => self.zero_push_len() + 4,
                CompactPush::Not { value } => self.fixed_push_len(self.push_width(value)) + 1,
                CompactPush::Shl { value, .. } => self.fixed_push_len(self.push_width(value)) + 3,
            },
        )
    }

    fn compact_push(&self, value: U256) -> Option<CompactPush> {
        if self.config.optimization == OptimizationMode::None {
            return None;
        }

        let width = self.push_width(value);
        let normal_len = self.fixed_push_len(width);
        let mut best: Option<(usize, CompactPush)> = None;

        let mut consider = |len: usize, compact: CompactPush| {
            if len < normal_len && best.is_none_or(|(best_len, _)| len < best_len) {
                best = Some((len, compact));
            }
        };

        if value == U256::MAX {
            consider(2, CompactPush::FullWord);
        }

        if width >= 16 {
            let bytes = value.to_be_bytes::<32>();
            let start = 32 - width as usize;
            if bytes[start..].iter().all(|&byte| byte == 0xff) {
                let shift = 256 - u16::from(width) * 8;
                consider(5, CompactPush::LowerAllOnesMask { shift: shift as u8 });
            }
        }

        if width >= 16 {
            let inverted = !value;
            let inverted_width = self.push_width(inverted);
            let inverted_len = self.fixed_push_len(inverted_width) + 1;
            consider(inverted_len, CompactPush::Not { value: inverted });
        }

        let trailing_zero_bytes = (0..32).take_while(|&i| value.byte(i) == 0).count();
        if trailing_zero_bytes > 0 && trailing_zero_bytes < 32 {
            let shift = trailing_zero_bytes * 8;
            let shifted = value >> shift;
            let shifted_width = self.push_width(shifted);
            let shifted_len = self.fixed_push_len(shifted_width) + 3;
            consider(shifted_len, CompactPush::Shl { value: shifted, shift: shift as u8 });
        }

        best.map(|(_, compact)| compact)
    }

    /// Emits a PUSH instruction with automatically sized width.
    fn emit_push_value(&mut self, value: U256) {
        match self.compact_push(value) {
            Some(CompactPush::FullWord) => {
                self.emit_push_zero();
                self.bytecode.push(op::NOT);
                return;
            }
            Some(CompactPush::LowerAllOnesMask { shift }) => {
                self.emit_push_zero();
                self.bytecode.push(op::NOT);
                self.bytecode.push(op::PUSH1);
                self.bytecode.push(shift);
                self.bytecode.push(op::SHR);
                return;
            }
            Some(CompactPush::Not { value }) => {
                self.emit_push_fixed_width(value, self.push_width(value));
                self.bytecode.push(op::NOT);
                return;
            }
            Some(CompactPush::Shl { value, shift }) => {
                self.emit_push_fixed_width(value, self.push_width(value));
                self.bytecode.push(op::PUSH1);
                self.bytecode.push(shift);
                self.bytecode.push(op::SHL);
                return;
            }
            None => {}
        }

        self.emit_push_fixed_width(value, self.push_width(value));
    }

    /// Emits a PUSH instruction with a specific width.
    fn emit_push_fixed_width(&mut self, value: U256, width: u8) {
        if width == 0 {
            self.emit_push_zero();
            return;
        }

        self.bytecode.push(op::push(width));

        let bytes = value.to_be_bytes::<32>();
        let start = 32 - width as usize;
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

#[derive(Clone, Debug)]
struct TerminalBlock {
    label: Label,
    label_index: usize,
    key: Vec<AsmInst>,
    estimated_size: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Peephole {
    skip: u32,
    replacement_len: u32,
    replacement: [AsmInst; 2],
}

impl Peephole {
    fn delete(skip: u32) -> Self {
        Self { skip, replacement_len: 0, replacement: [AsmInst::PLACEHOLDER; 2] }
    }

    fn replace_1(skip: u32, inst: AsmInst) -> Self {
        Self { skip, replacement_len: 1, replacement: [inst, AsmInst::PLACEHOLDER] }
    }

    fn replace_2(skip: u32, a: AsmInst, b: AsmInst) -> Self {
        Self { skip, replacement_len: 2, replacement: [a, b] }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CompactPush {
    FullWord,
    LowerAllOnesMask { shift: u8 },
    Not { value: U256 },
    Shl { value: U256, shift: u8 },
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

    /// Returns the PUSH opcode for the given width (1-32).
    #[must_use]
    pub const fn push(width: u8) -> u8 {
        debug_assert!(width >= 1 && width <= 32);
        PUSH1 + width - 1
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

        assert_eq!(asm.instructions[0].kind(), AsmInstKind::PushInline(inline));
        assert_eq!(asm.instructions[1].kind(), AsmInstKind::Push(PushValueId::from_usize(0)));
        assert_eq!(asm.instructions[1], asm.instructions[2]);
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
        assert!(asm.instructions.is_empty());
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
        });

        asm.emit_push(U256::ZERO);
        let result = asm.assemble();

        assert_eq!(result.bytecode, vec![op::PUSH1, 0]);
    }

    #[test]
    fn compact_push_respects_optimization_mode() {
        let mut optimized = Assembler::with_config(AssemblerConfig {
            evm_version: EvmVersion::Shanghai,
            optimization: OptimizationMode::Gas,
        });
        optimized.emit_push(U256::MAX);

        let mut unoptimized = Assembler::with_config(AssemblerConfig {
            evm_version: EvmVersion::Shanghai,
            optimization: OptimizationMode::None,
        });
        unoptimized.emit_push(U256::MAX);

        assert_eq!(optimized.assemble().bytecode, vec![op::PUSH0, op::NOT]);

        let mut expected = vec![op::PUSH32];
        expected.extend(std::iter::repeat_n(0xff, 32));
        assert_eq!(unoptimized.assemble().bytecode, expected);
    }

    #[test]
    fn compact_push_uses_push1_zero_before_shanghai() {
        let mut asm = Assembler::with_config(AssemblerConfig {
            evm_version: EvmVersion::Berlin,
            optimization: OptimizationMode::Gas,
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
    fn peephole_removes_push_zero_add() {
        let mut asm = Assembler::new();

        asm.emit_push(U256::from(42));
        asm.emit_push(U256::ZERO);
        asm.emit_op(op::ADD);
        asm.emit_op(op::STOP);

        let result = asm.assemble();

        assert_eq!(result.bytecode, vec![0x60, 42, op::STOP]);
    }

    #[test]
    fn peephole_cascades_after_rewrite() {
        let mut asm = Assembler::new();

        asm.emit_push(U256::from(42));
        asm.emit_push(U256::ZERO);
        asm.emit_op(op::ADD);
        asm.emit_op(op::POP);

        let result = asm.assemble();

        assert!(result.bytecode.is_empty());
    }

    #[test]
    fn peephole_resolves_labels_after_rewrites() {
        let mut asm = Assembler::new();
        let label = asm.new_label();

        asm.emit_push(U256::from(42));
        asm.emit_push(U256::ZERO);
        asm.emit_op(op::ADD);
        asm.define_label(label);
        asm.emit_push_label(label);
        asm.emit_op(op::JUMP);

        let result = asm.assemble();

        assert_eq!(result.label_offsets[&label], 2);
        assert_eq!(result.bytecode, vec![0x60, 42, op::JUMPDEST, 0x60, 2, op::JUMP]);
    }

    #[test]
    fn mark_label_does_not_emit_jumpdest() {
        let mut asm = Assembler::new();
        let label = asm.new_label();

        asm.emit_push(U256::from(42));
        asm.mark_label(label);
        asm.emit_op(op::STOP);

        let result = asm.assemble();

        assert_eq!(result.label_offsets[&label], 2);
        assert_eq!(result.bytecode, vec![0x60, 42, op::STOP]);
    }

    #[test]
    fn peephole_replaces_mul_zero_with_pop_zero() {
        let mut asm = Assembler::new();

        asm.emit_push(U256::from(42));
        asm.emit_push(U256::ZERO);
        asm.emit_op(op::MUL);
        asm.emit_op(op::STOP);

        let result = asm.assemble();

        assert_eq!(result.bytecode, vec![op::PUSH0, op::STOP]);
    }

    #[test]
    fn peephole_preserves_push_zero_sub() {
        let mut asm = Assembler::new();

        asm.emit_push(U256::from(42));
        asm.emit_push(U256::ZERO);
        asm.emit_op(op::SUB);
        asm.emit_op(op::STOP);

        let result = asm.assemble();

        assert_eq!(result.bytecode, vec![0x60, 42, op::PUSH0, op::SUB, op::STOP]);
    }

    #[test]
    fn peephole_rewrites_push_zero_eq() {
        let mut asm = Assembler::new();

        asm.emit_push(U256::from(42));
        asm.emit_push(U256::ZERO);
        asm.emit_op(op::EQ);
        asm.emit_op(op::STOP);

        let result = asm.assemble();

        assert_eq!(result.bytecode, vec![0x60, 42, op::ISZERO, op::STOP]);
    }

    #[test]
    fn peephole_preserves_push_one_div() {
        let mut asm = Assembler::new();

        asm.emit_push(U256::from(42));
        asm.emit_push(U256::from(1));
        asm.emit_op(op::DIV);
        asm.emit_op(op::STOP);

        let result = asm.assemble();

        assert_eq!(result.bytecode, vec![0x60, 42, 0x60, 1, op::DIV, op::STOP]);
    }

    #[test]
    fn compact_full_word_all_ones_push() {
        let mut asm = Assembler::new();

        asm.emit_push(U256::MAX);
        asm.emit_op(op::STOP);

        let result = asm.assemble();

        assert_eq!(result.bytecode, vec![op::PUSH0, op::NOT, op::STOP]);
    }

    #[test]
    fn compact_lower_all_ones_mask_push() {
        let mut asm = Assembler::new();
        let mask = (U256::from(1) << 160) - U256::from(1);

        asm.emit_push(mask);
        asm.emit_op(op::STOP);

        let result = asm.assemble();

        assert_eq!(result.bytecode, vec![op::PUSH0, op::NOT, 0x60, 96, op::SHR, op::STOP]);
    }

    #[test]
    fn compact_not_small_push() {
        let mut asm = Assembler::new();

        asm.emit_push(!U256::from(31));
        asm.emit_op(op::STOP);

        let result = asm.assemble();

        assert_eq!(result.bytecode, vec![0x60, 31, op::NOT, op::STOP]);
    }

    #[test]
    fn compact_not_byte_push() {
        let mut asm = Assembler::new();

        asm.emit_push(!U256::from(255));
        asm.emit_op(op::STOP);

        let result = asm.assemble();

        assert_eq!(result.bytecode, vec![0x60, 255, op::NOT, op::STOP]);
    }

    #[test]
    fn compact_left_aligned_selector_push() {
        let mut asm = Assembler::new();
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
        let mut asm = Assembler::new();
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
