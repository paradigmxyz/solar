use rayon::prelude::*;
use std::{fmt, marker::PhantomData, sync::Arc};
use sulk_ast::ast;
use sulk_data_structures::{
    index::{Idx, IndexVec},
    newtype_index,
};
use sulk_interface::{diagnostics::ErrorGuaranteed, source_map::SourceFile, Ident, Span};

pub use ast::{
    BinOp, BinOpKind, ContractKind, DataLocation, ElementaryType, FunctionKind, StateMutability,
    UnOp, UnOpKind, VarMut, Visibility,
};

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

                #[doc = "Returns a parallel iterator over all of the " $singular " IDs."]
                #[inline]
                pub fn [<par_ $singular _ids>](&self) -> impl IndexedParallelIterator<Item = $id> {
                    (0..self.$plural.len()).into_par_iter().map($id::from_usize)
                }

                #[doc = "Returns an iterator over all of the " $singular " values."]
                #[inline]
                pub fn $plural(&self) -> impl ExactSizeIterator<Item = &$type> + DoubleEndedIterator {
                    self.$plural.raw.iter()
                }

                #[doc = "Returns a parallel iterator over all of the " $singular " values."]
                #[inline]
                pub fn [<par_ $plural>](&self) -> impl IndexedParallelIterator<Item = &$type> {
                    self.$plural.raw.par_iter()
                }

                #[doc = "Returns an iterator over all of the " $singular " IDs and their associated values."]
                #[inline]
                pub fn [<$plural _enumerated>](&self) -> impl ExactSizeIterator<Item = ($id, &$type)> + DoubleEndedIterator {
                    self.$plural().enumerate().map(|(i, v)| ($id::from_usize(i), v))
                }

                #[doc = "Returns an iterator over all of the " $singular " IDs and their associated values."]
                #[inline]
                pub fn [<par_ $plural _enumerated>](&self) -> impl IndexedParallelIterator<Item = ($id, &$type)> {
                    self.[<par_ $plural>]().enumerate().map(|(i, v)| ($id::from_usize(i), v))
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
    variable => variables, VariableId => Variable<'hir>;
}

impl<'hir> Hir<'hir> {
    /// Returns the item associated with the given ID.
    pub fn item(&self, id: ItemId) -> Item<'_, 'hir> {
        match id {
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
    Variable(&'a Variable<'hir>),
}

impl Item<'_, '_> {
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

    /// Returns `true` if the item is visible in derived contracts.
    #[inline]
    pub fn is_visible_in_derived_contracts(self) -> bool {
        // matches!(self, Item::Struct(_) | Item::Enum(_) | Item::Event(_) | Item::Error(_))
        // || (
        self.is_visible_in_contract() && self.visibility() >= Visibility::Internal
        // )
    }

    /// Returns `true` if the item is visible in the contract.
    #[inline]
    pub fn is_visible_in_contract(self) -> bool {
        (if let Item::Function(f) = self { f.kind == FunctionKind::Function } else { true })
            && self.visibility() != Visibility::External
    }

    /// Returns `true` if the item is public or external.
    #[inline]
    pub fn is_public(&self) -> bool {
        self.visibility() >= Visibility::Public
    }

    /// Returns the visibility of the item.
    #[inline]
    pub fn visibility(self) -> Visibility {
        self.visibility_opt().unwrap_or_else(|| self.default_visibility())
    }

    #[inline]
    fn visibility_opt(self) -> Option<Visibility> {
        match self {
            Item::Function(f) => f.visibility,
            Item::Variable(v) => v.visibility,
            Item::Contract(_)
            | Item::Struct(_)
            | Item::Enum(_)
            | Item::Udvt(_)
            | Item::Error(_)
            | Item::Event(_) => None,
        }
    }

    #[inline]
    fn default_visibility(self) -> Visibility {
        match self {
            Item::Function(f) => match f.kind {
                _ if f.is_free() => Visibility::Internal,
                FunctionKind::Modifier => Visibility::Internal,
                _ => Visibility::Public,
            },
            Item::Variable(_) => Visibility::Internal,
            Item::Contract(_)
            | Item::Struct(_)
            | Item::Enum(_)
            | Item::Udvt(_)
            | Item::Error(_)
            | Item::Event(_) => Visibility::Public,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
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
    /// The function span.
    pub span: Span,
    /// The contract this function is defined in, if any.
    pub contract: Option<ContractId>,
    /// The function name.
    /// Only `None` if this is a constructor, fallback, or receive function.
    pub name: Option<Ident>,
    /// The function kind.
    pub kind: FunctionKind,
    /// The visibility of the function.
    pub visibility: Option<Visibility>,
    pub state_mutability: Option<StateMutability>,
    pub modifiers: &'hir [FunctionId],
    pub virtual_: bool,
    pub overrides: &'hir [ContractId],
    /// The function parameters.
    pub params: &'hir [VariableId],
    /// The function returns.
    pub returns: &'hir [VariableId],
}

impl Function<'_> {
    /// Returns `true` if this is a free function, meaning it is not part of a contract.
    pub fn is_free(&self) -> bool {
        self.contract.is_some()
    }
}

/// A struct.
#[derive(Debug)]
pub struct Struct<'hir> {
    /// The contract this struct is defined in, if any.
    pub contract: Option<ContractId>,
    /// The struct span.
    pub span: Span,
    /// The struct name.
    pub name: Ident,
    pub fields: &'hir [StructField],
}

#[derive(Debug)]
pub struct StructField {
    pub name: Ident,
    // pub ty: Type, // TODO
}

/// An enum.
#[derive(Debug)]
pub struct Enum<'hir> {
    /// The enum span.
    pub span: Span,
    /// The contract this enum is defined in, if any.
    pub contract: Option<ContractId>,
    /// The enum name.
    pub name: Ident,
    /// The enum variants.
    pub variants: &'hir [Ident],
}

/// A user-defined value type.
#[derive(Debug)]
pub struct Udvt<'hir> {
    /// The UDVT span.
    pub span: Span,
    /// The contract this UDVT is defined in, if any.
    pub contract: Option<ContractId>,
    /// The UDVT name.
    pub name: Ident,
    pub _tmp: PhantomData<&'hir ()>,
}

/// An event.
#[derive(Debug)]
pub struct Event<'hir> {
    /// The event span.
    pub span: Span,
    /// The contract this event is defined in, if any.
    pub contract: Option<ContractId>,
    /// The event name.
    pub name: Ident,
    /// Whether this event is anonymous.
    pub anonymous: bool,
    pub _tmp: PhantomData<&'hir ()>,
}

/// A custom error.
#[derive(Debug)]
pub struct Error<'hir> {
    /// The error span.
    pub span: Span,
    /// The contract this error is defined in, if any.
    pub contract: Option<ContractId>,
    /// The error name.
    pub name: Ident,
    pub _tmp: PhantomData<&'hir ()>,
}

/// A constant or variable declaration.
#[derive(Debug)]
pub struct Variable<'hir> {
    /// The variable span.
    pub span: Span,
    /// The contract this variable is defined in, if any.
    pub contract: Option<ContractId>,
    /// The variable name.
    pub name: Option<Ident>,
    /// The visibility of the variable.
    pub visibility: Option<Visibility>,
    pub mutability: Option<VarMut>,
    pub data_location: Option<DataLocation>,
    pub indexed: bool,
    pub initializer: Option<&'hir Expr<'hir>>,
}

impl Variable<'_> {
    /// Returns `true` if the variable is a state variable.
    pub fn is_state_variable(&self) -> bool {
        self.contract.is_some()
    }

    /// Returns `true` if the variable is public.
    pub fn is_public(&self) -> bool {
        self.visibility >= Some(Visibility::Public)
    }
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
    pub kind: ExprKind<'hir>,
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

    /// A function call expression: `foo(42)` or `foo({ bar: 42 })`.
    Call(&'hir Expr<'hir>, &'hir [Expr<'hir>]),

    /// Function call options: `foo.bar{ value: 1, gas: 2 }`.
    CallOptions(&'hir Expr<'hir>, &'hir [NamedArg<'hir>]),

    /// A unary `delete` expression: `delete vector`.
    Delete(&'hir Expr<'hir>),

    /// An identifier: `foo`. A reference to an item or variable.
    Ident(ItemId),

    /// A square bracketed indexing expression: `vector[index]`, `slice[l:r]`.
    Index(&'hir Expr<'hir>, Option<&'hir Expr<'hir>>),

    /// A square bracketed slice expression: `slice[l:r]`.
    Slice(&'hir Expr<'hir>, Option<&'hir Expr<'hir>>, Option<&'hir Expr<'hir>>),

    // /// A literal: `hex"1234"`, `5.6 ether`.
    // Lit(Lit, Option<SubDenomination>),
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
}

/// A named argument: `name: value`.
#[derive(Debug)]
pub struct NamedArg<'hir> {
    pub name: Ident,
    pub value: Expr<'hir>,
}

/// A type name.
#[derive(Debug)]
pub struct Type<'hir> {
    pub span: Span,
    pub kind: TypeKind<'hir>,
}

/// The kind of a type.
#[derive(Debug)]
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

/// An array type.
#[derive(Debug)]
pub struct TypeArray<'hir> {
    pub element: Type<'hir>,
    pub size: Option<&'hir Expr<'hir>>,
}

/// A function type name.
#[derive(Debug)]
pub struct TypeFunction<'hir> {
    pub parameters: &'hir [Type<'hir>],
    pub visibility: Option<Visibility>,
    pub state_mutability: Option<StateMutability>,
    pub returns: &'hir [Type<'hir>],
}

/// A mapping type.
#[derive(Debug)]
pub struct TypeMapping<'hir> {
    pub key: Type<'hir>,
    pub key_name: Option<Ident>,
    pub value: Type<'hir>,
    pub value_name: Option<Ident>,
}
