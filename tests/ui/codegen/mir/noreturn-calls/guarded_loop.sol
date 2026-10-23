//@ codegen-matrix: standard opt
//@[opt] compile-flags: -Ogas -Zdump=mir
//@[opt] filecheck: --check-prefix=OPT
//@ run-call: sumFrom 0x010203, 1 => 5
//@ run-call: sumFrom 0x010203, 0 => 6
//@ run-call-fail: sumFrom 0x0102, 2 => 0xe143a034
//@ run-call-fail: sumFrom 0x, 0 => 0xe143a034

// The guard's revert helper cannot return, so its branch ends at the call
// rather than joining the loop, and every read the loop makes is below the
// length the guard and the loop condition compare.
// OPT-LABEL: fn @sumFrom()
// OPT-NOT: mstore 32, 50
// OPT-LABEL: fn @_fail()
contract GuardedLoop {
    error Bad();

    function sumFrom(bytes memory b, uint256 i) external pure returns (uint256 r) {
        if (i >= b.length) _fail();
        do {
            r += uint8(b[i]);
        } while (++i < b.length);
    }

    function _fail() private pure {
        revert Bad();
    }
}
