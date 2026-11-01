//! MIR analyses the verifier and the module queries rest on: the CFG with its dominator tree and
//! the call graph. The verifier lives here too.

mod call_graph;
pub use call_graph::{CallGraphInfo, direct_callees};

mod cfg;
pub use cfg::{CfgInfo, DominatorTree};

mod validator;
pub use validator::validate;
pub(crate) use validator::{validate_legal_shifts, validate_phase};
