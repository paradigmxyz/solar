//@ codegen-matrix: standard
//@ run-call: values 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => false, true, true, false
//@ run-call: ltBranch 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 2
//@ run-call: gtBranch 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 1
//@ run-call: eqBranch 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 1
//@ run-call: neBranch 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 2
//@ run-call: values 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe => true, false, false, true
//@ run-call: ltBranch 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe => 1
//@ run-call: gtBranch 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe => 2
//@ run-call: eqBranch 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe => 2
//@ run-call: neBranch 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe => 1
//@ run-call: values 0 => true, false, false, true
//@ run-call: ltBranch 0 => 1
//@ run-call: gtBranch 0 => 2
//@ run-call: eqBranch 0 => 2
//@ run-call: neBranch 0 => 1
//@ run-call: values 0x8000000000000000000000000000000000000000000000000000000000000000 => true, false, false, true
//@ run-call: ltBranch 0x8000000000000000000000000000000000000000000000000000000000000000 => 1
//@ run-call: gtBranch 0x8000000000000000000000000000000000000000000000000000000000000000 => 2
//@ run-call: eqBranch 0x8000000000000000000000000000000000000000000000000000000000000000 => 2
//@ run-call: neBranch 0x8000000000000000000000000000000000000000000000000000000000000000 => 1
//@ run-call: narrow 255 => false, true, true, false
//@ run-call: ltBranch8 255 => 2
//@ run-call: gtBranch8 255 => 1
//@ run-call: eqBranch8 255 => 1
//@ run-call: neBranch8 255 => 2
//@ run-call: narrow 254 => true, false, false, true
//@ run-call: ltBranch8 254 => 1
//@ run-call: gtBranch8 254 => 2
//@ run-call: eqBranch8 254 => 2
//@ run-call: neBranch8 254 => 1
//@ run-call: narrow 0 => true, false, false, true
//@ run-call: ltBranch8 0 => 1
//@ run-call: gtBranch8 0 => 2
//@ run-call: eqBranch8 0 => 2
//@ run-call: neBranch8 0 => 1

contract MaxCompares {
    function values(uint256 x) external pure returns (bool, bool, bool, bool) {
        return (x < type(uint256).max, x > type(uint256).max - 1, x == type(uint256).max, x != type(uint256).max);
    }

    function narrow(uint8 x) external pure returns (bool, bool, bool, bool) {
        return (x < type(uint8).max, x > type(uint8).max - 1, x == type(uint8).max, x != type(uint8).max);
    }

    function ltBranch(uint256 x) external pure returns (uint256) {
        if (x < type(uint256).max) return 1;
        return 2;
    }

    function gtBranch(uint256 x) external pure returns (uint256) {
        if (x > type(uint256).max - 1) return 1;
        return 2;
    }

    function eqBranch(uint256 x) external pure returns (uint256) {
        if (x == type(uint256).max) return 1;
        return 2;
    }

    function neBranch(uint256 x) external pure returns (uint256) {
        if (x != type(uint256).max) return 1;
        return 2;
    }

    function ltBranch8(uint8 x) external pure returns (uint256) {
        if (x < type(uint8).max) return 1;
        return 2;
    }

    function gtBranch8(uint8 x) external pure returns (uint256) {
        if (x > type(uint8).max - 1) return 1;
        return 2;
    }

    function eqBranch8(uint8 x) external pure returns (uint256) {
        if (x == type(uint8).max) return 1;
        return 2;
    }

    function neBranch8(uint8 x) external pure returns (uint256) {
        if (x != type(uint8).max) return 1;
        return 2;
    }
}
