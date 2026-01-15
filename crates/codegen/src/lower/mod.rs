//! Lowering from HIR to MIR (Mid-level IR).
//!
//! MIR is a simplified representation that's easier to generate EVM bytecode from.

use solar_ast::StateMutability;
use solar_sema::hir::{self, FunctionKind};

/// A function in MIR form.
#[derive(Debug, Clone)]
pub struct MirFunction {
    /// The function name (for selector calculation).
    pub name: Option<String>,
    /// The function kind.
    pub kind: FunctionKind,
    /// The function's state mutability.
    pub state_mutability: StateMutability,
    /// The 4-byte selector for this function.
    pub selector: Option<[u8; 4]>,
}

impl MirFunction {
    /// Creates a new MIR function from a HIR function.
    pub fn from_hir(func: &hir::Function<'_>) -> Self {
        let name = func.name.map(|n| n.to_string());
        let selector = name.as_ref().and_then(|n| {
            if func.kind.is_ordinary() && func.visibility >= solar_ast::Visibility::Public {
                Some(compute_selector(n, &[]))
            } else {
                None
            }
        });

        Self { name, kind: func.kind, state_mutability: func.state_mutability, selector }
    }

    /// Returns true if this function can receive ETH.
    pub fn is_payable(&self) -> bool {
        self.state_mutability == StateMutability::Payable
    }

    /// Returns true if this function should check for zero value.
    pub fn needs_value_check(&self) -> bool {
        !self.is_payable() && !matches!(self.kind, FunctionKind::Constructor)
    }
}

/// Computes the 4-byte function selector from the function signature.
pub fn compute_selector(name: &str, _param_types: &[&str]) -> [u8; 4] {
    use alloy_primitives::keccak256;

    let sig = format!("{name}()");
    let hash = keccak256(sig.as_bytes());
    [hash[0], hash[1], hash[2], hash[3]]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_selector() {
        let selector = compute_selector("deposit", &[]);
        assert_eq!(selector, [0xd0, 0xe3, 0x0d, 0xb0]);
    }
}
