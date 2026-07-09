use serde::Serialize;

mod abi;
mod devdoc;
mod storage_layout;
mod userdoc;

pub use devdoc::{DevDocItem, DevDocumentation, StateVariableDoc};
pub use storage_layout::{
    StorageEncoding, StorageLayoutEntry, StorageLayoutMember, StorageLayoutOutput,
    StorageLayoutType,
};
pub use userdoc::{UserDocNotice, UserDocumentation};

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DocumentationKind {
    User,
    Dev,
}
