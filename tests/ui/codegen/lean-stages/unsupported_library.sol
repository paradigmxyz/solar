//@ compile-flags: --emit=bin-runtime -Zemit-lean-stages=../../target/ui-lean-stages
// A library's address is a placeholder the linker fills, which the Lean semantics cannot
// represent, so exporting fails closed.

library L {
    function f(uint256 a) external pure returns (uint256) {
        return a + 1;
    }
}

contract LeanStagesLibrary {
    function g(uint256 a) external pure returns (uint256) {
        return L.f(a);
    }
}

//~? ERROR: -Zemit-lean-stages: after MIR pass `lower-evm-shaped` on `LeanStagesLibrary`: in `@g`: bb5: instruction `library_address` has no Lean semantics
