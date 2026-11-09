//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir-final
//@[dump] filecheck:
//@ run-call: twice 3 => 1, 7, 1
//@ run-call: once 3 => 1, 1

// Two calls to one recursive multi-word function whose extra results go unused, in a function
// that decodes several words returned by an external call. The decoder reads the scratch word
// that doubles as the multi-return buffer pointer, so the call before it publishes its results,
// while the call after it reaches no such read and returns on the stack. The final MIR records
// the stack return only for the second call.

interface V {
    function v() external returns (bool, uint256);
}

contract UnpublishedCall is V {
    function v() external pure returns (bool, uint256) {
        return (true, 7);
    }

    function rec(uint256 x) internal returns (uint256, bool) {
        if (x > 1) return rec(x - 1);
        return (x, true);
    }

    // CHECK: backend-stack [[TWICE:[0-9]+]] {{[0-9]+}} - -
    // CHECK-NEXT: backend-entry [[TWICE]]
    function twice(uint256 x) external returns (uint256 a, uint256 j, uint256 b) {
        (a, ) = rec(x);
        (, j) = V(address(this)).v();
        (b, ) = rec(x);
    }

    function once(uint256 x) external returns (uint256 a, uint256 b) {
        (a, ) = rec(x);
        (b, ) = rec(x);
    }
}
