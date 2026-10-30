//! Concrete execution of EVM IR.
//!
//! [`Machine::transact`] runs a transaction on the program an EVM IR module holds, as the EVM runs
//! the bytecode the module assembles to: it enters the first block with an empty stack, zeroed
//! memory, and the transaction's calldata, and runs until a terminal opcode, an exceptional halt,
//! or the end of the code ends it. A [`Host`] answers what the program reads from its context and
//! what storage held before the transaction, and the run reports how it ended, the events it
//! logged, and the storage it wrote. It is a testing oracle, not part of code generation.
//!
//! # Model
//!
//! The stack and memory are the EVM's. The stack holds at most 1024 words, and each logical stack
//! operation moves words as the `DUP`, `SWAP`, or `EXCHANGE` it lowers to. Pure opcodes evaluate
//! through the opcode table's word semantics, [`eval_opcode`], which constant folding shares, so
//! execution and folding have one definition of each opcode. Memory is byte-addressed: an access
//! with a nonzero length grows it to the word that holds its last byte, a zero-length access
//! ignores its offset, and `MSIZE` reports its exact size. The run also reads calldata, persistent
//! and transient storage, and the context values its host answers, and records the logs it emits.
//! It makes no calls, so its return data is always empty.
//!
//! Instructions run in order within a block. A block without a terminator falls through to the
//! next block in layout order, and past the last block the code ends, which stops. A raw `jumpi`
//! and a dynamic `jump` take their target from the stack.
//!
//! An opcode the target lacks, such as `shl` before Constantinople or `revert` before Byzantium,
//! halts as undefined, as it does in the EVM, and so does a stack operation that only the target's
//! missing `DUPN`, `SWAPN`, or `EXCHANGE` could encode.
//!
//! # Code layout
//!
//! EVM IR does not fix code addresses: the assembler places blocks and data after the EVM IR
//! pipeline. A run places them itself, on a [`Layout`]: blocks get increasing addresses in layout
//! order, and the known data follows them, with the data the runtime program must end with last.
//! A pushed block, `push_data`, and `CODESIZE` read that image, and `CODECOPY` copies data from it
//! and zeros past its end, but never the bytes of code, which only assembly decides. A dynamic
//! jump must land on a block whose address the program pushes. A program that reveals the
//! layout, such as by returning, storing, or hashing an address, or by copying past a data entry
//! into whatever follows it, can get another result on another layout:
//! [`transact`](crate::transact) runs every transaction on two layouts and reports a difference as
//! unsupported. One layout ends the code after a single byte, so its blocks share addresses with
//! the data, which only jumps tell apart. The other starts the blocks past 1 MiB, yet short enough
//! for a copy of the code size to fit in memory. This check detects differences between those
//! layouts; agreement does not prove an arbitrary address-dependent expression independent of
//! layout.
//!
//! `CODESIZE` stays unknown: equality with the assembled size could be false on both layouts.
//! The run tracks its unmodified value through stack operations. It may be discarded, or used
//! as `CODECOPY`'s source offset when its other operands are not code sizes, which copies zeros.
//! All other consumers, including arithmetic and comparisons, are unsupported before their result
//! can reach memory, storage, control flow, or a host read. NOTE: This also rejects arithmetic
//! whose result would cancel the code size; it does not try to prove such cancellations.
//!
//! # Limits
//!
//! A run has a fuel budget, one unit per operation and per copied, hashed, logged, or returned
//! word. Memory ends at [`MEMORY_LIMIT`], past which the EVM runs out of gas under any block gas
//! limit. Exceeding either ends the run with [`Outcome::Limit`]. What the interpreter does not
//! model ends it with [`Outcome::Unsupported`]: calls, contract creation, `selfdestruct`, `gas`,
//! `pc`, `extcodecopy`, immutables, library addresses, whether pushed or linked into data, the
//! bytes of code and of embedded contract code, a jump to an address that no pushed block has, and
//! an indexed jump outside its table. The interpreter relies on no verification: it checks the
//! stack as it runs, so a module the parser accepts never makes it panic.

use alloy_primitives::{U256, keccak256};
use smallvec::SmallVec;
use solar_config::EvmVersion;
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::{IndexVec, index_vec},
    map::FxHashMap,
};
use solar_evmir::{
    ir::{BlockId, Instruction, Module, Terminator, TerminatorKind},
    link::{DataBytes, DataId},
    op::{self, MAX_STACK_DEPTH, StackOp, WORD_BYTES, eval_opcode},
};
use std::{borrow::Cow, fmt, ops::Range};

/// Bytes of memory a run may use: growing memory to 16 MiB costs about 538 million gas, more than
/// any block holds.
pub(crate) const MEMORY_LIMIT: u64 = 1 << 24;

/// Where a run places the data of embedded contract code, whose bytes are unknown before linking:
/// far past any code, so that no other read reaches it. Each entry gets [`DEFERRED_DATA_SPAN`]
/// bytes, more than any linked data holds.
const DEFERRED_DATA: u64 = 1 << 40;

/// The bytes of an address, which linking writes into data where a library relocation points.
const ADDRESS_BYTES: u64 = 20;

/// The bytes a run reserves for each entry of embedded contract code.
const DEFERRED_DATA_SPAN: u64 = 1 << 32;

/// The opcodes whose values a run asks its [`Host`] for.
pub(crate) const HOST_OPCODES: [u8; 20] = [
    op::ADDRESS,
    op::BALANCE,
    op::ORIGIN,
    op::CALLER,
    op::CALLVALUE,
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

/// Answers what a run reads from outside the program: its context, and the chain's state before
/// the transaction.
pub trait Host {
    /// Returns what `opcode` reads for these operands from the transaction's context or the
    /// chain's state, such as `CALLER`, `TIMESTAMP`, or `BALANCE`, or `None` when the host does
    /// not model it.
    fn read(&mut self, opcode: u8, operands: &[U256]) -> Option<U256>;

    /// Returns the value a persistent storage slot of the running contract holds before the
    /// transaction.
    fn storage(&mut self, slot: U256) -> U256;

    /// Returns the value a transient storage slot holds when the run starts: zero for a
    /// transaction, which starts with empty transient storage.
    fn transient(&mut self, slot: U256) -> U256 {
        let _ = slot;
        U256::ZERO
    }
}

/// An event a run logged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Log {
    /// The topics, in order.
    pub topics: Vec<U256>,
    /// The data.
    pub data: Vec<u8>,
}

/// Why a run halted exceptionally, which consumes all of its gas and returns nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Halt {
    /// It reached `invalid`.
    Invalid,
    /// It reached an opcode the target does not define, such as `revert` before Byzantium.
    UndefinedOpcode(u8),
    /// An operation needed more words than the stack held.
    StackUnderflow,
    /// The stack outgrew its 1024 words.
    StackOverflow,
    /// `returndatacopy` read past the return data, which is always empty.
    ReturnDataOutOfBounds,
}

impl fmt::Display for Halt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Invalid => f.write_str("`invalid`"),
            Self::UndefinedOpcode(opcode) => match op::mnemonic(opcode) {
                Some(mnemonic) => write!(f, "`{mnemonic}`, which the target does not define"),
                None => write!(f, "undefined opcode 0x{opcode:02x}"),
            },
            Self::StackUnderflow => f.write_str("stack underflow"),
            Self::StackOverflow => f.write_str("stack overflow"),
            Self::ReturnDataOutOfBounds => f.write_str("`returndatacopy` out of bounds"),
        }
    }
}

/// How a run ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// It returned this data with `RETURN`.
    Return(Vec<u8>),
    /// It stopped, by `STOP` or at the end of the code.
    Stop,
    /// It reverted with this data.
    Revert(Vec<u8>),
    /// It halted exceptionally.
    Halt(Halt),
    /// It exceeded a limit before it ended.
    Limit(Limit),
    /// It reached something the interpreter does not model, named here.
    Unsupported(Cow<'static, str>),
}

/// A limit a run exceeded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Limit {
    /// The fuel budget.
    Fuel,
    /// [`MEMORY_LIMIT`].
    Memory,
}

/// What a run wrote and logged. A run that does not succeed keeps none of it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Effects {
    /// The events, in order.
    pub(crate) logs: Vec<Log>,
    /// The persistent storage slots written, with their final values.
    pub(crate) storage: FxHashMap<U256, U256>,
    /// The transient storage slots written, with their final values.
    pub(crate) transient: FxHashMap<U256, U256>,
}

/// What a run did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Execution {
    pub(crate) outcome: Outcome,
    pub(crate) effects: Effects,
}

/// Where a run places a program's blocks and data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Layout {
    /// The address of the first block.
    first_block: u64,
    /// The distance between the addresses of consecutive blocks.
    block_stride: u64,
    /// Where the code ends and the data starts, unless one byte past the last block's address.
    code_end: Option<u64>,
    /// Whether the data goes in reverse declaration order, the data that ends the runtime program
    /// aside.
    reverse_data: bool,
}

impl Layout {
    /// A single byte of code, the least a program that reads its code size has, blocks at
    /// consecutive addresses from zero, and data in declaration order.
    pub(crate) const COMPACT: Self =
        Self { first_block: 0, block_stride: 1, code_end: Some(1), reverse_data: false };

    /// Blocks spread out from an odd address past 1 MiB, so the code is longer than any deployable
    /// contract's, and data in reverse declaration order.
    pub(crate) const MOVED: Self = Self {
        first_block: (1 << 20) + 0x2a1,
        block_stride: 3,
        code_end: None,
        reverse_data: true,
    };
}

/// Runs the transactions of one EVM IR module.
pub(crate) struct Machine<'m> {
    module: &'m Module,
    evm_version: EvmVersion,
    /// The blocks whose addresses the program pushes, which are the only ones a dynamic jump can
    /// reach.
    pushed: DenseBitSet<BlockId>,
}

impl<'m> Machine<'m> {
    /// Creates a machine for `module` targeting `evm_version`.
    pub(crate) fn new(module: &'m Module, evm_version: EvmVersion) -> Self {
        let mut pushed = DenseBitSet::new_empty(module.blocks.len());
        for inst in module.blocks.iter().flat_map(|block| &block.instructions) {
            if let Some(block) = inst.pushed_block() {
                pushed.insert(block);
            }
        }
        Self { module, evm_version, pushed }
    }

    /// Runs a transaction with `calldata` on `layout`, asking `host` for its context, within
    /// `fuel` units of work.
    pub(crate) fn transact(
        &self,
        calldata: &[u8],
        host: &mut dyn Host,
        layout: Layout,
        fuel: u64,
    ) -> Execution {
        let mut run = Run {
            machine: self,
            image: CodeImage::new(self.module, layout),
            host,
            calldata,
            stack: Vec::new(),
            code_sizes: DenseBitSet::new_empty(MAX_STACK_DEPTH),
            memory: Memory::default(),
            effects: Effects::default(),
            fuel,
        };
        let outcome = run.execute();
        let effects = match outcome {
            Outcome::Return(_) | Outcome::Stop => run.effects,
            _ => Effects::default(),
        };
        Execution { outcome, effects }
    }
}

/// The code a run sees: the addresses of blocks and data on its layout, and the data bytes,
/// which follow the code.
struct CodeImage {
    layout: Layout,
    /// The number of blocks.
    blocks: usize,
    /// Where the code ends and the data starts.
    code_end: u64,
    /// The address of each data entry.
    data: IndexVec<DataId, u64>,
    /// The known data, in address order from `code_end`.
    bytes: Vec<u8>,
    /// The ranges of the data that linking fills with library addresses, whose bytes are unknown.
    linked: Vec<Range<u64>>,
    /// The end of the embedded contract code, which starts at [`DEFERRED_DATA`].
    deferred_end: u64,
}

impl CodeImage {
    fn new(module: &Module, layout: Layout) -> Self {
        let blocks = module.blocks.len();
        // One byte past the last block's address, which its code occupies.
        let code_end =
            layout.code_end.unwrap_or(layout.first_block + blocks as u64 * layout.block_stride + 1);
        let ends_runtime = |id: &DataId| module.data[*id].emit_in_runtime;
        let mut order = module.data.indices().filter(|id| !ends_runtime(id)).collect::<Vec<_>>();
        if layout.reverse_data {
            order.reverse();
        }
        order.extend(module.data.indices().filter(ends_runtime));
        let mut image = Self {
            layout,
            blocks,
            code_end,
            data: index_vec![0; module.data.len()],
            bytes: Vec::new(),
            linked: Vec::new(),
            deferred_end: DEFERRED_DATA,
        };
        for id in order {
            let data = &module.data[id];
            image.data[id] = match &data.bytes {
                DataBytes::Known(bytes) => {
                    let address = code_end + image.bytes.len() as u64;
                    image.bytes.extend_from_slice(bytes);
                    image.linked.extend(data.library_relocations.iter().map(|relocation| {
                        let start = address + relocation.offset as u64;
                        start..start + ADDRESS_BYTES
                    }));
                    address
                }
                DataBytes::Deferred(_) => {
                    let address = image.deferred_end;
                    image.deferred_end += DEFERRED_DATA_SPAN;
                    address
                }
            };
        }
        image
    }

    /// Returns the address of `block`.
    fn block_address(&self, block: BlockId) -> U256 {
        U256::from(self.layout.first_block + block.index() as u64 * self.layout.block_stride)
    }

    /// Returns the block at `address`, if one starts there.
    fn block_at(&self, address: U256) -> Option<BlockId> {
        let offset = u64::try_from(address).ok()?.checked_sub(self.layout.first_block)?;
        if offset % self.layout.block_stride != 0 {
            return None;
        }
        let index = usize::try_from(offset / self.layout.block_stride).ok()?;
        (index < self.blocks).then(|| BlockId::from_usize(index))
    }

    /// Returns the size of the code, data included.
    fn size(&self) -> u64 {
        self.code_end + self.bytes.len() as u64
    }

    /// Copies the code at `offset` into `out`, with zeros past the end of the code.
    fn copy(&self, offset: U256, out: &mut [u8]) -> Result<(), Outcome> {
        if out.is_empty() {
            return Ok(());
        }
        let offset = u64::try_from(offset).unwrap_or(u64::MAX);
        if (DEFERRED_DATA..self.deferred_end).contains(&offset) {
            return Err(unsupported("copies the code of another contract"));
        }
        if offset >= self.size() {
            out.fill(0);
            return Ok(());
        }
        let Some(start) = offset.checked_sub(self.code_end) else {
            return Err(unsupported("copies code bytes, which only assembly decides"));
        };
        let end = offset.saturating_add(out.len() as u64);
        if self.linked.iter().any(|linked| linked.start < end && offset < linked.end) {
            return Err(unsupported("copies a library address, which linking sets"));
        }
        copy_padded(&self.bytes, U256::from(start), out);
        Ok(())
    }
}

/// Byte-addressed EVM memory.
#[derive(Default)]
struct Memory(Vec<u8>);

impl Memory {
    /// Returns the range of `len` bytes at `offset`, growing memory to the word that holds its last
    /// byte. An empty range leaves memory alone, whatever its offset.
    fn range(&mut self, offset: U256, len: U256) -> Result<Range<usize>, Outcome> {
        if len.is_zero() {
            return Ok(0..0);
        }
        let end = offset
            .checked_add(len)
            .filter(|&end| end <= U256::from(MEMORY_LIMIT))
            .ok_or(Outcome::Limit(Limit::Memory))?;
        let (start, end) = (offset.to::<usize>(), end.to::<usize>());
        let size = end.next_multiple_of(WORD_BYTES);
        if self.0.len() < size {
            self.0.resize(size, 0);
        }
        Ok(start..end)
    }

    /// Returns the word at `offset`.
    fn load(&mut self, offset: U256) -> Result<U256, Outcome> {
        let range = self.range(offset, U256::from(WORD_BYTES))?;
        Ok(U256::from_be_slice(&self.0[range]))
    }

    /// Writes `word` at `offset`.
    fn store(&mut self, offset: U256, word: U256) -> Result<(), Outcome> {
        let range = self.range(offset, U256::from(WORD_BYTES))?;
        self.0[range].copy_from_slice(&word.to_be_bytes::<32>());
        Ok(())
    }
}

/// What an operation does besides consuming its operands.
enum Effect {
    /// Nothing more: control goes on to the next operation.
    Next,
    /// It pushes this word.
    Push(U256),
    /// It jumps to this block.
    Jump(BlockId),
}

/// One transaction in progress.
struct Run<'r, 'm> {
    machine: &'r Machine<'m>,
    image: CodeImage,
    host: &'r mut dyn Host,
    calldata: &'r [u8],
    /// The stack, bottom first.
    stack: Vec<U256>,
    /// Stack slots holding an unmodified `CODESIZE`, which is unknown before assembly.
    code_sizes: DenseBitSet<usize>,
    memory: Memory,
    effects: Effects,
    fuel: u64,
}

impl<'m> Run<'_, 'm> {
    /// Runs blocks from the entry until the run ends.
    fn execute(&mut self) -> Outcome {
        if self.machine.module.blocks.is_empty() {
            return Outcome::Stop;
        }
        let mut block = BlockId::ENTRY;
        loop {
            match self.block(block) {
                Ok(next) => block = next,
                Err(outcome) => return outcome,
            }
        }
    }

    /// Runs `block` and returns the block control reaches next, or how the run ended.
    fn block(&mut self, block: BlockId) -> Result<BlockId, Outcome> {
        let module: &'m Module = self.machine.module;
        let body = &module.blocks[block];
        for inst in &body.instructions {
            if let Some(target) = self.instruction(inst)? {
                return Ok(target);
            }
        }
        match &body.terminator {
            Some(terminator) => self.terminator(block, terminator),
            None => self.fall_through(block),
        }
    }

    /// Returns the block after `block` in layout order, or ends the run at the end of the code.
    fn fall_through(&self, block: BlockId) -> Result<BlockId, Outcome> {
        self.machine.module.next_block(block).ok_or(Outcome::Stop)
    }

    /// Runs one instruction, returning the block it jumps to, if any.
    fn instruction(&mut self, inst: &Instruction) -> Result<Option<BlockId>, Outcome> {
        self.burn(1)?;
        if let Some(stack_op) = inst.as_stack_op() {
            self.stack_op(stack_op)?;
            return Ok(None);
        }
        if inst.is_encoded_push() {
            let value = self.pushed(inst);
            self.push(value?)?;
            return Ok(None);
        }
        self.opcode(inst.opcode)
    }

    /// Returns the word a push instruction pushes.
    fn pushed(&self, inst: &Instruction) -> Result<U256, Outcome> {
        if inst.immutable_push().is_some() {
            return Err(unsupported("reads an immutable, which deployment sets"));
        }
        if let Some(value) = inst.concrete_immediate() {
            return Ok(value);
        }
        if let Some(block) = inst.pushed_block() {
            return Ok(self.image.block_address(block));
        }
        if let Some(data) = inst.pushed_data() {
            let address =
                self.image.data.get(data.id).ok_or_else(|| unsupported("unknown data"))?;
            return Ok(U256::from(*address) + U256::from(data.offset));
        }
        if let Some(size) = inst.pushed_data_size() {
            let Some(DataBytes::Known(bytes)) =
                self.machine.module.data.get(size.data).map(|data| &data.bytes)
            else {
                return Err(unsupported("pushes the size of another contract's code"));
            };
            return Ok(size.value(bytes.len()));
        }
        if inst.pushed_library().is_some() {
            return Err(unsupported("pushes a library address, which linking sets"));
        }
        Err(unsupported("pushes an unresolved relocation"))
    }

    /// Runs a logical stack operation.
    fn stack_op(&mut self, stack_op: StackOp) -> Result<(), Outcome> {
        if !stack_op.is_valid() {
            return Err(unsupported("a stack operation no opcode encodes"));
        }
        // Past the 16th word, only the extended stack opcodes reach, which older targets lack.
        if stack_op.lowering(self.machine.evm_version).is_none() {
            return Err(Outcome::Halt(Halt::UndefinedOpcode(stack_op.ir_opcode())));
        }
        let depth = self.stack.len();
        if depth < stack_op.required_depth() {
            return Err(Outcome::Halt(Halt::StackUnderflow));
        }
        let top = depth - 1;
        match stack_op {
            StackOp::Dup(n) => {
                let source = depth - usize::from(n);
                let value = self.stack[source];
                let code_size = self.code_sizes.contains(source);
                self.push(value)?;
                if code_size {
                    self.code_sizes.insert(depth);
                }
            }
            StackOp::Swap(n) => {
                let other = top - usize::from(n);
                self.swap(top, other);
            }
            StackOp::Exchange(n, m) => {
                let (first, second) = (top - usize::from(n), top - usize::from(m));
                self.swap(first, second);
            }
            StackOp::Pop => {
                self.stack.pop();
                self.code_sizes.remove(top);
            }
        }
        Ok(())
    }

    /// Runs the terminator of `block`, returning the block control reaches next.
    fn terminator(&mut self, block: BlockId, terminator: &Terminator) -> Result<BlockId, Outcome> {
        self.burn(1)?;
        // Lowering pushes a branch's target, or a table's words, above the stack before it jumps.
        let growth = terminator.kind.lowering_stack_growth(self.machine.module.next_block(block));
        if self.stack.len() + growth > MAX_STACK_DEPTH {
            return Err(Outcome::Halt(Halt::StackOverflow));
        }
        match &terminator.kind {
            TerminatorKind::Jump(target) => Ok(*target),
            TerminatorKind::JumpI { then_block, else_block } => {
                let condition = self.pop()?;
                Ok(if condition.is_zero() { *else_block } else { *then_block })
            }
            TerminatorKind::IndexedJump(targets) => {
                let index = self.pop()?;
                // Lowering emits no bounds check, so the assembled code reads whatever follows
                // the table.
                usize::try_from(index)
                    .ok()
                    .and_then(|index| targets.get(index))
                    .copied()
                    .ok_or_else(|| unsupported("an indexed jump outside its table"))
            }
            &TerminatorKind::Op(opcode) => match self.opcode(opcode)? {
                Some(target) => Ok(target),
                // An opcode that does not end the run falls through, as the assembled code does.
                None => self.fall_through(block),
            },
        }
    }

    /// Runs `opcode`, returning the block it jumps to, if any.
    fn opcode(&mut self, opcode: u8) -> Result<Option<BlockId>, Outcome> {
        let Some(definition) = op::definition(opcode) else {
            return Err(Outcome::Halt(Halt::UndefinedOpcode(opcode)));
        };
        if !definition.is_available(self.machine.evm_version) {
            return Err(Outcome::Halt(Halt::UndefinedOpcode(opcode)));
        }
        let Some((inputs, _)) = definition.stack_io else {
            return Err(unsupported("an extended stack opcode without its immediate"));
        };
        if (op::PUSH1..=op::PUSH32).contains(&opcode) {
            return Err(unsupported("a push without its immediate"));
        }
        let inputs = usize::from(inputs);
        if self.stack.len() < inputs {
            return Err(Outcome::Halt(Halt::StackUnderflow));
        }
        let start = self.stack.len() - inputs;
        if self.code_sizes.contains_any(start..self.stack.len()) {
            // NOTE: Agreement between two layouts cannot prove a size comparison or arithmetic
            // independent of assembly. Only using an unmodified size as CODECOPY's source is
            // supported: it reads zeros, and its other operands must not be code sizes.
            if opcode != op::CODECOPY
                || !self.code_sizes.contains(start + 1)
                || self.code_sizes.contains(start)
                || self.code_sizes.contains(start + 2)
            {
                return Err(unsupported("observes the code size, which only assembly decides"));
            }
            self.code_sizes.remove(start + 1);
        }
        // Operands in pop order: the top of the stack first.
        let operands =
            self.stack.drain(self.stack.len() - inputs..).rev().collect::<SmallVec<[U256; 8]>>();
        let effect = if definition.is_pure() {
            eval_opcode(opcode, &operands)
                .map(Effect::Push)
                .ok_or_else(|| unsupported(format!("evaluates `{}`", definition.mnemonic)))
        } else {
            self.effect(opcode, &operands)
        };
        match effect? {
            Effect::Next => Ok(None),
            Effect::Push(value) => {
                let slot = self.stack.len();
                self.push(value)?;
                if opcode == op::CODESIZE {
                    self.code_sizes.insert(slot);
                }
                Ok(None)
            }
            Effect::Jump(target) => Ok(Some(target)),
        }
    }

    /// Runs an opcode that is not pure on its operands, in pop order.
    fn effect(&mut self, opcode: u8, operands: &[U256]) -> Result<Effect, Outcome> {
        Ok(match (opcode, operands) {
            (op::KECCAK256, &[offset, len]) => {
                let range = self.memory_range(offset, len)?;
                Effect::Push(U256::from_be_bytes(keccak256(&self.memory.0[range]).0))
            }
            (op::CALLDATALOAD, &[offset]) => {
                let mut word = [0; WORD_BYTES];
                copy_padded(self.calldata, offset, &mut word);
                Effect::Push(U256::from_be_bytes(word))
            }
            (op::CALLDATASIZE, &[]) => Effect::Push(U256::from(self.calldata.len())),
            (op::CALLDATACOPY, &[dest, offset, len]) => {
                let range = self.memory_range(dest, len)?;
                copy_padded(self.calldata, offset, &mut self.memory.0[range]);
                Effect::Next
            }
            (op::CODESIZE, &[]) => Effect::Push(U256::from(self.image.size())),
            (op::CODECOPY, &[dest, offset, len]) => {
                let range = self.memory_range(dest, len)?;
                self.image.copy(offset, &mut self.memory.0[range])?;
                Effect::Next
            }
            (op::RETURNDATASIZE, &[]) => Effect::Push(U256::ZERO),
            (op::RETURNDATACOPY, &[_, offset, len]) => {
                // The run makes no calls, so its return data is empty and any copy from it reads
                // out of bounds, even an empty one at a nonzero offset.
                if !offset.is_zero() || !len.is_zero() {
                    return Err(Outcome::Halt(Halt::ReturnDataOutOfBounds));
                }
                Effect::Next
            }
            (op::MLOAD, &[offset]) => Effect::Push(self.memory.load(offset)?),
            (op::MSTORE, &[offset, value]) => {
                self.memory.store(offset, value)?;
                Effect::Next
            }
            (op::MSTORE8, &[offset, value]) => {
                let range = self.memory.range(offset, U256::ONE)?;
                self.memory.0[range.start] = value.byte(0);
                Effect::Next
            }
            (op::MSIZE, &[]) => Effect::Push(U256::from(self.memory.0.len())),
            (op::MCOPY, &[dest, source, len]) => {
                let source = self.memory_range(source, len)?;
                let dest = self.memory.range(dest, len)?;
                self.memory.0.copy_within(source, dest.start);
                Effect::Next
            }
            (op::SLOAD, &[slot]) => Effect::Push(match self.effects.storage.get(&slot) {
                Some(&value) => value,
                None => self.host.storage(slot),
            }),
            (op::SSTORE, &[slot, value]) => {
                self.effects.storage.insert(slot, value);
                Effect::Next
            }
            (op::TLOAD, &[slot]) => Effect::Push(match self.effects.transient.get(&slot) {
                Some(&value) => value,
                None => self.host.transient(slot),
            }),
            (op::TSTORE, &[slot, value]) => {
                self.effects.transient.insert(slot, value);
                Effect::Next
            }
            (op::JUMP, &[target]) => Effect::Jump(self.jump_target(target)?),
            (op::JUMPI, &[target, condition]) => {
                if condition.is_zero() {
                    Effect::Next
                } else {
                    Effect::Jump(self.jump_target(target)?)
                }
            }
            (op::JUMPDEST, &[]) => Effect::Next,
            (op::PUSH0, &[]) => Effect::Push(U256::ZERO),
            (op::LOG0..=op::LOG4, &[offset, len, ref topics @ ..]) => {
                let range = self.memory_range(offset, len)?;
                let data = self.memory.0[range].to_vec();
                self.effects.logs.push(Log { topics: topics.to_vec(), data });
                Effect::Next
            }
            (op::RETURN, &[offset, len]) => {
                let range = self.memory_range(offset, len)?;
                return Err(Outcome::Return(self.memory.0[range].to_vec()));
            }
            (op::REVERT, &[offset, len]) => {
                let range = self.memory_range(offset, len)?;
                return Err(Outcome::Revert(self.memory.0[range].to_vec()));
            }
            (op::STOP, &[]) => return Err(Outcome::Stop),
            (op::INVALID, &[]) => return Err(Outcome::Halt(Halt::Invalid)),
            _ if HOST_OPCODES.contains(&opcode) => match self.host.read(opcode, operands) {
                Some(value) => Effect::Push(value),
                None => return Err(unsupported(format!("reads `{}`", mnemonic(opcode)))),
            },
            _ => return Err(unsupported(format!("reaches `{}`", mnemonic(opcode)))),
        })
    }

    /// Returns the memory range of `len` bytes at `offset`, paying a unit of fuel per word.
    fn memory_range(&mut self, offset: U256, len: U256) -> Result<Range<usize>, Outcome> {
        let range = self.memory.range(offset, len)?;
        self.burn(range.len().div_ceil(WORD_BYTES) as u64)?;
        Ok(range)
    }

    /// Returns the block a jump to `target` reaches.
    fn jump_target(&self, target: U256) -> Result<BlockId, Outcome> {
        self.image
            .block_at(target)
            .filter(|&block| self.machine.pushed.contains(block))
            .ok_or_else(|| unsupported("jumps to an address that no pushed block has"))
    }

    /// Pops the top of the stack, which must not hold the code size.
    fn pop(&mut self) -> Result<U256, Outcome> {
        let value = self.stack.pop().ok_or(Outcome::Halt(Halt::StackUnderflow))?;
        if self.code_sizes.remove(self.stack.len()) {
            return Err(unsupported("observes the code size, which only assembly decides"));
        }
        Ok(value)
    }

    /// Pushes `value`.
    fn push(&mut self, value: U256) -> Result<(), Outcome> {
        if self.stack.len() == MAX_STACK_DEPTH {
            return Err(Outcome::Halt(Halt::StackOverflow));
        }
        self.code_sizes.remove(self.stack.len());
        self.stack.push(value);
        Ok(())
    }

    /// Spends `units` of fuel.
    fn burn(&mut self, units: u64) -> Result<(), Outcome> {
        self.fuel = self.fuel.checked_sub(units).ok_or(Outcome::Limit(Limit::Fuel))?;
        Ok(())
    }

    /// Exchanges two stack slots, retaining whether they hold the code size.
    fn swap(&mut self, first: usize, second: usize) {
        self.stack.swap(first, second);
        let first_size = self.code_sizes.remove(first);
        let second_size = self.code_sizes.remove(second);
        if first_size {
            self.code_sizes.insert(second);
        }
        if second_size {
            self.code_sizes.insert(first);
        }
    }
}

/// Copies the bytes of `source` at `offset` into `out`, with zeros past the end of `source`.
fn copy_padded(source: &[u8], offset: U256, out: &mut [u8]) {
    let available = usize::try_from(offset).ok().and_then(|offset| source.get(offset..));
    let available = available.unwrap_or_default();
    let copied = available.len().min(out.len());
    out[..copied].copy_from_slice(&available[..copied]);
    out[copied..].fill(0);
}

/// Returns the mnemonic of a defined opcode.
fn mnemonic(opcode: u8) -> &'static str {
    op::mnemonic(opcode).unwrap_or("unknown")
}

/// Ends a run as unsupported for `reason`.
fn unsupported(reason: impl Into<Cow<'static, str>>) -> Outcome {
    Outcome::Unsupported(reason.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_pad_with_zeros() {
        let mut out = [0xff; 4];
        copy_padded(&[1, 2, 3], U256::from(1), &mut out);
        assert_eq!(out, [2, 3, 0, 0]);
        copy_padded(&[1, 2, 3], U256::MAX, &mut out);
        assert_eq!(out, [0; 4]);
    }

    #[test]
    fn memory_grows_by_words() {
        let mut memory = Memory::default();
        assert_eq!(memory.range(U256::MAX, U256::ZERO), Ok(0..0));
        assert!(memory.0.is_empty());
        assert_eq!(memory.range(U256::from(33), U256::ONE), Ok(33..34));
        assert_eq!(memory.0.len(), 64);
        assert_eq!(memory.range(U256::MAX, U256::ONE), Err(Outcome::Limit(Limit::Memory)));
        let limit = U256::from(MEMORY_LIMIT);
        assert_eq!(memory.range(limit - U256::ONE, U256::ONE).map(|range| range.end), Ok(1 << 24));
        assert_eq!(memory.range(limit, U256::ONE), Err(Outcome::Limit(Limit::Memory)));
    }
}
