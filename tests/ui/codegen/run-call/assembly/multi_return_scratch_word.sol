//~[gas, size] ERROR: codegen cannot lower `f`: multi-word call result through memory
//@ revisions: none gas size
//@ compile-flags: --no-warnings
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@[none] run-call: f 5 => 5, 7

// MIR models an internal call that returns several words as writing the pointer to its result buffer to
// scratch word 0x20. Assembly that reads the word after the call, expecting the callee's own store, relies on
// scratch memory surviving a statement. solc keeps the word and returns 7. At `-O none` we return 7
// too, but at `-O gas` and `-O size` the planner treats the read as the protocol's pointer read and
// rejects the function (CODEGEN-032).
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
        assembly ("memory-safe") {
            s := mload(32)
        }
    }

    // A second caller keeps `g` from being inlined.
    function other(uint256 v) external pure returns (uint256 a) {
        (a,) = g(v + 1);
    }
}
