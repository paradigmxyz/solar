//! Storage layout tags: `@custom:solar-fuse`, `@custom:solar-inline`, `@custom:solar-bitmap` and
//! `@custom:solar-handle`.
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
//! `@custom:solar-bitmap` on a mapping from an unsigned integer to `bool` keeps the value for key
//! `k` in bit `k % 256` of the word at `keccak256((k / 256) . slot)`, so 256 consecutive keys
//! share one word where the standard layout gives each its own.
//!
//! `@custom:solar-handle <field> <dictionary>` on a struct declared in a contract keeps, in place
//! of a field's value, its handle: one plus the index of the value in the dictionary, a state
//! variable of the contract that lists values of the field's type, or zero for the value zero. A
//! handle takes 9 bytes, so the field can share a word with others: a `bytes32` market id next to
//! an `address` takes one word where the standard layout takes two. Reading the field reads the
//! dictionary element, so the field can only be set to an element of its dictionary,
//! `dictionary[index]`, or to zero, and the dictionary can only grow: its elements never change,
//! so every handle keeps its value. A struct with handles cannot be written as a whole in storage,
//! and only the struct's contract and the contracts that inherit it, which have the dictionary,
//! can store it.
//!
//! The standard layout stays observable through a variable's slot, so the tagged variables may
//! only be used where the compiler knows the tag. A fused or bitmap mapping may only be indexed. An
//! inline array may be indexed, measured, pushed, popped, deleted and copied into memory, but not
//! bound to a storage reference, a storage parameter or a storage return, passed to library
//! functions, chosen by a conditional expression, or assigned as a whole. A handle dictionary has
//! the same limits, and is never popped, deleted or written. Inline assembly cannot take the
//! `.slot` or `.offset` of any of them, nor of a storage value that holds a struct with handles.

use crate::{
    builtins::Builtin,
    hir::{self, ExprKind, StmtKind, Visit},
    ty::{Gcx, Ty, TyKind},
};
use solar_ast::{DataLocation, ElementaryType, LitKind, UnOpKind};
use solar_data_structures::{
    Never,
    map::{FxHashMap, FxHashSet, FxIndexMap},
};
use solar_interface::{Span, Symbol, kw, sym};
use std::ops::ControlFlow;

pub(super) fn check(gcx: Gcx<'_>) {
    let fused = check_fused_groups(gcx);
    let inline = check_inline_arrays(gcx);
    let bitmaps = check_bitmaps(gcx);
    let handles = check_handles(gcx);
    if fused.is_empty() && inline.is_empty() && bitmaps.is_empty() && handles.fields.is_empty() {
        return;
    }
    check_handle_storage(gcx, &handles);
    let mut uses = Uses {
        gcx,
        fused: &fused,
        inline: &inline,
        bitmaps: &bitmaps,
        handles: &handles,
        function: None,
    };
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

/// Checks every `@custom:solar-bitmap` mapping and returns the mappings the tags pack into bits.
fn check_bitmaps(gcx: Gcx<'_>) -> FxHashSet<hir::VariableId> {
    let mut bitmaps = FxHashSet::default();
    for contract in gcx.hir.contracts() {
        for id in contract.variables() {
            // A tag on anything but a mapping state variable is reported as misplaced.
            let Some(tag) = gcx.hir.solar_bitmap(id) else { continue };
            let Some((key, value)) = mapping_types(gcx, id) else { continue };
            bitmaps.insert(id);
            let unsigned = matches!(key.kind, TyKind::Elementary(ElementaryType::UInt(_)));
            let boolean = matches!(value.kind, TyKind::Elementary(ElementaryType::Bool));
            if !unsigned || !boolean {
                gcx.dcx()
                    .err("a bitmap mapping must map an unsigned integer to `bool`")
                    .span(tag)
                    .span_note(
                        gcx.hir.variable(id).span,
                        format!("it maps `{}` to `{}`", key.display(gcx), value.display(gcx)),
                    )
                    .note("key `k` keeps its value in bit `k % 256` of the word for `k / 256`")
                    .emit();
            }
        }
    }
    bitmaps
}

/// The handle fields that `@custom:solar-handle` tags declare.
#[derive(Default)]
struct Handles {
    /// The dictionary of every handle field.
    fields: FxHashMap<hir::VariableId, hir::VariableId>,
    /// The dictionaries of the handle fields.
    dictionaries: FxHashSet<hir::VariableId>,
    /// The structs with handle fields, each with the contract that declares it.
    structs: FxHashMap<hir::StructId, hir::ContractId>,
}

impl Handles {
    /// The first struct with handle fields that a value of type `ty` holds: the value itself, or
    /// one of its members, elements or mapping values.
    fn struct_in(&self, gcx: Gcx<'_>, ty: Ty<'_>) -> Option<hir::StructId> {
        if self.structs.is_empty() {
            return None;
        }
        self.struct_in_inner(gcx, ty, &mut FxHashSet::default())
    }

    fn struct_in_inner(
        &self,
        gcx: Gcx<'_>,
        ty: Ty<'_>,
        visited: &mut FxHashSet<hir::StructId>,
    ) -> Option<hir::StructId> {
        match ty.peel_refs().kind {
            TyKind::Struct(id) if self.structs.contains_key(&id) => Some(id),
            TyKind::Struct(id) if visited.insert(id) => gcx
                .struct_field_types(id)
                .iter()
                .find_map(|&field| self.struct_in_inner(gcx, field, visited)),
            TyKind::Array(element, _) | TyKind::DynArray(element) | TyKind::Mapping(_, element) => {
                self.struct_in_inner(gcx, element, visited)
            }
            _ => None,
        }
    }
}

/// Checks every `@custom:solar-handle` tag and returns the handle fields the tags declare.
fn check_handles(gcx: Gcx<'_>) -> Handles {
    let mut handles = Handles::default();
    for id in gcx.hir.strukt_ids() {
        let strukt = gcx.hir.strukt(id);
        // A tag on a struct outside a contract is reported as misplaced.
        let Some(contract) = strukt.contract.filter(|&contract| {
            matches!(
                gcx.hir.contract(contract).kind,
                hir::ContractKind::Contract | hir::ContractKind::AbstractContract
            )
        }) else {
            continue;
        };
        let named = |id: hir::VariableId, name: Symbol| {
            gcx.hir.variable(id).name.is_some_and(|ident| ident.name == name)
        };
        for (names, tag) in gcx.hir.solar_handle_tags(id) {
            let Some((field_name, dictionary_name)) = names else {
                gcx.dcx()
                    .err("`@custom:solar-handle` must name a field and its dictionary")
                    .span(tag)
                    .help(
                        "name a field of the struct and the state variable that lists its values: \
                         `@custom:solar-handle marketId markets`",
                    )
                    .emit();
                continue;
            };
            let Some(field) = strukt.fields.iter().copied().find(|&id| named(id, field_name))
            else {
                gcx.dcx()
                    .err(format!("struct `{}` has no field `{field_name}`", strukt.name))
                    .span(tag)
                    .emit();
                continue;
            };
            let field_ty = gcx.type_of_item(field.into());
            if value_bytes(field_ty) != Some(32) {
                gcx.dcx()
                    .err("a handle field must be a value type that fills a word")
                    .span(tag)
                    .span_note(
                        gcx.hir.variable(field).span,
                        format!("`{field_name}` is `{}`", field_ty.display(gcx)),
                    )
                    .note(
                        "the field keeps a 9-byte handle, which only saves space in place of a \
                         word",
                    )
                    .emit();
                continue;
            }
            let Some(dictionary) =
                gcx.hir.contract(contract).variables().find(|&id| {
                    gcx.hir.variable(id).is_state_variable() && named(id, dictionary_name)
                })
            else {
                gcx.dcx()
                    .err(format!(
                        "contract `{}` has no state variable `{dictionary_name}`",
                        gcx.hir.contract(contract).name
                    ))
                    .span(tag)
                    .note("the dictionary must be a state variable of the struct's contract")
                    .emit();
                continue;
            };
            let variable = gcx.hir.variable(dictionary);
            let dictionary_ty = gcx.type_of_item(dictionary.into());
            let lists = matches!(
                dictionary_ty.peel_refs().kind,
                TyKind::DynArray(element) if element == field_ty
            );
            if !lists
                || variable.is_constant()
                || variable.is_immutable()
                || variable.data_location == Some(DataLocation::Transient)
            {
                gcx.dcx()
                    .err(format!(
                        "a handle dictionary must be a storage array of `{}`",
                        field_ty.display(gcx)
                    ))
                    .span(tag)
                    .span_note(
                        variable.span,
                        format!("`{dictionary_name}` is `{}`", dictionary_ty.display(gcx)),
                    )
                    .emit();
                continue;
            }
            if handles.fields.insert(field, dictionary).is_some() {
                gcx.dcx()
                    .err(format!("field `{field_name}` has more than one handle tag"))
                    .span(tag)
                    .emit();
            }
            handles.dictionaries.insert(dictionary);
            handles.structs.insert(id, contract);
        }
    }
    handles
}

/// Rejects storage that holds a struct with handles outside the contracts that have its
/// dictionaries, the struct's contract and the contracts that inherit it, and state variable
/// initializers that would write such a struct as a whole.
fn check_handle_storage(gcx: Gcx<'_>, handles: &Handles) {
    if handles.structs.is_empty() {
        return;
    }
    for id in gcx.hir.variable_ids() {
        let variable = gcx.hir.variable(id);
        let storage = if variable.is_state_variable() {
            !variable.is_constant() && !variable.is_immutable()
        } else {
            variable.data_location == Some(DataLocation::Storage)
        };
        if !storage {
            continue;
        }
        let Some(strukt) = handles.struct_in(gcx, gcx.type_of_item(id.into())) else { continue };
        let owner = handles.structs[&strukt];
        // A local the compiler adds to a getter belongs to the getter's contract.
        let contract = variable.contract.or_else(|| match variable.parent {
            Some(hir::ItemId::Function(function)) => gcx.hir.function(function).contract,
            _ => None,
        });
        let inherits = contract
            .is_some_and(|contract| gcx.hir.contract(contract).linearized_bases.contains(&owner));
        if !inherits {
            gcx.dcx()
                .err(
                    "a struct with handles can only be stored by its contract and the contracts \
                     that inherit it",
                )
                .span(variable.span)
                .span_note(
                    gcx.hir.strukt(strukt).span,
                    format!(
                        "`{}` keeps handles into the dictionaries of `{}`",
                        gcx.hir.strukt(strukt).name,
                        gcx.hir.contract(owner).name
                    ),
                )
                .emit();
        } else if variable.is_state_variable()
            && let Some(initializer) = variable.initializer
        {
            report_whole_write(gcx, initializer.span);
        }
    }
}

/// Reports a write of a whole value that holds a struct with handles into storage at `span`.
fn report_whole_write(gcx: Gcx<'_>, span: Span) {
    gcx.dcx()
        .err("a struct with handles cannot be written to storage as a whole")
        .span(span)
        .note(
            "`@custom:solar-handle` fields keep indexes into their dictionaries, which a value from \
             elsewhere does not have",
        )
        .help("set the fields one by one, each handle field to an element of its dictionary")
        .emit();
}

/// Whether `expr` is a literal zero, possibly converted or wrapped: `0`, `bytes32(0)` or
/// `Id.wrap(0)`.
fn is_zero(gcx: Gcx<'_>, expr: &hir::Expr<'_>) -> bool {
    let expr = expr.peel_parens();
    if let ExprKind::Lit(lit) = expr.kind {
        return matches!(lit.kind, LitKind::Number(value) if value.is_zero());
    }
    let Some((callee, args, None)) = expr.as_call() else { return false };
    let hir::CallArgsKind::Unnamed([arg]) = args.kind else { return false };
    let conversion = matches!(callee.peel_parens().kind, ExprKind::Type(_))
        || gcx.resolved_builtin(callee) == Some(Builtin::UdvtWrap);
    conversion && is_zero(gcx, arg)
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

/// Rejects the uses of tagged state variables that would reach their standard layout, and the
/// writes that would give a handle field a value its dictionary does not list.
struct Uses<'gcx, 'a> {
    gcx: Gcx<'gcx>,
    fused: &'a FxHashSet<hir::VariableId>,
    inline: &'a FxHashSet<hir::VariableId>,
    bitmaps: &'a FxHashSet<hir::VariableId>,
    handles: &'a Handles,
    /// The function whose body is visited.
    function: Option<hir::FunctionId>,
}

/// A storage array that must not become a storage reference.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kept {
    /// An array documented `@custom:solar-inline`, whose inline form a storage reference would
    /// read with the standard layout.
    Inline,
    /// The dictionary of handle fields, whose elements a storage reference could change.
    Dictionary,
}

impl Kept {
    fn what(self) -> &'static str {
        match self {
            Self::Inline => "an inline array",
            Self::Dictionary => "a handle dictionary",
        }
    }

    /// What a storage reference to the array would do wrong.
    fn reference_risk(self) -> &'static str {
        match self {
            Self::Inline => "which reads the standard layout",
            Self::Dictionary => "which could change its elements",
        }
    }
}

impl<'gcx> Uses<'gcx, '_> {
    fn is_fused(&self, expr: &hir::Expr<'_>) -> bool {
        self.gcx.resolved_variable(expr.peel_parens()).is_some_and(|id| self.fused.contains(&id))
    }

    fn is_bitmap(&self, expr: &hir::Expr<'_>) -> bool {
        self.gcx.resolved_variable(expr.peel_parens()).is_some_and(|id| self.bitmaps.contains(&id))
    }

    /// The kind of array `expr` names, if it names an inline array or a handle dictionary.
    fn kept(&self, expr: &hir::Expr<'_>) -> Option<Kept> {
        let id = self.gcx.resolved_variable(expr.peel_parens())?;
        if self.inline.contains(&id) {
            Some(Kept::Inline)
        } else if self.handles.dictionaries.contains(&id) {
            Some(Kept::Dictionary)
        } else {
            None
        }
    }

    /// Whether `expr` names a storage variable that holds a struct with handles.
    fn holds_handles(&self, expr: &hir::Expr<'_>) -> bool {
        self.gcx.resolved_variable(expr.peel_parens()).is_some_and(|id| {
            self.handles.struct_in(self.gcx, self.gcx.type_of_item(id.into())).is_some()
        })
    }

    /// Reports an array of kind `kept` that becomes a storage reference at `span`.
    fn report_reference(&self, kept: Kept, span: Span) {
        let (note, help) = match kept {
            Kept::Inline => (
                "`@custom:solar-inline` keeps a short array in its own slot, which a storage \
                 reference would read with the standard layout",
                "index the array or copy it into memory",
            ),
            Kept::Dictionary => (
                "`@custom:solar-handle` fields keep indexes into the dictionary, whose elements a \
                 storage reference could change",
                "index the dictionary or copy it into memory",
            ),
        };
        self.gcx
            .dcx()
            .err(format!("{} cannot be a storage reference", kept.what()))
            .span(span)
            .note(note)
            .help(help)
            .emit();
    }

    /// Reports a write at `span` that would change an element of a handle dictionary.
    fn report_append_only(&self, span: Span) {
        self.gcx
            .dcx()
            .err("a handle dictionary is append-only")
            .span(span)
            .note(
                "`@custom:solar-handle` fields keep indexes into the dictionary, so its elements \
                 must never change",
            )
            .help("`push` new elements instead")
            .emit();
    }

    /// Reports a write at `span` of a value that a handle field into `dictionary` cannot keep.
    fn report_handle_value(&self, span: Span, dictionary: hir::VariableId) {
        let name = self.gcx.hir.variable(dictionary).name.map_or(kw::Empty, |ident| ident.name);
        self.gcx
            .dcx()
            .err(format!("a handle field can only be set to an element of `{name}` or to zero"))
            .span(span)
            .note(
                "`@custom:solar-handle` keeps the index of the field's value in the dictionary, \
                 which other values do not have",
            )
            .help(format!("assign `{name}[index]`, or `delete` the field"))
            .emit();
    }

    /// The dictionary of the handle field that `expr` names in storage.
    fn handle_field(&self, expr: &hir::Expr<'_>) -> Option<hir::VariableId> {
        let expr = expr.peel_parens();
        let ExprKind::Member(base, _) = expr.kind else { return None };
        let dictionary = *self.handles.fields.get(&self.gcx.resolved_variable(expr)?)?;
        self.gcx.type_of_expr(base.id)?.is_ref_at(DataLocation::Storage).then_some(dictionary)
    }

    /// Whether assigning to `place` writes a whole storage value that holds a struct with handles.
    /// Assigning to a local storage reference rebinds it instead.
    fn writes_handles(&self, place: &hir::Expr<'_>) -> bool {
        let Some(ty) = self.gcx.type_of_expr(place.id) else { return false };
        let local = self
            .gcx
            .resolved_variable(place.peel_parens())
            .is_some_and(|id| !self.gcx.hir.variable(id).is_state_variable());
        ty.is_ref_at(DataLocation::Storage)
            && !local
            && self.handles.struct_in(self.gcx, ty).is_some()
    }

    /// Whether writing to `place` changes an element of a handle dictionary.
    fn writes_dictionary(&self, place: &hir::Expr<'_>) -> bool {
        match place.peel_parens().kind {
            // dictionary[index] = value
            ExprKind::Index(base, _) => self.kept(base) == Some(Kept::Dictionary),
            // dictionary.push() = value
            ExprKind::Call(callee, _) => matches!(
                callee.peel_parens().kind,
                ExprKind::Member(base, _) if self.kept(base) == Some(Kept::Dictionary)
            ),
            ExprKind::Tuple(elements) => {
                elements.iter().flatten().any(|element| self.writes_dictionary(element))
            }
            _ => false,
        }
    }

    /// Checks the writes of `expr` to handle fields, to storage values that hold them, and to the
    /// elements of handle dictionaries.
    fn check_handle_writes(&self, expr: &hir::Expr<'_>) {
        match expr.kind {
            ExprKind::Assign(lhs, op, rhs) => {
                if let Some(dictionary) = self.handle_field(lhs) {
                    // field = dictionary[index], field = 0
                    let element = matches!(
                        rhs.peel_parens().kind,
                        ExprKind::Index(base, Some(_))
                            if self.gcx.resolved_variable(base.peel_parens()) == Some(dictionary)
                    );
                    if op.is_some() || !(element || is_zero(self.gcx, rhs)) {
                        self.report_handle_value(expr.span, dictionary);
                    }
                } else if self.writes_handles(lhs) {
                    report_whole_write(self.gcx, expr.span);
                } else if let ExprKind::Tuple(elements) = lhs.peel_parens().kind
                    && elements.iter().flatten().any(|element| {
                        self.handle_field(element).is_some() || self.writes_handles(element)
                    })
                {
                    self.gcx
                        .dcx()
                        .err("a tuple assignment cannot write handle fields")
                        .span(expr.span)
                        .help("assign the handle fields in their own statements")
                        .emit();
                }
                if self.writes_dictionary(lhs) {
                    self.report_append_only(expr.span);
                }
            }
            // field++, field--
            ExprKind::Unary(op, operand)
                if matches!(
                    op.kind,
                    UnOpKind::PreInc | UnOpKind::PreDec | UnOpKind::PostInc | UnOpKind::PostDec
                ) =>
            {
                if let Some(dictionary) = self.handle_field(operand) {
                    self.report_handle_value(expr.span, dictionary);
                }
            }
            // delete dictionary, delete dictionary[index]
            ExprKind::Delete(place)
                if self.writes_dictionary(place) || self.kept(place) == Some(Kept::Dictionary) =>
            {
                self.report_append_only(expr.span);
            }
            // array.push(value) with elements that hold handles
            ExprKind::Call(callee, ref args) if !args.is_empty() => {
                if let ExprKind::Member(base, member) = callee.peel_parens().kind
                    && member.name == sym::push
                    && let Some(ty) = self.gcx.type_of_expr(base.id)
                    && ty.is_ref_at(DataLocation::Storage)
                    && let TyKind::DynArray(element) = ty.peel_refs().kind
                    && self.handles.struct_in(self.gcx, element).is_some()
                {
                    report_whole_write(self.gcx, expr.span);
                }
            }
            _ => {}
        }
    }

    /// Reports every inline array or handle dictionary among `args` passed to a storage
    /// parameter of a function of type `callee`, named `names` for named arguments.
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
                    if let Some(kept) = self.kept(arg)
                        && storage(index)
                    {
                        self.report_reference(kept, arg.span);
                    }
                }
            }
            hir::CallArgsKind::Named(args) => {
                for arg in args {
                    let index = names.and_then(|names| arg.parameter_index(names));
                    // A parameter the names cannot place is treated as a storage one.
                    if let Some(kept) = self.kept(&arg.value)
                        && index.is_none_or(storage)
                    {
                        self.report_reference(kept, arg.value.span);
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
                    && let Some(kept) = self.kept(initializer)
                    && self.gcx.type_of_item(id.into()).is_ref_at(DataLocation::Storage)
                {
                    self.report_reference(kept, initializer.span);
                }
            }
            // (T[] storage r, ..) = (array, ..);
            StmtKind::DeclMulti(variables, initializer) => {
                if let ExprKind::Tuple(elements) = initializer.peel_parens().kind {
                    for (&variable, element) in variables.iter().zip(elements) {
                        if let (Some(variable), Some(element)) = (variable, element)
                            && let Some(kept) = self.kept(element)
                            && self
                                .gcx
                                .type_of_item(variable.into())
                                .is_ref_at(DataLocation::Storage)
                        {
                            self.report_reference(kept, element.span);
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
                            && let Some(kept) = self.kept(value)
                            && self.gcx.type_of_item(ret.into()).is_ref_at(DataLocation::Storage)
                        {
                            self.report_reference(kept, value.span);
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
        self.check_handle_writes(expr);
        match expr.kind {
            // mapping[key], array[index]
            ExprKind::Index(base, index)
                if self.is_fused(base)
                    || self.kept(base) == Some(Kept::Inline)
                    || self.is_bitmap(base) =>
            {
                if let Some(index) = index {
                    self.visit_expr(index)?;
                }
                return ControlFlow::Continue(());
            }
            // array.length, array.push, array.pop
            ExprKind::Member(base, member) if let Some(kept) = self.kept(base) => {
                match (kept, member.name) {
                    (_, sym::length | sym::push) | (Kept::Inline, kw::Pop) => {}
                    (Kept::Dictionary, kw::Pop) => self.report_append_only(expr.span),
                    _ => {
                        self.gcx
                            .dcx()
                            .err(format!(
                                "{} has no member `{member}` this compiler keeps",
                                kept.what()
                            ))
                            .span(expr.span)
                            .note(format!(
                                "a library function attached to it would take it as a storage \
                                 reference, {}",
                                kept.reference_risk()
                            ))
                            .emit();
                    }
                }
                return ControlFlow::Continue(());
            }
            // delete array
            ExprKind::Delete(value) if self.kept(value).is_some() => {
                return ControlFlow::Continue(());
            }
            // array = value
            ExprKind::Assign(lhs, _, rhs) if let Some(kept) = self.kept(lhs) => {
                match kept {
                    Kept::Inline => {
                        self.gcx
                            .dcx()
                            .err("an inline array cannot be assigned as a whole")
                            .span(expr.span)
                            .help("`delete` it and `push` the elements")
                            .emit();
                    }
                    Kept::Dictionary => self.report_append_only(expr.span),
                }
                return self.visit_expr(rhs);
            }
            // r = array; with a local storage reference r
            ExprKind::Assign(lhs, None, rhs)
                if let Some(kept) = self.kept(rhs)
                    && self.gcx.resolved_variable(lhs.peel_parens()).is_some_and(|id| {
                        !self.gcx.hir.variable(id).is_state_variable()
                            && self.gcx.type_of_item(id.into()).is_ref_at(DataLocation::Storage)
                    }) =>
            {
                self.report_reference(kept, rhs.span);
            }
            // condition ? array : other
            ExprKind::Ternary(_, then, otherwise)
                if let Some(kept) = self.kept(then).or_else(|| self.kept(otherwise)) =>
            {
                self.gcx
                    .dcx()
                    .err(format!("{} cannot be chosen by a conditional expression", kept.what()))
                    .span(expr.span)
                    .note(format!("the result is a storage reference, {}", kept.reference_risk()))
                    .help("branch with `if` instead")
                    .emit();
            }
            // f(array) with a storage parameter
            ExprKind::Call(callee, ref args) => {
                let names = self.parameter_names(callee);
                self.check_arguments(self.gcx.type_of_expr(callee.id), names.as_deref(), args);
            }
            // variable.slot, variable.offset
            ExprKind::YulMember(base, member)
                if self.is_fused(base)
                    || self.kept(base).is_some()
                    || self.is_bitmap(base)
                    || self.holds_handles(base) =>
            {
                let (what, note) = if self.is_fused(base) {
                    (
                        "a fused mapping",
                        "`@custom:solar-fuse` moves the mapping's values out of their standard \
                         slots",
                    )
                } else if self.is_bitmap(base) {
                    (
                        "a bitmap mapping",
                        "`@custom:solar-bitmap` moves the mapping's values out of their standard \
                         slots",
                    )
                } else if self.kept(base) == Some(Kept::Inline) {
                    (
                        "an inline array",
                        "`@custom:solar-inline` moves the array's elements out of their standard \
                         slots",
                    )
                } else if self.kept(base) == Some(Kept::Dictionary) {
                    (
                        "a handle dictionary",
                        "`@custom:solar-handle` fields keep indexes into the dictionary, so its \
                         elements must never change",
                    )
                } else {
                    (
                        "a storage value that holds a struct with handles",
                        "`@custom:solar-handle` narrows the handle fields out of their standard \
                         slots",
                    )
                };
                self.gcx
                    .dcx()
                    .err(format!("inline assembly cannot take the `.{member}` of {what}"))
                    .span(expr.span)
                    .note(note)
                    .emit();
                return ControlFlow::Continue(());
            }
            _ if self.is_fused(expr) || self.is_bitmap(expr) => {
                let (what, tag) = if self.is_fused(expr) {
                    ("a fused mapping", "`@custom:solar-fuse`")
                } else {
                    ("a bitmap mapping", "`@custom:solar-bitmap`")
                };
                self.gcx
                    .dcx()
                    .err(format!("{what} can only be indexed"))
                    .span(expr.span)
                    .note(format!(
                        "{tag} moves the mapping's values out of the standard slots that a \
                         storage reference reaches"
                    ))
                    .emit();
                return ControlFlow::Continue(());
            }
            _ => {}
        }
        self.walk_expr(expr)
    }
}
