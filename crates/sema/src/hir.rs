use std::{marker::PhantomData, sync::Arc};
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
pub struct Hir<'hir> {
    /// All sources.
    pub(crate) sources: IndexVec<SourceId, Source>,
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
                    self.$plural.indices()
                }

                #[doc = "Returns an iterator over all of the " $singular " IDs and their associated values."]
                #[inline]
                pub fn $plural(&self) -> impl ExactSizeIterator<Item = ($id, &$type)> + DoubleEndedIterator {
                    self.$plural.iter_enumerated()
                }
            )*
        }
    }};
}

indexvec_methods! {
    source => sources, SourceId => Source;
    contract => contracts, ContractId => Contract<'hir>;
    function => functions, FunctionId => Function<'hir>;
    strukt => structs, StructId => Struct<'hir>;
    enumm => enums, EnumId => Enum<'hir>;
    udvt => udvts, UdvtId => Udvt<'hir>;
    event => events, EventId => Event<'hir>;
    error => errors, ErrorId => Error<'hir>;
    var => vars, VarId => Var<'hir>;
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
#[derive(Debug)]
pub struct Source {
    pub file: Arc<SourceFile>,
    /// The AST of the source. None if Yul, parsing failed, or is after lowering where it's no
    /// longer needed.
    pub ast: Option<ast::SourceUnit>,
    pub imports: Vec<(ast::ItemId, SourceId)>,
}

/// A contract, interface, or library.
#[derive(Debug)]
pub struct Contract<'hir> {
    /// The function name.
    pub name: Ident,
    /// The contract kind.
    pub kind: ContractKind,
    /// The contract bases.
    pub bases: &'hir [ContractId],
    /// The constructor function.
    pub ctor: Option<FunctionId>,
    /// The `fallback` function.
    pub fallback: Option<FunctionId>,
    /// The `receive` function.
    pub receive: Option<FunctionId>,
    /// The contract items.
    pub items: &'hir [ItemId],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemId {
    Function(FunctionId),
    Var(VarId),
    Struct(StructId),
    Enum(EnumId),
    Udvt(UdvtId),
    Error(ErrorId),
    Event(EventId),
}

/// A function.
#[derive(Debug)]
pub struct Function<'hir> {
    /// The function name.
    pub name: Ident,
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
    /// The declaration name.
    pub name: Ident,
    /// The declaration span.
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
