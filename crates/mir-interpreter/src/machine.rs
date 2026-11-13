//! Concrete execution of word-level lowered MIR.
//!
//! [`Machine::transact`] executes a whole transaction of a lowered module: its dispatch entry on
//! calldata, with zeroed memory and a [`Host`] answering what the contract reads from its context,
//! such as the caller or a storage slot's value before the transaction. The run reads and writes
//! persistent and transient storage and logs events, and reports how it ended and what it wrote
//! and logged. It is a testing oracle, not part of code generation: the UI test runner checks it
//! against an EVM running the compiled bytecode. The unit tests below also run single functions
//! on concrete arguments and memory.
//! [`crate::replay`] also runs typed semantic MIR directly, without implicit backend setup.
//! It supports checked arithmetic and casts before lowering, with target opcode availability.
//!
//! # Model
//!
//! Each instruction runs by the [`Semantics`] its operation schema row declares, so the
//! interpreter lists no operations of its own. Lowered values are words; semantic products
//! retain a tree of words and undefined fields. Typed words keep the
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
//! `STOP` does. Direct semantic state replay can also execute external messages through
//! [`Host::message`], with sufficient caller gas and explicit child budgets for creation.
//! The host owns the resulting journal and outer rollback. Return data is shared across
//! internal activations; a child message replaces it, including failed and empty calls.
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

use crate::values::{self, RuntimeValue};
use alloy_primitives::{I256, U256, keccak256};
use smallvec::SmallVec;
use solar_config::EvmVersion;
use solar_data_structures::{
    index::{IndexVec, index_vec},
    map::FxHashMap,
};
use solar_evmir::op;
use solar_mir::{
    AllocationFailure, AllocationInitialization, AllocationPlacement, AllocationSemantics, ArgIdx,
    ArithmeticKind, BlockId, Builtin, Callee, CheckedOp, Function, FunctionId, InstId, InstKind,
    MirType, Module, Semantics, Terminator, Value, ValueId, memory::EvmMemoryLayout,
};
use std::{convert::Infallible, ops::ControlFlow};

mod abi;
mod arrays;
mod builtins;
mod frames;
mod links;
mod messages;
mod placement;
mod precompiles;
mod returns;
mod storage;

/// Bytes of memory an execution may use: growing memory to 16 MiB costs about 538 million gas,
/// more than any block holds.
pub(crate) const MEMORY_LIMIT: u64 = 1 << 24;

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
    /// Return data shared across internal activations, not private SSA locals.
    pub(crate) return_data: Vec<u8>,
}

/// Answers what a run reads from its context and cannot know itself.
pub trait Host {
    /// Returns what `opcode` reads for these operands from the transaction's context or the
    /// chain's state, such as `CALLER`, `TIMESTAMP`, or `BALANCE`, or `None` when the host does
    /// not model it.
    fn read(&mut self, opcode: u8, operands: &[U256]) -> Option<U256>;

    /// A placement or link input at this MIR window, never a guessed backend address.
    fn placement(&mut self, query: PlacementRead<'_>) -> Option<U256> {
        let _ = query;
        None
    }

    /// The fixed code image containing this window, when supplied explicitly.
    /// This does not authorize executing unknown MIR root code through a child call.
    fn code_image(&mut self) -> Option<Vec<u8>> {
        None
    }

    /// Original external code bytes, without resolving delegation, or `None` if unknown.
    /// Account access must be recorded even when a caller requests zero copied bytes.
    fn code(&mut self, address: U256) -> Option<Vec<u8>> {
        let _ = address;
        None
    }

    /// Returns the host's current persistent value, overlaid by unpublished local writes.
    /// A message host publishes pending writes before child entry, so subsequent reads see
    /// changes committed by that child. Without messages this is the initial value.
    fn storage(&mut self, slot: U256) -> U256;

    /// Returns the host's current transient value, overlaid by unpublished writes.
    /// Without messages this is its value when the run starts: zero for a transaction.
    fn transient(&mut self, slot: U256) -> U256 {
        let _ = slot;
        U256::ZERO
    }

    /// Executes a real child message after publishing tentative caller changes. The caller
    /// has sufficient gas to forward the requested CALL budget without EIP-150 clipping;
    /// creation's child gas is an explicit host boundary input. Implementations own the
    /// resulting world and outer rollback, and must not substitute an arbitrary result.
    fn message(
        &mut self,
        message: ExternalMessage<'_>,
        pending: PendingChanges<'_>,
    ) -> Result<MessageOutput, &'static str> {
        let _ = (message, pending);
        Err("external messages are not modeled by this host")
    }

    /// Returns where the free memory pointer starts: the heap start the backend's layout chose,
    /// which programs can observe only by exposing an address.
    fn free_memory_start(&mut self) -> U256 {
        U256::from(EvmMemoryLayout::HEAP_START)
    }

    /// Whether storage writes and logs are forbidden in this call frame.
    fn is_static(&self) -> bool {
        false
    }

    /// Applies SELFDESTRUCT to the current account after publishing its changes.
    /// The host owns fork-specific balance transfer and deferred deletion.
    fn destroy(
        &mut self,
        beneficiary: U256,
        pending: PendingChanges<'_>,
    ) -> Result<(), &'static str> {
        let _ = (beneficiary, pending);
        Err("selfdestruct is not modeled by this host")
    }

    /// Returns the next explicit child budget for an implicit-gas builtin call.
    /// MIR does not determine the caller's physical gas consumption.
    fn builtin_gas(&mut self) -> Option<U256> {
        None
    }

    /// Whether compiler-generated checks retain their diagnostic strings.
    fn debug_reverts(&self) -> bool {
        false
    }
}

/// Explicit inputs for layout-dependent MIR operations.
#[derive(Clone, Copy, Debug)]
pub enum PlacementRead<'a> {
    /// The entry heap lower bound for a named function.
    HeapFloor(&'a str),
    /// The current activation's internal frame base.
    FrameBase(&'a str, usize),
    /// The explicitly selected memory-return convention's buffer.
    ReturnBuffer(&'a str),
    /// A deferred allocation's fixed address, absent for an FMP-backed placement.
    AllocationAddress(&'a str, usize),
    /// Its explicitly selected extent, for either placement kind.
    AllocationSize(&'a str, usize),
    /// Copied constructor argument range start.
    ConstructorBase,
    /// Copied constructor argument range end.
    ConstructorEnd,
    /// The byte length of a linked data entry.
    DataSize(usize),
    /// The first byte of an explicitly placed data entry in the supplied code image.
    DataOffset(usize),
    /// Constructor immutable staging word address.
    ImmutableSlot(usize),
    /// Typed runtime immutable value after patching.
    ImmutableValue(usize),
    /// Linked library address.
    Library(usize),
    /// Gas just before an isolated GAS opcode window.
    GasBefore,
}

/// A semantic MIR message; no caller bytecode or stack protocol is synthesized.
#[derive(Debug)]
pub enum ExternalMessage<'a> {
    /// CALL, CALLCODE, DELEGATECALL or STATICCALL with explicit requested gas.
    Call { opcode: u8, gas: U256, address: U256, value: U256, input: &'a [u8] },
    /// CREATE or CREATE2; the host supplies the explicit child budget.
    Create { value: U256, salt: Option<U256>, initcode: &'a [u8] },
}

/// Tentative caller changes since the last message, published before entering its child.
pub struct PendingChanges<'a> {
    /// Current persistent writes in the caller's storage context.
    pub storage: &'a [(U256, U256)],
    /// Current transient writes in the caller's storage context.
    pub transient: &'a [(U256, U256)],
    /// Ordered caller logs preceding the message.
    pub logs: &'a [Log],
}

/// A completed child message, with full return/revert data and its scalar result.
pub struct MessageOutput {
    /// Success flag for calls, or created address (zero on failure).
    pub value: U256,
    /// Empty on exceptions, complete output on success or revert.
    pub return_data: Vec<u8>,
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
        let (word, index) =
            (address / EvmMemoryLayout::WORD_SIZE, (address % EvmMemoryLayout::WORD_SIZE) as usize);
        self.words.get(&word).map_or(0, |bytes| bytes[index])
    }

    fn set_byte(&mut self, address: u64, value: u8) {
        let (word, index) =
            (address / EvmMemoryLayout::WORD_SIZE, (address % EvmMemoryLayout::WORD_SIZE) as usize);
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
        self.size = self.size.max((offset + len).div_ceil(EvmMemoryLayout::WORD_SIZE));
        Ok(Some(offset))
    }

    fn load(&mut self, offset: U256) -> Result<U256, Limit> {
        let offset =
            self.access(offset, U256::from(EvmMemoryLayout::WORD_SIZE))?.unwrap_or_default();
        Ok(self.word(offset))
    }

    fn store(&mut self, offset: U256, value: U256) -> Result<(), Limit> {
        let offset =
            self.access(offset, U256::from(EvmMemoryLayout::WORD_SIZE))?.unwrap_or_default();
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

    fn zero(&mut self, offset: U256, len: U256) -> Result<(), Limit> {
        let Some(offset) = self.access(offset, len)? else { return Ok(()) };
        for index in 0..len.to::<u64>() {
            self.set_byte(offset + index, 0);
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

    pub(crate) fn from_prefix(prefix: &[u8], size: u64) -> Result<Self, String> {
        if size > MEMORY_LIMIT || !size.is_multiple_of(32) || prefix.len() as u64 > size {
            return Err("invalid or oversized replay memory".into());
        }
        let mut memory = Self::zeroed();
        memory.size = size / 32;
        for (index, &byte) in prefix.iter().enumerate() {
            memory.set_byte(index as u64, byte);
        }
        Ok(memory)
    }

    pub(crate) fn prefix(&self, count: u64) -> Vec<u8> {
        (0..count).map(|i| self.byte(i)).collect()
    }

    pub(crate) fn size_bytes(&self) -> u64 {
        self.size * 32
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
            semantic: None,
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
            semantic: None,
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

    pub(crate) fn replay(
        &self,
        function: FunctionId,
        args: &[U256],
        memory: Memory,
        limits: Limits,
        evm_version: EvmVersion,
    ) -> (Outcome, Memory) {
        let mut run = Run {
            machine: self,
            memory,
            limits,
            fuel: 0,
            frames: Vec::new(),
            world: None,
            transaction: None,
            frames_end: U256::ZERO,
            semantic: Some(evm_version),
        };
        let ControlFlow::Break(outcome) = run.execute(function, args);
        (outcome, run.memory)
    }

    /// Direct semantic replay with explicit state, without backend entry setup.
    pub(crate) fn replay_state(
        &self,
        function: FunctionId,
        args: &[U256],
        memory: Memory,
        host: &mut dyn Host,
        transaction: (&[u8], EvmVersion),
        limits: Limits,
    ) -> (Outcome, Memory, Effects) {
        let (calldata, evm_version) = transaction;
        let mut run = Run {
            machine: self,
            memory,
            limits,
            fuel: 0,
            frames: Vec::new(),
            world: Some(World { host, effects: Effects::default() }),
            transaction: Some(Transaction { calldata, evm_version }),
            frames_end: U256::ZERO,
            semantic: Some(evm_version),
        };
        let ControlFlow::Break(outcome) = run.execute(function, args);
        let effects =
            kept_effects(&outcome, run.world.expect("state replay keeps its host").effects);
        (outcome, run.memory, effects)
    }
}

/// The state of one call.
struct Frame<'a> {
    function: FunctionId,
    body: &'a Function,
    /// The original callee whose caller publishes its return buffer, across tail calls.
    return_owner: &'a Function,
    caller_inst: Option<InstId>,
    args: IndexVec<ArgIdx, RuntimeValue>,
    values: IndexVec<ValueId, Option<RuntimeValue>>,
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
        args: IndexVec<ArgIdx, RuntimeValue>,
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
            Value::Arg(index) => match self.args.get(*index)? {
                RuntimeValue::Word(word) => Some(*word),
                _ => None,
            },
            Value::Inst(_) => match self.values[value].as_ref()? {
                RuntimeValue::Word(word) => Some(*word),
                _ => None,
            },
            _ => None,
        }
    }

    fn value(&self, value: ValueId) -> Option<RuntimeValue> {
        if value.index() >= self.body.num_values() {
            return None;
        }
        match self.body.value(value) {
            Value::Immediate(immediate) => immediate.as_u256().map(RuntimeValue::Word),
            Value::Arg(index) => self.args.get(*index).cloned(),
            Value::Inst(_) => self.values[value].clone(),
            Value::Undef(_) => Some(RuntimeValue::Undefined),
            Value::Error(_) => None,
        }
    }

    fn read_value(&self, value: ValueId) -> ControlFlow<Outcome, RuntimeValue> {
        self.value(value).map_or_else(
            || ControlFlow::Break(Outcome::Unsupported("value without a definition")),
            ControlFlow::Continue,
        )
    }

    fn read_values(
        &self,
        values: &[ValueId],
    ) -> ControlFlow<Outcome, IndexVec<ArgIdx, RuntimeValue>> {
        let mut result = IndexVec::new();
        for &value in values {
            result.push(self.read_value(value)?);
        }
        ControlFlow::Continue(result)
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
    /// Direct semantic function execution, without an implied backend stack layout.
    semantic: Option<EvmVersion>,
}

impl<'a> Run<'_, 'a, '_> {
    fn execute(&mut self, function: FunctionId, args: &[U256]) -> ControlFlow<Outcome, Infallible> {
        let Some(body) = self.machine.body(function) else {
            return ControlFlow::Break(Outcome::Unsupported("call to an unknown function"));
        };
        let args = if self.semantic.is_some() {
            let mut words = args.iter().copied();
            let mut args = IndexVec::new();
            for &ty in &body.params {
                let value = RuntimeValue::input(self.machine.module, ty, &mut words).map_or_else(
                    |e| ControlFlow::Break(Outcome::Unsupported(e)),
                    ControlFlow::Continue,
                )?;
                args.push(value);
            }
            if words.next().is_some() {
                return ControlFlow::Break(Outcome::Unsupported("extra flattened MIR argument"));
            }
            args
        } else {
            args.iter().copied().map(RuntimeValue::Word).collect()
        };
        self.call(body, args, None)?;
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
        self.burn(len.to::<u64>().div_ceil(EvmMemoryLayout::WORD_SIZE))
    }

    /// Pushes a frame running `body` on `args`, returning its first value into `result`.
    fn call(
        &mut self,
        body: &'a Function,
        args: IndexVec<ArgIdx, RuntimeValue>,
        result: Option<ValueId>,
    ) -> ControlFlow<Outcome> {
        if self.frames.len() >= self.limits.depth {
            return ControlFlow::Break(Outcome::Limit(Limit::Depth));
        }
        if args.len() != body.params.len() || body.arg_indices().count() != args.len() {
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
        if self.semantic.is_some() {
            self.burn(args.iter().map(RuntimeValue::nodes).sum())?;
        }
        self.frames.push(Frame::new(function, body, args, result));
        ControlFlow::Continue(())
    }

    /// Returns whether the execution's EVM version lacks `opcode`, which then halts as an
    /// undefined opcode, as `REVERT` does before Byzantium.
    fn lacks(&self, opcode: u8) -> bool {
        self.transaction.as_ref().map(|tx| tx.evm_version).or(self.semantic).is_some_and(
            |version| op::definition(opcode).is_some_and(|def| !def.is_available(version)),
        )
    }

    fn instruction(&mut self, body: &'a Function, inst: InstId) -> ControlFlow<Outcome> {
        self.burn(1)?;
        let instruction = body.inst(inst);
        let mnemonic = instruction.kind.op_def().mnemonic;
        // Transactions run lowered operations; direct replay can use scalar semantic operations.
        if self.semantic.is_none() && instruction.unlowered_reason(body).is_some() {
            return ControlFlow::Break(Outcome::Unsupported(mnemonic));
        }
        let Some(semantics) = instruction.kind.semantics() else {
            return ControlFlow::Break(Outcome::Unsupported(mnemonic));
        };
        let frame = self.frames.last().expect("a run always has a frame");
        if self.semantic.is_some() {
            match semantics {
                Semantics::HeapFloor => {
                    let name = body.name.to_string();
                    let word = self.placement(PlacementRead::HeapFloor(&name))?;
                    self.frame().values[instruction.result().expect("heap floor")] =
                        Some(RuntimeValue::Word(word));
                    return ControlFlow::Continue(());
                }
                Semantics::InternalFrameAddress(offset) => {
                    let name = body.name.to_string();
                    let activation =
                        self.frames.iter().filter(|frame| std::ptr::eq(frame.body, body)).count()
                            - 1;
                    let base = self.placement(PlacementRead::FrameBase(&name, activation))?;
                    self.frame().values[instruction.result().expect("frame address")] =
                        Some(RuntimeValue::Word(base + U256::from(offset)));
                    return ControlFlow::Continue(());
                }
                Semantics::ConstructorArgs(end) => {
                    let word = self.placement(if end {
                        PlacementRead::ConstructorEnd
                    } else {
                        PlacementRead::ConstructorBase
                    })?;
                    self.frame().values[instruction.result().expect("constructor range")] =
                        Some(RuntimeValue::Word(word));
                    return ControlFlow::Continue(());
                }
                Semantics::LinkedDataSize(size) => {
                    let length = self.placement(PlacementRead::DataSize(size.data.index()))?;
                    if length > U256::from(u32::MAX) {
                        return ControlFlow::Break(Outcome::Unsupported(
                            "linked data exceeds the u32 extent bound",
                        ));
                    }
                    let word = size.value(length.to::<usize>());
                    self.frame().values[instruction.result().expect("linked size")] =
                        Some(RuntimeValue::Word(word));
                    return ControlFlow::Continue(());
                }
                Semantics::LibraryAddress(id) => {
                    let word = self.placement(PlacementRead::Library(id.index()))?;
                    if word >> 160 != U256::ZERO {
                        return ControlFlow::Break(Outcome::Unsupported(
                            "unclean linked library address",
                        ));
                    }
                    self.frame().values[instruction.result().expect("library address")] =
                        Some(RuntimeValue::Word(word));
                    return ControlFlow::Continue(());
                }
                Semantics::ImmutableRead(id) => {
                    let word = self.semantic_immutable(id, None)?;
                    let bits = instruction.result_ty.and_then(MirType::integer_bits).unwrap_or(256);
                    self.frame().values[instruction.result().expect("immutable read")] =
                        Some(RuntimeValue::Word(word & (U256::MAX >> (256 - bits))));
                    return ControlFlow::Continue(());
                }
                Semantics::ImmutableStore(id, value) => {
                    let word = frame.read(value)?;
                    self.semantic_immutable(id, Some(word))?;
                    return ControlFlow::Continue(());
                }
                Semantics::AbiEncode(mode, base, selector, args, layout) => {
                    let base = if let Some(v) = base { Some(frame.read(v)?) } else { None };
                    let selector = if let Some(v) = selector { Some(frame.read(v)?) } else { None };
                    let values = frame.read_values(args)?;
                    let mut sources = Vec::new();
                    let mut types = Vec::new();
                    for ((ty, &arg), value) in layout.types.iter().zip(args).zip(values) {
                        let location = body.value_slice_location(arg);
                        let memory_slice = location == Some(solar_mir::PtrLocation::Memory)
                            && matches!(
                                ty,
                                solar_mir::AbiType::FixedArray { .. }
                                    | solar_mir::AbiType::Tuple(_)
                            );
                        sources.push((value, memory_slice));
                        types.push(if let Some(location) = location {
                            ty.clone().at_location(location)
                        } else {
                            ty.clone()
                        });
                    }
                    let layout = solar_mir::AbiLayout::new(types);
                    let value =
                        self.semantic_abi_encode_values(mode, base, selector, &sources, &layout)?;
                    self.frame().values[instruction.result().expect("ABI encoding result")] =
                        Some(value);
                    return ControlFlow::Continue(());
                }
                Semantics::AbiDecode(data, layout) => {
                    let input = frame.read_value(data)?;
                    let location =
                        body.value_slice_location(data).unwrap_or(solar_mir::PtrLocation::Memory);
                    let value = self.semantic_abi_values(input, location, layout)?;
                    if let Some(result) = instruction.result() {
                        self.frame().values[result] = value;
                    }
                    return ControlFlow::Continue(());
                }
                Semantics::ValidateAbi(value) => {
                    let word = frame.read(value)?;
                    self.semantic_validate_abi(body, value, word)?;
                    return ControlFlow::Continue(());
                }
                Semantics::Packed(parts, hash) => {
                    if parts.iter().any(|part| {
                        matches!(
                            part,
                            solar_mir::PackedPart::Bytes { .. }
                                | solar_mir::PackedPart::Array { .. }
                        )
                    }) {
                        let word = self.semantic_packed_references(body, parts, hash)?;
                        self.frame().values[instruction.result().expect("packed result")] =
                            Some(RuntimeValue::Word(word));
                        return ControlFlow::Continue(());
                    }
                    let mut values = Vec::new();
                    for part in parts {
                        if let solar_mir::PackedPart::Scalar { value, .. } = part {
                            values.push(frame.read(*value)?);
                        }
                    }
                    let word = self.semantic_packed(parts, &values, hash)?;
                    self.frame().values[instruction.result().expect("packed result")] =
                        Some(RuntimeValue::Word(word));
                    return ControlFlow::Continue(());
                }
                Semantics::Opcode("gas", _) => {
                    if !self.gas_window(body, inst) {
                        return ControlFlow::Break(Outcome::Unsupported(
                            "GAS requires an isolated opcode window",
                        ));
                    }
                    let budget = self.placement(PlacementRead::GasBefore)?;
                    let price = U256::from(
                        op::definition(op::GAS)
                            .expect("GAS opcode")
                            .gas
                            .gas(self.semantic.expect("semantic version")),
                    );
                    if budget < price {
                        return ControlFlow::Break(Outcome::Invalid);
                    }
                    self.frame().values[instruction.result().expect("gas result")] =
                        Some(RuntimeValue::Word(budget - price));
                    return ControlFlow::Continue(());
                }
                Semantics::ValidateStorageBytes(header) => {
                    Self::bytes_header(frame.read(header)?)?;
                    return ControlFlow::Continue(());
                }
                Semantics::LoadStorageBytes(slot) => {
                    let slot = frame.read(slot)?;
                    let pointer = self.load_storage_bytes(slot)?;
                    self.frame().values[instruction.result().expect("storage bytes result")] =
                        Some(RuntimeValue::Word(pointer));
                    return ControlFlow::Continue(());
                }
                Semantics::StoreStorageBytes(slot, object) => {
                    let (slot, object) = (frame.read(slot)?, frame.read(object)?);
                    return self.store_storage_bytes(slot, object, None);
                }
                Semantics::StoreStorageBytesLiteral(slot, bytes) => {
                    let slot = frame.read(slot)?;
                    return self.store_storage_bytes(slot, U256::ZERO, Some(bytes));
                }
                Semantics::ClearStorageWords(slot, first, last) => {
                    let (slot, first, last) =
                        (frame.read(slot)?, frame.read(first)?, frame.read(last)?);
                    return self.clear_storage_words(slot, first, last);
                }
                Semantics::LoadStorageArray(slot, element, variants) => {
                    let slot = frame.read(slot)?;
                    let pointer = self.load_storage_array(slot, element, variants)?;
                    self.frame().values[instruction.result().expect("storage array result")] =
                        Some(RuntimeValue::Word(pointer));
                    return ControlFlow::Continue(());
                }
                Semantics::FrameLoad(offset, mode, kind) => {
                    let address = self.semantic_frame_address(body, offset, mode, kind)?;
                    let value = self.semantic_frame_load(address, kind)?;
                    self.frame().values[instruction.result().expect("frame load result")] =
                        Some(value);
                    return ControlFlow::Continue(());
                }
                Semantics::FrameStore(offset, mode, kind, value) => {
                    let value = frame.read_value(value)?;
                    let address = self.semantic_frame_address(body, offset, mode, kind)?;
                    return self.semantic_frame_store(address, kind, value);
                }
                Semantics::AddressCall(name, address, input, gas, value) => {
                    let Some(gas) = gas else {
                        return ControlFlow::Break(Outcome::Unsupported(
                            "implicit address-call gas requires caller pricing",
                        ));
                    };
                    let value =
                        if let Some(value) = value { Some(frame.read(value)?) } else { None };
                    let (address, input, gas) =
                        (frame.read(address)?, frame.read_value(input)?, frame.read(gas)?);
                    let opcode = op::from_mnemonic(name).expect("schema message opcode");
                    let result = self.semantic_address_call(opcode, address, input, gas, value)?;
                    self.frame().values[instruction.result().expect("address call result")] =
                        Some(RuntimeValue::Word(result));
                    return ControlFlow::Continue(());
                }
                Semantics::Call(Callee::Builtin(Builtin::ReturndataBytes), []) => {
                    let pointer = self.semantic_returndata_bytes()?;
                    self.frame().values[instruction.result().expect("returndata object")] =
                        Some(RuntimeValue::Word(pointer));
                    return ControlFlow::Continue(());
                }
                Semantics::MappingSlotBytes(key, slot) => {
                    let key = frame.read_value(key)?;
                    let slot = frame.read(slot)?;
                    let (source, length, calldata) = match key {
                        RuntimeValue::Word(object) => {
                            let length = limit(self.memory.load(object))?;
                            (object.wrapping_add(U256::from(32)), length, false)
                        }
                        RuntimeValue::Aggregate(fields) if fields.len() == 2 => {
                            let [RuntimeValue::Word(source), RuntimeValue::Word(length)] =
                                fields.as_slice()
                            else {
                                return ControlFlow::Break(Outcome::Unsupported(
                                    "undefined mapping bytes key",
                                ));
                            };
                            (*source, *length, true)
                        }
                        _ => {
                            return ControlFlow::Break(Outcome::Unsupported(
                                "invalid mapping bytes key",
                            ));
                        }
                    };
                    let scratch = limit(self.memory.load(U256::from(64)))?;
                    self.burn_words(length)?;
                    if calldata {
                        self.world_opcode(op::CALLDATACOPY, &[scratch, source, length])?;
                    } else {
                        limit(self.memory.copy(scratch, source, length))?;
                    }
                    limit(self.memory.store(scratch.wrapping_add(length), slot))?;
                    let count = length.wrapping_add(U256::from(32));
                    self.burn_words(count)?;
                    let bytes = limit(self.memory.read(scratch, count))?;
                    self.frame().values[instruction.result().expect("mapping result")] =
                        Some(RuntimeValue::Word(U256::from_be_bytes(keccak256(bytes).0)));
                    return ControlFlow::Continue(());
                }
                Semantics::InsertValue(ty, aggregate, index, value) => {
                    if let Err(e) = values::widths(self.machine.module, ty) {
                        return ControlFlow::Break(Outcome::Unsupported(e));
                    }
                    let fields = values::fields(self.machine.module, ty).map_or_else(
                        |e| ControlFlow::Break(Outcome::Unsupported(e)),
                        ControlFlow::Continue,
                    )?;
                    let mut fields = match frame.read_value(aggregate)? {
                        RuntimeValue::Undefined => vec![RuntimeValue::Undefined; fields.len()],
                        RuntimeValue::Aggregate(values) if values.len() == fields.len() => values,
                        _ => {
                            return ControlFlow::Break(Outcome::Unsupported(
                                "MIR aggregate shape mismatch",
                            ));
                        }
                    };
                    let value = frame.read_value(value)?;
                    let Some(field) = fields.get_mut(index as usize) else {
                        return ControlFlow::Break(Outcome::Unsupported("MIR aggregate index"));
                    };
                    *field = value;
                    let value = RuntimeValue::Aggregate(fields);
                    self.burn(value.nodes())?;
                    self.frame().values[instruction.result().expect("aggregate result")] =
                        Some(value);
                    return ControlFlow::Continue(());
                }
                Semantics::ExtractValue(ty, aggregate, index) => {
                    if let Err(e) = values::widths(self.machine.module, ty) {
                        return ControlFlow::Break(Outcome::Unsupported(e));
                    }
                    let value = match frame.read_value(aggregate)? {
                        RuntimeValue::Undefined => RuntimeValue::Undefined,
                        RuntimeValue::Aggregate(fields) => fields
                            .get(index as usize)
                            .cloned()
                            .ok_or(Outcome::Unsupported("MIR aggregate index"))
                            .map_or_else(ControlFlow::Break, ControlFlow::Continue)?,
                        _ => {
                            return ControlFlow::Break(Outcome::Unsupported(
                                "MIR aggregate shape mismatch",
                            ));
                        }
                    };
                    self.burn(value.nodes())?;
                    self.frame().values[instruction.result().expect("aggregate result")] =
                        Some(value);
                    return ControlFlow::Continue(());
                }
                Semantics::MakeSlice(_, ptr, len) => {
                    let value = RuntimeValue::Aggregate(vec![
                        RuntimeValue::Word(frame.read(ptr)?),
                        RuntimeValue::Word(frame.read(len)?),
                    ]);
                    self.burn(value.nodes())?;
                    self.frame().values[instruction.result().expect("slice result")] = Some(value);
                    return ControlFlow::Continue(());
                }
                Semantics::Select(condition, yes, no) => {
                    let value = frame.read_value(if frame.read(condition)?.is_zero() {
                        no
                    } else {
                        yes
                    })?;
                    self.burn(value.nodes())?;
                    self.frame().values[instruction.result().expect("select result")] = Some(value);
                    return ControlFlow::Continue(());
                }
                _ => {}
            }
        }
        let result = match semantics {
            Semantics::HeapFloor if self.transaction.is_some() && self.semantic.is_none() => {
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
                let mut words = frame.read_all(&operands)?;
                if self.semantic.is_some() {
                    let opcode = op::from_mnemonic(name);
                    for (index, (value, word)) in operands.iter().zip(&mut words).enumerate() {
                        if !matches!(opcode, Some(op::SLT | op::SGT | op::SDIV | op::SMOD))
                            && !(opcode == Some(op::SAR) && index == 1)
                        {
                            continue;
                        }
                        let bits =
                            body.value_ty(*value).and_then(|ty| ty.integer_bits()).unwrap_or(256);
                        *word = sign_extend(*word, bits);
                    }
                }
                match self.opcode(mnemonic, name, &words)? {
                    Some(word) => {
                        if self.semantic.is_some() && name == "clz" {
                            let bits = body
                                .value_ty(operands[0])
                                .and_then(|ty| ty.integer_bits())
                                .unwrap_or(256);
                            word - U256::from(256 - bits)
                        } else {
                            word
                        }
                    }
                    None => return ControlFlow::Continue(()),
                }
            }
            Semantics::Call(&Callee::Function(id), args) => {
                let args = if self.semantic.is_some() {
                    frame.read_values(args)?
                } else {
                    frame.read_all(args)?.into_iter().map(RuntimeValue::Word).collect()
                };
                let Some(callee) = self.machine.body(id) else {
                    return ControlFlow::Break(Outcome::Unsupported("call to an unknown function"));
                };
                self.call(callee, args, instruction.result())?;
                self.frame().caller_inst = Some(inst);
                return ControlFlow::Continue(());
            }
            Semantics::Call(Callee::Builtin(Builtin::Require(kind)), args)
                if self.semantic.is_some() =>
            {
                self.semantic_require(body, kind, args)?;
                return ControlFlow::Continue(());
            }
            Semantics::Call(
                Callee::Builtin(
                    builtin @ (Builtin::Sha256 | Builtin::Ripemd160 | Builtin::EcRecover),
                ),
                args,
            ) if self.semantic.is_some() => {
                let args = frame.read_values(args)?;
                self.semantic_precompile(builtin, args.as_raw_slice())?
            }
            Semantics::Call(Callee::Builtin(builtin), args) if self.semantic.is_some() => {
                let words = frame.read_all(args)?;
                let Some(result) = self.semantic_builtin_words(builtin, &words)? else {
                    return ControlFlow::Continue(());
                };
                result
            }
            Semantics::Call(..) => return ControlFlow::Break(Outcome::Unsupported(mnemonic)),
            Semantics::DataCopy(data, dest, len) => {
                let (dest, len) = (frame.read(dest)?, frame.read(len)?);
                return self.data_copy(data, dest, len);
            }
            Semantics::Allocate(size, _)
                if self.transaction.is_some() && self.semantic.is_none() =>
            {
                let size = frame.read(size)?;
                self.allocate(frame.function, inst, size)?
            }
            Semantics::FreePointer if self.semantic.is_some() => {
                limit(self.memory.load(U256::from(EvmMemoryLayout::FMP_SLOT)))?
            }
            Semantics::Allocate(size, semantics) if self.semantic.is_some() => {
                let size = frame.read(size)?;
                if instruction.metadata.deferred_alloc() {
                    self.semantic_deferred_allocate(body, inst, size, semantics)?
                } else {
                    self.semantic_allocate(size, semantics)?
                }
            }
            Semantics::MemoryZero(offset, size) if self.semantic.is_some() => {
                let (offset, size) = (frame.read(offset)?, frame.read(size)?);
                self.burn_words(size)?;
                limit(self.memory.zero(offset, size))?;
                return ControlFlow::Continue(());
            }
            Semantics::MappingSlot(key, slot) if self.semantic.is_some() => {
                let words = [frame.read(key)?, frame.read(slot)?];
                self.fixed_slot(&words)?
            }
            Semantics::StorageArrayDataSlot(slot) if self.semantic.is_some() => {
                let slot = frame.read(slot)?;
                self.fixed_slot(&[slot])?
            }
            Semantics::StorageArrayElementSlot(slot, index, stride) if self.semantic.is_some() => {
                let slot = frame.read(slot)?;
                let offset = frame.read(index)?.wrapping_mul(U256::from(stride));
                self.fixed_slot(&[slot])?.wrapping_add(offset)
            }
            Semantics::Word(value) => frame.read(value)?,
            Semantics::PointerAdd(ptr, offset) if self.semantic.is_some() => {
                frame.read(ptr)?.wrapping_add(frame.read(offset)?)
            }
            Semantics::LowBits(value, bits @ 1..=256) => {
                frame.read(value)? & (U256::MAX >> (256 - bits))
            }
            Semantics::NotEqual(a, b) => U256::from(frame.read(a)? != frame.read(b)?),
            Semantics::SignExtend(value, from @ 1..=256, to @ 1..=256)
                if self.semantic.is_some() && from <= to =>
            {
                sign_extend(frame.read(value)?, from) & (U256::MAX >> (256 - to))
            }
            Semantics::Checked(op, kind, a, b) if self.semantic.is_some() => {
                match checked_arithmetic(op, kind, frame.read(a)?, frame.read(b)?) {
                    Ok(result) => result,
                    Err(code) => {
                        return ControlFlow::Break(Outcome::Revert(panic_data(U256::from(code))));
                    }
                }
            }
            // Sign extensions, checked arithmetic, and builtin calls are semantic operations.
            _ => return ControlFlow::Break(Outcome::Unsupported(mnemonic)),
        };
        let Some(value) = instruction.result() else {
            return ControlFlow::Break(Outcome::Unsupported("result without a value"));
        };
        let result = if self.semantic.is_some() {
            let Some(bits @ 1..=256) = body.value_ty(value).and_then(scalar_bits) else {
                return ControlFlow::Break(Outcome::Unsupported("non-integer replay result"));
            };
            result & (U256::MAX >> (256 - bits))
        } else {
            result
        };
        self.frame().values[value] = Some(RuntimeValue::Word(result));
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
        // MIR memory copies are logical memmoves. `lower-mcopy` expands them on
        // pre-Cancun targets; only scheduled EVM MCOPY has opcode availability.
        if opcode != op::MCOPY && self.lacks(opcode) {
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
                let size = U256::from(self.memory.size * EvmMemoryLayout::WORD_SIZE);
                // Spills and frame writes MIR does not show may have grown EVM memory up to the
                // end of the backend's frames, which a transaction needs its layout to know.
                if self.transaction.is_some() && self.semantic.is_none() {
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
        if (matches!(opcode, op::SSTORE | op::TSTORE) || (op::LOG0..=op::LOG4).contains(&opcode))
            && self.world.as_ref().is_some_and(|world| world.host.is_static())
        {
            return ControlFlow::Break(Outcome::Invalid);
        }
        if self.semantic.is_some()
            && self.world.is_some()
            && matches!(
                opcode,
                op::CALL
                    | op::CALLCODE
                    | op::DELEGATECALL
                    | op::STATICCALL
                    | op::CREATE
                    | op::CREATE2
            )
        {
            return ControlFlow::Continue(Some(self.external_message(opcode, operands)?));
        }
        // Operations on memory come first, while no borrow of the world is live.
        match (opcode, operands) {
            (op::CODECOPY, &[dest, offset, len]) if self.semantic.is_some() => {
                if len.is_zero() {
                    return ControlFlow::Continue(None);
                }
                let Some(bytes) = self.world.as_mut().and_then(|w| w.host.code_image()) else {
                    return ControlFlow::Break(Outcome::Unsupported(
                        "missing explicit MIR code image",
                    ));
                };
                self.burn_words(len)?;
                let data = (0..len.to::<u64>())
                    .map(|i| calldata_byte(&bytes, offset, i))
                    .collect::<Vec<_>>();
                limit(self.memory.write(dest, &data))?;
                return ControlFlow::Continue(None);
            }
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
            (op::EXTCODECOPY, &[address, destination, offset, length]) => {
                self.burn_words(length)?;
                let Some(bytes) = self.world.as_mut().and_then(|world| world.host.code(address))
                else {
                    return ControlFlow::Break(unsupported());
                };
                let bytes = (0..length.to::<u64>())
                    .map(|index| calldata_byte(&bytes, offset, index))
                    .collect::<Vec<_>>();
                limit(self.memory.write(destination, &bytes))?;
                return ControlFlow::Continue(None);
            }
            (op::RETURNDATACOPY, &[destination, offset, len]) => {
                let Some(world) = &self.world else {
                    return ControlFlow::Break(unsupported());
                };
                let data = &world.effects.return_data;
                if offset > U256::from(data.len()) || len > U256::from(data.len()) - offset {
                    return ControlFlow::Break(Outcome::ReturndataOutOfBounds);
                }
                let bytes = data[offset.to::<usize>()..(offset + len).to::<usize>()].to_vec();
                self.burn_words(len)?;
                limit(self.memory.write(destination, &bytes))?;
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
            (op::RETURNDATASIZE, &[]) if transaction.is_some() => {
                Some(U256::from(world.effects.return_data.len()))
            }
            _ if HOST_OPCODES.contains(&opcode) => match world.host.read(opcode, operands) {
                Some(word) => Some(word),
                None => return ControlFlow::Break(unsupported()),
            },
            _ => return ControlFlow::Break(unsupported()),
        })
    }

    /// Evaluates external MIR operands directly. Memory expansion belongs to the caller;
    /// child execution and journal commit/rollback belong to the explicitly supplied host.
    fn external_message(&mut self, opcode: u8, args: &[U256]) -> ControlFlow<Outcome, U256> {
        let world = self.world.as_ref().expect("external messages require a host");
        let creation = matches!(opcode, op::CREATE | op::CREATE2);
        let explicit_value = matches!(opcode, op::CALL | op::CALLCODE);
        let value = if creation {
            args[0]
        } else if explicit_value {
            args[2]
        } else {
            U256::ZERO
        };
        if world.host.is_static() && (creation || opcode == op::CALL && !value.is_zero()) {
            return ControlFlow::Break(Outcome::Invalid);
        }
        let start = if creation {
            1
        } else if explicit_value {
            3
        } else {
            2
        };
        let (offset, size) = (args[start], args[start + 1]);
        if creation
            && self.semantic.is_some_and(|version| version >= EvmVersion::Shanghai)
            && size > U256::from(49152)
        {
            return ControlFlow::Break(Outcome::Invalid);
        }
        let input = limit(self.memory.read(offset, size))?;
        self.burn_words(size)?;
        let output = if creation {
            None
        } else {
            let (offset, size) = (args[start + 2], args[start + 3]);
            limit(self.memory.access(offset, size))?;
            Some((offset, size))
        };
        let message = if creation {
            ExternalMessage::Create {
                value,
                salt: (opcode == op::CREATE2).then(|| args[3]),
                initcode: &input,
            }
        } else {
            ExternalMessage::Call { opcode, gas: args[0], address: args[1], value, input: &input }
        };
        let world = self.world.as_mut().expect("external messages require a host");
        let storage = world.effects.storage.iter().map(|(&k, &v)| (k, v)).collect::<Vec<_>>();
        let transient = world.effects.transient.iter().map(|(&k, &v)| (k, v)).collect::<Vec<_>>();
        let pending =
            PendingChanges { storage: &storage, transient: &transient, logs: &world.effects.logs };
        let result = match world.host.message(message, pending) {
            Ok(result) => result,
            Err(reason) => return ControlFlow::Break(Outcome::Unsupported(reason)),
        };
        world.effects.storage.clear();
        world.effects.transient.clear();
        world.effects.logs.clear();
        if let Some((offset, size)) = output {
            let copied = result.return_data.len().min(size.to::<usize>());
            limit(self.memory.write(offset, &result.return_data[..copied]))?;
        }
        world.effects.return_data = result.return_data;
        ControlFlow::Continue(result.value)
    }

    /// Hashes word keys and retains their scratch writes and memory expansion.
    fn fixed_slot(&mut self, words: &[U256]) -> ControlFlow<Outcome, U256> {
        for (index, &word) in words.iter().enumerate() {
            limit(self.memory.store(U256::from(index * 32), word))?;
        }
        let bytes = limit(self.memory.read(U256::ZERO, U256::from(words.len() * 32)))?;
        ControlFlow::Continue(U256::from_be_bytes(keccak256(bytes).0))
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

    /// Execute the allocation contract against the caller's explicit memory. Reservation alone
    /// does not touch the allocated bytes; zeroing does. The FMP update precedes initialization,
    /// even when an explicitly supplied cursor aliases the FMP word itself.
    fn semantic_allocate(
        &mut self,
        size: U256,
        semantics: AllocationSemantics,
    ) -> ControlFlow<Outcome, U256> {
        let slot = U256::from(EvmMemoryLayout::FMP_SLOT);
        let pointer = limit(self.memory.load(slot))?;
        let (next, wrapped) = pointer.overflowing_add(size);
        if wrapped || next > U256::from(EvmMemoryLayout::MAX_ALLOCATION_END) {
            return ControlFlow::Break(if semantics.failure == AllocationFailure::Checked {
                Outcome::Revert(panic_data(U256::from(0x41)))
            } else {
                Outcome::Unsupported("infallible allocation validity precondition")
            });
        }
        // mstore 64, next
        // memory_zero pointer, size  (when zeroed)
        limit(self.memory.store(slot, next))?;
        if semantics.initialization == AllocationInitialization::Zeroed {
            self.burn_words(size)?;
            limit(self.memory.zero(pointer, size))?;
        }
        ControlFlow::Continue(pointer)
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
                if self.semantic.is_some() {
                    let values = frame.read_values(values)?;
                    self.burn(values.iter().map(RuntimeValue::nodes).sum())?;
                    let frame = self.frames.pop().expect("a run always has a frame");
                    if self.frames.is_empty() {
                        let mut words = Vec::new();
                        for (value, &ty) in values.iter().zip(body.return_components()) {
                            if let Err(e) = value.flatten(self.machine.module, ty, &mut words) {
                                return ControlFlow::Break(Outcome::Unsupported(e));
                            }
                        }
                        return ControlFlow::Break(Outcome::Return(words.into_iter().collect()));
                    }
                    self.publish_semantic_returns(frame.return_owner, values.as_raw_slice())?;
                    if let Some(result) = frame.result {
                        if values.is_empty() {
                            return ControlFlow::Break(Outcome::Unsupported(
                                "call without a result",
                            ));
                        }
                        self.frame().values[result] =
                            Some(values.into_iter().next().expect("one result"));
                    }
                    return ControlFlow::Continue(());
                }
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
                                self.frame().values[*value] = Some(RuntimeValue::Word(word));
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
                    caller.values[result] = Some(RuntimeValue::Word(value));
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
                let args = if self.semantic.is_some() {
                    frame.read_values(args)?
                } else {
                    frame.read_all(args)?.into_iter().map(RuntimeValue::Word).collect()
                };
                let Some(callee) = self.machine.body(*function) else {
                    return ControlFlow::Break(Outcome::Unsupported("call to an unknown function"));
                };
                let frame = self.frames.pop().expect("a run always has a frame");
                self.call(callee, args, frame.result)?;
                self.frame().return_owner = frame.return_owner;
                self.frame().caller_inst = frame.caller_inst;
                ControlFlow::Continue(())
            }
            &Terminator::SelfDestruct { recipient } if self.semantic.is_some() => {
                let beneficiary = frame.read(recipient)?;
                self.semantic_destroy(beneficiary)?;
                ControlFlow::Break(Outcome::Stop)
            }
            Terminator::SelfDestruct { .. } => {
                ControlFlow::Break(Outcome::Unsupported("selfdestruct"))
            }
            Terminator::RevertReturndata => {
                if self.semantic.is_none() {
                    return ControlFlow::Break(Outcome::Unsupported("revert_returndata"));
                }
                let data = if self.lacks(op::RETURNDATASIZE) {
                    Vec::new()
                } else {
                    self.world
                        .as_ref()
                        .expect("semantic replay has a host")
                        .effects
                        .return_data
                        .clone()
                };
                ControlFlow::Break(Outcome::Revert(data))
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
        let mut incoming = SmallVec::<[(ValueId, RuntimeValue); 8]>::new();
        for &inst in &block.instructions {
            let instruction = body.inst(inst);
            let InstKind::Phi(inputs) = &instruction.kind else { break };
            let (Some(&(_, value)), Some(result)) =
                (inputs.iter().find(|&&(block, _)| block == from), instruction.result())
            else {
                return ControlFlow::Break(Outcome::Unsupported("phi without an input"));
            };
            let value = if self.semantic.is_some() {
                frame.read_value(value)?
            } else {
                RuntimeValue::Word(frame.read(value)?)
            };
            incoming.push((result, value));
        }
        self.burn(incoming.iter().map(|(_, v)| v.nodes()).sum())?;
        let frame = self.frame();
        frame.next = incoming.len();
        for (result, value) in incoming {
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

/// A memory pointer carries one address word, without validity or provenance assumptions.
pub(crate) fn scalar_bits(ty: MirType) -> Option<u32> {
    if matches!(ty, MirType::Ptr(_)) { Some(256) } else { ty.integer_bits() }
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

fn sign_extend(value: U256, bits: u32) -> U256 {
    if bits < 256 && value.bit((bits - 1) as usize) { value | (U256::MAX << bits) } else { value }
}

fn panic_data(code: U256) -> Vec<u8> {
    let mut bytes = vec![0x4e, 0x48, 0x7b, 0x71];
    bytes.extend(code.to_be_bytes::<32>());
    bytes
}

fn checked_word(op: CheckedOp, kind: ArithmeticKind, a: U256, b: U256) -> Option<U256> {
    let bits = match kind {
        ArithmeticKind::Unsigned(bits) | ArithmeticKind::Signed(bits) => u32::from(bits),
    };
    let mask = U256::MAX >> (256 - bits);
    match kind {
        ArithmeticKind::Unsigned(_) => {
            let result = match op {
                CheckedOp::Add => a.checked_add(b),
                CheckedOp::Sub => a.checked_sub(b),
                CheckedOp::Mul => a.checked_mul(b),
                _ => None,
            }?;
            (result & mask == result).then_some(result)
        }
        ArithmeticKind::Signed(_) => {
            let a = I256::from_raw(sign_extend(a, bits));
            let b = I256::from_raw(sign_extend(b, bits));
            let result = match op {
                CheckedOp::Add => a.checked_add(b),
                CheckedOp::Sub => a.checked_sub(b),
                CheckedOp::Mul => a.checked_mul(b),
                _ => None,
            }?
            .into_raw();
            (sign_extend(result & mask, bits) == result).then_some(result & mask)
        }
    }
}

fn checked_arithmetic(op: CheckedOp, kind: ArithmeticKind, a: U256, b: U256) -> Result<U256, u8> {
    let bits = match kind {
        ArithmeticKind::Unsigned(bits) | ArithmeticKind::Signed(bits) => u32::from(bits),
    };
    let mask = U256::MAX >> (256 - bits);
    match op {
        CheckedOp::Add | CheckedOp::Sub | CheckedOp::Mul => {
            checked_word(op, kind, a, b).ok_or(0x11)
        }
        CheckedOp::Div | CheckedOp::WrappingDiv | CheckedOp::Rem => {
            if b.is_zero() {
                return Err(0x12);
            }
            let signed = matches!(kind, ArithmeticKind::Signed(_));
            if op == CheckedOp::Div && signed && a == U256::ONE << (bits - 1) && b == mask {
                return Err(0x11);
            }
            let opcode = match (signed, op) {
                (true, CheckedOp::Rem) => op::SMOD,
                (false, CheckedOp::Rem) => op::MOD,
                (true, _) => op::SDIV,
                (false, _) => op::DIV,
            };
            let operands =
                if signed { [sign_extend(a, bits), sign_extend(b, bits)] } else { [a, b] };
            Ok(op::eval_opcode(opcode, &operands).expect("division opcode") & mask)
        }
        CheckedOp::Pow => {
            let mut result = U256::ONE;
            let mut base = a;
            let mut exponent = b;
            while !exponent.is_zero() {
                if exponent.bit(0) {
                    result = checked_word(CheckedOp::Mul, kind, result, base).ok_or(0x11)?;
                }
                exponent >>= 1;
                if !exponent.is_zero() {
                    base = checked_word(CheckedOp::Mul, kind, base, base).ok_or(0x11)?;
                }
            }
            Ok(result)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{B256, KECCAK256_EMPTY};
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
            let empty = U256::from_be_bytes(KECCAK256_EMPTY.0);
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
            B256::with_last_byte(3).as_slice(),
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

fn @echo() {
  bb0:
    v900 = calldataload 4
    v0 = alloc 32 !metadata(deferred_alloc)
    datacopy literal_0, v0, 2
    v1 = mload v0
    v2 = eq v900, 0
    jumpi v2, bb1, bb2
  bb1:
    revert 0, 0
  bb2:
    mstore 0, v900
    mstore 32, v1
    returndata 0, 64
}

fn @entry() [entry] {
  bb0:
    tail_call @echo
}
";
        // A runtime entry reads its argument from calldata after the selector.
        let calldata = [&[0, 0, 0, 1][..], B256::with_last_byte(42).as_slice()].concat();
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

    #[test]
    fn checked_replay_word_boundaries() {
        for bits in [160, 256] {
            let unsigned = ArithmeticKind::Unsigned(bits);
            let signed = ArithmeticKind::Signed(bits);
            let mask = U256::MAX >> (256 - u32::from(bits));
            let minimum = U256::ONE << (bits - 1);
            let maximum = minimum - U256::ONE;
            assert_eq!(checked_word(CheckedOp::Add, unsigned, mask, U256::ONE), None);
            assert_eq!(checked_word(CheckedOp::Sub, unsigned, U256::ZERO, U256::ONE), None);
            assert_eq!(checked_word(CheckedOp::Mul, unsigned, mask, U256::from(2)), None);
            assert_eq!(checked_word(CheckedOp::Add, signed, maximum, U256::ONE), None);
            assert_eq!(checked_word(CheckedOp::Sub, signed, minimum, U256::ONE), None);
            assert_eq!(checked_word(CheckedOp::Mul, signed, minimum, mask), None);
            assert_eq!(checked_word(CheckedOp::Add, signed, mask, U256::ONE), Some(U256::ZERO));
            assert_eq!(checked_word(CheckedOp::Mul, signed, minimum, U256::ONE), Some(minimum));
        }
    }
}
