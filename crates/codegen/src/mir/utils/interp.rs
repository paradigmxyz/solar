//! Concrete execution of word-level lowered MIR.
//!
//! [`Machine::run`] executes one function of a lowered module on concrete arguments and memory,
//! following internal calls into the rest of the module, and reports how the run ended, the
//! memory it left behind, the blocks of the function it completed, and whether each value of that
//! function was ever zero and ever nonzero. It is a testing oracle, not
//! part of code generation: the `llm-optimize` pass runs an original function and a candidate
//! replacement on the same inputs and compares what they do.
//!
//! # Model
//!
//! Values are 256-bit words. Typed values keep the clean bits their types promise because every
//! operation computes them as the backend's instructions do: word operations and casts evaluate
//! through [`eval_inst`], so folding and execution share one definition of each opcode. `select`
//! picks an operand, and the phis at the start of a block read their incoming values for the edge
//! taken, all before any of them is assigned.
//!
//! Memory is byte-addressed EVM memory. An access with a nonzero length grows memory to the word
//! containing its last byte, and a zero-length access ignores its offset. `mcopy` behaves as if it
//! copied through a buffer, and `keccak256` hashes the bytes it reads. Bytes that no run has
//! written hold deterministic pseudo-random contents derived from a seed unless the caller set
//! them, so a function reading memory it does not own sees garbage, as it could on chain.
//!
//! Internal calls and tail calls run the callee on the same memory, and a call's result is the
//! value the callee returns. The backend passes further results through a memory buffer of its
//! own, so a callee returning several values is unsupported. `revert`, `returndata`, `stop`, and
//! `invalid` end the whole execution wherever they run, as they end the transaction.
//!
//! # Limits
//!
//! A run has a fuel budget, one unit per operation and per copied, hashed, or returned word, and a
//! call depth. Memory ends at [`MEMORY_LIMIT`], past which the EVM runs out of gas under any block
//! gas limit. Exceeding a limit ends the run with [`Outcome::Limit`]. Operations outside
//! [`supports`] and [`supports_terminator`] end it with [`Outcome::Unsupported`]: storage,
//! calldata, code, the environment, external calls, logs, `msize`, frame addresses, allocations,
//! and every semantic operation. The interpreter relies on the validator only for the existence
//! of the instructions, values, and blocks a function names, and checks the rest as it runs, so a
//! value used before its definition or a phi missing an edge also ends a run as unsupported.

use crate::mir::{
    ArgIdx, BlockId, Callee, Function, FunctionId, InstId, InstKind, MirPhase, Module, Terminator,
    Value, ValueId, utils::eval::eval_inst,
};
use alloy_primitives::{U256, keccak256};
use smallvec::SmallVec;
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::{IndexVec, index_vec},
    map::FxHashMap,
};
use std::{convert::Infallible, ops::ControlFlow};

/// Bytes of memory an execution may use: growing memory to 16 MiB costs about 538 million gas,
/// more than any block holds.
pub(crate) const MEMORY_LIMIT: u64 = 1 << 24;

/// Bytes per memory word.
const WORD_BYTES: u64 = 32;

/// How an execution ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// The function returned these values.
    Return(SmallVec<[U256; 2]>),
    /// Execution reverted with this payload.
    Revert(Vec<u8>),
    /// Execution returned this payload to the transaction with `RETURN`.
    ReturnData(Vec<u8>),
    /// Execution stopped the transaction.
    Stop,
    /// Execution reached `INVALID`.
    Invalid,
    /// Execution exceeded a limit before it ended.
    Limit(Limit),
    /// Execution reached something the interpreter does not model, named here.
    Unsupported(&'static str),
}

/// A bound on one execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Limit {
    /// The fuel budget ran out.
    Fuel,
    /// Calls nested deeper than the call depth.
    Depth,
    /// An access reached past [`MEMORY_LIMIT`].
    Memory,
}

/// The bounds of one execution.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Limits {
    /// Units of work before the run stops: one per operation and per word copied or hashed.
    pub(crate) fuel: u64,
    /// Frames that may be live at once, the function under test included.
    pub(crate) depth: usize,
}

/// What one execution did.
#[derive(Clone, Debug)]
pub(crate) struct Execution {
    /// How it ended.
    pub(crate) outcome: Outcome,
    /// The memory it left.
    pub(crate) memory: Memory,
    /// The fuel it spent.
    pub(crate) fuel: u64,
    /// The blocks of the function under test whose terminators it reached, before any tail call
    /// left it. A block that a call inside it never returns from is not complete.
    pub(crate) visited: DenseBitSet<BlockId>,
    /// For each instruction of the function under test, whether its result was ever zero (bit 0)
    /// and ever nonzero (bit 1).
    pub(crate) outcomes: IndexVec<InstId, u8>,
}

/// Observes every operation an execution runs, for example to price it.
///
/// Each method runs before the operation it reports, and `operand` reads the values of the
/// frame running it.
pub(crate) trait Meter {
    /// Instruction `inst` of `function` runs, including each phi of a block the run enters.
    fn instruction(
        &mut self,
        function: &Function,
        inst: InstId,
        operand: &dyn Fn(ValueId) -> Option<U256>,
    ) {
        let _ = (function, inst, operand);
    }

    /// The terminator of `block` in `function` runs.
    fn terminator(
        &mut self,
        function: &Function,
        block: BlockId,
        operand: &dyn Fn(ValueId) -> Option<U256>,
    ) {
        let _ = (function, block, operand);
    }
}

impl Meter for () {}

/// Returns whether [`Machine::run`] executes an instruction of this kind: word operations and
/// casts, `select`, phis, memory reads, writes, copies and hashes, and calls to functions.
pub(crate) fn supports(kind: &InstKind) -> bool {
    if !kind.op_def().phases.contains(MirPhase::Lowered) {
        return false;
    }
    match kind {
        InstKind::Phi(_)
        | InstKind::Select(..)
        | InstKind::MLoad(_)
        | InstKind::MStore(..)
        | InstKind::MStore8(..)
        | InstKind::MCopy(..)
        | InstKind::Keccak256(..) => true,
        InstKind::ICall { function, .. } => matches!(function, Callee::Function(_)),
        _ => matches!(eval_inst(kind, |_| Ok::<_, Infallible>(U256::ZERO)), Ok(Some(_))),
    }
}

/// Returns whether [`Machine::run`] executes this terminator.
pub(crate) fn supports_terminator(terminator: &Terminator) -> bool {
    !matches!(terminator, Terminator::SelfDestruct { .. } | Terminator::RevertReturndata)
}

/// Mixes a word into a well-distributed one: the SplitMix64 finalizer.
pub(crate) fn mix64(value: u64) -> u64 {
    let mut z = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Byte-addressed EVM memory whose unwritten bytes derive from a seed.
#[derive(Clone, Debug)]
pub(crate) struct Memory {
    seed: u64,
    /// Materialized words by word index.
    words: FxHashMap<u64, [u8; 32]>,
    /// Bytes a run wrote, as a mask per word index.
    written: FxHashMap<u64, u32>,
    /// The size in words, as `MSIZE` reports it.
    size: u64,
}

impl Memory {
    /// Creates memory of `size` words whose contents derive from `seed`.
    pub(crate) fn new(seed: u64, size: u64) -> Self {
        Self { seed, words: FxHashMap::default(), written: FxHashMap::default(), size }
    }

    /// Sets the word at byte `offset` before a run, without counting it as written or growing
    /// memory.
    pub(crate) fn set(&mut self, offset: u64, value: U256) {
        for (index, byte) in value.to_be_bytes::<32>().into_iter().enumerate() {
            self.set_byte(offset.wrapping_add(index as u64), byte);
        }
    }

    /// Returns the size in words.
    pub(crate) fn size(&self) -> u64 {
        self.size
    }

    /// Returns the word at byte `offset`, without growing memory.
    pub(crate) fn word(&self, offset: u64) -> U256 {
        let mut bytes = [0; 32];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = self.byte(offset.wrapping_add(index as u64));
        }
        U256::from_be_bytes(bytes)
    }

    /// Returns the lowest byte address that either memory wrote and where their contents
    /// differ.
    pub(crate) fn first_difference(&self, other: &Self) -> Option<u64> {
        let mut first = None::<u64>;
        for (&word, &mask) in self.written.iter().chain(&other.written) {
            for index in 0..WORD_BYTES {
                let address = word * WORD_BYTES + index;
                if mask & (1 << index) != 0
                    && first.is_none_or(|first| address < first)
                    && self.byte(address) != other.byte(address)
                {
                    first = Some(address);
                }
            }
        }
        first
    }

    /// Returns the lowest byte address this memory wrote and `other` did not.
    pub(crate) fn first_write_outside(&self, other: &Self) -> Option<u64> {
        let mut first = None::<u64>;
        for (&word, &mask) in &self.written {
            let outside = mask & !other.written.get(&word).copied().unwrap_or_default();
            if outside != 0 {
                let address = word * WORD_BYTES + u64::from(outside.trailing_zeros());
                first = Some(first.map_or(address, |first| first.min(address)));
            }
        }
        first
    }

    fn byte(&self, address: u64) -> u8 {
        let (word, index) = (address / WORD_BYTES, (address % WORD_BYTES) as usize);
        self.words.get(&word).map_or_else(|| self.seeded(word)[index], |bytes| bytes[index])
    }

    fn set_byte(&mut self, address: u64, value: u8) {
        let (word, index) = (address / WORD_BYTES, (address % WORD_BYTES) as usize);
        let seeded = self.seeded(word);
        self.words.entry(word).or_insert(seeded)[index] = value;
    }

    fn write_byte(&mut self, address: u64, value: u8) {
        self.set_byte(address, value);
        *self.written.entry(address / WORD_BYTES).or_default() |= 1 << (address % WORD_BYTES);
    }

    fn seeded(&self, word: u64) -> [u8; 32] {
        let mut bytes = [0; 32];
        for (index, chunk) in bytes.as_chunks_mut::<8>().0.iter_mut().enumerate() {
            let lane = mix64(word.wrapping_mul(4).wrapping_add(index as u64));
            *chunk = mix64(self.seed ^ lane).to_be_bytes();
        }
        bytes
    }

    /// Checks an access of `len` bytes at `offset` against the memory limit and grows memory
    /// over it. Returns the offset, or `None` for a zero-length access, which touches nothing.
    fn access(&mut self, offset: U256, len: U256) -> Result<Option<u64>, Limit> {
        if len.is_zero() {
            return Ok(None);
        }
        let limit = U256::from(MEMORY_LIMIT);
        if offset > limit || len > limit || offset + len > limit {
            return Err(Limit::Memory);
        }
        let (offset, len) = (offset.to::<u64>(), len.to::<u64>());
        self.size = self.size.max((offset + len).div_ceil(WORD_BYTES));
        Ok(Some(offset))
    }

    fn load(&mut self, offset: U256) -> Result<U256, Limit> {
        let offset = self.access(offset, U256::from(WORD_BYTES))?.unwrap_or_default();
        Ok(self.word(offset))
    }

    fn store(&mut self, offset: U256, value: U256) -> Result<(), Limit> {
        let offset = self.access(offset, U256::from(WORD_BYTES))?.unwrap_or_default();
        for (index, byte) in value.to_be_bytes::<32>().into_iter().enumerate() {
            self.write_byte(offset + index as u64, byte);
        }
        Ok(())
    }

    fn store8(&mut self, offset: U256, value: U256) -> Result<(), Limit> {
        let offset = self.access(offset, U256::ONE)?.unwrap_or_default();
        self.write_byte(offset, value.byte(0));
        Ok(())
    }

    fn read(&mut self, offset: U256, len: U256) -> Result<Vec<u8>, Limit> {
        let Some(offset) = self.access(offset, len)? else { return Ok(Vec::new()) };
        Ok((0..len.to::<u64>()).map(|index| self.byte(offset + index)).collect())
    }

    fn copy(&mut self, dest: U256, src: U256, len: U256) -> Result<(), Limit> {
        let Some(dest) = self.access(dest, len)? else { return Ok(()) };
        let bytes = self.read(src, len)?;
        for (index, byte) in bytes.into_iter().enumerate() {
            self.write_byte(dest + index as u64, byte);
        }
        Ok(())
    }
}

/// Runs functions of one lowered module, optionally with one function's body replaced.
pub(crate) struct Machine<'a> {
    module: &'a Module,
    replacement: Option<(FunctionId, &'a Function)>,
}

impl<'a> Machine<'a> {
    /// Runs `module`'s functions as they are.
    pub(crate) fn new(module: &'a Module) -> Self {
        Self { module, replacement: None }
    }

    /// Runs `module`'s functions with `body` in place of function `id`.
    pub(crate) fn with_replacement(module: &'a Module, id: FunctionId, body: &'a Function) -> Self {
        Self { module, replacement: Some((id, body)) }
    }

    /// Runs `function` on `args` and `memory` within `limits`, reporting each operation to
    /// `meter`.
    pub(crate) fn run(
        &self,
        function: FunctionId,
        args: &[U256],
        memory: Memory,
        limits: Limits,
        meter: &mut dyn Meter,
    ) -> Execution {
        let mut run = Run {
            machine: self,
            memory,
            limits,
            fuel: 0,
            frames: Vec::new(),
            visited: DenseBitSet::new_empty(0),
            outcomes: IndexVec::new(),
        };
        let ControlFlow::Break(outcome) = run.execute(function, args, meter);
        let Run { memory, fuel, visited, outcomes, .. } = run;
        Execution { outcome, memory, fuel, visited, outcomes }
    }

    fn body(&self, id: FunctionId) -> Option<&'a Function> {
        match self.replacement {
            Some((replaced, body)) if replaced == id => Some(body),
            _ => self.module.functions.get(id),
        }
    }
}

/// The state of one call.
struct Frame<'a> {
    body: &'a Function,
    args: IndexVec<ArgIdx, U256>,
    values: IndexVec<ValueId, Option<U256>>,
    block: BlockId,
    /// The next instruction of `block` to run.
    next: usize,
    /// The caller's value receiving this call's first returned value.
    result: Option<ValueId>,
    /// Whether this frame runs the function under test.
    entry: bool,
}

impl<'a> Frame<'a> {
    fn new(body: &'a Function, args: IndexVec<ArgIdx, U256>, result: Option<ValueId>) -> Self {
        let values = index_vec![None; body.num_values()];
        Self { body, args, values, block: BlockId::ENTRY, next: 0, result, entry: false }
    }

    fn word(&self, value: ValueId) -> Option<U256> {
        if value.index() >= self.body.num_values() {
            return None;
        }
        match self.body.value(value) {
            Value::Immediate(immediate) => immediate.as_u256(),
            Value::Arg(index) => self.args.get(*index).copied(),
            Value::Inst(_) => self.values[value],
            Value::Undef(_) | Value::Error(_) => None,
        }
    }

    fn read(&self, value: ValueId) -> ControlFlow<Outcome, U256> {
        match self.word(value) {
            Some(word) => ControlFlow::Continue(word),
            None => ControlFlow::Break(Outcome::Unsupported("value without a definition")),
        }
    }

    fn read_all(&self, values: &[ValueId]) -> ControlFlow<Outcome, SmallVec<[U256; 4]>> {
        let mut words = SmallVec::with_capacity(values.len());
        for &value in values {
            words.push(self.read(value)?);
        }
        ControlFlow::Continue(words)
    }
}

/// One execution in progress.
struct Run<'m, 'a> {
    machine: &'m Machine<'a>,
    memory: Memory,
    limits: Limits,
    fuel: u64,
    frames: Vec<Frame<'a>>,
    visited: DenseBitSet<BlockId>,
    outcomes: IndexVec<InstId, u8>,
}

impl<'a> Run<'_, 'a> {
    fn execute(
        &mut self,
        function: FunctionId,
        args: &[U256],
        meter: &mut dyn Meter,
    ) -> ControlFlow<Outcome, Infallible> {
        let Some(body) = self.machine.body(function) else {
            return ControlFlow::Break(Outcome::Unsupported("call to an unknown function"));
        };
        self.visited = DenseBitSet::new_empty(body.blocks.len());
        self.outcomes = index_vec![0; body.num_insts()];
        self.call(body, args.iter().copied().collect(), None)?;
        self.frame().entry = true;
        loop {
            let frame = self.frame();
            let (body, block, next) = (frame.body, frame.block, frame.next);
            match body.blocks[block].instructions.get(next) {
                Some(&inst) => {
                    self.frame().next += 1;
                    self.instruction(body, inst, meter)?;
                }
                None => self.terminator(body, block, meter)?,
            }
        }
    }

    fn frame(&mut self) -> &mut Frame<'a> {
        self.frames.last_mut().expect("a run always has a frame")
    }

    fn burn(&mut self, units: u64) -> ControlFlow<Outcome> {
        self.fuel = self.fuel.saturating_add(units);
        if self.fuel > self.limits.fuel {
            return ControlFlow::Break(Outcome::Limit(Limit::Fuel));
        }
        ControlFlow::Continue(())
    }

    /// Burns a unit per word of an access `len` bytes long, once the access fits in memory.
    fn burn_words(&mut self, len: U256) -> ControlFlow<Outcome> {
        if len > U256::from(MEMORY_LIMIT) {
            return ControlFlow::Break(Outcome::Limit(Limit::Memory));
        }
        self.burn(len.to::<u64>().div_ceil(WORD_BYTES))
    }

    /// Pushes a frame running `body` on `args`, returning its first value into `result`.
    fn call(
        &mut self,
        body: &'a Function,
        args: IndexVec<ArgIdx, U256>,
        result: Option<ValueId>,
    ) -> ControlFlow<Outcome> {
        if self.frames.len() >= self.limits.depth {
            return ControlFlow::Break(Outcome::Limit(Limit::Depth));
        }
        if args.len() != body.params.len() || body.arg_indices().count() != body.params.len() {
            return ControlFlow::Break(Outcome::Unsupported("call with mismatched arguments"));
        }
        if body.blocks.is_empty() {
            return ControlFlow::Break(Outcome::Unsupported("function without blocks"));
        }
        self.frames.push(Frame::new(body, args, result));
        ControlFlow::Continue(())
    }

    fn instruction(
        &mut self,
        body: &'a Function,
        inst: InstId,
        meter: &mut dyn Meter,
    ) -> ControlFlow<Outcome> {
        self.burn(1)?;
        let instruction = body.inst(inst);
        let mnemonic = instruction.kind.op_def().mnemonic;
        // `supports` in full costs an evaluation per step; the arms below and `eval_inst` decide
        // the rest the same way.
        if !instruction.kind.op_def().phases.contains(MirPhase::Lowered) {
            return ControlFlow::Break(Outcome::Unsupported(mnemonic));
        }
        let frame = self.frames.last().expect("a run always has a frame");
        let entry = frame.entry;
        meter.instruction(body, inst, &|value| frame.word(value));
        let result = match &instruction.kind {
            InstKind::Phi(_) => {
                return ControlFlow::Break(Outcome::Unsupported("phi after other instructions"));
            }
            InstKind::Select(condition, if_true, if_false) => {
                let condition = frame.read(*condition)?;
                frame.read(if condition.is_zero() { *if_false } else { *if_true })?
            }
            &InstKind::MLoad(offset) => {
                let offset = frame.read(offset)?;
                limit(self.memory.load(offset))?
            }
            &InstKind::MStore(offset, value) => {
                let (offset, value) = (frame.read(offset)?, frame.read(value)?);
                return limit(self.memory.store(offset, value));
            }
            &InstKind::MStore8(offset, value) => {
                let (offset, value) = (frame.read(offset)?, frame.read(value)?);
                return limit(self.memory.store8(offset, value));
            }
            &InstKind::MCopy(dest, src, len) => {
                let (dest, src, len) = (frame.read(dest)?, frame.read(src)?, frame.read(len)?);
                self.burn_words(len)?;
                return limit(self.memory.copy(dest, src, len));
            }
            &InstKind::Keccak256(offset, len) => {
                let (offset, len) = (frame.read(offset)?, frame.read(len)?);
                self.burn_words(len)?;
                let bytes = limit(self.memory.read(offset, len))?;
                U256::from_be_bytes(keccak256(bytes).0)
            }
            InstKind::ICall { function: Callee::Function(callee), args } => {
                let args = frame.read_all(args)?.into_iter().collect();
                let Some(callee) = self.machine.body(*callee) else {
                    return ControlFlow::Break(Outcome::Unsupported("call to an unknown function"));
                };
                return self.call(callee, args, instruction.result());
            }
            InstKind::ICall { .. } => return ControlFlow::Break(Outcome::Unsupported(mnemonic)),
            kind => match eval_inst(kind, |value| frame.word(value).ok_or(())) {
                Ok(Some(word)) => word,
                Ok(None) => return ControlFlow::Break(Outcome::Unsupported(mnemonic)),
                Err(()) => {
                    return ControlFlow::Break(Outcome::Unsupported("value without a definition"));
                }
            },
        };
        let Some(value) = instruction.result() else {
            return ControlFlow::Break(Outcome::Unsupported("result without a value"));
        };
        if entry {
            self.outcomes[inst] |= if result.is_zero() { 1 } else { 2 };
        }
        self.frame().values[value] = Some(result);
        ControlFlow::Continue(())
    }

    fn terminator(
        &mut self,
        body: &'a Function,
        block: BlockId,
        meter: &mut dyn Meter,
    ) -> ControlFlow<Outcome> {
        self.burn(1)?;
        let Some(terminator) = &body.blocks[block].terminator else {
            return ControlFlow::Break(Outcome::Unsupported("block without a terminator"));
        };
        let frame = self.frames.last().expect("a run always has a frame");
        meter.terminator(body, block, &|value| frame.word(value));
        if frame.entry {
            self.visited.insert(block);
        }
        match terminator {
            &Terminator::Jump(target) => self.enter(target, meter),
            &Terminator::Branch { condition, then_block, else_block } => {
                let condition = frame.read(condition)?;
                self.enter(if condition.is_zero() { else_block } else { then_block }, meter)
            }
            Terminator::Switch { value, default, cases } => {
                let value = frame.read(*value)?;
                let mut target = *default;
                for &(case, block) in cases {
                    if frame.read(case)? == value {
                        target = block;
                        break;
                    }
                }
                self.enter(target, meter)
            }
            Terminator::Return { values } => {
                let values = frame.read_all(values)?.into_iter().collect::<SmallVec<_>>();
                let frame = self.frames.pop().expect("a run always has a frame");
                let Some(caller) = self.frames.last_mut() else {
                    return ControlFlow::Break(Outcome::Return(values));
                };
                // Further results pass through a memory buffer the backend owns.
                if values.len() > 1 {
                    return ControlFlow::Break(Outcome::Unsupported(
                        "call returning several values",
                    ));
                }
                if let Some(result) = frame.result {
                    let Some(&value) = values.first() else {
                        return ControlFlow::Break(Outcome::Unsupported("call without a result"));
                    };
                    caller.values[result] = Some(value);
                }
                ControlFlow::Continue(())
            }
            &Terminator::Revert { offset, size } => {
                let (offset, size) = (frame.read(offset)?, frame.read(size)?);
                self.burn_words(size)?;
                ControlFlow::Break(Outcome::Revert(limit(self.memory.read(offset, size))?))
            }
            &Terminator::ReturnData { offset, size } => {
                let (offset, size) = (frame.read(offset)?, frame.read(size)?);
                self.burn_words(size)?;
                ControlFlow::Break(Outcome::ReturnData(limit(self.memory.read(offset, size))?))
            }
            Terminator::Stop => ControlFlow::Break(Outcome::Stop),
            Terminator::Invalid => ControlFlow::Break(Outcome::Invalid),
            Terminator::TailCall { function, args } => {
                let args = frame.read_all(args)?.into_iter().collect();
                let Some(callee) = self.machine.body(*function) else {
                    return ControlFlow::Break(Outcome::Unsupported("call to an unknown function"));
                };
                let frame = self.frames.pop().expect("a run always has a frame");
                self.call(callee, args, frame.result)
            }
            Terminator::SelfDestruct { .. } => {
                ControlFlow::Break(Outcome::Unsupported("selfdestruct"))
            }
            Terminator::RevertReturndata => {
                ControlFlow::Break(Outcome::Unsupported("revert_returndata"))
            }
        }
    }

    /// Enters `target` from the current block, assigning its phis for the edge taken.
    fn enter(&mut self, target: BlockId, meter: &mut dyn Meter) -> ControlFlow<Outcome> {
        let frame = self.frames.last().expect("a run always has a frame");
        let (body, from) = (frame.body, frame.block);
        let Some(block) = body.blocks.get(target) else {
            return ControlFlow::Break(Outcome::Unsupported("jump to an unknown block"));
        };
        let mut incoming = SmallVec::<[(InstId, ValueId, U256); 8]>::new();
        for &inst in &block.instructions {
            let instruction = body.inst(inst);
            let InstKind::Phi(inputs) = &instruction.kind else { break };
            let (Some(&(_, value)), Some(result)) =
                (inputs.iter().find(|&&(block, _)| block == from), instruction.result())
            else {
                return ControlFlow::Break(Outcome::Unsupported("phi without an input"));
            };
            meter.instruction(body, inst, &|value| frame.word(value));
            incoming.push((inst, result, frame.read(value)?));
        }
        self.burn(incoming.len() as u64)?;
        let frame = self.frame();
        frame.next = incoming.len();
        let entry = frame.entry;
        for &(_, result, value) in &incoming {
            frame.values[result] = Some(value);
        }
        frame.block = target;
        if entry {
            for (inst, _, value) in incoming {
                self.outcomes[inst] |= if value.is_zero() { 1 } else { 2 };
            }
        }
        ControlFlow::Continue(())
    }
}

/// Ends a run whose memory access exceeded its limit.
fn limit<T>(result: Result<T, Limit>) -> ControlFlow<Outcome, T> {
    match result {
        Ok(value) => ControlFlow::Continue(value),
        Err(limit) => ControlFlow::Break(Outcome::Limit(limit)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::parser::parse_module;
    use solar_interface::{ColorChoice, Session};

    const LIMITS: Limits = Limits { fuel: 10_000, depth: 16 };

    const MODULE: &str = "@module Interp
@phase lowered
fn @sum(arg0: i256) -> i256 {
  bb0:
    jump bb1
  bb1:
    v0 = phi [bb0: 0], [bb2: v3]
    v1 = phi [bb0: 0], [bb2: v4]
    v2 = lt v0, arg0
    jumpi v2, bb2, bb3
  bb2:
    v3 = add v0, 1
    v4 = add v1, v3
    jump bb1
  bb3:
    v5 = select v2, v0, v1
    ret v5
}

fn @swap(arg0: i256) -> i256 {
  bb0:
    jump bb1
  bb1:
    v0 = phi [bb0: 1], [bb2: v1]
    v1 = phi [bb0: 2], [bb2: v0]
    v2 = phi [bb0: 0], [bb2: v3]
    v4 = lt v2, arg0
    jumpi v4, bb2, bb3
  bb2:
    v3 = add v2, 1
    jump bb1
  bb3:
    v5 = mul v0, 10
    v6 = add v5, v1
    ret v6
}

fn @memory(arg0: i256) -> i256 {
  bb0:
    mstore 128, arg0
    mstore8 160, 0x1234
    mcopy 129, 128, 32
    v0 = mload 128
    v1 = keccak256 0x100000000000000000000, 0
    mcopy 0xffffffffffffffffffff, 0xffffffffffffffffffffff, 0
    v2 = xor v0, v1
    ret v2
}

fn @double(arg0: i256) -> i256 {
  bb0:
    v0 = add arg0, arg0
    ret v0
}

fn @fail(arg0: i256) {
  bb0:
    mstore 0, arg0
    revert 0, 32
}

fn @forward(arg0: i256) -> i256 {
  bb0:
    tail_call @double, arg0
}

fn @calls(arg0: i256) -> i256 {
  bb0:
    v0 = icall @double, arg0
    v1 = icall @forward, v0
    v2 = gt v1, 100
    jumpi v2, bb1, bb2
  bb1:
    icall @fail, v1
    stop
  bb2:
    ret v1
}

fn @spin(arg0: i256) {
  bb0:
    jump bb1
  bb1:
    jump bb1
}

fn @deep(arg0: i256) -> i256 {
  bb0:
    v0 = icall @deep, arg0
    ret v0
}

fn @far(arg0: i256) -> i256 {
  bb0:
    v0 = mload arg0
    ret v0
}

fn @storage(arg0: i256) -> i256 {
  bb0:
    v0 = sload arg0
    ret v0
}
";

    /// Counts the operations an execution reports.
    #[derive(Default)]
    struct Counter(u64);

    impl Meter for Counter {
        fn instruction(&mut self, _: &Function, _: InstId, _: &dyn Fn(ValueId) -> Option<U256>) {
            self.0 += 1;
        }

        fn terminator(&mut self, _: &Function, _: BlockId, _: &dyn Fn(ValueId) -> Option<U256>) {
            self.0 += 1;
        }
    }

    fn with_module(f: impl FnOnce(&Module, &dyn Fn(&str) -> FunctionId) + Send) {
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        sess.enter(|| {
            let module = parse_module(&sess, MODULE).unwrap();
            let id = |name: &str| {
                module
                    .iter_functions()
                    .find(|(_, function)| function.name.symbol.as_str() == name)
                    .map(|(id, _)| id)
                    .unwrap()
            };
            f(&module, &id);
        });
    }

    fn run(module: &Module, function: FunctionId, arg: U256) -> Execution {
        Machine::new(module).run(function, &[arg], Memory::new(7, 0), LIMITS, &mut ())
    }

    fn returned(value: u64) -> Outcome {
        Outcome::Return([U256::from(value)].into_iter().collect())
    }

    #[test]
    fn words_and_merges() {
        with_module(|module, id| {
            assert_eq!(run(module, id("sum"), U256::from(10)).outcome, returned(55));
            // The phis of a block read their inputs before any of them is assigned.
            assert_eq!(run(module, id("swap"), U256::from(3)).outcome, returned(21));
            let execution = run(module, id("sum"), U256::ZERO);
            let blocks = execution.visited.iter().map(BlockId::index).collect::<Vec<_>>();
            assert_eq!(blocks, [0, 1, 3]);
            // `v2 = lt v0, arg0` only ever came out false, and both ways over ten iterations.
            let compare = InstId::from_usize(2);
            assert_eq!(execution.outcomes[compare], 1);
            assert_eq!(run(module, id("sum"), U256::from(10)).outcomes[compare], 3);
            // A block counts once its terminator runs: the call to `@fail` never returns.
            let visited = run(module, id("calls"), U256::from(30)).visited;
            assert_eq!(visited.iter().map(BlockId::index).collect::<Vec<_>>(), [0]);
        });
    }

    #[test]
    fn memory_rules() {
        with_module(|module, id| {
            let arg = U256::from_be_bytes(std::array::from_fn::<u8, 32, _>(|i| i as u8 + 1));
            let execution = run(module, id("memory"), arg);
            // The overlapping copy moves every byte one place up, as if through a buffer.
            let mut shifted = [0; 32];
            shifted[0] = 1;
            shifted[1..].copy_from_slice(&arg.to_be_bytes::<32>()[..31]);
            let empty = U256::from_be_bytes(keccak256([]).0);
            let expected = U256::from_be_bytes(shifted) ^ empty;
            assert_eq!(execution.outcome, Outcome::Return([expected].into_iter().collect()),);
            // `mstore8` and the copy reach byte 160; zero-length accesses reach nothing.
            assert_eq!(execution.memory.size(), 6);
            assert_eq!(execution.memory.word(129), arg);
        });
    }

    #[test]
    fn seeded_memory() {
        let mut memory = Memory::new(1, 0);
        let fresh = memory.clone();
        assert_eq!(memory.word(64), Memory::new(1, 0).word(64));
        assert_ne!(memory.word(64), Memory::new(2, 0).word(64));
        memory.set(64, U256::from(0x80));
        assert_eq!(memory.word(64), U256::from(0x80));
        assert_eq!(memory.first_difference(&fresh), None);

        let mut written = fresh.clone();
        written.store8(U256::from(70), U256::from(fresh.byte(70))).unwrap();
        assert_eq!(written.first_difference(&fresh), None);
        written.store8(U256::from(90), U256::from(fresh.byte(90) ^ 1)).unwrap();
        written.store8(U256::from(80), U256::from(fresh.byte(80) ^ 1)).unwrap();
        assert_eq!(written.first_difference(&fresh), Some(80));
        assert_eq!(fresh.first_difference(&written), Some(80));
        assert_eq!(written.first_write_outside(&fresh), Some(70));
        assert_eq!(fresh.first_write_outside(&written), None);
    }

    #[test]
    fn calls_and_halts() {
        with_module(|module, id| {
            assert_eq!(run(module, id("calls"), U256::from(5)).outcome, returned(20));
            let payload = U256::from(120).to_be_bytes::<32>().to_vec();
            assert_eq!(run(module, id("calls"), U256::from(30)).outcome, Outcome::Revert(payload));

            // The meter sees every unit of fuel a run without copies burns.
            let mut counter = Counter::default();
            let execution = Machine::new(module).run(
                id("calls"),
                &[U256::from(5)],
                Memory::new(7, 0),
                LIMITS,
                &mut counter,
            );
            assert_eq!(counter.0, execution.fuel);

            // A replacement runs in place of its original, including through calls.
            let module_double = module.function(id("double"));
            let replacement = Machine::with_replacement(module, id("double"), module_double);
            let execution =
                replacement.run(id("calls"), &[U256::from(5)], Memory::new(7, 0), LIMITS, &mut ());
            assert_eq!(execution.outcome, returned(20));
        });
    }

    #[test]
    fn supported_operations() {
        let mut supported = Vec::new();
        for &name in InstKind::MNEMONICS {
            if let Some((arity, build)) = InstKind::operand_only(name) {
                let operands = (0..arity).map(ValueId::from_usize).collect::<Vec<_>>();
                if supports(&build(&operands)) {
                    supported.push(name);
                }
            }
        }
        snapbox::assert_data_eq!(
            supported.join(" "),
            snapbox::str![
                "zext inttoptr add sub mul div sdiv mod smod exp addmod mulmod and or xor not clz shl shr sar byte lt gt slt sgt eq ne mload mstore mstore8 mcopy keccak256 select signextend"
            ]
        );
        assert!(supports_terminator(&Terminator::Stop));
        assert!(!supports_terminator(&Terminator::RevertReturndata));
    }

    #[test]
    fn limits() {
        with_module(|module, id| {
            assert_eq!(run(module, id("spin"), U256::ZERO).outcome, Outcome::Limit(Limit::Fuel));
            assert_eq!(run(module, id("deep"), U256::ZERO).outcome, Outcome::Limit(Limit::Depth));
            let last_word = U256::from(MEMORY_LIMIT - 32);
            assert!(matches!(run(module, id("far"), last_word).outcome, Outcome::Return(_)));
            let past_limit = last_word + U256::ONE;
            let outcome = run(module, id("far"), past_limit).outcome;
            assert_eq!(outcome, Outcome::Limit(Limit::Memory));
            assert_eq!(run(module, id("far"), U256::MAX).outcome, Outcome::Limit(Limit::Memory));
            let outcome = run(module, id("storage"), U256::ZERO).outcome;
            assert_eq!(outcome, Outcome::Unsupported("sload"));
        });
    }
}
