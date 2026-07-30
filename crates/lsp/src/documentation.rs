//! Resolves and renders NatSpec documentation for LSP responses.

use lsp_types::{Documentation as LspDocumentation, MarkupContent, MarkupKind};
use solar_interface::Symbol;
use solar_sema::{
    Gcx,
    hir::{self, HirPrinter},
    ty::NatSpecView,
};
use std::fmt::Write;

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
    append_markdown_documentation(&mut markdown, &documentation);
    let mut plain_text = signature;
    append_plain_documentation(&mut plain_text, &documentation);
    ResolvedDocumentation {
        markdown: MarkupContent { kind: MarkupKind::Markdown, value: markdown },
        plain_text,
    }
}

#[derive(Default)]
struct NatSpecDocumentation {
    notice: Vec<String>,
    dev: Vec<String>,
    params: Vec<(Symbol, String)>,
    returns: Vec<(Option<Symbol>, String)>,
}

fn documentation(gcx: Gcx<'_>, item_id: hir::ItemId) -> NatSpecDocumentation {
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

fn callable_documentation(
    gcx: Gcx<'_>,
    item_id: hir::ItemId,
    doc_id: hir::DocId,
    parameters: &[hir::VariableId],
    returns: &[hir::VariableId],
) -> NatSpecDocumentation {
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

fn variable_documentation(gcx: Gcx<'_>, id: hir::VariableId) -> NatSpecDocumentation {
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

fn selected_parameter_documentation(
    gcx: Gcx<'_>,
    id: hir::VariableId,
    item_id: hir::ItemId,
    parameters: &[hir::VariableId],
) -> NatSpecDocumentation {
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

fn selected_return_documentation(
    gcx: Gcx<'_>,
    id: hir::VariableId,
    item_id: hir::ItemId,
    returns: &[hir::VariableId],
) -> NatSpecDocumentation {
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

fn item_documentation(items: &[hir::NatSpecItem]) -> NatSpecDocumentation {
    let mut documentation = NatSpecDocumentation::default();
    for item in items {
        let Some(content) = item_content(item) else { continue };
        match item.kind {
            hir::NatSpecKind::Notice => documentation.notice.push(content.to_string()),
            hir::NatSpecKind::Dev => documentation.dev.push(content.to_string()),
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

fn return_documentation(items: &[hir::NatSpecItem]) -> Vec<(Option<Symbol>, String)> {
    items
        .iter()
        .filter_map(|item| {
            let hir::NatSpecKind::Return { name } = item.kind else { return None };
            let content = item_content(item)?;
            Some((name.map(|name| name.name), content.to_string()))
        })
        .collect()
}

fn parameter_doc_at(
    gcx: Gcx<'_>,
    id: hir::VariableId,
    index: usize,
    documentation: NatSpecView<'_>,
) -> Option<(Symbol, String)> {
    let content = join_docs(documentation.parameter(index).iter().filter_map(item_content))?;
    let name = gcx.hir.variable(id).name?.name;
    Some((name, content))
}

fn return_doc_at(
    gcx: Gcx<'_>,
    id: hir::VariableId,
    index: usize,
    documentation: NatSpecView<'_>,
) -> Option<(Option<Symbol>, String)> {
    let content = join_docs(documentation.return_(index).iter().filter_map(item_content))?;
    let name = gcx.hir.variable(id).name.map(|name| name.name);
    Some((name, content))
}

fn item_content(item: &hir::NatSpecItem) -> Option<&str> {
    let content = item.content().trim();
    (!content.is_empty()).then_some(content)
}

fn join_docs<'a>(mut docs: impl Iterator<Item = &'a str>) -> Option<String> {
    let first = docs.next()?;
    let mut joined = first.to_string();
    for doc in docs {
        joined.push_str("\n\n");
        joined.push_str(doc);
    }
    Some(joined)
}

fn append_markdown_documentation(output: &mut String, documentation: &NatSpecDocumentation) {
    for notice in &documentation.notice {
        output.push_str("\n\n");
        output.push_str(notice);
    }
    if !documentation.dev.is_empty() {
        output.push_str("\n\n**@dev**\n\n");
        for (index, dev) in documentation.dev.iter().enumerate() {
            if index != 0 {
                output.push_str("\n\n");
            }
            output.push_str(dev);
        }
    }
    append_list(
        output,
        "@param",
        documentation.params.iter().map(|(name, content)| (Some(name.as_str()), content.as_str())),
    );
    append_list(
        output,
        "@return",
        documentation
            .returns
            .iter()
            .map(|(name, content)| (name.as_ref().map(|name| name.as_str()), content.as_str())),
    );
}

fn append_plain_documentation(output: &mut String, documentation: &NatSpecDocumentation) {
    for notice in &documentation.notice {
        output.push_str("\n\n");
        output.push_str(notice);
    }
    if !documentation.dev.is_empty() {
        output.push_str("\n\n@dev");
        for dev in &documentation.dev {
            output.push_str("\n\n");
            output.push_str(dev);
        }
    }
    append_plain_list(
        output,
        "@param",
        documentation.params.iter().map(|(name, content)| (Some(name.as_str()), content.as_str())),
    );
    append_plain_list(
        output,
        "@return",
        documentation
            .returns
            .iter()
            .map(|(name, content)| (name.as_ref().map(|name| name.as_str()), content.as_str())),
    );
}

fn append_plain_list<'a>(
    output: &mut String,
    heading: &str,
    items: impl Iterator<Item = (Option<&'a str>, &'a str)>,
) {
    let mut items = items.peekable();
    if items.peek().is_none() {
        return;
    }
    write!(output, "\n\n{heading}").unwrap();
    for (name, content) in items {
        output.push_str("\n\n");
        if let Some(name) = name {
            write!(output, "{name}: ").unwrap();
        }
        let mut lines = content.lines();
        output.push_str(lines.next().unwrap_or_default());
        for line in lines {
            output.push_str("\n  ");
            output.push_str(line);
        }
    }
}

fn append_list<'a>(
    output: &mut String,
    heading: &str,
    items: impl Iterator<Item = (Option<&'a str>, &'a str)>,
) {
    let mut items = items.peekable();
    if items.peek().is_none() {
        return;
    }
    write!(output, "\n\n**{heading}**").unwrap();
    for (name, content) in items {
        output.push_str("\n\n- ");
        if let Some(name) = name {
            write!(output, "`{name}`: ").unwrap();
        }
        let mut lines = content.lines();
        output.push_str(lines.next().unwrap_or_default());
        for line in lines {
            output.push_str("\n  ");
            output.push_str(line);
        }
    }
}
