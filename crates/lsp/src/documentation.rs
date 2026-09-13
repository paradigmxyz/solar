//! Resolves and renders NatSpec documentation for LSP responses.
//!
//! Borrow resolved compiler text while assembling the output formats. Only the finished strings
//! escape analysis, so hover and completion requests need no compiler context or deferred
//! formatting. Intermediate single-tag sections stay inline to avoid temporary allocations.

use lsp_types::{Documentation as LspDocumentation, MarkupContent, MarkupKind};
use solar_interface::{Symbol, data_structures::smallvec::SmallVec};
use solar_sema::{
    Gcx,
    hir::{self, HirPrinter},
    ty::NatSpecView,
};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ResolvedDocumentation {
    markdown: MarkupContent,
    plain_text: String,
}

impl ResolvedDocumentation {
    pub(crate) fn hover(&self) -> MarkupContent {
        self.markdown.clone()
    }

    pub(crate) fn completion(&self, markdown: bool) -> LspDocumentation {
        if markdown {
            LspDocumentation::MarkupContent(self.markdown.clone())
        } else {
            LspDocumentation::String(self.plain_text.clone())
        }
    }

    pub(crate) fn renders_identically(&self, other: &Self) -> bool {
        self == other
    }
}

pub(crate) fn resolve(gcx: Gcx<'_>, item_id: hir::ItemId) -> ResolvedDocumentation {
    let signature = HirPrinter::display(gcx, item_id).to_string();
    let documentation = documentation(gcx, item_id);
    let mut markdown = format!("```solidity\n{signature}\n```");
    append_documentation(&mut markdown, &documentation, true);
    let mut plain_text = signature;
    append_documentation(&mut plain_text, &documentation, false);
    ResolvedDocumentation {
        markdown: MarkupContent { kind: MarkupKind::Markdown, value: markdown },
        plain_text,
    }
}

// These borrowed sections only live during `resolve`. Keep the common single-tag case inline;
// published responses own their rendered strings and never retain compiler-owned symbols or text.
#[derive(Default)]
struct NatSpecDocumentation<'a> {
    notice: SmallVec<[&'a str; 1]>,
    dev: SmallVec<[&'a str; 1]>,
    params: SmallVec<[(Symbol, &'a [hir::NatSpecItem]); 1]>,
    returns: SmallVec<[(Option<Symbol>, &'a [hir::NatSpecItem]); 1]>,
}

fn documentation(gcx: Gcx<'_>, item_id: hir::ItemId) -> NatSpecDocumentation<'_> {
    match item_id {
        hir::ItemId::Contract(id) => {
            let contract = gcx.hir.contract(id);
            if contract.doc.is_empty() {
                NatSpecDocumentation::default()
            } else {
                item_documentation(gcx.natspec_view(item_id).items())
            }
        }
        hir::ItemId::Function(id) => {
            let function = gcx.hir.function(id);
            callable_documentation(
                gcx,
                hir::ItemId::Function(id),
                function.doc,
                function.parameters,
                function.returns,
            )
        }
        hir::ItemId::Variable(id) => variable_documentation(gcx, id),
        hir::ItemId::Event(id) => {
            let event = gcx.hir.event(id);
            callable_documentation(gcx, hir::ItemId::Event(id), event.doc, event.parameters, &[])
        }
        hir::ItemId::Error(id) => {
            let error = gcx.hir.error(id);
            callable_documentation(gcx, hir::ItemId::Error(id), error.doc, error.parameters, &[])
        }
        hir::ItemId::Struct(_) | hir::ItemId::Enum(_) | hir::ItemId::Udvt(_) => {
            let doc = gcx.hir.item(item_id).doc();
            if doc.is_empty() {
                NatSpecDocumentation::default()
            } else {
                item_documentation(gcx.natspec_view(item_id).items())
            }
        }
    }
}

fn callable_documentation<'gcx>(
    gcx: Gcx<'gcx>,
    item_id: hir::ItemId,
    doc_id: hir::DocId,
    parameters: &[hir::VariableId],
    returns: &[hir::VariableId],
) -> NatSpecDocumentation<'gcx> {
    if doc_id.is_empty() {
        return NatSpecDocumentation::default();
    }
    let view = gcx.natspec_view(item_id);
    let mut documentation = item_documentation(view.items());
    let params = parameters
        .iter()
        .enumerate()
        .filter_map(|(index, &id)| parameter_doc_at(gcx, id, index, view))
        .collect();
    let returns = returns
        .iter()
        .enumerate()
        .filter_map(|(index, &id)| return_doc_at(gcx, id, index, view))
        .collect();
    documentation.params = params;
    documentation.returns = returns;
    documentation
}

fn variable_documentation(gcx: Gcx<'_>, id: hir::VariableId) -> NatSpecDocumentation<'_> {
    let variable = gcx.hir.variable(id);
    match (variable.kind, variable.parent) {
        (hir::VarKind::FunctionParam, Some(hir::ItemId::Function(parent))) => {
            let function = gcx.hir.function(parent);
            selected_parameter_documentation(
                gcx,
                id,
                hir::ItemId::Function(parent),
                function.parameters,
            )
        }
        (hir::VarKind::FunctionReturn, Some(hir::ItemId::Function(parent))) => {
            let function = gcx.hir.function(parent);
            selected_return_documentation(gcx, id, hir::ItemId::Function(parent), function.returns)
        }
        (hir::VarKind::Event, Some(hir::ItemId::Event(parent))) => {
            let event = gcx.hir.event(parent);
            selected_parameter_documentation(gcx, id, hir::ItemId::Event(parent), event.parameters)
        }
        (hir::VarKind::Error, Some(hir::ItemId::Error(parent))) => {
            let error = gcx.hir.error(parent);
            selected_parameter_documentation(gcx, id, hir::ItemId::Error(parent), error.parameters)
        }
        (hir::VarKind::FunctionTyParam | hir::VarKind::FunctionTyReturn, _) => {
            NatSpecDocumentation::default()
        }
        _ if variable.doc.is_empty() => NatSpecDocumentation::default(),
        _ => {
            let view = gcx.natspec_view(hir::ItemId::Variable(id));
            let items = view.items();
            let mut documentation = item_documentation(items);
            if let Some(getter) = variable.getter {
                let returns = gcx.hir.function(getter).returns;
                documentation.returns = returns
                    .iter()
                    .enumerate()
                    .filter_map(|(index, &id)| return_doc_at(gcx, id, index, view))
                    .collect();
            } else {
                documentation.returns = return_documentation(items);
            }
            documentation
        }
    }
}

fn selected_parameter_documentation<'gcx>(
    gcx: Gcx<'gcx>,
    id: hir::VariableId,
    item_id: hir::ItemId,
    parameters: &[hir::VariableId],
) -> NatSpecDocumentation<'gcx> {
    let Some(index) = parameters.iter().position(|&parameter| parameter == id) else {
        return NatSpecDocumentation::default();
    };
    if gcx.hir.item(item_id).doc().is_empty() {
        return NatSpecDocumentation::default();
    }
    let view = gcx.natspec_view(item_id);
    let params = parameter_doc_at(gcx, id, index, view).into_iter().collect();
    NatSpecDocumentation { params, ..NatSpecDocumentation::default() }
}

fn selected_return_documentation<'gcx>(
    gcx: Gcx<'gcx>,
    id: hir::VariableId,
    item_id: hir::ItemId,
    returns: &[hir::VariableId],
) -> NatSpecDocumentation<'gcx> {
    let Some(index) = returns.iter().position(|&return_id| return_id == id) else {
        return NatSpecDocumentation::default();
    };
    if gcx.hir.item(item_id).doc().is_empty() {
        return NatSpecDocumentation::default();
    }
    let view = gcx.natspec_view(item_id);
    let returns = return_doc_at(gcx, id, index, view).into_iter().collect();
    NatSpecDocumentation { returns, ..NatSpecDocumentation::default() }
}

fn item_documentation(items: &[hir::NatSpecItem]) -> NatSpecDocumentation<'_> {
    let mut documentation = NatSpecDocumentation::default();
    for item in items {
        let Some(content) = item_content(item) else { continue };
        match item.kind {
            hir::NatSpecKind::Notice => documentation.notice.push(content),
            hir::NatSpecKind::Dev => documentation.dev.push(content),
            hir::NatSpecKind::Return { .. } => {}
            hir::NatSpecKind::Title
            | hir::NatSpecKind::Author
            | hir::NatSpecKind::Param { .. }
            | hir::NatSpecKind::Inheritdoc { .. }
            | hir::NatSpecKind::Custom { .. }
            | hir::NatSpecKind::Internal { .. } => {}
        }
    }
    documentation
}

fn return_documentation(
    items: &[hir::NatSpecItem],
) -> SmallVec<[(Option<Symbol>, &[hir::NatSpecItem]); 1]> {
    items
        .iter()
        .filter_map(|item| {
            let hir::NatSpecKind::Return { name } = item.kind else { return None };
            item_content(item)?;
            Some((name.map(|name| name.name), std::slice::from_ref(item)))
        })
        .collect()
}

fn parameter_doc_at<'gcx>(
    gcx: Gcx<'_>,
    id: hir::VariableId,
    index: usize,
    documentation: NatSpecView<'gcx>,
) -> Option<(Symbol, &'gcx [hir::NatSpecItem])> {
    let content = documentation.parameter(index);
    content.iter().find_map(item_content)?;
    let name = gcx.hir.variable(id).name?.name;
    Some((name, content))
}

fn return_doc_at<'gcx>(
    gcx: Gcx<'_>,
    id: hir::VariableId,
    index: usize,
    documentation: NatSpecView<'gcx>,
) -> Option<(Option<Symbol>, &'gcx [hir::NatSpecItem])> {
    let content = documentation.return_(index);
    content.iter().find_map(item_content)?;
    let name = gcx.hir.variable(id).name.map(|name| name.name);
    Some((name, content))
}

fn item_content(item: &hir::NatSpecItem) -> Option<&str> {
    let content = item.content().trim();
    (!content.is_empty()).then_some(content)
}

fn append_documentation(
    output: &mut String,
    documentation: &NatSpecDocumentation<'_>,
    markdown: bool,
) {
    for notice in &documentation.notice {
        output.push_str("\n\n");
        output.push_str(notice);
    }
    if !documentation.dev.is_empty() {
        output.push_str(if markdown { "\n\n**@dev**" } else { "\n\n@dev" });
        for dev in &documentation.dev {
            output.push_str("\n\n");
            output.push_str(dev);
        }
    }
    append_list(
        output,
        "@param",
        documentation.params.iter().map(|(name, content)| (Some(name.as_str()), *content)),
        markdown,
    );
    append_list(
        output,
        "@return",
        documentation
            .returns
            .iter()
            .map(|(name, content)| (name.as_ref().map(|name| name.as_str()), *content)),
        markdown,
    );
}

fn append_list<'a>(
    output: &mut String,
    heading: &str,
    items: impl Iterator<Item = (Option<&'a str>, &'a [hir::NatSpecItem])>,
    markdown: bool,
) {
    let mut items = items.peekable();
    if items.peek().is_none() {
        return;
    }
    let emphasis = if markdown { "**" } else { "" };
    output.push_str("\n\n");
    output.push_str(emphasis);
    output.push_str(heading);
    output.push_str(emphasis);
    for (name, content) in items {
        output.push_str(if markdown { "\n\n- " } else { "\n\n" });
        if let Some(name) = name {
            let quote = if markdown { "`" } else { "" };
            output.push_str(quote);
            output.push_str(name);
            output.push_str(quote);
            output.push_str(": ");
        }
        // Render joined paragraphs directly from compiler-owned text. Each additional tag
        // contributes the same indented blank line as joining with `\n\n` before rendering.
        for (index, content) in content.iter().filter_map(item_content).enumerate() {
            if index > 0 {
                output.push_str("\n  \n  ");
            }
            let mut lines = content.lines();
            output.push_str(lines.next().unwrap_or_default());
            for line in lines {
                output.push_str("\n  ");
                output.push_str(line);
            }
        }
    }
}
