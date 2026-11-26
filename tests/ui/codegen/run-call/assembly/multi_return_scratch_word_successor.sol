//@ revisions: none gas size
//@ compile-flags: --no-warnings
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@[none] run-call: f 5 => 5, 7
//@[gas] run-call: f 5 => 5, 160
//@[size] run-call: f 5 => 5, 160

// The read is in a later block than the call, so the planner does not take it for the protocol's
// pointer read and the call publishes the pointer to scratch word 0x20. The read returns the
// pointer where solc and `-O none` return the callee's word (CODEGEN-034).
contract C {
    function g(uint256 v) internal pure returns (uint256 a, uint256 b) {
        assembly ("memory-safe") {
            mstore(32, 7)
            a := v
        }
        if (a == 0) revert();
    }

    function f(uint256 v) external pure returns (uint256 a, uint256 s) {
        (a,) = g(v);
        if (a > 3) {
            assembly ("memory-safe") {
                s := mload(32)
            }
        }
    }

    // A second caller keeps `g` from being inlined.
    function other(uint256 v) external pure returns (uint256 a) {
        (a,) = g(v + 1);
    }
}
