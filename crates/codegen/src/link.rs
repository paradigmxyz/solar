//! Program data, library identities, and relocatable bytecode shared by MIR and the backend, from
//! `solar_evmir`, and the names of compiled contracts.

use solar_sema::{Gcx, hir::ContractId};

pub use solar_evmir::link::*;

/// Returns the fully qualified name of contract `id`.
pub fn qualified_name(gcx: Gcx<'_>, id: ContractId) -> QualifiedName {
    let name = gcx.contract_fully_qualified_name(id).to_string();
    QualifiedName::parse(&name).expect("contract names are qualified by their source")
}
