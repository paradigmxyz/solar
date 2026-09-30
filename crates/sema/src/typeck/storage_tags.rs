//! Storage layout tags: `@custom:solar-fuse` and `@custom:solar-inline`.
//!
//! A layout tag changes where the values of a state variable live in storage, never what the
//! program computes. Every Solidity-level read and write behaves as without the tag; what differs
//! is the raw layout: the slots that inline assembly, `eth_getStorageAt` or a raw-slot reader see,
//! and the storage credits a chain such as Tempo grants for clearing slots. Other compilers read
//! the tags as documentation and use the standard layout, so a contract built both ways must not
//! share storage, as an upgrade would.
//!
//! `@custom:solar-fuse <group>` on mapping state variables of one contract that share a key type
//! keeps the values of the whole group for one key in one record, laid out like the fields of a
//! struct in declaration order, at `keccak256(key . slot)` with the slot of the group's first
//! mapping. Three mappings of `address` to `uint128`, `uint64` and `uint64` then fill one word per
//! address instead of three, and code that reads or writes them for one key shares the hash and
//! the word. Each mapping keeps its declared slot, so the layout of the contract's state
//! variables is the standard one; only the values move.
//!
//! `@custom:solar-inline` on a dynamic storage array state variable keeps the length and the
//! elements in the array's own slot while they fit below its top byte, as the codegen module
//! `inline_arrays` describes; a longer array has the standard layout. Its elements must be value
//! types narrower than a word.
//!
//! The standard layout stays observable through a variable's slot, so the tagged variables may
//! only be used where the compiler knows the tag. A fused mapping may only be indexed. An inline
//! array may be indexed, measured, pushed, popped, deleted and copied into memory, but not bound
//! to a storage reference, a storage parameter or a storage return, passed to library functions,
//! chosen by a conditional expression, or assigned as a whole. Inline assembly cannot take the
//! `.slot` or `.offset` of either.

use crate::{
    hir::{self, ExprKind, StmtKind, Visit},
    ty::{Gcx, Ty, TyKind},
};
use solar_ast::{DataLocation, ElementaryType};
use solar_data_structures::{
    Never,
    map::{FxHashSet, FxIndexMap},
};
use solar_interface::{Span, Symbol, kw, sym};
use std::ops::ControlFlow;

pub(super) fn check(gcx: Gcx<'_>) {
    let fused = check_fused_groups(gcx);
    let inline = check_inline_arrays(gcx);
    if fused.is_empty() && inline.is_empty() {
        return;
    }
    let mut uses = Uses { gcx, fused: &fused, inline: &inline, function: None };
    for id in gcx.hir.function_ids() {
        uses.function = Some(id);
        let _ = uses.visit_nested_function(id);
    }
}

/// Checks every `@custom:solar-fuse` group and returns the mappings the tags fuse.
fn check_fused_groups(gcx: Gcx<'_>) -> FxHashSet<hir::VariableId> {
    let mut fused = FxHashSet::default();
    for contract in gcx.hir.contracts() {
        let mut groups = FxIndexMap::<Symbol, Vec<(hir::VariableId, Span)>>::default();
        for id in contract.variables() {
            // A tag on anything but a mapping state variable is reported as misplaced.
            let Some((group, tag)) = gcx.hir.solar_fuse(id) else { continue };
            if mapping_types(gcx, id).is_none() {
                continue;
            }
            fused.insert(id);
            if group == kw::Empty {
                gcx.dcx()
                    .err("`@custom:solar-fuse` must name the group of its mapping")
                    .span(tag)
                    .help("name the group on every mapping of it: `@custom:solar-fuse account`")
                    .emit();
                continue;
            }
            groups.entry(group).or_default().push((id, tag));
        }
        for (group, members) in &groups {
            check_group(gcx, *group, members);
        }
    }
    fused
}

/// Checks the mappings `members` of the fused group `group`, each with its tag's span.
fn check_group(gcx: Gcx<'_>, group: Symbol, members: &[(hir::VariableId, Span)]) {
    let &[(first, first_tag), ..] = members else { return };
    if members.len() == 1 {
        gcx.dcx()
            .err(format!("fused group `{group}` has only one mapping"))
            .span(first_tag)
            .note("a group keeps the values that several mappings hold for one key in one record")
            .help("tag the other mappings of the group, or remove the tag")
            .emit();
        return;
    }
    let first_key = mapping_types(gcx, first).map(|(key, _)| key);
    for &(id, tag) in members {
        let Some((key, value)) = mapping_types(gcx, id) else { continue };
        if Some(key) != first_key {
            gcx.dcx()
                .err(format!("the mappings of fused group `{group}` must have the same key type"))
                .span(tag)
                .span_note(
                    gcx.hir.variable(first).span,
                    format!(
                        "the group's first mapping takes `{}` keys",
                        first_key.unwrap_or(key).display(gcx)
                    ),
                )
                .emit();
        }
        if !value.is_value_type() {
            gcx.dcx()
                .err("a fused mapping must map to a value type")
                .span(tag)
                .span_note(
                    gcx.hir.variable(id).span,
                    format!("it maps to `{}`", value.display(gcx)),
                )
                .note(
                    "a record holds the values of the group's mappings side by side, like a \
                     struct's fields",
                )
                .emit();
        }
    }
}

/// The key and value types of the mapping state variable `id`.
fn mapping_types<'gcx>(gcx: Gcx<'gcx>, id: hir::VariableId) -> Option<(Ty<'gcx>, Ty<'gcx>)> {
    let TyKind::Mapping(key, value) = gcx.type_of_item(id.into()).peel_refs().kind else {
        return None;
    };
    Some((key, value))
}

/// Checks every `@custom:solar-inline` array and returns the arrays the tags keep inline.
fn check_inline_arrays(gcx: Gcx<'_>) -> FxHashSet<hir::VariableId> {
    let mut inline = FxHashSet::default();
    for contract in gcx.hir.contracts() {
        for id in contract.variables() {
            // A tag on anything but a dynamic storage array is reported as misplaced.
            let Some(tag) = gcx.hir.solar_inline(id) else { continue };
            let TyKind::DynArray(element) = gcx.type_of_item(id.into()).peel_refs().kind else {
                continue;
            };
            inline.insert(id);
            if value_bytes(element).is_none_or(|bytes| bytes >= 32) {
                gcx.dcx()
                    .err("the elements of an inline array must be value types narrower than a word")
                    .span(tag)
                    .span_note(
                        gcx.hir.variable(id).span,
                        format!("its elements are `{}`", element.display(gcx)),
                    )
                    .note("the array's slot keeps its length in the top byte, below its elements")
                    .emit();
            }
        }
    }
    inline
}

/// The bytes a value of the value type `ty` takes in a storage word, or `None` for a type that
/// does not pack.
fn value_bytes(ty: Ty<'_>) -> Option<u64> {
    Some(match ty.peel_refs().kind {
        TyKind::Elementary(ElementaryType::Address(_)) | TyKind::Contract(_) => 20,
        TyKind::Elementary(ElementaryType::Bool) | TyKind::Enum(_) => 1,
        TyKind::Elementary(
            ElementaryType::Int(size)
            | ElementaryType::UInt(size)
            | ElementaryType::FixedBytes(size),
        ) => u64::from(size.bytes()),
        TyKind::Udvt(inner, _) => return value_bytes(inner),
        TyKind::Fn(function) if function.is_external() => 24,
        TyKind::Fn(_) => 8,
        _ => return None,
    })
}

/// Rejects the uses of tagged state variables that would reach their standard layout.
struct Uses<'gcx, 'a> {
    gcx: Gcx<'gcx>,
    fused: &'a FxHashSet<hir::VariableId>,
    inline: &'a FxHashSet<hir::VariableId>,
    /// The function whose body is visited.
    function: Option<hir::FunctionId>,
}

impl<'gcx> Uses<'gcx, '_> {
    fn is_fused(&self, expr: &hir::Expr<'_>) -> bool {
        self.gcx.resolved_variable(expr.peel_parens()).is_some_and(|id| self.fused.contains(&id))
    }

    fn is_inline(&self, expr: &hir::Expr<'_>) -> bool {
        self.gcx.resolved_variable(expr.peel_parens()).is_some_and(|id| self.inline.contains(&id))
    }

    /// Reports an inline array that becomes a storage reference at `span`.
    fn report_reference(&self, span: Span) {
        self.gcx
            .dcx()
            .err("an inline array cannot be a storage reference")
            .span(span)
            .note(
                "`@custom:solar-inline` keeps a short array in its own slot, which a storage \
                 reference would read with the standard layout",
            )
            .help("index the array or copy it into memory")
            .emit();
    }

    /// Reports every inline array among `args` passed to a storage parameter of a function of
    /// type `callee`, named `names` for named arguments.
    fn check_arguments(
        &self,
        callee: Option<Ty<'gcx>>,
        names: Option<&[Option<Symbol>]>,
        args: &hir::CallArgs<'gcx>,
    ) {
        let Some(TyKind::Fn(function)) = callee.map(|ty| ty.kind) else { return };
        let storage = |index: usize| {
            function.parameters.get(index).is_some_and(|ty| ty.is_ref_at(DataLocation::Storage))
        };
        match args.kind {
            hir::CallArgsKind::Unnamed(args) => {
                for (index, arg) in args.iter().enumerate() {
                    if self.is_inline(arg) && storage(index) {
                        self.report_reference(arg.span);
                    }
                }
            }
            hir::CallArgsKind::Named(args) => {
                for arg in args {
                    let index = names.and_then(|names| arg.parameter_index(names));
                    // A parameter the names cannot place is treated as a storage one.
                    if self.is_inline(&arg.value) && index.is_none_or(storage) {
                        self.report_reference(arg.value.span);
                    }
                }
            }
        }
    }

    /// The names of the parameters of the function `callee` resolves to.
    fn parameter_names(&self, callee: &hir::Expr<'_>) -> Option<Vec<Option<Symbol>>> {
        let function = self.gcx.resolved_function(callee)?;
        let parameters = self.gcx.hir.function(function).parameters;
        Some(
            parameters
                .iter()
                .map(|&parameter| self.gcx.hir.variable(parameter).name.map(|name| name.name))
                .collect(),
        )
    }
}

impl<'gcx> Visit<'gcx> for Uses<'gcx, '_> {
    type BreakValue = Never;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_stmt(&mut self, stmt: &'gcx hir::Stmt<'gcx>) -> ControlFlow<Self::BreakValue> {
        match stmt.kind {
            // T[] storage r = array;
            StmtKind::DeclSingle(id) => {
                let variable = self.gcx.hir.variable(id);
                if let Some(initializer) = variable.initializer
                    && self.is_inline(initializer)
                    && self.gcx.type_of_item(id.into()).is_ref_at(DataLocation::Storage)
                {
                    self.report_reference(initializer.span);
                }
            }
            // (T[] storage r, ..) = (array, ..);
            StmtKind::DeclMulti(variables, initializer) => {
                if let ExprKind::Tuple(elements) = initializer.peel_parens().kind {
                    for (&variable, element) in variables.iter().zip(elements) {
                        if let (Some(variable), Some(element)) = (variable, element)
                            && self.is_inline(element)
                            && self
                                .gcx
                                .type_of_item(variable.into())
                                .is_ref_at(DataLocation::Storage)
                        {
                            self.report_reference(element.span);
                        }
                    }
                }
            }
            // return array; with a storage return
            StmtKind::Return(Some(value)) => {
                if let Some(function) = self.function {
                    let returns = self.gcx.hir.function(function).returns;
                    let values = match value.peel_parens().kind {
                        ExprKind::Tuple(elements) => elements.to_vec(),
                        _ => vec![Some(value)],
                    };
                    for (&ret, value) in returns.iter().zip(values) {
                        if let Some(value) = value
                            && self.is_inline(value)
                            && self.gcx.type_of_item(ret.into()).is_ref_at(DataLocation::Storage)
                        {
                            self.report_reference(value.span);
                        }
                    }
                }
            }
            _ => {}
        }
        self.walk_stmt(stmt)
    }

    fn visit_modifier(
        &mut self,
        modifier: &'gcx hir::Modifier<'gcx>,
    ) -> ControlFlow<Self::BreakValue> {
        if let hir::ItemId::Function(function) = modifier.id {
            let parameters = self.gcx.hir.function(function).parameters;
            let names = parameters
                .iter()
                .map(|&parameter| self.gcx.hir.variable(parameter).name.map(|name| name.name))
                .collect::<Vec<_>>();
            self.check_arguments(
                Some(self.gcx.type_of_item(function.into())),
                Some(&names),
                &modifier.args,
            );
        }
        self.walk_modifier(modifier)
    }

    fn visit_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Self::BreakValue> {
        match expr.kind {
            // mapping[key], array[index]
            ExprKind::Index(base, index) if self.is_fused(base) || self.is_inline(base) => {
                if let Some(index) = index {
                    self.visit_expr(index)?;
                }
                return ControlFlow::Continue(());
            }
            // array.length, array.push, array.pop
            ExprKind::Member(base, member) if self.is_inline(base) => {
                if !matches!(member.name, sym::length | sym::push | kw::Pop) {
                    self.gcx
                        .dcx()
                        .err(format!(
                            "an inline array has no member `{member}` this compiler keeps"
                        ))
                        .span(expr.span)
                        .note(
                            "a library function attached to it would take it as a storage \
                             reference, which reads the standard layout",
                        )
                        .emit();
                }
                return ControlFlow::Continue(());
            }
            // delete array
            ExprKind::Delete(value) if self.is_inline(value) => return ControlFlow::Continue(()),
            // array = value
            ExprKind::Assign(lhs, _, rhs) if self.is_inline(lhs) => {
                self.gcx
                    .dcx()
                    .err("an inline array cannot be assigned as a whole")
                    .span(expr.span)
                    .help("`delete` it and `push` the elements")
                    .emit();
                return self.visit_expr(rhs);
            }
            // r = array; with a local storage reference r
            ExprKind::Assign(lhs, None, rhs)
                if self.is_inline(rhs)
                    && self.gcx.resolved_variable(lhs.peel_parens()).is_some_and(|id| {
                        !self.gcx.hir.variable(id).is_state_variable()
                            && self.gcx.type_of_item(id.into()).is_ref_at(DataLocation::Storage)
                    }) =>
            {
                self.report_reference(rhs.span);
            }
            // condition ? array : other
            ExprKind::Ternary(_, then, otherwise)
                if self.is_inline(then) || self.is_inline(otherwise) =>
            {
                self.gcx
                    .dcx()
                    .err("an inline array cannot be chosen by a conditional expression")
                    .span(expr.span)
                    .note("the result is a storage reference, which reads the standard layout")
                    .help("branch with `if` instead")
                    .emit();
            }
            // f(array) with a storage parameter
            ExprKind::Call(callee, ref args) => {
                let names = self.parameter_names(callee);
                self.check_arguments(self.gcx.type_of_expr(callee.id), names.as_deref(), args);
            }
            // variable.slot, variable.offset
            ExprKind::YulMember(base, member) if self.is_fused(base) || self.is_inline(base) => {
                let (what, tag) = if self.is_fused(base) {
                    ("a fused mapping", "`@custom:solar-fuse` moves the mapping's values")
                } else {
                    ("an inline array", "`@custom:solar-inline` moves the array's elements")
                };
                self.gcx
                    .dcx()
                    .err(format!("inline assembly cannot take the `.{member}` of {what}"))
                    .span(expr.span)
                    .note(format!("{tag} out of their standard slots"))
                    .emit();
                return ControlFlow::Continue(());
            }
            _ if self.is_fused(expr) => {
                self.gcx
                    .dcx()
                    .err("a fused mapping can only be indexed")
                    .span(expr.span)
                    .note(
                        "`@custom:solar-fuse` moves the mapping's values out of the standard \
                         slots that a storage reference reaches",
                    )
                    .emit();
                return ControlFlow::Continue(());
            }
            _ => {}
        }
        self.walk_expr(expr)
    }
}
