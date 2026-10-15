use super::TypeChecker;
use crate::{
    hir,
    ty::{Ty, TyKind},
    typeck::constant_cycles::MAX_DEPTH,
};
use solar_ast::{DataLocation, ElementaryType, LitKind, Span};
use solar_data_structures::map::FxHashSet;
use solar_interface::{
    Ident, Symbol,
    diagnostics::{DiagId, DiagMsg, ErrorGuaranteed},
    error_code, kw, sym,
};

impl<'gcx> TypeChecker<'gcx> {
    pub(super) fn check_yul_lit(&self, lit: &'gcx hir::Lit<'gcx>) -> Ty<'gcx> {
        match &lit.kind {
            LitKind::Str(_, s, _) => {
                let len = s.as_byte_str().len();
                if len <= 32 {
                    return self.gcx.types.uint(256);
                }
                return self.gcx.mk_ty_err(
                    self.dcx().emit_err(lit.span, format!("string literal too long ({len} > 32)")),
                );
            }
            LitKind::Address(_) | LitKind::Bool(_) => return self.gcx.types.uint(256),
            LitKind::BigNumber(_) => {
                let msg = "number literal too large (> 256 bits)";
                let err = self.dcx().err(msg).code(error_code!(6708)).span(lit.span);
                return self.gcx.mk_ty_err(err.emit());
            }
            _ => {}
        }

        self.gcx.type_of_lit(lit)
    }

    /// Checks whether a resolved identifier should be treated as an external Solidity reference in
    /// Yul. Returns `Ok(true)` when the identifier was accepted and should be typed as a Yul word,
    /// `Ok(false)` when it is not a Yul external reference and normal typing should continue, and
    /// `Err` when the type already holds an error or after emitting a diagnostic for invalid
    /// external-reference use.
    pub(super) fn check_yul_external_ident(
        &mut self,
        res: hir::Res,
        ty: Ty<'gcx>,
        span: Span,
    ) -> Result<bool, ErrorGuaranteed> {
        ty.error_reported()?;
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
                    .code(error_code!(1990))
                    .span(span)
                    .emit());
            }
            if let hir::ItemId::Function(_) = item {
                return Err(self
                    .dcx()
                    .err("access to functions is not allowed in inline assembly")
                    .code(error_code!(2025))
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
                .code(error_code!(3773))
                .span(span)
                .emit());
        }

        if var.is_constant() && self.is_constant_recursive(var_id) {
            return Err(self
                .dcx()
                .err("constant variable is circular")
                .code(error_code!(3558))
                .span(span)
                .emit());
        }

        if var.is_constant() && !self.in_lvalue() && !ty.is_value_type() {
            return Err(self
                .dcx()
                .err("only direct number constants are supported in inline assembly")
                .code(error_code!(7615))
                .span(span)
                .emit());
        }

        if var.is_state_variable() && !var.is_constant() {
            return Err(self
                .dcx()
                .err("only local variables are supported in inline assembly")
                .code(error_code!(1408))
                .span(span)
                .help("use `.slot` and `.offset` to access storage or transient storage variables")
                .emit());
        }

        if ty.data_stored_in(DataLocation::Storage) || ty.data_stored_in(DataLocation::Transient) {
            return Err(self
                .dcx()
                .err("storage reference variables need a suffix in inline assembly")
                .code(error_code!(9068))
                .span(span)
                .help("use `.slot` or `.offset`")
                .emit());
        }

        if is_dynamic_calldata_array(ty) {
            return Err(self
                .dcx()
                .err("calldata variables need a suffix in inline assembly")
                .code(error_code!(1397))
                .span(span)
                .help("use `.offset` or `.length`")
                .emit());
        }

        if matches!(ty.kind, TyKind::Fn(f) if f.is_external()) {
            return Err(self
                .dcx()
                .err("only types that use one stack slot are supported")
                .code(error_code!(9857))
                .span(span)
                .emit());
        }

        Ok(true)
    }

    /// Returns whether following the value of the constant `id` through the constants it names
    /// reaches a cycle.
    ///
    /// Like solc's `isConstantVariableRecursive`, this only follows values that are an identifier
    /// or a member access. Other sources may not be type checked yet, so this uses name
    /// resolution alone.
    ///
    /// This gives up after [`MAX_DEPTH`] constants, since the constant cycle check after type
    /// checking reports longer chains.
    fn is_constant_recursive(&mut self, id: hir::VariableId) -> bool {
        if let Some(&recursive) = self.circular_constants.get(&id) {
            return recursive;
        }
        let recursive = self.follow_constant_values(id);
        self.circular_constants.insert(id, recursive);
        recursive
    }

    fn follow_constant_values(&self, mut id: hir::VariableId) -> bool {
        let mut seen = FxHashSet::default();
        for _ in 0..MAX_DEPTH {
            if !seen.insert(id) {
                return true;
            }
            let Some(next) = self
                .gcx
                .hir
                .variable(id)
                .initializer
                .and_then(|init| self.gcx.resolve_path_expr(init)?.as_variable())
                .filter(|&next| self.gcx.hir.variable(next).is_constant())
            else {
                return false;
            };
            id = next;
        }
        false
    }

    pub(super) fn check_yul_member(
        &mut self,
        expr: &'gcx hir::Expr<'gcx>,
        member: Ident,
    ) -> Ty<'gcx> {
        enum ErrorKind {
            NonVariable,
            Immutable,
            Circular,
            UnsupportedBase,
            UnsupportedMember(YulMemberSet),
            NonExternalFunction,
            Assignment(YulMemberAssignmentError),
        }

        let (ty, res) = self.check_yul_member_base(expr);
        if let Err(guar) = ty.error_reported() {
            return self.gcx.mk_ty_err(guar);
        }
        let circular = if let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res
            && self.gcx.hir.variable(var_id).is_constant()
        {
            self.is_constant_recursive(var_id)
        } else {
            false
        };

        let inner = || -> Result<(), ErrorKind> {
            let Some(res) = res else { return Err(ErrorKind::NonVariable) };
            let hir::Res::Item(hir::ItemId::Variable(var_id)) = res else {
                return Err(ErrorKind::NonVariable);
            };
            let var = self.gcx.hir.variable(var_id);

            if var.is_immutable() {
                return Err(ErrorKind::Immutable);
            }

            if circular {
                return Err(ErrorKind::Circular);
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

        let (span, code, msg): (Span, DiagId, DiagMsg) = match inner() {
            Ok(()) => return self.gcx.types.uint(256),
            Err(ErrorKind::NonVariable) => (
                member.span,
                error_code!(7944),
                "inline assembly suffixes can only be used with variables".into(),
            ),
            Err(ErrorKind::Immutable) => (
                expr.span,
                error_code!(3773),
                "assembly access to immutable variables is not supported".into(),
            ),
            Err(ErrorKind::Circular) => {
                (expr.span, error_code!(3558), "constant variable is circular".into())
            }
            Err(ErrorKind::UnsupportedBase) => (
                member.span,
                error_code!(3622),
                format!("suffix `.{member}` is not supported by this variable or type").into(),
            ),
            Err(ErrorKind::UnsupportedMember(member_set)) => {
                let (code, msg) = member_set.unsupported_member_error();
                (member.span, code, msg.into())
            }
            Err(ErrorKind::NonExternalFunction) => (
                member.span,
                error_code!(8533),
                "only external function pointer variables support `.selector` and `.address`"
                    .into(),
            ),
            Err(ErrorKind::Assignment(YulMemberAssignmentError::StateVariable)) => (
                expr.span,
                error_code!(4713),
                "state variables cannot be assigned to in inline assembly".into(),
            ),
            Err(ErrorKind::Assignment(YulMemberAssignmentError::StorageOffset)) => {
                (member.span, error_code!(9739), "only `.slot` can be assigned to".into())
            }
        };

        self.gcx.mk_ty_err(self.dcx().err(msg).code(code).span(span).emit())
    }

    fn check_yul_member_base(
        &mut self,
        expr: &'gcx hir::Expr<'gcx>,
    ) -> (Ty<'gcx>, Option<hir::Res>) {
        if let hir::ExprKind::Ident(resolutions) = expr.kind {
            let res = self.resolve_value(expr, resolutions);
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

    fn unsupported_member_error(self) -> (DiagId, &'static str) {
        match self {
            Self::Storage { .. } => {
                (error_code!(4656), "storage variables only support `.slot` and `.offset`")
            }
            Self::Calldata => {
                (error_code!(1536), "calldata variables only support `.offset` and `.length`")
            }
            Self::Function { .. } => (
                error_code!(9272),
                "function pointer variables only support `.selector` and `.address`",
            ),
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
