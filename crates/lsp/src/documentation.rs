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
    pub(crate) fn signature(signature: String) -> Self {
        Self {
            markdown: MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("```solidity\n{signature}\n```"),
            },
            plain_text: signature,
        }
    }

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

#[derive(Default)]
struct NatSpecDocumentation {
    notice: Vec<String>,
    dev: Vec<String>,
    params: Vec<(Symbol, String)>,
    returns: Vec<(Option<Symbol>, String)>,
}

fn documentation(gcx: Gcx<'_>, item_id: hir::ItemId) -> NatSpecDocumentation {
    if let hir::ItemId::Variable(id) = item_id {
        return variable_documentation(gcx, id);
    }
    let item = gcx.hir.item(item_id);
    if item.doc().is_empty() {
        return NatSpecDocumentation::default();
    }
    let view = gcx.natspec_view(item_id);
    let mut documentation = item_documentation(view.items());
    if matches!(item_id, hir::ItemId::Function(_) | hir::ItemId::Event(_) | hir::ItemId::Error(_)) {
        documentation.params = item
            .parameters()
            .unwrap_or_default()
            .iter()
            .enumerate()
            .filter_map(|(index, &id)| parameter_doc_at(gcx, id, index, view))
            .collect();
        let returns = item_id.as_function().map_or(&[][..], |id| gcx.hir.function(id).returns);
        documentation.returns = return_docs(gcx, returns, view);
    }
    documentation
}

fn variable_documentation(gcx: Gcx<'_>, id: hir::VariableId) -> NatSpecDocumentation {
    let variable = gcx.hir.variable(id);
    match (variable.kind, variable.parent) {
        (hir::VarKind::FunctionParam, Some(parent @ hir::ItemId::Function(_)))
        | (hir::VarKind::Event, Some(parent @ hir::ItemId::Event(_)))
        | (hir::VarKind::Error, Some(parent @ hir::ItemId::Error(_))) => {
            let parameters = gcx.hir.item(parent).parameters().unwrap_or_default();
            selected_documentation(gcx, id, parent, parameters, false)
        }
        (hir::VarKind::FunctionReturn, Some(parent @ hir::ItemId::Function(function))) => {
            selected_documentation(gcx, id, parent, gcx.hir.function(function).returns, true)
        }
        (hir::VarKind::FunctionTyParam(_) | hir::VarKind::FunctionTyReturn(_), _) => {
            NatSpecDocumentation::default()
        }
        _ if variable.doc.is_empty() => NatSpecDocumentation::default(),
        _ => {
            let view = gcx.natspec_view(hir::ItemId::Variable(id));
            let items = view.items();
            let mut documentation = item_documentation(items);
            documentation.returns = match variable.getter {
                Some(getter) => return_docs(gcx, gcx.hir.function(getter).returns, view),
                None => return_documentation(items),
            };
            documentation
        }
    }
}

/// Documents the parameter or return variable `id` of `item_id` at its position in `variables`.
fn selected_documentation(
    gcx: Gcx<'_>,
    id: hir::VariableId,
    item_id: hir::ItemId,
    variables: &[hir::VariableId],
    returns: bool,
) -> NatSpecDocumentation {
    let Some(index) = variables.iter().position(|&variable| variable == id) else {
        return NatSpecDocumentation::default();
    };
    if gcx.hir.item(item_id).doc().is_empty() {
        return NatSpecDocumentation::default();
    }
    let view = gcx.natspec_view(item_id);
    if returns {
        let returns = return_doc_at(gcx, id, index, view).into_iter().collect();
        NatSpecDocumentation { returns, ..NatSpecDocumentation::default() }
    } else {
        let params = parameter_doc_at(gcx, id, index, view).into_iter().collect();
        NatSpecDocumentation { params, ..NatSpecDocumentation::default() }
    }
}

fn item_documentation(items: &[hir::NatSpecItem]) -> NatSpecDocumentation {
    let mut documentation = NatSpecDocumentation::default();
    for item in items {
        let Some(content) = item_content(item) else { continue };
        match item.kind {
            hir::NatSpecKind::Notice => documentation.notice.push(content.to_string()),
            hir::NatSpecKind::Dev => documentation.dev.push(content.to_string()),
            hir::NatSpecKind::Return { .. }
            | hir::NatSpecKind::Title
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

fn return_docs(
    gcx: Gcx<'_>,
    returns: &[hir::VariableId],
    documentation: NatSpecView<'_>,
) -> Vec<(Option<Symbol>, String)> {
    returns
        .iter()
        .enumerate()
        .filter_map(|(index, &id)| return_doc_at(gcx, id, index, documentation))
        .collect()
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

fn join_docs<'a>(docs: impl Iterator<Item = &'a str>) -> Option<String> {
    let docs = docs.collect::<Vec<_>>();
    (!docs.is_empty()).then(|| docs.join("\n\n"))
}

fn append_documentation(output: &mut String, documentation: &NatSpecDocumentation, markdown: bool) {
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
        documentation.params.iter().map(|(name, content)| (Some(name.as_str()), content.as_str())),
        markdown,
    );
    append_list(
        output,
        "@return",
        documentation
            .returns
            .iter()
            .map(|(name, content)| (name.as_ref().map(|name| name.as_str()), content.as_str())),
        markdown,
    );
}

fn append_list<'a>(
    output: &mut String,
    heading: &str,
    items: impl Iterator<Item = (Option<&'a str>, &'a str)>,
    markdown: bool,
) {
    let mut items = items.peekable();
    if items.peek().is_none() {
        return;
    }
    let emphasis = if markdown { "**" } else { "" };
    write!(output, "\n\n{emphasis}{heading}{emphasis}").unwrap();
    for (name, content) in items {
        output.push_str(if markdown { "\n\n- " } else { "\n\n" });
        if let Some(name) = name {
            let quote = if markdown { "`" } else { "" };
            write!(output, "{quote}{name}{quote}: ").unwrap();
        }
        let mut lines = content.lines();
        output.push_str(lines.next().unwrap_or_default());
        for line in lines {
            output.push_str("\n  ");
            output.push_str(line);
        }
    }
}
