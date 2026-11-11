//! SMT-LIB support for the compiler's provers, such as the analysis-fact prover in
//! `tools/prove-analyses`.
//!
//! [`sexp`] parses the S-expressions of SMT-LIB scripts and of the responses solvers print, and
//! [`certify`] checks `unsat` verdicts with cvc5 proofs and Ethos.

pub mod certify;
pub mod sexp;

pub use certify::{Certificate, Certifier};
pub use sexp::Sexp;
