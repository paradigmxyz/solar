//! Renders Solidity declarations and resolved NatSpec for LSP hover responses.

use lsp_types::{MarkupContent, MarkupKind};
use solar_sema::{Gcx, hir, ty::NatSpecView};
use std::fmt::Write;

pub(crate) fn render(gcx: Gcx<'_>, item_id: hir::ItemId) -> Option<MarkupContent> {
    let signature = match item_id {
        hir::ItemId::Function(id) => render_function(gcx, id),
        hir::ItemId::Variable(id) => render_variable(gcx, id),
        hir::ItemId::Event(id) => render_event(gcx, id),
        hir::ItemId::Error(id) => render_error(gcx, id),
        hir::ItemId::Contract(_)
        | hir::ItemId::Struct(_)
        | hir::ItemId::Enum(_)
        | hir::ItemId::Udvt(_) => None,
    }?;
    let mut value = format!("```solidity\n{signature}\n```");
    append_documentation(&mut value, &documentation(gcx, item_id));
    Some(MarkupContent { kind: MarkupKind::Markdown, value })
}

#[derive(Default)]
struct Documentation {
    notice: Vec<String>,
    dev: Vec<String>,
    params: Vec<(String, String)>,
    returns: Vec<(Option<String>, String)>,
}

fn documentation(gcx: Gcx<'_>, item_id: hir::ItemId) -> Documentation {
    match item_id {
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
        hir::ItemId::Contract(_)
        | hir::ItemId::Struct(_)
        | hir::ItemId::Enum(_)
        | hir::ItemId::Udvt(_) => Documentation::default(),
    }
}

fn callable_documentation(
    gcx: Gcx<'_>,
    item_id: hir::ItemId,
    doc_id: hir::DocId,
    parameters: &[hir::VariableId],
    returns: &[hir::VariableId],
) -> Documentation {
    if doc_id.is_empty() {
        return Documentation::default();
    }
    let view = gcx.natspec_view(item_id);
    let mut documentation = item_documentation(view.items());
    let params = parameters
        .iter()
        .enumerate()
        .filter_map(|(index, _)| parameter_doc_at(gcx, parameters, index, view))
        .collect();
    let returns = returns
        .iter()
        .enumerate()
        .filter_map(|(index, _)| return_doc_at(gcx, returns, index, view))
        .collect();
    documentation.params = params;
    documentation.returns = returns;
    documentation
}

fn variable_documentation(gcx: Gcx<'_>, id: hir::VariableId) -> Documentation {
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
            Documentation::default()
        }
        _ if variable.doc.is_empty() => Documentation::default(),
        _ => {
            let view = gcx.natspec_view(hir::ItemId::Variable(id));
            let items = view.items();
            let mut documentation = item_documentation(items);
            if let Some(getter) = variable.getter {
                let returns = gcx.hir.function(getter).returns;
                documentation.returns = returns
                    .iter()
                    .enumerate()
                    .filter_map(|(index, _)| return_doc_at(gcx, returns, index, view))
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
) -> Documentation {
    let Some(index) = parameters.iter().position(|&parameter| parameter == id) else {
        return Documentation::default();
    };
    if gcx.hir.item(item_id).doc().is_empty() {
        return Documentation::default();
    }
    let view = gcx.natspec_view(item_id);
    let params = parameter_doc_at(gcx, parameters, index, view).into_iter().collect();
    Documentation { params, ..Documentation::default() }
}

fn selected_return_documentation(
    gcx: Gcx<'_>,
    id: hir::VariableId,
    item_id: hir::ItemId,
    returns: &[hir::VariableId],
) -> Documentation {
    let Some(index) = returns.iter().position(|&return_id| return_id == id) else {
        return Documentation::default();
    };
    if gcx.hir.item(item_id).doc().is_empty() {
        return Documentation::default();
    }
    let view = gcx.natspec_view(item_id);
    let returns = return_doc_at(gcx, returns, index, view).into_iter().collect();
    Documentation { returns, ..Documentation::default() }
}

fn item_documentation(items: &[hir::NatSpecItem]) -> Documentation {
    let mut documentation = Documentation::default();
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

fn return_documentation(items: &[hir::NatSpecItem]) -> Vec<(Option<String>, String)> {
    items
        .iter()
        .filter_map(|item| {
            let hir::NatSpecKind::Return { name } = item.kind else { return None };
            let content = item_content(item)?;
            Some((name.map(|name| name.to_string()), content.to_string()))
        })
        .collect()
}

fn parameter_doc_at(
    gcx: Gcx<'_>,
    parameters: &[hir::VariableId],
    index: usize,
    documentation: NatSpecView<'_>,
) -> Option<(String, String)> {
    let content = join_docs(documentation.parameter(index).iter().filter_map(item_content))?;
    let name = gcx.hir.variable(*parameters.get(index)?).name?.to_string();
    Some((name, content))
}

fn return_doc_at(
    gcx: Gcx<'_>,
    returns: &[hir::VariableId],
    index: usize,
    documentation: NatSpecView<'_>,
) -> Option<(Option<String>, String)> {
    let content = join_docs(documentation.return_(index).iter().filter_map(item_content))?;
    let name = gcx.hir.variable(*returns.get(index)?).name.map(|name| name.to_string());
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

fn append_documentation(output: &mut String, documentation: &Documentation) {
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
        documentation.returns.iter().map(|(name, content)| (name.as_deref(), content.as_str())),
    );
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

fn render_function(gcx: Gcx<'_>, id: hir::FunctionId) -> Option<String> {
    let function = gcx.hir.function(id);
    if function.is_yul {
        return None;
    }

    let mut signature = function.kind.to_string();
    if let Some(name) = function.name {
        write!(signature, " {name}").ok()?;
    }
    signature.push('(');
    render_variables(gcx, function.parameters, &mut signature)?;
    signature.push(')');

    match function.kind {
        hir::FunctionKind::Function => {
            write!(signature, " {}", function.visibility).ok()?;
            render_state_mutability(function.state_mutability, &mut signature)?;
        }
        hir::FunctionKind::Constructor => {
            if function.state_mutability == hir::StateMutability::Payable {
                signature.push_str(" payable");
            }
        }
        hir::FunctionKind::Fallback | hir::FunctionKind::Receive => {
            write!(signature, " {}", function.visibility).ok()?;
            render_state_mutability(function.state_mutability, &mut signature)?;
        }
        hir::FunctionKind::Modifier => {}
    }

    if function.marked_virtual {
        signature.push_str(" virtual");
    }
    if function.override_ {
        signature.push_str(" override");
        render_override_list(gcx, function.overrides, &mut signature)?;
    }
    for modifier in function.modifiers {
        let name = gcx.item_name_opt(modifier.id)?;
        write!(signature, " {name}").ok()?;
    }
    if !function.returns.is_empty() {
        signature.push_str(" returns (");
        render_variables(gcx, function.returns, &mut signature)?;
        signature.push(')');
    }
    Some(signature)
}

fn render_variable(gcx: Gcx<'_>, id: hir::VariableId) -> Option<String> {
    let variable = gcx.hir.variable(id);
    let mut signature = String::new();
    render_type(gcx, &variable.ty, &mut signature)?;
    if let Some(visibility) = variable.visibility {
        write!(signature, " {visibility}").ok()?;
    }
    if let Some(mutability) = variable.mutability {
        write!(signature, " {mutability}").ok()?;
    }
    if let Some(data_location) = variable.data_location {
        write!(signature, " {data_location}").ok()?;
    }
    if variable.indexed {
        signature.push_str(" indexed");
    }
    let name = variable.name?;
    write!(signature, " {name}").ok()?;
    Some(signature)
}

fn render_event(gcx: Gcx<'_>, id: hir::EventId) -> Option<String> {
    let event = gcx.hir.event(id);
    let mut signature = format!("event {}(", event.name);
    render_variables(gcx, event.parameters, &mut signature)?;
    signature.push(')');
    if event.anonymous {
        signature.push_str(" anonymous");
    }
    Some(signature)
}

fn render_error(gcx: Gcx<'_>, id: hir::ErrorId) -> Option<String> {
    let error = gcx.hir.error(id);
    let mut signature = format!("error {}(", error.name);
    render_variables(gcx, error.parameters, &mut signature)?;
    signature.push(')');
    Some(signature)
}

fn render_variables(
    gcx: Gcx<'_>,
    variables: &[hir::VariableId],
    output: &mut String,
) -> Option<()> {
    for (index, &id) in variables.iter().enumerate() {
        if index != 0 {
            output.push_str(", ");
        }
        let variable = gcx.hir.variable(id);
        render_type(gcx, &variable.ty, output)?;
        if let Some(data_location) = variable.data_location {
            write!(output, " {data_location}").ok()?;
        }
        if variable.indexed {
            output.push_str(" indexed");
        }
        if let Some(name) = variable.name {
            write!(output, " {name}").ok()?;
        }
    }
    Some(())
}

fn render_type(gcx: Gcx<'_>, ty: &hir::Type<'_>, output: &mut String) -> Option<()> {
    match &ty.kind {
        hir::TypeKind::Elementary(elementary) => write!(output, "{elementary}").ok()?,
        hir::TypeKind::Array(array) => {
            render_type(gcx, &array.element, output)?;
            output.push('[');
            if let Some(size) = array.size {
                let size = gcx.sess.source_map().span_to_snippet(size.span).ok()?;
                output.push_str(size.trim());
            }
            output.push(']');
        }
        hir::TypeKind::Function(function) => {
            output.push_str("function(");
            render_variables(gcx, function.parameters, output)?;
            write!(output, ") {}", function.visibility).ok()?;
            render_state_mutability(function.state_mutability, output)?;
            if !function.returns.is_empty() {
                output.push_str(" returns (");
                render_variables(gcx, function.returns, output)?;
                output.push(')');
            }
        }
        hir::TypeKind::Mapping(mapping) => {
            output.push_str("mapping(");
            render_type(gcx, &mapping.key, output)?;
            if let Some(name) = mapping.key_name {
                write!(output, " {name}").ok()?;
            }
            output.push_str(" => ");
            render_type(gcx, &mapping.value, output)?;
            if let Some(name) = mapping.value_name {
                write!(output, " {name}").ok()?;
            }
            output.push(')');
        }
        hir::TypeKind::Custom(item_id) => {
            let name = gcx.item_name_opt(*item_id)?;
            write!(output, "{name}").ok()?;
        }
        hir::TypeKind::Err(_) => return None,
    }
    Some(())
}

fn render_state_mutability(
    state_mutability: hir::StateMutability,
    output: &mut String,
) -> Option<()> {
    if state_mutability != hir::StateMutability::NonPayable {
        write!(output, " {state_mutability}").ok()?;
    }
    Some(())
}

fn render_override_list(
    gcx: Gcx<'_>,
    overrides: &[hir::ContractId],
    output: &mut String,
) -> Option<()> {
    if overrides.is_empty() {
        return Some(());
    }
    output.push('(');
    for (index, &contract) in overrides.iter().enumerate() {
        if index != 0 {
            output.push_str(", ");
        }
        let name = gcx.item_name_opt(contract)?;
        write!(output, "{name}").ok()?;
    }
    output.push(')');
    Some(())
}
