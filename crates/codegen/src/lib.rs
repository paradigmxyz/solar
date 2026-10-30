#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/favicon.ico"
)]
#![cfg_attr(feature = "nightly", feature(rustc_attrs), allow(internal_features))]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(test, allow(unused_crate_dependencies))]

extern crate alloc;
extern crate derive_more as _;
extern crate tracing as _;

pub mod mir;

pub mod backend;
pub use backend::{Backend, evm::EvmCodegen};

mod contract;
pub use contract::{
    ContractArtifact, ContractSelection, ImmutableReference, LibraryReference, RuntimeDataFn,
    generate_contract_bytecodes, generate_mir_input_bytecode,
};

mod link;
pub use link::{
    ContractBytecodes, EmbeddedBytecodes, LibraryId, LibraryRelocation, LibraryTable,
    QualifiedName, RelocatableBytecode, qualified_name,
};
mod isle_log;

pub(crate) mod target;
mod timing;
