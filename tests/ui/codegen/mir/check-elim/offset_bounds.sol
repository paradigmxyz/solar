//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: copyHigh => 0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000f4f1833d

// The fast copy uses calldata's stable size. The fallback loop keeps its length load and
// bounds check when alias analysis cannot prove that indexed stores miss the header.
contract OffsetBounds {
    // CHECK-LABEL: fn @copyHigh(
    // CHECK: calldatacopy
    // CHECK: [[LENGTH:v[0-9]+]] = mload
    // CHECK-NEXT: {{v[0-9]+}} = lt {{v[0-9]+}}, [[LENGTH]]
    // CHECK-NEXT: jumpi
    // CHECK: mstore 32, 50
    // CHECK: {{^}}}{{$}}
    function copyHigh() external pure returns (bytes memory d) {
        uint256 n = msg.data.length;
        d = new bytes(n + 64);
        for (uint256 k; k < n; ++k) {
            d[64 + k] = msg.data[k];
        }
    }
}
