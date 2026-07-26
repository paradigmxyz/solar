//! Call hierarchy indexing.

use crate::{
    proto,
    symbols::{DeclarationSymbol, SymbolId, remap_symbol_id},
};
use lsp_types::{
    CallHierarchyIncomingCall, CallHierarchyItem, CallHierarchyOutgoingCall, Position, Range, Url,
};
use serde::{Deserialize, Serialize};
use solar_interface::{
    Span,
    data_structures::{
        Never,
        index::IndexVec,
        map::{FxHashMap, FxHashSet},
    },
};
use solar_sema::{
    Gcx,
    hir::{self, ItemId, Visit},
    ty::TyKind,
};
use std::{cmp::Ordering, ops::ControlFlow, sync::OnceLock};

const DATA_VERSION: u8 = 1;

#[derive(Debug, Default)]
pub(crate) struct CallHierarchyIndex {
    facts: CallHierarchyFacts,
    query: OnceLock<QueryIndex>,
}

impl Clone for CallHierarchyIndex {
    fn clone(&self) -> Self {
        Self { facts: self.facts.clone(), query: OnceLock::new() }
    }
}

#[derive(Clone, Debug, Default)]
struct CallHierarchyFacts {
    callables: Vec<CallableFact>,
    direct_calls: Vec<DirectCall>,
}

#[derive(Clone, Copy, Debug)]
struct CallableFact {
    symbol: SymbolId,
    body_range: Option<Range>,
}

#[derive(Debug, Default)]
struct QueryIndex {
    items_by_symbol: FxHashMap<SymbolId, CallHierarchyItem>,
    candidate_key_by_symbol: FxHashMap<SymbolId, CallableKey>,
    body_range_by_symbol: FxHashMap<SymbolId, Range>,
    canonical_symbol_by_key: FxHashMap<CallableKey, SymbolId>,
    key_by_symbol: FxHashMap<SymbolId, CallableKey>,
    outgoing_by_key: CallRelations,
    incoming_by_key: CallRelations,
    incomplete_outgoing: FxHashSet<CallableKey>,
    incomplete_incoming: FxHashSet<CallableKey>,
    call_sites_by_uri: FxHashMap<Url, Vec<CallSite>>,
    bodies_by_uri: FxHashMap<Url, Vec<CallableBody>>,
}

type CallRelations = FxHashMap<CallableKey, FxHashMap<CallableKey, Vec<Range>>>;

#[derive(Clone, Copy, Debug)]
struct DirectCall {
    caller: SymbolId,
    callee: SymbolId,
    from_range: Range,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CallSite {
    range: Range,
    callee: CallableKey,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CallableBody {
    range: Range,
    callable: CallableKey,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct CallableKey {
    uri: Url,
    selection_range: Range,
}

impl Ord for CallableKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.uri
            .as_str()
            .cmp(other.uri.as_str())
            .then_with(|| range_key(self.selection_range).cmp(&range_key(other.selection_range)))
    }
}

impl PartialOrd for CallableKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CallHierarchyData {
    version: u8,
    uri: Url,
    selection_range: Range,
}

impl CallHierarchyIndex {
    #[cfg(test)]
    pub(crate) fn is_query_initialized(&self) -> bool {
        self.query.get().is_some()
    }

    pub(crate) fn build(
        gcx: Gcx<'_>,
        item_symbols: &FxHashMap<ItemId, SymbolId>,
        declarations: &IndexVec<SymbolId, DeclarationSymbol>,
    ) -> Self {
        let mut index = Self::default();
        for function_id in gcx.hir.function_ids() {
            let function = gcx.hir.function(function_id);
            if function.is_yul || function.is_getter() {
                continue;
            }
            let Some(&symbol_id) = item_symbols.get(&ItemId::Function(function_id)) else {
                continue;
            };
            let body_range = if function.body.is_some() {
                proto::span_to_location(gcx.sess.source_map(), function.body_span)
                    .map(|location| location.range)
            } else {
                None
            };
            index.facts.callables.push(CallableFact { symbol: symbol_id, body_range });
        }

        if gcx.has_typeck_results() {
            collect_direct_calls(&mut index.facts, gcx, item_symbols, declarations);
        }
        index
    }

    pub(crate) fn extend(&mut self, other: Self, symbol_offset: usize) {
        let Self { facts, query: _ } = other;
        self.facts.callables.extend(facts.callables.into_iter().map(|fact| CallableFact {
            symbol: remap_symbol_id(fact.symbol, symbol_offset),
            body_range: fact.body_range,
        }));
        self.facts.direct_calls.extend(facts.direct_calls.into_iter().map(|call| DirectCall {
            caller: remap_symbol_id(call.caller, symbol_offset),
            callee: remap_symbol_id(call.callee, symbol_offset),
            from_range: call.from_range,
        }));
        self.invalidate_query();
    }

    pub(crate) fn rebuild(&mut self) {
        self.invalidate_query();
    }

    pub(crate) fn prepare(
        &self,
        declarations: &IndexVec<SymbolId, DeclarationSymbol>,
        conflicting_contents: &FxHashSet<Url>,
        uri: &Url,
        position: Position,
        declaration: Option<SymbolId>,
    ) -> Option<Vec<CallHierarchyItem>> {
        self.query(declarations, conflicting_contents).prepare(uri, position, declaration)
    }

    pub(crate) fn incoming(
        &self,
        declarations: &IndexVec<SymbolId, DeclarationSymbol>,
        conflicting_contents: &FxHashSet<Url>,
        item: &CallHierarchyItem,
    ) -> Option<Vec<CallHierarchyIncomingCall>> {
        self.query(declarations, conflicting_contents).incoming(item)
    }

    pub(crate) fn outgoing(
        &self,
        declarations: &IndexVec<SymbolId, DeclarationSymbol>,
        conflicting_contents: &FxHashSet<Url>,
        item: &CallHierarchyItem,
    ) -> Option<Vec<CallHierarchyOutgoingCall>> {
        self.query(declarations, conflicting_contents).outgoing(item)
    }

    fn query(
        &self,
        declarations: &IndexVec<SymbolId, DeclarationSymbol>,
        conflicting_contents: &FxHashSet<Url>,
    ) -> &QueryIndex {
        self.query
            .get_or_init(|| QueryIndex::build(&self.facts, declarations, conflicting_contents))
    }

    fn invalidate_query(&mut self) {
        let _ = self.query.take();
    }
}

impl QueryIndex {
    fn build(
        facts: &CallHierarchyFacts,
        declarations: &IndexVec<SymbolId, DeclarationSymbol>,
        conflicting_contents: &FxHashSet<Url>,
    ) -> Self {
        let mut index = Self::default();
        for fact in &facts.callables {
            let declaration = &declarations[fact.symbol];
            let key = CallableKey {
                uri: declaration.location.uri.clone(),
                selection_range: declaration.name_range,
            };
            let data = CallHierarchyData {
                version: DATA_VERSION,
                uri: key.uri.clone(),
                selection_range: key.selection_range,
            };
            let detail = declaration.parent.map(|parent| declarations[parent].name.clone());
            index.items_by_symbol.insert(
                fact.symbol,
                CallHierarchyItem {
                    name: declaration.name.clone(),
                    kind: declaration.kind,
                    tags: None,
                    detail,
                    uri: key.uri.clone(),
                    range: declaration.location.range,
                    selection_range: key.selection_range,
                    data: Some(
                        serde_json::to_value(data)
                            .expect("call hierarchy data should be serializable"),
                    ),
                },
            );
            index.candidate_key_by_symbol.insert(fact.symbol, key);
            if let Some(range) = fact.body_range {
                index.body_range_by_symbol.insert(fact.symbol, range);
            }
        }
        index.rebuild(&facts.direct_calls, conflicting_contents);
        index
    }

    fn rebuild(&mut self, direct_calls: &[DirectCall], conflicting_contents: &FxHashSet<Url>) {
        let mut outgoing_facts_by_symbol = FxHashMap::<SymbolId, Vec<_>>::default();
        for call in direct_calls {
            let Some(callee) = self.candidate_key_by_symbol.get(&call.callee) else {
                continue;
            };
            outgoing_facts_by_symbol
                .entry(call.caller)
                .or_default()
                .push((callee.clone(), range_key(call.from_range)));
        }
        for facts in outgoing_facts_by_symbol.values_mut() {
            facts.sort_unstable();
            facts.dedup();
        }

        let mut incompatible_keys = FxHashSet::default();
        for (&symbol, key) in &self.candidate_key_by_symbol {
            if conflicting_contents.contains(&key.uri) || incompatible_keys.contains(key) {
                continue;
            }
            if let Some(&existing) = self.canonical_symbol_by_key.get(key) {
                let facts =
                    outgoing_facts_by_symbol.get(&symbol).map(Vec::as_slice).unwrap_or_default();
                let existing_facts =
                    outgoing_facts_by_symbol.get(&existing).map(Vec::as_slice).unwrap_or_default();
                if self.items_by_symbol.get(&symbol) != self.items_by_symbol.get(&existing)
                    || self.body_range_by_symbol.get(&symbol)
                        != self.body_range_by_symbol.get(&existing)
                    || facts != existing_facts
                {
                    self.canonical_symbol_by_key.remove(key);
                    incompatible_keys.insert(key.clone());
                }
            } else {
                self.canonical_symbol_by_key.insert(key.clone(), symbol);
            }
        }
        self.canonical_symbol_by_key.retain(|_, symbol| self.items_by_symbol.contains_key(symbol));
        for (&symbol, key) in &self.candidate_key_by_symbol {
            if self.items_by_symbol.contains_key(&symbol)
                && self.canonical_symbol_by_key.contains_key(key)
            {
                self.key_by_symbol.insert(symbol, key.clone());
            }
        }

        for call in direct_calls {
            let caller = self.key_by_symbol.get(&call.caller);
            let callee = self.key_by_symbol.get(&call.callee);
            let (Some(caller), Some(callee)) = (caller, callee) else {
                if let Some(caller) = caller {
                    self.incomplete_outgoing.insert(caller.clone());
                }
                if let Some(callee) = callee {
                    self.incomplete_incoming.insert(callee.clone());
                }
                continue;
            };
            self.outgoing_by_key
                .entry(caller.clone())
                .or_default()
                .entry(callee.clone())
                .or_default()
                .push(call.from_range);
            self.incoming_by_key
                .entry(callee.clone())
                .or_default()
                .entry(caller.clone())
                .or_default()
                .push(call.from_range);
            self.call_sites_by_uri
                .entry(caller.uri.clone())
                .or_default()
                .push(CallSite { range: call.from_range, callee: callee.clone() });
        }
        normalize_relations(&mut self.outgoing_by_key);
        normalize_relations(&mut self.incoming_by_key);
        for sites in self.call_sites_by_uri.values_mut() {
            sites.sort_by(|a, b| {
                range_key(a.range).cmp(&range_key(b.range)).then_with(|| a.callee.cmp(&b.callee))
            });
            sites.dedup();
        }

        for (key, &symbol) in &self.canonical_symbol_by_key {
            if let Some(&range) = self.body_range_by_symbol.get(&symbol) {
                self.bodies_by_uri
                    .entry(key.uri.clone())
                    .or_default()
                    .push(CallableBody { range, callable: key.clone() });
            }
        }
        for bodies in self.bodies_by_uri.values_mut() {
            bodies.sort_by(|a, b| {
                range_key(a.range)
                    .cmp(&range_key(b.range))
                    .then_with(|| a.callable.cmp(&b.callable))
            });
            bodies.dedup();
        }
    }

    pub(crate) fn prepare(
        &self,
        uri: &Url,
        position: Position,
        declaration: Option<SymbolId>,
    ) -> Option<Vec<CallHierarchyItem>> {
        if let Some(key) = self.call_site_key(uri, position) {
            return Some(vec![self.item(key)?.clone()]);
        }
        if let Some(key) = declaration.and_then(|symbol| self.key_by_symbol.get(&symbol)) {
            return Some(vec![self.item(key)?.clone()]);
        }
        let key = self.enclosing_body_key(uri, position)?;
        Some(vec![self.item(key)?.clone()])
    }

    pub(crate) fn incoming(
        &self,
        item: &CallHierarchyItem,
    ) -> Option<Vec<CallHierarchyIncomingCall>> {
        let key = self.resolve_item(item)?;
        if self.incomplete_incoming.contains(&key) {
            return None;
        }
        let mut callers = self.incoming_by_key.get(&key).into_iter().flatten().collect::<Vec<_>>();
        callers.sort_by_key(|(caller, _)| *caller);
        Some(
            callers
                .into_iter()
                .filter_map(|(caller, ranges)| {
                    Some(CallHierarchyIncomingCall {
                        from: self.item(caller)?.clone(),
                        from_ranges: ranges.clone(),
                    })
                })
                .collect(),
        )
    }

    pub(crate) fn outgoing(
        &self,
        item: &CallHierarchyItem,
    ) -> Option<Vec<CallHierarchyOutgoingCall>> {
        let key = self.resolve_item(item)?;
        if self.incomplete_outgoing.contains(&key) {
            return None;
        }
        let mut callees = self.outgoing_by_key.get(&key).into_iter().flatten().collect::<Vec<_>>();
        callees.sort_by_key(|(callee, _)| *callee);
        Some(
            callees
                .into_iter()
                .filter_map(|(callee, ranges)| {
                    Some(CallHierarchyOutgoingCall {
                        to: self.item(callee)?.clone(),
                        from_ranges: ranges.clone(),
                    })
                })
                .collect(),
        )
    }

    fn call_site_key(&self, uri: &Url, position: Position) -> Option<&CallableKey> {
        self.call_sites_by_uri
            .get(uri)?
            .iter()
            .filter(|site| range_contains(site.range, position))
            .min_by_key(|site| (range_size_key(site.range), range_key(site.range)))
            .map(|site| &site.callee)
    }

    fn enclosing_body_key(&self, uri: &Url, position: Position) -> Option<&CallableKey> {
        self.bodies_by_uri
            .get(uri)?
            .iter()
            .filter(|body| range_contains(body.range, position))
            .min_by_key(|body| (range_size_key(body.range), range_key(body.range)))
            .map(|body| &body.callable)
    }

    fn item(&self, key: &CallableKey) -> Option<&CallHierarchyItem> {
        self.items_by_symbol.get(self.canonical_symbol_by_key.get(key)?)
    }

    fn resolve_item(&self, item: &CallHierarchyItem) -> Option<CallableKey> {
        let data = CallHierarchyData::deserialize(item.data.as_ref()?).ok()?;
        if data.version != DATA_VERSION {
            return None;
        }
        let key = CallableKey { uri: data.uri, selection_range: data.selection_range };
        let current = self.item(&key)?;
        // Name and kind distinguish a declaration replacement at the same source position. The
        // full range and detail are presentation data that may change while the callable remains.
        (item.uri == current.uri
            && item.selection_range == current.selection_range
            && item.name == current.name
            && item.kind == current.kind)
            .then_some(key)
    }
}

fn collect_direct_calls<'gcx>(
    facts: &mut CallHierarchyFacts,
    gcx: Gcx<'gcx>,
    item_symbols: &FxHashMap<ItemId, SymbolId>,
    declarations: &IndexVec<SymbolId, DeclarationSymbol>,
) {
    for caller_id in gcx.hir.function_ids() {
        let function = gcx.hir.function(caller_id);
        if !is_source_callable(function) {
            continue;
        }
        let Some(&caller) = item_symbols.get(&ItemId::Function(caller_id)) else {
            continue;
        };

        let mut collector = CallCollector { gcx, item_symbols, declarations, facts, caller };
        if function.is_constructor()
            && let Some(contract_id) = function.contract
        {
            for base in gcx.hir.contract(contract_id).bases_args {
                if !base.args.is_dummy() {
                    let _ = collector.visit_modifier(base);
                }
            }
        }
        for modifier in function.modifiers {
            let _ = collector.visit_modifier(modifier);
        }
        if let Some(body) = function.body {
            for statement in body.stmts {
                let _ = collector.visit_stmt(statement);
            }
        }
    }
}

struct CallCollector<'a, 'gcx> {
    gcx: Gcx<'gcx>,
    item_symbols: &'a FxHashMap<ItemId, SymbolId>,
    declarations: &'a IndexVec<SymbolId, DeclarationSymbol>,
    facts: &'a mut CallHierarchyFacts,
    caller: SymbolId,
}

impl<'gcx> Visit<'gcx> for CallCollector<'_, 'gcx> {
    type BreakValue = Never;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_modifier(
        &mut self,
        modifier: &'gcx hir::Modifier<'gcx>,
    ) -> ControlFlow<Self::BreakValue> {
        let callee = match modifier.id {
            ItemId::Function(callee) => Some(callee),
            ItemId::Contract(contract) => self.gcx.hir.contract(contract).ctor,
            _ => None,
        };
        if let Some(callee) = callee {
            self.push_call(callee, modifier.name_span);
        }
        self.walk_modifier(modifier)
    }

    fn visit_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Self::BreakValue> {
        if let Some((callee, span)) = resolved_source_call(self.gcx, expr) {
            self.push_call(callee, span);
        }
        self.walk_expr(expr)
    }

    fn visit_stmt(&mut self, stmt: &'gcx hir::Stmt<'gcx>) -> ControlFlow<Self::BreakValue> {
        if matches!(stmt.kind, hir::StmtKind::AssemblyBlock(_)) {
            ControlFlow::Continue(())
        } else {
            self.walk_stmt(stmt)
        }
    }

    fn visit_var(&mut self, variable: &'gcx hir::Variable<'gcx>) -> ControlFlow<Self::BreakValue> {
        if let Some(initializer) = variable.initializer {
            self.visit_expr(initializer)?;
        }
        ControlFlow::Continue(())
    }
}

impl CallCollector<'_, '_> {
    fn push_call(&mut self, callee_id: hir::FunctionId, span: Span) {
        if !is_source_callable(self.gcx.hir.function(callee_id)) {
            return;
        }
        let Some(&callee) = self.item_symbols.get(&ItemId::Function(callee_id)) else {
            return;
        };
        let Some(location) = proto::span_to_location(self.gcx.sess.source_map(), span) else {
            return;
        };
        if self.declarations[self.caller].location.uri != location.uri {
            return;
        }
        self.facts.direct_calls.push(DirectCall {
            caller: self.caller,
            callee,
            from_range: location.range,
        });
    }
}

fn is_source_callable(function: &hir::Function<'_>) -> bool {
    !function.is_yul && !function.is_getter()
}

fn resolved_source_call<'gcx>(
    gcx: Gcx<'gcx>,
    expr: &hir::Expr<'gcx>,
) -> Option<(hir::FunctionId, Span)> {
    let hir::ExprKind::Call(callee, ..) = expr.kind else {
        return None;
    };
    let callee = callee.peel_parens();
    if let hir::ExprKind::New(ty) = &callee.kind
        && let TyKind::Fn(function) = gcx.type_of_expr(callee.id)?.kind
        && function.is_creation()
        && let hir::TypeKind::Custom(ItemId::Contract(contract_id)) = ty.kind
    {
        return Some((gcx.hir.contract(contract_id).ctor?, ty.span));
    }
    let callee_id = gcx.resolved_call(expr)?.res.as_function()?;
    let span = match callee.kind {
        hir::ExprKind::Member(_, member) => member.span,
        _ => callee.span,
    };
    Some((callee_id, span))
}

fn normalize_relations(relations: &mut CallRelations) {
    for targets in relations.values_mut() {
        for ranges in targets.values_mut() {
            ranges.sort_by_key(|&range| range_key(range));
            ranges.dedup();
        }
    }
}

fn range_contains(range: Range, position: Position) -> bool {
    if range.start == range.end {
        position == range.start
    } else {
        position >= range.start && position < range.end
    }
}

fn range_size_key(range: Range) -> (u32, u32) {
    (
        range.end.line.saturating_sub(range.start.line),
        range.end.character.saturating_sub(range.start.character),
    )
}

fn range_key(range: Range) -> (u32, u32, u32, u32) {
    (range.start.line, range.start.character, range.end.line, range.end.character)
}
