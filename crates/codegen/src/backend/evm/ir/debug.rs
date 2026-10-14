//! Salvages non-executable variable records across machine rewrites.
//!
//! Before and after a pass, replay each block with symbolic input words. Stack operations retain
//! identities and pure operations intern operand expressions. Memory and storage reads depend
//! on separate write epochs; gas observations require identical machine history. A variable can
//! move to another stack slot containing the same word. Spill observations require an unchanged
//! memory epoch. No executable operand, pass decision, or instruction equality includes these
//! records.
//!
//! Input-dependent recovery across changed prefixes requires unchanged control flow and equal
//! symbolic block exit stacks and memory epochs throughout the module. This also prevents a
//! spill observation surviving removal of its store in another block. Otherwise only closed
//! expressions are salvaged. Shared source origins remain unavailable; no arbitrary origin is
//! selected.

use super::{Block, BlockId, Instruction, Module, TerminatorKind};
use crate::{
    backend::evm::{
        DebugStackVariable, DebugWord,
        op::{self, OpcodeTraits, StackOp},
    },
    mir::utils::eval::eval_opcode,
};
use alloy_primitives::U256;
use solar_data_structures::{bit_set::GrowableBitSet, index::IndexVec, map::FxHashMap};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Word {
    Input(u32, usize),
    Constant(U256),
    Expression(String, Vec<usize>, Option<usize>),
}

#[derive(Default)]
struct Words {
    table: FxHashMap<Word, usize>,
    values: Vec<Word>,
    closed: GrowableBitSet<usize>,
}

impl Words {
    fn intern(&mut self, word: Word) -> usize {
        if let Some(&index) = self.table.get(&word) {
            return index;
        }
        let closed = match &word {
            Word::Input(_, slot) => *slot == usize::MAX - 1 || *slot == usize::MAX - 2,
            Word::Constant(_) => true,
            Word::Expression(_, inputs, history) => {
                history.is_none_or(|history| self.closed.contains(history))
                    && inputs.iter().all(|&input| self.closed.contains(input))
            }
        };
        let index = self.table.len();
        if closed {
            self.closed.insert(index);
        }
        self.values.push(word.clone());
        self.table.insert(word, index);
        index
    }
}

struct State {
    stack: Vec<usize>,
    history: usize,
    memory: usize,
    storage: usize,
}

impl State {
    fn new(words: &mut Words, label: u32) -> Self {
        Self {
            stack: (0..1024).rev().map(|slot| words.intern(Word::Input(label, slot))).collect(),
            history: words.intern(Word::Input(label, usize::MAX)),
            memory: words.intern(Word::Input(label, usize::MAX - 1)),
            storage: words.intern(Word::Input(label, usize::MAX - 2)),
        }
    }

    fn step(
        &mut self,
        words: &mut Words,
        inst: &Instruction,
        labels: &IndexVec<BlockId, u32>,
    ) -> Option<()> {
        let mut machine = inst.clone();
        machine.metadata = Default::default();
        let key = if let Some(value) = inst.concrete_immediate() {
            format!("push:{value}")
        } else if let Some(target) = inst.pushed_block() {
            format!("push-block:{}", labels[target])
        } else {
            format!("{machine:?}")
        };
        match inst.as_stack_op() {
            Some(StackOp::Dup(depth)) => {
                self.stack.push(*self.stack.get(self.stack.len().checked_sub(depth as usize)?)?)
            }
            Some(StackOp::Swap(depth)) => {
                let top = self.stack.len().checked_sub(1)?;
                self.stack.swap(top, top.checked_sub(depth as usize)?);
            }
            Some(StackOp::Exchange(a, b)) => {
                let top = self.stack.len().checked_sub(1)?;
                self.stack.swap(top.checked_sub(a as usize)?, top.checked_sub(b as usize)?);
            }
            Some(StackOp::Pop) => {
                self.stack.pop()?;
            }
            None if inst.is_encoded_push() => {
                let value = if let Some(value) = inst.concrete_immediate() {
                    Word::Constant(value)
                } else {
                    Word::Expression(key, Vec::new(), None)
                };
                self.stack.push(words.intern(value));
            }
            None if inst.as_evm_opcode() == Some(op::PUSH0) => {
                self.stack.push(words.intern(Word::Constant(U256::ZERO)));
            }
            None => {
                let definition = inst.definition()?;
                let (inputs, outputs) = definition.stack_io?;
                let mut operands =
                    (0..inputs).map(|_| self.stack.pop()).collect::<Option<Vec<_>>>()?;
                let folded = operands
                    .iter()
                    .map(|&input| match words.values[input] {
                        Word::Constant(value) => Some(value),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>()
                    .and_then(|values| eval_opcode(definition.opcode, &values));
                if definition.traits.contains(OpcodeTraits::COMMUTATIVE) {
                    operands.sort_unstable();
                }
                let stable = (definition.traits.contains(OpcodeTraits::PURE)
                    || definition.traits.contains(OpcodeTraits::CONTEXT_READ))
                    && !definition.traits.contains(OpcodeTraits::OBSERVES_GAS)
                    && !definition.traits.contains(OpcodeTraits::OBSERVES_MEMORY_SIZE)
                    && !definition.traits.contains(OpcodeTraits::EXECUTION_READ);
                let dependency = if stable {
                    None
                } else if definition.traits.contains(OpcodeTraits::READS_MEMORY) {
                    Some(self.memory)
                } else if definition.traits.contains(OpcodeTraits::READS_STORAGE) {
                    Some(self.storage)
                } else {
                    Some(self.history)
                };
                for output in 0..outputs {
                    let word = folded.map(Word::Constant).unwrap_or_else(|| {
                        Word::Expression(format!("{key}:{output}"), operands.clone(), dependency)
                    });
                    self.stack.push(words.intern(word));
                }
                if definition.writes_memory() {
                    self.memory = words.intern(Word::Expression(
                        key.clone(),
                        operands.clone(),
                        Some(self.memory),
                    ));
                }
                if definition.writes_storage() {
                    self.storage =
                        words.intern(Word::Expression(key, operands, Some(self.storage)));
                }
            }
        }
        self.history = words.intern(Word::Expression(
            format!("{machine:?}"),
            vec![self.history],
            Some(self.history),
        ));
        Some(())
    }
}

pub(super) fn snapshot(module: &Module) -> Option<Module> {
    module
        .blocks
        .iter()
        .any(|block| block.instructions.iter().any(|inst| !inst.metadata.variables.is_empty()))
        .then(|| module.clone())
}

pub(super) fn salvage(module: &mut Module, before: Option<Module>) {
    let Some(before) = before else { return };
    if before.blocks == module.blocks {
        for block in &mut module.blocks {
            for inst in &mut block.instructions {
                inst.metadata.variable_locations_preserved = false;
                inst.metadata.variable_memory_preserved = false;
                if inst.metadata.source_spans().len() != 1 {
                    inst.metadata.variables.clear();
                }
            }
        }
        return;
    }
    let mut words = Words::default();
    let old_labels = before.blocks.iter().map(|block| block.label).collect::<IndexVec<_, _>>();
    let labels = module.blocks.iter().map(|block| block.label).collect::<IndexVec<_, _>>();
    let blocks_by_label =
        module.blocks.iter().map(|block| (block.label, block)).collect::<FxHashMap<_, _>>();
    let mut memory_convention_unchanged = before.blocks.len() == module.blocks.len();
    let convention_unchanged = before.blocks.len() == module.blocks.len()
        && before.blocks.iter().fold(true, |unchanged, old| {
            let Some(new) = blocks_by_label.get(&old.label) else {
                memory_convention_unchanged = false;
                return false;
            };
            if boundary(old, &old_labels) != boundary(new, &labels) {
                memory_convention_unchanged = false;
                return false;
            }
            let mut old_state = State::new(&mut words, old.label);
            let mut new_state = State::new(&mut words, new.label);
            let replayed = old
                .instructions
                .iter()
                .all(|inst| old_state.step(&mut words, inst, &old_labels).is_some())
                && new
                    .instructions
                    .iter()
                    .all(|inst| new_state.step(&mut words, inst, &labels).is_some());
            memory_convention_unchanged &= replayed && old_state.memory == new_state.memory;
            unchanged && replayed && old_state.stack == new_state.stack
        });
    for block in &mut module.blocks {
        let Some(old) = before.blocks.iter().find(|old| old.label == block.label) else {
            for inst in &mut block.instructions {
                let memory_preserved = std::mem::take(&mut inst.metadata.variable_memory_preserved);
                let unique_source = inst.metadata.source_spans().len() == 1;
                if !std::mem::take(&mut inst.metadata.variable_locations_preserved)
                    || !unique_source
                {
                    inst.metadata.variables.retain(|record| {
                        memory_preserved && fixed_carriers(record) && unique_source
                    });
                }
            }
            continue;
        };
        let mut anchors = Vec::new();
        let mut state = State::new(&mut words, block.label);
        for inst in &old.instructions {
            if state.step(&mut words, inst, &old_labels).is_none() {
                break;
            }
            if !inst.metadata.variables.is_empty() {
                anchors.push((
                    inst.metadata.source_spans().to_vec(),
                    inst.metadata.variables.clone(),
                    state.stack.clone(),
                    state.memory,
                ));
            }
        }
        let mut state = State::new(&mut words, block.label);
        let mut replay_valid = true;
        for inst in &mut block.instructions {
            let carried = std::mem::take(&mut inst.metadata.variables);
            let preserved = std::mem::take(&mut inst.metadata.variable_locations_preserved);
            let memory_preserved = std::mem::take(&mut inst.metadata.variable_memory_preserved);
            if !replay_valid || state.step(&mut words, inst, &labels).is_none() {
                replay_valid = false;
                continue;
            }
            if inst.metadata.source_spans().len() != 1 {
                continue;
            }
            if preserved {
                inst.metadata.variables = carried;
                continue;
            }
            let mut recovered = if memory_preserved {
                carried.into_iter().filter(fixed_carriers).collect::<Vec<_>>()
            } else {
                Vec::<DebugStackVariable>::new()
            };
            for (spans, records, stack, memory) in &anchors {
                if spans.as_slice() != inst.metadata.source_spans() {
                    continue;
                }
                for record in records {
                    let mut record = record.clone();
                    let valid = record.words.iter_mut().all(|word| {
                        word.rewrite_leaves(&mut |word| match *word {
                            DebugWord::Stack(slot) => {
                                let Some(&value) = stack.iter().rev().nth(slot as usize) else {
                                    return false;
                                };
                                if !convention_unchanged && !words.closed.contains(value) {
                                    return false;
                                }
                                let Some(slot) =
                                    state.stack.iter().rev().position(|&word| word == value)
                                else {
                                    return false;
                                };
                                *word = DebugWord::Stack(slot as u32);
                                true
                            }
                            DebugWord::Constant(_) => true,
                            DebugWord::Calldata(_) => true,
                            _ => memory_convention_unchanged && *memory == state.memory,
                        })
                    });
                    if valid && !recovered.contains(&record) {
                        recovered.push(record);
                    }
                }
            }
            // Different source assignment epochs at one source point are ambiguous.
            let ambiguous = recovered
                .iter()
                .filter(|record| {
                    recovered.iter().any(|other| {
                        other.variable == record.variable
                            && other.inline == record.inline
                            && other.words != record.words
                    })
                })
                .map(|record| (record.variable, record.inline.clone()))
                .collect::<Vec<_>>();
            recovered
                .retain(|record| !ambiguous.contains(&(record.variable, record.inline.clone())));
            inst.metadata.variables = recovered;
        }
    }
}

// Compare semantic targets by stable labels, independent of block layout or numbering.
fn boundary(block: &Block, labels: &IndexVec<BlockId, u32>) -> Option<Boundary> {
    Some(match &block.terminator.as_ref()?.kind {
        TerminatorKind::Jump(target) => Boundary::Jump(labels[*target]),
        TerminatorKind::JumpI { then_block, else_block } => {
            Boundary::Branch(labels[*then_block], labels[*else_block])
        }
        TerminatorKind::IndexedJump(targets) => {
            Boundary::Indexed(targets.iter().map(|target| labels[*target]).collect())
        }
        TerminatorKind::Op(opcode) => Boundary::Op(*opcode),
    })
}

#[derive(PartialEq, Eq)]
enum Boundary {
    Jump(u32),
    Branch(u32, u32),
    Indexed(Vec<u32>),
    Op(u8),
}

fn fixed_carriers(record: &DebugStackVariable) -> bool {
    record.words.iter().all(|word| {
        let mut word = word.clone();
        word.rewrite_leaves(&mut |leaf| {
            matches!(
                leaf,
                DebugWord::Memory(_)
                    | DebugWord::DeferredMemory(_)
                    | DebugWord::Constant(_)
                    | DebugWord::Calldata(_)
            )
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_recipes_preserve_memory_epochs() {
        let mut words = Words::default();
        let labels = IndexVec::new();
        let mut literal = State::new(&mut words, 0);
        let mut recipe = State::new(&mut words, 0);
        for inst in [
            Instruction::push_value(U256::MAX),
            Instruction::push_value(U256::from(128)),
            Instruction::opcode(op::MSTORE),
        ] {
            assert!(literal.step(&mut words, &inst, &labels).is_some());
        }
        for inst in [
            Instruction::opcode(op::PUSH0),
            Instruction::opcode(op::NOT),
            Instruction::push_value(U256::from(128)),
            Instruction::opcode(op::MSTORE),
        ] {
            assert!(recipe.step(&mut words, &inst, &labels).is_some());
        }
        assert_eq!(literal.stack, recipe.stack);
        assert_eq!(literal.memory, recipe.memory);
        // Dropping a store changes the block's memory boundary even when its exit stack agrees.
        let empty = State::new(&mut words, 0);
        assert_eq!(literal.stack, empty.stack);
        assert_ne!(literal.memory, empty.memory);
    }

    #[test]
    fn history_observers_do_not_collapse() {
        for opcode in [op::GAS, op::MSIZE, op::PC, op::RETURNDATASIZE] {
            let mut words = Words::default();
            let labels = IndexVec::new();
            let mut direct = State::new(&mut words, 0);
            let mut changed = State::new(&mut words, 0);
            for inst in [Instruction::push_value(U256::ZERO), Instruction::opcode(op::POP)] {
                assert!(changed.step(&mut words, &inst, &labels).is_some());
            }
            let observer = Instruction::opcode(opcode);
            assert!(direct.step(&mut words, &observer, &labels).is_some());
            assert!(changed.step(&mut words, &observer, &labels).is_some());
            assert_ne!(direct.stack.last(), changed.stack.last());
        }
    }
}
