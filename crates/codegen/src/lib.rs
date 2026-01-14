#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/favicon.ico"
)]
#![cfg_attr(feature = "nightly", feature(rustc_attrs), allow(internal_features))]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate derive_more as _;
extern crate tracing as _;

pub use solar_sema as sema;

pub mod mir;
pub use mir::{
    BasicBlock, BlockId, Function, FunctionId, Immediate, InstId, InstKind, Instruction, MirType,
    Module, Terminator, Value, ValueId,
};

pub mod lower;
pub use lower::Lowerer;

pub mod codegen;
pub use codegen::EvmCodegen;
