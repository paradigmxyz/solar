//! Signature help data collected from compiler analysis.

use crate::{config::SignatureHelpClientOptions, proto};
use crop::Rope;
use lsp_types::{
    Documentation, MarkupContent, MarkupKind, ParameterInformation, ParameterLabel, Position,
    Range, SignatureHelp, SignatureInformation, Url,
};
use solar_interface::{
    Span,
    data_structures::{
        Never,
        map::{FxHashMap, FxHashSet},
    },
};
use solar_parse::Cursor;
use solar_sema::{
    Gcx,
    builtins::Builtin,
    hir::{self, CallArgs, FunctionKind, ItemId, NatSpecKind, Res, StateMutability, Visit},
    ty::{CallableParamSource, CallableSignature, ResolvedMember, TyKind},
};
use std::{fmt::Write, ops::ControlFlow, sync::Arc};

#[derive(Clone, Debug, Default)]
pub(crate) struct SignatureHelpIndex {
    calls: FxHashMap<Url, Vec<CallSite>>,
    callables_by_name: FxHashMap<String, Vec<CatalogEntry>>,
    signatures_by_label: FxHashMap<String, Vec<Arc<CallSignature>>>,
}

#[derive(Clone, Debug)]
struct CallSite {
    range: Range,
    callee_range: Range,
    callee_tokens: Vec<String>,
    form: CallForm,
    signatures: Vec<Arc<CallSignature>>,
}

#[derive(Clone, Debug)]
struct CatalogEntry {
    origin: Option<Url>,
    form: CallForm,
    signature: Arc<CallSignature>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CallForm {
    Regular,
    New,
    Event,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CallSignature {
    information: SignatureInformation,
    parameter_names: Vec<Option<String>>,
    variadic: bool,
}

#[derive(Clone, Debug)]
struct ActiveArgument<'a> {
    ordinal: usize,
    name: Option<&'a str>,
}

impl SignatureHelpIndex {
    pub(crate) fn build(gcx: Gcx<'_>) -> Self {
        let mut index = Self::default();
        index.build_callable_catalog(gcx);
        let mut collector = CallCollector { index: &mut index, gcx, source: None, contract: None };
        for source_id in gcx.hir.source_ids() {
            collector.source = Some(source_id);
            collector.contract = None;
            let _ = collector.visit_nested_source(source_id);
        }
        for calls in index.calls.values_mut() {
            calls.sort_by_key(|call| range_size_key(call.range));
        }
        index
    }

    pub(crate) fn extend(&mut self, other: Self) {
        for (uri, mut calls) in other.calls {
            for call in &mut calls {
                for signature in &mut call.signatures {
                    *signature = self.intern_shared_signature(signature.clone());
                }
            }
            let destination = self.calls.entry(uri).or_default();
            destination.extend(calls);
            destination.sort_by_key(|call| range_size_key(call.range));
        }
        for (name, entries) in other.callables_by_name {
            for entry in entries {
                self.push_shared_callable(name.clone(), entry.origin, entry.form, entry.signature);
            }
        }
    }

    pub(crate) fn retain_failed_files(&mut self, previous: &Self, uris: &[Url]) {
        let indexed_origins = self
            .callables_by_name
            .values()
            .flatten()
            .filter_map(|entry| entry.origin.clone())
            .collect::<FxHashSet<_>>();
        for uri in uris {
            let Some(previous_calls) = previous.calls.get(uri) else { continue };
            let mut current_starts = self
                .calls
                .get(uri)
                .into_iter()
                .flatten()
                .map(|call| call.range.start)
                .collect::<FxHashSet<_>>();
            for previous_call in previous_calls {
                if !current_starts.insert(previous_call.range.start) {
                    continue;
                }
                let mut call = previous_call.clone();
                call.signatures.retain(|signature| {
                    self.signatures_by_label
                        .get(&signature.information.label)
                        .is_some_and(|candidates| candidates.contains(signature))
                });
                if call.signatures.is_empty() {
                    continue;
                }
                for signature in &mut call.signatures {
                    *signature = self.intern_shared_signature(signature.clone());
                }
                self.calls.entry(uri.clone()).or_default().push(call);
            }
            if let Some(calls) = self.calls.get_mut(uri) {
                calls.sort_by_key(|call| range_size_key(call.range));
            }
        }
        // Parsing failures can leave a file without a new catalog, so keep its old fallback
        // entries.
        for (name, entries) in &previous.callables_by_name {
            for entry in entries {
                if let Some(origin) = &entry.origin
                    && uris.contains(origin)
                    && (!indexed_origins.contains(origin)
                        || self
                            .signatures_by_label
                            .get(&entry.signature.information.label)
                            .is_some_and(|candidates| candidates.contains(&entry.signature)))
                {
                    self.push_shared_callable(
                        name.clone(),
                        entry.origin.clone(),
                        entry.form,
                        entry.signature.clone(),
                    );
                }
            }
        }
    }

    pub(crate) fn signature_help(
        &self,
        uri: &Url,
        position: Position,
        contents: &Rope,
        options: SignatureHelpClientOptions,
    ) -> Option<SignatureHelp> {
        let cursor = proto::text_range(contents, Range::new(position, position)).start;
        let text = contents.byte_slice(..cursor).to_string();
        let context = call_context(&text)?;
        let call = self.calls.get(uri).and_then(|calls| {
            calls.iter().find(|call| {
                valid_text_position(contents, call.range.start)
                    && proto::text_range(contents, Range::new(call.range.start, call.range.start))
                        .start
                        == context.open
                    && call
                        .callee_tokens
                        .last()
                        .map(String::as_str)
                        .filter(|token| is_identifier(token))
                        == context.callee_name
                    && call.form == context.form
                    && call.matches_current_callee(contents)
            })
        });
        let (mut signatures, fallback): (Vec<&CallSignature>, _) = if let Some(call) = call {
            (call.signatures.iter().map(Arc::as_ref).collect(), false)
        } else {
            if context.member_call {
                return None;
            }
            (
                self.callables_by_name
                    .get(context.callee_name?)?
                    .iter()
                    .filter(|entry| entry.form == context.form)
                    .map(|entry| entry.signature.as_ref())
                    .collect(),
                true,
            )
        };
        deduplicate_signatures(&mut signatures);
        if signatures.is_empty() {
            return None;
        }
        if fallback {
            signatures.sort_by_key(|signature| signature.fallback_rank(&context.active_argument));
        }
        let active_parameter = signatures
            .first()
            .and_then(|signature| signature.active_parameter(&context.active_argument))
            .map(|index| index as u32);
        let signatures = signatures
            .into_iter()
            .map(|signature| {
                let signature_active =
                    signature.active_parameter(&context.active_argument).map(|index| index as u32);
                let mut information = signature.information.clone();
                information.active_parameter =
                    options.signature_active_parameter.then_some(signature_active).flatten();
                if !options.label_offsets {
                    use_simple_parameter_labels(&mut information);
                }
                if options.markdown_documentation {
                    use_markdown_documentation(&mut information);
                }
                information
            })
            .collect();
        Some(SignatureHelp { signatures, active_signature: Some(0), active_parameter })
    }

    fn build_callable_catalog(&mut self, gcx: Gcx<'_>) {
        for item_id in gcx.hir.item_ids() {
            if let Some(name) = gcx.hir.item(item_id).name()
                && let Some(signature) = render_item(gcx, item_id)
            {
                let origin =
                    proto::span_to_location(gcx.sess.source_map(), gcx.hir.item(item_id).span())
                        .map(|location| location.uri);
                let form = match item_id {
                    ItemId::Contract(_) => CallForm::New,
                    ItemId::Event(_) => CallForm::Event,
                    ItemId::Error(_) => CallForm::Error,
                    _ => CallForm::Regular,
                };
                self.push_callable(name.to_string(), origin, form, signature);
            }
        }
        for builtin in Builtin::global() {
            if let Some(signature) = render_res(gcx, Res::Builtin(builtin)) {
                self.push_callable(builtin.name().to_string(), None, CallForm::Regular, signature);
            }
        }
    }

    fn push_callable(
        &mut self,
        name: String,
        origin: Option<Url>,
        form: CallForm,
        signature: CallSignature,
    ) {
        self.push_shared_callable(name, origin, form, Arc::new(signature));
    }

    fn push_shared_callable(
        &mut self,
        name: String,
        origin: Option<Url>,
        form: CallForm,
        signature: Arc<CallSignature>,
    ) {
        let signature = self.intern_shared_signature(signature);
        let entries = self.callables_by_name.entry(name).or_default();
        if !entries.iter().any(|entry| {
            entry.origin == origin && entry.form == form && entry.signature == signature
        }) {
            entries.push(CatalogEntry { origin, form, signature });
        }
    }

    fn push(
        &mut self,
        gcx: Gcx<'_>,
        args: &CallArgs<'_>,
        callee_span: Span,
        form: CallForm,
        signatures: Vec<CallSignature>,
    ) {
        if args.is_dummy() || signatures.is_empty() {
            return;
        }
        let Some(location) = proto::span_to_location(gcx.sess.source_map(), args.span) else {
            return;
        };
        let Some(callee_location) = proto::span_to_location(gcx.sess.source_map(), callee_span)
        else {
            return;
        };
        if callee_location.uri != location.uri {
            return;
        }
        let Ok(callee_text) = gcx.sess.source_map().span_to_snippet(callee_span) else { return };
        let callee_tokens = significant_tokens(&callee_text);
        let signatures =
            signatures.into_iter().map(|signature| self.intern_signature(signature)).collect();
        self.calls.entry(location.uri).or_default().push(CallSite {
            range: location.range,
            callee_range: callee_location.range,
            callee_tokens,
            form,
            signatures,
        });
    }

    fn intern_signature(&mut self, signature: CallSignature) -> Arc<CallSignature> {
        self.intern_shared_signature(Arc::new(signature))
    }

    fn intern_shared_signature(&mut self, signature: Arc<CallSignature>) -> Arc<CallSignature> {
        let candidates =
            self.signatures_by_label.entry(signature.information.label.clone()).or_default();
        if let Some(existing) =
            candidates.iter().find(|existing| existing.as_ref() == signature.as_ref())
        {
            return existing.clone();
        }
        candidates.push(signature.clone());
        signature
    }
}

impl CallSite {
    fn matches_current_callee(&self, contents: &Rope) -> bool {
        if !valid_text_position(contents, self.callee_range.start)
            || !valid_text_position(contents, self.callee_range.end)
        {
            return false;
        }
        let range = proto::text_range(contents, self.callee_range);
        if range.start > range.end {
            return false;
        }
        let current = contents.byte_slice(range).to_string();
        significant_token_slices(&current).eq(self.callee_tokens.iter().map(String::as_str))
    }
}

fn valid_text_position(rope: &Rope, position: Position) -> bool {
    let line = position.line as usize;
    if line >= rope.line_len() {
        return false;
    }
    let line = rope.line(line);
    let character = position.character as usize;
    if character > line.utf16_len() {
        return false;
    }
    let byte = line.byte_of_utf16_code_unit(character);
    line.utf16_code_unit_of_byte(byte) == character
}

impl CallSignature {
    fn fallback_rank(&self, argument: &ActiveArgument<'_>) -> u8 {
        if argument.name.is_some_and(|name| {
            self.parameter_names.iter().any(|parameter| parameter.as_deref() == Some(name))
        }) {
            0
        } else if argument.ordinal < self.parameter_names.len() || self.variadic {
            1
        } else {
            2
        }
    }

    fn active_parameter(&self, argument: &ActiveArgument<'_>) -> Option<usize> {
        let index = argument.name.and_then(|name| {
            self.parameter_names.iter().position(|parameter| parameter.as_deref() == Some(name))
        });
        index.or_else(|| {
            if argument.ordinal < self.parameter_names.len() {
                Some(argument.ordinal)
            } else if self.variadic {
                self.parameter_names.len().checked_sub(1)
            } else {
                None
            }
        })
    }
}

struct CallCollector<'a, 'gcx> {
    index: &'a mut SignatureHelpIndex,
    gcx: Gcx<'gcx>,
    source: Option<hir::SourceId>,
    contract: Option<hir::ContractId>,
}

impl<'gcx> CallCollector<'_, 'gcx> {
    fn collect_call(&mut self, callee: &'gcx hir::Expr<'gcx>, args: &CallArgs<'gcx>) {
        let callee_ty = self.gcx.type_of_expr(callee.id);
        let form = match callee.kind {
            hir::ExprKind::New(_) => CallForm::New,
            _ if callee_ty.is_some_and(|ty| matches!(ty.kind, TyKind::Event(..))) => {
                CallForm::Event
            }
            _ if callee_ty.is_some_and(|ty| matches!(ty.kind, TyKind::Error(..))) => {
                CallForm::Error
            }
            hir::ExprKind::Ident(resolutions)
                if resolutions.iter().any(|res| matches!(res, Res::Item(ItemId::Event(_)))) =>
            {
                CallForm::Event
            }
            hir::ExprKind::Ident(resolutions)
                if resolutions.iter().any(|res| matches!(res, Res::Item(ItemId::Error(_)))) =>
            {
                CallForm::Error
            }
            _ => CallForm::Regular,
        };
        let callee_span = callee.span.with_hi(args.span.lo());
        let selected = self.gcx.resolved_callee(callee.id);
        let mut candidates = Vec::<(bool, CallSignature)>::new();

        match callee.kind {
            hir::ExprKind::Ident(resolutions) => {
                for &res in resolutions {
                    if matches!(res, Res::Item(ItemId::Contract(_))) {
                        continue;
                    }
                    if let Some(signature) = render_res(self.gcx, res) {
                        candidates.push((selected.is_some_and(|it| it.res == res), signature));
                    }
                }
            }
            hir::ExprKind::Member(receiver, name) => {
                if let Some(source) = self.source
                    && let Some(receiver_ty) = self.gcx.type_of_expr(receiver.id)
                {
                    for completion in self
                        .gcx
                        .member_completions_of(receiver_ty, source, self.contract)
                        .filter(|completion| completion.member.name == name.name)
                    {
                        let member = completion.member;
                        let Some(mut callable) = self
                            .gcx
                            .callable_signature_of_member(receiver_ty, &member)
                            .or_else(|| self.gcx.callable_signature_of_ty(member.ty))
                        else {
                            continue;
                        };
                        if callable.param_source.is_none()
                            && let Some(ResolvedMember::StructField { struct_id, field_index }) =
                                completion.resolved
                            && let Some(&variable_id) =
                                self.gcx.hir.strukt(struct_id).fields.get(field_index)
                        {
                            callable.param_source =
                                Some(CallableParamSource::FunctionType(variable_id));
                        }
                        let is_selected = selected.is_some_and(|selected| {
                            member.res == Some(selected.res) && member.attached == selected.attached
                        });
                        if let Some(signature) =
                            render_callable(self.gcx, callable, member.res, Some(name.to_string()))
                        {
                            candidates.push((is_selected, signature));
                        }
                    }
                }
            }
            _ => {
                let signature = if let hir::ExprKind::New(ref ty) = callee.kind
                    && let TyKind::Contract(id) = self.gcx.type_of_hir_ty(ty).kind
                {
                    render_item(self.gcx, ItemId::Contract(id))
                } else if let Some(ty) = callee_ty
                    && let Some(callable) = self.gcx.callable_signature_of_ty(ty)
                {
                    let fallback_name =
                        self.gcx.sess.source_map().span_to_snippet(callee.span).ok();
                    render_callable(self.gcx, callable, None, fallback_name)
                } else {
                    None
                };
                if let Some(signature) = signature {
                    candidates.push((true, signature));
                }
            }
        }

        candidates.sort_by_key(|(selected, _)| !selected);
        let mut signatures = Vec::with_capacity(candidates.len());
        for (_, signature) in candidates {
            if !signatures.iter().any(|existing: &CallSignature| {
                existing.information.label == signature.information.label
            }) {
                signatures.push(signature);
            }
        }
        self.index.push(self.gcx, args, callee_span, form, signatures);
    }

    fn collect_modifier(&mut self, modifier: &'gcx hir::Modifier<'gcx>) {
        let Some(signature) = render_item(self.gcx, modifier.id) else { return };
        self.index.push(
            self.gcx,
            &modifier.args,
            modifier.span.with_hi(modifier.args.span.lo()),
            CallForm::Regular,
            vec![signature],
        );
    }
}

impl<'gcx> Visit<'gcx> for CallCollector<'_, 'gcx> {
    type BreakValue = Never;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_nested_contract(&mut self, id: hir::ContractId) -> ControlFlow<Self::BreakValue> {
        let previous = self.contract.replace(id);
        let result = self.visit_contract(self.hir().contract(id));
        self.contract = previous;
        result
    }

    fn visit_modifier(
        &mut self,
        modifier: &'gcx hir::Modifier<'gcx>,
    ) -> ControlFlow<Self::BreakValue> {
        self.collect_modifier(modifier);
        self.visit_call_args(&modifier.args)
    }

    fn visit_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Self::BreakValue> {
        if let hir::ExprKind::Call(callee, ref args, _) = expr.kind {
            self.collect_call(callee, args);
        }
        hir::Visit::walk_expr(self, expr)
    }
}

fn render_res(gcx: Gcx<'_>, res: Res) -> Option<CallSignature> {
    if let Res::Item(item_id) = res {
        return render_item(gcx, item_id);
    }
    let callable = gcx.callable_signature_of_ty(gcx.type_of_res(res))?;
    let fallback_name = match res {
        Res::Builtin(builtin) => Some(builtin.name().to_string()),
        Res::Item(item_id) => gcx.hir.item(item_id).name().map(|name| name.to_string()),
        Res::Namespace(_) | Res::Err(_) => None,
    };
    render_callable(gcx, callable, Some(res), fallback_name)
}

fn render_item(gcx: Gcx<'_>, item_id: ItemId) -> Option<CallSignature> {
    if let ItemId::Contract(id) = item_id {
        let contract = gcx.hir.contract(id);
        if let Some(constructor) = contract.ctor {
            return render_item(gcx, ItemId::Function(constructor));
        }
        return Some(CallSignature {
            information: SignatureInformation {
                label: "constructor()".into(),
                documentation: None,
                parameters: Some(Vec::new()),
                active_parameter: None,
            },
            parameter_names: Vec::new(),
            variadic: false,
        });
    }

    let res = Res::Item(item_id);
    let mut callable = gcx.callable_signature_of_ty(gcx.type_of_res(res))?;
    if let ItemId::Variable(id) = item_id {
        callable.param_source = Some(CallableParamSource::FunctionType(id));
    }
    let fallback_name = gcx.hir.item(item_id).name().map(|name| name.to_string());
    render_callable(gcx, callable, Some(res), fallback_name)
}

fn render_callable<'gcx>(
    gcx: Gcx<'gcx>,
    callable: CallableSignature<'gcx>,
    res: Option<Res>,
    fallback_name: Option<String>,
) -> Option<CallSignature> {
    let names =
        callable.param_source.map(|source| gcx.callable_param_names(source)).unwrap_or_default();
    let (prefix, suffix, doc_id) = signature_declaration_parts(gcx, callable, res, fallback_name)?;
    let (documentation, parameter_docs) = documentation(gcx, doc_id);

    let mut label = prefix;
    let mut label_utf16_len = label.encode_utf16().count();
    label.push('(');
    label_utf16_len += 1;
    let mut parameters = Vec::with_capacity(callable.parameters.len());
    let mut parameter_names = Vec::with_capacity(callable.parameters.len());
    for (index, &ty) in callable.parameters.iter().enumerate() {
        if index != 0 {
            label.push_str(", ");
            label_utf16_len += 2;
        }
        let start = label_utf16_len as u32;
        let parameter_start = label.len();
        write!(label, "{}", ty.display(gcx)).ok()?;
        if parameter_is_indexed(gcx, callable.param_source, index) {
            label.push_str(" indexed");
        }
        let name = names.get(index).and_then(|name| *name);
        if let Some(name) = name {
            write!(label, " {name}").ok()?;
        }
        label_utf16_len += label[parameter_start..].encode_utf16().count();
        let documentation = name.and_then(|name| parameter_docs.get(name.as_str())).cloned();
        parameter_names.push(name.map(|name| name.to_string()));
        parameters.push(ParameterInformation {
            label: ParameterLabel::LabelOffsets([start, label_utf16_len as u32]),
            documentation: documentation.map(Documentation::String),
        });
    }
    label.push(')');
    label.push_str(&suffix);

    Some(CallSignature {
        information: SignatureInformation {
            label,
            documentation: documentation.map(Documentation::String),
            parameters: Some(parameters),
            active_parameter: None,
        },
        parameter_names,
        variadic: callable.parameters.last().is_some_and(|ty| matches!(ty.kind, TyKind::Variadic)),
    })
}

fn signature_declaration_parts<'gcx>(
    gcx: Gcx<'gcx>,
    callable: CallableSignature<'gcx>,
    res: Option<Res>,
    fallback_name: Option<String>,
) -> Option<(String, String, Option<hir::DocId>)> {
    let mut prefix = String::new();
    let mut suffix = String::new();
    let mut doc_id = None;

    match callable.param_source {
        Some(CallableParamSource::Function { id, .. }) => {
            let function = gcx.hir.function(id);
            doc_id = Some(function.gettee.map_or(function.doc, |id| gcx.hir.variable(id).doc));
            write!(prefix, "{}", function.kind).ok()?;
            if let Some(name) = function.name {
                write!(prefix, " {name}").ok()?;
            }
            if matches!(function.kind, FunctionKind::Function) {
                write!(suffix, " {}", function.visibility).ok()?;
                if function.state_mutability != StateMutability::NonPayable {
                    write!(suffix, " {}", function.state_mutability).ok()?;
                }
            } else if matches!(function.kind, FunctionKind::Constructor)
                && function.state_mutability == StateMutability::Payable
            {
                suffix.push_str(" payable");
            }
        }
        Some(CallableParamSource::Struct(id)) => {
            let strukt = gcx.hir.strukt(id);
            write!(prefix, "struct {}", strukt.name).ok()?;
            doc_id = Some(strukt.doc);
        }
        Some(CallableParamSource::Event(id)) => {
            let event = gcx.hir.event(id);
            write!(prefix, "event {}", event.name).ok()?;
            doc_id = Some(event.doc);
            if event.anonymous {
                suffix.push_str(" anonymous");
            }
        }
        Some(CallableParamSource::Error(id)) => {
            let error = gcx.hir.error(id);
            write!(prefix, "error {}", error.name).ok()?;
            doc_id = Some(error.doc);
        }
        Some(CallableParamSource::FunctionType(id)) => {
            let variable = gcx.hir.variable(id);
            prefix.push_str(
                &variable.name.map_or_else(|| "function".into(), |name| name.to_string()),
            );
            doc_id = Some(variable.doc);
        }
        None => {
            let fallback_name = fallback_name.or_else(|| match res {
                Some(Res::Builtin(builtin)) => Some(builtin.name().to_string()),
                _ => None,
            })?;
            prefix.push_str(&fallback_name);
        }
    }

    if !callable.returns.is_empty()
        && !matches!(callable.param_source, Some(CallableParamSource::Struct(_)))
    {
        let return_variables = callable_return_variables(gcx, callable.param_source);
        suffix.push_str(" returns (");
        for (index, &ty) in callable.returns.iter().enumerate() {
            if index != 0 {
                suffix.push_str(", ");
            }
            write!(suffix, "{}", ty.display(gcx)).ok()?;
            if let Some(name) =
                return_variables.get(index).and_then(|&id| gcx.hir.variable(id).name)
            {
                write!(suffix, " {name}").ok()?;
            }
        }
        suffix.push(')');
    }
    Some((prefix, suffix, doc_id))
}

fn callable_return_variables<'gcx>(
    gcx: Gcx<'gcx>,
    source: Option<CallableParamSource>,
) -> &'gcx [hir::VariableId] {
    match source {
        Some(CallableParamSource::Function { id, .. }) => gcx.hir.function(id).returns,
        Some(CallableParamSource::FunctionType(id)) => match gcx.hir.variable(id).ty.kind {
            hir::TypeKind::Function(function) => function.returns,
            _ => &[],
        },
        _ => &[],
    }
}

fn parameter_is_indexed(gcx: Gcx<'_>, source: Option<CallableParamSource>, index: usize) -> bool {
    let Some(CallableParamSource::Event(id)) = source else { return false };
    gcx.hir.event(id).parameters.get(index).is_some_and(|&id| gcx.hir.variable(id).indexed)
}

fn documentation(
    gcx: Gcx<'_>,
    doc_id: Option<hir::DocId>,
) -> (Option<String>, FxHashMap<String, String>) {
    let Some(doc_id) = doc_id.filter(|id| !id.is_empty()) else {
        return Default::default();
    };
    let mut docs = Vec::new();
    let mut params = FxHashMap::default();
    for item in gcx.natspec_doc_comments(doc_id) {
        match item.kind {
            NatSpecKind::Notice | NatSpecKind::Dev => docs.push(item.content().to_string()),
            NatSpecKind::Param { name } => {
                params.insert(name.name.to_string(), item.content().to_string());
            }
            _ => {}
        }
    }
    ((!docs.is_empty()).then(|| docs.join("\n\n")), params)
}

fn markdown(value: String) -> Documentation {
    Documentation::MarkupContent(MarkupContent { kind: MarkupKind::Markdown, value })
}

fn use_markdown_documentation(signature: &mut SignatureInformation) {
    convert_documentation_to_markdown(&mut signature.documentation);
    if let Some(parameters) = &mut signature.parameters {
        for parameter in parameters {
            convert_documentation_to_markdown(&mut parameter.documentation);
        }
    }
}

fn convert_documentation_to_markdown(documentation: &mut Option<Documentation>) {
    let Some(Documentation::String(value)) = documentation.take() else { return };
    *documentation = Some(markdown(value));
}

fn deduplicate_signatures(signatures: &mut Vec<&CallSignature>) {
    let mut unique = Vec::with_capacity(signatures.len());
    for signature in signatures.drain(..) {
        if !unique.contains(&signature) {
            unique.push(signature);
        }
    }
    *signatures = unique;
}

fn significant_tokens(text: &str) -> Vec<String> {
    significant_token_slices(text).map(str::to_owned).collect()
}

fn significant_token_slices(text: &str) -> impl Iterator<Item = &str> {
    Cursor::new(text)
        .with_position()
        .filter(|(_, token)| !token.kind.is_trivial())
        .map(|(start, token)| &text[start..start + token.len as usize])
}

fn use_simple_parameter_labels(signature: &mut SignatureInformation) {
    let Some(parameters) = &mut signature.parameters else { return };
    for parameter in parameters {
        let ParameterLabel::LabelOffsets([start, end]) = parameter.label else { continue };
        let Some(label) = utf16_slice(&signature.label, start, end) else { continue };
        parameter.label = ParameterLabel::Simple(label.to_string());
    }
}

fn utf16_slice(value: &str, start: u32, end: u32) -> Option<&str> {
    let mut utf16 = 0u32;
    let mut start_byte = None;
    let mut end_byte = None;
    for (byte, ch) in value.char_indices() {
        if utf16 == start {
            start_byte = Some(byte);
        }
        if utf16 == end {
            end_byte = Some(byte);
            break;
        }
        utf16 += ch.len_utf16() as u32;
    }
    if start_byte.is_none() && utf16 == start {
        start_byte = Some(value.len());
    }
    if end_byte.is_none() && utf16 == end {
        end_byte = Some(value.len());
    }
    value.get(start_byte?..end_byte?)
}

#[derive(Debug)]
struct CallContext<'a> {
    open: usize,
    callee_name: Option<&'a str>,
    form: CallForm,
    member_call: bool,
    active_argument: ActiveArgument<'a>,
}

#[derive(Clone, Copy, Debug)]
struct DelimiterFrame {
    delimiter: char,
    open: usize,
}

fn call_context(text: &str) -> Option<CallContext<'_>> {
    let mut frames = Vec::<DelimiterFrame>::new();
    let mut significant = Vec::<(usize, usize)>::new();

    for (start, token) in Cursor::new(text).with_position() {
        let end = start + token.len as usize;
        let lexeme = &text[start..end];
        if token.kind.is_trivial() {
            continue;
        }
        match lexeme {
            "(" | "[" | "{" => frames
                .push(DelimiterFrame { delimiter: lexeme.chars().next().unwrap(), open: start }),
            ";" => {
                frames.clear();
                significant.clear();
            }
            ")" | "]" | "}" => {
                let expected = match lexeme {
                    ")" => '(',
                    "]" => '[',
                    "}" => '{',
                    _ => unreachable!(),
                };
                if let Some(index) = frames.iter().rposition(|frame| frame.delimiter == expected) {
                    frames.truncate(index);
                }
            }
            _ => {}
        }
        significant.push((start, end));
    }

    for frame in frames.iter().rev().filter(|frame| frame.delimiter == '(') {
        let head_index = significant.iter().rposition(|(_, end)| *end <= frame.open)?;
        let (start, end) = significant[head_index];
        let candidate = &text[start..end];
        let callee_name = is_identifier(candidate).then_some(candidate);
        if callee_name.is_none() && !matches!(candidate, ")" | "]" | "}") {
            continue;
        }
        if callee_name.is_some_and(is_non_call_head) {
            continue;
        }
        if callee_name.is_some() && is_declaration_head(text, &significant, head_index) {
            continue;
        }
        let arguments = text.get(frame.open + 1..)?;
        return Some(CallContext {
            open: frame.open,
            callee_name,
            form: lexical_call_form(text, &significant, head_index),
            member_call: head_index
                .checked_sub(1)
                .and_then(|index| significant.get(index))
                .is_some_and(|&(start, end)| &text[start..end] == "."),
            active_argument: scan_active_argument(arguments),
        });
    }
    None
}

fn scan_active_argument(text: &str) -> ActiveArgument<'_> {
    let mut commas = 0;
    let mut frames = Vec::<char>::new();
    let mut first_significant = None;
    let mut named = false;
    let mut segment_start = 0;
    for (start, token) in Cursor::new(text).with_position() {
        let end = start + token.len as usize;
        let lexeme = &text[start..end];
        if token.kind.is_trivial() {
            continue;
        }
        if first_significant.is_none() {
            first_significant = Some(lexeme);
            named = lexeme == "{";
        }
        match lexeme {
            "(" | "[" | "{" => frames.push(lexeme.chars().next().unwrap()),
            ")" | "]" | "}" => {
                let expected = match lexeme {
                    ")" => '(',
                    "]" => '[',
                    "}" => '{',
                    _ => unreachable!(),
                };
                if let Some(index) = frames.iter().rposition(|&frame| frame == expected) {
                    frames.truncate(index);
                }
            }
            "," if frames.is_empty() || named && frames.as_slice() == ['{'] => {
                commas += 1;
                segment_start = end;
            }
            _ => {}
        }
    }
    let name = named.then(|| named_argument_name(text.get(segment_start..)?)).flatten();
    ActiveArgument { ordinal: commas, name }
}

fn named_argument_name(text: &str) -> Option<&str> {
    let mut tokens = Cursor::new(text)
        .with_position()
        .filter(|(_, token)| !token.kind.is_trivial())
        .map(|(start, token)| &text[start..start + token.len as usize]);
    let mut name = tokens.next()?;
    if name == "{" {
        name = tokens.next()?;
    }
    if is_identifier(name) && tokens.next() == Some(":") { Some(name) } else { None }
}

fn is_declaration_head(text: &str, significant: &[(usize, usize)], head_index: usize) -> bool {
    let Some(&(start, end)) = head_index.checked_sub(1).and_then(|index| significant.get(index))
    else {
        return false;
    };
    matches!(&text[start..end], "function" | "modifier" | "event" | "error")
}

fn lexical_call_form(text: &str, significant: &[(usize, usize)], head_index: usize) -> CallForm {
    let mut braces = 0usize;
    let mut brackets = 0usize;
    let mut parentheses = 0usize;
    let balance_parentheses = significant
        .get(head_index)
        .is_some_and(|&(start, end)| matches!(&text[start..end], ")" | "}"));
    for index in (0..=head_index).rev() {
        let (start, end) = significant[index];
        let token = &text[start..end];
        if token == "}" {
            braces += 1;
            continue;
        }
        if braces != 0 {
            if token == "{" {
                braces -= 1;
            }
            continue;
        }
        if token == "]" {
            brackets += 1;
            continue;
        }
        if brackets != 0 {
            if token == "[" {
                brackets -= 1;
            }
            continue;
        }
        if balance_parentheses && token == ")" {
            parentheses += 1;
            continue;
        }
        if parentheses != 0 {
            if token == "new" {
                return CallForm::New;
            }
            if token == "(" {
                parentheses -= 1;
            }
            continue;
        }
        if token == "new" {
            return CallForm::New;
        }
        if index != head_index && token == "emit" {
            return CallForm::Event;
        }
        if index != head_index && token == "revert" {
            return CallForm::Error;
        }
        if is_identifier(token) || token == "." || token.bytes().all(|byte| byte.is_ascii_digit()) {
            continue;
        }
        break;
    }
    CallForm::Regular
}

fn is_non_call_head(value: &str) -> bool {
    matches!(
        value,
        "if" | "for"
            | "while"
            | "catch"
            | "returns"
            | "function"
            | "modifier"
            | "event"
            | "error"
            | "constructor"
    )
}

fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    chars.next().is_some_and(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
}

fn range_size_key(range: Range) -> (u32, u32) {
    (
        range.end.line.saturating_sub(range.start.line),
        range.end.character.saturating_sub(range.start.character),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_signature(label: &str, parameter_names: Vec<Option<&str>>) -> CallSignature {
        CallSignature {
            information: SignatureInformation {
                label: label.into(),
                documentation: None,
                parameters: None,
                active_parameter: None,
            },
            parameter_names: parameter_names
                .into_iter()
                .map(|name| name.map(str::to_owned))
                .collect(),
            variadic: false,
        }
    }

    #[test]
    fn extend_reinterns_callsite_signatures() {
        let mut destination = SignatureHelpIndex::default();
        let canonical =
            destination.intern_signature(test_signature("function f(uint256)", vec![None]));
        let mut source = SignatureHelpIndex::default();
        let duplicate = source.intern_signature(test_signature("function f(uint256)", vec![None]));
        let uri = Url::parse("file:///Signature.sol").unwrap();
        source.calls.insert(
            uri.clone(),
            vec![CallSite {
                range: Range::default(),
                callee_range: Range::default(),
                callee_tokens: vec!["f".into()],
                form: CallForm::Regular,
                signatures: vec![duplicate],
            }],
        );

        destination.extend(source);

        assert!(Arc::ptr_eq(&canonical, &destination.calls[&uri][0].signatures[0]));
    }

    #[test]
    fn fallback_ranking_prefers_named_and_compatible_arity() {
        let short = test_signature("short", vec![Some("first")]);
        let long = test_signature("long", vec![Some("first"), Some("second")]);

        assert!(
            long.fallback_rank(&ActiveArgument { ordinal: 1, name: None })
                < short.fallback_rank(&ActiveArgument { ordinal: 1, name: None })
        );
        assert!(
            long.fallback_rank(&ActiveArgument { ordinal: 0, name: Some("second") })
                < short.fallback_rank(&ActiveArgument { ordinal: 0, name: Some("second") })
        );
    }

    #[test]
    fn lexical_call_form_distinguishes_event_and_error_invocations() {
        let event = call_context("emit   E(").unwrap();
        let error = call_context("revert E(").unwrap();
        let builtin_revert = call_context("revert(").unwrap();

        assert_eq!(event.form, CallForm::Event);
        assert_eq!(error.form, CallForm::Error);
        assert_eq!(builtin_revert.form, CallForm::Regular);
    }

    #[test]
    fn successful_analysis_does_not_retain_stale_catalog_entries() {
        let mut previous = SignatureHelpIndex::default();
        previous.push_callable(
            "removed".into(),
            Some(Url::parse("file:///Signature.sol").unwrap()),
            CallForm::Regular,
            test_signature("function removed(uint256)", vec![None]),
        );
        let mut current = SignatureHelpIndex::default();

        current.retain_failed_files(&previous, &[]);

        assert!(!current.callables_by_name.contains_key("removed"));
    }

    #[test]
    fn stale_callee_range_splitting_a_surrogate_pair_is_rejected() {
        let call = CallSite {
            range: Range::default(),
            callee_range: Range::new(Position::new(0, 1), Position::new(0, 3)),
            callee_tokens: vec!["f".into()],
            form: CallForm::Regular,
            signatures: Vec::new(),
        };

        assert!(!call.matches_current_callee(&Rope::from("😀f")));
    }

    #[test]
    fn stale_callee_range_beyond_the_current_file_is_rejected() {
        let call = CallSite {
            range: Range::default(),
            callee_range: Range::new(Position::new(2, 0), Position::new(2, 1)),
            callee_tokens: vec!["f".into()],
            form: CallForm::Regular,
            signatures: Vec::new(),
        };

        assert!(!call.matches_current_callee(&Rope::from("f")));
    }

    #[test]
    fn failed_file_retention_merges_missing_call_sites() {
        let uri = Url::parse("file:///Signature.sol").unwrap();
        let mut current = SignatureHelpIndex::default();
        let signature = current.intern_signature(test_signature("function f(uint256)", vec![None]));
        let first = CallSite {
            range: Range::new(Position::new(1, 1), Position::new(1, 4)),
            callee_range: Range::new(Position::new(1, 0), Position::new(1, 1)),
            callee_tokens: vec!["f".into()],
            form: CallForm::Regular,
            signatures: vec![signature.clone()],
        };
        let second = CallSite {
            range: Range::new(Position::new(2, 1), Position::new(2, 4)),
            callee_range: Range::new(Position::new(2, 0), Position::new(2, 1)),
            callee_tokens: vec!["f".into()],
            form: CallForm::Regular,
            signatures: vec![signature],
        };
        let mut previous = SignatureHelpIndex::default();
        previous.calls.insert(uri.clone(), vec![first.clone(), second]);
        current.calls.insert(uri.clone(), vec![first]);

        current.retain_failed_files(&previous, std::slice::from_ref(&uri));

        assert_eq!(current.calls[&uri].len(), 2);
    }
}
