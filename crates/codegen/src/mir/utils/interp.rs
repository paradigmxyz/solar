//! Concrete execution of word-level lowered MIR.
//!
//! [`Machine::run`] executes one function of a lowered module on concrete arguments and memory,
//! following internal calls into the rest of the module, and reports how the run ended. With a
//! [`Host`], the run also reads and writes persistent and transient storage, logs events, and
//! reads the context values the host answers, and reports what it wrote and logged. It is a
//! testing oracle, not part of code generation.
//!
//! [`Machine::transact`] executes a whole transaction instead: the module's dispatch entry on
//! calldata, with zeroed memory and a [`Host`] answering what the contract reads from its context,
//! such as the caller or a storage slot's value before the transaction. The UI test runner checks
//! it against an EVM running the compiled bytecode.
//!
//! # Model
//!
//! Each instruction runs by the [`Semantics`] its operation schema row declares, so the
//! interpreter lists no operations of its own. Values are 256-bit words. Typed values keep the
//! clean bits their types promise because every operation computes them as the backend's
//! instructions do: word operations and casts evaluate through [`eval_semantics`], which
//! computes opcodes with the opcode table's word semantics, so folding and execution share one
//! definition of each opcode. `select` picks an operand, and the phis at the start of a block
//! read their incoming values for the edge taken, all before any of them is assigned.
//!
//! Memory is byte-addressed EVM memory, on which the memory opcodes run: `MLOAD`, `MSTORE`,
//! `MSTORE8`, `MCOPY`, and `KECCAK256`. An access with a nonzero length grows memory to the word
//! containing its last byte, and a zero-length access ignores its offset. `mcopy` behaves as if it
//! copied through a buffer, and `keccak256` hashes the bytes it reads. Memory that nothing wrote
//! is zero, unless the caller set it before the run.
//!
//! Internal calls and tail calls run the callee on the same memory, and a call's result is the
//! value the callee returns. The backend passes further results through a buffer of its own that
//! the word at `0x20` points to, so a function run alone may not call a callee returning several
//! values. A transaction places that buffer at the top of memory, where nothing else reaches,
//! and reuses it: callers read the results right after the call. `revert`, `returndata`, `stop`,
//! and `invalid` end the whole execution wherever they run, as they end the transaction.
//!
//! A transaction follows the backend's conventions for external entries: the free memory pointer
//! starts at the heap start the host reports, and argument `i` of an external entry is the calldata
//! word at `4 + 32 * i`. A call takes the heap frame the host reports for its callee at the free
//! memory pointer, and releases it on return when the backend does. It also reads calldata,
//! persistent and transient storage, and the context values its host provides, and records the logs
//! it emits. It makes no calls, so its return data is always empty. Returning from the dispatch
//! entry stops the transaction, as the backend's `STOP` does.
//!
//! # Limits
//!
//! A run has a fuel budget, one unit per operation and per copied, hashed, or returned word, and a
//! call depth. Memory ends at [`MEMORY_LIMIT`], past which the EVM runs out of gas under any block
//! gas limit. Exceeding a limit ends the run with [`Outcome::Limit`]. Operations the interpreter
//! does not model end it with [`Outcome::Unsupported`]: opcodes on storage, calldata, code, the
//! environment, external calls, logs, and `msize`, allocations, and operations that declare no
//! semantics, such as frame addresses and every semantic operation. A run with a host also runs
//! the storage, transient storage, and log opcodes and the context reads its host answers. A
//! transaction also runs the calldata and
//! return data opcodes and places the allocations the backend would place, but not `gas`, calls,
//! or contract creation. The interpreter relies on the validator only for the existence of the
//! instructions, values, and blocks a function names, and checks the rest as it runs, so a value
//! used before its definition or a phi missing an edge also ends a run as unsupported.

use crate::{
    backend::evm::op,
    mir::{
        AllocationKind, ArgIdx, BlockId, Callee, DataRef, Function, FunctionId, InstId, InstKind,
        MirPhase, Module, Semantics, Terminator, Value, ValueId, memory::EvmMemoryLayout,
        utils::eval::eval_semantics,
    },
};
use alloy_primitives::{U256, keccak256};
use smallvec::SmallVec;
use solar_config::EvmVersion;
use solar_data_structures::{
    index::{IndexVec, index_vec},
    map::FxHashMap,
};
use std::{convert::Infallible, ops::ControlFlow};

/// Bytes of memory an execution may use: growing memory to 16 MiB costs about 538 million gas,
/// more than any block holds.
pub(crate) const MEMORY_LIMIT: u64 = 1 << 24;

/// Bytes per memory word.
const WORD_BYTES: u64 = 32;

/// The opcodes a run executes on its memory, besides the pure opcodes it evaluates.
const MEMORY_OPCODES: [u8; 5] = [op::MLOAD, op::MSTORE, op::MSTORE8, op::MCOPY, op::KECCAK256];

/// The opcodes whose values a run asks its [`Host`] for.
pub(crate) const HOST_OPCODES: [u8; 21] = [
    op::ADDRESS,
    op::BALANCE,
    op::ORIGIN,
    op::CALLER,
    op::CALLVALUE,
    op::CODESIZE,
    op::GASPRICE,
    op::EXTCODESIZE,
    op::EXTCODEHASH,
    op::BLOCKHASH,
    op::COINBASE,
    op::TIMESTAMP,
    op::NUMBER,
    op::PREVRANDAO,
    op::GASLIMIT,
    op::CHAINID,
    op::SELFBALANCE,
    op::BASEFEE,
    op::BLOBHASH,
    op::BLOBBASEFEE,
    op::SLOTNUM,
];

/// Where a transaction places the multi-return buffer the backend keeps in memory of its own:
/// three quarters of [`MEMORY_LIMIT`], which no execution within a block's gas reaches.
const MULTI_RETURN_BUFFER: u64 = MEMORY_LIMIT / 4 * 3;

/// Where a transaction starts placing the allocations the backend places itself, below the
/// multi-return buffer.
const ALLOCATION_REGION: u64 = MEMORY_LIMIT / 2;

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
    /// Execution reached `INVALID`, or another exceptional halt that consumes all gas and
    /// returns nothing.
    Invalid,
    /// Execution exceeded a limit before it ended.
    Limit(Limit),
    /// Execution reached something the interpreter does not model, named here.
    Unsupported(&'static str),
}

/// A bound on one execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Limit {
    /// The fuel budget, or the budget of the run's meter, ran out.
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
    /// Frames that may be live at once, the called function's included.
    pub(crate) depth: usize,
}

/// What one execution did.
#[derive(Clone, Debug)]
pub(crate) struct Execution {
    /// How it ended.
    pub(crate) outcome: Outcome,
    /// What it did outside memory, which only a run with a [`Host`] can do.
    pub(crate) effects: Effects,
}

/// What an execution did outside memory.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Effects {
    /// The persistent storage slots it wrote, with their final values.
    pub(crate) storage: FxHashMap<U256, U256>,
    /// The transient storage slots it wrote, with their final values.
    pub(crate) transient: FxHashMap<U256, U256>,
    /// The events it logged, in order.
    pub(crate) logs: Vec<Log>,
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

    /// The instruction [`Self::instruction`] just reported produced `value`. A call's result is
    /// not reported: the callee's return delivers it.
    fn result(&mut self, value: U256) {
        let _ = value;
    }
}

impl Meter for () {}

/// Answers what a run reads from its context and cannot know itself.
pub trait Host {
    /// Returns what `opcode` reads for these operands from the transaction's context or the
    /// chain's state, such as `CALLER`, `TIMESTAMP`, or `BALANCE`, or `None` when the host does
    /// not model it.
    fn read(&mut self, opcode: u8, operands: &[U256]) -> Option<U256>;

    /// Returns the value a persistent storage slot of the running contract holds when the run
    /// starts, which for a transaction is its value before the transaction.
    fn storage(&mut self, slot: U256) -> U256;

    /// Returns the value a transient storage slot holds when the run starts: zero for a
    /// transaction, which starts with empty transient storage.
    fn transient(&mut self, slot: U256) -> U256 {
        let _ = slot;
        U256::ZERO
    }

    /// Returns where the free memory pointer starts: the heap start the backend's layout chose,
    /// which programs can observe only by exposing an address.
    fn free_memory_start(&mut self) -> U256 {
        U256::from(EvmMemoryLayout::HEAP_START)
    }

    /// Returns the frame the backend takes from the heap on every call to `function`, which
    /// programs can observe only through the addresses of their later allocations.
    fn heap_frame(&mut self, function: &str) -> Option<HeapFrame> {
        let _ = function;
        None
    }
}

/// A frame the backend takes from the heap, at the free memory pointer, on every call to a
/// function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeapFrame {
    /// Bytes the frame takes above the free memory pointer.
    pub size: u64,
    /// Whether the caller moves the free memory pointer back to the frame's base afterwards.
    pub restores_free_memory: bool,
}

/// An event a transaction logged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Log {
    /// The topics, in order.
    pub topics: Vec<U256>,
    /// The data.
    pub data: Vec<u8>,
}

/// What one transaction did.
pub(crate) struct TransactionExecution {
    /// How it ended. Returning from the dispatch entry is a [`Outcome::Stop`].
    pub(crate) outcome: Outcome,
    /// What it did to storage and its logs.
    pub(crate) effects: Effects,
}

/// Byte-addressed EVM memory, zero where nothing wrote.
#[derive(Clone, Debug)]
pub(crate) struct Memory {
    /// Materialized words by word index.
    words: FxHashMap<u64, [u8; 32]>,
    /// The size in words, as `MSIZE` reports it.
    size: u64,
}

impl Memory {
    /// Creates empty memory whose bytes are zero, as a transaction starts with.
    pub(crate) fn zeroed() -> Self {
        Self { words: FxHashMap::default(), size: 0 }
    }

    /// Sets the word at byte `offset` before a run, without counting it as written or growing
    /// memory.
    pub(crate) fn set(&mut self, offset: u64, value: U256) {
        for (index, byte) in value.to_be_bytes::<32>().into_iter().enumerate() {
            self.set_byte(offset.wrapping_add(index as u64), byte);
        }
    }

    /// Returns the word at byte `offset`, without growing memory.
    pub(crate) fn word(&self, offset: u64) -> U256 {
        let mut bytes = [0; 32];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = self.byte(offset.wrapping_add(index as u64));
        }
        U256::from_be_bytes(bytes)
    }

    fn byte(&self, address: u64) -> u8 {
        let (word, index) = (address / WORD_BYTES, (address % WORD_BYTES) as usize);
        self.words.get(&word).map_or(0, |bytes| bytes[index])
    }

    fn set_byte(&mut self, address: u64, value: u8) {
        let (word, index) = (address / WORD_BYTES, (address % WORD_BYTES) as usize);
        self.words.entry(word).or_insert([0; 32])[index] = value;
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
            self.set_byte(offset + index as u64, byte);
        }
        Ok(())
    }

    fn store8(&mut self, offset: U256, value: U256) -> Result<(), Limit> {
        let offset = self.access(offset, U256::ONE)?.unwrap_or_default();
        self.set_byte(offset, value.byte(0));
        Ok(())
    }

    fn read(&mut self, offset: U256, len: U256) -> Result<Vec<u8>, Limit> {
        let Some(offset) = self.access(offset, len)? else { return Ok(Vec::new()) };
        Ok((0..len.to::<u64>()).map(|index| self.byte(offset + index)).collect())
    }

    /// Writes `bytes` at `offset`, growing memory over them.
    fn write(&mut self, offset: U256, bytes: &[u8]) -> Result<(), Limit> {
        let Some(offset) = self.access(offset, U256::from(bytes.len()))? else { return Ok(()) };
        for (index, &byte) in bytes.iter().enumerate() {
            self.set_byte(offset + index as u64, byte);
        }
        Ok(())
    }

    fn copy(&mut self, dest: U256, src: U256, len: U256) -> Result<(), Limit> {
        let Some(dest) = self.access(dest, len)? else { return Ok(()) };
        let bytes = self.read(src, len)?;
        for (index, byte) in bytes.into_iter().enumerate() {
            self.set_byte(dest + index as u64, byte);
        }
        Ok(())
    }
}

/// Runs functions of one lowered module.
pub(crate) struct Machine<'a> {
    module: &'a Module,
}

impl<'a> Machine<'a> {
    /// Runs `module`'s functions as they are.
    pub(crate) fn new(module: &'a Module) -> Self {
        Self { module }
    }

    /// Runs `function` on `args` and `memory` within `limits`, reporting each operation to
    /// `meter`. With a `host`, the run also reads and writes storage and transient storage, logs
    /// events, and reads the context values the host answers.
    pub(crate) fn run(
        &self,
        function: FunctionId,
        args: &[U256],
        memory: Memory,
        host: Option<&mut dyn Host>,
        limits: Limits,
        meter: &mut dyn Meter,
    ) -> Execution {
        let mut run = Run {
            machine: self,
            memory,
            limits,
            fuel: 0,
            frames: Vec::new(),
            world: host.map(|host| World { host, effects: Effects::default() }),
            transaction: None,
        };
        let ControlFlow::Break(outcome) = run.execute(function, args, meter);
        let effects = run.world.map(|world| world.effects).unwrap_or_default();
        Execution { outcome, effects }
    }

    /// Runs the module's dispatch entry as a transaction with `calldata` on `evm_version`, asking
    /// `host` for its context, within `limits`, reporting each operation to `meter`.
    pub(crate) fn transact(
        &self,
        calldata: &[u8],
        host: &mut dyn Host,
        evm_version: EvmVersion,
        limits: Limits,
        meter: &mut dyn Meter,
    ) -> TransactionExecution {
        let Some(entry) = self.module.dispatch_entry() else {
            return TransactionExecution {
                outcome: Outcome::Unsupported("module without a dispatch entry"),
                effects: Effects::default(),
            };
        };
        // mstore 0x40, heap start
        let mut memory = Memory::zeroed();
        memory.set(EvmMemoryLayout::FMP_SLOT, host.free_memory_start());
        let mut run = Run {
            machine: self,
            memory,
            limits,
            fuel: 0,
            frames: Vec::new(),
            world: Some(World { host, effects: Effects::default() }),
            transaction: Some(Transaction {
                calldata,
                evm_version,
                allocations: ALLOCATION_REGION,
                heap_frames: FxHashMap::default(),
            }),
        };
        let ControlFlow::Break(outcome) = run.execute(entry, &[], meter);
        let outcome = match outcome {
            // return [] => stop
            Outcome::Return(values) if values.is_empty() => Outcome::Stop,
            Outcome::Return(_) => Outcome::Unsupported("dispatch entry returning values"),
            outcome => outcome,
        };
        let world = run.world.expect("a transaction keeps its world");
        TransactionExecution { outcome, effects: world.effects }
    }

    fn body(&self, id: FunctionId) -> Option<&'a Function> {
        self.module.functions.get(id)
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
    /// Where the free memory pointer returns to when this call returns, if the backend takes a
    /// heap frame for the call and releases it.
    heap_frame_base: Option<U256>,
}

impl<'a> Frame<'a> {
    fn new(body: &'a Function, args: IndexVec<ArgIdx, U256>, result: Option<ValueId>) -> Self {
        let values = index_vec![None; body.num_values()];
        Self { body, args, values, block: BlockId::ENTRY, next: 0, result, heap_frame_base: None }
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

/// What a run with a host reads and changes outside memory.
struct World<'t> {
    host: &'t mut dyn Host,
    effects: Effects,
}

/// The context of a run that executes a whole transaction.
struct Transaction<'t> {
    calldata: &'t [u8],
    /// The EVM version the module targets, which decides what `revert` does.
    evm_version: EvmVersion,
    /// The next free byte of the region holding the allocations the backend places itself.
    allocations: u64,
    /// The heap frame of each called function, as the host reports it.
    heap_frames: FxHashMap<FunctionId, Option<HeapFrame>>,
}

/// One execution in progress.
struct Run<'m, 'a, 't> {
    machine: &'m Machine<'a>,
    memory: Memory,
    limits: Limits,
    fuel: u64,
    frames: Vec<Frame<'a>>,
    /// Storage, logs, and context, when the run has a host.
    world: Option<World<'t>>,
    /// The transaction a run of the dispatch entry executes, which always has a world.
    transaction: Option<Transaction<'t>>,
}

impl<'a> Run<'_, 'a, '_> {
    fn execute(
        &mut self,
        function: FunctionId,
        args: &[U256],
        meter: &mut dyn Meter,
    ) -> ControlFlow<Outcome, Infallible> {
        let Some(body) = self.machine.body(function) else {
            return ControlFlow::Break(Outcome::Unsupported("call to an unknown function"));
        };
        self.call(body, args.iter().copied().collect(), None)?;
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
        let external = self.transaction.is_some() && body.is_external_entry();
        let args = match &self.transaction {
            // arg i = calldataload 4 + 32 * i
            Some(transaction) if external && args.is_empty() => body
                .arg_indices()
                .map(|index| {
                    let offset = 4 + index.index() as u64 * WORD_BYTES;
                    calldata_word(transaction.calldata, U256::from(offset))
                })
                .collect(),
            _ => args,
        };
        let params = if external { body.arg_indices().count() } else { body.params.len() };
        if args.len() != params || body.arg_indices().count() != args.len() {
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
        // Only lowered operations run; the arms below and `eval_semantics` decide the rest.
        if !instruction.kind.op_def().phases.contains(MirPhase::Lowered) {
            return ControlFlow::Break(Outcome::Unsupported(mnemonic));
        }
        let Some(semantics) = instruction.kind.semantics() else {
            return ControlFlow::Break(Outcome::Unsupported(mnemonic));
        };
        let frame = self.frames.last().expect("a run always has a frame");
        meter.instruction(body, inst, &|value| frame.word(value));
        let result = match semantics {
            Semantics::Phi(_) => {
                return ControlFlow::Break(Outcome::Unsupported("phi after other instructions"));
            }
            Semantics::Select(condition, if_true, if_false) => {
                let condition = frame.read(condition)?;
                frame.read(if condition.is_zero() { if_false } else { if_true })?
            }
            Semantics::Opcode(opcode, operands) if MEMORY_OPCODES.contains(&opcode) => {
                let operands = frame.read_all(&operands)?;
                match self.memory_opcode(opcode, &operands)? {
                    Some(word) => word,
                    None => return ControlFlow::Continue(()),
                }
            }
            Semantics::Opcode(opcode, operands) if self.world.is_some() && !op::is_pure(opcode) => {
                let operands = frame.read_all(&operands)?;
                match self.world_opcode(opcode, &operands)? {
                    Some(word) => word,
                    None => return ControlFlow::Continue(()),
                }
            }
            Semantics::Call(&Callee::Function(id), args) => {
                let args = frame.read_all(args)?.into_iter().collect();
                let Some(callee) = self.machine.body(id) else {
                    return ControlFlow::Break(Outcome::Unsupported("call to an unknown function"));
                };
                let base = self.take_heap_frame(id, callee)?;
                self.call(callee, args, instruction.result())?;
                self.frame().heap_frame_base = base;
                return ControlFlow::Continue(());
            }
            Semantics::Call(..) => return ControlFlow::Break(Outcome::Unsupported(mnemonic)),
            Semantics::DataCopy(data, dest, len) => {
                let (dest, len) = (frame.read(dest)?, frame.read(len)?);
                return self.data_copy(data, dest, len);
            }
            Semantics::Allocate(size, kind, _) if self.transaction.is_some() => {
                let size = frame.read(size)?;
                self.allocate(size, kind)?
            }
            semantics => match eval_semantics(semantics, |value| frame.word(value).ok_or(())) {
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
        meter.result(result);
        self.frame().values[value] = Some(result);
        ControlFlow::Continue(())
    }

    /// Runs a memory opcode on its operand words, returning its result when it has one.
    fn memory_opcode(
        &mut self,
        opcode: u8,
        operands: &[U256],
    ) -> ControlFlow<Outcome, Option<U256>> {
        ControlFlow::Continue(match (opcode, operands) {
            (op::MLOAD, &[offset]) => Some(limit(self.memory.load(offset))?),
            (op::MSTORE, &[offset, value]) => {
                limit(self.memory.store(offset, value))?;
                None
            }
            (op::MSTORE8, &[offset, value]) => {
                limit(self.memory.store8(offset, value))?;
                None
            }
            (op::MCOPY, &[dest, src, len]) => {
                self.burn_words(len)?;
                limit(self.memory.copy(dest, src, len))?;
                None
            }
            (op::KECCAK256, &[offset, len]) => {
                self.burn_words(len)?;
                let bytes = limit(self.memory.read(offset, len))?;
                Some(U256::from_be_bytes(keccak256(bytes).0))
            }
            _ => {
                let mnemonic = op::mnemonic(opcode).unwrap_or("opcode");
                return ControlFlow::Break(Outcome::Unsupported(mnemonic));
            }
        })
    }

    /// Runs an opcode on the world outside memory: storage, transient storage, logs, and the
    /// host's context, and in a transaction also its calldata and return data. Returns its result
    /// when it has one.
    fn world_opcode(
        &mut self,
        opcode: u8,
        operands: &[U256],
    ) -> ControlFlow<Outcome, Option<U256>> {
        let unsupported = || Outcome::Unsupported(op::mnemonic(opcode).unwrap_or("opcode"));
        // Operations on memory come first, while no borrow of the world is live.
        match (opcode, operands) {
            (op::CALLDATACOPY, &[dest, offset, len]) => {
                let Some(transaction) = &self.transaction else {
                    return ControlFlow::Break(unsupported());
                };
                let calldata = transaction.calldata;
                self.burn_words(len)?;
                let bytes = (0..len.to::<u64>())
                    .map(|index| calldata_byte(calldata, offset, index))
                    .collect::<Vec<_>>();
                limit(self.memory.write(dest, &bytes))?;
                return ControlFlow::Continue(None);
            }
            (op::LOG0..=op::LOG4, &[offset, len, ref topics @ ..]) => {
                self.burn_words(len)?;
                let data = limit(self.memory.read(offset, len))?;
                let Some(world) = &mut self.world else {
                    return ControlFlow::Break(unsupported());
                };
                world.effects.logs.push(Log { topics: topics.to_vec(), data });
                return ControlFlow::Continue(None);
            }
            _ => {}
        }
        let Some(world) = &mut self.world else {
            return ControlFlow::Break(unsupported());
        };
        let transaction = self.transaction.as_ref();
        ControlFlow::Continue(match (opcode, operands) {
            (op::CALLDATALOAD, &[offset]) if let Some(transaction) = transaction => {
                Some(calldata_word(transaction.calldata, offset))
            }
            (op::CALLDATASIZE, &[]) if let Some(transaction) = transaction => {
                Some(U256::from(transaction.calldata.len()))
            }
            (op::SLOAD, &[slot]) => {
                let original = world.host.storage(slot);
                let current = world.effects.storage.get(&slot).copied().unwrap_or(original);
                Some(current)
            }
            (op::SSTORE, &[slot, value]) => {
                world.effects.storage.insert(slot, value);
                None
            }
            (op::TLOAD, &[slot]) => Some(match world.effects.transient.get(&slot) {
                Some(&value) => value,
                None => world.host.transient(slot),
            }),
            (op::TSTORE, &[slot, value]) => {
                world.effects.transient.insert(slot, value);
                None
            }
            // The transaction makes no calls, so its return data stays empty.
            (op::RETURNDATASIZE, &[]) if transaction.is_some() => Some(U256::ZERO),
            // Copying past the end of the return data halts.
            (op::RETURNDATACOPY, &[_, offset, len]) if transaction.is_some() => {
                if !offset.is_zero() || !len.is_zero() {
                    return ControlFlow::Break(Outcome::Invalid);
                }
                None
            }
            _ if HOST_OPCODES.contains(&opcode) => match world.host.read(opcode, operands) {
                Some(word) => Some(word),
                None => return ControlFlow::Break(unsupported()),
            },
            _ => return ControlFlow::Break(unsupported()),
        })
    }

    /// Copies `len` bytes of the constant data at `data` to memory at `dest`.
    fn data_copy(&mut self, data: DataRef, dest: U256, len: U256) -> ControlFlow<Outcome> {
        self.burn_words(len)?;
        // Another contract's bytecode is known only once final assembly links it in.
        let Some(bytes) = self.machine.module.data.get(data.id).and_then(|data| data.bytes.known())
        else {
            return ControlFlow::Break(Outcome::Unsupported("datacopy of unlinked data"));
        };
        let start = data.offset as usize;
        // The backend copies from where it placed the data in the code, so the bytes past its end
        // are whatever follows it there.
        let Some(bytes) = bytes.get(start..start.saturating_add(len.to::<usize>())) else {
            return ControlFlow::Break(Outcome::Unsupported("datacopy past its data"));
        };
        limit(self.memory.write(dest, bytes))
    }

    /// Places an allocation whose placement the backend decides in the region the transaction
    /// owns. Such an allocation runs at most once per call and its address never escapes, so
    /// any fresh region serves. A fresh region is zero, as a zeroed allocation needs, and its
    /// alignment and failure rules cannot matter for a size that fits.
    fn allocate(&mut self, size: U256, kind: &AllocationKind) -> ControlFlow<Outcome, U256> {
        let Some(transaction) = &mut self.transaction else {
            return ControlFlow::Break(Outcome::Unsupported("alloc"));
        };
        if !matches!(kind, AllocationKind::Raw) || size > U256::from(MEMORY_LIMIT) {
            return ControlFlow::Break(Outcome::Unsupported("alloc"));
        }
        // address = next free byte, rounded up to a word after the region
        let address = transaction.allocations;
        let end = address + size.to::<u64>().next_multiple_of(WORD_BYTES);
        if end > MULTI_RETURN_BUFFER {
            return ControlFlow::Break(Outcome::Limit(Limit::Memory));
        }
        transaction.allocations = end;
        ControlFlow::Continue(U256::from(address))
    }

    /// Takes the frame the backend allocates on the heap for a call to `callee`, returning the
    /// base the free memory pointer returns to when the call returns, if the backend releases it.
    fn take_heap_frame(
        &mut self,
        id: FunctionId,
        callee: &Function,
    ) -> ControlFlow<Outcome, Option<U256>> {
        let (Some(transaction), Some(world)) = (&mut self.transaction, &mut self.world) else {
            return ControlFlow::Continue(None);
        };
        let heap_frame = *transaction
            .heap_frames
            .entry(id)
            .or_insert_with(|| world.host.heap_frame(&callee.name.to_string()));
        let Some(heap_frame) = heap_frame else { return ControlFlow::Continue(None) };
        // base = mload 0x40
        // mstore 0x40, base + frame size
        let slot = U256::from(EvmMemoryLayout::FMP_SLOT);
        let base = limit(self.memory.load(slot))?;
        limit(self.memory.store(slot, base.wrapping_add(U256::from(heap_frame.size))))?;
        ControlFlow::Continue(heap_frame.restores_free_memory.then_some(base))
    }

    /// Publishes the results after the first of a call returning several values the way the
    /// backend does: result `k` at `buffer + 32 * k`, with `buffer` in the word at `0x20`.
    fn publish_multi_return(&mut self, values: &[U256]) -> ControlFlow<Outcome> {
        if self.transaction.is_none() {
            // Further results pass through a memory buffer the backend owns.
            return ControlFlow::Break(Outcome::Unsupported("call returning several values"));
        }
        // mstore buffer + 32 * k, result k
        // mstore 0x20, buffer
        for (index, &value) in values.iter().enumerate().skip(1) {
            let offset = U256::from(MULTI_RETURN_BUFFER + index as u64 * WORD_BYTES);
            limit(self.memory.store(offset, value))?;
        }
        let slot = U256::from(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT);
        limit(self.memory.store(slot, U256::from(MULTI_RETURN_BUFFER)))
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
                if self.frames.is_empty() {
                    return ControlFlow::Break(Outcome::Return(values));
                }
                if values.len() > 1 {
                    self.publish_multi_return(&values)?;
                }
                if let Some(base) = frame.heap_frame_base {
                    // mstore 0x40, heap frame base
                    let slot = U256::from(EvmMemoryLayout::FMP_SLOT);
                    limit(self.memory.store(slot, base))?;
                }
                let caller = self.frame();
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
                // `REVERT` is an undefined opcode before Byzantium, so it halts.
                if let Some(transaction) = &self.transaction
                    && op::definition(op::REVERT)
                        .is_some_and(|revert| !revert.is_available(transaction.evm_version))
                {
                    return ControlFlow::Break(Outcome::Invalid);
                }
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
                self.call(callee, args, frame.result)?;
                // The backend jumps to the callee without taking a heap frame for it, and the
                // callee returns to this frame's caller, which releases this frame's heap.
                self.frame().heap_frame_base = frame.heap_frame_base;
                ControlFlow::Continue(())
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
        let mut incoming = SmallVec::<[(ValueId, U256); 8]>::new();
        for &inst in &block.instructions {
            let instruction = body.inst(inst);
            let InstKind::Phi(inputs) = &instruction.kind else { break };
            let (Some(&(_, value)), Some(result)) =
                (inputs.iter().find(|&&(block, _)| block == from), instruction.result())
            else {
                return ControlFlow::Break(Outcome::Unsupported("phi without an input"));
            };
            meter.instruction(body, inst, &|value| frame.word(value));
            let word = frame.read(value)?;
            meter.result(word);
            incoming.push((result, word));
        }
        self.burn(incoming.len() as u64)?;
        let frame = self.frame();
        frame.next = incoming.len();
        for &(result, value) in &incoming {
            frame.values[result] = Some(value);
        }
        frame.block = target;
        ControlFlow::Continue(())
    }
}

/// Returns the calldata word at `offset`, padded with zero bytes past the end.
fn calldata_word(calldata: &[u8], offset: U256) -> U256 {
    U256::from_be_bytes(std::array::from_fn::<u8, 32, _>(|index| {
        calldata_byte(calldata, offset, index as u64)
    }))
}

/// Returns byte `index` of the calldata word read at `offset`, zero past the end.
fn calldata_byte(calldata: &[u8], offset: U256, index: u64) -> u8 {
    let Ok(offset) = usize::try_from(offset) else { return 0 };
    offset
        .checked_add(index as usize)
        .and_then(|position| calldata.get(position))
        .copied()
        .unwrap_or_default()
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
        Machine::new(module).run(function, &[arg], Memory::zeroed(), None, LIMITS, &mut ())
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
            assert_eq!(execution.outcome, Outcome::Return([expected].into_iter().collect()));
        });
    }

    #[test]
    fn calls_and_halts() {
        with_module(|module, id| {
            assert_eq!(run(module, id("calls"), U256::from(5)).outcome, returned(20));
            let payload = U256::from(120).to_be_bytes::<32>().to_vec();
            assert_eq!(run(module, id("calls"), U256::from(30)).outcome, Outcome::Revert(payload));
        });
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

    /// Answers `CALLVALUE`, slot 7, and the heap frames of `@kept` and `@released`.
    struct TestHost;

    impl Host for TestHost {
        fn read(&mut self, opcode: u8, _: &[U256]) -> Option<U256> {
            (opcode == op::CALLVALUE).then_some(U256::from(5))
        }

        fn storage(&mut self, slot: U256) -> U256 {
            if slot == U256::from(7) { U256::from(100) } else { U256::ZERO }
        }

        fn heap_frame(&mut self, function: &str) -> Option<HeapFrame> {
            match function {
                "kept" => Some(HeapFrame { size: 0x40, restores_free_memory: false }),
                "released" => Some(HeapFrame { size: 0x60, restores_free_memory: true }),
                _ => None,
            }
        }
    }

    fn transact(source: &str, calldata: &[u8]) -> TransactionExecution {
        transact_on(source, calldata, EvmVersion::default())
    }

    fn transact_on(source: &str, calldata: &[u8], evm_version: EvmVersion) -> TransactionExecution {
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        sess.enter(|| {
            let module = parse_module(&sess, source).unwrap();
            Machine::new(&module).transact(calldata, &mut TestHost, evm_version, LIMITS, &mut ())
        })
    }

    #[test]
    fn transactions() {
        let context = transact(
            "@module Tx
@phase lowered
@types
  struct0: {i256, i256}

fn @pair(arg0: i256) -> struct0 {
  bb0:
    v0 = add arg0, 1
    ret arg0, v0
}

fn @entry() [entry] {
  bb0:
    v0 = callvalue
    v1 = calldataload 0
    v2 = sload 7
    v3 = add v2, v1
    sstore 7, v3
    tstore 1, v3
    v4 = tload 1
    v5 = icall @pair, v4
    v6 = mload 32
    v7 = add v6, 32
    v8 = mload v7
    mstore 0, v8
    log1 0, 32, v0
    v9 = calldatasize
    v10 = calldataload 1
    v11 = returndatasize
    mstore 32, v9
    mstore 64, v10
    mstore 96, v11
    returndata 0, 128
}
",
            &U256::from(3).to_be_bytes::<32>(),
        );
        let words = [104, 32, 3 << 8, 0].map(|word| U256::from(word).to_be_bytes::<32>());
        assert_eq!(context.outcome, Outcome::ReturnData(words.concat()));
        assert_eq!(
            context.effects.storage.into_iter().collect::<Vec<_>>(),
            [(U256::from(7), U256::from(103))]
        );
        let log = Log { topics: vec![U256::from(5)], data: words[0].to_vec() };
        assert_eq!(context.effects.logs, [log]);

        let module = "@module Tx
@phase lowered
fn @entry() [entry] {
  bb0:
    v0 = calldatasize
    switch v0, default bb1, [0 => bb2, 1 => bb3]
  bb1:
    ret
  bb2:
    returndatacopy 0, 0, 1
    stop
  bb3:
    v1 = caller
    stop
}
";
        assert_eq!(transact(module, &[0, 0]).outcome, Outcome::Stop);
        assert_eq!(transact(module, &[]).outcome, Outcome::Invalid);
        assert_eq!(transact(module, &[0]).outcome, Outcome::Unsupported("caller"));

        let module = "@module Tx
@phase lowered
@data
  literal_0: hex\"0102\"

fn @echo() [selector=0x00000001, abi_wrapper] {
  bb0:
    v0 = alloc raw, exact, uninitialized, infallible, 32 !metadata(deferred_alloc)
    datacopy literal_0, v0, 2
    v1 = mload v0
    v2 = eq arg0, 0
    jumpi v2, bb1, bb2
  bb1:
    revert 0, 0
  bb2:
    mstore 0, arg0
    mstore 32, v1
    returndata 0, 64
}

fn @entry() [entry] {
  bb0:
    tail_call @echo
}
";
        // An external entry reads its argument from calldata after the selector.
        let calldata = [&[0, 0, 0, 1][..], &U256::from(42).to_be_bytes::<32>()].concat();
        let data = U256::from(0x0102) << 240;
        let words = [U256::from(42), data].map(|word| word.to_be_bytes::<32>());
        assert_eq!(transact(module, &calldata).outcome, Outcome::ReturnData(words.concat()));
        // `REVERT` does not exist before Byzantium.
        assert_eq!(transact(module, &[0, 0, 0, 1]).outcome, Outcome::Revert(Vec::new()));
        let outcome = transact_on(module, &[0, 0, 0, 1], EvmVersion::Homestead).outcome;
        assert_eq!(outcome, Outcome::Invalid);
    }

    #[test]
    fn functions_with_a_host() {
        let source = "@module World
@phase lowered
fn @world(arg0: i256) -> i256 {
  bb0:
    v0 = sload 7
    v1 = add v0, arg0
    sstore 7, v1
    sstore 8, v0
    v2 = sload 7
    tstore 1, v2
    v3 = tload 1
    v4 = callvalue
    mstore 0, v4
    log2 0, 32, v3, arg0
    ret v3
}
";
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        sess.enter(|| {
            let module = parse_module(&sess, source).unwrap();
            let id = module.functions.indices().next().unwrap();
            let machine = Machine::new(&module);
            let args = [U256::from(3)];
            let host: &mut dyn Host = &mut TestHost;
            let execution = machine.run(id, &args, Memory::zeroed(), Some(host), LIMITS, &mut ());
            assert_eq!(execution.outcome, returned(103));
            let effects = execution.effects;
            let mut storage = effects.storage.into_iter().collect::<Vec<_>>();
            storage.sort_unstable();
            let words = |words: &[u64]| words.iter().map(|&word| U256::from(word)).collect();
            assert_eq!(
                storage,
                [(U256::from(7), U256::from(103)), (U256::from(8), U256::from(100))]
            );
            assert_eq!(
                effects.transient.into_iter().collect::<Vec<_>>(),
                [(U256::ONE, U256::from(103))]
            );
            let data = U256::from(5).to_be_bytes::<32>().to_vec();
            assert_eq!(effects.logs, [Log { topics: words(&[103, 3]), data }]);
            // Without a host, the run stops at its first storage access.
            let outcome = machine.run(id, &args, Memory::zeroed(), None, LIMITS, &mut ()).outcome;
            assert_eq!(outcome, Outcome::Unsupported("sload"));
        });
    }

    #[test]
    fn heap_frames() {
        let module = "@module Tx
@phase lowered
fn @kept() -> i256 {
  bb0:
    v0 = mload 64
    ret v0
}

fn @released() -> i256 {
  bb0:
    v0 = mload 64
    ret v0
}

fn @entry() [entry] {
  bb0:
    v0 = icall @released
    v1 = mload 64
    v2 = icall @kept
    v3 = mload 64
    mstore 512, v0
    mstore 544, v1
    mstore 576, v2
    mstore 608, v3
    returndata 512, 128
}
";
        // A call takes its frame above the heap start, 0x80, and a released frame is given back.
        let words = [0xe0, 0x80, 0xc0, 0xc0].map(|word| U256::from(word).to_be_bytes::<32>());
        assert_eq!(transact(module, &[]).outcome, Outcome::ReturnData(words.concat()));
    }
}
