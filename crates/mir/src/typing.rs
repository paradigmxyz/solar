//! Helpers for attribute-dependent operand signatures declared in the MIR operation schema.
//! Raw EVM operands use i256; semantic operations retain object and slice types. Result-dependent
//! and module-dependent signatures are checked by their dedicated validators.

use super::{
    AbiLayout, AbiType, Builtin, Callee, Function, MirType, PackedArraySource, PackedPart,
    PtrLocation, RequireKind, ValueId,
};
use smallvec::{SmallVec, smallvec};

impl AbiType {
    /// Returns the carrier consumed by ABI encoding, before layout cleanup.
    pub(crate) fn operand_type(&self) -> MirType {
        match self {
            Self::Word(_) | Self::Function => MirType::I256,
            Self::Bytes(PtrLocation::Memory)
            | Self::DynamicArray { location: PtrLocation::Memory, .. }
            | Self::FixedArray { .. }
            | Self::Tuple(_) => MirType::MEMPTR,
            Self::Bytes(location) | Self::DynamicArray { location, .. } => {
                MirType::Slice(*location)
            }
        }
    }

    /// Checks location-dependent layouts that the encoder can consume without materialization.
    pub(crate) fn accepts_input_type(&self, actual: MirType) -> bool {
        if self.input_type(Some(actual)) != actual {
            return false;
        }
        match (self, actual) {
            (Self::DynamicArray { element, .. }, MirType::Slice(PtrLocation::Returndata)) => {
                matches!(element.as_ref(), Self::Word(_) | Self::Function)
            }
            _ => true,
        }
    }

    fn input_type(&self, actual: Option<MirType>) -> MirType {
        if matches!(
            self,
            Self::Bytes(_) | Self::DynamicArray { .. } | Self::FixedArray { .. } | Self::Tuple(_)
        ) && let Some(ty @ MirType::Slice(PtrLocation::Memory | PtrLocation::Calldata)) = actual
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
        _ => MirType::Slice(PtrLocation::Memory),
    }
}

/// The pointer operand of pointer arithmetic: its own pointer type, or a memory pointer to reject
/// a word.
pub(crate) fn pointer(func: &Function, value: ValueId) -> MirType {
    match func.value_ty(value) {
        Some(ty @ MirType::Ptr(_)) => ty,
        _ => MirType::MEMPTR,
    }
}

/// An address operand in `location`: a pointer into it, or a raw word.
pub(super) fn address(func: &Function, value: ValueId, location: PtrLocation) -> MirType {
    match func.value_ty(value) {
        Some(ty @ MirType::Ptr(actual)) if actual == location => ty,
        _ => MirType::I256,
    }
}

pub(super) fn bytes_key(func: &Function, value: ValueId) -> MirType {
    match func.value_ty(value) {
        Some(ty @ MirType::Slice(PtrLocation::Calldata)) => ty,
        _ => MirType::MEMPTR,
    }
}

pub(super) fn read_object(func: &Function, value: ValueId) -> MirType {
    match func.value_ty(value) {
        Some(ty @ MirType::Slice(PtrLocation::Memory | PtrLocation::Calldata)) => ty,
        _ => MirType::MEMPTR,
    }
}

pub fn memory_object(func: &Function, value: ValueId) -> MirType {
    match func.value_ty(value) {
        Some(ty @ MirType::Slice(PtrLocation::Memory)) => ty,
        _ => MirType::MEMPTR,
    }
}

/// The operands of `address_call`: its input is a bytes object, or a memory slice the call reads
/// where it lies.
pub(super) fn address_call_types(
    func: &Function,
    input: ValueId,
    gas: bool,
    value: bool,
) -> SmallVec<[MirType; 8]> {
    let input = match func.value_ty(input) {
        Some(ty @ MirType::Slice(PtrLocation::Memory)) => ty,
        _ => MirType::MEMPTR,
    };
    let mut types = smallvec![MirType::I160, input];
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
            PackedPart::Bytes(value) => Some(read_object(func, *value)),
            PackedPart::Array { source, .. } => Some(match source {
                PackedArraySource::Memory { .. } => MirType::MEMPTR,
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
    let bytes = MirType::MEMPTR;
    let Callee::Builtin(builtin) = callee else { return None };
    Some(match builtin {
        Builtin::Check { .. } => smallvec![MirType::I1],
        Builtin::Require(kind) => {
            let mut types = smallvec![MirType::I1];
            match kind {
                RequireKind::EmptyString | RequireKind::LongString(_) => {}
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
    })
}

/// Infers a scalar integer signature; invalid operands still fail validation.
pub(super) fn integer_type(func: &Function, value: ValueId) -> MirType {
    match func.value_ty(value) {
        Some(ty @ MirType::Int(_)) => ty,
        _ => MirType::I256,
    }
}
