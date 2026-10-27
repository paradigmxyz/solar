//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: loadSome 0x0a0b0c0d0e, 1, 3 => 0x0b0c0d0000000000000000000000000000000000000000000000000000000000
//@ run-call: loadSome 0x0a0b, 0x10000000000000000, 0 => 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: loadSome 0x0a0b, 0x10000000000000000, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: storeSome 0xaaaaaaaa, 1, 0x0102000000000000000000000000000000000000000000000000000000000000, 2 => 0xaa0102aa
//@ run-call: storeSome 0xaaaaaaaa, 0x10000000000000000, 0x0102000000000000000000000000000000000000000000000000000000000000, 0 => 0xaaaaaaaa
//@ run-call-fail: storeSome 0xaaaaaaaa, 0x10000000000000000, 0x0102000000000000000000000000000000000000000000000000000000000000, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: loadNowhere 0x10000000000000000, 0 => 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: storeNowhere 0x10000000000000000, 0x0102000000000000000000000000000000000000000000000000000000000000, 0 => 0

// A gather or scatter's fast path accesses one word at the offset it checked, for a nonzero
// count only. A count of zero checks nothing and touches no memory, so the offset,
// and even the buffer, may be anything: a word there could expand memory until the call runs out
// of gas.
contract Test {
    // CHECK-LABEL: fn @loadSome(
    // CHECK-NOT: byte 0
    // CHECK: [[EMPTY:v[0-9]+]] = eq arg2, 0
    // CHECK-NEXT: jumpi [[EMPTY]]
    // CHECK: [[BASE:v[0-9]+]] = add {{v[0-9]+}}, arg1
    // CHECK-NEXT: [[ADDRESS:v[0-9]+]] = add [[BASE]], 32
    // CHECK-NEXT: mload [[ADDRESS]]
    // CHECK: phi [{{bb[0-9]+}}: 0], [{{bb[0-9]+}}: {{v[0-9]+}}]
    function loadSome(bytes memory b, uint256 offset, uint256 count) external pure returns (bytes32 result) {
        for (uint256 k; k < count; ++k) {
            result |= bytes32(b[offset + k]) >> (8 * k);
        }
    }

    // CHECK-LABEL: fn @storeSome(
    // CHECK-NOT: mstore8
    // CHECK: [[EMPTY:v[0-9]+]] = eq arg3, 0
    // CHECK-NEXT: jumpi [[EMPTY]]
    // CHECK: [[BASE:v[0-9]+]] = add {{v[0-9]+}}, arg1
    // CHECK-NEXT: [[ADDRESS:v[0-9]+]] = add [[BASE]], 32
    // CHECK: mload [[ADDRESS]]
    // CHECK: mstore [[ADDRESS]],
    // CHECK: mstore8
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

    /// A count of zero touches no memory, so the buffer may lie where one word costs more gas than
    /// the call has.
    function loadNowhere(uint256 at, uint256 count) external pure returns (bytes32) {
        bytes memory b;
        assembly {
            b := at
        }
        return _loadSome(b, count);
    }

    function storeNowhere(uint256 at, bytes32 value, uint256 count) external pure returns (uint256) {
        bytes memory b;
        assembly {
            b := at
        }
        _storeSome(b, value, count);
        return count;
    }

    /// @custom:solar-noinline
    function _loadSome(bytes memory b, uint256 count) private pure returns (bytes32 result) {
        for (uint256 k; k < count; ++k) {
            result |= bytes32(b[k]) >> (8 * k);
        }
    }

    /// @custom:solar-noinline
    function _storeSome(bytes memory b, bytes32 value, uint256 count) private pure {
        for (uint256 k; k < count; ++k) {
            b[k] = value[k];
        }
    }
}
