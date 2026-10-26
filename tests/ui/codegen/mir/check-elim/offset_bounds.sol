//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: copyHigh => 0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000f4f1833d

// A copy of the calldata 64 bytes into a buffer 64 bytes longer keeps no bounds check: every
// `calldatasize` read equals the one the loop is bounded by, and the destination index
// `64 + k` stays below `n + 64` while `k < n`, since that sum cannot wrap.
contract OffsetBounds {
    // CHECK-LABEL: fn @copyHigh(
    // CHECK-NOT: mstore 32, 50
    // CHECK: {{^}}}{{$}}
    function copyHigh() external pure returns (bytes memory d) {
        uint256 n = msg.data.length;
        d = new bytes(n + 64);
        for (uint256 k; k < n; ++k) {
            d[64 + k] = msg.data[k];
        }
    }
}
