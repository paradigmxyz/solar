use std::{fmt, marker::PhantomData, sync::Arc};
use sulk_ast::ast;
use sulk_data_structures::{
    index::{Idx, IndexVec},
    newtype_index,
};
use sulk_interface::{source_map::SourceFile, Ident, Span};

pub use sulk_ast::ast::{ContractKind, FunctionKind, StateMutability, Visibility};

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
    pub(crate) vars: IndexVec<VarId, Var<'hir>>,
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
            vars: IndexVec::new(),
        }
    }
}

macro_rules! indexvec_methods {
    ($($singular:ident => $plural:ident, $id:ty => $type:ty;)*) => { paste::paste! {
        impl<'hir> Hir<'hir> {
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
                pub fn [<$singular _ids>](&self) -> impl ExactSizeIterator<Item = $id> + DoubleEndedIterator {
                    (0..self.$plural.len()).map($id::from_usize)
                }

                #[doc = "Returns an iterator over all of the " $singular " values."]
                #[inline]
                pub fn $plural(&self) -> impl ExactSizeIterator<Item = &$type> + DoubleEndedIterator {
                    self.$plural.raw.iter()
                }

                #[doc = "Returns an iterator over all of the " $singular " IDs and their associated values."]
                #[inline]
                pub fn [<$plural _enumerated>](&self) -> impl ExactSizeIterator<Item = ($id, &$type)> + DoubleEndedIterator {
                    self.$plural().enumerate().map(|(i, v)| ($id::from_usize(i), v))
                }
            )*

            pub(crate) fn shrink_to_fit(&mut self) {
                $(
                    self.$plural.shrink_to_fit();
                )*
            }
        }
    }};
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
    var => vars, VarId => Var<'hir>;
}

impl<'hir> Hir<'hir> {
    /// Returns the item associated with the given ID.
    pub fn item(&self, id: ItemId) -> Item<'_, 'hir> {
        match id {
            ItemId::Contract(id) => Item::Contract(self.contract(id)),
            ItemId::Function(id) => Item::Function(self.function(id)),
            ItemId::Var(id) => Item::Var(self.var(id)),
            ItemId::Struct(id) => Item::Struct(self.strukt(id)),
            ItemId::Enum(id) => Item::Enum(self.enumm(id)),
            ItemId::Udvt(id) => Item::Udvt(self.udvt(id)),
            ItemId::Error(id) => Item::Error(self.error(id)),
            ItemId::Event(id) => Item::Event(self.event(id)),
        }
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

    /// A [`Var`] ID.
    pub struct VarId;
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

#[derive(Clone, Copy, Debug)]
pub enum Item<'a, 'hir> {
    Contract(&'a Contract<'hir>),
    Function(&'a Function<'hir>),
    Struct(&'a Struct<'hir>),
    Enum(&'a Enum<'hir>),
    Udvt(&'a Udvt<'hir>),
    Error(&'a Error<'hir>),
    Event(&'a Event<'hir>),
    Var(&'a Var<'hir>),
}

impl Item<'_, '_> {
    /// Returns the name of the item.
    pub fn name(self) -> Option<Ident> {
        match self {
            Item::Contract(c) => Some(c.name),
            Item::Function(f) => f.name,
            Item::Struct(s) => Some(s.name),
            Item::Enum(e) => Some(e.name),
            Item::Udvt(u) => Some(u.name),
            Item::Error(e) => Some(e.name),
            Item::Event(e) => Some(e.name),
            Item::Var(v) => v.name,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemId {
    Contract(ContractId),
    Function(FunctionId),
    Var(VarId),
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
            Self::Var(id) => id.fmt(f),
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
            Self::Var(_) => "variable",
            Self::Struct(_) => "struct",
            Self::Enum(_) => "enum",
            Self::Udvt(_) => "UDVT",
            Self::Error(_) => "error",
            Self::Event(_) => "event",
        }
    }

    /// Returns the contract ID if this is a contract.
    pub fn as_contract(&self) -> Option<&ContractId> {
        if let Self::Contract(v) = self {
            Some(v)
        } else {
            None
        }
    }
}

/// A contract, interface, or library.
#[derive(Debug)]
pub struct Contract<'hir> {
    /// The contract name.
    pub name: Ident,
    /// The contract span.
    pub span: Span,
    /// The contract kind.
    pub kind: ContractKind,
    /// The source this contract is defined in.
    pub source_id: SourceId,
    /// The contract bases.
    pub bases: &'hir [ContractId],
    /// The linearized contract bases.
    pub linearized_bases: &'hir [ContractId],
    /// The constructor function.
    pub ctor: Option<FunctionId>,
    /// The `fallback` function.
    pub fallback: Option<FunctionId>,
    /// The `receive` function.
    pub receive: Option<FunctionId>,
    /// The contract items.
    pub items: &'hir [ItemId],
}

/// A function.
#[derive(Debug)]
pub struct Function<'hir> {
    /// The function name.
    /// Only `None` if this is a constructor, fallback, or receive function.
    pub name: Option<Ident>,
    /// The function span.
    pub span: Span,
    pub _tmp: PhantomData<&'hir ()>,
}

/// A struct.
#[derive(Debug)]
pub struct Struct<'hir> {
    /// The struct name.
    pub name: Ident,
    /// The struct span.
    pub span: Span,
    pub _tmp: PhantomData<&'hir ()>,
}

/// An enum.
#[derive(Debug)]
pub struct Enum<'hir> {
    /// The enum name.
    pub name: Ident,
    /// The enum span.
    pub span: Span,
    /// The enum variants.
    pub variants: &'hir [Ident],
}

/// A user-defined value type.
#[derive(Debug)]
pub struct Udvt<'hir> {
    /// The UDVT name.
    pub name: Ident,
    /// The UDVT span.
    pub span: Span,
    pub _tmp: PhantomData<&'hir ()>,
}

/// An event.
#[derive(Debug)]
pub struct Event<'hir> {
    /// The event name.
    pub name: Ident,
    /// The event span.
    pub span: Span,
    pub _tmp: PhantomData<&'hir ()>,
}

/// A custom error.
#[derive(Debug)]
pub struct Error<'hir> {
    /// The error name.
    pub name: Ident,
    /// The error span.
    pub span: Span,
    pub _tmp: PhantomData<&'hir ()>,
}

/// A constant or variable declaration.
#[derive(Debug)]
pub struct Var<'hir> {
    /// The variable name.
    pub name: Option<Ident>,
    /// The variable span.
    pub span: Span,
    pub _tmp: PhantomData<&'hir ()>,
}

/// A statement.
#[derive(Debug)]
pub struct Stmt<'hir> {
    /// The statement span.
    pub span: Span,
    pub _tmp: PhantomData<&'hir ()>,
}

/// An expression.
#[derive(Debug)]
pub struct Expr<'hir> {
    /// The expression span.
    pub span: Span,
    pub _tmp: PhantomData<&'hir ()>,
}
