//! Instruction effects shared by elimination, value numbering, and code motion.
//!
//! Resource footprints belong to ModRef and alias analysis. These small derived properties
//! describe behavior that memory purity alone cannot capture: failure, divergence, external
//! termination, allocation identity, and observations of execution. Speculation additionally
//! requires dependence and profitability proofs; a memory read may expand EVM memory.

use super::{EffectKind, InstKind};

/// Control behavior that must survive even when an operation's result is unused.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ControlEffects {
    pub(crate) may_revert: bool,
    pub(crate) may_diverge: bool,
    pub(crate) may_terminate: bool,
}

impl ControlEffects {
    pub(crate) const NONE: Self =
        Self { may_revert: false, may_diverge: false, may_terminate: false };
    pub(crate) const UNKNOWN: Self =
        Self { may_revert: true, may_diverge: true, may_terminate: true };

    pub(crate) const fn any(self) -> bool {
        self.may_revert || self.may_diverge || self.may_terminate
    }

    pub(crate) fn merge(&mut self, other: Self) {
        self.may_revert |= other.may_revert;
        self.may_diverge |= other.may_diverge;
        self.may_terminate |= other.may_terminate;
    }
}

/// Derived instruction properties, never stored alongside every instruction.
#[derive(Clone, Copy, Debug)]
pub(crate) struct InstructionEffects {
    pub(crate) control: ControlEffects,
    pub(crate) observable: bool,
    pub(crate) expands_memory: bool,
    observes_execution: bool,
    has_identity: bool,
}

impl InstructionEffects {
    /// Whether removing an unused operation could remove required behavior.
    pub(crate) const fn must_execute(self, observes_msize: bool) -> bool {
        self.observable || self.control.any() || (observes_msize && self.expands_memory)
    }

    /// Whether equal operands and unchanged read dependencies can justify sharing a result.
    pub(crate) const fn can_common(self) -> bool {
        !self.must_execute(false) && !self.observes_execution && !self.has_identity
    }

    /// Whether motion still needs an execution guarantee or a more specific safety proof.
    pub(crate) const fn can_speculate(self) -> bool {
        self.can_common() && !self.expands_memory
    }
}

impl InstKind {
    /// Returns context-free effects; use alias and call summaries for resource footprints.
    pub(crate) const fn effects(&self) -> InstructionEffects {
        let control = match self {
            Self::ValidateStorageBytes(..)
            | Self::StorageBytesLoad(..)
            | Self::StorageArrayLoad { .. }
            | Self::StorageBytesStore(..)
            | Self::StorageBytesStoreLiteral { .. }
            | Self::ValidateAbi(..)
            | Self::CheckedAddMod(..)
            | Self::CheckedMulMod(..)
            | Self::CheckedBinary { .. }
            | Self::Check { .. }
            | Self::Require { .. } => ControlEffects { may_revert: true, ..ControlEffects::NONE },
            Self::ICall { .. } => ControlEffects::UNKNOWN,
            Self::Alloc { semantics, .. } => ControlEffects {
                may_revert: matches!(semantics.failure, super::AllocationFailure::Panic),
                ..ControlEffects::NONE
            },
            Self::Erc7201(..)
            | Self::Concat(..)
            | Self::Sha256(..)
            | Self::Ripemd160(..)
            | Self::EcRecover(..) => ControlEffects { may_revert: true, ..ControlEffects::NONE },
            Self::AbiEncode { .. } | Self::AbiEncodePacked { .. } => {
                ControlEffects { may_revert: true, ..ControlEffects::NONE }
            }
            Self::AbiDecode { .. } => ControlEffects { may_revert: true, ..ControlEffects::NONE },
            Self::ReturnDataCopy(..) => ControlEffects { may_revert: true, ..ControlEffects::NONE },
            Self::InsertValue { .. }
            | Self::ExtractValue { .. }
            | Self::MemoryObjectFromPtr { .. }
            | Self::WordCast(..)
            | Self::Add(..)
            | Self::Sub(..)
            | Self::Mul(..)
            | Self::Div(..)
            | Self::SDiv(..)
            | Self::Mod(..)
            | Self::SMod(..)
            | Self::Exp(..)
            | Self::AddMod(..)
            | Self::MulMod(..)
            | Self::And(..)
            | Self::Or(..)
            | Self::Xor(..)
            | Self::Not(..)
            | Self::Clz(..)
            | Self::Shl(..)
            | Self::Shr(..)
            | Self::Sar(..)
            | Self::Byte(..)
            | Self::Lt(..)
            | Self::Gt(..)
            | Self::SLt(..)
            | Self::SGt(..)
            | Self::Eq(..)
            | Self::IsZero(..)
            | Self::MLoad(..)
            | Self::MStore(..)
            | Self::MStore8(..)
            | Self::MemoryZero(..)
            | Self::MSize
            | Self::Fmp
            | Self::SetFmp(..)
            | Self::MemoryObjectLen(..)
            | Self::SetMemoryObjectLen(..)
            | Self::MemoryObjectData(..)
            | Self::MemoryObjectFieldAddr { .. }
            | Self::MemoryObjectElementAddr { .. }
            | Self::MemoryObjectLoadField { .. }
            | Self::MemoryObjectStoreField { .. }
            | Self::MemoryObjectLoadElement { .. }
            | Self::MemoryObjectLoadByte { .. }
            | Self::MemoryObjectStoreElement { .. }
            | Self::MemoryObjectStoreByte { .. }
            | Self::MemoryObjectStoreWord { .. }
            | Self::MemorySliceLoadWord { .. }
            | Self::CalldataSliceLoadWord { .. }
            | Self::MemoryObjectCopyFromSlice { .. }
            | Self::MemoryObjectCopyFromSliceAt { .. }
            | Self::MemoryObjectCopy { .. }
            | Self::StorageToMemory { .. }
            | Self::MemoryToStorage { .. }
            | Self::ClearStorage { .. }
            | Self::MCopy(..)
            | Self::SLoad(..)
            | Self::SStore(..)
            | Self::TLoad(..)
            | Self::TStore(..)
            | Self::CalldataLoad(..)
            | Self::CalldataCopy(..)
            | Self::CalldataSize
            | Self::MakeSlice { .. }
            | Self::SlicePtr(..)
            | Self::SliceLen(..)
            | Self::InternalFrameAddr(..)
            | Self::FrameLoad { .. }
            | Self::FrameStore { .. }
            | Self::ConstructorArgsBase
            | Self::ConstructorArgsEnd
            | Self::DataCopy(..)
            | Self::CodeSize
            | Self::CodeCopy(..)
            | Self::ExtCodeSize(..)
            | Self::ExtCodeCopy(..)
            | Self::ExtCodeHash(..)
            | Self::StoreImmutable(..)
            | Self::LoadImmutable(..)
            | Self::ReturnDataSize
            | Self::Caller
            | Self::CallValue
            | Self::Origin
            | Self::GasPrice
            | Self::BlockHash(..)
            | Self::Coinbase
            | Self::Timestamp
            | Self::BlockNumber
            | Self::PrevRandao
            | Self::GasLimit
            | Self::SlotNum
            | Self::ChainId
            | Self::Address
            | Self::Balance(..)
            | Self::SelfBalance
            | Self::Gas
            | Self::BaseFee
            | Self::BlobBaseFee
            | Self::BlobHash(..)
            | Self::Keccak256(..)
            | Self::Keccak256Bytes(..)
            | Self::MappingSlot(..)
            | Self::MappingSlotMemory(..)
            | Self::MappingSlotCalldata(..)
            | Self::StorageArrayDataSlot(..)
            | Self::StorageClearWords(..)
            | Self::StorageArrayElementSlot { .. }
            | Self::Call { .. }
            | Self::CallCode { .. }
            | Self::StaticCall { .. }
            | Self::DelegateCall { .. }
            | Self::ExtCall { .. }
            | Self::ExtDelegateCall { .. }
            | Self::ExtStaticCall { .. }
            | Self::Create(..)
            | Self::Create2(..)
            | Self::Log0(..)
            | Self::Log1(..)
            | Self::Log2(..)
            | Self::Log3(..)
            | Self::Log4(..)
            | Self::Phi(..)
            | Self::Select(..)
            | Self::SignExtend(..) => ControlEffects::NONE,
        };
        let kind = self.effect_kind();
        InstructionEffects {
            control,
            observable: matches!(
                kind,
                EffectKind::MemoryWrite
                    | EffectKind::StorageWrite
                    | EffectKind::TransientWrite
                    | EffectKind::ImmutableWrite
                    | EffectKind::ExternalCall
                    | EffectKind::ICall
                    | EffectKind::Create
                    | EffectKind::Log
            ),
            expands_memory: matches!(
                kind,
                EffectKind::MemoryRead
                    | EffectKind::MemoryWrite
                    | EffectKind::ExternalCall
                    | EffectKind::Create
                    | EffectKind::Log
            ) || matches!(self, Self::StorageBytesStore(..)),
            observes_execution: matches!(self, Self::Gas | Self::MSize),
            has_identity: matches!(
                self,
                Self::Alloc { .. }
                    | Self::AbiEncode { .. }
                    | Self::AbiEncodePacked { .. }
                    | Self::StorageToMemory { .. }
                    | Self::StorageBytesLoad(..)
                    | Self::StorageArrayLoad { .. }
                    | Self::Concat(..)
            ),
        }
    }
}
