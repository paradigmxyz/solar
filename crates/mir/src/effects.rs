//! Instruction effects shared by elimination, value numbering, and code motion.
//!
//! Resource footprints belong to ModRef and alias analysis. These small derived properties
//! describe behavior that memory purity alone cannot capture: failure, divergence, external
//! termination, allocation identity, and observations of execution. Speculation additionally
//! requires dependence and profitability proofs; a memory read may expand EVM memory.

use super::{EffectKind, InstKind};
use crate::{AddressCallKind, Builtin, Callee, PtrLocation};

/// Control behavior that must survive even when an operation's result is unused.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ControlEffects {
    pub may_revert: bool,
    pub may_diverge: bool,
    pub may_terminate: bool,
}

impl ControlEffects {
    pub const NONE: Self = Self { may_revert: false, may_diverge: false, may_terminate: false };
    pub const UNKNOWN: Self = Self { may_revert: true, may_diverge: true, may_terminate: true };

    pub const fn any(self) -> bool {
        self.may_revert || self.may_diverge || self.may_terminate
    }

    pub fn merge(&mut self, other: Self) {
        self.may_revert |= other.may_revert;
        self.may_diverge |= other.may_diverge;
        self.may_terminate |= other.may_terminate;
    }
}

/// Derived instruction properties, never stored alongside every instruction.
#[derive(Clone, Copy, Debug)]
pub struct InstructionEffects {
    pub control: ControlEffects,
    pub observable: bool,
    pub expands_memory: bool,
    observes_execution: bool,
    has_identity: bool,
    idempotent_write: bool,
}

impl InstructionEffects {
    /// Whether removing an unused operation could remove required behavior.
    pub(crate) const fn must_execute(self) -> bool {
        self.observable || self.control.any()
    }

    /// Whether equal operands and unchanged read/write dependencies allow sharing a result.
    pub const fn can_common(self) -> bool {
        (!self.observable || self.idempotent_write)
            && !self.control.any()
            && !self.observes_execution
            && !self.has_identity
    }

    /// Whether motion still needs an execution guarantee or a more specific safety proof.
    pub const fn can_speculate(self) -> bool {
        self.can_common() && !self.observable && !self.expands_memory
    }

    /// Whether the result reads the state of execution, such as remaining gas or memory size,
    /// so two evaluations with the same operands can differ.
    pub const fn observes_execution(self) -> bool {
        self.observes_execution
    }
}

impl InstKind {
    /// Whether this operation reads remaining gas, directly or during builtin expansion.
    pub const fn observes_gas(&self) -> bool {
        matches!(
            self,
            Self::Gas
                | Self::ICall {
                    function: Callee::Builtin(
                        Builtin::Sha256 | Builtin::Ripemd160 | Builtin::EcRecover
                    ),
                    ..
                }
                | Self::AddressCall { gas: None, .. }
        )
    }

    /// Whether this operation reads account state, a balance or deployed code, which
    /// [`may_change_account_state`](Self::may_change_account_state) operations can change.
    pub const fn reads_account_state(&self) -> bool {
        matches!(
            self,
            Self::ExtCodeSize(_) | Self::ExtCodeHash(_) | Self::Balance(_) | Self::SelfBalance
        )
    }

    /// Whether this operation can move value or deploy or destroy code. Static calls cannot:
    /// the static context forbids value transfers, `CREATE`, and `SELFDESTRUCT`.
    pub const fn may_change_account_state(&self) -> bool {
        matches!(
            self.effect_kind(),
            EffectKind::ExternalCall | EffectKind::ICall | EffectKind::Create
        ) && !matches!(
            self,
            Self::StaticCall { .. }
                | Self::AddressCall { kind: AddressCallKind::Static, .. }
                | Self::ICall {
                    function: Callee::Builtin(
                        Builtin::Sha256 | Builtin::Ripemd160 | Builtin::EcRecover
                    ),
                    ..
                }
        )
    }

    /// Returns context-free effects; use alias and call summaries for resource footprints.
    pub const fn effects(&self) -> InstructionEffects {
        let control = match self {
            Self::ICall {
                function: Callee::Builtin(Builtin::ReturndataBytes | Builtin::Transfer),
                ..
            }
            | Self::ValidateStorageBytes(..)
            | Self::StorageBytesLoad(..)
            | Self::StorageArrayLoad { .. }
            | Self::StorageBytesStore(..)
            | Self::StorageBytesStoreLiteral { .. }
            | Self::ValidateAbi(..)
            | Self::ICall {
                function: Callee::Builtin(Builtin::CheckedAddMod | Builtin::CheckedMulMod),
                ..
            }
            | Self::CheckedBinary { .. }
            | Self::ICall {
                function: Callee::Builtin(Builtin::Check { .. } | Builtin::Require(_)),
                ..
            } => ControlEffects { may_revert: true, ..ControlEffects::NONE },
            Self::ICall { function: Callee::Function(_), .. } => ControlEffects::UNKNOWN,
            Self::Alloc { semantics, .. } => ControlEffects {
                may_revert: matches!(semantics.failure, super::AllocationFailure::Checked),
                ..ControlEffects::NONE
            },
            Self::ICall {
                function:
                    Callee::Builtin(
                        Builtin::Erc7201
                        | Builtin::Sha256
                        | Builtin::Ripemd160
                        | Builtin::EcRecover,
                    ),
                ..
            } => ControlEffects { may_revert: true, ..ControlEffects::NONE },
            Self::AbiEncode { .. } | Self::AbiEncodePacked { .. } => {
                ControlEffects { may_revert: true, ..ControlEffects::NONE }
            }
            Self::AbiDecode { .. } => ControlEffects { may_revert: true, ..ControlEffects::NONE },
            Self::Copy(.., PtrLocation::Returndata) => {
                ControlEffects { may_revert: true, ..ControlEffects::NONE }
            }
            Self::InsertValue { .. }
            | Self::ExtractValue { .. }
            | Self::Zext(..)
            | Self::Trunc(..)
            | Self::Sext(..)
            | Self::PtrToInt(..)
            | Self::IntToPtr(..)
            | Self::PtrAdd(..)
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
            | Self::Ne(..)
            | Self::Load(..)
            | Self::Store(..)
            | Self::Store8(..)
            | Self::MemoryZero(..)
            | Self::MSize
            | Self::Fmp
            | Self::SetFmp(..)
            | Self::Copy(..)
            | Self::CalldataSize
            | Self::MakeSlice { .. }
            | Self::InternalFrameAddr(..)
            | Self::FrameLoad { .. }
            | Self::FrameStore { .. }
            | Self::ConstructorArgsBase
            | Self::ConstructorArgsEnd
            | Self::HeapFloor
            | Self::DataCopy(..)
            | Self::CodeSize
            | Self::CodeCopy(..)
            | Self::ExtCodeSize(..)
            | Self::ExtCodeCopy(..)
            | Self::ExtCodeHash(..)
            | Self::StoreImmutable(..)
            | Self::LibraryAddress(..)
            | Self::DataSize(..)
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
            | Self::MappingSlot(..)
            | Self::MappingSlotBytes(..)
            | Self::StorageArrayDataSlot(..)
            | Self::StorageClearWords(..)
            | Self::StorageArrayElementSlot { .. }
            | Self::ICall { function: Callee::Builtin(Builtin::Send), .. }
            | Self::AddressCall { .. }
            | Self::Call { .. }
            | Self::CallCode { .. }
            | Self::StaticCall { .. }
            | Self::DelegateCall { .. }
            | Self::Create(..)
            | Self::Create2(..)
            | Self::Log { .. }
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
            ) || matches!(
                self,
                Self::StorageBytesStore(..)
                    | Self::StorageBytesStoreLiteral { .. }
                    | Self::StorageClearWords(..)
            ),
            observes_execution: self.observes_gas() || matches!(self, Self::MSize),
            idempotent_write: matches!(
                self,
                Self::MappingSlot(..)
                    | Self::StorageArrayDataSlot(..)
                    | Self::StorageArrayElementSlot { .. }
            ),
            has_identity: matches!(
                self,
                Self::Alloc { .. }
                    | Self::AbiEncode { .. }
                    | Self::AbiEncodePacked { .. }
                    | Self::StorageBytesLoad(..)
                    | Self::StorageArrayLoad { .. }
                    | Self::ICall { function: Callee::Builtin(Builtin::ReturndataBytes), .. }
            ),
        }
    }
}
