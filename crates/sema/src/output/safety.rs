use crate::{
    hir,
    ty::{Gcx, TyAbiPrinter, TyAbiPrinterMode},
    typeck::safe_profile::{self, Violation},
};
use serde::Serialize;

/// The safety properties of the code a contract runs, the `solarSafety` output.
///
/// These are the properties `@custom:solar-safe` can require, reported for every contract. Created
/// by [`Gcx::safety`].
#[derive(Debug, Serialize)]
pub struct SafetyOutput {
    /// Whether the code uses no inline assembly outside trusted code: `memory`.
    pub memory: bool,
    /// Whether all of its arithmetic outside trusted code is checked: `arithmetic`.
    pub arithmetic: bool,
    /// The reviewed functions the code runs, tagged `@custom:solar-trusted` or in a contract
    /// tagged so, which the properties take on trust.
    pub trusted: Vec<String>,
}

impl Gcx<'_> {
    /// Returns the safety properties of the code the contract `id` runs.
    pub fn safety(self, id: hir::ContractId) -> SafetyOutput {
        let findings = safe_profile::findings(self, id);
        let violated = |kinds: &[Violation]| {
            findings.violations.iter().any(|finding| kinds.contains(&finding.violation))
        };
        let trusted = findings
            .trusted
            .iter()
            .map(|&function| {
                let declaration = self.hir.function(function);
                let mut signature = declaration.name_or_kind().to_string();
                // Internal signatures need not have an ABI representation. Naming structs also
                // handles recursive parameters without trying to expand their fields forever.
                TyAbiPrinter::new(self, &mut signature, TyAbiPrinterMode::Signature)
                    .with_in_library(
                        !self.has_abi_signature(function)
                            || declaration.contract.is_some_and(|contract| {
                                self.hir.contract(contract).kind.is_library()
                            }),
                    )
                    .print_tuple(self.item_parameter_types(function).iter().copied())
                    .unwrap();
                match declaration.contract {
                    Some(contract) => format!("{}.{signature}", self.hir.contract(contract).name),
                    None => signature,
                }
            })
            .collect();
        SafetyOutput {
            memory: !violated(&[Violation::Assembly]),
            arithmetic: !violated(&[Violation::Assembly, Violation::Unchecked]),
            trusted,
        }
    }
}
