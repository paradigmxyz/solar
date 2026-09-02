//! Declarative metadata for MIR operations.
//!
//! The payload of [`InstKind`](super::InstKind) remains a Rust enum because
//! several operations carry domain-specific layouts and variable-length
//! operands. This table is the operation-definition layer: it is the single
//! source for stable names, effects, traits, and phase legality. The same
//! descriptors can later drive textual and machine serialization without
//! replacing the typed MIR representation.

use super::{AllocationKind, EffectKind, InstKind, InstructionMetadata, MirPhase};

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
        $(
            $pattern:pat => {
                tag: $tag:ident,
                mnemonic: $mnemonic:literal,
                phases: $phases:expr,
                effect: $effect:ident,
                traits: $traits:expr,
                side_effects: $side_effects:expr,
                category: $category:expr $(,)?
            }
        ),+ $(,)?
    ) => {
        /// Compact compiler-internal tag for a MIR operation.
        #[repr(u8)]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub(crate) enum OpTag {
            $($tag),+
        }

        /// Generated metadata for one MIR operation.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub(crate) struct OpDef {
            /// Compact compiler-internal operation tag.
            pub(crate) tag: OpTag,
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

        impl InstKind {
            /// Returns the declarative definition for this operation.
            #[inline]
            #[must_use]
            pub(crate) const fn op_def(&self) -> &'static OpDef {
                match self {
                    $(
                        $pattern => &OpDef {
                            tag: OpTag::$tag,
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
    Self::Add(_, _) => { tag: Add, mnemonic: "add", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::REORDERABLE, side_effects: false, category: None },
    Self::Sub(_, _) => { tag: Sub, mnemonic: "sub", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Mul(_, _) => { tag: Mul, mnemonic: "mul", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::REORDERABLE, side_effects: false, category: None },
    Self::Div(_, _) => { tag: Div, mnemonic: "div", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::SDiv(_, _) => { tag: SDiv, mnemonic: "sdiv", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Mod(_, _) => { tag: Mod, mnemonic: "mod", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::SMod(_, _) => { tag: SMod, mnemonic: "smod", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Exp(_, _) => { tag: Exp, mnemonic: "exp", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::AddMod(_, _, _) => { tag: AddMod, mnemonic: "addmod", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::MulMod(_, _, _) => { tag: MulMod, mnemonic: "mulmod", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::And(_, _) => { tag: And, mnemonic: "and", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::REORDERABLE, side_effects: false, category: None },
    Self::Or(_, _) => { tag: Or, mnemonic: "or", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::REORDERABLE, side_effects: false, category: None },
    Self::Xor(_, _) => { tag: Xor, mnemonic: "xor", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::REORDERABLE, side_effects: false, category: None },
    Self::Not(_) => { tag: Not, mnemonic: "not", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Clz(_) => { tag: Clz, mnemonic: "clz", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Shl(_, _) => { tag: Shl, mnemonic: "shl", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Shr(_, _) => { tag: Shr, mnemonic: "shr", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Sar(_, _) => { tag: Sar, mnemonic: "sar", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Byte(_, _) => { tag: Byte, mnemonic: "byte", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Lt(_, _) => { tag: Lt, mnemonic: "lt", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::REORDERABLE, side_effects: false, category: None },
    Self::Gt(_, _) => { tag: Gt, mnemonic: "gt", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::REORDERABLE, side_effects: false, category: None },
    Self::SLt(_, _) => { tag: SLt, mnemonic: "slt", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::REORDERABLE, side_effects: false, category: None },
    Self::SGt(_, _) => { tag: SGt, mnemonic: "sgt", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::REORDERABLE, side_effects: false, category: None },
    Self::Eq(_, _) => { tag: Eq, mnemonic: "eq", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::REORDERABLE, side_effects: false, category: None },
    Self::IsZero(_) => { tag: IsZero, mnemonic: "iszero", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },

    Self::MLoad(_) => { tag: MLoad, mnemonic: "mload", phases: PhaseSet::ALL, effect: MemoryRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::MStore(_, _) => { tag: MStore, mnemonic: "mstore", phases: PhaseSet::ALL, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::MStore8(_, _) => { tag: MStore8, mnemonic: "mstore8", phases: PhaseSet::ALL, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::MemoryZero(_, _) => { tag: MemoryZero, mnemonic: "memory_zero", phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: Some("memory zero") },
    Self::MSize => { tag: MSize, mnemonic: "msize", phases: PhaseSet::ALL, effect: MemoryRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Fmp => { tag: Fmp, mnemonic: "fmp", phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: MemoryRead, traits: OpTraits::NONE, side_effects: false, category: Some("abstract allocation") },
    Self::SetFmp(_) => { tag: SetFmp, mnemonic: "set_fmp", phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: Some("abstract allocation") },
    Self::Alloc { .. } => { tag: Alloc, mnemonic: "alloc", phases: PhaseSet::ALL, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: Some("abstract allocation") },
    Self::MemoryObjectLen(_, _) => { tag: MemoryObjectLen, mnemonic: "memory_object_len", phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryRead, traits: OpTraits::MEMORY_OBJECT, side_effects: false, category: Some("memory-object") },
    Self::SetMemoryObjectLen(_, _, _) => { tag: SetMemoryObjectLen, mnemonic: "set_memory_object_len", phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryWrite, traits: OpTraits::MEMORY_OBJECT, side_effects: true, category: Some("memory-object") },
    Self::MemoryObjectData(_, _) => { tag: MemoryObjectData, mnemonic: "memory_object_data", phases: PhaseSet::THROUGH_DISPATCH, effect: Pure, traits: OpTraits::MEMORY_OBJECT, side_effects: false, category: Some("memory-object") },
    Self::MemoryObjectFieldAddr { .. } => { tag: MemoryObjectFieldAddr, mnemonic: "memory_object_field_addr", phases: PhaseSet::THROUGH_DISPATCH, effect: Pure, traits: OpTraits::MEMORY_OBJECT, side_effects: false, category: Some("memory-object") },
    Self::MemoryObjectElementAddr { .. } => { tag: MemoryObjectElementAddr, mnemonic: "memory_object_element_addr", phases: PhaseSet::THROUGH_DISPATCH, effect: Pure, traits: OpTraits::MEMORY_OBJECT, side_effects: false, category: Some("memory-object") },
    Self::MemoryObjectLoadField { .. } => { tag: MemoryObjectLoadField, mnemonic: "memory_object_load_field", phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryRead, traits: OpTraits::MEMORY_OBJECT, side_effects: false, category: Some("memory-object") },
    Self::MemoryObjectStoreField { .. } => { tag: MemoryObjectStoreField, mnemonic: "memory_object_store_field", phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryWrite, traits: OpTraits::MEMORY_OBJECT, side_effects: true, category: Some("memory-object") },
    Self::MemoryObjectLoadElement { .. } => { tag: MemoryObjectLoadElement, mnemonic: "memory_object_load_element", phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryRead, traits: OpTraits::MEMORY_OBJECT, side_effects: false, category: Some("memory-object") },
    Self::MemoryObjectLoadByte { .. } => { tag: MemoryObjectLoadByte, mnemonic: "memory_object_load_byte", phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryRead, traits: OpTraits::MEMORY_OBJECT, side_effects: false, category: Some("memory-object") },
    Self::MemoryObjectStoreElement { .. } => { tag: MemoryObjectStoreElement, mnemonic: "memory_object_store_element", phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryWrite, traits: OpTraits::MEMORY_OBJECT, side_effects: true, category: Some("memory-object") },
    Self::MemoryObjectStoreByte { .. } => { tag: MemoryObjectStoreByte, mnemonic: "memory_object_store_byte", phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryWrite, traits: OpTraits::MEMORY_OBJECT, side_effects: true, category: Some("memory-object") },
    Self::MemoryObjectStoreWord { .. } => { tag: MemoryObjectStoreWord, mnemonic: "memory_object_store_word", phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryWrite, traits: OpTraits::MEMORY_OBJECT, side_effects: true, category: Some("memory-object") },
    Self::MemorySliceLoadWord { .. } => { tag: MemorySliceLoadWord, mnemonic: "memory_slice_load_word", phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryRead, traits: OpTraits::MEMORY_OBJECT, side_effects: false, category: Some("memory-object") },
    Self::CalldataSliceLoadWord { .. } => { tag: CalldataSliceLoadWord, mnemonic: "calldata_slice_load_word", phases: PhaseSet::THROUGH_DISPATCH, effect: EnvironmentRead, traits: OpTraits::MEMORY_OBJECT, side_effects: false, category: Some("memory-object") },
    Self::MemoryObjectCopyFromSlice { .. } => { tag: MemoryObjectCopyFromSlice, mnemonic: "memory_object_copy_from_slice", phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryWrite, traits: OpTraits::MEMORY_OBJECT, side_effects: true, category: Some("memory-object") },
    Self::MemoryObjectCopyFromSliceAt { .. } => { tag: MemoryObjectCopyFromSliceAt, mnemonic: "memory_object_copy_from_slice_at", phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryWrite, traits: OpTraits::MEMORY_OBJECT, side_effects: true, category: Some("memory-object") },
    Self::MemoryObjectCopy { .. } => { tag: MemoryObjectCopy, mnemonic: "memory_object_copy", phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryWrite, traits: OpTraits::MEMORY_OBJECT, side_effects: true, category: Some("memory-object") },
    Self::AbiEncode { .. } => { tag: AbiEncode, mnemonic: "abi_encode", phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: Some("ABI encoding") },
    Self::AbiDecode { .. } => { tag: AbiDecode, mnemonic: "abi_decode", phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: Some("ABI decoding") },
    Self::StorageToMemory { .. } => { tag: StorageToMemory, mnemonic: "storage_to_memory", phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: Some("aggregate") },
    Self::MemoryToStorage { .. } => { tag: MemoryToStorage, mnemonic: "memory_to_storage", phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: StorageWrite, traits: OpTraits::NONE, side_effects: true, category: Some("aggregate") },
    Self::ClearStorage { .. } => { tag: ClearStorage, mnemonic: "clear_storage", phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: StorageWrite, traits: OpTraits::NONE, side_effects: true, category: Some("aggregate") },
    Self::MCopy(_, _, _) => { tag: MCopy, mnemonic: "mcopy", phases: PhaseSet::ALL, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: None },

    Self::SLoad(_) => { tag: SLoad, mnemonic: "sload", phases: PhaseSet::ALL, effect: StorageRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::SStore(_, _) => { tag: SStore, mnemonic: "sstore", phases: PhaseSet::ALL, effect: StorageWrite, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::TLoad(_) => { tag: TLoad, mnemonic: "tload", phases: PhaseSet::ALL, effect: TransientRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::TStore(_, _) => { tag: TStore, mnemonic: "tstore", phases: PhaseSet::ALL, effect: TransientWrite, traits: OpTraits::NONE, side_effects: true, category: None },

    Self::CalldataLoad(_) => { tag: CalldataLoad, mnemonic: "calldataload", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::CalldataCopy(_, _, _) => { tag: CalldataCopy, mnemonic: "calldatacopy", phases: PhaseSet::ALL, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::CalldataSize => { tag: CalldataSize, mnemonic: "calldatasize", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::MakeSlice { .. } => { tag: MakeSlice, mnemonic: "make_slice", phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: Some("slice") },
    Self::SlicePtr(_) => { tag: SlicePtr, mnemonic: "slice_ptr", phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: Some("slice") },
    Self::SliceLen(_) => { tag: SliceLen, mnemonic: "slice_len", phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: Some("slice") },
    Self::InternalFrameAddr(_) => { tag: InternalFrameAddr, mnemonic: "internal_frame_addr", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::FrameLoad { .. } => { tag: FrameLoad, mnemonic: "frame_load", phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryRead, traits: OpTraits::NONE, side_effects: false, category: Some("frame slot") },
    Self::FrameStore { .. } => { tag: FrameStore, mnemonic: "frame_store", phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: Some("frame slot") },
    Self::ConstructorArgsBase => { tag: ConstructorArgsBase, mnemonic: "constructor_args_base", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::ConstructorArgsEnd => { tag: ConstructorArgsEnd, mnemonic: "constructor_args_end", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: None },

    Self::DataCopy(_, _, _) => { tag: DataCopy, mnemonic: "data_copy", phases: PhaseSet::ALL, effect: MemoryWrite, traits: OpTraits::REORDERABLE, side_effects: true, category: None },
    Self::CodeSize => { tag: CodeSize, mnemonic: "codesize", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::CodeCopy(_, _, _) => { tag: CodeCopy, mnemonic: "codecopy", phases: PhaseSet::ALL, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::ExtCodeSize(_) => { tag: ExtCodeSize, mnemonic: "extcodesize", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::ExtCodeCopy(_, _, _, _) => { tag: ExtCodeCopy, mnemonic: "extcodecopy", phases: PhaseSet::ALL, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::ExtCodeHash(_) => { tag: ExtCodeHash, mnemonic: "extcodehash", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::StoreImmutable(..) => { tag: StoreImmutable, mnemonic: "storeimmutable", phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: ImmutableWrite, traits: OpTraits::NONE, side_effects: true, category: Some("immutable assignment") },
    Self::LoadImmutable(_) => { tag: LoadImmutable, mnemonic: "loadimmutable", phases: PhaseSet::ALL, effect: ImmutableRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::ReturnDataSize => { tag: ReturnDataSize, mnemonic: "returndatasize", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::ReturnDataCopy(_, _, _) => { tag: ReturnDataCopy, mnemonic: "returndatacopy", phases: PhaseSet::ALL, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: None },

    Self::Caller => { tag: Caller, mnemonic: "caller", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::CallValue => { tag: CallValue, mnemonic: "callvalue", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::Origin => { tag: Origin, mnemonic: "origin", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::GasPrice => { tag: GasPrice, mnemonic: "gasprice", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::BlockHash(_) => { tag: BlockHash, mnemonic: "blockhash", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Coinbase => { tag: Coinbase, mnemonic: "coinbase", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::Timestamp => { tag: Timestamp, mnemonic: "timestamp", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::BlockNumber => { tag: BlockNumber, mnemonic: "number", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::PrevRandao => { tag: PrevRandao, mnemonic: "prevrandao", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::GasLimit => { tag: GasLimit, mnemonic: "gaslimit", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::SlotNum => { tag: SlotNum, mnemonic: "slotnum", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::ChainId => { tag: ChainId, mnemonic: "chainid", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::Address => { tag: Address, mnemonic: "address", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::Balance(_) => { tag: Balance, mnemonic: "balance", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::SelfBalance => { tag: SelfBalance, mnemonic: "selfbalance", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Gas => { tag: Gas, mnemonic: "gas", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::BaseFee => { tag: BaseFee, mnemonic: "basefee", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::BlobBaseFee => { tag: BlobBaseFee, mnemonic: "blobbasefee", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::BlobHash(_) => { tag: BlobHash, mnemonic: "blobhash", phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: None },

    Self::Keccak256(_, _) => { tag: Keccak256, mnemonic: "keccak256", phases: PhaseSet::ALL, effect: MemoryRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Keccak256Bytes(_) => { tag: Keccak256Bytes, mnemonic: "keccak256_bytes", phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryRead, traits: OpTraits::MEMORY_OBJECT, side_effects: false, category: Some("memory-object") },
    Self::MappingSlot(_, _) => { tag: MappingSlot, mnemonic: "mapping_slot", phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: MemoryRead, traits: OpTraits::NONE, side_effects: false, category: Some("storage slot") },
    Self::MappingSlotMemory(_, _) => { tag: MappingSlotMemory, mnemonic: "mapping_slot_memory", phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: MemoryRead, traits: OpTraits::NONE, side_effects: false, category: Some("storage slot") },
    Self::MappingSlotCalldata(_, _) => { tag: MappingSlotCalldata, mnemonic: "mapping_slot_calldata", phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: Some("storage slot") },
    Self::StorageArrayDataSlot(_) => { tag: StorageArrayDataSlot, mnemonic: "storage_array_data_slot", phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: Some("storage slot") },
    Self::StorageArrayElementSlot { .. } => { tag: StorageArrayElementSlot, mnemonic: "storage_array_element_slot", phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: Some("storage slot") },

    Self::Call { .. } => { tag: Call, mnemonic: "call", phases: PhaseSet::ALL, effect: ExternalCall, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::CallCode { .. } => { tag: CallCode, mnemonic: "callcode", phases: PhaseSet::ALL, effect: ExternalCall, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::StaticCall { .. } => { tag: StaticCall, mnemonic: "staticcall", phases: PhaseSet::ALL, effect: ExternalCall, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::DelegateCall { .. } => { tag: DelegateCall, mnemonic: "delegatecall", phases: PhaseSet::ALL, effect: ExternalCall, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::ExtCall { .. } => { tag: ExtCall, mnemonic: "extcall", phases: PhaseSet::ALL, effect: ExternalCall, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::ExtDelegateCall { .. } => { tag: ExtDelegateCall, mnemonic: "extdelegatecall", phases: PhaseSet::ALL, effect: ExternalCall, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::ExtStaticCall { .. } => { tag: ExtStaticCall, mnemonic: "extstaticcall", phases: PhaseSet::ALL, effect: ExternalCall, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::InternalCall { .. } => { tag: InternalCall, mnemonic: "internal_call", phases: PhaseSet::ALL, effect: InternalCall, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::Create(_, _, _) => { tag: Create, mnemonic: "create", phases: PhaseSet::ALL, effect: Create, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::Create2(_, _, _, _) => { tag: Create2, mnemonic: "create2", phases: PhaseSet::ALL, effect: Create, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::Log0(_, _) => { tag: Log0, mnemonic: "log0", phases: PhaseSet::ALL, effect: Log, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::Log1(_, _, _) => { tag: Log1, mnemonic: "log1", phases: PhaseSet::ALL, effect: Log, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::Log2(_, _, _, _) => { tag: Log2, mnemonic: "log2", phases: PhaseSet::ALL, effect: Log, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::Log3(_, _, _, _, _) => { tag: Log3, mnemonic: "log3", phases: PhaseSet::ALL, effect: Log, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::Log4(_, _, _, _, _, _) => { tag: Log4, mnemonic: "log4", phases: PhaseSet::ALL, effect: Log, traits: OpTraits::NONE, side_effects: true, category: None },

    Self::Phi(_) => { tag: Phi, mnemonic: "phi", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Select(_, _, _) => { tag: Select, mnemonic: "select", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::SignExtend(_, _) => { tag: SignExtend, mnemonic: "signextend", phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{AllocationKind, AllocationSemantics, ValueId};

    #[test]
    fn descriptors_drive_operation_properties() {
        let add = InstKind::Add(ValueId::new(0), ValueId::new(1));
        assert_eq!(add.op_def().tag, OpTag::Add);
        assert_eq!(add.mnemonic(), "add");
        assert_eq!(add.effect_kind(), EffectKind::Pure);
        assert!(add.op_def().traits.contains(OpTraits::REORDERABLE));
        assert!(!add.has_side_effects());

        let calldata_size = InstKind::CalldataSize;
        assert!(calldata_size.is_always_rematerializable());
        assert!(calldata_size.op_def().phases.contains(MirPhase::EvmShaped));
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
