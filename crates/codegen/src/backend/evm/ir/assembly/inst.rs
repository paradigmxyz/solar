//! Compact instructions for finalized, layout-linear EVM IR.

use solar_data_structures::{index::Idx, newtype_index};

newtype_index! {
    /// A label identifier.
    pub(crate) struct Label;

    /// A deferred constant identifier.
    ///
    /// Deferred constants are immediates whose final value is only known after
    /// bytecode emission has observed lazy backend state, such as exact spill
    /// slot allocation. They must be resolved before assembly.
    pub(crate) struct DeferredConst;

    /// An interned push immediate identifier.
    pub(in crate::backend::evm) struct PushValueId;
}

pub(in crate::backend::evm) trait AsmIndex: Idx {
    const NAME: &'static str;

    fn inst_payload(self) -> u32 {
        let index =
            u32::try_from(self.index()).unwrap_or_else(|_| panic!("{} overflow", Self::NAME));
        assert!(index <= AsmInst::PAYLOAD_MASK, "{} overflow", Self::NAME);
        index
    }

    fn from_inst_payload(payload: u32) -> Self {
        Self::from_usize(payload as usize)
    }
}

impl AsmIndex for Label {
    const NAME: &'static str = "assembler label index";
}

impl AsmIndex for DeferredConst {
    const NAME: &'static str = "assembler deferred constant index";
}

impl AsmIndex for PushValueId {
    const NAME: &'static str = "assembler push value index";
}

/// An instruction in the assembler.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::backend::evm) struct AsmInst(u32);

impl AsmInst {
    pub(in crate::backend::evm) const PAYLOAD_MASK: u32 = 0x0fff_ffff;
    const INLINE_PUSH_MAX: u32 = 0x7fff_ffff;
    const TAG_MASK: u32 = 0xf000_0000;
    const TAG_OP: u32 = 0x8000_0000;
    const TAG_PUSH: u32 = 0x9000_0000;
    const TAG_PUSH_LABEL: u32 = 0xa000_0000;
    const TAG_PUSH_DEFERRED: u32 = 0xb000_0000;
    const TAG_PUSH_IMMUTABLE: u32 = 0xc000_0000;
    const TAG_LABEL: u32 = 0xd000_0000;

    pub(in crate::backend::evm) fn op(opcode: u8) -> Self {
        Self(Self::TAG_OP | u32::from(opcode))
    }

    pub(in crate::backend::evm) fn push_inline(value: u32) -> Option<Self> {
        (value <= Self::INLINE_PUSH_MAX).then_some(Self(value))
    }

    pub(in crate::backend::evm) fn push(index: PushValueId) -> Self {
        Self::tagged(Self::TAG_PUSH, index.inst_payload())
    }

    pub(in crate::backend::evm) fn push_label(label: Label) -> Self {
        Self::tagged(Self::TAG_PUSH_LABEL, label.inst_payload())
    }

    pub(in crate::backend::evm) fn push_deferred(id: DeferredConst) -> Self {
        Self::tagged(Self::TAG_PUSH_DEFERRED, id.inst_payload())
    }

    pub(in crate::backend::evm) fn push_immutable(id: u32) -> Self {
        Self::tagged(Self::TAG_PUSH_IMMUTABLE, id)
    }

    pub(in crate::backend::evm) fn label(label: Label) -> Self {
        Self::tagged(Self::TAG_LABEL, label.inst_payload())
    }

    fn tagged(tag: u32, payload: u32) -> Self {
        assert!(payload <= Self::PAYLOAD_MASK, "assembler instruction payload overflow");
        Self(tag | payload)
    }

    pub(in crate::backend::evm) fn kind(self) -> AsmInstKind {
        if self.0 <= Self::INLINE_PUSH_MAX {
            return AsmInstKind::PushInline(self.0);
        }

        let payload = self.0 & Self::PAYLOAD_MASK;
        match self.0 & Self::TAG_MASK {
            Self::TAG_OP => AsmInstKind::Op(payload as u8),
            Self::TAG_PUSH => AsmInstKind::Push(PushValueId::from_inst_payload(payload)),
            Self::TAG_PUSH_LABEL => AsmInstKind::PushLabel(Label::from_inst_payload(payload)),
            Self::TAG_PUSH_DEFERRED => {
                AsmInstKind::PushDeferred(DeferredConst::from_inst_payload(payload))
            }
            Self::TAG_PUSH_IMMUTABLE => AsmInstKind::PushImmutable(payload),
            Self::TAG_LABEL => AsmInstKind::Label(Label::from_inst_payload(payload)),
            _ => unreachable!("invalid assembler instruction tag"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::backend::evm) enum AsmInstKind {
    Op(u8),
    PushInline(u32),
    Push(PushValueId),
    PushLabel(Label),
    PushDeferred(DeferredConst),
    PushImmutable(u32),
    Label(Label),
}
