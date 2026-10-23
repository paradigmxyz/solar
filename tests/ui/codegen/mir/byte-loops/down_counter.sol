//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: fromCalldata 0x0102030405, 3 => 0x010203, 0
//@ run-call: fromCalldata 0x0102030405, 0 => 0x, 0
//@ run-call-fail: fromCalldata 0x0102, 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: fromFresh 3 => 0x010203, 0
//@ run-call: fromFresh 0 => 0x, 0

// A downward copy leaves the loop with its counter at zero, so a read of the counter after a
// copy that became one `calldatacopy` or `mcopy` reads zero, not the count it started from.
contract Test {
    // CHECK-LABEL: fn @fromCalldata(
    // CHECK-NOT: mstore8
    // CHECK: calldatacopy {{v[0-9]+}}, {{v[0-9]+}}, arg1
    // CHECK: mcopy
    // CHECK-NEXT: [[K:v[0-9]+]] = add {{v[0-9]+}}, 32
    // CHECK-NEXT: mstore [[K]], 0
    function fromCalldata(bytes calldata src, uint256 count)
        external
        pure
        returns (bytes memory out, uint256 k)
    {
        out = new bytes(count);
        for (k = count; k > 0; --k) {
            out[k - 1] = src[k - 1];
        }
    }

    // Between two fresh buffers, the copy needs no overlap test.
    // CHECK-LABEL: fn @fromFresh(
    // CHECK: mcopy {{v[0-9]+}}, {{v[0-9]+}}, arg0
    // CHECK: mcopy
    // CHECK-NEXT: [[K:v[0-9]+]] = add {{v[0-9]+}}, 32
    // CHECK-NEXT: mstore [[K]], 0
    function fromFresh(uint256 count) external pure returns (bytes memory out, uint256 k) {
        bytes memory tmp = new bytes(count);
        for (uint256 i; i < count; ++i) {
            tmp[i] = bytes1(uint8(i + 1));
        }
        out = new bytes(count);
        for (k = count; k > 0; --k) {
            out[k - 1] = tmp[k - 1];
        }
    }
}
