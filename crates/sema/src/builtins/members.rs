use crate::{
    hir,
    ty::{Gcx, Ty, TyFnPtr, TyKind},
};
use solar_ast::ast::{DataLocation, ElementaryType, StateMutability as SM};
use solar_interface::{kw, sym, Symbol};

pub(crate) use super::builtin_members::*;

pub type MemberMap<'gcx> = &'gcx [Member<'gcx>];
pub(crate) type MemberMapOwned<'gcx> = Vec<Member<'gcx>>;

#[derive(Clone, Copy)]
pub struct Member<'gcx> {
    pub name: Symbol,
    pub ty: Ty<'gcx>,
    pub id: Option<hir::ItemId>,
}

impl<'gcx> Member<'gcx> {
    pub fn new(name: Symbol, ty: Ty<'gcx>) -> Self {
        Self { name, ty, id: None }
    }

    pub fn with_id(name: Symbol, ty: Ty<'gcx>, id: hir::ItemId) -> Self {
        Self { name, ty, id: Some(id) }
    }
}

pub(crate) fn address_payable(gcx: Gcx<'_>) -> MemberMapOwned<'_> {
    address_iter(gcx).chain(_address_payable_iter(gcx)).collect()
}

pub(crate) fn contract(gcx: Gcx<'_>, id: hir::ContractId) -> MemberMapOwned<'_> {
    let c = gcx.hir.contract(id);
    if c.kind.is_library() {
        return MemberMapOwned::default();
    }
    gcx.interface_functions(id)
        .all_functions()
        .iter()
        .map(|f| {
            Member::with_id(
                gcx.item_name(f.id.into()).name,
                f.ty.as_externally_callable_function(&gcx.interner),
                f.id.into(),
            )
        })
        .collect()
}

pub(crate) fn function<'gcx>(gcx: Gcx<'gcx>, f: &'gcx TyFnPtr<'gcx>) -> MemberMapOwned<'gcx> {
    let _ = (gcx, f);
    todo!()
}

pub(crate) fn reference<'gcx>(
    gcx: Gcx<'gcx>,
    this: Ty<'gcx>,
    inner: Ty<'gcx>,
    loc: DataLocation,
) -> MemberMapOwned<'gcx> {
    match (&inner.kind, loc) {
        (&TyKind::Struct(tys, id), _) => {
            let fields = gcx.hir.strukt(id).fields;
            debug_assert_eq!(fields.len(), tys.len());
            fields
                .iter()
                .zip(tys)
                .map(|(f, &ty)| Member::new(f.name.name, ty.with_loc(&gcx.interner, loc)))
                .collect()
        }
        (
            TyKind::DynArray(_) | TyKind::Elementary(ElementaryType::Bytes),
            DataLocation::Storage,
        ) => {
            let inner = if let TyKind::DynArray(inner) = inner.kind {
                inner
            } else {
                gcx.types.fixed_bytes(1)
            };
            vec![
                Member::new(
                    sym::length,
                    gcx.mk_builtin_fn(&[this], SM::View, &[gcx.types.uint(256)]),
                ),
                Member::new(sym::push, gcx.mk_builtin_fn(&[this, inner], SM::NonPayable, &[])),
                Member::new(sym::push, gcx.mk_builtin_fn(&[this], SM::NonPayable, &[inner])),
                Member::new(kw::Pop, gcx.mk_builtin_fn(&[this], SM::NonPayable, &[])),
            ]
        }
        (
            TyKind::Array(..) | TyKind::DynArray(_) | TyKind::Elementary(ElementaryType::Bytes),
            _,
        ) => array(gcx),
        _ => Default::default(),
    }
}

// `Enum.Variant`, `Udvt.wrap`
pub(crate) fn slf<'gcx>(gcx: Gcx<'gcx>, ty: Ty<'gcx>) -> MemberMapOwned<'gcx> {
    match ty.kind {
        // TODO: https://github.com/ethereum/solidity/blob/9d7cc42bc1c12bb43e9dccf8c6c36833fdfcbbca/libsolidity/ast/Types.cpp#L3913
        TyKind::Contract(_) => Default::default(),
        TyKind::Enum(id) => {
            gcx.hir.enumm(id).variants.iter().map(|v| Member::new(v.name, ty)).collect()
        }
        TyKind::Udvt(inner, _id) => {
            vec![
                Member::new(sym::wrap, gcx.mk_builtin_fn(&[inner], SM::Pure, &[ty])),
                Member::new(sym::unwrap, gcx.mk_builtin_fn(&[ty], SM::Pure, &[inner])),
            ]
        }
        TyKind::Elementary(ElementaryType::String) => String(gcx),
        TyKind::Elementary(ElementaryType::Bytes) => Bytes(gcx),
        _ => Default::default(),
    }
}

// `type(T)`
pub(crate) fn meta<'gcx>(gcx: Gcx<'gcx>, ty: Ty<'gcx>) -> MemberMapOwned<'gcx> {
    match ty.kind {
        TyKind::Contract(id) => {
            if gcx.hir.contract(id).can_be_deployed() {
                type_contract(gcx)
            } else {
                type_interface(gcx)
            }
        }
        TyKind::Elementary(ElementaryType::Int(_) | ElementaryType::UInt(_)) | TyKind::Enum(_) => {
            vec![Member::new(sym::min, ty), Member::new(sym::max, ty)]
        }
        _ => Default::default(),
    }
}
