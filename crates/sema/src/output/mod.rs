use serde::Serialize;

mod abi;
mod natspec;
mod storage_layout;

pub use natspec::{
    DevDocItem, DevDocumentation, StateVariableDoc, UserDocNotice, UserDocumentation,
};
pub use storage_layout::{
    StorageEncoding, StorageLayoutEntry, StorageLayoutMember, StorageLayoutOutput,
    StorageLayoutType,
};
#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DocumentationKind {
    User,
    Dev,
}
