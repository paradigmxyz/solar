//! Concrete execution of word-level lowered MIR.
//!
//! [`Machine::transact`] executes a whole transaction of a lowered module: its dispatch entry on
//! calldata, with zeroed memory and a [`Host`] answering what the contract reads from its context,
//! such as the caller or a storage slot's value before the transaction. The run reads and writes
//! persistent and transient storage and logs events, and reports how it ended and what it wrote
//! and logged. It is a testing oracle, not part of code generation: the UI test runner checks it
//! against an EVM running the compiled bytecode. The unit tests below also run single functions
//! on concrete arguments and memory.
//!
//! # Model
//!
//! Each instruction runs by the [`Semantics`] its operation schema row declares, so the
//! interpreter lists no operations of its own. Values are 256-bit words. Typed values keep the
//! clean bits their types promise because every operation computes them as the backend's
//! instructions do. The schema names opcodes by mnemonic, which the interpreter looks up in the
//! EVM IR opcode table; pure opcodes compute with the table's word semantics, as the compiler's
//! constant folding and the EVM IR interpreter do. Casts keep or clear bits, `select` picks an
//! operand, and the phis at the start of a block read their incoming values for the edge taken,
//! all before any of them is assigned.
//!
//! Memory is byte-addressed EVM memory, on which the memory opcodes run: `MLOAD`, `MSTORE`,
//! `MSTORE8`, `MCOPY`, `KECCAK256`, and `MSIZE`. An access with a nonzero length grows memory to
//! the word containing its last byte, and a zero-length access ignores its offset. `mcopy` behaves
//! as if it copied through a buffer, and `keccak256` hashes the bytes it reads. Memory that nothing
//! wrote is zero, unless the caller set it before the run.
//!
//! Internal calls and tail calls run the callee on the same memory, and a call's result is the
//! value the callee returns. The backend passes further results through a buffer of its own that
//! the word at `0x20` points to, so a function run alone may not call a callee returning several
//! values. A transaction uses the backend placements recorded in final MIR for each callee.
//! `revert`, `returndata`, `stop`, and `invalid` end the whole execution wherever they run, as they
//! end the transaction.
//!
//! A transaction follows the backend's conventions for external entries: the free memory pointer
//! starts at the heap start the host reports, and argument `i` of an external entry is the calldata
//! word at `4 + 32 * i`. It also reads calldata, persistent and transient storage, and the context
//! values its host provides, and records the logs it emits. It makes no calls, so its return data
//! is always empty. Returning from the dispatch entry stops the transaction, as the backend's
//! `STOP` does.
//!
//! # Limits
//!
//! A run has a fuel budget, one unit per operation and per copied, hashed, or returned word, and a
//! call depth. Memory ends at [`MEMORY_LIMIT`], past which the EVM runs out of gas under any block
//! gas limit. Exceeding a limit ends the run with [`Outcome::Limit`]. Operations the interpreter
//! does not model end it with [`Outcome::Unsupported`]: opcodes on storage, calldata, code, the
//! environment, external calls, logs, allocations, and operations that declare no semantics, such
//! as frame addresses and every semantic operation. A run with a host also runs the storage,
//! transient storage, and log opcodes and the context reads its host answers. A transaction also
//! runs the calldata and return data opcodes and places the allocations the backend would place,
//! but not `gas`, calls, or contract creation. It runs `msize` only once memory reaches the end of
//! the frames and spill slots the backend records for the entries it entered, since the backend
//! writes them without MIR showing it. The interpreter relies on the validator only for the
//! existence of the instructions, values, and blocks a function names, and checks the rest as it
//! runs, so a value used before its definition or a phi missing an edge also ends a run as
//! unsupported.

use alloy_primitives::{U256, keccak256};
use smallvec::SmallVec;
use solar_config::EvmVersion;
use solar_data_structures::{
    index::{IndexVec, index_vec},
    map::FxHashMap,
};
use solar_evmir::op;
use solar_mir::{
    AllocationPlacement, ArgIdx, BlockId, Callee, DataRef, Function, FunctionId, InstId, InstKind,
    Module, Semantics, Terminator, Value, ValueId, memory::EvmMemoryLayout,
};
use std::{convert::Infallible, ops::ControlFlow};

/// Bytes of memory an execution may use: growing memory to 16 MiB costs about 538 million gas,
/// more than any block holds.
pub(crate) const MEMORY_LIMIT: u64 = 1 << 24;

/// Bytes per memory word.
const WORD_BYTES: u64 = 32;

/// The opcodes a run executes on its memory, besides the pure opcodes it evaluates.
const MEMORY_OPCODES: [u8; 6] =
    [op::MLOAD, op::MSTORE, op::MSTORE8, op::MCOPY, op::KECCAK256, op::MSIZE];

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
    /// Execution reached `INVALID` or an opcode the EVM version lacks.
    Invalid,
    /// A returndata copy exceeded the available bytes.
    ReturndataOutOfBounds,
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
    /// Frames that may be live at once, the called function's included.
    pub(crate) depth: usize,
}

/// What one execution did.
#[cfg(test)]
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

    /// Runs `function` on `args` and `memory` within `limits`. With a `host`, the run also reads
    /// and writes storage and transient storage, logs events, and reads the context values the
    /// host answers.
    #[cfg(test)]
    pub(crate) fn run(
        &self,
        function: FunctionId,
        args: &[U256],
        memory: Memory,
        host: Option<&mut dyn Host>,
        limits: Limits,
    ) -> Execution {
        let mut run = Run {
            machine: self,
            memory,
            limits,
            fuel: 0,
            frames: Vec::new(),
            world: host.map(|host| World { host, effects: Effects::default() }),
            transaction: None,
            frames_end: U256::ZERO,
        };
        let ControlFlow::Break(outcome) = run.execute(function, args);
        let effects = run.world.map(|world| world.effects).unwrap_or_default();
        let effects = kept_effects(&outcome, effects);
        Execution { outcome, effects }
    }

    /// Runs the module's dispatch entry on `calldata` and `evm_version`, with the host's
    /// initial heap and the final dump's backend memory placements.
    pub(crate) fn transact(
        &self,
        calldata: &[u8],
        host: &mut dyn Host,
        evm_version: EvmVersion,
        limits: Limits,
    ) -> TransactionExecution {
        let Some(entry) = self.module.dispatch_entry() else {
            return TransactionExecution {
                outcome: Outcome::Unsupported("module without a dispatch entry"),
                effects: Effects::default(),
            };
        };
        // mstore 0x40, heap start
        let mut memory = Memory::zeroed();
        if self.module.backend_memory.is_none() {
            memory.set(EvmMemoryLayout::FMP_SLOT, host.free_memory_start());
        }
        let mut run = Run {
            machine: self,
            memory,
            limits,
            fuel: 0,
            frames: Vec::new(),
            world: Some(World { host, effects: Effects::default() }),
            transaction: Some(Transaction { calldata, evm_version }),
            frames_end: U256::ZERO,
        };
        let ControlFlow::Break(outcome) = run.execute(entry, &[]);
        let outcome = match outcome {
            // return [] => stop
            Outcome::Return(values) if values.is_empty() => Outcome::Stop,
            Outcome::Return(_) => Outcome::Unsupported("dispatch entry returning values"),
            outcome => outcome,
        };
        let world = run.world.expect("a transaction keeps its world");
        let effects = kept_effects(&outcome, world.effects);
        TransactionExecution { outcome, effects }
    }

    fn body(&self, id: FunctionId) -> Option<&'a Function> {
        self.module.functions.get(id)
    }
    fn function_id(&self, body: &Function) -> FunctionId {
        self.module
            .functions
            .iter_enumerated()
            .find_map(|(id, function)| std::ptr::eq(function, body).then_some(id))
            .expect("body belongs to module")
    }
}

/// The state of one call.
struct Frame<'a> {
    function: FunctionId,
    body: &'a Function,
    /// The original callee whose caller publishes its return buffer, across tail calls.
    return_owner: &'a Function,
    caller_inst: Option<InstId>,
    args: IndexVec<ArgIdx, U256>,
    values: IndexVec<ValueId, Option<U256>>,
    block: BlockId,
    /// The next instruction of `block` to run.
    next: usize,
    /// The caller's value receiving this call's first returned value.
    result: Option<ValueId>,
}

impl<'a> Frame<'a> {
    fn new(
        function: FunctionId,
        body: &'a Function,
        args: IndexVec<ArgIdx, U256>,
        result: Option<ValueId>,
    ) -> Self {
        let values = index_vec![None; body.num_values()];
        Self {
            function,
            body,
            return_owner: body,
            caller_inst: None,
            args,
            values,
            block: BlockId::ENTRY,
            next: 0,
            result,
        }
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
    /// The EVM version the module targets, which decides which opcodes exist.
    evm_version: EvmVersion,
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
    /// The end of the frames and spill slots of the runtime entries the run entered, which the
    /// backend writes without MIR showing it.
    frames_end: U256,
}

impl<'a> Run<'_, 'a, '_> {
    fn execute(&mut self, function: FunctionId, args: &[U256]) -> ControlFlow<Outcome, Infallible> {
        let Some(body) = self.machine.body(function) else {
            return ControlFlow::Break(Outcome::Unsupported("call to an unknown function"));
        };
        self.call(body, args.iter().copied().collect(), None)?;
        loop {
            let frame = self.frame();
            let (function, body, block, next) =
                (frame.function, frame.body, frame.block, frame.next);
            match body.blocks[block].instructions.get(next) {
                Some(&inst) => {
                    self.frame().next += 1;
                    let elided =
                        self.machine.module.backend_memory.as_ref().is_some_and(|layout| {
                            layout.stack_returns.iter().any(|(&(owner, _), protocol)| {
                                owner == function && protocol.elided.contains(&inst)
                            })
                        });
                    if !elided {
                        self.instruction(body, inst)?;
                    }
                }
                None => self.terminator(body, block)?,
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
        let function = self.machine.function_id(body);
        if let Some(layout) = &self.machine.module.backend_memory {
            if let Some(&end) = layout.frame_ends.get(&function) {
                self.frames_end = self.frames_end.max(end);
            }
            if let Some(floor) = layout.entry_heaps.get(&function) {
                // mstore 0x40, entry_heap
                limit(self.memory.store(U256::from(EvmMemoryLayout::FMP_SLOT), *floor))?;
            }
        }
        self.frames.push(Frame::new(function, body, args, result));
        ControlFlow::Continue(())
    }

    /// Returns whether the transaction's EVM version lacks `opcode`, which then halts as an
    /// undefined opcode, as `REVERT` does before Byzantium.
    fn lacks(&self, opcode: u8) -> bool {
        self.transaction.as_ref().is_some_and(|transaction| {
            op::definition(opcode).is_some_and(|def| !def.is_available(transaction.evm_version))
        })
    }

    fn instruction(&mut self, body: &'a Function, inst: InstId) -> ControlFlow<Outcome> {
        self.burn(1)?;
        let instruction = body.inst(inst);
        let mnemonic = instruction.kind.op_def().mnemonic;
        // Only lowered operations run; the arms below decide the rest.
        if instruction.unlowered_reason(body).is_some() {
            return ControlFlow::Break(Outcome::Unsupported(mnemonic));
        }
        let Some(semantics) = instruction.kind.semantics() else {
            return ControlFlow::Break(Outcome::Unsupported(mnemonic));
        };
        let frame = self.frames.last().expect("a run always has a frame");
        let result = match semantics {
            Semantics::HeapFloor if self.transaction.is_some() => {
                let function = frame.function;
                self.machine
                    .module
                    .backend_memory
                    .as_ref()
                    .and_then(|layout| layout.heap_floors.get(&function).copied())
                    .unwrap_or_else(|| {
                        self.world
                            .as_mut()
                            .expect("a transaction has a host")
                            .host
                            .free_memory_start()
                    })
            }
            Semantics::Phi(_) => {
                return ControlFlow::Break(Outcome::Unsupported("phi after other instructions"));
            }
            Semantics::Select(condition, if_true, if_false) => {
                let condition = frame.read(condition)?;
                frame.read(if condition.is_zero() { if_false } else { if_true })?
            }
            Semantics::Opcode(name, operands) => {
                let operands = frame.read_all(&operands)?;
                match self.opcode(mnemonic, name, &operands)? {
                    Some(word) => word,
                    None => return ControlFlow::Continue(()),
                }
            }
            Semantics::Call(&Callee::Function(id), args) => {
                let args = frame.read_all(args)?.into_iter().collect();
                let Some(callee) = self.machine.body(id) else {
                    return ControlFlow::Break(Outcome::Unsupported("call to an unknown function"));
                };
                self.call(callee, args, instruction.result())?;
                self.frame().caller_inst = Some(inst);
                return ControlFlow::Continue(());
            }
            Semantics::Call(..) => return ControlFlow::Break(Outcome::Unsupported(mnemonic)),
            Semantics::DataCopy(data, dest, len) => {
                let (dest, len) = (frame.read(dest)?, frame.read(len)?);
                return self.data_copy(data, dest, len);
            }
            Semantics::Allocate(size, _) if self.transaction.is_some() => {
                let size = frame.read(size)?;
                self.allocate(frame.function, inst, size)?
            }
            Semantics::Word(value) => frame.read(value)?,
            Semantics::LowBits(value, bits @ 1..=256) => {
                frame.read(value)? & (U256::MAX >> (256 - bits))
            }
            Semantics::NotEqual(a, b) => U256::from(frame.read(a)? != frame.read(b)?),
            // Sign extensions, checked arithmetic, and builtin calls are semantic operations.
            _ => return ControlFlow::Break(Outcome::Unsupported(mnemonic)),
        };
        let Some(value) = instruction.result() else {
            return ControlFlow::Break(Outcome::Unsupported("result without a value"));
        };
        self.frame().values[value] = Some(result);
        ControlFlow::Continue(())
    }

    /// Runs the opcode the schema names `name` on its operand words, for an instruction with the
    /// mnemonic `mnemonic`, returning its result when it has one.
    fn opcode(
        &mut self,
        mnemonic: &'static str,
        name: &str,
        operands: &[U256],
    ) -> ControlFlow<Outcome, Option<U256>> {
        let Some(opcode) = op::from_mnemonic(name) else {
            return ControlFlow::Break(Outcome::Unsupported(mnemonic));
        };
        if self.lacks(opcode) {
            return ControlFlow::Break(Outcome::Invalid);
        }
        if MEMORY_OPCODES.contains(&opcode) {
            return self.memory_opcode(opcode, operands);
        }
        if op::is_pure(opcode) {
            return match op::eval_opcode(opcode, operands) {
                Some(word) => ControlFlow::Continue(Some(word)),
                None => ControlFlow::Break(Outcome::Unsupported(mnemonic)),
            };
        }
        if self.world.is_none() {
            return ControlFlow::Break(Outcome::Unsupported(mnemonic));
        }
        self.world_opcode(opcode, operands)
    }

    /// Runs a memory opcode on its operand words, returning its result when it has one.
    fn memory_opcode(
        &mut self,
        opcode: u8,
        operands: &[U256],
    ) -> ControlFlow<Outcome, Option<U256>> {
        ControlFlow::Continue(match (opcode, operands) {
            (op::MSIZE, &[]) => {
                let size = U256::from(self.memory.size * WORD_BYTES);
                // Spills and frame writes MIR does not show may have grown EVM memory up to the
                // end of the backend's frames, which a transaction needs its layout to know.
                if self.transaction.is_some() {
                    if self.machine.module.backend_memory.is_none() {
                        return ControlFlow::Break(Outcome::Unsupported(
                            "msize without backend layout",
                        ));
                    }
                    if size < self.frames_end {
                        return ControlFlow::Break(Outcome::Unsupported(
                            "msize below backend frames",
                        ));
                    }
                }
                Some(size)
            }
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
                    return ControlFlow::Break(Outcome::ReturndataOutOfBounds);
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

    /// Executes the backend's recorded static address or ordered dynamic FMP bump.
    /// Missing placements are unsupported, since synthetic pointers can change execution.
    fn allocate(
        &mut self,
        function: FunctionId,
        inst: InstId,
        size: U256,
    ) -> ControlFlow<Outcome, U256> {
        let Some(_) = &self.transaction else {
            return ControlFlow::Break(Outcome::Unsupported("alloc"));
        };
        if size > U256::from(MEMORY_LIMIT) {
            return ControlFlow::Break(Outcome::Unsupported("alloc"));
        }
        if let Some(layout) = &self.machine.module.backend_memory {
            let Some(placement) = layout.allocations.get(&(function, inst)) else {
                return ControlFlow::Break(Outcome::Unsupported(
                    "missing backend allocation placement",
                ));
            };
            return match placement {
                AllocationPlacement::Static(address) => ControlFlow::Continue(*address),
                AllocationPlacement::Dynamic(size) => {
                    // pointer = mload 0x40
                    // mstore 0x40, pointer + size
                    let slot = U256::from(EvmMemoryLayout::FMP_SLOT);
                    let address = limit(self.memory.load(slot))?;
                    limit(self.memory.store(slot, address.wrapping_add(*size)))?;
                    ControlFlow::Continue(address)
                }
            };
        }
        ControlFlow::Break(Outcome::Unsupported("allocation without backend placement"))
    }

    /// Publishes the results after the first of a call returning several values the way the
    /// backend does: result `k` at `buffer + 32 * k`, with `buffer` in the word at `0x20`.
    fn publish_multi_return(&mut self, body: &Function, values: &[U256]) -> ControlFlow<Outcome> {
        let Some(_) = &self.transaction else {
            // Further results pass through a memory buffer the backend owns.
            return ControlFlow::Break(Outcome::Unsupported("call returning several values"));
        };
        let buffer = if let Some(layout) = &self.machine.module.backend_memory {
            let function = self.machine.function_id(body);
            let Some(buffer) = layout.returns.get(&function) else {
                return ControlFlow::Break(Outcome::Unsupported(
                    "missing backend return buffer placement",
                ));
            };
            *buffer
        } else {
            return ControlFlow::Break(Outcome::Unsupported(
                "return buffer without backend placement",
            ));
        };
        // mstore buffer + 32 * k, result k
        // mstore 0x20, buffer
        for (index, &value) in values.iter().enumerate().skip(1) {
            let offset = buffer.wrapping_add(U256::from(index as u64 * WORD_BYTES));
            limit(self.memory.store(offset, value))?;
        }
        let slot = U256::from(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT);
        limit(self.memory.store(slot, U256::from(buffer)))
    }

    fn terminator(&mut self, body: &'a Function, block: BlockId) -> ControlFlow<Outcome> {
        self.burn(1)?;
        let Some(terminator) = &body.blocks[block].terminator else {
            return ControlFlow::Break(Outcome::Unsupported("block without a terminator"));
        };
        let frame = self.frames.last().expect("a run always has a frame");
        match terminator {
            &Terminator::Jump(target) => self.enter(target),
            &Terminator::Branch { condition, then_block, else_block } => {
                let condition = frame.read(condition)?;
                self.enter(if condition.is_zero() { else_block } else { then_block })
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
                self.enter(target)
            }
            Terminator::Return { values } => {
                let values = frame.read_all(values)?.into_iter().collect::<SmallVec<_>>();
                let frame = self.frames.pop().expect("a run always has a frame");
                if self.frames.is_empty() {
                    return ControlFlow::Break(Outcome::Return(values));
                }
                if values.len() > 1 {
                    let caller_id = self.frames.last().expect("caller").function;
                    let protocol = self.machine.module.backend_memory.as_ref().and_then(|layout| {
                        layout.stack_returns.get(&(caller_id, frame.caller_inst?))
                    });
                    if let Some(protocol) = protocol {
                        if protocol.extras.len() != values.len() - 1 {
                            return ControlFlow::Break(Outcome::Unsupported(
                                "invalid backend return arity",
                            ));
                        }
                        for (value, &word) in protocol.extras.iter().zip(&values[1..]) {
                            if let Some(value) = value {
                                self.frame().values[*value] = Some(word);
                            }
                        }
                    } else {
                        self.publish_multi_return(frame.return_owner, &values)?;
                    }
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
                if self.lacks(op::REVERT) {
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
                self.frame().return_owner = frame.return_owner;
                self.frame().caller_inst = frame.caller_inst;
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
    fn enter(&mut self, target: BlockId) -> ControlFlow<Outcome> {
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
            let word = frame.read(value)?;
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

/// Returns what an execution that ended with `outcome` did outside memory: nothing unless it
/// returned or stopped, since a revert or exceptional halt discards its writes and logs.
fn kept_effects(outcome: &Outcome, effects: Effects) -> Effects {
    match outcome {
        Outcome::Return(_) | Outcome::ReturnData(_) | Outcome::Stop => effects,
        _ => Effects::default(),
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
    use solar_interface::{ColorChoice, Session, source_map::FileName};
    use solar_mir::ResultKind;

    const LIMITS: Limits = Limits { fuel: 10_000, depth: 16 };

    fn parse_module(sess: &Session, input: &str) -> solar_interface::Result<Module> {
        let name = FileName::Custom("test.mir".into());
        Module::parse(sess, &sess.source_map().new_source_file(name, input).unwrap())
    }

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
        Machine::new(module).run(function, &[arg], Memory::zeroed(), None, LIMITS)
    }

    fn returned(value: u64) -> Outcome {
        Outcome::Return([U256::from(value)].into_iter().collect())
    }

    #[test]
    fn declared_opcodes_exist() {
        let samples = solar_mir::semantics::opcode_samples();
        assert!(!samples.is_empty());
        for kind in samples {
            let Some(Semantics::Opcode(name, operands)) = kind.semantics() else { unreachable!() };
            let pushes = u8::from(kind.op_def().result != ResultKind::None);
            let stack_io = op::from_mnemonic(name)
                .and_then(op::stack_io)
                .map(|(pops, pushes)| (usize::from(pops), pushes));
            assert_eq!(stack_io, Some((operands.len(), pushes)), "{kind:?}");
        }
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

    /// Answers `CALLVALUE` and slot 7.
    struct TestHost;

    impl Host for TestHost {
        fn read(&mut self, opcode: u8, _: &[U256]) -> Option<U256> {
            (opcode == op::CALLVALUE).then_some(U256::from(5))
        }

        fn storage(&mut self, slot: U256) -> U256 {
            if slot == U256::from(7) { U256::from(100) } else { U256::ZERO }
        }
    }

    fn transact(source: &str, calldata: &[u8]) -> TransactionExecution {
        transact_on(source, calldata, EvmVersion::default())
    }

    fn transact_on(source: &str, calldata: &[u8], evm_version: EvmVersion) -> TransactionExecution {
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        sess.enter(|| {
            let module = parse_module(&sess, source).unwrap();
            Machine::new(&module).transact(calldata, &mut TestHost, evm_version, LIMITS)
        })
    }

    #[test]
    fn transactions() {
        let context = transact(
            "@module Tx
@phase lowered
// backend-memory-v1
// backend-return 0 0x200
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
    log 0, 32, v0
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
        assert_eq!(transact(module, &[]).outcome, Outcome::ReturndataOutOfBounds);
        assert_eq!(transact(module, &[0]).outcome, Outcome::Unsupported("caller"));

        let module = "@module Tx
@phase lowered
// backend-memory-v1
// backend-alloc 0 0 dynamic 0x20
// backend-entry 0 0x80
@data
  literal_0: hex\"0102\"

fn @echo() [selector=0x00000001, abi_wrapper] {
  bb0:
    v0 = alloc 32 !metadata(deferred_alloc)
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

        // Neither does `TSTORE` before Cancun.
        let module = "@module Transient
@phase lowered
fn @entry() [entry] {
  bb0:
    tstore 1, 2
    stop
}
";
        assert_eq!(transact_on(module, &[], EvmVersion::Shanghai).outcome, Outcome::Invalid);
        assert_eq!(transact_on(module, &[], EvmVersion::Cancun).outcome, Outcome::Stop);
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
    log 0, 32, v3, arg0
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
            let execution = machine.run(id, &args, Memory::zeroed(), Some(host), LIMITS);
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
            let outcome = machine.run(id, &args, Memory::zeroed(), None, LIMITS).outcome;
            assert_eq!(outcome, Outcome::Unsupported("sload"));
        });
    }
}
