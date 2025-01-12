use super::{Interner, Ty, TyFlags, TyKind};
use solar_ast::{DataLocation, ElementaryType, TypeSize};

/// Pre-interned types.
pub struct CommonTypes<'gcx> {
    /// The unit type `()`, AKA empty tuple, void.
    #[doc(alias = "empty_tuple", alias = "void")]
    pub unit: Ty<'gcx>,
    /// `bool`.
    pub bool: Ty<'gcx>,

    /// `address`.
    pub address: Ty<'gcx>,
    /// `address payable`.
    pub address_payable: Ty<'gcx>,

    /// `string`.
    pub string: Ty<'gcx>,
    /// `string` references.
    pub string_ref: EachDataLoc<Ty<'gcx>>,

    /// `bytes`.
    pub bytes: Ty<'gcx>,
    /// `bytes` references.
    pub bytes_ref: EachDataLoc<Ty<'gcx>>,

    ints: [Ty<'gcx>; 32],
    uints: [Ty<'gcx>; 32],
    fbs: [Ty<'gcx>; 32],
}

impl<'gcx> CommonTypes<'gcx> {
    #[instrument(name = "new_common_types", level = "debug", skip_all)]
    #[inline]
    pub(super) fn new(interner: &Interner<'gcx>) -> Self {
        use std::array::from_fn;
        use ElementaryType::*;
        use TyKind::*;

        // NOTE: We need to skip calculating flags here because it would require `Gcx` when we
        // haven't built one yet. This is fine since elementary types don't have any flags.
        // If that ever changes, then this closure should also reflect that.
        let mk = |kind| interner.intern_ty_with_flags(kind, |_| TyFlags::empty());
        let mk_refs = |ty| EachDataLoc {
            storage: mk(Ref(ty, DataLocation::Storage)),
            transient: mk(Ref(ty, DataLocation::Transient)),
            memory: mk(Ref(ty, DataLocation::Memory)),
            calldata: mk(Ref(ty, DataLocation::Calldata)),
        };

        let string = mk(Elementary(String));
        let bytes = mk(Elementary(Bytes));

        Self {
            unit: mk(Tuple(&[])),
            // never: mk(Elementary(Never)),
            bool: mk(Elementary(Bool)),

            address: mk(Elementary(Address(false))),
            address_payable: mk(Elementary(Address(true))),

            string,
            string_ref: mk_refs(string),

            bytes,
            bytes_ref: mk_refs(bytes),

            ints: from_fn(|i| mk(Elementary(Int(TypeSize::new(i as u8 + 1).unwrap())))),
            uints: from_fn(|i| mk(Elementary(UInt(TypeSize::new(i as u8 + 1).unwrap())))),
            fbs: from_fn(|i| mk(Elementary(FixedBytes(TypeSize::new(i as u8 + 1).unwrap())))),
        }
    }

    /// `int<bits>`.
    #[inline]
    #[track_caller]
    pub fn int(&self, bits: u16) -> Ty<'gcx> {
        self.int_(TypeSize::new_int_bits(bits))
    }
    /// `int<size>`.
    #[inline]
    pub fn int_(&self, size: TypeSize) -> Ty<'gcx> {
        self.ints[size.bytes() as usize - 1]
    }

    /// `uint<bits>`.
    #[inline]
    #[track_caller]
    pub fn uint(&self, bits: u16) -> Ty<'gcx> {
        self.uint_(TypeSize::new_int_bits(bits))
    }
    /// `uint<size>`.
    #[inline]
    pub fn uint_(&self, size: TypeSize) -> Ty<'gcx> {
        self.uints[size.bytes() as usize - 1]
    }

    /// `bytes<bytes>`.
    #[inline]
    #[track_caller]
    pub fn fixed_bytes(&self, bytes: u8) -> Ty<'gcx> {
        self.fixed_bytes_(TypeSize::new_fb_bytes(bytes))
    }
    /// `bytes<size>`.
    #[inline]
    pub fn fixed_bytes_(&self, size: TypeSize) -> Ty<'gcx> {
        self.fbs[size.bytes() as usize - 1]
    }
}

/// Holds an instance of `T` for each data location.
pub struct EachDataLoc<T> {
    pub storage: T,
    pub transient: T,
    pub memory: T,
    pub calldata: T,
}

impl<T> EachDataLoc<T> {
    /// Gets a copy for the given data location.
    #[inline]
    pub fn get(&self, loc: DataLocation) -> T
    where
        T: Copy,
    {
        match loc {
            DataLocation::Storage => self.storage,
            DataLocation::Transient => self.transient,
            DataLocation::Memory => self.memory,
            DataLocation::Calldata => self.calldata,
        }
    }

    /// Gets a reference for the given data location.
    #[inline]
    pub fn get_ref(&self, loc: DataLocation) -> &T {
        match loc {
            DataLocation::Storage => &self.storage,
            DataLocation::Transient => &self.transient,
            DataLocation::Memory => &self.memory,
            DataLocation::Calldata => &self.calldata,
        }
    }

    /// Gets a mutable reference for the given data location.
    #[inline]
    pub fn get_mut(&mut self, loc: DataLocation) -> &mut T {
        match loc {
            DataLocation::Storage => &mut self.storage,
            DataLocation::Transient => &mut self.transient,
            DataLocation::Memory => &mut self.memory,
            DataLocation::Calldata => &mut self.calldata,
        }
    }
}

impl<T> std::ops::Index<DataLocation> for EachDataLoc<T> {
    type Output = T;

    #[inline]
    fn index(&self, loc: DataLocation) -> &Self::Output {
        self.get_ref(loc)
    }
}

impl<T> std::ops::IndexMut<DataLocation> for EachDataLoc<T> {
    #[inline]
    fn index_mut(&mut self, loc: DataLocation) -> &mut Self::Output {
        self.get_mut(loc)
    }
}
