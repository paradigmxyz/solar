//@ codegen-matrix: standard ir ir-size
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@[ir-size] compile-flags: -Osize -Zdump=evm-ir-runtime
//@[ir-size] filecheck: --check-prefix=SIZE
//@ normalize-stdout-test: "(?s).+" -> ""
//@ run-call: check 5, 3 => 6
//@ run-call-fail: check 11, 3 => 0xea10261a000000000000000000000000000000000000000000000000000000000000000b00000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000003030a110000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: alwaysFails 0 => 0xea10261a0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000010100000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: alwaysFails 3 => 0xea10261a000000000000000000000000000000000000000000000000000000000000000300000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000004040b121900000000000000000000000000000000000000000000000000000000

// The loop in `describe` leaves through a return and stays hot. The loops that only lead to reverts
// and the code after a call on a revert path are cold.
// CHECK: bb{{[0-9]+}} [loop]:
// CHECK: bb{{[0-9]+}} [cold, loop]:
// CHECK: bb{{[0-9]+}} [cold, continuation]:
//
// A size build moves only the blocks that lead to aborts.
// SIZE-NOT: [cold, loop]
// SIZE-NOT: [cold, continuation]
//
// `check` reverts after calling `describe`, so the code after the call is a cold continuation.
// `alwaysFails` only leaves its loop into reverts, so the loop and the blocks before it are cold.
contract ColdFailurePaths {
    error Bad(uint256 code, bytes message);

    function describe(uint256 n) internal pure returns (bytes memory m) {
        m = new bytes(n);
        for (uint256 i; i < n; ++i) {
            m[i] = bytes1(uint8(i * 7 + n));
        }
    }

    function check(uint256 x, uint256 n) external pure returns (uint256) {
        if (x > 10) {
            revert Bad(x, describe(n));
        }
        return x + 1;
    }

    function alwaysFails(uint256 n) external pure returns (uint256) {
        for (uint256 i; i < n; ++i) {
            if (i * i == 77) {
                revert Bad(i, describe(n));
            }
        }
        revert Bad(n, describe(n + 1));
    }
}
