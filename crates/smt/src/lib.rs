//! SMT-LIB support for the compiler's provers, such as the analysis-fact prover in
//! `tools/prove-analyses`.
//!
//! [`sexp`] parses the S-expressions of SMT-LIB scripts and of the responses solvers print.

pub mod sexp;
pub use sexp::Sexp;
