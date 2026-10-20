//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: loadSome 0x0a0b0c0d0e, 1, 3 => 0x0b0c0d0000000000000000000000000000000000000000000000000000000000
//@ run-call: loadSome 0x0a0b, 0x10000000000000000, 0 => 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: loadSome 0x0a0b, 0x10000000000000000, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: storeSome 0xaaaaaaaa, 1, 0x0102000000000000000000000000000000000000000000000000000000000000, 2 => 0xaa0102aa
//@ run-call: storeSome 0xaaaaaaaa, 0x10000000000000000, 0x0102000000000000000000000000000000000000000000000000000000000000, 0 => 0xaaaaaaaa
//@ run-call-fail: storeSome 0xaaaaaaaa, 0x10000000000000000, 0x0102000000000000000000000000000000000000000000000000000000000000, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

// A gather or scatter of a count that may be zero accesses one word at the offset it checked.
// A count of zero checks nothing and touches no memory, so the offset may be anything; the word
// is then taken at offset zero, the buffer's first word, rather than where a huge offset would
// expand memory until the call runs out of gas.
contract Test {
    // CHECK-LABEL: fn @loadSome(
    // CHECK-NOT: byte 0
    // CHECK: [[NONZERO:v[0-9]+]] = ne arg2, 0
    // CHECK: [[SCALE:v[0-9]+]] = zext i1 [[NONZERO]] to i256
    // CHECK: [[OFFSET:v[0-9]+]] = mul arg1, [[SCALE]]
    // CHECK-NEXT: [[BASE:v[0-9]+]] = add {{v[0-9]+}}, [[OFFSET]]
    // CHECK-NEXT: [[ADDRESS:v[0-9]+]] = add [[BASE]], 32
    // CHECK-NEXT: mload [[ADDRESS]]
    function loadSome(bytes memory b, uint256 offset, uint256 count) external pure returns (bytes32 result) {
        for (uint256 k; k < count; ++k) {
            result |= bytes32(b[offset + k]) >> (8 * k);
        }
    }

    // CHECK-LABEL: fn @storeSome(
    // CHECK-NOT: mstore8
    // CHECK: [[NONZERO:v[0-9]+]] = ne arg3, 0
    // CHECK: [[SCALE:v[0-9]+]] = zext i1 [[NONZERO]] to i256
    // CHECK: [[OFFSET:v[0-9]+]] = mul arg1, [[SCALE]]
    // CHECK-NEXT: [[BASE:v[0-9]+]] = add {{v[0-9]+}}, [[OFFSET]]
    // CHECK-NEXT: [[ADDRESS:v[0-9]+]] = add [[BASE]], 32
    // CHECK: mload [[ADDRESS]]
    // CHECK: mstore [[ADDRESS]],
    function storeSome(bytes memory b, uint256 offset, bytes32 value, uint256 count)
        external
        pure
        returns (bytes memory)
    {
        for (uint256 k; k < count; ++k) {
            b[offset + k] = value[k];
        }
        return b;
    }
}
