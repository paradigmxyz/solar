//! Source-level place queries for lint and analysis passes.

use super::{ContractId, Expr, ExprId, ExprKind, Hir, Res, VariableId, Visit};
use crate::{
    builtins::Builtin,
    ty::{Gcx, ResolvedMember, TyKind},
};
use solar_ast::{DataLocation, ElementaryType};
use solar_data_structures::smallvec::SmallVec;
use solar_interface::Symbol;
use std::ops::ControlFlow;

/// One scalar component of a source assignment.
#[derive(Clone, Copy, Debug)]
pub struct Assignment<'hir> {
    /// The assigned lvalue component.
    pub target: &'hir Expr<'hir>,
    /// The corresponding rvalue component, when it is syntactically available.
    ///
    /// Calls and other expressions which return a tuple do not expose their individual result
    /// expressions and therefore produce `None` for destructured targets.
    pub value: Option<&'hir Expr<'hir>>,
    /// The target's top-level tuple output index.
    pub output_index: usize,
    /// The number of top-level outputs in the assignment target.
    pub output_count: usize,
}

/// Pairs scalar lvalue and rvalue components of an assignment.
///
/// This recursively handles tuple destructuring and preserves Solidity's simultaneous-assignment
/// shape, so consumers can evaluate all returned [`Assignment::value`] entries before applying
/// writes to their [`Assignment::target`] entries.
pub fn assignment_pairs<'hir>(
    target: &'hir Expr<'hir>,
    value: Option<&'hir Expr<'hir>>,
) -> SmallVec<[Assignment<'hir>; 4]> {
    fn collect<'hir>(
        target: &'hir Expr<'hir>,
        value: Option<&'hir Expr<'hir>>,
        output_index: usize,
        output_count: usize,
        assignments: &mut SmallVec<[Assignment<'hir>; 4]>,
    ) {
        let target = target.peel_parens();
        let value = value.map(Expr::peel_parens);
        if let ExprKind::Tuple(targets) = &target.kind {
            let values = match value.map(|value| &value.kind) {
                Some(ExprKind::Tuple(values)) => Some(*values),
                _ => None,
            };
            for (index, target) in targets.iter().enumerate() {
                let Some(target) = target else { continue };
                let value = values.and_then(|values| values.get(index)).copied().flatten();
                let (output_index, output_count) = if output_count == 1 {
                    (index, targets.len())
                } else {
                    (output_index, output_count)
                };
                collect(target, value, output_index, output_count, assignments);
            }
        } else {
            assignments.push(Assignment { target, value, output_index, output_count });
        }
    }

    let mut assignments = SmallVec::new();
    collect(target, value, 0, 1, &mut assignments);
    assignments
}

/// A source-level variable place such as `account.balances[user]`.
///
/// This intentionally mirrors rustc's `Place` shape: a root variable followed by projections.
/// It describes source identity and does not attempt to compute an EVM storage slot.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Place {
    local: VariableId,
    projection: SmallVec<[ProjectionElem; 4]>,
}

impl Place {
    /// Creates an unprojected place rooted at `local`.
    pub fn from_local(local: VariableId) -> Self {
        Self { local, projection: SmallVec::new() }
    }

    /// Returns the root variable.
    pub fn local(&self) -> VariableId {
        self.local
    }

    /// Returns the projections applied to the root variable.
    pub fn projection(&self) -> &[ProjectionElem] {
        &self.projection
    }

    pub(super) fn with_local(&self, local: VariableId) -> Self {
        Self { local, projection: self.projection.clone() }
    }

    pub(super) fn with_projection_suffix(&self, projection: &[ProjectionElem]) -> Self {
        let mut place = self.clone();
        place.projection.extend_from_slice(projection);
        place
    }

    /// Returns whether the root is a state variable.
    pub fn is_state(&self, hir: &Hir<'_>) -> bool {
        hir.variable(self.local).kind.is_state()
    }

    /// Returns whether this place is backed by storage or transient state.
    ///
    /// Besides state variables, this includes local `storage` and `transient` references whose
    /// root variable aliases contract state.
    pub fn is_state_backed(&self, hir: &Hir<'_>) -> bool {
        let variable = hir.variable(self.local);
        variable.kind.is_state()
            || matches!(
                variable.data_location,
                Some(DataLocation::Storage | DataLocation::Transient)
            )
    }

    /// Returns whether the two source places may overlap.
    ///
    /// Different root variables and different named fields are disjoint. Index expressions are
    /// conservatively treated as possibly equal unless they are the same HIR expression.
    pub fn may_overlap(&self, other: &Self) -> bool {
        if self.local != other.local {
            return false;
        }
        for (lhs, rhs) in self.projection.iter().zip(&other.projection) {
            match (lhs, rhs) {
                (
                    ProjectionElem::Field { variable: Some(lhs), .. },
                    ProjectionElem::Field { variable: Some(rhs), .. },
                ) if lhs != rhs => return false,
                (
                    ProjectionElem::Field { name: lhs, .. },
                    ProjectionElem::Field { name: rhs, .. },
                ) if lhs != rhs => return false,
                (ProjectionElem::Field { .. }, ProjectionElem::Index(_))
                | (ProjectionElem::Field { .. }, ProjectionElem::Slice { .. })
                | (ProjectionElem::Index(_), ProjectionElem::Field { .. })
                | (ProjectionElem::Slice { .. }, ProjectionElem::Field { .. }) => return false,
                _ => {}
            }
        }
        true
    }
}

/// A projection applied to a source-level [`Place`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProjectionElem {
    /// A named member or resolved struct field.
    Field {
        /// The source-level field name.
        name: Symbol,
        /// The field declaration when type checking resolved one.
        variable: Option<VariableId>,
    },
    /// An array or mapping index. `None` represents an omitted index.
    Index(Option<ExprId>),
    /// An array slice.
    Slice {
        /// The lower bound.
        start: Option<ExprId>,
        /// The upper bound.
        end: Option<ExprId>,
    },
}

impl<'hir> Gcx<'hir> {
    /// Peels `payable(...)` and explicit type conversions from an expression.
    pub fn peel_type_conversions(self, mut expr: &'hir Expr<'hir>) -> &'hir Expr<'hir> {
        loop {
            expr = expr.peel_parens();
            match &expr.kind {
                ExprKind::Payable(inner) => expr = inner,
                ExprKind::Call(callee, args, options)
                    if options.is_none()
                        && matches!(
                            self.type_of_expr(callee.id).map(|ty| ty.kind),
                            Some(TyKind::Type(target))
                                if !matches!(target.kind, TyKind::Struct(_))
                        ) =>
                {
                    let mut args = args.exprs();
                    let Some(inner) = args.next() else { return expr };
                    if args.next().is_some() {
                        return expr;
                    }
                    expr = inner;
                }
                _ => return expr,
            }
        }
    }

    /// Returns whether `expr` denotes the currently executing contract's own address.
    ///
    /// Parentheses and injective address/payable/contract conversions are transparent. Lossy
    /// conversions are retained because they can change the address. Yul's `address()` builtin is
    /// the inline-assembly equivalent of `address(this)`.
    pub fn expr_is_self_address(self, expr: &'hir Expr<'hir>) -> bool {
        let expr = self.peel_injective_type_conversions(expr);
        match &expr.kind {
            ExprKind::Ident(resolutions) => {
                resolutions.iter().any(|resolution| resolution.as_builtin() == Some(Builtin::This))
            }
            ExprKind::Call(..) => {
                self.call_info(expr).and_then(|info| info.builtin()) == Some(Builtin::YulAddress)
            }
            _ => false,
        }
    }

    /// Peels only explicit conversions which are injective over the operand's type.
    ///
    /// Unlike [`Gcx::peel_type_conversions`], this is suitable for equality proofs: equality after
    /// an injective conversion implies equality before it. Narrowing integer and fixed-bytes casts
    /// are deliberately retained.
    pub fn peel_injective_type_conversions(self, mut expr: &'hir Expr<'hir>) -> &'hir Expr<'hir> {
        loop {
            expr = expr.peel_parens();
            match &expr.kind {
                ExprKind::Payable(inner) => expr = inner,
                ExprKind::Call(callee, args, options)
                    if options.is_none()
                        && matches!(
                            self.type_of_expr(callee.id).map(|ty| ty.kind),
                            Some(TyKind::Type(target))
                                if !matches!(target.kind, TyKind::Struct(_))
                        ) =>
                {
                    let mut args = args.exprs();
                    let Some(inner) = args.next() else { return expr };
                    if args.next().is_some() {
                        return expr;
                    }
                    let Some(source) = self.type_of_expr(inner.id) else { return expr };
                    let Some(target) = self.type_of_expr(callee.id).and_then(|ty| match ty.kind {
                        TyKind::Type(target) => Some(target),
                        _ => None,
                    }) else {
                        return expr;
                    };
                    if !conversion_is_injective(source, target) {
                        return expr;
                    }
                    expr = inner;
                }
                _ => return expr,
            }
        }
    }

    /// Returns the source-level place denoted by `expr`.
    ///
    /// Parentheses are transparent. Casts, calls, and other computed values are rvalues and return
    /// `None`; use [`Gcx::expr_underlying_place`] for provenance through type conversions.
    pub fn expr_place(self, expr: &'hir Expr<'hir>) -> Option<Place> {
        fn lower<'hir>(gcx: Gcx<'hir>, expr: &'hir Expr<'hir>) -> Option<Place> {
            let expr = expr.peel_parens();
            match &expr.kind {
                ExprKind::Ident(resolutions) => {
                    let mut variables = resolutions.iter().filter_map(Res::as_variable);
                    let variable = variables.next()?;
                    variables.next().is_none().then(|| Place::from_local(variable))
                }
                ExprKind::Member(base, member) => {
                    let mut place = lower(gcx, base)?;
                    let variable = match gcx.resolved_member(expr.id) {
                        Some(ResolvedMember::StructField { struct_id, field_index }) => {
                            gcx.hir.strukt(struct_id).fields.get(field_index).copied()
                        }
                        Some(ResolvedMember::Res(Res::Item(item))) => item.as_variable(),
                        _ => None,
                    };
                    place.projection.push(ProjectionElem::Field { name: member.name, variable });
                    Some(place)
                }
                ExprKind::Index(base, index) => {
                    let mut place = lower(gcx, base)?;
                    place.projection.push(ProjectionElem::Index(index.map(|expr| expr.id)));
                    Some(place)
                }
                ExprKind::Slice(base, start, end) => {
                    let mut place = lower(gcx, base)?;
                    place.projection.push(ProjectionElem::Slice {
                        start: start.map(|expr| expr.id),
                        end: end.map(|expr| expr.id),
                    });
                    Some(place)
                }
                _ => None,
            }
        }

        lower(self, expr)
    }

    /// Returns the source-level place underlying transparent type conversions in `expr`.
    pub fn expr_underlying_place(self, expr: &'hir Expr<'hir>) -> Option<Place> {
        self.expr_place(self.peel_type_conversions(expr))
    }

    /// Returns the variable at the root of a source-level place expression.
    pub fn expr_root_variable(self, expr: &'hir Expr<'hir>) -> Option<VariableId> {
        self.expr_place(expr).map(|place| place.local())
    }

    /// Returns the root variable after peeling transparent type conversions.
    pub fn expr_underlying_variable(self, expr: &'hir Expr<'hir>) -> Option<VariableId> {
        self.expr_underlying_place(expr).map(|place| place.local())
    }

    /// Returns the root variable after peeling only injective explicit conversions.
    pub fn expr_injective_underlying_variable(self, expr: &'hir Expr<'hir>) -> Option<VariableId> {
        self.expr_place(self.peel_injective_type_conversions(expr)).map(|place| place.local())
    }

    /// Returns the statically typed contract behind transparent address and payable conversions.
    pub fn expr_underlying_contract(self, expr: &'hir Expr<'hir>) -> Option<ContractId> {
        let expr = self.peel_type_conversions(expr);
        match self.type_of_expr(expr.id)?.peel_refs().kind {
            TyKind::Contract(contract) => Some(contract),
            _ => None,
        }
    }

    /// Returns whether the root of `expr` is one of `variables`.
    pub fn expr_root_is_any_variable(
        self,
        expr: &'hir Expr<'hir>,
        variables: &[VariableId],
    ) -> bool {
        self.expr_root_variable(expr).is_some_and(|variable| variables.contains(&variable))
    }

    /// Returns whether the root of `expr` is `variable`.
    pub fn expr_root_is_variable(self, expr: &'hir Expr<'hir>, variable: VariableId) -> bool {
        self.expr_root_variable(expr) == Some(variable)
    }

    /// Returns whether `expr` mentions any variable in `variables`.
    pub fn expr_mentions_any_variable(
        self,
        expr: &'hir Expr<'hir>,
        variables: &[VariableId],
    ) -> bool {
        struct Finder<'a, 'hir> {
            hir: &'hir Hir<'hir>,
            variables: &'a [VariableId],
        }

        impl<'hir> Visit<'hir> for Finder<'_, 'hir> {
            type BreakValue = ();

            fn hir(&self) -> &'hir Hir<'hir> {
                self.hir
            }

            fn visit_expr(&mut self, expr: &'hir Expr<'hir>) -> ControlFlow<Self::BreakValue> {
                if let ExprKind::Ident(resolutions) = expr.peel_parens().kind
                    && resolutions.iter().any(|resolution| {
                        resolution
                            .as_variable()
                            .is_some_and(|variable| self.variables.contains(&variable))
                    })
                {
                    ControlFlow::Break(())
                } else {
                    self.walk_expr(expr)
                }
            }
        }

        Finder { hir: &self.hir, variables }.visit_expr(expr).is_break()
    }

    /// Returns whether `expr` mentions `variable`.
    pub fn expr_mentions_variable(self, expr: &'hir Expr<'hir>, variable: VariableId) -> bool {
        self.expr_mentions_any_variable(expr, &[variable])
    }

    /// Returns the source places assigned by an lvalue, including tuple components.
    pub fn assigned_places(self, expr: &'hir Expr<'hir>) -> SmallVec<[Place; 4]> {
        assignment_pairs(expr, None)
            .into_iter()
            .filter_map(|assignment| self.expr_place(assignment.target))
            .collect()
    }

    /// Returns the root variables assigned by an lvalue, including tuple components.
    pub fn assigned_variables(self, expr: &'hir Expr<'hir>) -> SmallVec<[VariableId; 4]> {
        self.assigned_places(expr).into_iter().map(|place| place.local()).collect()
    }

    /// Returns source places assigned by an lvalue which are backed by persistent or transient
    /// EVM storage.
    pub fn assigned_state_backed_places(self, expr: &'hir Expr<'hir>) -> SmallVec<[Place; 4]> {
        self.assigned_places(expr)
            .into_iter()
            .filter(|place| place.is_state_backed(&self.hir))
            .collect()
    }

    /// Returns the state-variable roots assigned by an lvalue.
    pub fn assigned_state_variables(self, expr: &'hir Expr<'hir>) -> SmallVec<[VariableId; 4]> {
        self.assigned_places(expr)
            .into_iter()
            .filter(|place| place.is_state(&self.hir))
            .map(|place| place.local())
            .collect()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScalarRepr {
    Unsigned,
    Signed,
    FixedBytes,
}

fn scalar_repr(ty: crate::ty::Ty<'_>) -> Option<(ScalarRepr, u16)> {
    match ty.peel_refs().kind {
        TyKind::Elementary(ElementaryType::Address(_)) | TyKind::Contract(_) => {
            Some((ScalarRepr::Unsigned, 160))
        }
        TyKind::Elementary(ElementaryType::UInt(size)) => Some((ScalarRepr::Unsigned, size.bits())),
        TyKind::Elementary(ElementaryType::Int(size)) => Some((ScalarRepr::Signed, size.bits())),
        TyKind::Elementary(ElementaryType::FixedBytes(size)) => {
            Some((ScalarRepr::FixedBytes, size.bits()))
        }
        TyKind::Udvt(inner, _) => scalar_repr(inner),
        _ => None,
    }
}

fn conversion_is_injective(source: crate::ty::Ty<'_>, target: crate::ty::Ty<'_>) -> bool {
    if source.peel_refs() == target.peel_refs() {
        return true;
    }
    let Some((source_repr, source_bits)) = scalar_repr(source) else { return false };
    let Some((target_repr, target_bits)) = scalar_repr(target) else { return false };
    source_repr == target_repr && source_bits <= target_bits
        || source_bits == target_bits
            && matches!(
                (source_repr, target_repr),
                (ScalarRepr::Unsigned, ScalarRepr::FixedBytes)
                    | (ScalarRepr::FixedBytes, ScalarRepr::Unsigned)
            )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Compiler;
    use solar_interface::{Session, config::CompileOpts};
    use std::path::PathBuf;

    const SOURCE: &str = r#"
struct Data {
    uint256 value;
    mapping(uint256 => uint256) values;
}

struct Pair {
    address raw;
}

contract C {
    Data data;
    mapping(uint256 => uint256) public publicValues;

    function write(uint256 index, address raw) external {
        data.value = 1;
        data.values[index] = 2;
        publicValues[index] = 3;
        Data storage alias_ = data;
        alias_.value = 4;
        payable(raw).transfer(1);
        address(raw).call("");
        Pair memory pair = Pair(raw);
    }
}
"#;

    struct Expressions<'hir> {
        hir: &'hir Hir<'hir>,
        assignments: Vec<&'hir Expr<'hir>>,
        receivers: Vec<&'hir Expr<'hir>>,
    }

    impl<'hir> Visit<'hir> for Expressions<'hir> {
        type BreakValue = ();

        fn hir(&self) -> &'hir Hir<'hir> {
            self.hir
        }

        fn visit_expr(&mut self, expr: &'hir Expr<'hir>) -> ControlFlow<Self::BreakValue> {
            match &expr.kind {
                ExprKind::Assign(lhs, ..) => self.assignments.push(lhs),
                ExprKind::Call(callee, ..) => {
                    if let ExprKind::Member(receiver, _) = &callee.peel_parens().kind {
                        self.receivers.push(receiver);
                    }
                }
                _ => {}
            }
            self.walk_expr(expr)
        }
    }

    #[test]
    fn resolves_places_projections_conversions_and_mentions() {
        let sess = Session::builder().opts(CompileOpts::default()).with_test_emitter().build();
        let mut compiler = Compiler::new(sess);

        compiler.enter_mut(|c| {
            let mut pcx = c.parse();
            let file =
                c.sess().source_map().new_source_file(PathBuf::from("test.sol"), SOURCE).unwrap();
            pcx.add_file(file);
            pcx.parse();

            assert_eq!(c.lower_asts(), Ok(ControlFlow::Continue(())));
            assert_eq!(c.analysis(), Ok(ControlFlow::Continue(())));
        });

        compiler.enter(|c| {
            let gcx = c.gcx();
            let source = gcx.hir.source_ids().next().unwrap();
            let mut expressions =
                Expressions { hir: &gcx.hir, assignments: Vec::new(), receivers: Vec::new() };
            assert_eq!(expressions.visit_nested_source(source), ControlFlow::Continue(()));

            let [value_expr, indexed_expr, public_expr, aliased_expr] =
                expressions.assignments.as_slice()
            else {
                panic!("expected four assignments")
            };
            let value = gcx.expr_place(value_expr).unwrap();
            let indexed = gcx.expr_place(indexed_expr).unwrap();
            let aliased = gcx.expr_place(aliased_expr).unwrap();
            assert!(value.is_state(&gcx.hir));
            assert_eq!(value.local(), indexed.local());
            assert!(!value.may_overlap(&indexed));
            assert!(!aliased.is_state(&gcx.hir));
            assert!(aliased.is_state_backed(&gcx.hir));
            assert_eq!(gcx.assigned_state_variables(value_expr).as_slice(), [value.local()]);
            assert_eq!(gcx.assigned_state_variables(public_expr).len(), 1);
            assert_eq!(gcx.assigned_state_backed_places(aliased_expr).as_slice(), [aliased]);
            assert!(matches!(
                value.projection(),
                [ProjectionElem::Field { variable: Some(_), .. }]
            ));
            assert!(matches!(
                indexed.projection(),
                [ProjectionElem::Field { variable: Some(_), .. }, ProjectionElem::Index(Some(_))]
            ));

            let [payable, address] = expressions.receivers.as_slice() else {
                panic!("expected two call receivers")
            };
            assert!(gcx.expr_place(payable).is_none());
            assert!(gcx.expr_place(address).is_none());
            let raw = gcx.expr_underlying_variable(payable).unwrap();
            assert_eq!(gcx.expr_underlying_variable(address), Some(raw));

            let function = gcx
                .hir
                .function_ids()
                .find(|&id| gcx.item_canonical_name(id).to_string() == "C.write")
                .map(|id| gcx.hir.function(id))
                .unwrap();
            let [index, raw_parameter] = function.parameters else { panic!("expected parameters") };
            assert_eq!(raw, *raw_parameter);
            assert!(gcx.expr_mentions_variable(indexed_expr, *index));
            assert!(!gcx.expr_mentions_variable(indexed_expr, raw));

            let pair = gcx
                .hir
                .variables()
                .find(|variable| variable.name.is_some_and(|name| name.name.as_str() == "pair"))
                .and_then(|variable| variable.initializer)
                .unwrap();
            assert!(gcx.expr_place(pair).is_none());
        });
    }

    #[test]
    fn index_may_overlap_slice() {
        let mut indexed = Place::from_local(VariableId::new(0));
        indexed.projection.push(ProjectionElem::Index(Some(ExprId::new(0))));
        let mut sliced = Place::from_local(VariableId::new(0));
        sliced
            .projection
            .push(ProjectionElem::Slice { start: Some(ExprId::new(1)), end: Some(ExprId::new(2)) });

        assert!(indexed.may_overlap(&sliced));
        assert!(sliced.may_overlap(&indexed));
    }
}
