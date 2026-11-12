//@ compile-flags: --emit=bin-runtime -Zemit-lean-stages=../../target/ui-lean-stages
// A call of a function that returns several words binds the further results to the values that
// read them, as stack scheduling returns them on the stack, and leaves out MIR's reads of the
// multi-return buffer. In `LeanStagesPublish`, an ABI decoder reads the scratch word that is
// also the buffer's pointer, so the call before it publishes its results to the callee's return
// buffer too, and the call names that buffer.

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

interface V {
    function v() external returns (bool, uint256);
}

contract LeanStagesPublish is V {
    function v() external pure returns (bool, uint256) {
        return (true, 7);
    }

    function rec(uint256 x) internal returns (uint256, bool) {
        if (x > 1) return rec(x - 1);
        return (x, true);
    }

    function twice(uint256 x) external returns (uint256 a, uint256 j, uint256 b) {
        (a, ) = rec(x);
        (, j) = V(address(this)).v();
        (b, ) = rec(x);
    }
}
