use super::{Gcx, Ty, TyFn, TyFnKind, TyKind};
use crate::{builtins::Builtin, eval::ConstValue, hir};
use alloy_primitives::{hex, keccak256};
use num_bigint::BigInt;
use num_traits::Signed;
use solar_ast::{DataLocation, ElementaryType};
use std::fmt;

impl<'gcx> Gcx<'gcx> {
    /// Formats the ABI signature of a function in the form `{name}({tys},*)`.
    pub(super) fn mk_abi_signature(
        self,
        name: &str,
        tys: impl IntoIterator<Item = Ty<'gcx>>,
        in_library: bool,
    ) -> String {
        let mut s = String::with_capacity(64);
        s.push_str(name);
        TyAbiPrinter::new(self, &mut s, TyAbiPrinterMode::Signature)
            .with_in_library(in_library)
            .print_tuple(tys)
            .unwrap();
        s
    }
}

/// Prints types as specified by the Solidity ABI.
///
/// Reference: <https://docs.soliditylang.org/en/latest/abi-spec.html>
pub struct TyAbiPrinter<'gcx, W> {
    gcx: Gcx<'gcx>,
    buf: W,
    mode: TyAbiPrinterMode,
    /// Print types in the library function form used by solc.
    ///
    /// Enums and contracts use their canonical names. Signatures also name structs
    /// and may include `mapping`/`storage` references (e.g. `f(DataTypes.Reserve storage)`).
    /// JSON ABI structs remain tuples, and storage-reference functions are omitted.
    in_library: bool,
    /// Whether the printer is inside the key or value type of a `mapping`.
    ///
    /// solc prints these with `canonicalName`, which keeps user-defined value type names.
    in_mapping: bool,
}

/// [`TyAbiPrinter`] configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TyAbiPrinterMode {
    /// Printing types for a function signature.
    ///
    /// Prints the fields of the struct in a tuple, recursively.
    ///
    /// Note that this will make the printer panic if it encounters a recursive struct.
    Signature,
    /// Printing types for a JSON ABI `type` field.
    ///
    /// Print the word `tuple` when encountering structs.
    Abi,
}

impl<'gcx, W: fmt::Write> TyAbiPrinter<'gcx, W> {
    /// Creates a new ABI printer.
    pub fn new(gcx: Gcx<'gcx>, buf: W, mode: TyAbiPrinterMode) -> Self {
        Self { gcx, buf, mode, in_library: false, in_mapping: false }
    }

    /// Sets whether types are printed for a `library` function.
    pub fn with_in_library(mut self, yes: bool) -> Self {
        self.in_library = yes;
        self
    }

    /// Returns a mutable reference to the underlying buffer.
    pub fn buf(&mut self) -> &mut W {
        &mut self.buf
    }

    /// Consumes the printer and returns the underlying buffer.
    pub fn into_buf(self) -> W {
        self.buf
    }

    /// Prints the ABI representation of `ty`.
    pub fn print(&mut self, ty: Ty<'gcx>) -> fmt::Result {
        match ty.kind {
            TyKind::Elementary(ty) => ty.write_abi_str(&mut self.buf),
            TyKind::Contract(id) if self.in_library => {
                write!(self.buf, "{}", self.gcx.item_canonical_name(id))
            }
            TyKind::Contract(_) => self.buf.write_str("address"),
            TyKind::Fn(_) => self.buf.write_str("function"),
            TyKind::Struct(id) => match self.mode {
                TyAbiPrinterMode::Signature if self.in_library => {
                    write!(self.buf, "{}", self.gcx.item_canonical_name(id))
                }
                TyAbiPrinterMode::Signature => {
                    if self.gcx.struct_recursiveness(id).is_recursive() {
                        assert!(
                            self.gcx.dcx().has_errors().is_err(),
                            "trying to print recursive struct and no error has been emitted"
                        );
                        write!(self.buf, "<recursive struct {}>", self.gcx.item_canonical_name(id))
                    } else {
                        self.print_tuple(self.gcx.struct_field_types(id).iter().copied())
                    }
                }
                TyAbiPrinterMode::Abi => self.buf.write_str("tuple"),
            },
            TyKind::Enum(id) if self.in_library => {
                write!(self.buf, "{}", self.gcx.item_canonical_name(id))
            }
            TyKind::Enum(_) => self.buf.write_str("uint8"),
            TyKind::Udvt(_, id) if self.in_mapping => {
                write!(self.buf, "{}", self.gcx.item_canonical_name(id))
            }
            TyKind::Udvt(ty, _) => self.print(ty),
            TyKind::Ref(ty, loc) => {
                self.print(ty)?;
                // solc encodes the `storage` location in `library` function
                // signatures (e.g. `f(uint256[] storage)`), but never `memory`
                // or `calldata`.
                if self.in_library && loc == DataLocation::Storage {
                    self.buf.write_str(" storage")?;
                }
                Ok(())
            }
            // A `mapping` can appear in a `library` function signature. solc prints
            // it structurally as `mapping(key => value)`.
            TyKind::Mapping(key, value) if self.mode == TyAbiPrinterMode::Signature => {
                let in_mapping = std::mem::replace(&mut self.in_mapping, true);
                self.buf.write_str("mapping(")?;
                self.print(key)?;
                self.buf.write_str(" => ")?;
                self.print(value)?;
                self.in_mapping = in_mapping;
                self.buf.write_str(")")
            }
            TyKind::DynArray(ty) => {
                self.print(ty)?;
                self.buf.write_str("[]")
            }
            TyKind::Array(ty, len) => {
                self.print(ty)?;
                write!(self.buf, "[{len}]")
            }

            TyKind::Slice(..)
            | TyKind::StringLiteral(..)
            | TyKind::CallOptions(_)
            | TyKind::IntLiteral(..)
            | TyKind::RationalLiteral
            | TyKind::Tuple(_)
            | TyKind::Mapping(..)
            | TyKind::Super(_)
            // ^ `Mapping` only reaches here in `Abi` mode; `Signature` mode handles it above.
            | TyKind::Error(..)
            | TyKind::Event(..)
            | TyKind::Module(_)
            | TyKind::BuiltinModule(_)
            | TyKind::Variadic
            | TyKind::Type(_)
            | TyKind::Meta(_)
            | TyKind::Err(_) => {
                assert!(
                    self.gcx.dcx().has_errors().is_err(),
                    "printing unsupported type as ABI: {ty:?}"
                );
                self.buf.write_str("<error>")
            }
        }
    }

    /// Prints `tys` in a comma-delimited parenthesized tuple.
    pub fn print_tuple(&mut self, tys: impl IntoIterator<Item = Ty<'gcx>>) -> fmt::Result {
        self.buf.write_str("(")?;
        for (i, ty) in tys.into_iter().enumerate() {
            if i > 0 {
                self.buf.write_str(",")?;
            }
            self.print(ty)?;
        }
        self.buf.write_str(")")
    }
}

/// Prints types as implemented in `Type::toString(bool)` in solc.
///
/// This is mainly used in the `internalType` field of the ABI.
///
/// Example: <https://github.com/argotorg/solidity/blob/9d7cc42bc1c12bb43e9dccf8c6c36833fdfcbbca/libsolidity/ast/Types.cpp#L2352-L2358>
pub(crate) struct TySolcPrinter<'gcx, W> {
    gcx: Gcx<'gcx>,
    buf: W,
    data_locations: bool,
    /// The location of the reference type being printed, if printing the locations of the AST
    /// JSON. See [`Self::locations`].
    location: Option<Location>,
    /// The value of the number or string literal that the type belongs to.
    literal: Option<&'gcx ConstValue>,
    /// What solc knows of the components of a tuple expression.
    components: Vec<Component<'gcx>>,
    /// Whether the call options `gas`, `value` and `salt` are set on the function type.
    call_options: [bool; 3],
    /// How deep the printed type is nested in other types.
    depth: usize,
    /// The kind of the builtin function that the function type belongs to.
    function_kind: Option<&'static str>,
}

/// A component of a tuple expression.
#[derive(Clone)]
pub(crate) struct Component<'gcx> {
    /// The value of the component, if it is a literal.
    pub(crate) literal: Option<&'gcx ConstValue>,
    /// Whether solc types the component as a pointer.
    pub(crate) pointer: bool,
    /// Whether the component is left out: solc has no type for it.
    pub(crate) empty: bool,
    /// The components of the component, if it is a tuple.
    pub(crate) components: Vec<Self>,
}

/// The data location of a reference type, and whether solc treats the type as a pointer.
#[derive(Clone, Copy)]
struct Location {
    loc: DataLocation,
    pointer: bool,
}

impl<'gcx, W: fmt::Write> TySolcPrinter<'gcx, W> {
    pub(crate) fn new(gcx: Gcx<'gcx>, buf: W) -> Self {
        Self {
            gcx,
            buf,
            data_locations: false,
            location: None,
            literal: None,
            components: Vec::new(),
            call_options: [false; 3],
            depth: 0,
            function_kind: None,
        }
    }

    /// Prints the data location of every reference type like `Type::toString(false)` in solc,
    /// which is the `typeString` of an expression: `storage ref` and `storage pointer` for storage
    /// types, where `pointer` tells which one the type is.
    ///
    /// Default: `false`.
    pub(crate) fn locations(mut self, pointer: bool) -> Self {
        self.location = Some(Location { loc: DataLocation::Storage, pointer });
        self
    }

    /// Sets the value of the literal that the printed type belongs to.
    pub(crate) fn literal(mut self, value: Option<&'gcx ConstValue>) -> Self {
        self.literal = value;
        self
    }

    /// Sets what solc knows of the components of the printed tuple.
    pub(crate) fn components(mut self, components: Vec<Component<'gcx>>) -> Self {
        self.components = components;
        self
    }

    /// Sets the call options `gas`, `value` and `salt` that are set on the printed function.
    pub(crate) fn call_options(mut self, options: [bool; 3]) -> Self {
        self.call_options = options;
        self
    }

    /// Sets the kind of the builtin function that the printed function type belongs to.
    pub(crate) fn function_kind(mut self, kind: Option<&'static str>) -> Self {
        self.function_kind = kind;
        self
    }

    /// Whether to print data locations for reference types.
    ///
    /// Default: `false`.
    pub(crate) fn data_locations(mut self, yes: bool) -> Self {
        self.data_locations = yes;
        self
    }

    pub(crate) fn print(&mut self, ty: Ty<'gcx>) -> fmt::Result {
        self.depth += 1;
        let result = self.print_inner(ty);
        self.depth -= 1;
        result
    }

    fn print_inner(&mut self, ty: Ty<'gcx>) -> fmt::Result {
        match ty.kind {
            TyKind::Elementary(ty) => {
                ty.write_abi_str(&mut self.buf)?;
                if matches!(ty, ElementaryType::Address(true)) {
                    self.buf.write_str(" payable")?;
                }
                if matches!(ty, ElementaryType::String | ElementaryType::Bytes) {
                    self.print_location()?;
                }
                Ok(())
            }
            TyKind::Contract(id) => {
                let c = self.gcx.hir.contract(id);
                self.buf.write_str(if c.kind.is_library() { "library" } else { "contract" })?;
                write!(self.buf, " {}", c.name)
            }
            TyKind::Super(id) => {
                let c = self.gcx.hir.contract(id);
                write!(self.buf, "contract super {}", c.name)
            }
            TyKind::Fn(f) => {
                self.buf.write_str("function ")?;
                if f.is_declaration()
                    && let Some(id) = f.function_id
                {
                    let name = self.gcx.item_canonical_name(hir::ItemId::from(id));
                    write!(self.buf, "{name}")?;
                }
                // solc gives `abi.decode` no types, since they depend on its arguments.
                let hidden = self.function_kind == Some("abidecode");
                let (parameters, returns) =
                    if hidden { (&[][..], &[][..]) } else { (f.parameters, f.returns) };
                self.in_fn_list(f, |this| {
                    this.print_list(parameters, f.attached && f.kind == TyFnKind::Builtin)
                })?;

                let state_mutability = self.fn_state_mutability(f);
                if state_mutability != hir::StateMutability::NonPayable {
                    write!(self.buf, " {state_mutability}")?;
                }
                if f.kind == TyFnKind::External {
                    self.buf.write_str(" external")?;
                }

                if visible(returns, self.location.is_some()).next().is_some() {
                    self.buf.write_str(" returns ")?;
                    self.in_fn_list(f, |this| this.print_tuple(returns))?;
                }
                Ok(())
            }
            // solc types events and errors as functions.
            TyKind::Event(tys, _) if self.location.is_some() => {
                self.buf.write_str("function ")?;
                self.in_list(|this| this.print_tuple(tys))
            }
            TyKind::Error(tys, _) if self.location.is_some() && self.depth <= 1 => {
                self.buf.write_str("function ")?;
                self.in_list(|this| this.print_tuple(tys))?;
                self.buf.write_str(" pure returns (error)")
            }
            TyKind::Struct(id) => {
                write!(self.buf, "struct {}", self.gcx.item_canonical_name(id))?;
                self.print_location()
            }
            TyKind::Enum(id) => write!(self.buf, "enum {}", self.gcx.item_canonical_name(id)),
            TyKind::Udvt(_, id) => write!(self.buf, "{}", self.gcx.item_canonical_name(id)),
            TyKind::Ref(ty, loc) => {
                if let Some(Location { pointer, .. }) = self.location {
                    return self.in_location(loc, pointer, |this| this.print(ty));
                }
                self.print(ty)?;
                if self.data_locations {
                    write!(self.buf, " {loc}")?;
                }
                Ok(())
            }
            TyKind::DynArray(ty) => {
                self.in_elements(|this| this.print(ty))?;
                self.buf.write_str("[]")?;
                self.print_location()
            }
            TyKind::Array(ty, len) => {
                self.in_elements(|this| this.print(ty))?;
                write!(self.buf, "[{len}]")?;
                self.print_location()
            }

            // Internal types.
            TyKind::CallOptions(ty) if self.location.is_some() => self.print(ty),
            TyKind::CallOptions(ty) => {
                self.print(ty)?;
                self.buf.write_str(" with call options")
            }
            TyKind::StringLiteral(_, _) if let Some(ConstValue::Bytes(value, _)) = self.literal => {
                let value = value.as_byte_str();
                if value.iter().all(|&byte| (0x20..0x7f).contains(&byte)) {
                    write!(self.buf, "literal_string \"{}\"", String::from_utf8_lossy(value))
                } else {
                    write!(self.buf, "literal_string hex\"{}\"", hex::encode(value))
                }
            }
            TyKind::StringLiteral(utf8, len) => {
                let kind = if utf8 { "utf8" } else { "bytes" };
                write!(self.buf, "{kind}_string_literal[{len}]")
            }
            TyKind::IntLiteral(..) | TyKind::RationalLiteral
                if let Some(value) = self.literal.and_then(literal_ratio) =>
            {
                match value {
                    (num, None) => write!(self.buf, "int_const {}", readable(num)),
                    (num, Some(den)) => {
                        write!(self.buf, "rational_const {} / {}", readable(num), readable(den))
                    }
                }
            }
            TyKind::IntLiteral(_, bits, _) => write!(self.buf, "int_literal[{bits}]"),
            TyKind::RationalLiteral => self.buf.write_str("rational_literal"),
            TyKind::Slice(ty) => {
                self.print(ty)?;
                self.buf.write_str(" slice")
            }
            TyKind::Tuple(tys) => {
                self.buf.write_str("tuple")?;
                self.in_list(|this| this.print_tuple(tys))
            }
            TyKind::Error(..) if self.location.is_some() && self.depth > 1 => {
                self.buf.write_str("error")
            }
            TyKind::Mapping(key, value) => {
                self.buf.write_str("mapping(")?;
                self.in_location(DataLocation::Memory, true, |this| this.print(key))?;
                self.buf.write_str(" => ")?;
                self.in_location(DataLocation::Storage, false, |this| this.print(value))?;
                self.buf.write_str(")")
            }
            TyKind::Module(id) => {
                let s = self.gcx.hir.source(id);
                write!(self.buf, "module \"{}\"", s.file.name.display())
            }
            TyKind::BuiltinModule(b) => self.buf.write_str(b.name().as_str()),
            TyKind::Variadic => self.buf.write_str("..."),
            TyKind::Type(ty) | TyKind::Meta(ty) => {
                self.buf.write_str("type(")?;
                self.in_location(type_location(ty), true, |this| this.print(ty))?; // TODO: `richIdentifier`
                self.buf.write_str(")")
            }
            TyKind::Error(tys, id) => {
                self.buf.write_str("error ")?;
                write!(self.buf, "{}", self.gcx.item_canonical_name(id))?;
                self.print_tuple(tys)
            }
            TyKind::Event(tys, id) => {
                self.buf.write_str("event ")?;
                write!(self.buf, "{}", self.gcx.item_canonical_name(id))?;
                self.print_tuple(tys)
            }

            TyKind::Err(_) => self.buf.write_str("<error>"),
        }
    }

    /// Prints the data location of the reference type, if printing locations.
    fn print_location(&mut self) -> fmt::Result {
        let Some(Location { loc, pointer }) = self.location else { return Ok(()) };
        write!(self.buf, " {loc}")?;
        match (loc, pointer) {
            (DataLocation::Storage, true) => self.buf.write_str(" pointer"),
            (DataLocation::Storage, false) => self.buf.write_str(" ref"),
            _ => Ok(()),
        }
    }

    /// Prints with the location of the reference types, if printing locations.
    fn in_location(
        &mut self,
        loc: DataLocation,
        pointer: bool,
        f: impl FnOnce(&mut Self) -> fmt::Result,
    ) -> fmt::Result {
        let Some(prev) = self.location else { return f(self) };
        self.location = Some(Location { loc, pointer });
        let result = f(self);
        self.location = Some(prev);
        result
    }

    /// Prints the elements of a reference type, which are pointers unless they live in storage.
    fn in_elements(&mut self, f: impl FnOnce(&mut Self) -> fmt::Result) -> fmt::Result {
        match self.location {
            Some(Location { loc, .. }) => self.in_location(loc, loc != DataLocation::Storage, f),
            None => f(self),
        }
    }

    /// Prints the types of a list, which solc treats as storage pointers.
    fn in_list(&mut self, f: impl FnOnce(&mut Self) -> fmt::Result) -> fmt::Result {
        self.in_location(DataLocation::Storage, true, f)
    }

    fn print_tuple(&mut self, tys: &[Ty<'gcx>]) -> fmt::Result {
        self.print_list(tys, false)
    }

    /// Returns the state mutability of a function type. A function declaration is as payable as
    /// its function, which its type as an internal function loses.
    fn fn_state_mutability(&self, f: &TyFn<'gcx>) -> hir::StateMutability {
        match f.function_id {
            Some(id) if f.is_declaration() => self.gcx.hir.function(id).state_mutability,
            _ => f.state_mutability,
        }
    }

    /// Prints a comma-delimited parenthesized list. The first type is a pointer if it is the
    /// receiver of an attached function.
    fn print_list(&mut self, tys: &[Ty<'gcx>], receiver: bool) -> fmt::Result {
        self.buf.write_str("(")?;
        for (i, &ty) in visible(tys, self.location.is_some()).enumerate() {
            if i > 0 {
                self.buf.write_str(",")?;
            }
            if i == 0 && receiver {
                self.in_location(DataLocation::Memory, true, |this| this.print(ty))?;
            } else {
                self.print_component(i, ty)?;
            }
        }
        self.buf.write_str(")")
    }

    /// Prints the type of a component of a tuple.
    fn print_component(&mut self, index: usize, ty: Ty<'gcx>) -> fmt::Result {
        let Some(component) = self.components.get(index).cloned() else {
            return self.print(ty);
        };
        if component.empty {
            return Ok(());
        }
        let literal = std::mem::replace(&mut self.literal, component.literal);
        let components = std::mem::replace(&mut self.components, component.components);
        let result =
            self.in_location(DataLocation::Storage, component.pointer, |this| this.print(ty));
        self.components = components;
        self.literal = literal;
        result
    }

    /// Prints the types of the parameters or returns of a function. Those of builtin functions
    /// default to memory and are references unless they are the receiver of an attached function.
    fn in_fn_list(
        &mut self,
        f: &TyFn<'gcx>,
        print: impl FnOnce(&mut Self) -> fmt::Result,
    ) -> fmt::Result {
        match f.kind {
            // The element of `push` is in the storage of the array.
            TyFnKind::Builtin if f.attached => {
                self.in_location(DataLocation::Storage, false, print)
            }
            TyFnKind::Builtin => self.in_location(DataLocation::Memory, false, print),
            _ => self.in_list(print),
        }
    }

    /// Prints the type as implemented in `Type::identifier()` in solc, the `typeIdentifier` of
    /// the AST JSON.
    ///
    /// solc marks the reference types of every declaration but a storage state variable as
    /// pointers: `pointer` is that mark. A reference type outside of a [`TyKind::Ref`] lives in
    /// storage.
    ///
    /// The types that have no solc counterpart, such as error types, print `t_unsupported`.
    pub(crate) fn print_identifier(&mut self, ty: Ty<'gcx>, pointer: bool) -> fmt::Result {
        self.identifier(ty, DataLocation::Storage, pointer)
    }

    fn identifier(&mut self, ty: Ty<'gcx>, loc: DataLocation, pointer: bool) -> fmt::Result {
        self.depth += 1;
        let result = self.identifier_inner(ty, loc, pointer);
        self.depth -= 1;
        result
    }

    fn identifier_inner(&mut self, ty: Ty<'gcx>, loc: DataLocation, pointer: bool) -> fmt::Result {
        // The elements of a reference type are pointers unless the type lives in storage.
        let element_pointer = loc != DataLocation::Storage;
        match ty.kind {
            TyKind::Elementary(ty) => match ty {
                ElementaryType::Address(payable) => {
                    self.buf.write_str(if payable { "t_address_payable" } else { "t_address" })
                }
                ElementaryType::Bool => self.buf.write_str("t_bool"),
                ElementaryType::String => {
                    self.buf.write_str("t_string")?;
                    self.identifier_location(loc, pointer)
                }
                ElementaryType::Bytes => {
                    self.buf.write_str("t_bytes")?;
                    self.identifier_location(loc, pointer)
                }
                ElementaryType::Int(size) => write!(self.buf, "t_int{}", size.bits()),
                ElementaryType::UInt(size) => write!(self.buf, "t_uint{}", size.bits()),
                ElementaryType::FixedBytes(size) => write!(self.buf, "t_bytes{}", size.bytes()),
            },
            TyKind::Ref(ty, loc) => self.identifier(ty, loc, pointer),
            TyKind::DynArray(element) => {
                self.buf.write_str("t_array$_")?;
                self.identifier(element, loc, element_pointer)?;
                self.buf.write_str("_$dyn")?;
                self.identifier_location(loc, pointer)
            }
            TyKind::Array(element, len) => {
                self.buf.write_str("t_array$_")?;
                self.identifier(element, loc, element_pointer)?;
                write!(self.buf, "_${len}")?;
                self.identifier_location(loc, pointer)
            }
            TyKind::Contract(id) => self.identifier_item("t_contract", id),
            TyKind::Super(id) => self.identifier_item("t_super", id),
            TyKind::Struct(id) => {
                self.identifier_item("t_struct", id)?;
                self.identifier_location(loc, pointer)
            }
            TyKind::Enum(id) => self.identifier_item("t_enum", id),
            TyKind::Udvt(_, id) => self.identifier_item("t_userDefinedValueType", id),
            TyKind::Mapping(key, value) => {
                self.buf.write_str("t_mapping$_")?;
                self.identifier(key, DataLocation::Memory, true)?;
                self.buf.write_str("_$_")?;
                self.identifier(value, DataLocation::Storage, false)?;
                self.buf.write_str("_$")
            }
            TyKind::Fn(f) => {
                let kind = match f.kind {
                    TyFnKind::External => "external",
                    TyFnKind::Declaration => "declaration",
                    TyFnKind::DelegateCall => "delegatecall",
                    TyFnKind::BareCall => "barecall",
                    TyFnKind::BareDelegateCall => "baredelegatecall",
                    TyFnKind::BareStaticCall => "barestaticcall",
                    TyFnKind::Creation => "creation",
                    TyFnKind::Internal | TyFnKind::InternalWithSelector => "internal",
                    TyFnKind::Builtin => self.function_kind.unwrap_or("internal"),
                };
                write!(self.buf, "t_function_{kind}_{}", self.fn_state_mutability(f))?;
                let hidden = self.function_kind == Some("abidecode");
                let (parameters, returns) =
                    if hidden { (&[][..], &[][..]) } else { (f.parameters, f.returns) };
                let builtin = f.kind == TyFnKind::Builtin;
                let (loc, pointer) = match f.kind {
                    TyFnKind::Builtin if f.attached => (DataLocation::Storage, false),
                    TyFnKind::Builtin => (DataLocation::Memory, false),
                    _ => (DataLocation::Storage, true),
                };
                let receiver = f.attached && builtin;
                self.identifier_list(parameters, loc, pointer, receiver)?;
                self.buf.write_str("returns")?;
                self.identifier_list(returns, loc, pointer, false)?;
                if f.attached
                    && let Some(receiver) = f.parameters.first().filter(|_| !hidden)
                {
                    self.buf.write_str("attached_to")?;
                    self.identifier_list(std::slice::from_ref(receiver), loc, true, false)?;
                }
                Ok(())
            }
            TyKind::Event(tys, _) => {
                self.buf.write_str("t_function_event_nonpayable")?;
                self.identifier_list(tys, DataLocation::Storage, true, false)?;
                self.buf.write_str("returns$__$")
            }
            TyKind::Error(..) if self.depth > 1 => self.buf.write_str("t_error"),
            TyKind::Error(tys, _) => {
                self.buf.write_str("t_function_error_pure")?;
                self.identifier_list(tys, DataLocation::Storage, true, false)?;
                self.buf.write_str("returns$_t_error_$")
            }
            TyKind::CallOptions(ty) => {
                self.identifier(ty, loc, pointer)?;
                let [gas, value, salt] = self.call_options;
                let flags = [(gas, "gas"), (value, "value"), (salt, "salt")];
                flags
                    .into_iter()
                    .filter(|(set, _)| *set)
                    .try_for_each(|(_, name)| self.buf.write_str(name))
            }
            TyKind::Tuple(tys) => {
                self.buf.write_str("t_tuple")?;
                self.identifier_list(tys, DataLocation::Storage, true, false)
            }
            TyKind::Type(ty) => {
                self.buf.write_str("t_type$_")?;
                self.identifier(ty, type_location(ty), true)?;
                self.buf.write_str("_$")
            }
            TyKind::Slice(ty) => {
                self.identifier(ty, loc, pointer)?;
                self.buf.write_str("_slice")
            }
            TyKind::Meta(ty) => {
                self.buf.write_str("t_magic_meta_type_")?;
                self.identifier(ty, DataLocation::Storage, true)
            }
            TyKind::BuiltinModule(builtin) => {
                let name = match builtin {
                    Builtin::Block => "block",
                    Builtin::Msg => "message",
                    Builtin::Tx => "transaction",
                    _ => "abi",
                };
                write!(self.buf, "t_magic_{name}")
            }
            TyKind::Module(id) => write!(self.buf, "t_module_{}", self.gcx.hir.source_unit_id(id)),
            TyKind::StringLiteral(..) if let Some(ConstValue::Bytes(value, _)) = self.literal => {
                write!(self.buf, "t_stringliteral_{}", hex::encode(keccak256(value.as_byte_str())))
            }
            TyKind::IntLiteral(..) | TyKind::RationalLiteral
                if let Some((num, den)) = self.literal.and_then(literal_ratio) =>
            {
                let sign = if num.is_negative() { "minus_" } else { "" };
                write!(self.buf, "t_rational_{sign}{}_by_", num.magnitude())?;
                match den {
                    Some(den) => write!(self.buf, "{den}"),
                    None => self.buf.write_char('1'),
                }
            }
            _ => self.buf.write_str("t_unsupported"),
        }
    }

    /// Prints the location suffix of a reference type.
    fn identifier_location(&mut self, loc: DataLocation, pointer: bool) -> fmt::Result {
        write!(self.buf, "_{loc}")?;
        if pointer || loc != DataLocation::Storage { self.buf.write_str("_ptr") } else { Ok(()) }
    }

    /// Prints `prefix`, the escaped name and the AST ID of an item.
    fn identifier_item(&mut self, prefix: &str, id: impl Into<hir::ItemId> + Copy) -> fmt::Result {
        write!(self.buf, "{prefix}$_")?;
        for c in self.gcx.item_name(id).as_str().chars() {
            if c == '$' {
                self.buf.write_str("$$$")?;
            } else {
                self.buf.write_char(c)?;
            }
        }
        write!(self.buf, "_${}", self.gcx.hir.global_item_id(id))
    }

    /// Prints the identifiers of a list of types. The first is a pointer if it is the receiver of
    /// an attached function.
    fn identifier_list(
        &mut self,
        tys: &[Ty<'gcx>],
        loc: DataLocation,
        pointer: bool,
        receiver: bool,
    ) -> fmt::Result {
        self.buf.write_str("$_")?;
        for (i, &ty) in visible(tys, true).enumerate() {
            if i > 0 {
                self.buf.write_str("_$_")?;
            }
            let Some(component) = self.components.get(i).cloned() else {
                self.identifier(ty, loc, pointer || (i == 0 && receiver))?;
                continue;
            };
            if component.empty {
                continue;
            }
            let literal = std::mem::replace(&mut self.literal, component.literal);
            let components = std::mem::replace(&mut self.components, component.components);
            self.identifier(ty, loc, component.pointer || (i == 0 && receiver))?;
            self.components = components;
            self.literal = literal;
        }
        self.buf.write_str("_$")
    }
}

/// Returns the numerator and, for a fraction, the denominator of the value of a number literal.
fn literal_ratio(value: &ConstValue) -> Option<(&BigInt, Option<&BigInt>)> {
    match value {
        ConstValue::Int(int) => Some((int.as_bigint(), None)),
        ConstValue::Rational(ratio) => Some((ratio.numer(), Some(ratio.denom()))),
        _ => None,
    }
}

/// Returns a number like `RationalNumberType::bigintToReadableString` in solc: long numbers omit
/// their middle digits.
fn readable(num: &BigInt) -> String {
    let digits = num.to_string();
    if digits.len() <= 32 {
        return digits;
    }
    let (head, tail) = (&digits[..4], &digits[digits.len() - 4..]);
    format!("{head}...({} digits omitted)...{tail}", digits.len() - 8)
}

/// Returns the types of a list that solc prints, which leaves out the `...` of a variadic builtin.
fn visible<'a, 'gcx>(
    tys: &'a [Ty<'gcx>],
    solc: bool,
) -> impl Iterator<Item = &'a Ty<'gcx>> + use<'a, 'gcx> {
    tys.iter().filter(move |ty| !(solc && matches!(ty.kind, TyKind::Variadic)))
}

/// Returns the data location of the type in `type(T)`, which is memory for arrays and storage for
/// the other types.
fn type_location(ty: Ty<'_>) -> DataLocation {
    match ty.kind {
        TyKind::Array(..) | TyKind::DynArray(_) => DataLocation::Memory,
        _ => DataLocation::Storage,
    }
}
