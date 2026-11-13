//@ compile-flags: --emit=bin-runtime -Zemit-lean-stages=../../target/ui-lean-stages -Zcheck-analyses=false
// A call of a function that returns several words, whose further result the code reads through
// the multi-return buffer's pointer at 0x20 only after a storage write, so stack scheduling
// cannot bind it on the stack: codegen publishes it to the callee's return buffer, and the call
// names that buffer. The other call binds both results on the stack.

contract LeanStagesScratchReturn {
    uint256 total;

    function split(uint256 x) internal returns (uint256, uint256) {
        total += x;
        return (total, x ^ total);
    }

    function add(uint256 x) external returns (uint256 a, uint256 b) {
        (a, ) = split(x);
        total = a;
        assembly {
            b := mload(add(mload(0x20), 0x20))
        }
    }

    function addTwice(uint256 x) external returns (uint256) {
        (uint256 a, uint256 b) = split(x);
        (uint256 c, ) = split(b);
        return a + c;
    }
}
