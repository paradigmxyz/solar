//! Peephole optimization over assembler instructions.

use super::{
    assembler::{AsmInst, AsmInstKind, Assembler, Label, op},
    ir::EvmAsmProgram,
};
use alloy_primitives::U256;
use solar_data_structures::map::FxHashMap;

impl Assembler {
    /// Runs local peephole optimizations over assembler instructions.
    ///
    /// This pass runs before label resolution, so removing instructions cannot
    /// leave stale jump destinations.
    pub(super) fn optimize_instructions(&mut self) -> usize {
        let mut program = std::mem::take(&mut self.program);
        let changed = program.optimize_instructions(
            |inst| self.inst_push_value(inst),
            |inst| self.estimated_inst_size(inst),
        );
        self.program = program;
        changed
    }
}

impl EvmAsmProgram {
    /// Runs local peephole optimizations over the EVM backend IR.
    pub(in crate::backend::evm) fn optimize_instructions<P, S>(
        &mut self,
        inst_push_value: P,
        estimated_inst_size: S,
    ) -> usize
    where
        P: FnMut(AsmInst) -> Option<U256>,
        S: FnMut(AsmInst) -> usize,
    {
        EvmAsmProgramOptimizer { program: self, inst_push_value, estimated_inst_size }.run()
    }
}

struct EvmAsmProgramOptimizer<'a, P, S> {
    program: &'a mut EvmAsmProgram,
    inst_push_value: P,
    estimated_inst_size: S,
}

impl<P, S> EvmAsmProgramOptimizer<'_, P, S>
where
    P: FnMut(AsmInst) -> Option<U256>,
    S: FnMut(AsmInst) -> usize,
{
    fn run(&mut self) -> usize {
        let mut total = 0;
        let len = self.program.instructions.len();
        let mut read = 0;
        let mut write = 0;

        while read < len {
            if write != read {
                self.program.instructions.swap(write, read);
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
                self.program.instructions[start..start + replacement_len]
                    .copy_from_slice(&peephole.replacement[..replacement_len]);
                write = start + replacement_len;
                total += 1;
            }
        }
        self.program.instructions.truncate(write);

        total += self.deduplicate_terminal_blocks();
        total
    }

    fn deduplicate_terminal_blocks(&mut self) -> usize {
        let candidates = self.terminal_block_candidates();
        if candidates.is_empty() {
            return 0;
        }

        let mut canonical: FxHashMap<Vec<AsmInst>, Label> = FxHashMap::default();
        let mut replacements: FxHashMap<usize, Label> = FxHashMap::default();

        for block in candidates {
            if let Some(&target) = canonical.get(&block.key) {
                let replacement_size = 1 + 3 + 1; // JUMPDEST + PUSH2(label) + JUMP.
                if block.estimated_size > replacement_size {
                    replacements.insert(block.label_index, target);
                }
            } else {
                canonical.insert(block.key, block.label);
            }
        }

        if replacements.is_empty() {
            return 0;
        }

        let mut optimized = Vec::with_capacity(self.program.instructions.len());
        let mut removed = 0;
        let mut i = 0;
        while i < self.program.instructions.len() {
            if let Some(&target) = replacements.get(&i)
                && let AsmInstKind::Label(label) = self.program.instructions[i].kind()
                && let Some(end) = self.terminal_block_end(i)
            {
                optimized.push(AsmInst::label(label));
                optimized.push(AsmInst::push_label(target));
                optimized.push(AsmInst::op(op::JUMP));
                removed += 1;
                i = end + 1;
                continue;
            }
            optimized.push(self.program.instructions[i]);
            i += 1;
        }

        self.program.instructions = optimized;
        removed
    }

    fn terminal_block_candidates(&mut self) -> Vec<TerminalBlock> {
        let mut candidates = Vec::new();
        for i in 0..self.program.instructions.len().saturating_sub(1) {
            let AsmInstKind::Label(label) = self.program.instructions[i].kind() else {
                continue;
            };
            let Some(end) = self.terminal_block_end(i) else {
                continue;
            };
            let body = &self.program.instructions[i..=end];
            let key = body
                .iter()
                .copied()
                .filter(|inst| !matches!(inst.kind(), AsmInstKind::Label(_)))
                .collect();
            let estimated_size = body.iter().map(|&inst| (self.estimated_inst_size)(inst)).sum();
            candidates.push(TerminalBlock { label, label_index: i, key, estimated_size });
        }
        candidates
    }

    fn terminal_block_end(&self, start: usize) -> Option<usize> {
        for i in start..self.program.instructions.len() {
            if i != start && matches!(self.program.instructions[i].kind(), AsmInstKind::Label(_)) {
                return None;
            }
            if let AsmInstKind::Op(op) = self.program.instructions[i].kind()
                && op::is_terminal(op)
            {
                return Some(i);
            }
        }
        None
    }

    #[inline]
    fn try_peephole(&mut self, write: usize) -> Option<Peephole> {
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

        let stack = InstStack::new(&self.program.instructions[..write]);

        if stack.len() >= 3
            && is_removable_push(stack[2])
            && let (Some(value), AsmInstKind::Op(op)) =
                ((self.inst_push_value)(stack[1]), stack[0].kind())
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
                ((self.inst_push_value)(stack[1]), stack[0].kind())
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
            && is_removable_push(stack[1])
            && matches!(stack[0].kind(), AsmInstKind::Op(op::POP))
        {
            return peephole!(2 => []);
        }

        if stack.len() >= 2
            && let (AsmInstKind::Op(a), AsmInstKind::Op(b)) = (stack[1].kind(), stack[0].kind())
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
            && matches!(stack[2].kind(), AsmInstKind::Op(op::ISZERO))
            && matches!(stack[1].kind(), AsmInstKind::Op(op::ISZERO))
            && matches!(stack[0].kind(), AsmInstKind::Op(op::ISZERO))
        {
            return peephole!(3 => [AsmInst::op(op::ISZERO)]);
        }

        None
    }
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

#[derive(Clone, Debug)]
struct TerminalBlock {
    label: Label,
    label_index: usize,
    key: Vec<AsmInst>,
    estimated_size: usize,
}

#[derive(Clone, Copy, Debug)]
struct InstStack<'a> {
    instructions: &'a [AsmInst],
}

impl<'a> InstStack<'a> {
    fn new(instructions: &'a [AsmInst]) -> Self {
        Self { instructions }
    }

    fn len(self) -> usize {
        self.instructions.len()
    }
}

impl std::ops::Index<usize> for InstStack<'_> {
    type Output = AsmInst;

    fn index(&self, index: usize) -> &Self::Output {
        &self.instructions[self.instructions.len() - 1 - index]
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_push_zero_add() {
        let mut asm = Assembler::new();

        asm.emit_push(U256::from(42));
        asm.emit_push(U256::ZERO);
        asm.emit_op(op::ADD);
        asm.emit_op(op::STOP);

        let result = asm.assemble();

        assert_eq!(result.bytecode, vec![0x60, 42, op::STOP]);
    }

    #[test]
    fn cascades_after_rewrite() {
        let mut asm = Assembler::new();

        asm.emit_push(U256::from(42));
        asm.emit_push(U256::ZERO);
        asm.emit_op(op::ADD);
        asm.emit_op(op::POP);

        let result = asm.assemble();

        assert!(result.bytecode.is_empty());
    }

    #[test]
    fn resolves_labels_after_rewrites() {
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
    fn replaces_mul_zero_with_pop_zero() {
        let mut asm = Assembler::new();

        asm.emit_push(U256::from(42));
        asm.emit_push(U256::ZERO);
        asm.emit_op(op::MUL);
        asm.emit_op(op::STOP);

        let result = asm.assemble();

        assert_eq!(result.bytecode, vec![op::PUSH0, op::STOP]);
    }

    #[test]
    fn preserves_push_zero_sub() {
        let mut asm = Assembler::new();

        asm.emit_push(U256::from(42));
        asm.emit_push(U256::ZERO);
        asm.emit_op(op::SUB);
        asm.emit_op(op::STOP);

        let result = asm.assemble();

        assert_eq!(result.bytecode, vec![0x60, 42, op::PUSH0, op::SUB, op::STOP]);
    }

    #[test]
    fn rewrites_push_zero_eq() {
        let mut asm = Assembler::new();

        asm.emit_push(U256::from(42));
        asm.emit_push(U256::ZERO);
        asm.emit_op(op::EQ);
        asm.emit_op(op::STOP);

        let result = asm.assemble();

        assert_eq!(result.bytecode, vec![0x60, 42, op::ISZERO, op::STOP]);
    }

    #[test]
    fn preserves_push_one_div() {
        let mut asm = Assembler::new();

        asm.emit_push(U256::from(42));
        asm.emit_push(U256::from(1));
        asm.emit_op(op::DIV);
        asm.emit_op(op::STOP);

        let result = asm.assemble();

        assert_eq!(result.bytecode, vec![0x60, 42, 0x60, 1, op::DIV, op::STOP]);
    }
}
