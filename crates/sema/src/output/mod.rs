use serde::Serialize;

mod abi;
mod devdoc;
mod storage_layout;
mod userdoc;

pub use devdoc::DevDocumentation;
pub use storage_layout::StorageLayoutOutput;
pub use userdoc::UserDocumentation;

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum DocumentationKind {
    User,
    Dev,
}
