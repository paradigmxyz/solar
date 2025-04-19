//! High-level intermediate representation (HIR).

use crate::builtins::Builtin;
use derive_more::derive::From;
use either::Either;
use rayon::prelude::*;
use solar_ast as ast;
use solar_data_structures::{index::IndexVec, newtype_index, BumpExt};
use solar_interface::{diagnostics::ErrorGuaranteed, source_map::SourceFile, Ident, Span};
use std::{fmt, ops::ControlFlow, sync::Arc};
use strum::EnumIs;

pub use ast::{
    BinOp, BinOpKind, ContractKind, DataLocation, ElementaryType, FunctionKind, Lit,
    StateMutability, UnOp, UnOpKind, VarMut, Visibility,
};

mod visit;
pub use visit::Visit;

mod pretty;
pub use pretty::HirPrettyPrinter;

/// HIR arena allocator.
pub struct Arena {
    pub bump: bumpalo::Bump,
    pub literals: typed_arena::Arena<Lit>,
}

impl Arena {
    /// Creates a new HIR arena.
    pub fn new() -> Self {
        Self { bump: bumpalo::Bump::new(), literals: typed_arena::Arena::new() }
    }

    pub fn allocated_bytes(&self) -> usize {
        self.bump.allocated_bytes()
            + (self.literals.len() + self.literals.uninitialized_array().len())
                * std::mem::size_of::<Lit>()
    }

    pub fn used_bytes(&self) -> usize {
        self.bump.used_bytes() + self.literals.len() * std::mem::size_of::<Lit>()
    }
}

impl Default for Arena {
    fn default() -> Self {
        Self::new()
    }
}

impl std::ops::Deref for Arena {
    type Target = bumpalo::Bump;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.bump
    }
}

/// The high-level intermediate representation (HIR).
///
/// This struct contains all the information about the entire program.
#[derive(Debug)]
pub struct Hir<'hir> {
    /// All sources.
    pub(crate) sources: IndexVec<SourceId, Source<'hir>>,
    /// All contracts.
    pub(crate) contracts: IndexVec<ContractId, Contract<'hir>>,
    /// All functions.
    pub(crate) functions: IndexVec<FunctionId, Function<'hir>>,
    /// All structs.
    pub(crate) structs: IndexVec<StructId, Struct<'hir>>,
    /// All enums.
    pub(crate) enums: IndexVec<EnumId, Enum<'hir>>,
    /// All user-defined value types.
    pub(crate) udvts: IndexVec<UdvtId, Udvt<'hir>>,
    /// All events.
    pub(crate) events: IndexVec<EventId, Event<'hir>>,
    /// All custom errors.
    pub(crate) errors: IndexVec<ErrorId, Error<'hir>>,
    /// All constants and variables.
    pub(crate) variables: IndexVec<VariableId, Variable<'hir>>,
}

macro_rules! indexvec_methods {
    ($($singular:ident => $plural:ident, $id:ty => $type:ty;)*) => { paste::paste! {
        $(
            #[doc = "Returns the " $singular " associated with the given ID."]
            #[inline]
            #[cfg_attr(debug_assertions, track_caller)]
            pub fn $singular(&self, id: $id) -> &$type {
                if cfg!(debug_assertions) {
                    &self.$plural[id]
                } else {
                    unsafe { self.$plural.raw.get_unchecked(id.index()) }
                }
            }

            #[doc = "Returns an iterator over all of the " $singular " IDs."]
            #[inline]
            pub fn [<$singular _ids>](&self) -> impl ExactSizeIterator<Item = $id> + Clone {
                // SAFETY: `$plural` is an IndexVec, which guarantees that all indexes are in bounds
                // of the respective index type.
                (0..self.$plural.len()).map(|id| unsafe { $id::from_usize_unchecked(id) })
            }

            #[doc = "Returns a parallel iterator over all of the " $singular " IDs."]
            #[inline]
            pub fn [<par_ $singular _ids>](&self) -> impl IndexedParallelIterator<Item = $id> {
                // SAFETY: `$plural` is an IndexVec, which guarantees that all indexes are in bounds
                // of the respective index type.
                (0..self.$plural.len()).into_par_iter().map(|id| unsafe { $id::from_usize_unchecked(id) })
            }

            #[doc = "Returns an iterator over all of the " $singular " values."]
            #[inline]
            pub fn $plural(&self) -> impl ExactSizeIterator<Item = &$type> + Clone {
                self.$plural.raw.iter()
            }

            #[doc = "Returns a parallel iterator over all of the " $singular " values."]
            #[inline]
            pub fn [<par_ $plural>](&self) -> impl IndexedParallelIterator<Item = &$type> {
                self.$plural.raw.par_iter()
            }

            #[doc = "Returns an iterator over all of the " $singular " IDs and their associated values."]
            #[inline]
            pub fn [<$plural _enumerated>](&self) -> impl ExactSizeIterator<Item = ($id, &$type)> + Clone {
                // SAFETY: `$plural` is an IndexVec, which guarantees that all indexes are in bounds
                // of the respective index type.
                self.$plural().enumerate().map(|(i, v)| (unsafe { $id::from_usize_unchecked(i) }, v))
            }

            #[doc = "Returns an iterator over all of the " $singular " IDs and their associated values."]
            #[inline]
            pub fn [<par_ $plural _enumerated>](&self) -> impl IndexedParallelIterator<Item = ($id, &$type)> {
                // SAFETY: `$plural` is an IndexVec, which guarantees that all indexes are in bounds
                // of the respective index type.
                self.[<par_ $plural>]().enumerate().map(|(i, v)| (unsafe { $id::from_usize_unchecked(i) }, v))
            }
        )*

        pub(crate) fn shrink_to_fit(&mut self) {
            $(
                self.$plural.shrink_to_fit();
            )*
        }
    }};
}

impl<'hir> Hir<'hir> {
    pub(crate) fn new() -> Self {
        Self {
            sources: IndexVec::new(),
            contracts: IndexVec::new(),
            functions: IndexVec::new(),
            structs: IndexVec::new(),
            enums: IndexVec::new(),
            udvts: IndexVec::new(),
            events: IndexVec::new(),
            errors: IndexVec::new(),
            variables: IndexVec::new(),
        }
    }

    indexvec_methods! {
        source => sources, SourceId => Source<'hir>;
        contract => contracts, ContractId => Contract<'hir>;
        function => functions, FunctionId => Function<'hir>;
        strukt => structs, StructId => Struct<'hir>;
        enumm => enums, EnumId => Enum<'hir>;
        udvt => udvts, UdvtId => Udvt<'hir>;
        event => events, EventId => Event<'hir>;
        error => errors, ErrorId => Error<'hir>;
        variable => variables, VariableId => Variable<'hir>;
    }

    /// Returns the item associated with the given ID.
    #[inline]
    pub fn item(&self, id: impl Into<ItemId>) -> Item<'_, 'hir> {
        match id.into() {
            ItemId::Contract(id) => Item::Contract(self.contract(id)),
            ItemId::Function(id) => Item::Function(self.function(id)),
            ItemId::Variable(id) => Item::Variable(self.variable(id)),
            ItemId::Struct(id) => Item::Struct(self.strukt(id)),
            ItemId::Enum(id) => Item::Enum(self.enumm(id)),
            ItemId::Udvt(id) => Item::Udvt(self.udvt(id)),
            ItemId::Error(id) => Item::Error(self.error(id)),
            ItemId::Event(id) => Item::Event(self.event(id)),
        }
    }

    /// Returns an iterator over all item IDs.
    pub fn item_ids(&self) -> impl Iterator<Item = ItemId> + Clone {
        self.item_ids_vec().into_iter()
    }

    /// Returns a parallel iterator over all item IDs.
    pub fn par_item_ids(&self) -> impl ParallelIterator<Item = ItemId> {
        self.item_ids_vec().into_par_iter()
    }

    fn item_ids_vec(&self) -> Vec<ItemId> {
        // NOTE: This is essentially an unrolled `.chain().chain() ... .collect()` since it's not
        // very efficient.
        #[rustfmt::skip]
        let len =
              self.contracts.len()
            + self.functions.len()
            + self.variables.len()
            + self.structs.len()
            + self.enums.len()
            + self.udvts.len()
            + self.errors.len()
            + self.events.len();
        let mut v = Vec::<ItemId>::with_capacity(len);
        let mut items = v.spare_capacity_mut().iter_mut();
        macro_rules! extend_unchecked {
            ($iter:expr) => {
                for item in $iter {
                    unsafe { items.next().unwrap_unchecked().write(item) };
                }
            };
        }
        extend_unchecked!(self.contract_ids().map(ItemId::from));
        extend_unchecked!(self.function_ids().map(ItemId::from));
        extend_unchecked!(self.variable_ids().map(ItemId::from));
        extend_unchecked!(self.strukt_ids().map(ItemId::from));
        extend_unchecked!(self.enumm_ids().map(ItemId::from));
        extend_unchecked!(self.udvt_ids().map(ItemId::from));
        extend_unchecked!(self.error_ids().map(ItemId::from));
        extend_unchecked!(self.event_ids().map(ItemId::from));

        debug_assert!(items.next().is_none());
        unsafe { v.set_len(len) };
        debug_assert_eq!(v.len(), len);
        debug_assert_eq!(v.capacity(), len);

        v
    }

    /// Returns an iterator over all item IDs in a contract, including inheritance.
    pub fn contract_item_ids(
        &self,
        id: ContractId,
    ) -> impl Iterator<Item = ItemId> + Clone + use<'_> {
        self.contract(id)
            .linearized_bases
            .iter()
            .copied()
            .flat_map(|base| self.contract(base).items.iter().copied())
    }

    /// Returns an iterator over all items in a contract, including inheritance.
    pub fn contract_items(&self, id: ContractId) -> impl Iterator<Item = Item<'_, 'hir>> + Clone {
        self.contract_item_ids(id).map(move |id| self.item(id))
    }
}

newtype_index! {
    /// A [`Source`] ID.
    pub struct SourceId;

    /// A [`Contract`] ID.
    pub struct ContractId;

    /// A [`Function`] ID.
    pub struct FunctionId;

    /// A [`Struct`] ID.
    pub struct StructId;

    /// An [`Enum`] ID.
    pub struct EnumId;

    /// An [`Udvt`] ID.
    pub struct UdvtId;

    /// An [`Event`] ID.
    pub struct EventId;

    /// An [`Error`] ID.
    pub struct ErrorId;

    /// A [`Variable`] ID.
    pub struct VariableId;
}

newtype_index! {
    /// An [`Expr`] ID.
    pub struct ExprId;
}

/// A source file.
pub struct Source<'hir> {
    pub file: Arc<SourceFile>,
    pub imports: &'hir [(ast::ItemId, SourceId)],
    /// The source items.
    pub items: &'hir [ItemId],
}

impl fmt::Debug for Source<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Source")
            .field("file", &self.file.name)
            .field("imports", &self.imports)
            .field("items", &self.items)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, EnumIs)]
pub enum Item<'a, 'hir> {
    Contract(&'a Contract<'hir>),
    Function(&'a Function<'hir>),
    Struct(&'a Struct<'hir>),
    Enum(&'a Enum<'hir>),
    Udvt(&'a Udvt<'hir>),
    Error(&'a Error<'hir>),
    Event(&'a Event<'hir>),
    Variable(&'a Variable<'hir>),
}

impl<'hir> Item<'_, 'hir> {
    /// Returns the name of the item.
    #[inline]
    pub fn name(self) -> Option<Ident> {
        match self {
            Item::Contract(c) => Some(c.name),
            Item::Function(f) => f.name,
            Item::Struct(s) => Some(s.name),
            Item::Enum(e) => Some(e.name),
            Item::Udvt(u) => Some(u.name),
            Item::Error(e) => Some(e.name),
            Item::Event(e) => Some(e.name),
            Item::Variable(v) => v.name,
        }
    }

    /// Returns the description of the item.
    #[inline]
    pub fn description(self) -> &'static str {
        match self {
            Item::Contract(c) => c.description(),
            Item::Function(f) => f.description(),
            Item::Struct(_) => "struct",
            Item::Enum(_) => "enum",
            Item::Udvt(_) => "UDVT",
            Item::Error(_) => "error",
            Item::Event(_) => "event",
            Item::Variable(_) => "variable",
        }
    }

    /// Returns the span of the item.
    #[inline]
    pub fn span(self) -> Span {
        match self {
            Item::Contract(c) => c.span,
            Item::Function(f) => f.span,
            Item::Struct(s) => s.span,
            Item::Enum(e) => e.span,
            Item::Udvt(u) => u.span,
            Item::Error(e) => e.span,
            Item::Event(e) => e.span,
            Item::Variable(v) => v.span,
        }
    }

    /// Returns the contract ID if this item is part of a contract.
    #[inline]
    pub fn contract(self) -> Option<ContractId> {
        match self {
            Item::Contract(_) => None,
            Item::Function(f) => f.contract,
            Item::Struct(s) => s.contract,
            Item::Enum(e) => e.contract,
            Item::Udvt(u) => u.contract,
            Item::Error(e) => e.contract,
            Item::Event(e) => e.contract,
            Item::Variable(v) => v.contract,
        }
    }

    /// Returns the parameters of the item.
    #[inline]
    pub fn parameters(self) -> Option<&'hir [VariableId]> {
        Some(match self {
            Item::Struct(s) => s.fields,
            Item::Function(f) => f.parameters,
            Item::Event(e) => e.parameters,
            Item::Error(e) => e.parameters,
            _ => return None,
        })
    }

    /// Returns `true` if the item is visible in derived contracts.
    #[inline]
    pub fn is_visible_in_derived_contracts(self) -> bool {
        self.is_visible_in_contract() && self.visibility() >= Visibility::Internal
    }

    /// Returns `true` if the item is visible in the contract.
    #[inline]
    pub fn is_visible_in_contract(self) -> bool {
        (if let Item::Function(f) = self {
            matches!(f.kind, FunctionKind::Function | FunctionKind::Modifier)
        } else {
            true
        }) && self.visibility() != Visibility::External
    }

    /// Returns `true` if the item is public or external.
    #[inline]
    pub fn is_public(&self) -> bool {
        self.visibility() >= Visibility::Public
    }

    /// Returns the visibility of the item.
    #[inline]
    pub fn visibility(self) -> Visibility {
        match self {
            Item::Variable(v) => v.visibility.unwrap_or(Visibility::Internal),
            Item::Contract(_)
            | Item::Function(_)
            | Item::Struct(_)
            | Item::Enum(_)
            | Item::Udvt(_)
            | Item::Error(_)
            | Item::Event(_) => Visibility::Public,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, From, EnumIs)]
pub enum ItemId {
    Contract(ContractId),
    Function(FunctionId),
    Variable(VariableId),
    Struct(StructId),
    Enum(EnumId),
    Udvt(UdvtId),
    Error(ErrorId),
    Event(EventId),
}

impl fmt::Debug for ItemId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ItemId::")?;
        match self {
            Self::Contract(id) => id.fmt(f),
            Self::Function(id) => id.fmt(f),
            Self::Variable(id) => id.fmt(f),
            Self::Struct(id) => id.fmt(f),
            Self::Enum(id) => id.fmt(f),
            Self::Udvt(id) => id.fmt(f),
            Self::Error(id) => id.fmt(f),
            Self::Event(id) => id.fmt(f),
        }
    }
}

impl ItemId {
    /// Returns the description of the item.
    pub fn description(&self) -> &'static str {
        match self {
            Self::Contract(_) => "contract",
            Self::Function(_) => "function",
            Self::Variable(_) => "variable",
            Self::Struct(_) => "struct",
            Self::Enum(_) => "enum",
            Self::Udvt(_) => "UDVT",
            Self::Error(_) => "error",
            Self::Event(_) => "event",
        }
    }

    /// Returns `true` if the **item kinds** match.
    #[inline]
    pub fn matches(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }

    /// Returns the contract ID if this is a contract.
    pub fn as_contract(&self) -> Option<ContractId> {
        if let Self::Contract(v) = *self {
            Some(v)
        } else {
            None
        }
    }

    /// Returns the function ID if this is a function.
    pub fn as_function(&self) -> Option<FunctionId> {
        if let Self::Function(v) = *self {
            Some(v)
        } else {
            None
        }
    }

    /// Returns the variable ID if this is a variable.
    pub fn as_variable(&self) -> Option<VariableId> {
        if let Self::Variable(v) = *self {
            Some(v)
        } else {
            None
        }
    }
}

/// A contract, interface, or library.
#[derive(Debug)]
pub struct Contract<'hir> {
    /// The source this contract is defined in.
    pub source: SourceId,
    /// The contract span.
    pub span: Span,
    /// The contract name.
    pub name: Ident,
    /// The contract kind.
    pub kind: ContractKind,
    /// The contract bases, as declared in the source code.
    pub bases: &'hir [ContractId],
    /// The linearized contract bases.
    ///
    /// The first element is the contract itself, followed by its bases in order of inheritance.
    pub linearized_bases: &'hir [ContractId],
    /// The resolved constructor function.
    pub ctor: Option<FunctionId>,
    /// The resolved `fallback` function.
    pub fallback: Option<FunctionId>,
    /// The resolved `receive` function.
    pub receive: Option<FunctionId>,
    /// The contract items.
    ///
    /// Note that this only includes items defined in the contract itself, not inherited items.
    /// For getting all items, use [`Hir::contract_items`].
    pub items: &'hir [ItemId],
}

impl Contract<'_> {
    /// Returns an iterator over functions declared in the contract.
    ///
    /// Note that this does not include the constructor and fallback functions, as they are stored
    /// separately. Use [`Contract::all_functions`] to include them.
    pub fn functions(&self) -> impl Iterator<Item = FunctionId> + Clone + use<'_> {
        self.items.iter().filter_map(ItemId::as_function)
    }

    /// Returns an iterator over all functions declared in the contract.
    pub fn all_functions(&self) -> impl Iterator<Item = FunctionId> + Clone + use<'_> {
        self.functions().chain(self.ctor).chain(self.fallback).chain(self.receive)
    }

    /// Returns an iterator over all variables declared in the contract.
    pub fn variables(&self) -> impl Iterator<Item = VariableId> + Clone + use<'_> {
        self.items.iter().filter_map(ItemId::as_variable)
    }

    /// Returns `true` if the contract can be deployed.
    pub fn can_be_deployed(&self) -> bool {
        matches!(self.kind, ContractKind::Contract | ContractKind::Library)
    }

    /// Returns `true` if this is an abstract contract.
    pub fn is_abstract(&self) -> bool {
        self.kind.is_abstract_contract()
    }

    /// Returns the description of the contract.
    pub fn description(&self) -> &'static str {
        self.kind.to_str()
    }
}

/// A function.
#[derive(Debug)]
pub struct Function<'hir> {
    /// The source this function is defined in.
    pub source: SourceId,
    /// The contract this function is defined in, if any.
    pub contract: Option<ContractId>,
    /// The function span.
    pub span: Span,
    /// The function name.
    /// Only `None` if this is a constructor, fallback, or receive function.
    pub name: Option<Ident>,
    /// The function kind.
    pub kind: FunctionKind,
    /// The visibility of the function.
    pub visibility: Visibility,
    /// The state mutability of the function.
    pub state_mutability: StateMutability,
    /// Modifiers, or base classes if this is a constructor.
    pub modifiers: &'hir [ItemId],
    /// Whether this function is marked with the `virtual` keyword.
    pub marked_virtual: bool,
    /// Whether this function is marked with the `virtual` keyword or is defined in an interface.
    pub virtual_: bool,
    /// Whether this function is marked with the `override` keyword.
    pub override_: bool,
    pub overrides: &'hir [ContractId],
    /// The function parameters.
    pub parameters: &'hir [VariableId],
    /// The function returns.
    pub returns: &'hir [VariableId],
    /// The function body.
    pub body: Option<Block<'hir>>,
    /// The function body span.
    pub body_span: Span,
    /// The variable this function is a getter of, if any.
    pub gettee: Option<VariableId>,
}

impl Function<'_> {
    /// Returns `true` if this is a free function, meaning it is not part of a contract.
    pub fn is_free(&self) -> bool {
        self.contract.is_none()
    }

    pub fn is_ordinary(&self) -> bool {
        self.kind.is_ordinary()
    }

    /// Returns `true` if this is a getter function of a variable.
    pub fn is_getter(&self) -> bool {
        self.gettee.is_some()
    }

    pub fn is_part_of_external_interface(&self) -> bool {
        self.is_ordinary() && self.visibility >= Visibility::Public
    }

    /// Returns an iterator over all variables in the function.
    pub fn variables(&self) -> impl DoubleEndedIterator<Item = VariableId> + Clone + use<'_> {
        self.parameters.iter().copied().chain(self.returns.iter().copied())
    }

    /// Returns the description of the function.
    pub fn description(&self) -> &'static str {
        if self.is_getter() {
            "getter function"
        } else {
            self.kind.to_str()
        }
    }
}

/// A struct.
#[derive(Debug)]
pub struct Struct<'hir> {
    /// The source this struct is defined in.
    pub source: SourceId,
    /// The contract this struct is defined in, if any.
    pub contract: Option<ContractId>,
    /// The struct span.
    pub span: Span,
    /// The struct name.
    pub name: Ident,
    pub fields: &'hir [VariableId],
}

/// An enum.
#[derive(Debug)]
pub struct Enum<'hir> {
    /// The source this enum is defined in.
    pub source: SourceId,
    /// The contract this enum is defined in, if any.
    pub contract: Option<ContractId>,
    /// The enum span.
    pub span: Span,
    /// The enum name.
    pub name: Ident,
    /// The enum variants.
    pub variants: &'hir [Ident],
}

/// A user-defined value type.
#[derive(Debug)]
pub struct Udvt<'hir> {
    /// The source this UDVT is defined in.
    pub source: SourceId,
    /// The contract this UDVT is defined in, if any.
    pub contract: Option<ContractId>,
    /// The UDVT span.
    pub span: Span,
    /// The UDVT name.
    pub name: Ident,
    /// The UDVT type.
    pub ty: Type<'hir>,
}

/// An event.
#[derive(Debug)]
pub struct Event<'hir> {
    /// The source this event is defined in.
    pub source: SourceId,
    /// The contract this event is defined in, if any.
    pub contract: Option<ContractId>,
    /// The event span.
    pub span: Span,
    /// The event name.
    pub name: Ident,
    /// Whether this event is anonymous.
    pub anonymous: bool,
    pub parameters: &'hir [VariableId],
}

/// An event parameter.
#[derive(Debug)]
pub struct EventParameter<'hir> {
    pub ty: Type<'hir>,
    pub indexed: bool,
    pub name: Option<Ident>,
}

/// A custom error.
#[derive(Debug)]
pub struct Error<'hir> {
    /// The source this error is defined in.
    pub source: SourceId,
    /// The contract this error is defined in, if any.
    pub contract: Option<ContractId>,
    /// The error span.
    pub span: Span,
    /// The error name.
    pub name: Ident,
    pub parameters: &'hir [VariableId],
}

/// A constant or variable declaration.
#[derive(Debug)]
pub struct Variable<'hir> {
    /// The source this variable is defined in.
    pub source: SourceId,
    /// The contract this variable is defined in, if any.
    pub contract: Option<ContractId>,
    /// The function this variable is defined in, if any.
    pub function: Option<FunctionId>,
    /// The variable's span.
    pub span: Span,
    /// The kind of variable.
    pub kind: VarKind,
    /// The variable's type.
    pub ty: Type<'hir>,
    /// The variable's name.
    pub name: Option<Ident>,
    /// The visibility of the variable.
    pub visibility: Option<Visibility>,
    pub mutability: Option<VarMut>,
    pub data_location: Option<DataLocation>,
    pub override_: bool,
    pub overrides: &'hir [ContractId],
    pub indexed: bool,
    pub initializer: Option<&'hir Expr<'hir>>,
    /// The compiler-generated getter function, if any.
    pub getter: Option<FunctionId>,
}

impl<'hir> Variable<'hir> {
    /// Creates a new variable.
    pub fn new(source: SourceId, ty: Type<'hir>, name: Option<Ident>, kind: VarKind) -> Self {
        Self {
            source,
            contract: None,
            function: None,
            span: Span::DUMMY,
            kind,
            ty,
            name,
            visibility: None,
            mutability: None,
            data_location: None,
            override_: false,
            overrides: &[],
            indexed: false,
            initializer: None,
            getter: None,
        }
    }

    /// Creates a new variable statement.
    pub fn new_stmt(
        source: SourceId,
        contract: ContractId,
        function: FunctionId,
        ty: Type<'hir>,
        name: Ident,
    ) -> Self {
        Self {
            contract: Some(contract),
            function: Some(function),
            ..Self::new(source, ty, Some(name), VarKind::Statement)
        }
    }

    /// Returns the description of the variable.
    pub fn description(&self) -> &'static str {
        self.kind.to_str()
    }

    /// Returns `true` if the variable is [`constant`](VarMut::Constant).
    pub fn is_constant(&self) -> bool {
        self.mutability == Some(VarMut::Constant)
    }

    /// Returns `true` if the variable is [`immutable`](VarMut::Immutable).
    pub fn is_immutable(&self) -> bool {
        self.mutability == Some(VarMut::Immutable)
    }

    pub fn is_l_value(&self) -> bool {
        !self.is_constant()
    }

    pub fn is_struct_member(&self) -> bool {
        matches!(self.kind, VarKind::Struct)
    }

    pub fn is_event_or_error_parameter(&self) -> bool {
        matches!(self.kind, VarKind::Event | VarKind::Error)
    }

    pub fn is_local_variable(&self) -> bool {
        matches!(
            self.kind,
            VarKind::FunctionTyParam
                | VarKind::FunctionTyReturn
                | VarKind::Event
                | VarKind::Error
                | VarKind::FunctionParam
                | VarKind::FunctionReturn
                | VarKind::Statement
                | VarKind::TryCatch
        )
    }

    pub fn is_callable_or_catch_parameter(&self) -> bool {
        matches!(
            self.kind,
            VarKind::Event
                | VarKind::Error
                | VarKind::FunctionParam
                | VarKind::FunctionTyParam
                | VarKind::FunctionReturn
                | VarKind::FunctionTyReturn
                | VarKind::TryCatch
        )
    }

    pub fn is_local_or_return(&self) -> bool {
        self.is_return_parameter()
            || (self.is_local_variable() && !self.is_callable_or_catch_parameter())
    }

    pub fn is_return_parameter(&self) -> bool {
        matches!(self.kind, VarKind::FunctionReturn | VarKind::FunctionTyReturn)
    }

    pub fn is_try_catch_parameter(&self) -> bool {
        matches!(self.kind, VarKind::TryCatch)
    }

    /// Returns `true` if the variable is a state variable.
    pub fn is_state_variable(&self) -> bool {
        self.kind.is_state()
    }

    pub fn is_file_level_variable(&self) -> bool {
        matches!(self.kind, VarKind::Global)
    }

    /// Returns `true` if the variable is public.
    pub fn is_public(&self) -> bool {
        self.visibility >= Some(Visibility::Public)
    }
}

/// The kind of variable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumIs)]
pub enum VarKind {
    /// Defined at the top level.
    Global,
    /// Defined in a contract.
    State,
    /// Defined in a struct.
    Struct,
    /// Defined in an event.
    Event,
    /// Defined in an error.
    Error,
    /// Defined as a function parameter.
    FunctionParam,
    /// Defined as a function return.
    FunctionReturn,
    /// Defined as a function type parameter.
    FunctionTyParam,
    /// Defined as a function type return.
    FunctionTyReturn,
    /// Defined as a statement, inside of a function, block or `for` statement.
    Statement,
    /// Defined in a catch clause.
    TryCatch,
}

impl VarKind {
    pub fn to_str(self) -> &'static str {
        match self {
            Self::Global => "file-level variable",
            Self::State => "state variable",
            Self::Struct => "struct field",
            Self::Event => "event parameter",
            Self::Error => "error parameter",
            Self::FunctionParam | Self::FunctionTyParam => "function parameter",
            Self::FunctionReturn | Self::FunctionTyReturn => "function return parameter",
            Self::Statement => "variable",
            Self::TryCatch => "try/catch clause",
        }
    }
}

/// A block of statements.
pub type Block<'hir> = &'hir [Stmt<'hir>];

/// A statement.
#[derive(Debug)]
pub struct Stmt<'hir> {
    /// The statement span.
    pub span: Span,
    pub kind: StmtKind<'hir>,
}

/// A kind of statement.
#[derive(Debug)]
pub enum StmtKind<'hir> {
    // TODO: Yul to HIR.
    // /// An assembly block, with optional flags: `assembly "evmasm" (...) { ... }`.
    // Assembly(StmtAssembly<'hir>),
    /// A single-variable declaration statement: `uint256 foo = 42;`.
    DeclSingle(VariableId),

    /// A multi-variable declaration statement: `(bool success, bytes memory value) = ...;`.
    ///
    /// Multi-assignments require an expression on the right-hand side.
    DeclMulti(&'hir [Option<VariableId>], &'hir Expr<'hir>),

    /// A blocked scope: `{ ... }`.
    Block(Block<'hir>),

    /// An unchecked block: `unchecked { ... }`.
    UncheckedBlock(Block<'hir>),

    /// An emit statement: `emit Foo.bar(42);`.
    ///
    /// Always contains an `ExprKind::Call`.
    Emit(&'hir Expr<'hir>),

    /// A revert statement: `revert Foo.bar(42);`.
    ///
    /// Always contains an `ExprKind::Call`.
    Revert(&'hir Expr<'hir>),

    /// A return statement: `return 42;`.
    Return(Option<&'hir Expr<'hir>>),

    /// A break statement: `break;`.
    Break,

    /// A continue statement: `continue;`.
    Continue,

    /// A loop statement. This is desugared from all `for`, `while`, and `do while` statements.
    Loop(Block<'hir>, LoopSource),

    /// An `if` statement with an optional `else` block: `if (expr) { ... } else { ... }`.
    If(&'hir Expr<'hir>, &'hir Stmt<'hir>, Option<&'hir Stmt<'hir>>),

    /// A try statement: `try fooBar(42) returns (...) { ... } catch (...) { ... }`.
    Try(&'hir StmtTry<'hir>),

    /// An expression with a trailing semicolon.
    Expr(&'hir Expr<'hir>),

    /// A modifier placeholder statement: `_;`.
    Placeholder,

    Err(ErrorGuaranteed),
}

/// A try statement: `try fooBar(42) returns (...) { ... } catch (...) { ... }`.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.tryStatement>
#[derive(Debug)]
pub struct StmtTry<'hir> {
    /// The call expression.
    pub expr: Expr<'hir>,
    /// The list of clauses. Never empty.
    ///
    /// The first item is always the `returns` clause.
    pub clauses: &'hir [TryCatchClause<'hir>],
}

/// Clause of a try/catch block: `returns/catch (...) { ... }`.
///
/// Includes both the successful case and the unsuccessful cases.
/// Names are only allowed for unsuccessful cases.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.catchClause>
#[derive(Debug)]
pub struct TryCatchClause<'hir> {
    pub name: Option<Ident>,
    pub args: &'hir [VariableId],
    pub block: Block<'hir>,
}

/// The loop type that yielded an [`StmtKind::Loop`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopSource {
    /// A `for (...) { ... }` loop.
    For,
    /// A `while (...) { ... }` loop.
    While,
    /// A `do { ... } while (...);` loop.
    DoWhile,
}

impl LoopSource {
    /// Returns the name of the loop source.
    pub fn name(self) -> &'static str {
        match self {
            Self::For => "for",
            Self::While => "while",
            Self::DoWhile => "do while",
        }
    }
}

/// Resolved name.
#[derive(Clone, Copy, PartialEq, Eq, Hash, From, EnumIs)]
pub enum Res {
    /// A resolved item.
    Item(ItemId),
    /// Synthetic import namespace, X in `import * as X from "path"` or `import "path" as X`.
    Namespace(SourceId),
    /// A builtin symbol.
    Builtin(Builtin),
    /// An error occurred while resolving the item. Silences further errors regarding this name.
    Err(ErrorGuaranteed),
}

impl fmt::Debug for Res {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Res::")?;
        match self {
            Self::Item(id) => write!(f, "Item({id:?})"),
            Self::Namespace(id) => write!(f, "Namespace({id:?})"),
            Self::Builtin(b) => write!(f, "Builtin({b:?})"),
            Self::Err(_) => f.write_str("Err"),
        }
    }
}

macro_rules! impl_try_from {
    ($($t:ty => $pat:pat => $e:expr),* $(,)?) => {
        $(
            impl TryFrom<Res> for $t {
                type Error = ();

                fn try_from(decl: Res) -> Result<Self, ()> {
                    match decl {
                        $pat => $e,
                        _ => Err(()),
                    }
                }
            }
        )*
    };
}

impl_try_from!(
    ItemId => Res::Item(id) => Ok(id),
    ContractId => Res::Item(ItemId::Contract(id)) => Ok(id),
    // FunctionId => Res::Item(ItemId::Function(id)) => Ok(id),
    EventId => Res::Item(ItemId::Event(id)) => Ok(id),
    ErrorId => Res::Item(ItemId::Error(id)) => Ok(id),
);

impl Res {
    pub fn description(&self) -> &'static str {
        match self {
            Self::Item(item) => item.description(),
            Self::Namespace(_) => "namespace",
            Self::Builtin(_) => "builtin",
            Self::Err(_) => "<error>",
        }
    }

    pub fn matches(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Item(a), Self::Item(b)) => a.matches(b),
            _ => std::mem::discriminant(self) == std::mem::discriminant(other),
        }
    }

    pub fn as_variable(&self) -> Option<VariableId> {
        if let Self::Item(id) = self {
            id.as_variable()
        } else {
            None
        }
    }
}

/// An expression.
#[derive(Debug)]
pub struct Expr<'hir> {
    pub id: ExprId,
    pub kind: ExprKind<'hir>,
    /// The expression span.
    pub span: Span,
}

impl Expr<'_> {
    /// Peels off unnecessary parentheses from the expression.
    pub fn peel_parens(&self) -> &Self {
        let mut expr = self;
        while let ExprKind::Tuple([Some(inner)]) = expr.kind {
            expr = inner;
        }
        expr
    }
}

/// A kind of expression.
#[derive(Debug)]
pub enum ExprKind<'hir> {
    /// An array literal expression: `[a, b, c, d]`.
    Array(&'hir [Expr<'hir>]),

    /// An assignment: `a = b`, `a += b`.
    Assign(&'hir Expr<'hir>, Option<BinOp>, &'hir Expr<'hir>),

    /// A binary operation: `a + b`, `a >> b`.
    Binary(&'hir Expr<'hir>, BinOp, &'hir Expr<'hir>),

    /// A function call expression: `foo(42)`, `foo({ bar: 42 })`, `foo{ gas: 100_000 }(42)`.
    Call(&'hir Expr<'hir>, CallArgs<'hir>, Option<&'hir [NamedArg<'hir>]>),

    // TODO: Add a MethodCall variant
    /// A unary `delete` expression: `delete vector`.
    Delete(&'hir Expr<'hir>),

    /// A resolved symbol: `foo`.
    ///
    /// Potentially multiple references if it refers to something like an overloaded function.
    Ident(&'hir [Res]),

    /// A square bracketed indexing expression: `vector[index]`, `MyType[]`.
    Index(&'hir Expr<'hir>, Option<&'hir Expr<'hir>>),

    /// A square bracketed slice expression: `slice[l:r]`.
    Slice(&'hir Expr<'hir>, Option<&'hir Expr<'hir>>, Option<&'hir Expr<'hir>>),

    /// A literal: `hex"1234"`, `5.6 ether`.
    Lit(&'hir Lit),

    /// Access of a named member: `obj.k`.
    Member(&'hir Expr<'hir>, Ident),

    /// A `new` expression: `new Contract`.
    New(Type<'hir>),

    /// A `payable` expression: `payable(address(0x...))`.
    Payable(&'hir Expr<'hir>),

    /// A ternary (AKA conditional) expression: `foo ? bar : baz`.
    Ternary(&'hir Expr<'hir>, &'hir Expr<'hir>, &'hir Expr<'hir>),

    /// A tuple expression: `(a,,, b, c, d)`.
    Tuple(&'hir [Option<&'hir Expr<'hir>>]),

    /// A `type()` expression: `type(uint256)`.
    TypeCall(Type<'hir>),

    /// An elementary type name: `uint256`.
    Type(Type<'hir>),

    /// A unary operation: `!x`, `-x`, `x++`.
    Unary(UnOp, &'hir Expr<'hir>),

    Err(ErrorGuaranteed),
}

/// A named argument: `name: value`.
#[derive(Debug)]
pub struct NamedArg<'hir> {
    pub name: Ident,
    pub value: Expr<'hir>,
}

/// A list of function call arguments.
#[derive(Debug)]
pub struct CallArgs<'hir> {
    /// The span of the arguments. This points to the parenthesized list of arguments.
    ///
    /// If the list is empty, this points to the empty `()` or to where the `(` would be.
    pub span: Span,
    pub kind: CallArgsKind<'hir>,
}

impl<'hir> CallArgs<'hir> {
    /// Creates a new empty list of arguments.
    ///
    /// `span` should be an empty span.
    pub fn empty(span: Span) -> Self {
        Self { span, kind: CallArgsKind::empty() }
    }

    /// Returns `true` if the argument list is not present in the source code.
    ///
    /// For example, a modifier `m` can be invoked in a function declaration as `m` or `m()`. In the
    /// first case, this returns `true`, and the span will point to after `m`. In the second case,
    /// this returns `false`.
    pub fn is_dummy(&self) -> bool {
        self.span.lo() == self.span.hi()
    }

    /// Returns the length of the arguments.
    pub fn len(&self) -> usize {
        self.kind.len()
    }

    /// Returns `true` if the list of arguments is empty.
    pub fn is_empty(&self) -> bool {
        self.kind.is_empty()
    }

    /// Returns an iterator over the expressions.
    pub fn exprs(
        &self,
    ) -> impl ExactSizeIterator<Item = &Expr<'hir>> + DoubleEndedIterator + Clone {
        self.kind.exprs()
    }
}

/// A list of function call argument expressions.
#[derive(Debug)]
pub enum CallArgsKind<'hir> {
    /// A list of unnamed arguments: `(1, 2, 3)`.
    Unnamed(&'hir [Expr<'hir>]),

    /// A list of named arguments: `({x: 1, y: 2, z: 3})`.
    Named(&'hir [NamedArg<'hir>]),
}

impl Default for CallArgsKind<'_> {
    fn default() -> Self {
        Self::empty()
    }
}

impl<'hir> CallArgsKind<'hir> {
    /// Creates a new empty list of unnamed arguments.
    pub fn empty() -> Self {
        Self::Unnamed(Default::default())
    }

    /// Returns the length of the arguments.
    pub fn len(&self) -> usize {
        match self {
            Self::Unnamed(exprs) => exprs.len(),
            Self::Named(args) => args.len(),
        }
    }

    /// Returns `true` if the list of arguments is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns an iterator over the expressions.
    pub fn exprs(
        &self,
    ) -> impl ExactSizeIterator<Item = &Expr<'hir>> + DoubleEndedIterator + Clone {
        match self {
            Self::Unnamed(exprs) => Either::Left(exprs.iter()),
            Self::Named(args) => Either::Right(args.iter().map(|arg| &arg.value)),
        }
    }

    /// Returns the span of the argument expressions. Does not include the parentheses.
    pub fn span(&self) -> Option<Span> {
        if self.is_empty() {
            return None;
        }
        Some(Span::join_first_last(self.exprs().map(|e| e.span)))
    }
}

/// A type name.
#[derive(Clone, Debug)]
pub struct Type<'hir> {
    pub span: Span,
    pub kind: TypeKind<'hir>,
}

impl<'hir> Type<'hir> {
    /// Dummy placeholder type.
    pub const DUMMY: Self =
        Self { span: Span::DUMMY, kind: TypeKind::Err(ErrorGuaranteed::new_unchecked()) };

    /// Returns `true` if the type is a dummy type.
    pub fn is_dummy(&self) -> bool {
        self.span == Span::DUMMY && matches!(self.kind, TypeKind::Err(_))
    }

    pub fn visit<T>(
        &self,
        hir: &Hir<'hir>,
        f: &mut impl FnMut(&Self) -> ControlFlow<T>,
    ) -> ControlFlow<T> {
        f(self)?;
        match self.kind {
            TypeKind::Elementary(_) => ControlFlow::Continue(()),
            TypeKind::Array(ty) => ty.element.visit(hir, f),
            TypeKind::Function(ty) => {
                for &param in ty.parameters {
                    hir.variable(param).ty.visit(hir, f)?;
                }
                for &ret in ty.returns {
                    hir.variable(ret).ty.visit(hir, f)?;
                }
                ControlFlow::Continue(())
            }
            TypeKind::Mapping(ty) => {
                ty.key.visit(hir, f)?;
                ty.value.visit(hir, f)
            }
            TypeKind::Custom(_) => ControlFlow::Continue(()),
            TypeKind::Err(_) => ControlFlow::Continue(()),
        }
    }
}

/// The kind of a type.
#[derive(Clone, Debug)]
pub enum TypeKind<'hir> {
    /// An elementary/primitive type.
    Elementary(ElementaryType),

    /// `$element[$($size)?]`
    Array(&'hir TypeArray<'hir>),
    /// `function($($parameters),*) $($attributes)* $(returns ($($returns),+))?`
    Function(&'hir TypeFunction<'hir>),
    /// `mapping($key $($key_name)? => $value $($value_name)?)`
    Mapping(&'hir TypeMapping<'hir>),

    /// A custom type name.
    Custom(ItemId),

    Err(ErrorGuaranteed),
}

impl TypeKind<'_> {
    /// Returns `true` if the type is an elementary type.
    pub fn is_elementary(&self) -> bool {
        matches!(self, Self::Elementary(_))
    }

    /// Returns `true` if the type is a reference type.
    #[inline]
    pub fn is_reference_type(&self) -> bool {
        match self {
            TypeKind::Elementary(t) => t.is_reference_type(),
            TypeKind::Custom(ItemId::Struct(_)) | TypeKind::Array(_) => true,
            _ => false,
        }
    }
}

/// An array type.
#[derive(Debug)]
pub struct TypeArray<'hir> {
    pub element: Type<'hir>,
    pub size: Option<&'hir Expr<'hir>>,
}

/// A function type name.
#[derive(Debug)]
pub struct TypeFunction<'hir> {
    pub parameters: &'hir [VariableId],
    pub visibility: Visibility,
    pub state_mutability: StateMutability,
    pub returns: &'hir [VariableId],
}

/// A mapping type.
#[derive(Debug)]
pub struct TypeMapping<'hir> {
    pub key: Type<'hir>,
    pub key_name: Option<Ident>,
    pub value: Type<'hir>,
    pub value_name: Option<Ident>,
}
