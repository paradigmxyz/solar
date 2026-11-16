//@ compile-flags: --emit=bin-runtime -Zemit-lean-stages=../../target/ui-lean-stages
// Internal calls export as MIR calls and returns with frames, and in EVM IR as pushed return
// labels and dynamic jumps. A parameter read through several values binds all of them.

contract LeanStagesInternalCall {
    uint256 x;

    function g(uint256 a, uint256 b) internal returns (uint256) {
        x = a * b + x;
        x = x / (a + 1);
        return x ^ b;
    }

    function f(uint256 a) external returns (uint256) {
        return g(a, 1) + g(a, 2);
    }

    function h(uint256 a) external returns (uint256) {
        return g(a, 3);
    }
}
