//@ compile-flags: --emit=bin-runtime -Zemit-lean-stages=../../target/ui-lean-stages
// A call of a function that returns several words binds the further results to the values that
// read them, as stack scheduling returns them on the stack, and leaves out MIR's reads of the
// multi-return buffer.

contract LeanStagesMultiReturn {
    uint256 x;

    function g(uint256 a) internal returns (uint256, uint256) {
        x = a * x + 1;
        x = x / (a + 1);
        return (x, a ^ x);
    }

    function f(uint256 a) external returns (uint256) {
        (uint256 b, uint256 c) = g(a);
        (uint256 d, uint256 e) = g(b + c);
        return d + e;
    }

    function h(uint256 a) external returns (uint256) {
        (uint256 b, uint256 c) = g(a);
        return b * c;
    }
}
