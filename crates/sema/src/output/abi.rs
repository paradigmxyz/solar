use crate::{
    hir,
    ty::{Gcx, Ty, TyAbiPrinter, TyAbiPrinterMode, TyKind, TySolcPrinter},
};
use alloy_json_abi as json;
use std::ops::ControlFlow;

impl<'gcx> Gcx<'gcx> {
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
        // TODO: Does not include referenced items: https://github.com/paradigmxyz/solar/issues/305
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
            (item.json_type(), item.name())
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
