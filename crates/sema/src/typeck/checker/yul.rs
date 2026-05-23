use super::TypeChecker;
use crate::{
    hir,
    ty::{Ty, TyKind},
};
use solar_ast::{DataLocation, ElementaryType, LitKind, Span};
use solar_interface::{Ident, Symbol, diagnostics::ErrorGuaranteed, kw, sym};

impl<'gcx> TypeChecker<'gcx> {
    pub(super) fn check_yul_lit(&self, lit: &'gcx hir::Lit<'gcx>) -> Ty<'gcx> {
        match &lit.kind {
            LitKind::Str(_, s, _) => {
                let len = s.as_byte_str().len();
                if len <= 32 {
                    return self.gcx.types.uint(256);
                }
                return self.gcx.mk_ty_err(
                    self.dcx()
                        .err(format!("string literal too long ({len} > 32)"))
                        .span(lit.span)
                        .emit(),
                );
            }
            LitKind::Address(_) => return self.gcx.types.uint(256),
            _ => {}
        }

        self.gcx.type_of_lit(lit)
    }

    /// Checks whether a resolved identifier should be treated as an external Solidity reference in
    /// Yul. Returns `Ok(true)` when the identifier was accepted and should be typed as a Yul word,
    /// `Ok(false)` when it is not a Yul external reference and normal typing should continue, and
    /// `Err` after emitting a diagnostic for invalid external-reference use.
    pub(super) fn check_yul_external_ident(
        &mut self,
        res: hir::Res,
        ty: Ty<'gcx>,
        span: Span,
    ) -> Result<bool, ErrorGuaranteed> {
        let hir::Res::Item(item) = res else { return Ok(false) };
        let hir::ItemId::Variable(var_id) = item else {
            if let hir::ItemId::Function(id) = item
                && self.gcx.hir.function(id).is_yul
            {
                return Ok(false);
            }
            if self.in_lvalue() {
                return Err(self
                    .dcx()
                    .err("only local variables can be assigned to in inline assembly")
                    .span(span)
                    .emit());
            }
            if let hir::ItemId::Function(_) = item {
                return Err(self
                    .dcx()
                    .err("access to functions is not allowed in inline assembly")
                    .span(span)
                    .emit());
            }
            return Ok(false);
        };
        let var = self.gcx.hir.variable(var_id);

        if var.is_immutable() {
            return Err(self
                .dcx()
                .err("assembly access to immutable variables is not supported")
                .span(span)
                .emit());
        }

        if var.is_constant() && !self.in_lvalue() && !ty.is_value_type() {
            return Err(self
                .dcx()
                .err("only direct number constants are supported in inline assembly")
                .span(span)
                .emit());
        }

        if var.is_state_variable() && !var.is_constant() {
            return Err(self
                .dcx()
                .err("only local variables are supported in inline assembly")
                .span(span)
                .help("use `.slot` and `.offset` to access storage or transient storage variables")
                .emit());
        }

        if ty.data_stored_in(DataLocation::Storage) || ty.data_stored_in(DataLocation::Transient) {
            return Err(self
                .dcx()
                .err("storage reference variables need a suffix in inline assembly")
                .span(span)
                .help("use `.slot` or `.offset`")
                .emit());
        }

        if is_dynamic_calldata_array(ty) {
            return Err(self
                .dcx()
                .err("calldata variables need a suffix in inline assembly")
                .span(span)
                .help("use `.offset` or `.length`")
                .emit());
        }

        if matches!(ty.kind, TyKind::Fn(f) if f.is_external()) {
            return Err(self
                .dcx()
                .err("only types that use one stack slot are supported")
                .span(span)
                .emit());
        }

        Ok(true)
    }

    pub(super) fn check_yul_member(
        &mut self,
        expr: &'gcx hir::Expr<'gcx>,
        member: Ident,
    ) -> Ty<'gcx> {
        enum ErrorKind {
            NonVariable,
            Immutable,
            UnsupportedBase,
            UnsupportedMember(YulMemberSet),
            NonExternalFunction,
            Assignment(YulMemberAssignmentError),
        }

        let mut inner = || -> Result<(), ErrorKind> {
            let (ty, res) = self.check_yul_member_base(expr);
            let Some(res) = res else { return Err(ErrorKind::NonVariable) };
            let hir::Res::Item(hir::ItemId::Variable(var_id)) = res else {
                return Err(ErrorKind::NonVariable);
            };
            let var = self.gcx.hir.variable(var_id);

            if var.is_immutable() {
                return Err(ErrorKind::Immutable);
            }

            let Some(member_set) = yul_member_set(var, ty) else {
                return Err(ErrorKind::UnsupportedBase);
            };

            let Some(yul_member) = member_set.member(member.name) else {
                return Err(ErrorKind::UnsupportedMember(member_set));
            };

            if let YulMemberSet::Function { external: false } = member_set {
                return Err(ErrorKind::NonExternalFunction);
            }

            if self.in_lvalue()
                && !yul_member.assignable
                && let Some(error) = member_set.assignment_error()
            {
                return Err(ErrorKind::Assignment(error));
            }

            Ok(())
        };

        let guar = match inner() {
            Ok(()) => return self.gcx.types.uint(256),
            Err(ErrorKind::NonVariable) => self
                .dcx()
                .err("inline assembly suffixes can only be used with variables")
                .span(member.span)
                .emit(),
            Err(ErrorKind::Immutable) => self
                .dcx()
                .err("assembly access to immutable variables is not supported")
                .span(expr.span)
                .emit(),
            Err(ErrorKind::UnsupportedBase) => self
                .dcx()
                .err(format!("suffix `.{member}` is not supported by this variable or type"))
                .span(member.span)
                .emit(),
            Err(ErrorKind::UnsupportedMember(member_set)) => {
                self.dcx().err(member_set.unsupported_member_message()).span(member.span).emit()
            }
            Err(ErrorKind::NonExternalFunction) => self
                .dcx()
                .err("only external function pointer variables support `.selector` and `.address`")
                .span(member.span)
                .emit(),
            Err(ErrorKind::Assignment(YulMemberAssignmentError::StateVariable)) => self
                .dcx()
                .err("state variables cannot be assigned to in inline assembly")
                .span(expr.span)
                .emit(),
            Err(ErrorKind::Assignment(YulMemberAssignmentError::StorageOffset)) => {
                self.dcx().err("only `.slot` can be assigned to").span(member.span).emit()
            }
        };

        self.gcx.mk_ty_err(guar)
    }

    fn check_yul_member_base(
        &mut self,
        expr: &'gcx hir::Expr<'gcx>,
    ) -> (Ty<'gcx>, Option<hir::Res>) {
        if let hir::ExprKind::Ident(res) = expr.kind {
            let res = self.resolve_overloads(res, expr.span);
            if let Some(reason) = self.res_not_lvalue_reason(res) {
                self.try_set_not_lvalue(reason);
            }
            return (self.type_of_res(res), Some(res));
        }

        (self.check_expr(expr), None)
    }
}

fn is_dynamic_calldata_array(ty: Ty<'_>) -> bool {
    if ty.loc() != Some(DataLocation::Calldata) {
        return false;
    }
    matches!(
        ty.peel_refs().kind,
        TyKind::DynArray(_) | TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
    )
}

#[derive(Clone, Copy)]
struct YulMember {
    name: Symbol,
    assignable: bool,
}

impl YulMember {
    const fn new(name: Symbol, assignable: bool) -> Self {
        Self { name, assignable }
    }
}

#[derive(Clone, Copy)]
enum YulMemberSet {
    Storage { state: bool },
    Calldata,
    Function { external: bool },
}

impl YulMemberSet {
    fn members(self) -> [YulMember; 2] {
        match self {
            Self::Storage { state } => {
                [YulMember::new(sym::slot, !state), YulMember::new(sym::offset, false)]
            }
            Self::Calldata => {
                [YulMember::new(sym::offset, true), YulMember::new(sym::length, true)]
            }
            Self::Function { .. } => {
                [YulMember::new(sym::selector, true), YulMember::new(kw::Address, true)]
            }
        }
    }

    fn member(self, name: Symbol) -> Option<YulMember> {
        self.members().into_iter().find(|member| member.name == name)
    }

    fn unsupported_member_message(self) -> &'static str {
        match self {
            Self::Storage { .. } => "storage variables only support `.slot` and `.offset`",
            Self::Calldata => "calldata variables only support `.offset` and `.length`",
            Self::Function { .. } => {
                "function pointer variables only support `.selector` and `.address`"
            }
        }
    }

    fn assignment_error(self) -> Option<YulMemberAssignmentError> {
        match self {
            Self::Storage { state: true } => Some(YulMemberAssignmentError::StateVariable),
            Self::Storage { state: false } => Some(YulMemberAssignmentError::StorageOffset),
            Self::Calldata | Self::Function { .. } => None,
        }
    }
}

enum YulMemberAssignmentError {
    StateVariable,
    StorageOffset,
}

fn yul_member_set(var: &hir::Variable<'_>, ty: Ty<'_>) -> Option<YulMemberSet> {
    if !var.is_constant()
        && (var.is_state_variable()
            || ty.data_stored_in(DataLocation::Storage)
            || ty.data_stored_in(DataLocation::Transient))
    {
        return Some(YulMemberSet::Storage { state: var.is_state_variable() });
    }

    if is_dynamic_calldata_array(ty) {
        return Some(YulMemberSet::Calldata);
    }

    if let TyKind::Fn(f) = ty.kind {
        return Some(YulMemberSet::Function { external: f.is_external() });
    }

    None
}
