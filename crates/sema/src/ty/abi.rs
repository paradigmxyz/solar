use super::{Gcx, Ty, TyKind};
use crate::hir;
use alloy_json_abi as json;
use solar_ast::ElementaryType;
use std::{fmt, ops::ControlFlow};

impl<'gcx> Gcx<'gcx> {
    /// Formats the ABI signature of a function in the form `{name}({tys},*)`.
    pub(super) fn mk_abi_signature(
        self,
        name: &str,
        tys: impl IntoIterator<Item = Ty<'gcx>>,
    ) -> String {
        let mut s = String::with_capacity(64);
        s.push_str(name);
        TyAbiPrinter::new(self, &mut s, TyAbiPrinterMode::Signature).print_tuple(tys).unwrap();
        s
    }

    /// Returns the ABI of the given contract.
    ///
    /// Reference: <https://docs.soliditylang.org/en/develop/abi-spec.html>
    pub fn contract_abi<'a>(self, id: hir::ContractId) -> Vec<json::AbiItem<'a>> {
        let mut items = Vec::<json::AbiItem<'a>>::new();

        let c = self.hir.contract(id);
        if let Some(ctor) = c.ctor
            && !c.is_abstract()
        {
            let json::Function { inputs, state_mutability, .. } = self.function_abi(ctor);
            items.push(json::Constructor { inputs, state_mutability }.into());
        }
        if let Some(fallback) = c.fallback {
            let json::Function { state_mutability, .. } = self.function_abi(fallback);
            items.push(json::Fallback { state_mutability }.into());
        }
        if let Some(receive) = c.receive {
            let json::Function { state_mutability, .. } = self.function_abi(receive);
            items.push(json::Receive { state_mutability }.into());
        }
        for f in self.interface_functions(id) {
            items.push(self.function_abi(f.id).into());
        }
        // TODO: Does not include referenced items.
        // See solc `interfaceEvents` and `interfaceErrors`.
        for item in self.hir.contract_item_ids(id) {
            match item {
                hir::ItemId::Event(id) => items.push(self.event_abi(id).into()),
                hir::ItemId::Error(id) => items.push(self.error_abi(id).into()),
                _ => {}
            }
        }

        // https://github.com/argotorg/solidity/blob/87d86bfba64d8b88537a4a85c1d71f521986b614/libsolidity/interface/ABI.cpp#L43-L47
        fn cmp_key<'a>(item: &'a json::AbiItem<'_>) -> impl Ord + use<'a> {
            // TODO: Use `json_type` instead of `debug_name`.
            (item.debug_name(), item.name())
        }
        items.sort_by(|a, b| cmp_key(a).cmp(&cmp_key(b)));

        items
    }

    fn function_abi(self, id: hir::FunctionId) -> json::Function {
        let f = self.hir.function(id);
        json::Function {
            name: f.name.unwrap_or_default().to_string(),
            inputs: f.parameters.iter().map(|&p| self.var_param_abi(p)).collect(),
            outputs: f.returns.iter().map(|&p| self.var_param_abi(p)).collect(),
            state_mutability: json_state_mutability(f.state_mutability),
        }
    }

    fn event_abi(self, id: hir::EventId) -> json::Event {
        let e = self.hir.event(id);
        json::Event {
            name: e.name.to_string(),
            inputs: e.parameters.iter().map(|&p| self.event_param_abi(p)).collect(),
            anonymous: e.anonymous,
        }
    }

    fn error_abi(self, id: hir::ErrorId) -> json::Error {
        let e = self.hir.error(id);
        json::Error {
            name: e.name.to_string(),
            inputs: e.parameters.iter().map(|&p| self.var_param_abi(p)).collect(),
        }
    }

    fn var_param_abi(self, id: hir::VariableId) -> json::Param {
        let v = self.hir.variable(id);
        let ty = self.type_of_item(id.into());
        self.param_abi(ty, v.name.unwrap_or_default().to_string())
    }

    fn param_abi(self, ty: Ty<'gcx>, name: String) -> json::Param {
        let ty = ty.peel_refs();
        let struct_id = ty.visit(&mut |ty| match ty.kind {
            TyKind::Struct(id) => ControlFlow::Break(id),
            _ => ControlFlow::Continue(()),
        });
        json::Param {
            ty: self.print_abi_param_ty(ty),
            name,
            components: match struct_id {
                ControlFlow::Break(id) => self
                    .item_fields(id)
                    .map(|(ty, f)| self.param_abi(ty, self.item_name(f).to_string()))
                    .collect(),
                ControlFlow::Continue(()) => vec![],
            },
            internal_type: Some(json::InternalType::parse(&self.print_solc_param_ty(ty)).unwrap()),
        }
    }

    fn event_param_abi(self, id: hir::VariableId) -> json::EventParam {
        let json::Param { ty, name, components, internal_type } = self.var_param_abi(id);
        let indexed = self.hir.variable(id).indexed;
        json::EventParam { ty, name, components, internal_type, indexed }
    }

    fn print_abi_param_ty(self, ty: Ty<'gcx>) -> String {
        let mut s = String::new();
        TyAbiPrinter::new(self, &mut s, TyAbiPrinterMode::Abi).print(ty).unwrap();
        s
    }

    fn print_solc_param_ty(self, ty: Ty<'gcx>) -> String {
        let mut s = String::new();
        TySolcPrinter::new(self, &mut s).data_locations(false).print(ty).unwrap();
        s
    }
}

fn json_state_mutability(s: hir::StateMutability) -> json::StateMutability {
    match s {
        hir::StateMutability::Pure => json::StateMutability::Pure,
        hir::StateMutability::View => json::StateMutability::View,
        hir::StateMutability::Payable => json::StateMutability::Payable,
        hir::StateMutability::NonPayable => json::StateMutability::NonPayable,
    }
}

/// Prints types as specified by the Solidity ABI.
///
/// Reference: <https://docs.soliditylang.org/en/latest/abi-spec.html>
pub struct TyAbiPrinter<'gcx, W> {
    gcx: Gcx<'gcx>,
    buf: W,
    mode: TyAbiPrinterMode,
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
        Self { gcx, buf, mode }
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
            TyKind::Contract(_) => self.buf.write_str("address"),
            TyKind::FnPtr(_) => self.buf.write_str("function"),
            TyKind::Struct(id) => match self.mode {
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
            TyKind::Enum(_) => self.buf.write_str("uint8"),
            TyKind::Udvt(ty, _) => self.print(ty),
            TyKind::Ref(ty, _loc) => self.print(ty),
            TyKind::DynArray(ty) => {
                self.print(ty)?;
                self.buf.write_str("[]")
            }
            TyKind::Array(ty, len) => {
                self.print(ty)?;
                write!(self.buf, "[{len}]")
            }

            TyKind::StringLiteral(..)
            | TyKind::IntLiteral(_)
            | TyKind::Tuple(_)
            | TyKind::Mapping(..)
            | TyKind::Error(..)
            | TyKind::Event(..)
            | TyKind::Module(_)
            | TyKind::BuiltinModule(_)
            | TyKind::Type(_)
            | TyKind::Meta(_)
            | TyKind::Err(_) => panic!("printing unsupported type as ABI: {ty:?}"),
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
            TyKind::FnPtr(f) => {
                self.print_function(None, f.parameters, f.returns, f.state_mutability, f.visibility)
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
            TyKind::IntLiteral(size) => {
                write!(self.buf, "int_literal[{}]", size.bytes())
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
            TyKind::Type(ty) | TyKind::Meta(ty) => {
                self.buf.write_str("type(")?;
                self.print(ty)?; // TODO: `richIdentifier`
                self.buf.write_str(")")
            }
            TyKind::Error(tys, id) => self.print_function_like(tys, id.into()),
            TyKind::Event(tys, id) => self.print_function_like(tys, id.into()),

            TyKind::Err(_) => self.buf.write_str("<error>"),
        }
    }

    fn print_function_like(&mut self, parameters: &[Ty<'gcx>], id: hir::ItemId) -> fmt::Result {
        self.print_function(
            Some(id),
            parameters,
            &[],
            hir::StateMutability::NonPayable,
            solar_ast::Visibility::Internal,
        )
    }

    fn print_function(
        &mut self,
        def: Option<hir::ItemId>,
        parameters: &[Ty<'gcx>],
        returns: &[Ty<'gcx>],
        state_mutability: hir::StateMutability,
        visibility: hir::Visibility,
    ) -> fmt::Result {
        self.buf.write_str("function ")?;
        if let Some(def) = def {
            let name = self.gcx.item_canonical_name(def);
            write!(self.buf, "{name}")?;
        }
        self.print_tuple(parameters)?;

        if state_mutability != hir::StateMutability::NonPayable {
            write!(self.buf, " {state_mutability}")?;
        }
        if visibility == hir::Visibility::External {
            self.buf.write_str(" external")?;
        }

        if !returns.is_empty() {
            self.buf.write_str(" returns ")?;
            self.print_tuple(returns)?;
        }
        Ok(())
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
