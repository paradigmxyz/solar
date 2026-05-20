use super::Builtin;
use crate::{
    hir,
    ty::{Gcx, Ty, TyFn, TyFnKind, TyKind},
};
use solar_ast::{DataLocation, ElementaryType, StateMutability as SM, Visibility};
use solar_data_structures::BumpExt;
use solar_interface::Symbol;

pub type MemberList<'gcx> = &'gcx [Member<'gcx>];
pub(crate) type MemberListOwned<'gcx> = Vec<Member<'gcx>>;

pub(crate) fn native_members<'gcx>(gcx: Gcx<'gcx>, ty: Ty<'gcx>) -> MemberList<'gcx> {
    let expected_ref = || panic!("native_members: type {ty:?} should be wrapped in Ref");
    gcx.bump().alloc_vec(match ty.kind {
        TyKind::Elementary(elementary_type) => match elementary_type {
            ElementaryType::Address(false) => address(gcx).collect(),
            ElementaryType::Address(true) => address_payable(gcx).collect(),
            ElementaryType::Bool => Default::default(),
            ElementaryType::String => Default::default(),
            ElementaryType::Bytes => expected_ref(),
            ElementaryType::Fixed(..) | ElementaryType::UFixed(..) => Default::default(),
            ElementaryType::Int(_size) => Default::default(),
            ElementaryType::UInt(_size) => Default::default(),
            ElementaryType::FixedBytes(_size) => fixed_bytes(gcx),
        },
        TyKind::StringLiteral(_utf8, _size) => Default::default(),
        TyKind::IntLiteral(..) => Default::default(),
        TyKind::Ref(inner, loc) => reference(gcx, ty, inner, loc),
        TyKind::DynArray(_ty) => expected_ref(),
        TyKind::Array(_ty, _len) => expected_ref(),
        TyKind::Slice(_ty) => Default::default(),
        TyKind::Tuple(_tys) => Default::default(),
        TyKind::Mapping(..) => Default::default(),
        TyKind::Fn(f) => function(gcx, f),
        TyKind::Contract(id) => contract(gcx, id),
        TyKind::Struct(_id) => expected_ref(),
        TyKind::Enum(_id) => Default::default(),
        TyKind::Udvt(_ty, _id) => Default::default(),
        TyKind::Error(_tys, _id) => Member::of_builtins(gcx, [Builtin::FunctionSelector]),
        TyKind::Event(_tys, id) => {
            if gcx.hir.event(id).anonymous {
                Default::default()
            } else {
                Member::of_builtins(gcx, [Builtin::EventSelector])
            }
        }
        TyKind::Module(id) => gcx.symbol_resolver.source_scopes[id]
            .iter()
            .flat_map(|(name, decls)| {
                decls.iter().map(move |decl| Member::new(name, gcx.type_of_res(decl.res)))
            })
            .collect(),
        TyKind::BuiltinModule(builtin) => builtin
            .members()
            .unwrap_or_else(|| panic!("builtin module {builtin:?} has no inner builtins"))
            .map(|b| Member::of_builtin(gcx, b))
            .collect(),
        TyKind::Type(ty) => type_type(gcx, ty),
        TyKind::Meta(ty) => meta(gcx, ty),
        TyKind::Err(_guar) => Default::default(),
    })
}

#[derive(Clone, Copy, Debug)]
pub struct Member<'gcx> {
    pub name: Symbol,
    pub ty: Ty<'gcx>,
    pub res: Option<hir::Res>,
}

impl<'gcx> Member<'gcx> {
    pub fn new(name: Symbol, ty: Ty<'gcx>) -> Self {
        Self { name, ty, res: None }
    }

    pub fn with_res(name: Symbol, ty: Ty<'gcx>, res: impl Into<hir::Res>) -> Self {
        Self { name, ty, res: Some(res.into()) }
    }

    pub fn with_builtin(builtin: Builtin, ty: Ty<'gcx>) -> Self {
        Self::with_res(builtin.name(), ty, builtin)
    }

    pub fn of_builtin(gcx: Gcx<'gcx>, builtin: Builtin) -> Self {
        Self::new(builtin.name(), builtin.ty(gcx))
    }

    pub fn of_builtins(
        gcx: Gcx<'gcx>,
        builtins: impl IntoIterator<Item = Builtin>,
    ) -> MemberListOwned<'gcx> {
        Self::of_builtins_iter(gcx, builtins).collect()
    }

    pub fn of_builtins_iter(
        gcx: Gcx<'gcx>,
        builtins: impl IntoIterator<Item = Builtin>,
    ) -> impl Iterator<Item = Self> {
        builtins.into_iter().map(move |builtin| Self::of_builtin(gcx, builtin))
    }
}

fn address(gcx: Gcx<'_>) -> impl Iterator<Item = Member<'_>> {
    Member::of_builtins_iter(
        gcx,
        [
            Builtin::AddressBalance,
            Builtin::AddressCode,
            Builtin::AddressCodehash,
            Builtin::AddressCall,
            Builtin::AddressDelegatecall,
            Builtin::AddressStaticcall,
        ],
    )
}

fn address_payable(gcx: Gcx<'_>) -> impl Iterator<Item = Member<'_>> {
    address(gcx).chain(Member::of_builtins_iter(
        gcx,
        [Builtin::AddressPayableTransfer, Builtin::AddressPayableSend],
    ))
}

fn fixed_bytes(gcx: Gcx<'_>) -> MemberListOwned<'_> {
    Member::of_builtins(gcx, [Builtin::FixedBytesLength])
}

pub(crate) fn contract(gcx: Gcx<'_>, id: hir::ContractId) -> MemberListOwned<'_> {
    let c = gcx.hir.contract(id);
    if c.kind.is_library() {
        return MemberListOwned::default();
    }
    gcx.interface_functions(id)
        .iter()
        .map(|f| {
            let id = hir::ItemId::from(f.id);
            Member::with_res(
                gcx.item_name(id).name,
                f.ty.as_externally_callable_function(false, gcx),
                id,
            )
        })
        .collect()
}

fn function<'gcx>(gcx: Gcx<'gcx>, f: &'gcx TyFn<'gcx>) -> MemberListOwned<'gcx> {
    let mut members = Vec::with_capacity(2);
    if f.has_selector() {
        members.push(Member::of_builtin(gcx, Builtin::FunctionSelector));
    }
    if f.has_address() {
        members.push(Member::of_builtin(gcx, Builtin::FunctionAddress));
    }
    members
}

fn reference<'gcx>(
    gcx: Gcx<'gcx>,
    this: Ty<'gcx>,
    inner: Ty<'gcx>,
    loc: DataLocation,
) -> MemberListOwned<'gcx> {
    match (&inner.kind, loc) {
        (&TyKind::Struct(id), _) => {
            let fields = gcx.hir.strukt(id).fields;
            let tys = gcx.struct_field_types(id);
            debug_assert_eq!(fields.len(), tys.len());
            fields
                .iter()
                .zip(tys)
                .map(|(&f, &ty)| Member::new(gcx.item_name(f).name, ty.with_loc_if_ref(gcx, loc)))
                .collect()
        }
        (
            TyKind::DynArray(_) | TyKind::Elementary(ElementaryType::Bytes),
            DataLocation::Storage,
        ) => {
            let inner = if let TyKind::DynArray(inner) = inner.kind {
                inner.with_loc_if_ref(gcx, loc)
            } else {
                gcx.types.fixed_bytes(1)
            };
            vec![
                Member::of_builtin(gcx, Builtin::ArrayLength),
                Member::with_builtin(
                    Builtin::ArrayPush0,
                    gcx.mk_builtin_fn(&[this, inner], SM::NonPayable, &[]),
                ),
                Member::with_builtin(
                    Builtin::ArrayPush,
                    gcx.mk_builtin_fn(&[this], SM::NonPayable, &[inner]),
                ),
                Member::with_builtin(
                    Builtin::ArrayPop,
                    gcx.mk_builtin_fn(&[this], SM::NonPayable, &[]),
                ),
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
fn type_type<'gcx>(gcx: Gcx<'gcx>, ty: Ty<'gcx>) -> MemberListOwned<'gcx> {
    match ty.kind {
        TyKind::Contract(id) => contract_type(gcx, id),
        TyKind::Enum(id) => {
            gcx.hir.enumm(id).variants.iter().map(|v| Member::new(v.name, ty)).collect()
        }
        TyKind::Udvt(inner, _id) => {
            vec![
                Member::with_builtin(
                    Builtin::UdvtWrap,
                    gcx.mk_builtin_fn(&[inner], SM::Pure, &[ty]),
                ),
                Member::with_builtin(
                    Builtin::UdvtUnwrap,
                    gcx.mk_builtin_fn(&[ty], SM::Pure, &[inner]),
                ),
            ]
        }
        TyKind::Elementary(ElementaryType::String) => string_ty(gcx),
        TyKind::Elementary(ElementaryType::Bytes) => bytes_ty(gcx),
        _ => Default::default(),
    }
}

fn contract_type(gcx: Gcx<'_>, id: hir::ContractId) -> MemberListOwned<'_> {
    let contract = gcx.hir.contract(id);
    if contract.kind.is_library() {
        contract
            .functions()
            .filter(|&id| {
                let f = gcx.hir.function(id);
                f.is_ordinary() && f.visibility >= Visibility::Internal
            })
            .map(|id| {
                let item = hir::ItemId::from(id);
                Member::with_res(gcx.item_name(item).name, function_via_contract(gcx, id), item)
            })
            .collect()
    } else {
        gcx.interface_functions(id)
            .iter()
            .map(|f| {
                let id = hir::ItemId::from(f.id);
                let ty = function_via_contract(gcx, f.id);
                Member::with_res(gcx.item_name(id).name, ty, id)
            })
            .collect()
    }
}

fn function_via_contract<'gcx>(gcx: Gcx<'gcx>, id: hir::FunctionId) -> Ty<'gcx> {
    let f = gcx.hir.function(id);
    let ty = gcx.type_of_item(id.into());
    if f.contract.is_some_and(|contract_id| gcx.hir.contract(contract_id).kind.is_library()) {
        if f.visibility >= Visibility::Public {
            ty.as_externally_callable_function(true, gcx)
        } else {
            ty
        }
    } else {
        let TyKind::Fn(fn_ty) = ty.kind else { unreachable!() };
        gcx.mk_ty_fn(TyFn {
            kind: TyFnKind::Declaration,
            parameters: fn_ty.parameters,
            returns: fn_ty.returns,
            state_mutability: fn_ty.state_mutability,
            function_id: fn_ty.function_id,
            options: fn_ty.options,
        })
    }
}

// `type(T)`
fn meta<'gcx>(gcx: Gcx<'gcx>, ty: Ty<'gcx>) -> MemberListOwned<'gcx> {
    match ty.kind {
        TyKind::Contract(id) => {
            if gcx.hir.contract(id).can_be_deployed() {
                type_contract(gcx)
            } else {
                type_interface(gcx)
            }
        }
        TyKind::Elementary(ElementaryType::Int(_) | ElementaryType::UInt(_)) | TyKind::Enum(_) => {
            vec![
                Member::with_builtin(Builtin::TypeMin, ty),
                Member::with_builtin(Builtin::TypeMax, ty),
            ]
        }
        _ => Default::default(),
    }
}

fn array(gcx: Gcx<'_>) -> MemberListOwned<'_> {
    Member::of_builtins(gcx, [Builtin::ArrayLength])
}

fn string_ty(gcx: Gcx<'_>) -> MemberListOwned<'_> {
    Member::of_builtins(gcx, [Builtin::StringConcat])
}

fn bytes_ty(gcx: Gcx<'_>) -> MemberListOwned<'_> {
    Member::of_builtins(gcx, [Builtin::BytesConcat])
}

fn type_contract(gcx: Gcx<'_>) -> MemberListOwned<'_> {
    Member::of_builtins(
        gcx,
        [Builtin::ContractCreationCode, Builtin::ContractRuntimeCode, Builtin::ContractName],
    )
}

fn type_interface(gcx: Gcx<'_>) -> MemberListOwned<'_> {
    Member::of_builtins(gcx, [Builtin::InterfaceId, Builtin::ContractName])
}
