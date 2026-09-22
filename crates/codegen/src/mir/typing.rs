//! Helpers for attribute-dependent operand signatures declared in the MIR operation schema.
//! Raw EVM operands use i256; semantic operations retain object and slice types. Result-dependent
//! and module-dependent signatures are checked by their dedicated validators.

use super::{
    AbiLayout, AbiType, Builtin, Callee, Function, MemoryObjectKind, MirType, PackedArraySource,
    PackedPart, RequireKind, SliceLocation, StorageLayout, ValueId,
};
use smallvec::{SmallVec, smallvec};

impl AbiType {
    /// Returns the carrier consumed by ABI encoding, before layout cleanup.
    pub(crate) fn operand_type(&self) -> MirType {
        match self {
            Self::Word(_) | Self::Function => MirType::I256,
            Self::Bytes(SliceLocation::Memory) => MirType::MemoryObject(MemoryObjectKind::Bytes),
            Self::Bytes(location) => MirType::Slice(*location),
            Self::DynamicArray { location: SliceLocation::Memory, .. } => {
                MirType::MemoryObject(MemoryObjectKind::DynamicArray)
            }
            Self::DynamicArray { location, .. } => MirType::Slice(*location),
            Self::FixedArray { .. } => MirType::MemoryObject(MemoryObjectKind::FixedArray),
            Self::Tuple(_) => MirType::MemoryObject(MemoryObjectKind::Struct),
        }
    }

    /// Checks location-dependent layouts that the encoder can consume without materialization.
    pub(crate) fn accepts_input_type(&self, actual: MirType) -> bool {
        if self.input_type(Some(actual)) != actual {
            return false;
        }
        match (self, actual) {
            (Self::FixedArray { .. } | Self::Tuple(_), MirType::Slice(SliceLocation::Calldata)) => {
                !self.is_dynamic()
            }
            (
                Self::DynamicArray { element, .. },
                MirType::Slice(location @ (SliceLocation::Calldata | SliceLocation::Returndata)),
            ) => {
                matches!(element.as_ref(), Self::Word(_) | Self::Function)
                    || (location == SliceLocation::Calldata
                        && matches!(element.as_ref(), Self::Bytes(_)))
            }
            _ => true,
        }
    }

    fn input_type(&self, actual: Option<MirType>) -> MirType {
        if matches!(
            self,
            Self::Bytes(_) | Self::DynamicArray { .. } | Self::FixedArray { .. } | Self::Tuple(_)
        ) && let Some(ty @ MirType::Slice(SliceLocation::Memory | SliceLocation::Calldata)) =
            actual
        {
            ty
        } else {
            self.operand_type()
        }
    }
}

pub(super) fn slice_type(func: &Function, value: ValueId) -> MirType {
    match func.value_ty(value) {
        Some(ty @ MirType::Slice(_)) => ty,
        _ => MirType::Slice(SliceLocation::Memory),
    }
}

pub(super) fn read_object(func: &Function, value: ValueId, kind: MemoryObjectKind) -> MirType {
    match func.value_ty(value) {
        Some(ty @ MirType::Slice(SliceLocation::Memory | SliceLocation::Calldata)) => ty,
        _ => MirType::MemoryObject(kind),
    }
}

pub(super) fn memory_object(func: &Function, value: ValueId, kind: MemoryObjectKind) -> MirType {
    match func.value_ty(value) {
        Some(ty @ MirType::Slice(SliceLocation::Memory)) => ty,
        _ => MirType::MemoryObject(kind),
    }
}

pub(super) fn storage_object_type(layout: &StorageLayout) -> MirType {
    MirType::MemoryObject(match layout {
        StorageLayout::Struct(_) => MemoryObjectKind::Struct,
        StorageLayout::Array { .. } => MemoryObjectKind::FixedArray,
    })
}

pub(super) fn address_call_types(gas: bool, value: bool) -> SmallVec<[MirType; 8]> {
    let mut types = smallvec![MirType::I160, MirType::MemoryObject(MemoryObjectKind::Bytes)];
    types.extend(std::iter::repeat_n(MirType::I256, usize::from(gas) + usize::from(value)));
    types
}

pub(super) fn abi_encode_types(
    func: &Function,
    selector: bool,
    args: &[ValueId],
    layout: &AbiLayout,
) -> SmallVec<[MirType; 8]> {
    std::iter::repeat_n(MirType::I256, usize::from(selector))
        .chain(layout.types.iter().enumerate().map(|(index, ty)| {
            ty.input_type(args.get(index).and_then(|&value| func.value_ty(value)))
        }))
        .collect()
}

pub(super) fn packed_types(func: &Function, parts: &[PackedPart]) -> SmallVec<[MirType; 8]> {
    parts
        .iter()
        .filter_map(|part| match part {
            PackedPart::Literal(_) => None,
            PackedPart::Scalar { ty, .. } => Some(ty.mir_type()),
            PackedPart::Bytes(value) => Some(read_object(func, *value, MemoryObjectKind::Bytes)),
            PackedPart::Array { source, .. } => Some(match source {
                PackedArraySource::Memory { layout } => MirType::MemoryObject(layout.kind()),
                PackedArraySource::Slice(location) => MirType::Slice(*location),
            }),
        })
        .collect()
}

pub(super) fn callee_types(
    func: &Function,
    callee: &Callee,
    args: &[ValueId],
) -> Option<SmallVec<[MirType; 8]>> {
    let bytes = MirType::MemoryObject(MemoryObjectKind::Bytes);
    let Callee::Builtin(builtin) = callee else { return None };
    Some(match builtin {
        Builtin::Check { .. } => smallvec![MirType::I1],
        Builtin::Require(kind) => {
            let mut types = smallvec![MirType::I1];
            match kind {
                RequireKind::EmptyString => {}
                RequireKind::ShortString => types.extend([MirType::I256, MirType::I256]),
                RequireKind::ErrorString => types.push(bytes),
                RequireKind::CustomError(layout) => {
                    types.extend(abi_encode_types(
                        func,
                        true,
                        args.get(2..).unwrap_or_default(),
                        layout,
                    ));
                }
            }
            types
        }
        Builtin::Sha256 | Builtin::Ripemd160 | Builtin::Erc7201 => smallvec![bytes],
        Builtin::Send | Builtin::Transfer => smallvec![MirType::I160, MirType::I256],
        Builtin::EcRecover => smallvec![MirType::I256; 4],
        Builtin::CheckedAddMod | Builtin::CheckedMulMod => smallvec![MirType::I256; 3],
        Builtin::ReturndataBytes => smallvec![],
        Builtin::Concat(types) => types.iter().map(|ty| ty.mir_type()).collect(),
    })
}
