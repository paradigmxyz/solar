use super::{Gcx, Ty, TyKind};
use crate::hir;
use alloy_json_abi as json;
use solar_ast::ast::ElementaryType;
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
        TyPrinter::new(self, &mut s).print_tuple(tys).unwrap();
        s
    }

    /// Returns the ABI of the given contract.
    ///
    /// Reference: <https://docs.soliditylang.org/en/develop/abi-spec.html>
    pub fn contract_abi(self, id: hir::ContractId) -> Vec<json::AbiItem<'static>> {
        let mut items = Vec::<json::AbiItem<'static>>::new();

        let c = self.hir.contract(id);
        if let Some(ctor) = c.ctor {
            if !c.is_abstract() {
                let json::Function { inputs, state_mutability, .. } = self.function_abi(ctor);
                items.push(json::Constructor { inputs, state_mutability }.into());
            }
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

        // https://github.com/ethereum/solidity/blob/87d86bfba64d8b88537a4a85c1d71f521986b614/libsolidity/interface/ABI.cpp#L43-L47
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
            ty: self.print_param_ty(ty, false),
            name,
            components: match struct_id {
                ControlFlow::Break(id) => self
                    .item_fields(id)
                    .map(|(ty, f)| self.param_abi(ty, self.item_name(f).to_string()))
                    .collect(),
                ControlFlow::Continue(()) => vec![],
            },
            internal_type: Some(json::InternalType::parse(&self.print_param_ty(ty, true)).unwrap()),
        }
    }

    fn event_param_abi(self, id: hir::VariableId) -> json::EventParam {
        let json::Param { ty, name, components, internal_type } = self.var_param_abi(id);
        let indexed = self.hir.variable(id).indexed;
        json::EventParam { ty, name, components, internal_type, indexed }
    }

    fn print_param_ty(self, ty: Ty<'gcx>, solc: bool) -> String {
        let mut s = String::new();
        TyPrinter::new(self, &mut s).solc(solc).recurse(false).print(ty).unwrap();
        s
    }
}

fn json_state_mutability(s: hir::StateMutability) -> json::StateMutability {
    match s {
        hir::StateMutability::Pure => json::StateMutability::Pure,
        hir::StateMutability::View => json::StateMutability::View,
        hir::StateMutability::NonPayable => json::StateMutability::NonPayable,
        hir::StateMutability::Payable => json::StateMutability::Payable,
    }
}

struct TyPrinter<'gcx, W: fmt::Write> {
    gcx: Gcx<'gcx>,
    buf: W,
    /// If `true`, prints the type as it would appear in solc, otherwise as ABI.
    solc: bool,
    /// Recurse into structs to print their fields. If `false`, just print `tuple` instead.
    ///
    /// Has effect only when printing as ABI.
    recurse: bool,
    /// If `true`, prints the data location of references.
    ///
    /// Only has effect when printing as solc.
    data_locations: bool,
}

impl<'gcx, W: fmt::Write> TyPrinter<'gcx, W> {
    fn new(gcx: Gcx<'gcx>, buf: W) -> Self {
        Self { gcx, buf, recurse: true, solc: false, data_locations: false }
    }

    fn solc(mut self, yes: bool) -> Self {
        self.solc = yes;
        self
    }

    fn recurse(mut self, yes: bool) -> Self {
        self.recurse = yes;
        self
    }

    #[allow(dead_code)]
    fn data_locations(mut self, yes: bool) -> Self {
        self.data_locations = yes;
        self
    }

    fn print(&mut self, ty: Ty<'gcx>) -> fmt::Result {
        if self.solc {
            self.print_solc(ty)
        } else {
            self.print_abi(ty)
        }
    }

    fn print_abi(&mut self, ty: Ty<'gcx>) -> fmt::Result {
        match ty.kind {
            TyKind::Elementary(ty) => ty.write_abi_str(&mut self.buf),
            TyKind::Contract(_) => self.buf.write_str("address"),
            TyKind::FnPtr(_) => self.buf.write_str("function"),
            TyKind::Struct(id) => {
                if self.recurse {
                    self.print_tuple(self.gcx.struct_field_types(id).iter().copied())
                } else {
                    self.buf.write_str("tuple")
                }
            }
            TyKind::Enum(_) => self.buf.write_str("uint8"),
            TyKind::Udvt(ty, _) => self.print_abi(ty),
            TyKind::Ref(ty, _loc) => self.print_abi(ty),
            TyKind::DynArray(ty) => {
                self.print_abi(ty)?;
                self.buf.write_str("[]")
            }
            TyKind::Array(ty, len) => {
                self.print_abi(ty)?;
                write!(self.buf, "[{len}]")
            }
            _ => panic!("printing invalid ABI type: {ty:?}"),
        }
    }

    fn print_solc(&mut self, ty: Ty<'gcx>) -> fmt::Result {
        match ty.kind {
            TyKind::Elementary(ty) => {
                ty.write_abi_str(&mut self.buf)?;
                if matches!(ty, ElementaryType::Address(true)) {
                    self.buf.write_str(" payable")?;
                }
                Ok(())
            }
            TyKind::Contract(id) => {
                write!(self.buf, "contract {}", self.gcx.item_canonical_name(id))
            }
            TyKind::FnPtr(_) => self.buf.write_str("function"),
            TyKind::Struct(id) => {
                write!(self.buf, "struct {}", self.gcx.item_canonical_name(id))
            }
            TyKind::Enum(id) => write!(self.buf, "enum {}", self.gcx.item_canonical_name(id)),
            TyKind::Udvt(_, id) => write!(self.buf, "{}", self.gcx.item_canonical_name(id)),
            TyKind::Ref(ty, loc) => {
                self.print_solc(ty)?;
                if self.data_locations {
                    write!(self.buf, " {loc}")?;
                }
                Ok(())
            }
            TyKind::DynArray(ty) => {
                self.print_solc(ty)?;
                self.buf.write_str("[]")
            }
            TyKind::Array(ty, len) => {
                self.print_solc(ty)?;
                write!(self.buf, "[{len}]")
            }
            _ => panic!("printing invalid solc type: {ty:?}"),
        }
    }

    fn print_tuple(&mut self, tys: impl IntoIterator<Item = Ty<'gcx>>) -> fmt::Result {
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
