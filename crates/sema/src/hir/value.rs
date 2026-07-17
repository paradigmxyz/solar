//! Path-sensitive source-value identities for HIR analyses.

use super::{
    CallResults, DataLocation, EffectiveBodyCx, Expr, ExprId, ExprKind, JoinSemiLattice, Modifier,
    Place, ProjectionElem, Stmt, StmtKind, TryCatchClause, UnOpKind, VariableId, assignment_pairs,
};
use crate::ty::Gcx;
use solar_data_structures::{
    map::{FxHashMap, FxHashSet},
    smallvec::SmallVec,
};

type PlaceBinding<'hir, P> = (Place, Option<&'hir Expr<'hir>>, ValueSet<P>);
type ParameterBinding<'hir, P> = (VariableId, Option<(&'hir Expr<'hir>, ValueSet<P>)>);
type VariableBinding<'hir, P> = (VariableId, Option<&'hir Expr<'hir>>, ValueSet<P>);

/// The source identity of a runtime value.
///
/// Initial function inputs and state reads are identified by their declaration. Values produced
/// by expressions are identified by their HIR expression, while opaque resets use their variable
/// declaration without claiming dynamic identity. These finite sets let domains built from the
/// identities converge under [`JoinSemiLattice::join`], including in loops.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ValueOrigin {
    /// The value a variable has before any tracked assignment.
    Initial(VariableId),
    /// A source place in one aggregate generation, with a stable identity and movable anchor.
    ///
    /// The identity remains stable when an alias survives a source-root rebind, while the anchor
    /// moves to the surviving variable. Keeping both the place and owning aggregate generation
    /// distinct prevents facts from reaching a sibling field or replacement object.
    Place {
        /// Stable source identity of the projected value.
        identity: Place,
        /// Current source move path which stores the value.
        anchor: Place,
        /// Identity of the aggregate instance which owns the projection.
        generation: ValueGeneration,
    },
    /// A value produced by an expression.
    Expr(ExprId),
    /// A fresh reference-valued output of one internal call site.
    CallResult(ExprId, usize),
    /// A finite identity whose dynamic instances must not be treated as definite aliases.
    ///
    /// This is useful for source-level resets which have no expression ID, such as an
    /// uninitialized local declaration executed on each loop iteration.
    Opaque(VariableId),
}

/// Finite identity of the aggregate instance which owns a [`ValueOrigin::Place`].
///
/// Source places can be rebound, so a path such as `value.field` is not a stable value identity by
/// itself. Generations distinguish the initial object from objects produced by later expression or
/// call sites while remaining finite enough for loop and recursion fixpoints.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ValueGeneration {
    /// The aggregate initially held by a variable.
    Initial(VariableId),
    /// An aggregate produced by an expression site.
    Expr(ExprId),
    /// A reference-valued result of an internal call site.
    CallResult(ExprId, usize),
    /// An opaque reset which cannot be dynamically correlated.
    Opaque(VariableId),
}

impl ValueGeneration {
    fn from_origin(origin: &ValueOrigin) -> Self {
        match origin {
            ValueOrigin::Initial(variable) => Self::Initial(*variable),
            ValueOrigin::Place { generation, .. } => generation.clone(),
            ValueOrigin::Expr(expr) => Self::Expr(*expr),
            ValueOrigin::CallResult(expr, index) => Self::CallResult(*expr, *index),
            ValueOrigin::Opaque(variable) => Self::Opaque(*variable),
        }
    }

    fn expr(&self) -> Option<ExprId> {
        match self {
            Self::Expr(expr) | Self::CallResult(expr, _) => Some(*expr),
            Self::Initial(_) | Self::Opaque(_) => None,
        }
    }
}

impl ValueOrigin {
    /// Creates a projected origin whose identity and physical anchor are both `place`.
    pub fn place(place: Place, generation: ValueGeneration) -> Self {
        Self::Place { identity: place.clone(), anchor: place, generation }
    }

    /// Returns the expression site which produced this value, if any.
    pub fn expr(&self) -> Option<ExprId> {
        match self {
            Self::Place { generation, .. } => generation.expr(),
            Self::Expr(expr) | Self::CallResult(expr, _) => Some(*expr),
            Self::Initial(_) | Self::Opaque(_) => None,
        }
    }

    /// Returns whether two origins have the same finite source identity.
    ///
    /// Detached aliases may have different physical anchors while retaining the same identity.
    /// Expression and call-result identities can represent different dynamic values when their
    /// site repeats, so callers must also check [`EvaluatedSites::origin_is_correlatable`] before
    /// using this relation as a definite runtime alias.
    pub fn same_source(&self, other: &Self) -> bool {
        let place_identity = |origin: &Self| match origin {
            Self::Initial(variable) => {
                Some((Place::from_local(*variable), ValueGeneration::Initial(*variable)))
            }
            Self::Opaque(variable) => {
                Some((Place::from_local(*variable), ValueGeneration::Opaque(*variable)))
            }
            Self::Place { identity, generation, .. } => {
                Some((identity.clone(), generation.clone()))
            }
            Self::Expr(_) | Self::CallResult(..) => None,
        };
        match (place_identity(self), place_identity(other)) {
            (Some(lhs), Some(rhs)) => lhs == rhs,
            (None, None) => self == other,
            _ => false,
        }
    }
}

/// Tracks whether an expression site may execute more than once on one runtime path.
///
/// Joining two alternatives which each evaluate a site once does not make it repeated. A site is
/// repeated only when transfer reaches it again in a state where it was already seen, as happens
/// on loop backedges and recursive call summaries.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EvaluatedSites {
    seen: FxHashSet<ExprId>,
    repeated: FxHashSet<ExprId>,
}

impl EvaluatedSites {
    /// Records one evaluation and returns whether this made the site potentially repeated.
    pub fn record(&mut self, expr: ExprId) -> bool {
        if self.seen.insert(expr) { false } else { self.repeated.insert(expr) }
    }

    /// Returns whether one expression site is safe to correlate as a unique dynamic value.
    pub fn is_correlatable(&self, expr: ExprId) -> bool {
        !self.repeated.contains(&expr)
    }

    /// Returns whether a value origin is safe to correlate as a unique dynamic value.
    pub fn origin_is_correlatable(&self, origin: &ValueOrigin) -> bool {
        match origin {
            ValueOrigin::Initial(_) => true,
            ValueOrigin::Place { generation, .. } => match generation {
                ValueGeneration::Initial(_) => true,
                ValueGeneration::Expr(expr) | ValueGeneration::CallResult(expr, _) => {
                    self.is_correlatable(*expr)
                }
                ValueGeneration::Opaque(_) => false,
            },
            ValueOrigin::Expr(expr) | ValueOrigin::CallResult(expr, _) => {
                self.is_correlatable(*expr)
            }
            ValueOrigin::Opaque(_) => false,
        }
    }

    /// Returns whether two origins definitely name the same unique dynamic source value.
    pub fn origins_are_correlatable(&self, lhs: &ValueOrigin, rhs: &ValueOrigin) -> bool {
        lhs.same_source(rhs) && self.origin_is_correlatable(lhs) && self.origin_is_correlatable(rhs)
    }
}

impl JoinSemiLattice for EvaluatedSites {
    fn join(&mut self, other: &Self) -> bool {
        let old_seen = self.seen.len();
        let old_repeated = self.repeated.len();
        self.seen.extend(other.seen.iter().copied());
        self.repeated.extend(other.repeated.iter().copied());
        self.seen.len() != old_seen || self.repeated.len() != old_repeated
    }
}

/// One possible value and a path-sensitive property attached to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AbstractValue<P> {
    origin: ValueOrigin,
    property: P,
}

impl<P> AbstractValue<P> {
    /// Creates a possible value.
    pub fn new(origin: ValueOrigin, property: P) -> Self {
        Self { origin, property }
    }

    /// Returns the value's source identity.
    pub fn origin(&self) -> ValueOrigin {
        self.origin.clone()
    }

    /// Returns the path-sensitive property attached to the value.
    pub fn property(&self) -> &P {
        &self.property
    }

    /// Returns a mutable reference to the value's property.
    pub fn property_mut(&mut self) -> &mut P {
        &mut self.property
    }
}

/// The finite alternatives which may reach a program point as one source value.
///
/// Properties remain attached to individual alternatives. Joining `safe(then_value)` with
/// `safe(else_value)` therefore preserves that every reaching value is safe, while joining safe
/// and unsafe versions of the same origin retains both possibilities.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueSet<P> {
    values: SmallVec<[AbstractValue<P>; 2]>,
}

impl<P> Default for ValueSet<P> {
    fn default() -> Self {
        Self { values: SmallVec::new() }
    }
}

impl<P> ValueSet<P> {
    /// Creates a set containing one possible value.
    pub fn singleton(origin: ValueOrigin, property: P) -> Self {
        let mut values = SmallVec::new();
        values.push(AbstractValue::new(origin, property));
        Self { values }
    }

    /// Returns the possible values.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &AbstractValue<P>> {
        self.values.iter()
    }

    /// Returns mutable references to the possible values.
    pub fn iter_mut(&mut self) -> impl ExactSizeIterator<Item = &mut AbstractValue<P>> {
        self.values.iter_mut()
    }

    /// Returns whether there are no possible values.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    fn all(&self, predicate: impl FnMut(&AbstractValue<P>) -> bool) -> bool {
        self.values.iter().all(predicate)
    }

    /// Returns whether the set is non-empty and every possible value satisfies `predicate`.
    ///
    /// An accidentally empty abstract value cannot prove a property vacuously.
    pub fn is_proven(&self, predicate: impl FnMut(&AbstractValue<P>) -> bool) -> bool {
        !self.is_empty() && self.all(predicate)
    }

    /// Returns whether any possible value satisfies `predicate`.
    pub fn any(&self, predicate: impl FnMut(&AbstractValue<P>) -> bool) -> bool {
        self.values.iter().any(predicate)
    }

    /// Returns the common origin when every alternative denotes the same source value.
    pub fn common_origin(&self) -> Option<ValueOrigin> {
        let origin = &self.values.first()?.origin;
        self.values.iter().all(|value| value.origin == *origin).then(|| origin.clone())
    }

    /// Returns one representative origin when every alternative has the same stable source.
    ///
    /// The alternatives may have different physical anchors after a source-root rebind. Callers
    /// using the result as a definite runtime identity must also establish dynamic correlatability
    /// with [`EvaluatedSites::origin_is_correlatable`].
    pub fn common_source(&self) -> Option<ValueOrigin> {
        let origin = &self.values.first()?.origin;
        self.values.iter().all(|value| value.origin.same_source(origin)).then(|| origin.clone())
    }
}

impl<P: Clone + Eq> ValueSet<P> {
    /// Applies `update` to alternatives whose origins satisfy `predicate`.
    pub fn update_matching(
        &mut self,
        mut predicate: impl FnMut(ValueOrigin) -> bool,
        mut update: impl FnMut(&mut P),
    ) {
        for value in &mut self.values {
            if predicate(value.origin.clone()) {
                update(&mut value.property);
            }
        }
        self.deduplicate();
    }

    fn deduplicate(&mut self) {
        let mut index = 0;
        while index < self.values.len() {
            if self.values[..index].contains(&self.values[index]) {
                self.values.remove(index);
            } else {
                index += 1;
            }
        }
    }

    fn with_origin(&self, origin: ValueOrigin) -> Self {
        let mut values = Self::default();
        for value in &self.values {
            let value = AbstractValue::new(origin.clone(), value.property.clone());
            if !values.values.contains(&value) {
                values.values.push(value);
            }
        }
        values
    }

    fn map_origins(&self, mut map: impl FnMut(ValueOrigin) -> ValueOrigin) -> Self {
        let mut values = Self::default();
        for value in &self.values {
            let value = AbstractValue::new(map(value.origin.clone()), value.property.clone());
            if !values.values.contains(&value) {
                values.values.push(value);
            }
        }
        values
    }
}

impl<P: Clone + Eq> JoinSemiLattice for ValueSet<P> {
    fn join(&mut self, other: &Self) -> bool {
        let old_len = self.values.len();
        for value in &other.values {
            if !self.values.contains(value) {
                self.values.push(value.clone());
            }
        }
        self.values.len() != old_len
    }
}

/// Current abstract values of source variables.
///
/// This is a reusable value-numbering domain for lint dataflow. Assignments copy a [`ValueSet`],
/// mutations replace it with a new expression origin, and joins union the alternatives from each
/// predecessor. The property type lets a consumer attach a small taint, validity, or provenance
/// fact without inventing fresh value IDs at every join.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueState<P> {
    values: FxHashMap<VariableId, ValueSet<P>>,
    initial_properties: FxHashMap<VariableId, P>,
    initial_property: P,
}

impl<P: Clone + Eq> ValueState<P> {
    /// Creates an empty state whose untouched variables have `initial_property`.
    pub fn new(initial_property: P) -> Self {
        Self {
            values: FxHashMap::default(),
            initial_properties: FxHashMap::default(),
            initial_property,
        }
    }

    /// Returns the abstract value currently held by `variable`.
    pub fn variable(&self, variable: VariableId) -> ValueSet<P> {
        self.values.get(&variable).cloned().unwrap_or_else(|| {
            let property = self.initial_property(variable);
            ValueSet::singleton(ValueOrigin::Initial(variable), property)
        })
    }

    fn initial_property(&self, variable: VariableId) -> P {
        self.initial_properties
            .get(&variable)
            .cloned()
            .unwrap_or_else(|| self.initial_property.clone())
    }

    /// Materializes an initial variable value with a lint-derived property.
    ///
    /// Seed entry states before analysis begins so every control-flow predecessor uses the same
    /// property for an otherwise untouched variable.
    pub fn seed(&mut self, variable: VariableId, property: P) {
        self.initial_properties.insert(variable, property);
    }

    /// Forgets path-dependent values while retaining seeded entry properties.
    pub fn forget(&mut self) {
        self.values.clear();
    }

    /// Returns a variable's current value, or an expression-origin value for a computed rvalue.
    pub fn expr(&self, expr: &Expr<'_>, property: P) -> ValueSet<P> {
        match expr.as_variable() {
            Some(variable) => self.variable(variable),
            None => ValueSet::singleton(ValueOrigin::Expr(expr.id), property),
        }
    }

    /// Assigns `values` to `variable`.
    pub fn set(&mut self, variable: VariableId, values: ValueSet<P>) {
        self.values.insert(variable, values);
    }

    /// Replaces `variable` with a value produced by `expr`.
    pub fn invalidate(&mut self, variable: VariableId, expr: ExprId, property: P) {
        self.set(variable, ValueSet::singleton(ValueOrigin::Expr(expr), property));
    }

    /// Resets `variable` to an opaque value which is not eligible for definite-alias propagation.
    pub fn reset(&mut self, variable: VariableId, property: P) {
        self.set(variable, ValueSet::singleton(ValueOrigin::Opaque(variable), property));
    }

    /// Updates variables which are definitely aliases of `origins`.
    ///
    /// Joined sets are not relational: two variables containing `{a, b}` may hold opposite values
    /// on every predecessor. Expression origins are also HIR sites rather than dynamic instances,
    /// so they are not stable identities across loop iterations. Consequently this only propagates
    /// through a single initial-variable or projected-place origin on both sides. Callers using a
    /// projected origin must separately ensure its aggregate generation is dynamically
    /// correlatable, for example with [`EvaluatedSites::origin_is_correlatable`].
    pub fn update_definite_aliases(
        &mut self,
        origins: &ValueSet<P>,
        mut update: impl FnMut(&mut P),
    ) {
        let Some(origin) = origins.common_source() else { return };
        if let ValueOrigin::Initial(variable) = &origin {
            let initial_property = self.initial_property(*variable);
            self.values
                .entry(*variable)
                .or_insert_with(|| ValueSet::singleton(origin.clone(), initial_property));
        } else if !matches!(origin, ValueOrigin::Place { .. }) {
            return;
        }
        for values in self.values.values_mut() {
            if values.common_source().is_some_and(|candidate| candidate.same_source(&origin)) {
                values.update_matching(|candidate| candidate.same_source(&origin), &mut update);
            }
        }
    }

    /// Returns the variables whose values have been materialized by the analysis.
    pub fn variables(&self) -> impl Iterator<Item = VariableId> + '_ {
        self.values.keys().copied()
    }

    fn restore(&mut self, variable: VariableId, caller: &Self) {
        if let Some(values) = caller.values.get(&variable) {
            self.values.insert(variable, values.clone());
        } else {
            self.values.remove(&variable);
        }
    }
}

impl<P: Clone + Eq> JoinSemiLattice for ValueState<P> {
    fn join(&mut self, other: &Self) -> bool {
        debug_assert!(self.initial_property == other.initial_property);
        debug_assert!(self.initial_properties == other.initial_properties);
        let mut changed = false;
        let variables: SmallVec<[VariableId; 8]> =
            self.values.keys().chain(other.values.keys()).copied().collect();
        for variable in variables {
            let mut values = self.variable(variable);
            changed |= values.join(&other.variable(variable));
            if self.values.get(&variable) != Some(&values) {
                self.values.insert(variable, values);
                changed = true;
            }
        }
        changed
    }
}

/// Reusable variable and call-result value flow for effective-body analyses.
///
/// This is the source-HIR analogue of rustc's move/value propagation state. It implements the
/// mechanical parts shared by lints: simultaneous assignment, tuple declarations, parameter
/// binding, indexed call returns, and caller-activation restoration. Consumers choose the
/// abstract property attached to computed values and may layer lint-specific facts beside it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueFlowState<P> {
    values: ValueState<P>,
    places: FxHashMap<Place, ValueSet<P>>,
    call_results: CallResults<ValueSet<P>>,
    evaluated: EvaluatedSites,
}

impl<P: Clone + Eq> ValueFlowState<P> {
    /// Creates an empty flow state for the given default property.
    pub fn new(initial_property: P) -> Self {
        Self {
            values: ValueState::new(initial_property),
            places: FxHashMap::default(),
            call_results: CallResults::default(),
            evaluated: EvaluatedSites::default(),
        }
    }

    /// Returns the underlying variable-value state.
    pub fn values(&self) -> &ValueState<P> {
        &self.values
    }

    /// Forgets variable and call-result facts while preserving expression-evaluation history.
    ///
    /// Use this for opaque effects such as inline assembly or an unresolved internal call. Keeping
    /// evaluation history prevents a later occurrence of the same source expression from being
    /// mistaken for a uniquely evaluated dynamic value.
    pub fn forget_values(&mut self) {
        self.values.forget();
        self.places.clear();
        self.call_results.clear_all();
    }

    /// Returns expression evaluation multiplicity tracked by this flow state.
    pub fn evaluated_sites(&self) -> &EvaluatedSites {
        &self.evaluated
    }

    /// Records that `expr` was evaluated on this path.
    pub fn record_evaluation(&mut self, expr: ExprId) -> bool {
        self.evaluated.record(expr)
    }

    /// Returns the current value of `variable`.
    pub fn variable(&self, variable: VariableId) -> ValueSet<P> {
        self.value_for_place(&Place::from_local(variable))
    }

    /// Seeds an entry property for an otherwise untouched variable.
    pub fn seed(&mut self, variable: VariableId, property: P) {
        self.values.seed(variable, property);
    }

    /// Invalidates a whole variable and every tracked field projection rooted in it.
    pub fn invalidate_variable(&mut self, variable: VariableId, site: ExprId, property: P) {
        self.places.retain(|place, _| place.local() != variable);
        self.values.invalidate(variable, site, property);
    }

    /// Refines the value of an exact whole-variable or named-field place.
    ///
    /// Whole-variable refinements also update materialized definite aliases. Named-field
    /// refinements use distinct place origins, while indexed and sliced places are rejected
    /// because their runtime identity is not stable.
    pub fn refine_place<'hir>(
        &mut self,
        gcx: Gcx<'hir>,
        expr: &'hir Expr<'hir>,
        update: impl FnMut(&mut P),
    ) -> Option<ValueSet<P>> {
        let place = gcx.expr_place(expr)?;
        if !place.projection().is_empty() && place.is_state_backed(&gcx.hir) {
            return None;
        }
        if place.projection().is_empty() {
            let origins = self.variable(place.local());
            self.refine_variable_aliases(place.local(), &origins, update);
            return Some(origins);
        }
        if !precise_projected_place(&place) {
            return None;
        }
        let places = self.canonical_places(&place);
        let [place] = places.as_slice() else { return None };
        let origins = self.value_for_place(place);
        self.refine_projected_aliases(place, &origins, update);
        Some(origins)
    }

    /// Refines the value denoted by an arbitrary expression and returns its prior alternatives.
    ///
    /// Places use exact move-path refinement. Computed values update materialized aliases only
    /// when their origin is unique on the current runtime path. A scalar call condition is also
    /// materialized as a call result, covering `if (recipient.send(...))`.
    pub fn refine_expr<'hir>(
        &mut self,
        gcx: Gcx<'hir>,
        expr: &'hir Expr<'hir>,
        computed_property: P,
        mut update: impl FnMut(&mut P),
    ) -> ValueSet<P> {
        if gcx.expr_place(expr).is_some()
            && let Some(origins) = self.refine_place(gcx, expr, &mut update)
        {
            return origins;
        }

        let origins = self.expr(gcx, expr, computed_property);
        let mut refined = origins.clone();
        refined.update_matching(|_| true, &mut update);
        let is_call = matches!(expr.peel_parens().kind, ExprKind::Call(..));
        let Some(origin) = origins.common_source() else {
            if is_call {
                self.call_results.set_outputs(expr.id, [refined]);
            }
            return origins;
        };
        if !self.evaluated.origin_is_correlatable(&origin) {
            if is_call {
                self.call_results.set_outputs(expr.id, [refined]);
            }
            return origins;
        }
        for values in self.values.values.values_mut() {
            values.update_matching(
                |candidate| self.evaluated.origins_are_correlatable(&candidate, &origin),
                &mut update,
            );
        }
        for values in self.places.values_mut() {
            values.update_matching(
                |candidate| self.evaluated.origins_are_correlatable(&candidate, &origin),
                &mut update,
            );
        }
        for values in self.call_results.values_mut() {
            values.update_matching(
                |candidate| self.evaluated.origins_are_correlatable(&candidate, &origin),
                &mut update,
            );
        }
        if is_call {
            self.call_results.set_outputs(expr.id, [refined]);
        }
        origins
    }

    fn refine_variable_aliases(
        &mut self,
        variable: VariableId,
        origins: &ValueSet<P>,
        mut update: impl FnMut(&mut P),
    ) {
        let Some(origin) = origins.common_source() else {
            let mut refined = self.variable(variable);
            refined.update_matching(|_| true, update);
            self.values.set(variable, refined);
            return;
        };
        if !matches!(origin, ValueOrigin::Initial(_) | ValueOrigin::Place { .. })
            || !self.evaluated.origin_is_correlatable(&origin)
        {
            let mut refined = self.variable(variable);
            refined.update_matching(|_| true, update);
            self.values.set(variable, refined);
            return;
        }
        self.values.update_definite_aliases(origins, &mut update);
        let source = match origins.common_origin() {
            Some(ValueOrigin::Place { anchor, .. }) => Some(anchor),
            _ => None,
        };
        let mut source_present = false;
        for (place, values) in &mut self.places {
            if values.common_source().is_some_and(|candidate| candidate.same_source(&origin)) {
                values.update_matching(|candidate| candidate.same_source(&origin), &mut update);
                source_present |= source.as_ref() == Some(place);
            }
        }
        if !source_present && let Some(ValueOrigin::Place { anchor, .. }) = origins.common_origin()
        {
            let mut values = origins.clone();
            values.update_matching(|candidate| candidate.same_source(&origin), update);
            self.set_place(anchor, values);
        }
    }

    fn refine_projected_aliases(
        &mut self,
        place: &Place,
        origins: &ValueSet<P>,
        mut update: impl FnMut(&mut P),
    ) {
        let Some(origin) = origins.common_source() else {
            let mut refined = origins.clone();
            refined.update_matching(|_| true, update);
            self.set_place(place.clone(), refined);
            return;
        };
        if !matches!(origin, ValueOrigin::Initial(_) | ValueOrigin::Place { .. })
            || !self.evaluated.origin_is_correlatable(&origin)
        {
            let mut refined = origins.clone();
            refined.update_matching(|_| true, update);
            self.set_place(place.clone(), refined);
            return;
        }
        self.values.update_definite_aliases(origins, &mut update);
        let mut selected_present = false;
        for (candidate, values) in &mut self.places {
            if values.common_source().is_some_and(|candidate| candidate.same_source(&origin)) {
                values.update_matching(|candidate| candidate.same_source(&origin), &mut update);
                selected_present |= candidate == place;
            }
        }
        if !selected_present {
            let mut refined = origins.clone();
            refined.update_matching(|candidate| candidate.same_source(&origin), update);
            self.set_place(place.clone(), refined);
        }
    }

    /// Returns the value denoted by `expr`.
    ///
    /// `computed_property` is attached only when the expression has no tracked variable or call
    /// result identity.
    pub fn expr<'hir>(
        &self,
        gcx: Gcx<'hir>,
        expr: &'hir Expr<'hir>,
        computed_property: P,
    ) -> ValueSet<P> {
        self.expr_with_property(gcx, expr, &mut |_| computed_property.clone())
    }

    /// Returns the value denoted by `expr`, deriving properties per computed leaf.
    pub fn expr_with_property<'hir>(
        &self,
        gcx: Gcx<'hir>,
        expr: &'hir Expr<'hir>,
        property_of: &mut impl FnMut(&'hir Expr<'hir>) -> P,
    ) -> ValueSet<P> {
        let expr = expr.peel_parens();
        match &expr.kind {
            ExprKind::Assign(_, None, value) => self.expr_with_property(gcx, value, property_of),
            ExprKind::Ternary(_, then_expr, else_expr) => {
                let mut values = self.expr_with_property(gcx, then_expr, property_of);
                _ = values.join(&self.expr_with_property(gcx, else_expr, property_of));
                values
            }
            ExprKind::Call(..) => self
                .call_results
                .outputs(expr.id)
                .and_then(|outputs| match outputs {
                    [output] => Some(output),
                    _ => None,
                })
                .cloned()
                .unwrap_or_else(|| self.place_or_variable(gcx, expr, property_of(expr))),
            _ => self.place_or_variable(gcx, expr, property_of(expr)),
        }
    }

    /// Returns the tracked value of a source place, if `expr` denotes one.
    ///
    /// Whole variables and named-field projections are tracked precisely. Indexed and sliced
    /// places remain root-conservative because their runtime alias relation is not generally
    /// decidable from HIR expression identity.
    pub fn place_value<'hir>(&self, gcx: Gcx<'hir>, expr: &'hir Expr<'hir>) -> Option<ValueSet<P>> {
        let place = gcx.expr_place(expr)?;
        if !self.can_track_place(gcx, &place) {
            return None;
        }
        Some(self.value_for_place(&place))
    }

    /// Returns every tracked value observed by reading `place`.
    ///
    /// The result includes the exact move-path value and every tracked descendant field. Scalar
    /// places have no descendants; whole and projected aggregate roots include their contents,
    /// including through definite memory aliases. This is useful for analyses which must notice
    /// values escaping through a struct return, ABI encoding, or an aggregate call argument.
    pub fn values_read_from_place(&self, place: &Place) -> ValueSet<P> {
        let mut values = self.value_for_place(place);
        let mut roots = SmallVec::<[Place; 2]>::new();
        if place.projection().is_empty() {
            for value in values.iter() {
                let root = match value.origin() {
                    ValueOrigin::Initial(variable) | ValueOrigin::Opaque(variable) => {
                        Place::from_local(variable)
                    }
                    ValueOrigin::Place { anchor, .. } => anchor,
                    ValueOrigin::Expr(_) | ValueOrigin::CallResult(..) => continue,
                };
                if !roots.contains(&root) {
                    roots.push(root);
                }
            }
        } else {
            roots.extend(self.canonical_places(place));
        }
        for (candidate, descendant) in &self.places {
            if roots.iter().any(|root| {
                candidate.local() == root.local()
                    && candidate.projection().starts_with(root.projection())
                    && candidate != root
            }) {
                _ = values.join(descendant);
            }
        }
        values
    }

    /// Returns every tracked value observed by reading the place denoted by `expr`.
    pub fn values_read_from_expr<'hir>(
        &self,
        gcx: Gcx<'hir>,
        expr: &'hir Expr<'hir>,
    ) -> Option<ValueSet<P>> {
        let place = gcx.expr_place(expr)?;
        if !self.can_track_place(gcx, &place) {
            return None;
        }
        Some(self.values_read_from_place(&place))
    }

    /// Returns whether assignments and refinements can preserve the identity of `place`.
    ///
    /// Whole variables and named memory fields with known reference provenance are precise.
    /// Indexed/sliced places, storage-backed projections, and fields of opaque computed
    /// references are deliberately conservative.
    pub fn can_track_place<'hir>(&self, gcx: Gcx<'hir>, place: &Place) -> bool {
        place.projection().is_empty()
            || !place.is_state_backed(&gcx.hir)
                && precise_projected_place(place)
                && !self.canonical_places(place).is_empty()
    }

    fn place_or_variable<'hir>(
        &self,
        gcx: Gcx<'hir>,
        expr: &'hir Expr<'hir>,
        property: P,
    ) -> ValueSet<P> {
        self.place_value(gcx, expr)
            .unwrap_or_else(|| ValueSet::singleton(ValueOrigin::Expr(expr.id), property))
    }

    /// Returns one indexed output of a summarized call.
    pub fn call_result(&self, call: ExprId, index: usize) -> Option<&ValueSet<P>> {
        self.call_results.output(call, index)
    }

    /// Sets the abstract outputs of `call`.
    pub fn set_call_results(
        &mut self,
        call: ExprId,
        outputs: impl IntoIterator<Item = ValueSet<P>>,
    ) {
        self.call_results.set_outputs(call, outputs);
    }

    /// Applies a simultaneous source assignment.
    pub fn assign<'hir>(
        &mut self,
        gcx: Gcx<'hir>,
        target: &'hir Expr<'hir>,
        value: &'hir Expr<'hir>,
        computed_property: P,
    ) {
        let invalidated_property = computed_property.clone();
        self.assign_with(
            gcx,
            target,
            value,
            |_| true,
            |state, value| state.expr(gcx, value, computed_property.clone()),
            invalidated_property,
        );
    }

    /// Applies a simultaneous assignment with lint-specific tracking and value policies.
    pub fn assign_with<'hir>(
        &mut self,
        gcx: Gcx<'hir>,
        target: &'hir Expr<'hir>,
        value: &'hir Expr<'hir>,
        mut tracks: impl FnMut(VariableId) -> bool,
        mut value_of: impl FnMut(&Self, &'hir Expr<'hir>) -> ValueSet<P>,
        invalidated_property: P,
    ) {
        let site = value.id;
        let assignments: SmallVec<[PlaceBinding<'hir, P>; 4]> =
            assignment_pairs(target, Some(value))
                .into_iter()
                .filter_map(|assignment| {
                    let place = gcx.expr_place(assignment.target)?;
                    if !tracks(place.local()) {
                        return None;
                    }
                    let output = self.value_for_output_with(
                        value,
                        assignment.output_index,
                        assignment.output_count,
                        &mut value_of,
                        invalidated_property.clone(),
                    );
                    Some((place, assignment.value, output))
                })
                .collect();
        for (place, source, value) in assignments {
            if !place.projection().is_empty() && place.is_state_backed(&gcx.hir) {
                self.invalidate_variable(place.local(), site, invalidated_property.clone());
                continue;
            }
            if place.projection().is_empty() {
                if let Some(source) = source {
                    self.assign_variable_from_expr(gcx, place.local(), source, value);
                } else {
                    self.assign_variable_value(gcx, place.local(), value);
                }
            } else {
                self.set_place(place, value);
            }
        }
    }

    /// Invalidates variables assigned through `target`.
    pub fn invalidate_lvalue<'hir>(
        &mut self,
        gcx: Gcx<'hir>,
        target: &'hir Expr<'hir>,
        site: ExprId,
        property: P,
    ) {
        self.invalidate_lvalue_with(gcx, target, site, |_| true, property);
    }

    /// Invalidates selected variables assigned through `target`.
    pub fn invalidate_lvalue_with<'hir>(
        &mut self,
        gcx: Gcx<'hir>,
        target: &'hir Expr<'hir>,
        site: ExprId,
        mut tracks: impl FnMut(VariableId) -> bool,
        property: P,
    ) {
        let places: SmallVec<[Place; 4]> = assignment_pairs(target, None)
            .into_iter()
            .filter_map(|assignment| gcx.expr_place(assignment.target))
            .filter(|place| tracks(place.local()))
            .collect();
        for place in places {
            if !place.projection().is_empty() && place.is_state_backed(&gcx.hir) {
                self.invalidate_variable(place.local(), site, property.clone());
                continue;
            }
            self.invalidate_place(place, site, property.clone());
        }
    }

    /// Applies standard assignment and mutation effects with one computed-value property.
    pub fn apply_expr<'hir>(
        &mut self,
        gcx: Gcx<'hir>,
        expr: &'hir Expr<'hir>,
        computed_property: P,
        mutation_property: P,
        delete_property: P,
    ) {
        self.apply_expr_with(
            gcx,
            expr,
            &mut |_| true,
            &mut |state, value| state.expr(gcx, value, computed_property.clone()),
            mutation_property,
            delete_property,
        );
    }

    /// Applies standard assignment and mutation effects under lint-specific value policies.
    pub fn apply_expr_with<'hir>(
        &mut self,
        gcx: Gcx<'hir>,
        expr: &'hir Expr<'hir>,
        tracks: &mut impl FnMut(VariableId) -> bool,
        value_of: &mut impl FnMut(&Self, &'hir Expr<'hir>) -> ValueSet<P>,
        mutation_property: P,
        delete_property: P,
    ) {
        self.record_evaluation(expr.id);
        match &expr.kind {
            ExprKind::Assign(target, operator, value) if operator.is_none() => {
                self.assign_with(
                    gcx,
                    target,
                    value,
                    &mut *tracks,
                    &mut *value_of,
                    mutation_property,
                );
            }
            ExprKind::Assign(target, ..) => {
                self.invalidate_lvalue_with(gcx, target, expr.id, &mut *tracks, mutation_property)
            }
            ExprKind::Delete(target) => {
                self.invalidate_lvalue_with(gcx, target, expr.id, &mut *tracks, delete_property)
            }
            ExprKind::Unary(operation, target)
                if matches!(
                    operation.kind,
                    UnOpKind::PreInc | UnOpKind::PreDec | UnOpKind::PostInc | UnOpKind::PostDec
                ) =>
            {
                self.invalidate_lvalue_with(gcx, target, expr.id, tracks, mutation_property);
            }
            _ => {}
        }
    }

    /// Binds an applied modifier's parameters from its arguments.
    pub fn bind_modifier<'hir>(
        &mut self,
        cx: EffectiveBodyCx<'hir>,
        modifier: &'hir Modifier<'hir>,
        callee: super::FunctionId,
        computed_property: P,
    ) {
        let invalidated_property = computed_property.clone();
        self.bind_modifier_with(
            cx,
            modifier,
            callee,
            &mut |_| true,
            &mut |state, value| state.expr(cx.gcx(), value, computed_property.clone()),
            invalidated_property,
        );
    }

    /// Binds modifier parameters under lint-specific tracking and value policies.
    pub fn bind_modifier_with<'hir>(
        &mut self,
        cx: EffectiveBodyCx<'hir>,
        modifier: &'hir Modifier<'hir>,
        callee: super::FunctionId,
        tracks: &mut impl FnMut(VariableId) -> bool,
        value_of: &mut impl FnMut(&Self, &'hir Expr<'hir>) -> ValueSet<P>,
        unknown_property: P,
    ) {
        let function = cx.hir().function(callee);
        let bindings: SmallVec<[ParameterBinding<'hir, P>; 4]> = function
            .parameters
            .iter()
            .enumerate()
            .filter(|item| tracks(*item.1))
            .map(|(index, &parameter)| {
                let value = cx
                    .gcx()
                    .modifier_arg(modifier, index)
                    .map(|argument| (argument, value_of(self, argument)));
                (parameter, value)
            })
            .collect();
        let returns: SmallVec<[VariableId; 2]> =
            function.returns.iter().copied().filter(|&variable| tracks(variable)).collect();
        self.apply_parameter_bindings(cx.gcx(), &returns, bindings, unknown_property);
    }

    /// Binds an internal callee's parameters from its call arguments.
    pub fn bind_call<'hir>(
        &mut self,
        cx: EffectiveBodyCx<'hir>,
        call: &'hir Expr<'hir>,
        callee: super::FunctionId,
        computed_property: P,
    ) {
        let invalidated_property = computed_property.clone();
        self.bind_call_with(
            cx,
            call,
            callee,
            &mut |_| true,
            &mut |state, value| state.expr(cx.gcx(), value, computed_property.clone()),
            invalidated_property,
        );
    }

    /// Binds call parameters under lint-specific tracking and value policies.
    pub fn bind_call_with<'hir>(
        &mut self,
        cx: EffectiveBodyCx<'hir>,
        call: &'hir Expr<'hir>,
        callee: super::FunctionId,
        tracks: &mut impl FnMut(VariableId) -> bool,
        value_of: &mut impl FnMut(&Self, &'hir Expr<'hir>) -> ValueSet<P>,
        unknown_property: P,
    ) {
        let function = cx.hir().function(callee);
        let bindings: SmallVec<[ParameterBinding<'hir, P>; 4]> = function
            .parameters
            .iter()
            .enumerate()
            .filter(|item| tracks(*item.1))
            .map(|(index, &parameter)| {
                let value = cx
                    .gcx()
                    .call_arg_for_param(call, index)
                    .map(|argument| (argument, value_of(self, argument)));
                (parameter, value)
            })
            .collect();
        let returns: SmallVec<[VariableId; 2]> =
            function.returns.iter().copied().filter(|&variable| tracks(variable)).collect();
        self.apply_parameter_bindings(cx.gcx(), &returns, bindings, unknown_property);
    }

    /// Restores caller locals and maps callee return variables to `call`.
    pub fn return_from_call<'hir>(
        &mut self,
        cx: EffectiveBodyCx<'hir>,
        call: &'hir Expr<'hir>,
        callee: super::FunctionId,
        caller: &Self,
    ) {
        let variables = cx.activation_variables(callee);
        let outputs: SmallVec<[ValueSet<P>; 2]> = cx
            .hir()
            .function(callee)
            .returns
            .iter()
            .enumerate()
            .map(|(index, &variable)| {
                let values = self.variable(variable);
                if cx.hir().variable(variable).data_location != Some(DataLocation::Memory) {
                    return values;
                }
                values.map_origins(|origin| {
                    let is_local = match &origin {
                        ValueOrigin::Initial(variable) | ValueOrigin::Opaque(variable) => {
                            variables.contains(variable)
                        }
                        ValueOrigin::Place { anchor, .. } => variables.contains(&anchor.local()),
                        ValueOrigin::Expr(_) | ValueOrigin::CallResult(..) => true,
                    };
                    if is_local { ValueOrigin::CallResult(call.id, index) } else { origin }
                })
            })
            .collect();
        for &variable in &variables {
            self.values.restore(variable, &caller.values);
        }
        self.restore_places(&variables, caller);
        self.call_results.set_outputs(call.id, outputs);
    }

    /// Restores values owned by one completed modifier activation.
    pub fn return_from_modifier(
        &mut self,
        cx: EffectiveBodyCx<'_>,
        callee: super::FunctionId,
        caller: &Self,
    ) {
        let variables = cx.activation_variables(callee);
        for &variable in &variables {
            self.values.restore(variable, &caller.values);
        }
        self.restore_places(&variables, caller);
    }

    /// Binds values introduced by one `try` success or catch clause.
    pub fn bind_try_clause(
        &mut self,
        try_expr: &Expr<'_>,
        clause: &TryCatchClause<'_>,
        is_success: bool,
        unknown_property: P,
    ) {
        self.bind_try_clause_with(try_expr, clause, is_success, &mut |_| true, unknown_property);
    }

    /// Binds selected values introduced by one `try` success or catch clause.
    pub fn bind_try_clause_with(
        &mut self,
        try_expr: &Expr<'_>,
        clause: &TryCatchClause<'_>,
        is_success: bool,
        tracks: &mut impl FnMut(VariableId) -> bool,
        unknown_property: P,
    ) {
        let bindings: SmallVec<[(VariableId, Option<ValueSet<P>>); 4]> = clause
            .args
            .iter()
            .enumerate()
            .filter(|item| tracks(*item.1))
            .map(|(index, &variable)| {
                let value =
                    is_success.then(|| self.call_result(try_expr.id, index).cloned()).flatten();
                (variable, value)
            })
            .collect();
        for (variable, value) in bindings {
            if let Some(value) = value {
                self.set_variable(variable, value);
            } else {
                self.reset_variable(variable, unknown_property.clone());
            }
        }
    }

    /// Applies declaration or explicit-return value binding for `statement`.
    pub fn apply_statement<'hir>(
        &mut self,
        cx: EffectiveBodyCx<'hir>,
        statement: &'hir Stmt<'hir>,
        computed_property: P,
    ) {
        let invalidated_property = computed_property.clone();
        self.apply_statement_with(
            cx,
            statement,
            |_| true,
            |state, value| state.expr(cx.gcx(), value, computed_property.clone()),
            invalidated_property,
        );
    }

    /// Applies declaration/return binding with lint-specific tracking and value policies.
    pub fn apply_statement_with<'hir>(
        &mut self,
        cx: EffectiveBodyCx<'hir>,
        statement: &'hir Stmt<'hir>,
        mut tracks: impl FnMut(VariableId) -> bool,
        mut value_of: impl FnMut(&Self, &'hir Expr<'hir>) -> ValueSet<P>,
        invalidated_property: P,
    ) {
        match statement.kind {
            StmtKind::DeclSingle(variable) => {
                if !tracks(variable) {
                    return;
                }
                if let Some(initializer) = cx.hir().variable(variable).initializer {
                    let value = value_of(self, initializer);
                    self.assign_variable_from_expr(cx.gcx(), variable, initializer, value);
                } else {
                    self.reset_variable(variable, invalidated_property);
                }
            }
            StmtKind::DeclMulti(variables, initializer) => {
                self.assign_variables_with(
                    cx.gcx(),
                    variables,
                    initializer,
                    &mut tracks,
                    &mut value_of,
                    invalidated_property,
                );
            }
            StmtKind::Return(Some(value)) => {
                self.assign_variables_with(
                    cx.gcx(),
                    &cx.hir()
                        .function(cx.function())
                        .returns
                        .iter()
                        .copied()
                        .map(Some)
                        .collect::<SmallVec<[_; 4]>>(),
                    value,
                    &mut tracks,
                    &mut value_of,
                    invalidated_property,
                );
            }
            _ => {}
        }
    }

    fn value_for_place(&self, place: &Place) -> ValueSet<P> {
        if place.projection().is_empty() {
            return self.values.variable(place.local());
        }

        let canonical = self.canonical_places_with_generation(place);
        let mut values = ValueSet::default();
        for (observed, identity, generation) in canonical {
            let candidate_values = self.places.get(&observed).cloned().unwrap_or_else(|| {
                ValueSet::singleton(
                    ValueOrigin::Place { identity, anchor: observed.clone(), generation },
                    self.values.initial_property(place.local()),
                )
            });
            _ = values.join(&candidate_values);
        }
        if values.is_empty() {
            return ValueSet::singleton(
                ValueOrigin::Opaque(place.local()),
                self.values.initial_property(place.local()),
            );
        }
        values
    }

    fn set_place(&mut self, place: Place, value: ValueSet<P>) {
        let canonical = self.canonical_places(&place);
        if canonical.is_empty() {
            if let Some(property) = value.iter().next().map(|value| value.property().clone()) {
                self.reset_variable(place.local(), property);
            }
            return;
        }
        if !place.projection().is_empty() {
            let aliases = canonical.len();
            for place in canonical {
                let value = if aliases == 1 {
                    value.clone()
                } else {
                    let mut joined = self.value_for_place(&place);
                    _ = joined.join(&value);
                    joined
                };
                self.places.retain(|candidate, _| !candidate.may_overlap(&place));
                self.places.insert(place, value);
            }
        } else if place.projection().is_empty() {
            self.set_variable(place.local(), value);
        }
    }

    fn invalidate_place(&mut self, place: Place, site: ExprId, property: P) {
        self.set_place(place, ValueSet::singleton(ValueOrigin::Expr(site), property));
    }

    fn canonical_places(&self, place: &Place) -> SmallVec<[Place; 2]> {
        if place.projection().is_empty() {
            let mut places = SmallVec::new();
            places.push(place.clone());
            return places;
        }
        let mut places = SmallVec::new();
        for (canonical, _, _) in self.canonical_places_with_generation(place) {
            if !places.contains(&canonical) {
                places.push(canonical);
            }
        }
        places
    }

    fn canonical_places_with_generation(
        &self,
        place: &Place,
    ) -> SmallVec<[(Place, Place, ValueGeneration); 2]> {
        if !precise_projected_place(place) {
            return SmallVec::new();
        }
        let mut places = SmallVec::new();
        for value in self.values.variable(place.local()).iter() {
            let canonical = match value.origin() {
                ValueOrigin::Initial(variable) => {
                    let place = place.with_local(variable);
                    (place.clone(), place, ValueGeneration::Initial(variable))
                }
                ValueOrigin::Opaque(variable) => {
                    let place = place.with_local(variable);
                    (place.clone(), place, ValueGeneration::Opaque(variable))
                }
                ValueOrigin::Place { identity, anchor, generation } => (
                    anchor.with_projection_suffix(place.projection()),
                    identity.with_projection_suffix(place.projection()),
                    generation,
                ),
                ValueOrigin::Expr(_) | ValueOrigin::CallResult(..) => continue,
            };
            if !places.contains(&canonical) {
                places.push(canonical);
            }
        }
        places
    }

    fn assign_variable_from_expr<'hir>(
        &mut self,
        gcx: Gcx<'hir>,
        variable: VariableId,
        source: &'hir Expr<'hir>,
        values: ValueSet<P>,
    ) {
        let target = Place::from_local(variable);
        let values = if gcx.hir.variable(variable).data_location == Some(DataLocation::Memory) {
            values.map_origins(|origin| match origin {
                ValueOrigin::Expr(_) | ValueOrigin::CallResult(..) => {
                    let generation = ValueGeneration::from_origin(&origin);
                    ValueOrigin::place(target.clone(), generation)
                }
                origin => origin,
            })
        } else {
            values
        };
        if !self.assignment_deep_copies_into_memory(gcx, variable, source) {
            self.set_variable(variable, values);
            return;
        }

        let generation = ValueGeneration::Expr(source.id);
        let copied_places = gcx.expr_place(source).map_or_else(SmallVec::new, |source| {
            let mut copied = SmallVec::<[(Place, ValueSet<P>); 4]>::new();
            for source in self.canonical_places(&source) {
                for (place, values) in &self.places {
                    if place.local() != source.local() {
                        continue;
                    }
                    let Some(projection) = place.projection().strip_prefix(source.projection())
                    else {
                        continue;
                    };
                    if projection.is_empty() {
                        continue;
                    }
                    let destination = target.with_projection_suffix(projection);
                    let values = values
                        .with_origin(ValueOrigin::place(destination.clone(), generation.clone()));
                    if let Some((_, existing)) =
                        copied.iter_mut().find(|(place, _)| *place == destination)
                    {
                        _ = existing.join(&values);
                    } else {
                        copied.push((destination, values));
                    }
                }
            }
            copied
        });

        self.set_variable(variable, values.with_origin(ValueOrigin::place(target, generation)));
        self.places.extend(copied_places);
    }

    fn assign_variable_value(&mut self, gcx: Gcx<'_>, variable: VariableId, values: ValueSet<P>) {
        let values = if gcx.hir.variable(variable).data_location == Some(DataLocation::Memory) {
            let target = Place::from_local(variable);
            values.map_origins(|origin| match origin {
                ValueOrigin::Expr(_) | ValueOrigin::CallResult(..) => {
                    let generation = ValueGeneration::from_origin(&origin);
                    ValueOrigin::place(target.clone(), generation)
                }
                origin => origin,
            })
        } else {
            values
        };
        self.set_variable(variable, values);
    }

    fn assignment_deep_copies_into_memory<'hir>(
        &self,
        gcx: Gcx<'hir>,
        variable: VariableId,
        source: &'hir Expr<'hir>,
    ) -> bool {
        gcx.hir.variable(variable).data_location == Some(DataLocation::Memory)
            && matches!(
                gcx.type_of_expr(source.id).and_then(|ty| ty.loc()),
                Some(DataLocation::Calldata | DataLocation::Storage | DataLocation::Transient)
            )
    }

    fn set_variable(&mut self, variable: VariableId, value: ValueSet<P>) {
        if self.values.variable(variable) == value
            && value.iter().all(|value| self.evaluated.origin_is_correlatable(&value.origin))
        {
            self.values.set(variable, value);
            return;
        }
        self.detach_rebound_place_aliases(variable);
        self.places.retain(|place, _| place.local() != variable);
        self.values.set(variable, value);
    }

    /// Re-anchors surviving aliases and tracked fields when their source variable is rebound.
    ///
    /// Aliases must be detached even when no descendant field was materialized: otherwise a saved
    /// root from an earlier loop iteration would resolve through the freshly rebound source root.
    fn detach_rebound_place_aliases(&mut self, variable: VariableId) {
        // `set_variable` only removes move paths owned by this source variable. Rebinding a
        // non-owning alias therefore leaves its source anchor alive without any detachment.
        let old_root = Place::from_local(variable);
        let candidates: SmallVec<[VariableId; 4]> = self.values.variables().collect();
        let mut aliases = SmallVec::<[(VariableId, ValueOrigin, Place); 4]>::new();
        for candidate in candidates {
            if candidate == variable {
                continue;
            }
            let values = self.values.variable(candidate);
            for value in values.iter() {
                let origin = value.origin();
                let base = match &origin {
                    ValueOrigin::Initial(variable) | ValueOrigin::Opaque(variable) => {
                        Place::from_local(*variable)
                    }
                    ValueOrigin::Place { anchor, .. } => anchor.clone(),
                    ValueOrigin::Expr(_) | ValueOrigin::CallResult(..) => continue,
                };
                let alias = (candidate, origin, base);
                if alias.2.local() == old_root.local()
                    && alias.2.projection().starts_with(old_root.projection())
                    && !aliases.contains(&alias)
                {
                    aliases.push(alias);
                }
            }
        }
        aliases.sort_by_key(|(_, _, base)| base.projection().len());

        // One surviving alias anchors each disjoint subtree. A shallower alias also anchors every
        // nested alias so mutations through either path continue to share one object.
        let mut mappings = SmallVec::<[(Place, Place); 4]>::new();
        for &(alias, _, ref base) in &aliases {
            if !mappings.iter().any(|(source, _)| {
                base.local() == source.local() && base.projection().starts_with(source.projection())
            }) {
                mappings.push((base.clone(), Place::from_local(alias)));
            }
        }
        if mappings.is_empty() {
            return;
        }

        let mut detached = SmallVec::<[(Place, ValueSet<P>); 4]>::new();
        self.places.retain(|place, values| {
            if place.local() != old_root.local()
                || !place.projection().starts_with(old_root.projection())
            {
                return true;
            }
            if let Some((source, target)) = mappings.iter().find(|(source, _)| {
                place.local() == source.local()
                    && place.projection().starts_with(source.projection())
            }) {
                let projection = place.projection().strip_prefix(source.projection()).unwrap();
                if !projection.is_empty() {
                    let values = values.map_origins(|origin| match origin {
                        ValueOrigin::Place { identity, anchor, generation } => {
                            let Some(anchor) = mappings.iter().find_map(|(source, target)| {
                                if anchor.local() != source.local() {
                                    return None;
                                }
                                let projection =
                                    anchor.projection().strip_prefix(source.projection())?;
                                Some(target.with_projection_suffix(projection))
                            }) else {
                                return ValueOrigin::Place { identity, anchor, generation };
                            };
                            ValueOrigin::Place { identity, anchor, generation }
                        }
                        origin => origin,
                    });
                    detached.push((target.with_projection_suffix(projection), values));
                }
            }
            false
        });
        for (place, values) in detached {
            if let Some(existing) = self.places.get_mut(&place) {
                _ = existing.join(&values);
            } else {
                self.places.insert(place, values);
            }
        }

        for (alias, origin, base) in aliases {
            let (source, target) = mappings
                .iter()
                .find(|(source, _)| {
                    base.local() == source.local()
                        && base.projection().starts_with(source.projection())
                })
                .unwrap();
            let projection = base.projection().strip_prefix(source.projection()).unwrap();
            let (identity, generation) = match &origin {
                ValueOrigin::Initial(variable) => {
                    (Place::from_local(*variable), ValueGeneration::Initial(*variable))
                }
                ValueOrigin::Opaque(variable) => {
                    (Place::from_local(*variable), ValueGeneration::Opaque(*variable))
                }
                ValueOrigin::Place { identity, generation, .. } => {
                    (identity.clone(), generation.clone())
                }
                ValueOrigin::Expr(_) | ValueOrigin::CallResult(..) => unreachable!(),
            };
            let detached_origin = ValueOrigin::Place {
                identity,
                anchor: target.with_projection_suffix(projection),
                generation,
            };
            let values = self.values.variable(alias).map_origins(|candidate| {
                if candidate == origin { detached_origin.clone() } else { candidate }
            });
            self.values.set(alias, values);
        }
    }

    fn reset_variable(&mut self, variable: VariableId, property: P) {
        self.set_variable(variable, ValueSet::singleton(ValueOrigin::Opaque(variable), property));
    }

    fn restore_places(&mut self, variables: &[VariableId], caller: &Self) {
        self.places.retain(|place, _| !variables.contains(&place.local()));
        self.places.extend(
            caller
                .places
                .iter()
                .filter(|(place, _)| variables.contains(&place.local()))
                .map(|(place, values)| (place.clone(), values.clone())),
        );
    }

    fn apply_parameter_bindings<'hir>(
        &mut self,
        gcx: Gcx<'hir>,
        returns: &[VariableId],
        bindings: impl IntoIterator<Item = (VariableId, Option<(&'hir Expr<'hir>, ValueSet<P>)>)>,
        property: P,
    ) {
        for (parameter, value) in bindings {
            if let Some((source, value)) = value {
                self.assign_variable_from_expr(gcx, parameter, source, value);
            } else {
                self.reset_variable(parameter, property.clone());
            }
        }
        for &return_ in returns {
            self.reset_variable(return_, property.clone());
        }
    }

    fn assign_variables_with<'hir>(
        &mut self,
        gcx: Gcx<'hir>,
        variables: &[Option<VariableId>],
        value: &'hir Expr<'hir>,
        tracks: &mut impl FnMut(VariableId) -> bool,
        value_of: &mut impl FnMut(&Self, &'hir Expr<'hir>) -> ValueSet<P>,
        invalidated_property: P,
    ) {
        let bindings: SmallVec<[VariableBinding<'hir, P>; 4]> = variables
            .iter()
            .copied()
            .enumerate()
            .filter_map(|(index, variable)| {
                let variable = variable?;
                if !tracks(variable) {
                    return None;
                }
                let output = self.value_for_output_with(
                    value,
                    index,
                    variables.len(),
                    value_of,
                    invalidated_property.clone(),
                );
                let source = if let ExprKind::Tuple(values) = &value.peel_parens().kind
                    && variables.len() > 1
                {
                    values.get(index).copied().flatten()
                } else {
                    (variables.len() == 1).then_some(value)
                };
                Some((variable, source, output))
            })
            .collect();
        for (variable, source, value) in bindings {
            if let Some(source) = source {
                self.assign_variable_from_expr(gcx, variable, source, value);
            } else {
                self.assign_variable_value(gcx, variable, value);
            }
        }
    }

    fn value_for_output_with<'hir>(
        &self,
        value: &'hir Expr<'hir>,
        index: usize,
        outputs: usize,
        value_of: &mut impl FnMut(&Self, &'hir Expr<'hir>) -> ValueSet<P>,
        invalidated_property: P,
    ) -> ValueSet<P> {
        if let ExprKind::Tuple(values) = &value.peel_parens().kind
            && outputs > 1
            && let Some(Some(value)) = values.get(index)
        {
            return value_of(self, value);
        }
        if matches!(value.peel_parens().kind, ExprKind::Call(..))
            && let Some(result) = self.call_result(value.id, index)
        {
            return result.clone();
        }
        if outputs == 1 && index == 0 {
            value_of(self, value)
        } else {
            ValueSet::singleton(ValueOrigin::Expr(value.id), invalidated_property)
        }
    }
}

fn precise_projected_place(place: &Place) -> bool {
    !place.projection().is_empty()
        && place
            .projection()
            .iter()
            .all(|projection| matches!(projection, ProjectionElem::Field { .. }))
}

impl<P: Clone + Eq> JoinSemiLattice for ValueFlowState<P> {
    fn join(&mut self, other: &Self) -> bool {
        let left = self.clone();
        let places: SmallVec<[Place; 8]> =
            self.places.keys().chain(other.places.keys()).cloned().collect();
        let mut changed = false;
        for place in places {
            let mut values = left.value_for_place(&place);
            _ = values.join(&other.value_for_place(&place));
            if self.places.get(&place) != Some(&values) {
                self.places.insert(place, values);
                changed = true;
            }
        }
        changed |= self.values.join(&other.values);
        changed |= self.call_results.join(&other.call_results);
        changed |= self.evaluated.join(&other.evaluated);
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Compiler,
        hir::{
            EffectiveFlowAnalysis, ExprUse, FunctionId, InternalCallMode, OperandOrder,
            analyze_effective_body_flow,
        },
    };
    use solar_interface::{Session, config::CompileOpts};
    use std::{ops::ControlFlow, path::PathBuf};

    fn variable(index: usize) -> VariableId {
        VariableId::new(index)
    }

    fn expr(index: usize) -> ExprId {
        ExprId::new(index)
    }

    #[test]
    fn joins_properties_per_reaching_value() {
        let target = variable(0);
        let mut left = ValueState::new(false);
        left.invalidate(target, expr(0), true);
        let mut right = ValueState::new(false);
        right.invalidate(target, expr(1), true);

        assert!(left.join(&right));
        assert!(left.variable(target).all(|value| *value.property()));

        let mut unsafe_path = ValueState::new(false);
        unsafe_path.invalidate(target, expr(0), false);
        assert!(left.join(&unsafe_path));
        assert!(!left.variable(target).all(|value| *value.property()));
    }

    #[test]
    fn empty_values_do_not_prove_properties() {
        assert!(!ValueSet::<bool>::default().is_proven(|value| *value.property()));
    }

    #[test]
    fn seeds_properties_for_untouched_variables() {
        let target = variable(0);
        let mut state = ValueState::new(false);

        state.seed(target, true);
        assert!(state.variable(target).all(|value| *value.property()));

        let mut branch = state.clone();
        branch.invalidate(variable(1), expr(0), false);
        assert!(state.join(&branch));
        assert!(state.variable(target).all(|value| *value.property()));

        state.forget();
        assert!(state.variable(target).all(|value| *value.property()));
    }

    #[test]
    fn updates_materialized_aliases_by_origin() {
        let source = variable(0);
        let alias = variable(1);
        let mut state = ValueState::new(false);
        let source_values = state.variable(source);
        state.set(source, source_values.clone());
        state.set(alias, source_values.clone());
        state.update_definite_aliases(&source_values, |property| *property = true);

        assert!(state.variable(source).all(|value| *value.property()));
        assert!(state.variable(alias).all(|value| *value.property()));
    }

    #[test]
    fn updates_implicit_origin_variable() {
        let source = variable(0);
        let alias = variable(1);
        let mut state = ValueState::new(false);
        let source_values = state.variable(source);
        state.set(alias, source_values.clone());
        state.update_definite_aliases(&source_values, |property| *property = true);

        assert!(state.variable(source).all(|value| *value.property()));
        assert!(state.variable(alias).all(|value| *value.property()));
    }

    #[test]
    fn does_not_treat_joined_origins_as_definite_aliases() {
        let a = variable(0);
        let b = variable(1);
        let x = variable(2);
        let y = variable(3);
        let mut alternatives = ValueState::new(false).variable(a);
        _ = alternatives.join(&ValueState::new(false).variable(b));

        let mut state = ValueState::new(false);
        state.set(x, alternatives.clone());
        state.set(y, alternatives.clone());
        state.update_definite_aliases(&alternatives, |property| *property = true);

        assert!(!state.variable(x).all(|value| *value.property()));
        assert!(!state.variable(y).all(|value| *value.property()));
    }

    #[test]
    fn does_not_treat_expression_sites_as_dynamic_aliases() {
        let x = variable(0);
        let y = variable(1);
        let value = ValueSet::singleton(ValueOrigin::Expr(expr(0)), false);
        let mut state = ValueState::new(false);
        state.set(x, value.clone());
        state.set(y, value.clone());
        state.update_definite_aliases(&value, |property| *property = true);

        assert!(!state.variable(x).all(|value| *value.property()));
        assert!(!state.variable(y).all(|value| *value.property()));
    }

    #[test]
    fn evaluation_multiplicity_distinguishes_paths_from_repetition() {
        let site = expr(0);
        let mut left = EvaluatedSites::default();
        let mut right = EvaluatedSites::default();
        assert!(!left.record(site));
        assert!(!right.record(site));
        assert!(!left.join(&right));
        assert!(left.is_correlatable(site));

        assert!(left.record(site));
        assert!(!left.is_correlatable(site));

        let mut flow = ValueFlowState::new(false);
        let mut repeated = ValueFlowState::new(false);
        repeated.record_evaluation(site);
        repeated.record_evaluation(site);
        assert!(flow.join(&repeated));
        assert!(!flow.evaluated_sites().is_correlatable(site));
        assert!(!flow.evaluated_sites().origin_is_correlatable(&ValueOrigin::Opaque(variable(0))));

        flow.forget_values();
        assert!(!flow.evaluated_sites().is_correlatable(site));
    }

    #[test]
    fn repeated_projected_generations_do_not_refine_stale_aliases() {
        let owner = variable(0);
        let selected = variable(1);
        let stale_alias = variable(2);
        let site = expr(0);
        let origin = ValueOrigin::place(Place::from_local(owner), ValueGeneration::Expr(site));
        let values = ValueSet::singleton(origin, false);
        let mut flow = ValueFlowState::new(false);
        flow.values.set(selected, values.clone());
        flow.values.set(stale_alias, values.clone());
        flow.record_evaluation(site);
        flow.record_evaluation(site);

        flow.refine_variable_aliases(selected, &values, |property| *property = true);

        assert!(flow.variable(selected).is_proven(|value| *value.property()));
        assert!(!flow.variable(stale_alias).is_proven(|value| *value.property()));
    }

    #[test]
    fn detached_anchor_refines_scalar_copy_by_stable_source() {
        let holder = variable(0);
        let detached_alias = variable(1);
        let scalar_copy = variable(2);
        let identity = Place::from_local(holder);
        let generation = ValueGeneration::Initial(holder);
        let original = ValueOrigin::place(identity.clone(), generation.clone());
        let detached =
            ValueOrigin::Place { identity, anchor: Place::from_local(detached_alias), generation };
        let mut flow = ValueFlowState::new(false);
        flow.values.set(scalar_copy, ValueSet::singleton(original.clone(), false));
        let selected = ValueSet::singleton(detached.clone(), false);
        flow.values.set(detached_alias, selected.clone());

        flow.refine_variable_aliases(detached_alias, &selected, |property| *property = true);

        assert!(flow.variable(detached_alias).is_proven(|value| *value.property()));
        assert!(flow.variable(scalar_copy).is_proven(|value| *value.property()));
        assert!(flow.evaluated_sites().origins_are_correlatable(&original, &detached));
    }

    #[test]
    fn repeated_generations_reanchor_aliases_without_materialized_fields() {
        let current = variable(0);
        let saved = variable(1);
        let site = expr(0);
        let generation = ValueGeneration::CallResult(site, 0);
        let current_origin = ValueOrigin::place(Place::from_local(current), generation.clone());
        let values = ValueSet::singleton(current_origin.clone(), false);
        let mut flow = ValueFlowState::new(false);
        flow.values.set(current, values.clone());
        flow.values.set(saved, values.clone());
        flow.record_evaluation(site);
        flow.record_evaluation(site);

        flow.set_variable(current, values);

        let saved_origin = flow.variable(saved).common_origin().unwrap();
        assert_eq!(
            saved_origin,
            ValueOrigin::Place {
                identity: Place::from_local(current),
                anchor: Place::from_local(saved),
                generation,
            },
        );
        assert_ne!(current_origin, saved_origin);
        assert!(current_origin.same_source(&saved_origin));
        assert!(!flow.evaluated_sites().origins_are_correlatable(&current_origin, &saved_origin));
        assert!(flow.places.is_empty());
    }

    #[test]
    fn value_flow_composes_calls_modifiers_tuples_and_try_clauses() {
        const SOURCE: &str = r#"
contract C {
    address constant TRUSTED = address(1);

    struct Holder {
        address first;
        address second;
    }

    struct Outer {
        Holder inner;
    }

    modifier twice(address value) {
        address captured = value;
        _;
        sink(captured);
    }

    function entry(address unsafeValue, bool choose, Holder calldata input)
        external
        twice(unsafeValue)
        twice(TRUSTED)
    {
        address a = unsafeValue;
        address b = TRUSTED;
        (a, b) = (b, a);
        sink(a);
        sink(b);

        address returned = identity(a);
        sink(returned);

        try this.echo(a) returns (address clauseValue) {
            sink(clauseValue);
        } catch {}

        Holder memory local;
        local.first = TRUSTED;
        local.second = unsafeValue;
        sink(local.first);
        sink(local.second);
        sinkHolder(local);

        Outer memory outer;
        outer.inner.first = TRUSTED;
        outer.inner.second = unsafeValue;
        sinkHolder(outer.inner);

        Holder memory alias_ = local;
        alias_.first = unsafeValue;
        sink(local.first);

        Holder memory first;
        Holder memory second;
        first.first = TRUSTED;
        second.first = TRUSTED;
        Holder memory selected = choose ? first : second;
        selected.first = unsafeValue;
        sink(first.first);
        sink(second.first);

        Holder[] memory items = new Holder[](1);
        items[0].first = TRUSTED;
        sink(items[0].first);

        Holder memory calldataCopy = input;
        calldataCopy.first = TRUSTED;
        sink(input.first);

        canonicalizeCopy(input);
        sink(input.first);

        Holder memory returnedCopy = copyForReturn(input);
        returnedCopy.first = TRUSTED;
        sink(input.first);

        Holder memory freshA = fresh(unsafeValue);
        Holder memory freshB = fresh(unsafeValue);
        freshA.first = TRUSTED;
        sink(freshB.first);

        address tupleFirst;
        address tupleSecond;
        (tupleFirst, tupleSecond) = pair(TRUSTED, unsafeValue);
        sink(tupleFirst);
        sink(tupleSecond);
        address omitted;
        (, omitted) = pair(unsafeValue, TRUSTED);
        sink(omitted);
    }

    function modifierOnly(address unsafeValue) external twice(unsafeValue) twice(TRUSTED) {}

    function rebindAlias(address unsafeValue) external {
        Holder memory source;
        source.first = TRUSTED;
        Holder memory alias_ = source;
        source = fresh(unsafeValue);
        sink(alias_.first);
    }

    function rebindProjectedAlias(address unsafeValue) external {
        Outer memory outer;
        outer.inner.first = TRUSTED;
        Holder memory alias_ = outer.inner;
        Outer memory replacement;
        replacement.inner.first = unsafeValue;
        outer = replacement;
        sink(alias_.first);
    }

    function replacementGeneration(address unsafeValue, Holder memory holder) external {
        address oldGeneration = holder.first;
        holder = fresh(unsafeValue);
        address newGeneration = holder.first;
        sink(oldGeneration);
        sink(newGeneration);
    }

    function identity(address value) internal pure returns (address) {
        return value;
    }

    function echo(address value) external pure returns (address) {
        return value;
    }

    function canonicalizeCopy(Holder memory copy) internal pure {
        copy.first = TRUSTED;
    }

    function copyForReturn(Holder calldata input) internal pure returns (Holder memory) {
        return input;
    }

    function fresh(address value) internal pure returns (Holder memory result) {
        result.first = value;
    }

    function pair(address first, address second) internal pure returns (address, address) {
        return (first, second);
    }

    function sink(address) internal pure {}
    function sinkHolder(Holder memory) internal pure {}
}
"#;

        struct Analysis<'hir> {
            gcx: Gcx<'hir>,
            observed: Vec<(String, bool)>,
            modifier_entries: Vec<bool>,
            captured_declarations: Vec<bool>,
            aggregate_observed: Vec<(bool, bool)>,
            observed_origins: Vec<(String, Option<ValueOrigin>)>,
        }

        impl<'hir> EffectiveFlowAnalysis<'hir> for Analysis<'hir> {
            type Domain = ValueFlowState<bool>;

            fn operand_order(&self) -> OperandOrder {
                OperandOrder::Unspecified
            }

            fn apply_expr_effect(
                &mut self,
                cx: EffectiveBodyCx<'hir>,
                expr: &'hir Expr<'hir>,
                _use_: ExprUse,
                state: &mut Self::Domain,
            ) {
                state.apply_expr(self.gcx, expr, false, false, false);
                if cx.reports_enabled()
                    && cx.call_info(expr).and_then(|info| info.function()).is_some_and(|id| {
                        self.gcx
                            .hir
                            .function(id)
                            .name
                            .is_some_and(|name| name.name.as_str() == "sink")
                    })
                    && let Some(value) = self.gcx.call_arg(expr, 0)
                {
                    let values = state.expr(self.gcx, value, false);
                    let origin = values.common_origin();
                    let proven = values.all(|value| *value.property());
                    let snippet = self.gcx.sess.source_map().span_to_snippet(value.span).unwrap();
                    self.observed.push((snippet.clone(), proven));
                    self.observed_origins.push((snippet, origin));
                }
                if cx.reports_enabled()
                    && cx.call_info(expr).and_then(|info| info.function()).is_some_and(|id| {
                        self.gcx
                            .hir
                            .function(id)
                            .name
                            .is_some_and(|name| name.name.as_str() == "sinkHolder")
                    })
                    && let Some(value) = self.gcx.call_arg(expr, 0)
                    && let Some(values) = state.values_read_from_expr(self.gcx, value)
                {
                    self.aggregate_observed.push((
                        values.any(|value| *value.property()),
                        values.any(|value| !*value.property()),
                    ));
                }
            }

            fn apply_modifier_entry_effect(
                &mut self,
                cx: EffectiveBodyCx<'hir>,
                modifier: &'hir Modifier<'hir>,
                callee: FunctionId,
                state: &mut Self::Domain,
            ) {
                state.bind_modifier(cx, modifier, callee, false);
                let parameter = self.gcx.hir.function(callee).parameters[0];
                self.modifier_entries
                    .push(state.variable(parameter).is_proven(|value| *value.property()));
            }

            fn apply_modifier_return_effect(
                &mut self,
                cx: EffectiveBodyCx<'hir>,
                _modifier: &'hir Modifier<'hir>,
                callee: FunctionId,
                caller: &Self::Domain,
                state: &mut Self::Domain,
            ) {
                state.return_from_modifier(cx, callee, caller);
            }

            fn apply_call_entry_effect(
                &mut self,
                cx: EffectiveBodyCx<'hir>,
                call: &'hir Expr<'hir>,
                callee: FunctionId,
                state: &mut Self::Domain,
            ) {
                state.bind_call(cx, call, callee, false);
            }

            fn apply_call_return_effect(
                &mut self,
                cx: EffectiveBodyCx<'hir>,
                call: &'hir Expr<'hir>,
                callee: FunctionId,
                caller: &Self::Domain,
                state: &mut Self::Domain,
            ) {
                state.return_from_call(cx, call, callee, caller);
            }

            fn apply_try_clause_entry_effect(
                &mut self,
                _cx: EffectiveBodyCx<'hir>,
                try_expr: &'hir Expr<'hir>,
                clause: &'hir TryCatchClause<'hir>,
                is_success: bool,
                state: &mut Self::Domain,
            ) {
                state.bind_try_clause(try_expr, clause, is_success, false);
            }

            fn apply_statement_effect(
                &mut self,
                cx: EffectiveBodyCx<'hir>,
                statement: &'hir Stmt<'hir>,
                state: &mut Self::Domain,
            ) {
                state.apply_statement(cx, statement, false);
                if let StmtKind::DeclSingle(variable) = statement.kind
                    && self
                        .gcx
                        .hir
                        .variable(variable)
                        .name
                        .is_some_and(|name| name.name.as_str() == "captured")
                {
                    self.captured_declarations
                        .push(state.variable(variable).is_proven(|value| *value.property()));
                }
            }

            fn internal_call_mode(
                &mut self,
                _cx: EffectiveBodyCx<'hir>,
                _call: &'hir Expr<'hir>,
                _callee: FunctionId,
                _state: &Self::Domain,
            ) -> InternalCallMode {
                InternalCallMode::AnalyzeWithoutReports
            }

            fn apply_indirect_internal_call_effect(
                &mut self,
                _cx: EffectiveBodyCx<'hir>,
                _call: &'hir Expr<'hir>,
                state: &mut Self::Domain,
            ) {
                state.forget_values();
            }
        }

        let sess = Session::builder().opts(CompileOpts::default()).with_test_emitter().build();
        let mut compiler = Compiler::new(sess);
        compiler.enter_mut(|compiler| {
            let mut parser = compiler.parse();
            let file = compiler
                .sess()
                .source_map()
                .new_source_file(PathBuf::from("value_flow.sol"), SOURCE)
                .unwrap();
            parser.add_file(file);
            parser.parse();
            assert_eq!(compiler.lower_asts(), Ok(ControlFlow::Continue(())));
            assert_eq!(compiler.analysis(), Ok(ControlFlow::Continue(())));
        });

        compiler.enter(|compiler| {
            let gcx = compiler.gcx();
            let entry = gcx
                .hir
                .function_ids()
                .find(|&id| gcx.item_canonical_name(id).to_string() == "C.entry")
                .unwrap();
            let trusted = gcx
                .hir
                .variable_ids()
                .find(|&id| {
                    gcx.hir.variable(id).name.is_some_and(|name| name.name.as_str() == "TRUSTED")
                })
                .unwrap();
            let mut initial = ValueFlowState::new(false);
            initial.seed(trusted, true);
            let mut analysis = Analysis {
                gcx,
                observed: Vec::new(),
                modifier_entries: Vec::new(),
                captured_declarations: Vec::new(),
                aggregate_observed: Vec::new(),
                observed_origins: Vec::new(),
            };
            let _ = analyze_effective_body_flow(gcx, entry, initial, &mut analysis);

            assert_eq!(
                analysis.observed.iter().map(|(_, proven)| *proven).collect::<Vec<_>>(),
                [
                    true, false, true, false, true, false, false, false, false, false, false,
                    false, false, false, true, false, true, true, false,
                ],
                "{:?}",
                analysis.observed,
            );
            assert_eq!(analysis.modifier_entries, [false, true]);
            assert_eq!(analysis.captured_declarations, [false, true]);
            assert_eq!(analysis.aggregate_observed, [(true, true), (true, true)]);
            assert_eq!(
                &analysis.observed[4..10],
                [
                    ("local.first".to_owned(), true),
                    ("local.second".to_owned(), false),
                    ("local.first".to_owned(), false),
                    ("first.first".to_owned(), false),
                    ("second.first".to_owned(), false),
                    ("items[0].first".to_owned(), false),
                ]
            );
            assert_eq!(
                &analysis.observed[10..14],
                [
                    ("input.first".to_owned(), false),
                    ("input.first".to_owned(), false),
                    ("input.first".to_owned(), false),
                    ("freshB.first".to_owned(), false),
                ]
            );
            assert_eq!(
                &analysis.observed[14..],
                [
                    ("tupleFirst".to_owned(), true),
                    ("tupleSecond".to_owned(), false),
                    ("omitted".to_owned(), true),
                    ("captured".to_owned(), true),
                    ("captured".to_owned(), false),
                ]
            );
            assert_eq!(
                &analysis.observed[17..],
                [("captured".to_owned(), true), ("captured".to_owned(), false)]
            );

            let modifier_only = gcx
                .hir
                .function_ids()
                .find(|&id| gcx.item_canonical_name(id).to_string() == "C.modifierOnly")
                .unwrap();
            let mut initial = ValueFlowState::new(false);
            initial.seed(trusted, true);
            let mut analysis = Analysis {
                gcx,
                observed: Vec::new(),
                modifier_entries: Vec::new(),
                captured_declarations: Vec::new(),
                aggregate_observed: Vec::new(),
                observed_origins: Vec::new(),
            };
            let _ = analyze_effective_body_flow(gcx, modifier_only, initial, &mut analysis);
            assert_eq!(
                analysis.observed,
                [("captured".to_owned(), true), ("captured".to_owned(), false)]
            );
            assert_eq!(analysis.modifier_entries, [false, true]);
            assert_eq!(analysis.captured_declarations, [false, true]);

            let rebind_alias = gcx
                .hir
                .function_ids()
                .find(|&id| gcx.item_canonical_name(id).to_string() == "C.rebindAlias")
                .unwrap();
            let mut initial = ValueFlowState::new(false);
            initial.seed(trusted, true);
            let mut analysis = Analysis {
                gcx,
                observed: Vec::new(),
                modifier_entries: Vec::new(),
                captured_declarations: Vec::new(),
                aggregate_observed: Vec::new(),
                observed_origins: Vec::new(),
            };
            let _ = analyze_effective_body_flow(gcx, rebind_alias, initial, &mut analysis);
            assert_eq!(analysis.observed, [("alias_.first".to_owned(), true)]);

            let rebind_projected_alias = gcx
                .hir
                .function_ids()
                .find(|&id| gcx.item_canonical_name(id).to_string() == "C.rebindProjectedAlias")
                .unwrap();
            let mut initial = ValueFlowState::new(false);
            initial.seed(trusted, true);
            let mut analysis = Analysis {
                gcx,
                observed: Vec::new(),
                modifier_entries: Vec::new(),
                captured_declarations: Vec::new(),
                aggregate_observed: Vec::new(),
                observed_origins: Vec::new(),
            };
            let _ =
                analyze_effective_body_flow(gcx, rebind_projected_alias, initial, &mut analysis);
            assert_eq!(analysis.observed, [("alias_.first".to_owned(), true)]);

            let replacement_generation = gcx
                .hir
                .function_ids()
                .find(|&id| gcx.item_canonical_name(id).to_string() == "C.replacementGeneration")
                .unwrap();
            let mut analysis = Analysis {
                gcx,
                observed: Vec::new(),
                modifier_entries: Vec::new(),
                captured_declarations: Vec::new(),
                aggregate_observed: Vec::new(),
                observed_origins: Vec::new(),
            };
            let _ = analyze_effective_body_flow(
                gcx,
                replacement_generation,
                ValueFlowState::new(false),
                &mut analysis,
            );
            assert!(
                matches!(
                    analysis.observed_origins.as_slice(),
                    [
                        (
                            _,
                            Some(ValueOrigin::Place {
                                generation: ValueGeneration::Initial(_),
                                ..
                            })
                        ),
                        (
                            _,
                            Some(ValueOrigin::Place {
                                generation: ValueGeneration::CallResult(..),
                                ..
                            })
                        ),
                    ]
                ),
                "{:?}",
                analysis.observed_origins
            );
            assert_ne!(
                analysis.observed_origins[0].1, analysis.observed_origins[1].1,
                "replacement aggregate generations must not alias",
            );
        });
    }
}
