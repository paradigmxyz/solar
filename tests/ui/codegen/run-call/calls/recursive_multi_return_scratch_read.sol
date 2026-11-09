//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: branch 0 => 7
//@ run-call: branch 3 => 1
//@ run-call: sequence 3 => 1, 7
//@ run-call: loop 3 => 1, 7
//@ run-call: nested 2 => 14

// An internal call to a recursive function returning several words whose extra results go
// unused, in a function that also decodes several words returned by an external call: the
// decoder reads the scratch word that doubles as the multi-return buffer pointer.

interface V {
    function v() external returns (bool, uint256);
}

contract RecursiveMultiReturnScratchRead is V {
    function v() external pure returns (bool, uint256) {
        return (true, 7);
    }

    function rec(uint256 x) internal returns (uint256, bool) {
        if (x > 1) return rec(x - 1);
        return (x, true);
    }

    function branch(uint256 x) external returns (uint256 k) {
        if (x == 0) {
            (, k) = V(address(this)).v();
        } else {
            (k, ) = rec(x);
        }
    }

    function sequence(uint256 x) external returns (uint256 k, uint256 j) {
        (k, ) = rec(x);
        (, j) = V(address(this)).v();
    }

    function loop(uint256 x) external returns (uint256 k, uint256 j) {
        for (uint256 i; i < 2; i++) {
            (, j) = V(address(this)).v();
            (k, ) = rec(x + i);
        }
    }

    function sum(uint256 x) internal returns (uint256, bool) {
        if (x == 0) return (0, true);
        (uint256 k, ) = sum(x - 1);
        (, uint256 j) = V(address(this)).v();
        return (k + j, true);
    }

    function nested(uint256 x) external returns (uint256 k) {
        (k, ) = sum(x);
    }
}
