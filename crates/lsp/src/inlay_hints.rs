use crate::proto;
use lsp_types::{InlayHint, InlayHintKind, InlayHintLabel, Position, Range, Url};
use solar_interface::{
    Symbol,
    data_structures::{Never, map::FxHashMap},
};
use solar_sema::{
    Gcx,
    builtins::Builtin,
    hir::{self, CallArgsKind, ExprKind, ItemId, StmtKind, Visit},
    ty::{CallableParamSource, Ty, TyKind},
};
use std::ops::ControlFlow;

#[derive(Clone, Debug, Default)]
pub(crate) struct InlayHintIndex {
    by_file: FxHashMap<Url, Vec<StoredInlayHint>>,
}

#[derive(Clone, Debug)]
struct StoredInlayHint {
    position: Position,
    // Labels are built once during analysis and only copied into LSP responses,
    // so store the fixed text without String's unused capacity.
    label: Box<str>,
    kind: StoredInlayHintKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum StoredInlayHintKind {
    Parameter,
    CallType,
}

impl InlayHintIndex {
    /// Builds the LSP-owned inlay hint index from the compiler HIR.
    ///
    /// The compiler's HIR data is scoped to one analysis run. This index copies out the inlay
    /// hints that LSP requests can query after that run has finished.
    pub(crate) fn build(gcx: Gcx<'_>) -> Self {
        let mut collector = InlayHintCollector { gcx, index: Self::default() };
        for source_id in gcx.hir.source_ids() {
            let _ = collector.visit_nested_source(source_id);
        }
        collector.index.sort();
        collector.index
    }

    pub(crate) fn extend(&mut self, other: Self) {
        for (uri, hints) in other.by_file {
            self.by_file.entry(uri).or_default().extend(hints);
        }
        self.sort();
    }

    /// Returns the inlay hints requested by the LSP client for a file range.
    ///
    /// The LSP inlay hint request includes a range so clients can ask only for the part of a file
    /// they need. Empty ranges include hints exactly at the requested position.
    pub(crate) fn hints(&self, uri: &Url, range: Range) -> Vec<InlayHint> {
        let Some(hints) = self.by_file.get(uri) else {
            return Vec::new();
        };
        let start = hints.partition_point(|hint| hint.position < range.start);
        let include_end = range.start == range.end;
        hints[start..]
            .iter()
            .take_while(|hint| {
                hint.position < range.end || (include_end && hint.position == range.end)
            })
            .map(StoredInlayHint::to_lsp)
            .collect()
    }

    /// Stores a collected inlay hint under the URI it should be returned for.
    fn push(&mut self, uri: Url, hint: StoredInlayHint) {
        self.by_file.entry(uri).or_default().push(hint);
    }

    /// Orders each file's hints for deterministic LSP responses and range filtering.
    fn sort(&mut self) {
        for hints in self.by_file.values_mut() {
            hints.sort_by(|a, b| hint_sort_key(a).cmp(&hint_sort_key(b)));
        }
    }
}

impl StoredInlayHint {
    /// Creates a parameter-name hint to display before a positional argument.
    fn parameter(position: Position, label: impl Into<Box<str>>) -> Self {
        Self { position, label: label.into(), kind: StoredInlayHintKind::Parameter }
    }

    /// Creates a type hint to display after a call expression.
    fn call_type(position: Position, label: impl Into<Box<str>>) -> Self {
        Self { position, label: label.into(), kind: StoredInlayHintKind::CallType }
    }

    /// Converts the stored hint into the LSP response type.
    fn to_lsp(&self) -> InlayHint {
        let (kind, padding_left, padding_right) = self.kind.lsp_fields();
        InlayHint {
            position: self.position,
            label: InlayHintLabel::String(self.label.to_string()),
            kind: Some(kind),
            text_edits: None,
            tooltip: None,
            padding_left: Some(padding_left),
            padding_right: Some(padding_right),
            data: None,
        }
    }
}

impl StoredInlayHintKind {
    fn lsp_fields(self) -> (InlayHintKind, bool, bool) {
        match self {
            Self::Parameter => (InlayHintKind::PARAMETER, false, true),
            Self::CallType => (InlayHintKind::TYPE, true, false),
        }
    }
}

struct InlayHintCollector<'gcx> {
    gcx: Gcx<'gcx>,
    index: InlayHintIndex,
}

impl<'gcx> InlayHintCollector<'gcx> {
    /// Adds parameter-name hints for positional arguments when parameter names are known.
    ///
    /// Hints are skipped when the argument already carries the same name as the parameter.
    fn push_parameter_hints(
        &mut self,
        args: &hir::CallArgs<'gcx>,
        param_source: Option<CallableParamSource>,
    ) {
        let (CallArgsKind::Unnamed(exprs), Some(param_source)) = (args.kind, param_source) else {
            return;
        };
        self.push_parameter_hints_for_exprs(exprs, param_source);
    }

    fn push_parameter_hints_for_exprs(
        &mut self,
        exprs: impl IntoIterator<Item = &'gcx hir::Expr<'gcx>>,
        param_source: CallableParamSource,
    ) {
        let param_names = self.gcx.callable_param_names(param_source);
        self.push_parameter_hints_for_exprs_and_names(exprs, param_names);
    }

    fn push_parameter_hints_for_exprs_and_names(
        &mut self,
        exprs: impl IntoIterator<Item = &'gcx hir::Expr<'gcx>>,
        param_names: impl IntoIterator<Item = Option<Symbol>>,
    ) {
        for (arg, param_name) in exprs.into_iter().zip(param_names) {
            let Some(param_name) = param_name else {
                continue;
            };
            if self.argument_name_matches_param(arg, param_name) {
                continue;
            }
            let Some(location) = proto::span_to_location(self.gcx.sess.source_map(), arg.span)
            else {
                continue;
            };
            self.index.push(
                location.uri,
                StoredInlayHint::parameter(location.range.start, format!("{param_name}:")),
            );
        }
    }

    /// Adds target parameter-name hints for the encoded arguments in `abi.encodeCall`.
    fn push_abi_encode_call_parameter_hints(&mut self, args: &hir::CallArgs<'gcx>) {
        let CallArgsKind::Unnamed([target, arguments]) = args.kind else {
            return;
        };
        let Some(param_source) = self.call_param_source_from_expr(target) else {
            return;
        };
        let param_names = self.gcx.callable_param_names(param_source);
        let arguments = arguments.peel_parens();

        match arguments.kind {
            ExprKind::Tuple(components) => {
                if components.len() != param_names.len()
                    || components.iter().any(|component| component.is_none())
                {
                    return;
                }
                self.push_parameter_hints_for_exprs_and_names(
                    components.iter().filter_map(|component| *component),
                    param_names,
                );
            }
            _ if param_names.len() == 1 => {
                self.push_parameter_hints_for_exprs_and_names([arguments], param_names);
            }
            _ => {}
        }
    }

    /// Returns whether an argument expression already makes the parameter name visible.
    fn argument_name_matches_param(&self, arg: &'gcx hir::Expr<'gcx>, param_name: Symbol) -> bool {
        let arg = arg.peel_parens();
        if let ExprKind::Ident([res]) = arg.kind
            && let Some(variable) = res.as_variable()
            && self.gcx.hir.variable(variable).name.is_some_and(|name| name.name == param_name)
        {
            return true;
        }
        self.gcx
            .sess
            .source_map()
            .span_to_snippet(arg.span)
            .is_ok_and(|snippet| snippet == param_name.as_str())
    }

    /// Adds a result-type hint after a call when the call has a useful inferred type.
    fn push_call_type_hint(&mut self, expr: &'gcx hir::Expr<'gcx>, callee_ty: Option<Ty<'gcx>>) {
        if callee_ty.is_some_and(|ty| matches!(ty.kind, TyKind::Type(_))) {
            return;
        }
        let Some(ty) = self.gcx.type_of_expr(expr.id) else {
            return;
        };
        if ty.is_unit() || ty.references_error() {
            return;
        }
        let Some(location) = proto::span_to_location(self.gcx.sess.source_map(), expr.span) else {
            return;
        };
        self.index.push(
            location.uri,
            StoredInlayHint::call_type(location.range.end, self.call_type_label(ty)),
        );
    }

    fn call_type_label(&self, ty: Ty<'gcx>) -> String {
        if let TyKind::Tuple(tys) = ty.kind {
            let tys = tys
                .iter()
                .map(|ty| ty.display(self.gcx).to_string())
                .collect::<Vec<_>>()
                .join(", ");
            return format!(": ({tys})");
        }
        format!(": {}", ty.display(self.gcx))
    }

    /// Finds the callable declaration that supplies parameter names for a call expression.
    fn call_param_source(
        &self,
        callee: &'gcx hir::Expr<'gcx>,
        callee_ty: Option<Ty<'gcx>>,
    ) -> Option<CallableParamSource> {
        if let ExprKind::New(hir_ty) = &callee.kind
            && let TyKind::Contract(id) = self.gcx.type_of_hir_ty(hir_ty).kind
            && let Some(ctor) = self.gcx.hir.contract(id).ctor
        {
            return Some(CallableParamSource::Function { id: ctor, skips_receiver: false });
        }

        self.call_param_source_from_expr(callee).or_else(|| {
            callee_ty
                .and_then(|ty| self.gcx.callable_signature_of_ty(ty))
                .and_then(|signature| signature.param_source)
        })
    }

    /// Finds the parameter-name source attached to a specific expression.
    fn call_param_source_from_expr(
        &self,
        expr: &'gcx hir::Expr<'gcx>,
    ) -> Option<CallableParamSource> {
        let expr = expr.peel_parens();
        if let ExprKind::Ident([res]) = expr.kind
            && let Some(variable) = res.as_variable()
            && matches!(self.gcx.hir.variable(variable).ty.kind, hir::TypeKind::Function(_))
        {
            return Some(CallableParamSource::FunctionType(variable));
        }

        if let ExprKind::Member(receiver, ident) = expr.kind
            && let Some(receiver_ty) = self.gcx.type_of_expr(receiver.id)
            && let Some(field) = self.struct_field(receiver_ty, ident.name)
            && matches!(self.gcx.hir.variable(field).ty.kind, hir::TypeKind::Function(_))
        {
            return Some(CallableParamSource::FunctionType(field));
        }

        self.gcx
            .type_of_expr(expr.id)
            .and_then(|ty| self.gcx.callable_signature_of_ty(ty))
            .and_then(|signature| signature.param_source)
    }

    fn struct_field(&self, receiver_ty: Ty<'gcx>, name: Symbol) -> Option<hir::VariableId> {
        let struct_id = match receiver_ty.kind {
            TyKind::Ref(inner, _) => match inner.kind {
                TyKind::Struct(id) => id,
                _ => return None,
            },
            TyKind::Struct(id) => id,
            _ => return None,
        };
        self.gcx.hir.strukt(struct_id).fields.iter().copied().find(|&field| {
            self.gcx.hir.variable(field).name.is_some_and(|field_name| field_name.name == name)
        })
    }

    /// Finds the function or constructor declaration that supplies parameter names for a modifier.
    fn modifier_param_source(
        &self,
        modifier: &'gcx hir::Modifier<'gcx>,
    ) -> Option<CallableParamSource> {
        let id = match modifier.id {
            ItemId::Contract(id) => self.gcx.hir.contract(id).ctor?,
            ItemId::Function(id) => id,
            _ => return None,
        };
        Some(CallableParamSource::Function { id, skips_receiver: false })
    }
}

impl<'gcx> Visit<'gcx> for InlayHintCollector<'gcx> {
    type BreakValue = Never;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Self::BreakValue> {
        if let ExprKind::Call(callee, ref args, _) = expr.kind {
            let callee_ty = self.gcx.type_of_expr(callee.id);
            if self.gcx.builtin_callee(callee.id) == Some(Builtin::AbiEncodeCall) {
                self.push_abi_encode_call_parameter_hints(args);
            }
            self.push_parameter_hints(args, self.call_param_source(callee, callee_ty));
            self.push_call_type_hint(expr, callee_ty);
        }
        hir::Visit::walk_expr(self, expr)
    }

    fn visit_stmt(&mut self, stmt: &'gcx hir::Stmt<'gcx>) -> ControlFlow<Self::BreakValue> {
        if matches!(stmt.kind, StmtKind::AssemblyBlock(_)) {
            return ControlFlow::Continue(());
        }
        hir::Visit::walk_stmt(self, stmt)
    }

    fn visit_modifier(
        &mut self,
        modifier: &'gcx hir::Modifier<'gcx>,
    ) -> ControlFlow<Self::BreakValue> {
        self.push_parameter_hints(&modifier.args, self.modifier_param_source(modifier));
        hir::Visit::walk_modifier(self, modifier)
    }
}

fn hint_sort_key(hint: &StoredInlayHint) -> (Position, StoredInlayHintKind, &str) {
    (hint.position, hint.kind, hint.label.as_ref())
}
