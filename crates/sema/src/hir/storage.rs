//! Source-level storage provenance for HIR analyses.

use super::{
    CallResults, DataLocation, EffectiveBodyCx, Expr, ExprId, ExprKind, ExprUse, JoinSemiLattice,
    Stmt, StmtKind, VariableId,
};
use crate::{builtins::Builtin, ty::Gcx};
use solar_data_structures::{map::FxHashMap, smallvec::SmallVec};

/// A deterministic may-set of state-variable roots.
///
/// A root identifies the declaration whose storage is reached, not an exact EVM slot. Keeping the
/// abstraction at source level makes it useful to lints while remaining finite across loops.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StorageRoots {
    roots: SmallVec<[VariableId; 2]>,
    unknown: bool,
}

/// Storage provenance carried by one call output.
///
/// Solidity storage-reference identity and numeric Yul slot derivation are independent channels:
/// a helper can return either a `T storage` reference or an integer derived from `value.slot`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageProvenance {
    references: StorageRoots,
    slots: StorageRoots,
}

impl StorageProvenance {
    /// Creates provenance with independent reference and numeric-slot roots.
    pub fn new(references: StorageRoots, slots: StorageRoots) -> Self {
        Self { references, slots }
    }

    /// Creates provenance which may refer to any storage root through either channel.
    pub fn unknown() -> Self {
        Self::new(StorageRoots::unknown(), StorageRoots::unknown())
    }

    /// Returns source storage-reference roots.
    pub fn reference_roots(&self) -> &StorageRoots {
        &self.references
    }

    /// Returns numeric storage-slot roots.
    pub fn slot_roots(&self) -> &StorageRoots {
        &self.slots
    }
}

impl JoinSemiLattice for StorageProvenance {
    fn join(&mut self, other: &Self) -> bool {
        let mut changed = self.references.union(&other.references);
        changed |= self.slots.union(&other.slots);
        changed
    }
}

impl StorageRoots {
    /// Creates an empty root set.
    pub fn new() -> Self {
        Self { roots: SmallVec::new(), unknown: false }
    }

    /// Creates a root set which may reach any storage location.
    pub fn unknown() -> Self {
        Self { roots: SmallVec::new(), unknown: true }
    }

    /// Creates a root set containing one state variable.
    pub fn singleton(variable: VariableId) -> Self {
        let mut roots = Self::new();
        roots.insert(variable);
        roots
    }

    /// Returns the known roots in declaration order.
    ///
    /// This does not account for [`StorageRoots::may_be_unknown`]. Use
    /// [`StorageRoots::known_roots`] or [`StorageRoots::may_contain`] when unknown provenance must
    /// remain conservative.
    pub fn iter_known(&self) -> impl ExactSizeIterator<Item = VariableId> + '_ {
        self.roots.iter().copied()
    }

    /// Returns all possible roots when provenance is fully known.
    pub fn known_roots(&self) -> Option<&[VariableId]> {
        (!self.unknown).then_some(self.roots.as_slice())
    }

    /// Returns whether this value is known not to reach storage.
    pub fn is_empty(&self) -> bool {
        self.roots.is_empty() && !self.unknown
    }

    /// Returns whether storage outside the known roots may be reached.
    pub fn may_be_unknown(&self) -> bool {
        self.unknown
    }

    /// Returns whether the set contains `variable`.
    pub fn contains(&self, variable: VariableId) -> bool {
        self.roots.binary_search(&variable).is_ok()
    }

    /// Returns whether `variable` may be reached.
    pub fn may_contain(&self, variable: VariableId) -> bool {
        self.unknown || self.contains(variable)
    }

    /// Inserts one root and returns whether the set changed.
    pub fn insert(&mut self, variable: VariableId) -> bool {
        match self.roots.binary_search(&variable) {
            Ok(_) => false,
            Err(index) => {
                self.roots.insert(index, variable);
                true
            }
        }
    }

    /// Unions `other` into this set and returns whether it changed.
    pub fn union(&mut self, other: &Self) -> bool {
        let mut changed = !self.unknown && other.unknown;
        self.unknown |= other.unknown;
        for variable in other.iter_known() {
            changed |= self.insert(variable);
        }
        changed
    }
}

impl FromIterator<VariableId> for StorageRoots {
    fn from_iter<T: IntoIterator<Item = VariableId>>(iter: T) -> Self {
        let mut roots = Self::new();
        roots.extend(iter);
        roots
    }
}

impl Extend<VariableId> for StorageRoots {
    fn extend<T: IntoIterator<Item = VariableId>>(&mut self, iter: T) {
        for variable in iter {
            self.insert(variable);
        }
    }
}

impl JoinSemiLattice for StorageRoots {
    fn join(&mut self, other: &Self) -> bool {
        self.union(other)
    }
}

/// May-alias provenance for source storage references, Yul slot values, and call results.
///
/// This is the source-level counterpart of rustc's place provenance domains. Most consumers use
/// [`super::StorageFlowAdapter`] to own standard transfer and embed this in their analysis domain;
/// the methods remain public for analyses which need to compose transfer manually.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StorageAliasState {
    references: FxHashMap<VariableId, StorageRoots>,
    slots: FxHashMap<VariableId, StorageRoots>,
    call_results: CallResults<StorageProvenance>,
}

impl StorageAliasState {
    /// Forgets all known reference, slot, and call-result provenance.
    ///
    /// Subsequent storage-reference and slot lookups conservatively produce unknown roots.
    pub fn forget(&mut self) {
        self.references.clear();
        self.slots.clear();
        self.call_results.clear_all();
    }

    /// Returns the state-variable roots currently reached by `variable`.
    pub fn reference_roots<'hir>(&self, gcx: Gcx<'hir>, variable: VariableId) -> StorageRoots {
        self.variable_roots(gcx, Some(variable))
    }

    /// Returns the state-variable roots which may contribute to a local Yul slot value.
    pub fn slot_value_roots(&self, variable: VariableId) -> StorageRoots {
        self.slots.get(&variable).cloned().unwrap_or_else(StorageRoots::unknown)
    }

    /// Returns the state-variable roots reached by a source place or storage-reference value.
    pub fn roots<'hir>(&self, gcx: Gcx<'hir>, expr: &'hir Expr<'hir>) -> StorageRoots {
        let expr = expr.peel_parens();
        match &expr.kind {
            ExprKind::Ident(_) => self.variable_roots(gcx, storage_variable(gcx, expr)),
            ExprKind::Index(base, _)
            | ExprKind::Slice(base, ..)
            | ExprKind::Member(base, _)
            | ExprKind::YulMember(base, _)
            | ExprKind::Payable(base)
            | ExprKind::Unary(_, base)
            | ExprKind::Delete(base) => self.roots(gcx, base),
            ExprKind::Assign(_, _, value) => self.roots(gcx, value),
            ExprKind::Tuple(exprs) => {
                let mut roots = StorageRoots::new();
                for expr in exprs.iter().copied().flatten() {
                    roots.union(&self.roots(gcx, expr));
                }
                roots
            }
            ExprKind::Ternary(_, then_expr, else_expr) => {
                let mut roots = self.roots(gcx, then_expr);
                roots.union(&self.roots(gcx, else_expr));
                roots
            }
            ExprKind::Call(..) => self.flattened_call_results(gcx, expr),
            _ => StorageRoots::new(),
        }
    }

    /// Returns the state roots read when `expr` is evaluated in `use_` context.
    ///
    /// Child expressions are reported by their own flow callbacks. Place evaluation and numeric
    /// `.slot`/`.offset` provenance do not themselves read persistent state.
    pub fn read_roots<'hir>(
        &self,
        gcx: Gcx<'hir>,
        expr: &'hir Expr<'hir>,
        use_: &ExprUse,
    ) -> StorageRoots {
        let expr = expr.peel_parens();
        if use_ == &ExprUse::Place
            || matches!(expr.kind, ExprKind::Delete(_))
            || matches!(
                &expr.kind,
                ExprKind::YulMember(_, member) if matches!(member.as_str(), "slot" | "offset")
            )
        {
            StorageRoots::new()
        } else {
            self.roots(gcx, expr)
        }
    }

    /// Returns the state roots written through source assignment or mutation target `expr`.
    pub fn write_roots<'hir>(&self, gcx: Gcx<'hir>, expr: &'hir Expr<'hir>) -> StorageRoots {
        self.roots(gcx, expr)
    }

    /// Returns state-variable roots which may contribute to a Yul storage-slot value.
    pub fn slot_roots<'hir>(&self, gcx: Gcx<'hir>, expr: &'hir Expr<'hir>) -> StorageRoots {
        let expr = expr.peel_parens();
        if matches!(expr.kind, ExprKind::Call(..))
            && let Some(results) = self.call_results.outputs(expr.id)
        {
            return flatten_slot_roots(results);
        }

        match &expr.kind {
            ExprKind::Ident(_) => storage_variable(gcx, expr)
                .and_then(|variable| self.slots.get(&variable).cloned())
                .unwrap_or_else(StorageRoots::unknown),
            ExprKind::YulMember(base, member) if member.as_str() == "slot" => self.roots(gcx, base),
            ExprKind::Array(exprs) => self.slot_roots_many(gcx, exprs.iter()),
            ExprKind::Assign(lhs, _, rhs) | ExprKind::Binary(lhs, _, rhs) => {
                self.slot_roots_many(gcx, [*lhs, *rhs])
            }
            ExprKind::Call(callee, args, options) => {
                let mut roots = self.slot_roots(gcx, callee);
                if let Some(options) = options {
                    for option in options.args {
                        roots.union(&self.slot_roots(gcx, &option.value));
                    }
                }
                for arg in args.exprs() {
                    roots.union(&self.slot_roots(gcx, arg));
                }
                roots
            }
            ExprKind::Index(base, index) => {
                let mut roots = self.slot_roots(gcx, base);
                if let Some(index) = index {
                    roots.union(&self.slot_roots(gcx, index));
                }
                roots
            }
            ExprKind::Slice(base, start, end) => {
                let mut roots = self.slot_roots(gcx, base);
                if let Some(start) = start {
                    roots.union(&self.slot_roots(gcx, start));
                }
                if let Some(end) = end {
                    roots.union(&self.slot_roots(gcx, end));
                }
                roots
            }
            ExprKind::Member(base, _)
            | ExprKind::YulMember(base, _)
            | ExprKind::Payable(base)
            | ExprKind::Unary(_, base)
            | ExprKind::Delete(base) => self.slot_roots(gcx, base),
            ExprKind::Ternary(condition, then_expr, else_expr) => {
                self.slot_roots_many(gcx, [*condition, *then_expr, *else_expr])
            }
            ExprKind::Tuple(exprs) => self.slot_roots_many(gcx, exprs.iter().copied().flatten()),
            ExprKind::New(_)
            | ExprKind::TypeCall(_)
            | ExprKind::Type(_)
            | ExprKind::Lit(_)
            | ExprKind::Err(_) => StorageRoots::new(),
        }
    }

    /// Returns roots touched when `expr` is used as an EVM storage slot.
    ///
    /// A rootless slot expression is unknown at the access boundary: literals and locally
    /// computed slots can still address any contract storage. [`StorageAliasState::slot_roots`]
    /// intentionally keeps rootless constant offsets empty while composing expressions such as
    /// `data.slot + 1`.
    pub fn storage_access_roots<'hir>(
        &self,
        gcx: Gcx<'hir>,
        expr: &'hir Expr<'hir>,
    ) -> StorageRoots {
        let roots = self.slot_roots(gcx, expr);
        if roots.is_empty() { StorageRoots::unknown() } else { roots }
    }

    /// Binds storage and slot provenance for an applied modifier's parameters.
    pub fn bind_modifier<'hir>(
        &mut self,
        cx: EffectiveBodyCx<'hir>,
        modifier: &'hir super::Modifier<'hir>,
        callee: super::FunctionId,
    ) {
        let function = cx.hir().function(callee);
        let bindings: SmallVec<[(VariableId, StorageRoots, StorageRoots); 4]> = function
            .parameters
            .iter()
            .enumerate()
            .map(|(index, &parameter)| {
                let argument = cx.gcx().modifier_arg(modifier, index);
                let roots = argument
                    .map_or_else(StorageRoots::new, |argument| self.roots(cx.gcx(), argument));
                let slot_roots = argument
                    .map_or_else(StorageRoots::new, |argument| self.slot_roots(cx.gcx(), argument));
                (parameter, roots, slot_roots)
            })
            .collect();
        for (parameter, roots, slot_roots) in bindings {
            self.set_reference(cx.gcx(), parameter, roots);
            self.set_slot(parameter, slot_roots);
        }
        self.clear_returns(cx.gcx(), function.returns);
    }

    /// Binds storage and slot provenance for an internal call's declared parameters.
    pub fn bind_call<'hir>(
        &mut self,
        cx: EffectiveBodyCx<'hir>,
        call: &'hir Expr<'hir>,
        callee: super::FunctionId,
    ) {
        let function = cx.hir().function(callee);
        let bindings: SmallVec<[(VariableId, StorageRoots, StorageRoots); 4]> = function
            .parameters
            .iter()
            .enumerate()
            .map(|(index, &parameter)| {
                let argument = cx.gcx().call_arg_for_param(call, index);
                let roots = argument
                    .map_or_else(StorageRoots::new, |argument| self.roots(cx.gcx(), argument));
                let slot_roots = argument
                    .map_or_else(StorageRoots::new, |argument| self.slot_roots(cx.gcx(), argument));
                (parameter, roots, slot_roots)
            })
            .collect();
        for (parameter, roots, slot_roots) in bindings {
            self.set_reference(cx.gcx(), parameter, roots);
            self.set_slot(parameter, slot_roots);
        }
        self.clear_returns(cx.gcx(), function.returns);
    }

    /// Restores the caller activation and maps a normal callee's outputs to its call expression.
    pub fn return_from_call<'hir>(
        &mut self,
        cx: EffectiveBodyCx<'hir>,
        call: &'hir Expr<'hir>,
        callee: super::FunctionId,
        caller: &Self,
    ) {
        let function = cx.hir().function(callee);
        let results: SmallVec<[StorageProvenance; 2]> = function
            .returns
            .iter()
            .map(|&variable| {
                let slots = self.slot_value_roots(variable);
                let references = if function.is_yul {
                    StorageRoots::new()
                } else {
                    self.variable_roots(cx.gcx(), Some(variable))
                };
                StorageProvenance::new(references, slots)
            })
            .collect();
        self.restore_activation(cx, callee, caller);
        self.set_call_results(call.id, results);
    }

    /// Restores storage provenance owned by one completed modifier activation.
    pub fn return_from_modifier(
        &mut self,
        cx: EffectiveBodyCx<'_>,
        callee: super::FunctionId,
        caller: &Self,
    ) {
        self.restore_activation(cx, callee, caller);
    }

    /// Applies alias-producing effects of an expression after its operands have been evaluated.
    pub fn apply_expr_effect<'hir>(&mut self, cx: EffectiveBodyCx<'hir>, expr: &'hir Expr<'hir>) {
        match &expr.kind {
            ExprKind::Assign(target, operator, value) => {
                if operator.is_none() {
                    self.assign(cx.gcx(), target, value);
                } else if let Some(variable) = unprojected_variable(cx.gcx(), target) {
                    let roots = self.slot_roots(cx.gcx(), expr);
                    self.set_slot(variable, roots);
                }
            }
            ExprKind::Call(..)
                if cx
                    .call_info(expr)
                    .is_some_and(|info| matches!(info.builtin(), Some(Builtin::ArrayPush0)))
                    && let Some(receiver) = cx.gcx().call_receiver(expr) =>
            {
                let roots = self.roots(cx.gcx(), receiver);
                self.set_call_results(expr.id, [StorageProvenance::new(roots.clone(), roots)]);
            }
            _ => {}
        }
    }

    /// Applies declaration and explicit-return alias effects after statement operands are run.
    pub fn apply_statement_effect<'hir>(
        &mut self,
        cx: EffectiveBodyCx<'hir>,
        statement: &'hir Stmt<'hir>,
    ) {
        match statement.kind {
            StmtKind::DeclSingle(variable) => {
                let initializer = cx.hir().variable(variable).initializer;
                let roots =
                    initializer.map_or_else(StorageRoots::new, |expr| self.roots(cx.gcx(), expr));
                let slot_roots = initializer
                    .map_or_else(StorageRoots::new, |expr| self.slot_roots(cx.gcx(), expr));
                self.set_reference(cx.gcx(), variable, roots);
                self.set_slot(variable, slot_roots);
            }
            StmtKind::DeclMulti(variables, initializer) => {
                self.assign_variables(cx.gcx(), variables, initializer);
            }
            StmtKind::Return(Some(value)) => {
                self.assign_variables(
                    cx.gcx(),
                    &cx.hir()
                        .function(cx.function())
                        .returns
                        .iter()
                        .copied()
                        .map(Some)
                        .collect::<SmallVec<[_; 4]>>(),
                    value,
                );
            }
            _ => {}
        }
    }

    /// Sets the roots produced by `call`, indexed by return position.
    pub fn set_call_results(
        &mut self,
        call: ExprId,
        results: impl IntoIterator<Item = StorageProvenance>,
    ) {
        self.call_results.set_outputs(call, results);
    }

    /// Returns one output of a summarized call.
    pub fn call_result(&self, call: ExprId, index: usize) -> Option<&StorageProvenance> {
        self.call_results.output(call, index)
    }

    fn variable_roots<'hir>(&self, gcx: Gcx<'hir>, variable: Option<VariableId>) -> StorageRoots {
        let Some(variable) = variable else { return StorageRoots::new() };
        let declaration = gcx.hir.variable(variable);
        if declaration.kind.is_state() {
            StorageRoots::singleton(variable)
        } else if matches!(
            declaration.data_location,
            Some(DataLocation::Storage | DataLocation::Transient)
        ) {
            self.references.get(&variable).cloned().unwrap_or_else(StorageRoots::unknown)
        } else {
            StorageRoots::new()
        }
    }

    fn flattened_call_results<'hir>(&self, gcx: Gcx<'hir>, call: &'hir Expr<'hir>) -> StorageRoots {
        self.call_results.outputs(call.id).map_or_else(
            || {
                if gcx.type_of_expr(call.id).is_some_and(|ty| {
                    ty.is_ref_at(DataLocation::Storage) || ty.is_ref_at(DataLocation::Transient)
                }) {
                    StorageRoots::unknown()
                } else {
                    StorageRoots::new()
                }
            },
            flatten_reference_roots,
        )
    }

    fn slot_roots_many<'hir>(
        &self,
        gcx: Gcx<'hir>,
        exprs: impl IntoIterator<Item = &'hir Expr<'hir>>,
    ) -> StorageRoots {
        let mut roots = StorageRoots::new();
        for expr in exprs {
            roots.union(&self.slot_roots(gcx, expr));
        }
        roots
    }

    fn set_reference<'hir>(&mut self, gcx: Gcx<'hir>, variable: VariableId, roots: StorageRoots) {
        let declaration = gcx.hir.variable(variable);
        if !declaration.kind.is_state()
            && matches!(
                declaration.data_location,
                Some(DataLocation::Storage | DataLocation::Transient)
            )
        {
            self.references.insert(variable, roots);
        } else {
            self.references.remove(&variable);
        }
    }

    fn set_slot(&mut self, variable: VariableId, roots: StorageRoots) {
        self.slots.insert(variable, roots);
    }

    fn restore_activation<'hir>(
        &mut self,
        cx: EffectiveBodyCx<'hir>,
        callee: super::FunctionId,
        caller: &Self,
    ) {
        for variable in cx.activation_variables(callee) {
            restore_entry(&mut self.references, &caller.references, variable);
            restore_entry(&mut self.slots, &caller.slots, variable);
        }
    }

    fn clear_returns<'hir>(&mut self, gcx: Gcx<'hir>, returns: &[VariableId]) {
        for &variable in returns {
            self.set_reference(gcx, variable, StorageRoots::new());
            self.set_slot(variable, StorageRoots::new());
        }
    }

    fn assign<'hir>(&mut self, gcx: Gcx<'hir>, target: &'hir Expr<'hir>, value: &'hir Expr<'hir>) {
        if let ExprKind::YulMember(base, member) = &target.peel_parens().kind
            && member.as_str() == "slot"
            && let Some(variable) = storage_variable(gcx, base)
        {
            let roots = self.slot_roots(gcx, value);
            self.set_reference(gcx, variable, roots);
            return;
        }

        if let ExprKind::Tuple(targets) = &target.peel_parens().kind {
            let bindings: SmallVec<[(VariableId, StorageRoots, StorageRoots); 4]> = targets
                .iter()
                .copied()
                .enumerate()
                .filter_map(|(index, target)| {
                    self.output_binding(gcx, target?, value, index, targets.len())
                })
                .collect();
            self.apply_bindings(gcx, bindings);
        } else {
            if let Some(binding) = self.output_binding(gcx, target, value, 0, 1) {
                self.apply_bindings(gcx, [binding]);
            }
        }
    }

    fn assign_variables<'hir>(
        &mut self,
        gcx: Gcx<'hir>,
        variables: &[Option<VariableId>],
        value: &'hir Expr<'hir>,
    ) {
        let bindings: SmallVec<[(VariableId, StorageRoots, StorageRoots); 4]> = variables
            .iter()
            .copied()
            .enumerate()
            .filter_map(|(index, variable)| {
                let variable = variable?;
                let roots = self.roots_for_output(gcx, value, index, variables.len());
                let slot_roots = self.slot_roots_for_output(gcx, value, index, variables.len());
                Some((variable, roots, slot_roots))
            })
            .collect();
        self.apply_bindings(gcx, bindings);
    }

    fn output_binding<'hir>(
        &self,
        gcx: Gcx<'hir>,
        target: &'hir Expr<'hir>,
        value: &'hir Expr<'hir>,
        index: usize,
        outputs: usize,
    ) -> Option<(VariableId, StorageRoots, StorageRoots)> {
        let variable = unprojected_variable(gcx, target)?;
        let roots = self.roots_for_output(gcx, value, index, outputs);
        let slot_roots = self.slot_roots_for_output(gcx, value, index, outputs);
        Some((variable, roots, slot_roots))
    }

    fn apply_bindings(
        &mut self,
        gcx: Gcx<'_>,
        bindings: impl IntoIterator<Item = (VariableId, StorageRoots, StorageRoots)>,
    ) {
        for (variable, roots, slot_roots) in bindings {
            self.set_reference(gcx, variable, roots);
            self.set_slot(variable, slot_roots);
        }
    }

    fn roots_for_output<'hir>(
        &self,
        gcx: Gcx<'hir>,
        expr: &'hir Expr<'hir>,
        index: usize,
        outputs: usize,
    ) -> StorageRoots {
        if let ExprKind::Tuple(values) = &expr.peel_parens().kind
            && outputs > 1
        {
            return values
                .get(index)
                .copied()
                .flatten()
                .map_or_else(StorageRoots::new, |value| self.roots(gcx, value));
        }
        if matches!(expr.peel_parens().kind, ExprKind::Call(..)) {
            return self
                .call_result(expr.id, index)
                .map(|result| result.references.clone())
                .unwrap_or_else(StorageRoots::unknown);
        }
        if outputs == 1 && index == 0 { self.roots(gcx, expr) } else { StorageRoots::new() }
    }

    fn slot_roots_for_output<'hir>(
        &self,
        gcx: Gcx<'hir>,
        expr: &'hir Expr<'hir>,
        index: usize,
        outputs: usize,
    ) -> StorageRoots {
        if let ExprKind::Tuple(values) = &expr.peel_parens().kind
            && outputs > 1
        {
            return values
                .get(index)
                .copied()
                .flatten()
                .map_or_else(StorageRoots::new, |value| self.slot_roots(gcx, value));
        }
        if matches!(expr.peel_parens().kind, ExprKind::Call(..)) {
            return self
                .call_result(expr.id, index)
                .map(|result| result.slots.clone())
                .unwrap_or_else(StorageRoots::unknown);
        }
        if outputs == 1 && index == 0 { self.slot_roots(gcx, expr) } else { StorageRoots::new() }
    }
}

impl JoinSemiLattice for StorageAliasState {
    fn join(&mut self, other: &Self) -> bool {
        let mut changed = join_root_map(&mut self.references, &other.references);
        changed |= join_root_map(&mut self.slots, &other.slots);
        changed |= self.call_results.join(&other.call_results);
        changed
    }
}

fn flatten_reference_roots(roots: &[StorageProvenance]) -> StorageRoots {
    let mut flattened = StorageRoots::new();
    for roots in roots {
        flattened.union(&roots.references);
    }
    flattened
}

fn flatten_slot_roots(roots: &[StorageProvenance]) -> StorageRoots {
    let mut flattened = StorageRoots::new();
    for roots in roots {
        flattened.union(&roots.slots);
    }
    flattened
}

fn join_root_map(
    map: &mut FxHashMap<VariableId, StorageRoots>,
    other: &FxHashMap<VariableId, StorageRoots>,
) -> bool {
    let variables: SmallVec<[VariableId; 8]> = map.keys().chain(other.keys()).copied().collect();
    let mut changed = false;
    for variable in variables {
        let mut roots = map.get(&variable).cloned().unwrap_or_else(StorageRoots::unknown);
        roots.union(&other.get(&variable).cloned().unwrap_or_else(StorageRoots::unknown));
        if map.get(&variable) != Some(&roots) {
            map.insert(variable, roots);
            changed = true;
        }
    }
    changed
}

fn restore_entry(
    map: &mut FxHashMap<VariableId, StorageRoots>,
    caller: &FxHashMap<VariableId, StorageRoots>,
    variable: VariableId,
) {
    if let Some(roots) = caller.get(&variable) {
        map.insert(variable, roots.clone());
    } else {
        map.remove(&variable);
    }
}

/// Returns the variable named by a storage expression.
///
/// Local bindings normally have one unambiguous HIR resolution. Public state variables can also
/// resolve to their generated getter, so place lowering is the fallback for those expressions.
fn storage_variable<'hir>(gcx: Gcx<'hir>, expr: &'hir Expr<'hir>) -> Option<VariableId> {
    expr.as_variable().or_else(|| gcx.expr_root_variable(expr))
}

/// Returns the root of a whole-variable assignment, excluding projected writes.
fn unprojected_variable<'hir>(gcx: Gcx<'hir>, expr: &'hir Expr<'hir>) -> Option<VariableId> {
    let place = gcx.expr_place(expr)?;
    place.projection().is_empty().then_some(place.local())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Compiler;
    use solar_interface::{Session, config::CompileOpts};
    use std::{ops::ControlFlow, path::PathBuf};

    #[test]
    fn unknown_roots_are_not_observable_as_an_empty_known_set() {
        let variable = VariableId::new(0);
        let roots = StorageRoots::unknown();
        assert!(!roots.is_empty());
        assert!(roots.may_be_unknown());
        assert!(roots.may_contain(variable));
        assert_eq!(roots.known_roots(), None);
    }

    const SOURCE: &str = r#"
struct Data {
    uint256 value;
}

contract C {
    Data data;
    Data other;
    uint128 packed;

    function identity(Data storage input) internal returns (Data storage output) {
        output = input;
    }

    function write() external {
        Data storage alias_ = identity(data);
        alias_.value = 1;
        assembly {
            let slot := data.slot
            sstore(slot, 2)
        }
    }

    function swapAliases() external {
        Data storage left = data;
        Data storage right = other;
        (left, right) = (right, left);
        left.value = 3;
        right.value = 4;
    }

    function slotOf() internal returns (uint256 slot) {
        assembly {
            slot := data.slot
        }
    }

    function writeViaReturnedSlot() external {
        uint256 slot = slotOf();
        assembly {
            sstore(slot, 5)
        }
    }

    function writeRawSlot() external {
        assembly {
            sstore(0, 6)
        }
    }

    function metadataOnly() external view returns (uint256 metadata) {
        assembly {
            metadata := add(packed.slot, packed.offset)
        }
    }

    function deleteOnly() external {
        delete data;
    }
}
"#;

    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    struct Domain {
        storage: StorageAliasState,
    }

    impl JoinSemiLattice for Domain {
        fn join(&mut self, other: &Self) -> bool {
            self.storage.join(&other.storage)
        }
    }

    #[derive(Default)]
    struct StorageWrites {
        roots: Vec<StorageRoots>,
    }

    impl<'hir> super::super::EffectiveFlowAnalysis<'hir> for StorageWrites {
        type Domain = Domain;

        fn operand_order(&self) -> super::super::OperandOrder {
            super::super::OperandOrder::Unspecified
        }

        fn apply_expr_effect(
            &mut self,
            cx: EffectiveBodyCx<'hir>,
            expr: &'hir Expr<'hir>,
            _use_: super::super::ExprUse,
            state: &mut Self::Domain,
        ) {
            match &expr.kind {
                ExprKind::Assign(target, ..) => {
                    let roots = state.storage.roots(cx.gcx(), target);
                    if !roots.is_empty() {
                        self.roots.push(roots);
                    }
                }
                ExprKind::Call(..)
                    if cx.call_info(expr).is_some_and(|info| {
                        info.builtin().is_some_and(Builtin::is_persistent_state_write)
                    }) =>
                {
                    if let Some(slot) = cx.gcx().call_arg(expr, 0) {
                        self.roots.push(state.storage.storage_access_roots(cx.gcx(), slot));
                    }
                }
                _ => {}
            }
            state.storage.apply_expr_effect(cx, expr);
        }

        fn apply_modifier_entry_effect(
            &mut self,
            cx: EffectiveBodyCx<'hir>,
            modifier: &'hir super::super::Modifier<'hir>,
            callee: super::super::FunctionId,
            state: &mut Self::Domain,
        ) {
            state.storage.bind_modifier(cx, modifier, callee);
        }

        fn apply_modifier_return_effect(
            &mut self,
            cx: EffectiveBodyCx<'hir>,
            _modifier: &'hir super::super::Modifier<'hir>,
            callee: super::super::FunctionId,
            caller: &Self::Domain,
            state: &mut Self::Domain,
        ) {
            state.storage.return_from_modifier(cx, callee, &caller.storage);
        }

        fn apply_call_entry_effect(
            &mut self,
            cx: EffectiveBodyCx<'hir>,
            call: &'hir Expr<'hir>,
            callee: super::super::FunctionId,
            state: &mut Self::Domain,
        ) {
            state.storage.bind_call(cx, call, callee);
        }

        fn apply_call_return_effect(
            &mut self,
            cx: EffectiveBodyCx<'hir>,
            call: &'hir Expr<'hir>,
            callee: super::super::FunctionId,
            caller: &Self::Domain,
            state: &mut Self::Domain,
        ) {
            state.storage.return_from_call(cx, call, callee, &caller.storage);
        }

        fn apply_statement_effect(
            &mut self,
            cx: EffectiveBodyCx<'hir>,
            statement: &'hir Stmt<'hir>,
            state: &mut Self::Domain,
        ) {
            state.storage.apply_statement_effect(cx, statement);
        }

        fn apply_indirect_internal_call_effect(
            &mut self,
            _cx: EffectiveBodyCx<'hir>,
            call: &'hir Expr<'hir>,
            state: &mut Self::Domain,
        ) {
            state.storage.set_call_results(call.id, [StorageProvenance::unknown()]);
        }
    }

    #[derive(Default)]
    struct StorageReads {
        roots: Vec<StorageRoots>,
    }

    impl<'hir> super::super::EffectiveFlowAnalysis<'hir> for StorageReads {
        type Domain = Domain;

        fn operand_order(&self) -> super::super::OperandOrder {
            super::super::OperandOrder::Unspecified
        }

        fn apply_expr_effect(
            &mut self,
            cx: EffectiveBodyCx<'hir>,
            expr: &'hir Expr<'hir>,
            use_: super::super::ExprUse,
            state: &mut Self::Domain,
        ) {
            let roots = state.storage.read_roots(cx.gcx(), expr, &use_);
            if !roots.is_empty() {
                self.roots.push(roots);
            }
            state.storage.apply_expr_effect(cx, expr);
        }

        fn apply_statement_effect(
            &mut self,
            cx: EffectiveBodyCx<'hir>,
            statement: &'hir Stmt<'hir>,
            state: &mut Self::Domain,
        ) {
            state.storage.apply_statement_effect(cx, statement);
        }

        fn apply_indirect_internal_call_effect(
            &mut self,
            _cx: EffectiveBodyCx<'hir>,
            _call: &'hir Expr<'hir>,
            state: &mut Self::Domain,
        ) {
            state.storage.forget();
        }
    }

    #[test]
    fn follows_storage_references_call_returns_and_yul_slots() {
        let sess = Session::builder().opts(CompileOpts::default()).with_test_emitter().build();
        let mut compiler = Compiler::new(sess);

        compiler.enter_mut(|compiler| {
            let mut parser = compiler.parse();
            let file = compiler
                .sess()
                .source_map()
                .new_source_file(PathBuf::from("test.sol"), SOURCE)
                .unwrap();
            parser.add_file(file);
            parser.parse();
            assert_eq!(compiler.lower_asts(), Ok(ControlFlow::Continue(())));
            assert_eq!(compiler.analysis(), Ok(ControlFlow::Continue(())));
        });

        compiler.enter(|compiler| {
            let gcx = compiler.gcx();
            let function = gcx
                .hir
                .function_ids()
                .find(|&function| gcx.item_canonical_name(function).to_string() == "C.write")
                .unwrap();
            let data = gcx
                .hir
                .variable_ids()
                .find(|&variable| {
                    let variable = gcx.hir.variable(variable);
                    variable.kind.is_state()
                        && variable.name.is_some_and(|name| name.as_str() == "data")
                })
                .unwrap();
            let mut analysis = StorageWrites::default();
            super::super::analyze_effective_body_flow(
                gcx,
                function,
                Domain::default(),
                &mut analysis,
            );

            assert_eq!(analysis.roots.len(), 2);
            assert!(analysis.roots.iter().all(|roots| roots.iter_known().eq([data])));

            let function = |name: &str| {
                gcx.hir
                    .function_ids()
                    .find(|&function| gcx.item_canonical_name(function).to_string() == name)
                    .unwrap()
            };
            let mut returned_slot = StorageWrites::default();
            super::super::analyze_effective_body_flow(
                gcx,
                function("C.writeViaReturnedSlot"),
                Domain::default(),
                &mut returned_slot,
            );
            assert_eq!(returned_slot.roots.len(), 1);
            assert!(returned_slot.roots[0].iter_known().eq([data]));

            let mut raw_slot = StorageWrites::default();
            super::super::analyze_effective_body_flow(
                gcx,
                function("C.writeRawSlot"),
                Domain::default(),
                &mut raw_slot,
            );
            assert_eq!(raw_slot.roots.len(), 1);
            assert!(raw_slot.roots[0].may_be_unknown());

            for name in ["C.metadataOnly", "C.deleteOnly"] {
                let mut reads = StorageReads::default();
                super::super::analyze_effective_body_flow(
                    gcx,
                    function(name),
                    Domain::default(),
                    &mut reads,
                );
                assert!(reads.roots.is_empty(), "{name} unexpectedly read storage");
            }

            let other = gcx
                .hir
                .variable_ids()
                .find(|&variable| {
                    gcx.hir.variable(variable).kind.is_state()
                        && gcx
                            .hir
                            .variable(variable)
                            .name
                            .is_some_and(|name| name.as_str() == "other")
                })
                .unwrap();
            let swap = gcx
                .hir
                .function_ids()
                .find(|&function| gcx.item_canonical_name(function).to_string() == "C.swapAliases")
                .unwrap();
            let local = |name: &str| {
                gcx.hir
                    .variable_ids()
                    .find(|&variable| {
                        !gcx.hir.variable(variable).kind.is_state()
                            && gcx
                                .hir
                                .variable(variable)
                                .name
                                .is_some_and(|ident| ident.as_str() == name)
                    })
                    .unwrap()
            };
            let mut swap_analysis = StorageWrites::default();
            let result = super::super::analyze_effective_body_flow(
                gcx,
                swap,
                Domain::default(),
                &mut swap_analysis,
            )
            .normal_exit()
            .unwrap();
            assert_eq!(
                result.storage.reference_roots(gcx, local("left")).known_roots(),
                Some([other].as_slice())
            );
            assert_eq!(
                result.storage.reference_roots(gcx, local("right")).known_roots(),
                Some([data].as_slice())
            );
        });
    }
}
