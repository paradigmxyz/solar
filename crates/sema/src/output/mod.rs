use serde::Serialize;

mod abi;
mod ast_json;
mod natspec;
mod storage_layout;

pub use ast_json::{AstJson, SourceAstJson};
pub use natspec::{Documentation, DocumentationItem};
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
