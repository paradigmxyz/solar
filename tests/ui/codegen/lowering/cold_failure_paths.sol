//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@ run-call: check 5, 3 => 6
//@ run-call-fail: check 11, 3 => 0xea10261a000000000000000000000000000000000000000000000000000000000000000b00000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000003030a110000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: alwaysFails 0 => 0xea10261a0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000010100000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: alwaysFails 3 => 0xea10261a000000000000000000000000000000000000000000000000000000000000000300000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000004040b121900000000000000000000000000000000000000000000000000000000

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
