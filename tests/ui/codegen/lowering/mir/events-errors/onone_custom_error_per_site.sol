//@ revisions: none gas
//@[none] compile-flags: -Onone -Zdump=mir-final
//@[gas] compile-flags: -Ogas -Zdump=mir-final
//@[none] filecheck: --check-prefix=NONE
//@[gas] filecheck: --check-prefix=GAS
//@ normalize-stdout-test: "(?s).+" -> ""

// `-Onone` keeps one revert block per `revert` statement, so each statement and its `if` path own
// an instruction. Optimized builds share the identical blocks. Both share the blocks that
// panic, which no statement writes.

// GAS: mstore 0, 0x4e487b71
// GAS-NEXT: revert 28, 36
// GAS-NOT: 0x4e487b71
// GAS: mstore 0, 0xe3372e2d
// GAS-NEXT: revert 28, 4
// GAS-NOT: 0xe3372e2d
contract C {
    error Denied();

    // NONE-LABEL: fn @check{{[( ]}}
    // NONE: mstore 0, 0xe3372e2d
    // NONE-NEXT: revert 28, 4
    // NONE: mstore 0, 0xe3372e2d
    // NONE-NEXT: revert 28, 4
    function check(uint256 a, uint256 b) external pure returns (uint256) {
        if (a == 0) revert Denied();
        if (b == 0) revert Denied();
        return a + b;
    }

    // NONE-LABEL: fn @sum{{[( ]}}
    // NONE: mstore 0, 0x4e487b71
    // NONE-NEXT: revert 28, 36
    // NONE-NOT: 0x4e487b71
    function sum(uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        return a + b + c;
    }
}
