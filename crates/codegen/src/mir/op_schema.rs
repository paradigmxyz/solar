//! Declarative metadata for MIR operations.
//!
//! The table owns the typed [`InstKind`] enum as well as its compiler-facing
//! metadata. Several operations carry domain-specific layouts and
//! variable-length operands, so the generated representation remains a typed
//! Rust enum while the declaration stays in one place. The descriptors can
//! drive verification, textual serialization, and later machine serialization
//! without making the optimizer reason about untyped operands.

use super::{
    AbiEncodeMode, AbiLayoutRef, AbiParamLayoutRef, AllocationKind, AllocationSemantics, BlockId,
    DataRef, EffectKind, FrameMode, FrameSlotKind, FunctionId, ImmutableId, InstructionMetadata,
    MemoryObjectKind, MemoryObjectLayout, MirPhase, SliceLocation, StorageLayoutRef, ValueId,
};

/// A compact set of MIR phases in which an operation is structurally valid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PhaseSet(u8);

impl PhaseSet {
    /// All phases currently defined by MIR.
    pub(crate) const ALL: Self = Self::through(MirPhase::EvmShaped);
    /// Phases before semantic memory lowering has completed.
    pub(crate) const THROUGH_DISPATCH: Self = Self::through(MirPhase::Dispatch);
    /// Phases before the physical EVM shape boundary.
    pub(crate) const THROUGH_MEMORY_LOWERED: Self = Self::through(MirPhase::MemoryLowered);

    /// Creates a set containing every phase up to and including `phase`.
    const fn through(phase: MirPhase) -> Self {
        Self((1u8 << (phase as u8 + 1)) - 1)
    }

    /// Returns whether this operation is valid in `phase`.
    pub(crate) const fn contains(self, phase: MirPhase) -> bool {
        self.0 & (1u8 << phase as u8) != 0
    }
}

/// Declarative operation properties used by analyses and rewrites.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct OpTraits(u16);

impl OpTraits {
    /// No additional operation traits.
    pub(crate) const NONE: Self = Self(0);
    /// The operation's binary operands may be exchanged by the scheduler.
    pub(crate) const REORDERABLE: Self = Self(1 << 0);
    /// The operation is cheap and stable enough to rematerialize at uses.
    pub(crate) const REMATERIALIZABLE: Self = Self(1 << 1);
    /// The operation still carries a semantic memory-object representation.
    pub(crate) const MEMORY_OBJECT: Self = Self(1 << 2);

    /// Returns whether this set contains `trait_`.
    pub(crate) const fn contains(self, trait_: Self) -> bool {
        self.0 & trait_.0 == trait_.0
    }
}

macro_rules! define_mir_ops {
    (
        enum $inst_name:ident { $($variants:tt)* }
        defs {
            $(
                $pattern:pat => {
                    mnemonic: $mnemonic:literal,
                    phases: $phases:expr,
                    effect: $effect:ident,
                    traits: $traits:expr,
                    side_effects: $side_effects:expr,
                    category: $category:expr $(,)?
                }
            ),+ $(,)?
        }
    ) => {
        /// The kind of a MIR instruction.
        #[derive(Clone, Debug, PartialEq)]
        pub(crate) enum $inst_name {
            $($variants)*
        }

        /// Generated metadata for one MIR operation.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub(crate) struct OpDef {
            /// Canonical textual operation name.
            pub(crate) mnemonic: &'static str,
            /// Phases in which this operation is valid.
            pub(crate) phases: PhaseSet,
            /// Conservative effect classification.
            pub(crate) effect: EffectKind,
            /// Declarative operation properties.
            pub(crate) traits: OpTraits,
            /// Whether the operation must remain observable to DCE.
            pub(crate) has_side_effects: bool,
            /// Diagnostic category used when a phase boundary is violated.
            pub(crate) phase_category: Option<&'static str>,
        }

        impl $inst_name {
            /// Returns the declarative definition for this operation.
            #[inline]
            #[must_use]
            pub(crate) const fn op_def(&self) -> &'static OpDef {
                match self {
                    $(
                        $pattern => &OpDef {
                            mnemonic: $mnemonic,
                            phases: $phases,
                            effect: EffectKind::$effect,
                            traits: $traits,
                            has_side_effects: $side_effects,
                            phase_category: $category,
                        },
                    )+
                }
            }

            /// Returns the operation's phase-boundary diagnostic category.
            #[inline]
            #[must_use]
            pub(crate) fn phase_violation(
                &self,
                phase: MirPhase,
                metadata: &InstructionMetadata,
            ) -> Option<&'static str> {
                let definition = self.op_def();
                if !definition.phases.contains(phase) {
                    return definition.phase_category;
                }
                if matches!(self, Self::Alloc { kind: AllocationKind::Object(_), .. })
                    && phase >= MirPhase::MemoryLowered
                {
                    return Some("memory-object");
                }
                if matches!(self, Self::Alloc { .. })
                    && phase >= MirPhase::EvmShaped
                    && !metadata.deferred_alloc()
                {
                    return Some("abstract allocation");
                }
                None
            }
        }
    };
}

define_mir_ops! {
    enum InstKind {
    // Arithmetic operations
    /// Addition: `a + b`
    Add(ValueId, ValueId),
    /// Subtraction: `a - b`
    Sub(ValueId, ValueId),
    /// Multiplication: `a * b`
    Mul(ValueId, ValueId),
    /// Unsigned division: `a / b`
    Div(ValueId, ValueId),
    /// Signed division: `a / b`
    SDiv(ValueId, ValueId),
    /// Unsigned modulo: `a % b`
    Mod(ValueId, ValueId),
    /// Signed modulo: `a % b`
    SMod(ValueId, ValueId),
    /// Exponentiation: `a ** b`
    Exp(ValueId, ValueId),
    /// Add modulo: `(a + b) % n`
    AddMod(ValueId, ValueId, ValueId),
    /// Multiply modulo: `(a * b) % n`
    MulMod(ValueId, ValueId, ValueId),

    // Bitwise operations
    /// Bitwise AND: `a & b`
    And(ValueId, ValueId),
    /// Bitwise OR: `a | b`
    Or(ValueId, ValueId),
    /// Bitwise XOR: `a ^ b`
    Xor(ValueId, ValueId),
    /// Bitwise NOT: `~a`
    Not(ValueId),
    /// Count leading zero bits.
    Clz(ValueId),
    /// Left shift: `a << b`
    Shl(ValueId, ValueId),
    /// Logical right shift: `a >> b`
    Shr(ValueId, ValueId),
    /// Arithmetic right shift: `a >> b` (signed)
    Sar(ValueId, ValueId),
    /// Extract a byte: `byte(i, x)`
    Byte(ValueId, ValueId),

    // Comparison operations
    /// Less than (unsigned): `a < b`
    Lt(ValueId, ValueId),
    /// Greater than (unsigned): `a > b`
    Gt(ValueId, ValueId),
    /// Less than (signed): `a < b`
    SLt(ValueId, ValueId),
    /// Greater than (signed): `a > b`
    SGt(ValueId, ValueId),
    /// Equality: `a == b`
    Eq(ValueId, ValueId),
    /// Check if zero: `a == 0`
    IsZero(ValueId),

    // Memory operations
    /// Load from memory: `mload(offset)`
    MLoad(ValueId),
    /// Store to memory: `mstore(offset, value)`
    MStore(ValueId, ValueId),
    /// Store a single byte: `mstore8(offset, value)`
    MStore8(ValueId, ValueId),
    /// Set a contiguous memory range to zero: `memory_zero(offset, size)`
    MemoryZero(ValueId, ValueId),
    /// Get memory size: `msize()`
    MSize,
    /// Read the free-memory pointer.
    Fmp,
    /// Set the free-memory pointer.
    SetFmp(ValueId),
    /// Reserve memory and return the previous free-memory pointer.
    Alloc {
        /// Requested byte count.
        size: ValueId,
        /// Semantic shape of the returned reference.
        kind: AllocationKind,
        /// Alignment, initialization, and failure behavior.
        semantics: AllocationSemantics,
    },
    /// Read the logical length of a dynamic memory object.
    MemoryObjectLen(ValueId, MemoryObjectKind),
    /// Set the logical length of a dynamic memory object.
    SetMemoryObjectLen(ValueId, ValueId, MemoryObjectKind),
    /// Project the address of the first payload byte from an object.
    MemoryObjectData(ValueId, MemoryObjectKind),
    /// Address a direct field of a struct object.
    MemoryObjectFieldAddr {
        /// Struct object reference.
        object: ValueId,
        /// Complete direct-object layout.
        layout: MemoryObjectLayout,
        /// Zero-based direct field index.
        field: u64,
    },
    /// Address an array element under the semantic object layout.
    MemoryObjectElementAddr {
        /// Array object reference.
        object: ValueId,
        /// Complete direct-object layout.
        layout: MemoryObjectLayout,
        /// Runtime element index.
        index: ValueId,
    },
    /// Load one direct struct field without exposing its physical address.
    MemoryObjectLoadField {
        /// Struct object reference.
        object: ValueId,
        /// Complete direct-object layout.
        layout: MemoryObjectLayout,
        /// Zero-based direct field index.
        field: u64,
    },
    /// Store one direct struct field without exposing its physical address.
    MemoryObjectStoreField {
        /// Struct object reference.
        object: ValueId,
        /// Complete direct-object layout.
        layout: MemoryObjectLayout,
        /// Zero-based direct field index.
        field: u64,
        /// Value to store.
        value: ValueId,
    },
    /// Load one array element without exposing its physical address.
    MemoryObjectLoadElement {
        /// Array object reference.
        object: ValueId,
        /// Complete direct-object layout.
        layout: MemoryObjectLayout,
        /// Runtime element index.
        index: ValueId,
    },
    /// Load one byte from a bytes object without exposing its physical address.
    MemoryObjectLoadByte {
        /// Bytes object reference.
        object: ValueId,
        /// Runtime byte index.
        index: ValueId,
    },
    /// Store one array element without exposing its physical address.
    MemoryObjectStoreElement {
        /// Array object reference.
        object: ValueId,
        /// Complete direct-object layout.
        layout: MemoryObjectLayout,
        /// Runtime element index.
        index: ValueId,
        /// Value to store.
        value: ValueId,
    },
    /// Store one byte in a bytes object without exposing its physical address.
    MemoryObjectStoreByte {
        /// Bytes object reference.
        object: ValueId,
        /// Runtime byte index.
        index: ValueId,
        /// Low byte to store.
        value: ValueId,
    },
    /// Store one word at a byte offset in a bytes object without exposing its
    /// physical address.
    MemoryObjectStoreWord {
        /// Bytes object reference.
        object: ValueId,
        /// Runtime byte offset from the payload start.
        offset: ValueId,
        /// Word to store.
        value: ValueId,
    },
    /// Load one word from a memory slice at a byte offset without exposing its
    /// physical address.
    MemorySliceLoadWord {
        /// Memory slice reference.
        slice: ValueId,
        /// Runtime byte offset from the slice start.
        offset: ValueId,
    },
    /// Load one word from a calldata slice at a byte offset without exposing
    /// the physical calldata address.
    CalldataSliceLoadWord {
        /// Calldata slice reference.
        slice: ValueId,
        /// Runtime byte offset from the slice start.
        offset: ValueId,
    },
    /// Copy a typed slice into the payload of a dynamic memory object.
    MemoryObjectCopyFromSlice {
        /// Destination memory object reference.
        object: ValueId,
        /// Dynamic memory object kind.
        kind: MemoryObjectKind,
        /// Source logical slice.
        source: ValueId,
    },
    /// Copy a typed slice into a byte offset in a dynamic memory object.
    MemoryObjectCopyFromSliceAt {
        /// Destination memory object reference.
        object: ValueId,
        /// Dynamic memory object kind.
        kind: MemoryObjectKind,
        /// Byte offset from the destination payload start.
        offset: ValueId,
        /// Source logical slice.
        source: ValueId,
    },
    /// Copy a byte range between two dynamic memory objects.
    MemoryObjectCopy {
        /// Destination memory object reference.
        destination: ValueId,
        /// Destination memory object kind.
        destination_kind: MemoryObjectKind,
        /// Source memory object reference.
        source: ValueId,
        /// Source memory object kind.
        source_kind: MemoryObjectKind,
        /// Number of bytes to copy.
        length: ValueId,
    },
    /// ABI-encode values into memory.
    AbiEncode {
        /// Storage policy for the encoded result.
        mode: AbiEncodeMode,
        /// Optional left-aligned four-byte selector prefix.
        selector: Option<ValueId>,
        /// Values corresponding to the tuple layout.
        args: Box<[ValueId]>,
        /// Interned semantic ABI layout.
        layout: AbiLayoutRef,
    },
    /// Decode a memory-backed ABI tuple into semantic MIR values.
    ///
    /// The instruction result is the first tuple value. Additional values are
    /// published through the multi-return buffer, matching ordinary MIR calls.
    AbiDecode {
        /// ABI-encoded bytes object.
        data: ValueId,
        /// Interned ABI input layout, including scalar validation types.
        layout: AbiParamLayoutRef,
    },
    /// Copy a statically shaped aggregate from storage into an existing memory allocation.
    StorageToMemory {
        /// Base storage slot.
        storage: ValueId,
        /// Destination memory pointer.
        memory: ValueId,
        /// Aggregate layout.
        layout: StorageLayoutRef,
    },
    /// Copy a statically shaped aggregate from memory into storage.
    MemoryToStorage {
        /// Source memory pointer.
        memory: ValueId,
        /// Base storage slot.
        storage: ValueId,
        /// Aggregate layout.
        layout: StorageLayoutRef,
    },
    /// Clear every storage slot occupied by a statically shaped aggregate.
    ClearStorage {
        /// Base storage slot.
        storage: ValueId,
        /// Aggregate layout.
        layout: StorageLayoutRef,
    },
    /// Copy memory: `mcopy(dest, src, len)`
    MCopy(ValueId, ValueId, ValueId),

    // Storage operations
    /// Load from storage: `sload(slot)`
    SLoad(ValueId),
    /// Store to storage: `sstore(slot, value)`
    SStore(ValueId, ValueId),
    /// Transient load: `tload(slot)`
    TLoad(ValueId),
    /// Transient store: `tstore(slot, value)`
    TStore(ValueId, ValueId),

    // Calldata operations
    /// Load from calldata: `calldataload(offset)`
    CalldataLoad(ValueId),
    /// Copy calldata to memory: `calldatacopy(destOffset, offset, size)`
    CalldataCopy(ValueId, ValueId, ValueId),
    /// Get calldata size: `calldatasize()`
    CalldataSize,
    /// Construct a logical `(pointer, length, location)` slice.
    MakeSlice {
        /// Address of the first element or byte.
        ptr: ValueId,
        /// Logical element or byte length.
        len: ValueId,
        /// Address space containing the slice data.
        location: SliceLocation,
    },
    /// Project the data pointer from a slice.
    SlicePtr(ValueId),
    /// Project the logical length from a slice.
    SliceLen(ValueId),
    /// Address inside the current internal-call frame.
    InternalFrameAddr(u64),
    /// Load a mutable local through its logical frame slot.
    ///
    /// A plain memory read: deletable when its result is dead. Ordering
    /// against frame stores, calls, and other frame traffic is carried by
    /// effect kinds and the alias model's `frame_location`.
    FrameLoad {
        /// Byte offset within the function's local region.
        offset: u64,
        /// Calling convention that owns the local region.
        mode: FrameMode,
        /// Logical value representation stored in the slot.
        kind: FrameSlotKind,
    },
    /// Store a mutable local through its logical frame slot.
    FrameStore {
        /// Byte offset within the function's local region.
        offset: u64,
        /// Calling convention that owns the local region.
        mode: FrameMode,
        /// Logical value representation stored in the slot.
        kind: FrameSlotKind,
        /// Value to store.
        value: ValueId,
    },
    /// Base address of the constructor's copied ABI argument blob.
    ConstructorArgsBase,
    /// End address of the constructor's copied ABI argument blob.
    ConstructorArgsEnd,

    // Code operations
    /// Copy constant module data to memory.
    DataCopy(DataRef, ValueId, ValueId),
    /// Get code size: `codesize()`
    CodeSize,
    /// Copy code to memory: `codecopy(destOffset, offset, size)`
    CodeCopy(ValueId, ValueId, ValueId),
    /// Get external code size: `extcodesize(addr)`
    ExtCodeSize(ValueId),
    /// Copy external code to memory: `extcodecopy(addr, destOffset, offset, size)`
    ExtCodeCopy(ValueId, ValueId, ValueId, ValueId),
    /// Get external code hash: `extcodehash(addr)`
    ExtCodeHash(ValueId),
    /// Assign an immutable during construction: `storeimmutable <name>, value`.
    /// Lowered to constructor staging memory after MIR optimization.
    StoreImmutable(ImmutableId, ValueId),
    /// Read an immutable declared by the module: `loadimmutable <name>`.
    ///
    /// In runtime code this assembles to a typed `PUSH<N>` placeholder that the
    /// constructor patches with the staged value before returning the runtime
    /// code. In constructor code it reads the staging word instead.
    LoadImmutable(ImmutableId),

    // Return data operations
    /// Get the current call's return data size: `returndatasize()`.
    ///
    /// Raw volatile query used by Yul and high-level call lowering.
    ReturnDataSize,
    /// Copy return data to memory: `returndatacopy(destOffset, offset, size)`
    ReturnDataCopy(ValueId, ValueId, ValueId),

    // Environment operations
    /// Get caller address: `caller()`
    Caller,
    /// Get call value: `callvalue()`
    CallValue,
    /// Get origin address: `origin()`
    Origin,
    /// Get gas price: `gasprice()`
    GasPrice,
    /// Get block hash: `blockhash(blockNum)`
    BlockHash(ValueId),
    /// Get coinbase address: `coinbase()`
    Coinbase,
    /// Get block timestamp: `timestamp()`
    Timestamp,
    /// Get block number: `number()`
    BlockNumber,
    /// Get previous randao: `prevrandao()`
    PrevRandao,
    /// Get gas limit: `gaslimit()`
    GasLimit,
    /// Get beacon chain slot number: `slotnum()`
    SlotNum,
    /// Get chain ID: `chainid()`
    ChainId,
    /// Get this contract's address: `address()`
    Address,
    /// Get balance: `balance(addr)`
    Balance(ValueId),
    /// Get self balance: `selfbalance()`
    SelfBalance,
    /// Get remaining gas: `gas()`
    Gas,
    /// Get base fee: `basefee()`
    BaseFee,
    /// Get blob base fee: `blobbasefee()`
    BlobBaseFee,
    /// Get blob hash: `blobhash(index)`
    BlobHash(ValueId),

    // Hashing
    /// Keccak256 hash: `keccak256(offset, size)`
    Keccak256(ValueId, ValueId),
    /// Keccak256 hash of a `memorybytes` object's contents:
    /// `keccak256_bytes(object)`.
    ///
    /// Consumes the object reference directly, so the optimizer sees one
    /// whole-object read instead of separate length and data-pointer
    /// projections. `lower-memory-objects` expands it into those projections
    /// and a physical `keccak256`.
    Keccak256Bytes(ValueId),
    /// Hash a fixed-width mapping key and its parent slot.
    ///
    /// The temporary scratch memory used by its late lowering is not an
    /// observable part of this instruction's MIR semantics.
    MappingSlot(ValueId, ValueId),
    /// Hash a `[length][data...]` memory value and its parent mapping slot.
    MappingSlotMemory(ValueId, ValueId),
    /// Hash a dynamically-sized calldata value and its parent mapping slot.
    ///
    /// The temporary scratch memory used by its late lowering is not an
    /// observable part of this instruction's MIR semantics.
    MappingSlotCalldata(ValueId, ValueId),
    /// Hash the slot of a dynamically-sized storage array to find its data.
    ///
    /// The temporary scratch memory used by its late lowering is not an
    /// observable part of this instruction's MIR semantics.
    StorageArrayDataSlot(ValueId),
    /// Resolve one element slot in a dynamic storage array.
    ///
    /// The array's base slot, element index, and logical slot stride stay
    /// semantic until the mapping-slot lowering pass expands the hash and
    /// offset calculation.
    StorageArrayElementSlot { slot: ValueId, index: ValueId, element_slots: u64 },

    // Call operations
    // TODO(codegen): Consider unifying external calls as one instruction with a call-kind enum
    // and shared operands once the MIR shape stabilizes.
    /// External call: `call(gas, addr, value, argsOffset, argsSize, retOffset, retSize)`
    Call {
        gas: ValueId,
        addr: ValueId,
        value: ValueId,
        args_offset: ValueId,
        args_size: ValueId,
        ret_offset: ValueId,
        ret_size: ValueId,
    },
    /// Call code: `callcode(gas, addr, value, argsOffset, argsSize, retOffset, retSize)`
    CallCode {
        gas: ValueId,
        addr: ValueId,
        value: ValueId,
        args_offset: ValueId,
        args_size: ValueId,
        ret_offset: ValueId,
        ret_size: ValueId,
    },
    /// Static call: `staticcall(gas, addr, argsOffset, argsSize, retOffset, retSize)`
    StaticCall {
        gas: ValueId,
        addr: ValueId,
        args_offset: ValueId,
        args_size: ValueId,
        ret_offset: ValueId,
        ret_size: ValueId,
    },
    /// Delegate call: `delegatecall(gas, addr, argsOffset, argsSize, retOffset, retSize)`
    DelegateCall {
        gas: ValueId,
        addr: ValueId,
        args_offset: ValueId,
        args_size: ValueId,
        ret_offset: ValueId,
        ret_size: ValueId,
    },
    /// EOF external call: `extcall(addr, argsOffset, argsSize, value)`.
    ExtCall { addr: ValueId, args_offset: ValueId, args_size: ValueId, value: ValueId },
    /// EOF external delegate call: `extdelegatecall(addr, argsOffset, argsSize)`.
    ExtDelegateCall { addr: ValueId, args_offset: ValueId, args_size: ValueId },
    /// EOF external static call: `extstaticcall(addr, argsOffset, argsSize)`.
    ExtStaticCall { addr: ValueId, args_offset: ValueId, args_size: ValueId },
    /// Internal function call lowered to a direct jump.
    InternalCall { function: FunctionId, args: Box<[ValueId]>, returns: u32 },

    // Contract creation
    /// Create contract: `create(value, offset, size)`
    Create(ValueId, ValueId, ValueId),
    /// Create2 contract: `create2(value, offset, size, salt)`
    Create2(ValueId, ValueId, ValueId, ValueId),

    // Log operations
    // TODO(codegen): Consider unifying log0..log4 as one instruction with a topic list.
    /// Log with no topics: `log0(offset, size)`
    Log0(ValueId, ValueId),
    /// Log with 1 topic: `log1(offset, size, topic1)`
    Log1(ValueId, ValueId, ValueId),
    /// Log with 2 topics: `log2(offset, size, topic1, topic2)`
    Log2(ValueId, ValueId, ValueId, ValueId),
    /// Log with 3 topics: `log3(offset, size, topic1, topic2, topic3)`
    Log3(ValueId, ValueId, ValueId, ValueId, ValueId),
    /// Log with 4 topics: `log4(offset, size, topic1, topic2, topic3, topic4)`
    Log4(ValueId, ValueId, ValueId, ValueId, ValueId, ValueId),

    // SSA operations
    /// Phi node: merge values from different predecessors.
    Phi(Vec<(BlockId, ValueId)>),
    /// Select: `select(cond, true_val, false_val)`
    Select(ValueId, ValueId, ValueId),

    // Sign extension
    /// Sign extend: `signextend(b, x)` - extends the sign bit from byte position b
    SignExtend(ValueId, ValueId),
}
    defs {
    Self::Add(_, _) => { mnemonic: "add", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::REORDERABLE, side_effects: false, category: None },
    Self::Sub(_, _) => { mnemonic: "sub", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Mul(_, _) => { mnemonic: "mul", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::REORDERABLE, side_effects: false, category: None },
    Self::Div(_, _) => { mnemonic: "div", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::SDiv(_, _) => { mnemonic: "sdiv", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Mod(_, _) => { mnemonic: "mod", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::SMod(_, _) => { mnemonic: "smod", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Exp(_, _) => { mnemonic: "exp", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::AddMod(_, _, _) => { mnemonic: "addmod", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::MulMod(_, _, _) => { mnemonic: "mulmod", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::And(_, _) => { mnemonic: "and", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::REORDERABLE, side_effects: false, category: None },
    Self::Or(_, _) => { mnemonic: "or", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::REORDERABLE, side_effects: false, category: None },
    Self::Xor(_, _) => { mnemonic: "xor", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::REORDERABLE, side_effects: false, category: None },
    Self::Not(_) => { mnemonic: "not", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Clz(_) => { mnemonic: "clz", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Shl(_, _) => { mnemonic: "shl", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Shr(_, _) => { mnemonic: "shr", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Sar(_, _) => { mnemonic: "sar", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Byte(_, _) => { mnemonic: "byte", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Lt(_, _) => { mnemonic: "lt", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::REORDERABLE, side_effects: false, category: None },
    Self::Gt(_, _) => { mnemonic: "gt", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::REORDERABLE, side_effects: false, category: None },
    Self::SLt(_, _) => { mnemonic: "slt", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::REORDERABLE, side_effects: false, category: None },
    Self::SGt(_, _) => { mnemonic: "sgt", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::REORDERABLE, side_effects: false, category: None },
    Self::Eq(_, _) => { mnemonic: "eq", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::REORDERABLE, side_effects: false, category: None },
    Self::IsZero(_) => { mnemonic: "iszero", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },

    Self::MLoad(_) => { mnemonic: "mload", phases: PhaseSet::ALL, effect: MemoryRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::MStore(_, _) => { mnemonic: "mstore", phases: PhaseSet::ALL, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::MStore8(_, _) => { mnemonic: "mstore8", phases: PhaseSet::ALL, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::MemoryZero(_, _) => { mnemonic: "memory_zero", phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: Some("memory zero") },
    Self::MSize => { mnemonic: "msize", phases: PhaseSet::ALL, effect: MemoryRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Fmp => { mnemonic: "fmp", phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: MemoryRead, traits: OpTraits::NONE, side_effects: false, category: Some("abstract allocation") },
    Self::SetFmp(_) => { mnemonic: "set_fmp", phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: Some("abstract allocation") },
    Self::Alloc { .. } => { mnemonic: "alloc", phases: PhaseSet::ALL, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: Some("abstract allocation") },
    Self::MemoryObjectLen(_, _) => { mnemonic: "memory_object_len", phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryRead, traits: OpTraits::MEMORY_OBJECT, side_effects: false, category: Some("memory-object") },
    Self::SetMemoryObjectLen(_, _, _) => { mnemonic: "set_memory_object_len", phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryWrite, traits: OpTraits::MEMORY_OBJECT, side_effects: true, category: Some("memory-object") },
    Self::MemoryObjectData(_, _) => { mnemonic: "memory_object_data", phases: PhaseSet::THROUGH_DISPATCH, effect: Pure, traits: OpTraits::MEMORY_OBJECT, side_effects: false, category: Some("memory-object") },
    Self::MemoryObjectFieldAddr { .. } => { mnemonic: "memory_object_field_addr", phases: PhaseSet::THROUGH_DISPATCH, effect: Pure, traits: OpTraits::MEMORY_OBJECT, side_effects: false, category: Some("memory-object") },
    Self::MemoryObjectElementAddr { .. } => { mnemonic: "memory_object_element_addr", phases: PhaseSet::THROUGH_DISPATCH, effect: Pure, traits: OpTraits::MEMORY_OBJECT, side_effects: false, category: Some("memory-object") },
    Self::MemoryObjectLoadField { .. } => { mnemonic: "memory_object_load_field", phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryRead, traits: OpTraits::MEMORY_OBJECT, side_effects: false, category: Some("memory-object") },
    Self::MemoryObjectStoreField { .. } => { mnemonic: "memory_object_store_field", phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryWrite, traits: OpTraits::MEMORY_OBJECT, side_effects: true, category: Some("memory-object") },
    Self::MemoryObjectLoadElement { .. } => { mnemonic: "memory_object_load_element", phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryRead, traits: OpTraits::MEMORY_OBJECT, side_effects: false, category: Some("memory-object") },
    Self::MemoryObjectLoadByte { .. } => { mnemonic: "memory_object_load_byte", phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryRead, traits: OpTraits::MEMORY_OBJECT, side_effects: false, category: Some("memory-object") },
    Self::MemoryObjectStoreElement { .. } => { mnemonic: "memory_object_store_element", phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryWrite, traits: OpTraits::MEMORY_OBJECT, side_effects: true, category: Some("memory-object") },
    Self::MemoryObjectStoreByte { .. } => { mnemonic: "memory_object_store_byte", phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryWrite, traits: OpTraits::MEMORY_OBJECT, side_effects: true, category: Some("memory-object") },
    Self::MemoryObjectStoreWord { .. } => { mnemonic: "memory_object_store_word", phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryWrite, traits: OpTraits::MEMORY_OBJECT, side_effects: true, category: Some("memory-object") },
    Self::MemorySliceLoadWord { .. } => { mnemonic: "memory_slice_load_word", phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryRead, traits: OpTraits::MEMORY_OBJECT, side_effects: false, category: Some("memory-object") },
    Self::CalldataSliceLoadWord { .. } => { mnemonic: "calldata_slice_load_word", phases: PhaseSet::THROUGH_DISPATCH, effect: EnvironmentRead, traits: OpTraits::MEMORY_OBJECT, side_effects: false, category: Some("memory-object") },
    Self::MemoryObjectCopyFromSlice { .. } => { mnemonic: "memory_object_copy_from_slice", phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryWrite, traits: OpTraits::MEMORY_OBJECT, side_effects: true, category: Some("memory-object") },
    Self::MemoryObjectCopyFromSliceAt { .. } => { mnemonic: "memory_object_copy_from_slice_at", phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryWrite, traits: OpTraits::MEMORY_OBJECT, side_effects: true, category: Some("memory-object") },
    Self::MemoryObjectCopy { .. } => { mnemonic: "memory_object_copy", phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryWrite, traits: OpTraits::MEMORY_OBJECT, side_effects: true, category: Some("memory-object") },
    Self::AbiEncode { .. } => { mnemonic: "abi_encode", phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: Some("ABI encoding") },
    Self::AbiDecode { .. } => { mnemonic: "abi_decode", phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: Some("ABI decoding") },
    Self::StorageToMemory { .. } => { mnemonic: "storage_to_memory", phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: Some("aggregate") },
    Self::MemoryToStorage { .. } => { mnemonic: "memory_to_storage", phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: StorageWrite, traits: OpTraits::NONE, side_effects: true, category: Some("aggregate") },
    Self::ClearStorage { .. } => { mnemonic: "clear_storage", phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: StorageWrite, traits: OpTraits::NONE, side_effects: true, category: Some("aggregate") },
    Self::MCopy(_, _, _) => { mnemonic: "mcopy", phases: PhaseSet::ALL, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: None },

    Self::SLoad(_) => { mnemonic: "sload", phases: PhaseSet::ALL, effect: StorageRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::SStore(_, _) => { mnemonic: "sstore", phases: PhaseSet::ALL, effect: StorageWrite, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::TLoad(_) => { mnemonic: "tload", phases: PhaseSet::ALL, effect: TransientRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::TStore(_, _) => { mnemonic: "tstore", phases: PhaseSet::ALL, effect: TransientWrite, traits: OpTraits::NONE, side_effects: true, category: None },

    Self::CalldataLoad(_) => { mnemonic: "calldataload", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::CalldataCopy(_, _, _) => { mnemonic: "calldatacopy", phases: PhaseSet::ALL, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::CalldataSize => { mnemonic: "calldatasize", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::MakeSlice { location: SliceLocation::Memory, .. } => { mnemonic: "make_memory_slice", phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: Some("slice") },
    Self::MakeSlice { location: SliceLocation::Calldata, .. } => { mnemonic: "make_calldata_slice", phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: Some("slice") },
    Self::MakeSlice { location: SliceLocation::Returndata, .. } => { mnemonic: "make_returndata_slice", phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: Some("slice") },
    Self::SlicePtr(_) => { mnemonic: "slice_ptr", phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: Some("slice") },
    Self::SliceLen(_) => { mnemonic: "slice_len", phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: Some("slice") },
    Self::InternalFrameAddr(_) => { mnemonic: "internal_frame_addr", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::FrameLoad { .. } => { mnemonic: "frame_load", phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryRead, traits: OpTraits::NONE, side_effects: false, category: Some("frame slot") },
    Self::FrameStore { .. } => { mnemonic: "frame_store", phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: Some("frame slot") },
    Self::ConstructorArgsBase => { mnemonic: "constructor_args_base", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::ConstructorArgsEnd => { mnemonic: "constructor_args_end", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: None },

    Self::DataCopy(_, _, _) => { mnemonic: "data_copy", phases: PhaseSet::ALL, effect: MemoryWrite, traits: OpTraits::REORDERABLE, side_effects: true, category: None },
    Self::CodeSize => { mnemonic: "codesize", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::CodeCopy(_, _, _) => { mnemonic: "codecopy", phases: PhaseSet::ALL, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::ExtCodeSize(_) => { mnemonic: "extcodesize", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::ExtCodeCopy(_, _, _, _) => { mnemonic: "extcodecopy", phases: PhaseSet::ALL, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::ExtCodeHash(_) => { mnemonic: "extcodehash", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::StoreImmutable(..) => { mnemonic: "storeimmutable", phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: ImmutableWrite, traits: OpTraits::NONE, side_effects: true, category: Some("immutable assignment") },
    Self::LoadImmutable(_) => { mnemonic: "loadimmutable", phases: PhaseSet::ALL, effect: ImmutableRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::ReturnDataSize => { mnemonic: "returndatasize", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::ReturnDataCopy(_, _, _) => { mnemonic: "returndatacopy", phases: PhaseSet::ALL, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: None },

    Self::Caller => { mnemonic: "caller", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::CallValue => { mnemonic: "callvalue", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::Origin => { mnemonic: "origin", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::GasPrice => { mnemonic: "gasprice", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::BlockHash(_) => { mnemonic: "blockhash", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Coinbase => { mnemonic: "coinbase", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::Timestamp => { mnemonic: "timestamp", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::BlockNumber => { mnemonic: "number", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::PrevRandao => { mnemonic: "prevrandao", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::GasLimit => { mnemonic: "gaslimit", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::SlotNum => { mnemonic: "slotnum", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::ChainId => { mnemonic: "chainid", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::Address => { mnemonic: "address", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::Balance(_) => { mnemonic: "balance", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::SelfBalance => { mnemonic: "selfbalance", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Gas => { mnemonic: "gas", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::BaseFee => { mnemonic: "basefee", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::BlobBaseFee => { mnemonic: "blobbasefee", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::BlobHash(_) => { mnemonic: "blobhash", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: None },

    Self::Keccak256(_, _) => { mnemonic: "keccak256", phases: PhaseSet::ALL, effect: MemoryRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Keccak256Bytes(_) => { mnemonic: "keccak256_bytes", phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryRead, traits: OpTraits::MEMORY_OBJECT, side_effects: false, category: Some("memory-object") },
    Self::MappingSlot(_, _) => { mnemonic: "mapping_slot", phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: MemoryRead, traits: OpTraits::NONE, side_effects: false, category: Some("storage slot") },
    Self::MappingSlotMemory(_, _) => { mnemonic: "mapping_slot_memory", phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: MemoryRead, traits: OpTraits::NONE, side_effects: false, category: Some("storage slot") },
    Self::MappingSlotCalldata(_, _) => { mnemonic: "mapping_slot_calldata", phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: Some("storage slot") },
    Self::StorageArrayDataSlot(_) => { mnemonic: "storage_array_data_slot", phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: Some("storage slot") },
    Self::StorageArrayElementSlot { .. } => { mnemonic: "storage_array_element_slot", phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: Some("storage slot") },

    Self::Call { .. } => { mnemonic: "call", phases: PhaseSet::ALL, effect: ExternalCall, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::CallCode { .. } => { mnemonic: "callcode", phases: PhaseSet::ALL, effect: ExternalCall, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::StaticCall { .. } => { mnemonic: "staticcall", phases: PhaseSet::ALL, effect: ExternalCall, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::DelegateCall { .. } => { mnemonic: "delegatecall", phases: PhaseSet::ALL, effect: ExternalCall, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::ExtCall { .. } => { mnemonic: "extcall", phases: PhaseSet::ALL, effect: ExternalCall, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::ExtDelegateCall { .. } => { mnemonic: "extdelegatecall", phases: PhaseSet::ALL, effect: ExternalCall, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::ExtStaticCall { .. } => { mnemonic: "extstaticcall", phases: PhaseSet::ALL, effect: ExternalCall, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::InternalCall { .. } => { mnemonic: "internal_call", phases: PhaseSet::ALL, effect: InternalCall, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::Create(_, _, _) => { mnemonic: "create", phases: PhaseSet::ALL, effect: Create, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::Create2(_, _, _, _) => { mnemonic: "create2", phases: PhaseSet::ALL, effect: Create, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::Log0(_, _) => { mnemonic: "log0", phases: PhaseSet::ALL, effect: Log, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::Log1(_, _, _) => { mnemonic: "log1", phases: PhaseSet::ALL, effect: Log, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::Log2(_, _, _, _) => { mnemonic: "log2", phases: PhaseSet::ALL, effect: Log, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::Log3(_, _, _, _, _) => { mnemonic: "log3", phases: PhaseSet::ALL, effect: Log, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::Log4(_, _, _, _, _, _) => { mnemonic: "log4", phases: PhaseSet::ALL, effect: Log, traits: OpTraits::NONE, side_effects: true, category: None },

    Self::Phi(_) => { mnemonic: "phi", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Select(_, _, _) => { mnemonic: "select", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::SignExtend(_, _) => { mnemonic: "signextend", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{AllocationKind, AllocationSemantics, ValueId};

    #[test]
    fn descriptors_drive_operation_properties() {
        let add = InstKind::Add(ValueId::new(0), ValueId::new(1));
        assert_eq!(add.mnemonic(), "add");
        assert_eq!(add.effect_kind(), EffectKind::Pure);
        assert!(add.op_def().traits.contains(OpTraits::REORDERABLE));
        assert!(!add.has_side_effects());

        let calldata_size = InstKind::CalldataSize;
        assert!(calldata_size.is_always_rematerializable());
        assert!(calldata_size.op_def().phases.contains(MirPhase::EvmShaped));

        let slice = InstKind::MakeSlice {
            ptr: ValueId::new(0),
            len: ValueId::new(1),
            location: SliceLocation::Calldata,
        };
        assert_eq!(slice.mnemonic(), "make_calldata_slice");
    }

    #[test]
    fn descriptors_enforce_phase_boundaries() {
        let metadata = InstructionMetadata::EMPTY;
        let fmp = InstKind::Fmp;
        assert_eq!(
            fmp.phase_violation(MirPhase::EvmShaped, &metadata),
            Some("abstract allocation")
        );

        let object_load =
            InstKind::MemoryObjectLoadByte { object: ValueId::new(0), index: ValueId::new(1) };
        assert_eq!(
            object_load.phase_violation(MirPhase::MemoryLowered, &metadata),
            Some("memory-object")
        );

        let raw_alloc = InstKind::Alloc {
            size: ValueId::new(0),
            kind: AllocationKind::Raw,
            semantics: AllocationSemantics::INTERNAL,
        };
        assert_eq!(
            raw_alloc.phase_violation(MirPhase::EvmShaped, &metadata),
            Some("abstract allocation")
        );
    }
}
