use super::{Gcx, Ty, TyFnKind, TyKind};
use crate::hir;
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
    /// Print types in the library function signature form used by solc.
    ///
    /// Unlike contract functions, library functions may take `mapping`/`storage`
    /// reference parameters and refer to structs, enums, and contracts by name
    /// (e.g. `f(DataTypes.Reserve storage)`).
    in_library: bool,
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
        Self { gcx, buf, mode, in_library: false }
    }

    /// Sets whether types are printed as a `library` function signature.
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
            TyKind::Contract(id) if self.mode == TyAbiPrinterMode::Signature && self.in_library => {
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
            TyKind::Enum(id) => match self.mode {
                TyAbiPrinterMode::Signature if self.in_library => {
                    write!(self.buf, "{}", self.gcx.item_canonical_name(id))
                }
                _ => self.buf.write_str("uint8"),
            },
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
                self.buf.write_str("mapping(")?;
                self.print(key)?;
                self.buf.write_str(" => ")?;
                self.print(value)?;
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
            | TyKind::IntLiteral(..)
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
}

impl<'gcx, W: fmt::Write> TySolcPrinter<'gcx, W> {
    pub(crate) fn new(gcx: Gcx<'gcx>, buf: W) -> Self {
        Self { gcx, buf, data_locations: false }
    }

    /// Whether to print data locations for reference types.
    ///
    /// Default: `false`.
    pub(crate) fn data_locations(mut self, yes: bool) -> Self {
        self.data_locations = yes;
        self
    }

    pub(crate) fn print(&mut self, ty: Ty<'gcx>) -> fmt::Result {
        match ty.kind {
            TyKind::Elementary(ty) => {
                ty.write_abi_str(&mut self.buf)?;
                if matches!(ty, ElementaryType::Address(true)) {
                    self.buf.write_str(" payable")?;
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
                self.print_tuple(f.parameters)?;

                if f.state_mutability != hir::StateMutability::NonPayable {
                    write!(self.buf, " {}", f.state_mutability)?;
                }
                if f.kind == TyFnKind::External {
                    self.buf.write_str(" external")?;
                }

                if !f.returns.is_empty() {
                    self.buf.write_str(" returns ")?;
                    self.print_tuple(f.returns)?;
                }
                Ok(())
            }
            TyKind::Struct(id) => {
                write!(self.buf, "struct {}", self.gcx.item_canonical_name(id))
            }
            TyKind::Enum(id) => write!(self.buf, "enum {}", self.gcx.item_canonical_name(id)),
            TyKind::Udvt(_, id) => write!(self.buf, "{}", self.gcx.item_canonical_name(id)),
            TyKind::Ref(ty, loc) => {
                self.print(ty)?;
                if self.data_locations {
                    write!(self.buf, " {loc}")?;
                }
                Ok(())
            }
            TyKind::DynArray(ty) => {
                self.print(ty)?;
                self.buf.write_str("[]")
            }
            TyKind::Array(ty, len) => {
                self.print(ty)?;
                write!(self.buf, "[{len}]")
            }

            // Internal types.
            TyKind::StringLiteral(utf8, size) => {
                let kind = if utf8 { "utf8" } else { "bytes" };
                write!(self.buf, "{kind}_string_literal[{}]", size.bytes())
            }
            TyKind::IntLiteral(_, size, _) => {
                write!(self.buf, "int_literal[{}]", size.bits())
            }
            TyKind::Slice(ty) => {
                self.print(ty)?;
                self.buf.write_str(" slice")
            }
            TyKind::Tuple(tys) => {
                self.buf.write_str("tuple")?;
                self.print_tuple(tys)
            }
            TyKind::Mapping(key, value) => {
                self.buf.write_str("mapping(")?;
                self.print(key)?;
                self.buf.write_str(" => ")?;
                self.print(value)?;
                self.buf.write_str(")")
            }
            TyKind::Module(id) => {
                let s = self.gcx.hir.source(id);
                write!(self.buf, "module {}", s.file.name.display())
            }
            TyKind::BuiltinModule(b) => self.buf.write_str(b.name().as_str()),
            TyKind::Variadic => self.buf.write_str("..."),
            TyKind::Type(ty) | TyKind::Meta(ty) => {
                self.buf.write_str("type(")?;
                self.print(ty)?; // TODO: `richIdentifier`
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

    fn print_tuple(&mut self, tys: &[Ty<'gcx>]) -> fmt::Result {
        self.buf.write_str("(")?;
        for (i, &ty) in tys.iter().enumerate() {
            if i > 0 {
                self.buf.write_str(",")?;
            }
            self.print(ty)?;
        }
        self.buf.write_str(")")
    }
}
